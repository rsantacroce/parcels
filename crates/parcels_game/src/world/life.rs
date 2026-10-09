//! Cosmetic street life: cars and pedestrians wandering the road network, in
//! numbers that follow the neighbourhood's population. Render-only: they never
//! feed back into the simulation, so their randomness is free to be unseeded.

use bevy::prelude::*;
use parcels_sim::{GameState, Pos, Road, Terrain, TileKind};

use super::meshgen::{noise, MeshBuf, TILE};
use crate::driver::Driver;
use crate::settings::Settings;

#[derive(Component)]
pub struct Agent {
    from: Pos,
    to: Pos,
    t: f32,
    speed: f32,
    /// Sideways offset from the road centre line (lane or sidewalk).
    side: f32,
    walker: bool,
}

#[derive(Resource)]
pub struct LifeAssets {
    car: Handle<Mesh>,
    bus: Handle<Mesh>,
    person: Handle<Mesh>,
    paints: Vec<Handle<StandardMaterial>>,
    clothes: Vec<Handle<StandardMaterial>>,
}

#[derive(Resource, Default)]
pub struct LifeRng(u32);

impl LifeRng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(1);
        noise(self.0, 0x51ed, 77)
    }
}

pub fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut car = MeshBuf::default();
    car.cuboid(Vec3::new(-2.1, 0.3, -0.9), Vec3::new(2.1, 1.1, 0.9), [255, 255, 255], [255, 255, 255]);
    car.cuboid(Vec3::new(-1.1, 1.1, -0.8), Vec3::new(1.0, 1.7, 0.8), [60, 70, 90], [230, 230, 230]);
    for (x, z) in [(-1.3f32, -0.95f32), (1.3, -0.95), (-1.3, 0.75), (1.3, 0.75)] {
        car.block(Vec3::new(x - 0.35, 0.0, z), Vec3::new(x + 0.35, 0.6, z + 0.2), [30, 30, 30]);
    }
    let mut bus = MeshBuf::default();
    bus.cuboid(Vec3::new(-5.0, 0.4, -1.2), Vec3::new(5.0, 3.0, 1.2), [255, 255, 255], [220, 220, 220]);
    bus.block(Vec3::new(-4.6, 1.6, -1.25), Vec3::new(4.6, 2.5, 1.25), [50, 64, 84]);
    let mut person = MeshBuf::default();
    person.block(Vec3::new(-0.22, 0.0, -0.15), Vec3::new(0.22, 0.85, 0.15), [50, 50, 70]);
    person.cuboid(Vec3::new(-0.25, 0.85, -0.17), Vec3::new(0.25, 1.45, 0.17), [255, 255, 255], [255, 255, 255]);
    person.block(Vec3::new(-0.13, 1.47, -0.13), Vec3::new(0.13, 1.75, 0.13), [224, 186, 150]);
    let paint = |c: Color, materials: &mut Assets<StandardMaterial>| {
        materials.add(StandardMaterial { base_color: c, perceptual_roughness: 0.35, metallic: 0.3, ..default() })
    };
    let paints = [
        Color::srgb(0.85, 0.15, 0.12),
        Color::srgb(0.95, 0.95, 0.95),
        Color::srgb(0.1, 0.1, 0.12),
        Color::srgb(0.15, 0.35, 0.75),
        Color::srgb(0.95, 0.75, 0.1),
        Color::srgb(0.55, 0.58, 0.62),
        Color::srgb(0.2, 0.55, 0.3),
    ]
    .into_iter()
    .map(|c| paint(c, &mut materials))
    .collect();
    let clothes = [
        Color::srgb(0.8, 0.25, 0.2),
        Color::srgb(0.2, 0.45, 0.8),
        Color::srgb(0.9, 0.85, 0.3),
        Color::srgb(0.3, 0.7, 0.4),
        Color::srgb(0.9, 0.9, 0.9),
        Color::srgb(0.6, 0.3, 0.7),
    ]
    .into_iter()
    .map(|c| materials.add(StandardMaterial { base_color: c, perceptual_roughness: 0.9, ..default() }))
    .collect();
    commands.insert_resource(LifeAssets {
        car: meshes.add(car.into_mesh()),
        bus: meshes.add(bus.into_mesh()),
        person: meshes.add(person.into_mesh()),
        paints,
        clothes,
    });
}

fn is_road(state: &GameState, p: Pos) -> bool {
    state.map.get(p).is_some_and(|t| t.kind.is_road())
}

fn road_neighbours(state: &GameState, p: Pos) -> Vec<Pos> {
    let mut v = Vec::with_capacity(4);
    for (dx, dy) in [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)] {
        let (x, y) = (p.x as i32 + dx, p.y as i32 + dy);
        if x >= 0 && y >= 0 {
            let q = Pos::new(x as u16, y as u16);
            if is_road(state, q) {
                v.push(q);
            }
        }
    }
    v
}

fn center(p: Pos) -> Vec3 {
    Vec3::new((p.x as f32 + 0.5) * TILE, 0.0, (p.y as f32 + 0.5) * TILE)
}

