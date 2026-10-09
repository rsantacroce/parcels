//! Sun, sky, fog and the shared materials, plus a cosmetic day/night cycle:
//! the sun swings round, the sky shifts from blue to orange to navy, and
//! windows and street lamps light up after dusk.

use bevy::light::{CascadeShadowConfigBuilder, GlobalAmbientLight};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

use crate::camera::{CamMode, MainCamera};
use crate::driver::Driver;
use crate::settings::Settings;

#[derive(Resource)]
pub struct WorldMaterials {
    pub solid: Handle<StandardMaterial>,
    pub glass: Handle<StandardMaterial>,
    pub lights: Handle<StandardMaterial>,
    pub water: Handle<StandardMaterial>,
}

/// Time of day in hours, 0..24. Purely cosmetic; never touches the sim.
#[derive(Resource)]
pub struct Sky {
    pub hour: f32,
    /// 0 = full day, 1 = deep night.
    pub night: f32,
}

impl Default for Sky {
    fn default() -> Self {
        Self { hour: 10.0, night: 0.0 }
    }
}

#[derive(Component)]
pub struct Sun;

pub fn setup(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    let solid = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.92, reflectance: 0.2, ..default() });
    let glass = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.12,
        metallic: 0.1,
        reflectance: 0.7,
        ..default()
    });
    let lights = materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.4, ..default() });
    let water = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.82),
        perceptual_roughness: 0.06,
        reflectance: 0.9,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.insert_resource(WorldMaterials { solid, glass, lights, water });
    commands.spawn((
        Sun,
        DirectionalLight { illuminance: 11_000.0, shadow_maps_enabled: true, ..default() },
        Transform::from_xyz(0.0, 100.0, 0.0).looking_at(Vec3::new(-0.4, 0.0, -0.3), Vec3::Y),
        CascadeShadowConfigBuilder { num_cascades: 4, first_cascade_far_bound: 60.0, maximum_distance: 1600.0, ..default() }.build(),
    ));
    commands.insert_resource(GlobalAmbientLight { color: Color::WHITE, brightness: 500.0, ..default() });
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

#[allow(clippy::too_many_arguments)]
pub fn update_sky(
    time: Res<Time<Real>>,
    settings: Res<Settings>,
    driver: Option<Res<Driver>>,
    mut sky: ResMut<Sky>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sun: Query<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut cam: Query<(&mut DistanceFog, &CamMode), With<MainCamera>>,
    mats: Option<Res<WorldMaterials>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    match settings.day_cycle.seconds() {
        Some(len) => sky.hour = (sky.hour + time.delta_secs() * 24.0 / len) % 24.0,
        None => sky.hour = 11.0,
    }
    // Sun height: +1 at noon, -1 at midnight.
    let angle = (sky.hour - 6.0) / 24.0 * std::f32::consts::TAU;
    let elevation = angle.sin();
    let daylight = ((elevation + 0.12) / 0.4).clamp(0.0, 1.0);
    let dusk = (1.0 - (elevation.abs() / 0.3)).clamp(0.0, 1.0) * (elevation > -0.2) as u8 as f32;
    sky.night = 1.0 - daylight;

    let day_sky = [0.52, 0.72, 0.92];
    let dusk_sky = [0.95, 0.55, 0.35];
    let night_sky = [0.03, 0.04, 0.09];
    let mut c = lerp3(night_sky, day_sky, daylight);
    c = lerp3(c, dusk_sky, dusk * 0.6);
    clear.0 = Color::srgb(c[0], c[1], c[2]);

    let map_span = driver.as_ref().and_then(|d| d.state()).map_or(640.0, |s| s.map.width.max(s.map.height) as f32 * 10.0);
    for (mut fog, mode) in &mut cam {
        fog.color = Color::srgba(c[0], c[1], c[2], 1.0);
        fog.falloff = match mode {
            CamMode::Street { .. } => FogFalloff::Linear { start: 180.0, end: 900.0 },
            CamMode::Overview => FogFalloff::Linear { start: map_span * 1.2, end: map_span * 3.0 },
        };
    }

    if let Ok((mut light, mut tf)) = sun.single_mut() {
        // Arc from east to west, tilted south; at night a dim bluish moon from above.
        let az = angle;
        let dir = if elevation > -0.05 {
            Vec3::new(-az.cos(), -elevation.max(0.08), -0.45).normalize()
        } else {
            Vec3::new(0.3, -0.8, 0.4).normalize()
        };
        *tf = Transform::from_translation(-dir * 500.0).looking_to(dir, Vec3::Y);
        let warm = lerp3([1.0, 0.98, 0.94], [1.0, 0.62, 0.38], dusk);
        let col = lerp3([0.55, 0.62, 0.9], warm, daylight);
        light.color = Color::srgb(col[0], col[1], col[2]);
        light.illuminance = 400.0 + 10_600.0 * daylight;
        light.shadow_maps_enabled = settings.shadows;
    }
    ambient.brightness = 150.0 + 850.0 * daylight;
    ambient.color = Color::srgb(c[0] * 0.5 + 0.5, c[1] * 0.5 + 0.5, c[2] * 0.5 + 0.5);

    if let Some(mut m) = mats.and_then(|m| materials.get_mut(&m.lights)) {
        // Lamps glow a little by day, strongly at night.
        let k = 0.05 + sky.night * 5.0;
        m.emissive = LinearRgba::rgb(k, k * 0.7, k * 0.32);
    }
}
