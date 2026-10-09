//! Moving bits that don't belong in the static chunk meshes: spinning wind
//! turbine rotors and flames on burning lots.

use std::collections::HashMap;

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use parcels_sim::{Building, Pos, TileKind};

use super::meshgen::{noise, tile_center, MeshBuf};
use crate::driver::Driver;

#[derive(Component)]
pub struct Rotor {
    speed: f32,
}

#[derive(Component)]
pub struct Flame {
    seed: f32,
}

#[derive(Resource)]
pub struct PropAssets {
    rotor: Handle<Mesh>,
    flame: Handle<Mesh>,
    white: Handle<StandardMaterial>,
    fire: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
pub struct Props {
    tick: Option<(u64, u64)>,
    rotors: HashMap<Pos, Entity>,
    flames: HashMap<Pos, Entity>,
}

pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut rotor = MeshBuf::default();
    rotor.block(Vec3::new(-0.5, -0.5, -0.4), Vec3::new(0.5, 0.5, 0.6), [240, 240, 236]);
    for k in 0..3 {
        let a = k as f32 * std::f32::consts::TAU / 3.0;
        let tip = Vec3::new(a.cos(), a.sin(), 0.0) * 13.0;
        rotor.bar(Vec3::ZERO, tip, 0.7, [240, 240, 236]);
    }
    let mut flame = MeshBuf::default();
    flame.cone(Vec3::ZERO, 1.6, 4.5, 6, [255, 150, 40]);
    flame.cone(Vec3::new(0.8, 0.0, 0.5), 1.0, 3.0, 6, [255, 210, 60]);
    commands.insert_resource(PropAssets {
        rotor: meshes.add(rotor.into_mesh()),
        flame: meshes.add(flame.into_mesh()),
        white: materials.add(StandardMaterial { base_color: Color::WHITE, perceptual_roughness: 0.6, ..default() }),
        fire: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            emissive: LinearRgba::rgb(40.0, 14.0, 2.0),
            unlit: true,
            ..default()
        }),
    });
}

pub fn sync_props(mut commands: Commands, driver: Option<Res<Driver>>, assets: Option<Res<PropAssets>>, mut props: ResMut<Props>) {
    let Some(assets) = assets else { return };
    let state = driver.as_ref().and_then(|d| d.state());
    let key = state.map(|s| (s.tick, driver.as_ref().map_or(0, |d| d.generation)));
    if props.tick == key {
        return;
    }
    props.tick = key;
    let mut want_rotors = HashMap::new();
    let mut want_flames = HashMap::new();
    if let Some(state) = state {
        for (i, t) in state.map.tiles.iter().enumerate() {
            match t.kind {
                TileKind::Building(Building::WindTurbine) => {
                    want_rotors.insert(state.map.pos(i), t.powered);
                }
                TileKind::Zone(..) if t.burning > 0 => {
                    want_flames.insert(state.map.pos(i), ());
                }
                _ => {}
            }
        }
    }
    let Props { rotors, flames, .. } = &mut *props;
    rotors.retain(|p, e| {
        let keep = want_rotors.contains_key(p);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    for (p, _) in want_rotors {
        rotors.entry(p).or_insert_with(|| {
            let hub = tile_center(p) + Vec3::new(0.0, 30.3, 1.2);
            let speed = 1.2 + (noise(p.x as u32, p.y as u32, 5) % 10) as f32 * 0.06;
            commands
                .spawn((
                    Mesh3d(assets.rotor.clone()),
                    MeshMaterial3d(assets.white.clone()),
                    Transform::from_translation(hub).with_rotation(Quat::from_rotation_z(p.x as f32)),
                    Rotor { speed },
                ))
                .id()
        });
    }
    flames.retain(|p, e| {
        let keep = want_flames.contains_key(p);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    for (p, _) in want_flames {
        flames.entry(p).or_insert_with(|| {
            commands
                .spawn((
                    Mesh3d(assets.flame.clone()),
                    MeshMaterial3d(assets.fire.clone()),
                    Transform::from_translation(tile_center(p)),
                    Flame { seed: (p.x as f32) * 1.7 + p.y as f32 },
                    NotShadowCaster,
                ))
                .id()
        });
    }
}

pub fn animate(time: Res<Time<Real>>, mut rotors: Query<(&Rotor, &mut Transform), Without<Flame>>, mut flames: Query<(&Flame, &mut Transform), Without<Rotor>>) {
    let dt = time.delta_secs();
    for (r, mut tf) in &mut rotors {
        tf.rotate_local_z(-r.speed * dt);
    }
    let t = time.elapsed_secs();
    for (f, mut tf) in &mut flames {
        let s = 1.0 + (t * 9.0 + f.seed).sin() * 0.18 + (t * 23.0 + f.seed * 3.0).sin() * 0.08;
        tf.scale = Vec3::new(1.0, s, 1.0);
        tf.rotation = Quat::from_rotation_y(t * 0.7 + f.seed);
    }
}