/// Keep a population of cars and walkers proportional to roads and residents.
#[allow(clippy::too_many_arguments)]
pub fn populate(
    mut commands: Commands,
    driver: Option<Res<Driver>>,
    settings: Res<Settings>,
    assets: Option<Res<LifeAssets>>,
    mut rng: ResMut<LifeRng>,
    agents: Query<(Entity, &Agent)>,
    mut roads_cache: Local<(u64, u64, Vec<Pos>)>,
) {
    let Some(assets) = assets else { return };
    let Some(state) = driver.as_ref().and_then(|d| d.state()) else {
        for (e, _) in &agents {
            commands.entity(e).despawn();
        }
        return;
    };
    let generation = driver.as_ref().map_or(0, |d| d.generation);
    // Refresh the road list now and then, not every frame.
    if roads_cache.0 != generation || state.tick.abs_diff(roads_cache.1) >= 8 || roads_cache.2.is_empty() {
        roads_cache.0 = generation;
        roads_cache.1 = state.tick;
        roads_cache.2 = state.map.bounds().iter().filter(|&p| is_road(state, p)).collect();
    }
    let roads = &roads_cache.2;
    let mut cars = 0usize;
    let mut walkers = 0usize;
    for (e, a) in &agents {
        // Bulldozed out from under them.
        if !is_road(state, a.from) || !is_road(state, a.to) {
            commands.entity(e).despawn();
            continue;
        }
        if a.walker {
            walkers += 1;
        } else {
            cars += 1;
        }
    }
    if roads.is_empty() {
        return;
    }
    let busy = (state.global.population as f32 / (roads.len() as f32 * 6.0)).clamp(0.05, 1.0);
    let f = settings.crowds.factor();
    let want_cars = ((roads.len() as f32 * 0.5 * busy * f) as usize).min(500);
    let want_walkers = ((roads.len() as f32 * 0.35 * busy * f) as usize).min(300);
    for (e, a) in &agents {
        let excess = if a.walker { walkers > want_walkers } else { cars > want_cars };
        if excess {
            commands.entity(e).despawn();
            if a.walker {
                walkers -= 1;
            } else {
                cars -= 1;
            }
        }
    }
    let mut spawn = |walker: bool, rng: &mut LifeRng| {
        let from = roads[rng.next() as usize % roads.len()];
        let next = road_neighbours(state, from);
        if next.is_empty() {
            return;
        }
        let to = next[rng.next() as usize % next.len()];
        let r = rng.next();
        let bus = !walker && r % 23 == 0;
        let (mesh, mat) = if walker {
            (assets.person.clone(), assets.clothes[r as usize % assets.clothes.len()].clone())
        } else if bus {
            (assets.bus.clone(), assets.paints[4].clone())
        } else {
            (assets.car.clone(), assets.paints[r as usize % assets.paints.len()].clone())
        };
        let avenue = state.map.tile(from).kind == TileKind::Road(Road::Avenue);
        let side = if walker {
            if r % 2 == 0 { 4.1 } else { -4.1 }
        } else if avenue && r % 2 == 0 {
            3.4
        } else {
            1.7
        };
        let speed = if walker { 1.1 + (r % 5) as f32 * 0.15 } else if bus { 7.0 } else { 9.0 + (r % 7) as f32 };
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::from_translation(center(from)),
            Agent { from, to, t: (r % 100) as f32 / 100.0, speed, side, walker },
        ));
    };
    for _ in 0..(want_cars.saturating_sub(cars)).min(12) {
        spawn(false, &mut rng);
    }
    for _ in 0..(want_walkers.saturating_sub(walkers)).min(12) {
        spawn(true, &mut rng);
    }
}

pub fn drive(
    time: Res<Time<Real>>,
    driver: Option<Res<Driver>>,
    mut rng: ResMut<LifeRng>,
    mut agents: Query<(&mut Agent, &mut Transform)>,
) {
    let Some(state) = driver.as_ref().and_then(|d| d.state()) else { return };
    let paused = driver.as_ref().is_some_and(|d| d.paused);
    let dt = if paused { 0.0 } else { time.delta_secs().min(0.1) };
    for (mut a, mut tf) in &mut agents {
        a.t += a.speed * dt / TILE;
        while a.t >= 1.0 {
            a.t -= 1.0;
            // Pick the next street, avoiding U-turns unless it's a dead end.
            let opts: Vec<Pos> = road_neighbours(state, a.to).into_iter().filter(|&p| p != a.from).collect();
            let next = if opts.is_empty() { a.from } else { opts[rng.next() as usize % opts.len()] };
            a.from = a.to;
            a.to = next;
        }
        let (p0, p1) = (center(a.from), center(a.to));
        let dir = (p1 - p0).normalize_or_zero();
        // Right-hand traffic: offset to the right of the direction of travel.
        let right = Vec3::new(-dir.z, 0.0, dir.x);
        let bridge = state.map.get(a.from).is_some_and(|t| t.terrain == Terrain::Water)
            || state.map.get(a.to).is_some_and(|t| t.terrain == Terrain::Water);
        let y = if bridge { 0.4 } else if a.walker { 0.15 } else { 0.03 };
        let pos = p0.lerp(p1, a.t) + right * a.side + Vec3::Y * y;
        tf.translation = pos;
        if dir != Vec3::ZERO {
            let target = Transform::from_translation(pos).looking_to(dir, Vec3::Y).rotation;
            // Meshes are modelled along X; looking_to aligns -Z, so turn a quarter.
            let target = target * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
            tf.rotation = tf.rotation.slerp(target, (dt * 10.0).min(1.0));
        }
    }
}
