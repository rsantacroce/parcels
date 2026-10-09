//! Keeps one set of meshes per 16×16-tile chunk in step with the simulation.
//!
//! Each tick (or overlay change) every chunk's visible state is hashed; chunks
//! whose hash moved are rebuilt, nearest to the camera first, within a frame
//! budget so a big map never stalls a frame.

use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use parcels_sim::{GameState, Pos, Rect};

use super::meshgen::{build_chunk, Look, TILE};
use super::scene::WorldMaterials;
use crate::camera::MainCamera;
use crate::driver::Driver;
use crate::view::{overlay_tint, Overlay, View};

pub const CHUNK: u16 = 16;
const FRAME_BUDGET: Duration = Duration::from_millis(7);

#[derive(Component)]
pub struct ChunkPart;

struct Entry {
    rect: Rect,
    hash: u64,
    built: Option<u64>,
    entities: Vec<Entity>,
}

#[derive(Resource, Default)]
pub struct Chunks {
    size: (u16, u16),
    generation: u64,
    tick: Option<u64>,
    look: Look,
    entries: Vec<Entry>,
}

impl Chunks {
    /// Fraction of chunks drawn up to date (for a loading hint).
    pub fn progress(&self) -> f32 {
        if self.entries.is_empty() {
            return 1.0;
        }
        self.entries.iter().filter(|e| e.built == Some(e.hash)).count() as f32 / self.entries.len() as f32
    }
}

fn chunk_hash(state: &GameState, look: Look, rect: Rect) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    look.hash(&mut h);
    // One tile of margin: roads, wires and banks look at their neighbours.
    let grow = Rect::from_corners(
        Pos::new(rect.min.x.saturating_sub(1), rect.min.y.saturating_sub(1)),
        Pos::new((rect.max.x + 1).min(state.map.width - 1), (rect.max.y + 1).min(state.map.height - 1)),
    );
    for p in grow.iter() {
        let idx = state.map.idx(p);
        let t = &state.map.tiles[idx];
        (t.terrain, t.kind, t.wire, t.pipe, t.level, t.part, t.burning > 0).hash(&mut h);
        if t.kind.building().is_some() {
            t.powered.hash(&mut h);
        }
        if look.overlay != Overlay::None {
            overlay_tint(state, look.overlay, idx).hash(&mut h);
        }
    }
    h.finish()
}

#[allow(clippy::too_many_arguments)]
pub fn sync_chunks(
    mut commands: Commands,
    driver: Option<Res<Driver>>,
    view: Res<View>,
    mut chunks: ResMut<Chunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mats: Option<Res<WorldMaterials>>,
    cam: Query<&GlobalTransform, With<MainCamera>>,
) {
    let Some(mats) = mats else { return };
    let state = driver.as_ref().and_then(|d| d.state());
    let Some(state) = state else {
        for e in chunks.entries.drain(..) {
            for ent in e.entities {
                commands.entity(ent).despawn();
            }
        }
        chunks.tick = None;
        return;
    };
    let generation = driver.as_ref().map_or(0, |d| d.generation);
    let size = (state.map.width, state.map.height);
    if chunks.size != size || chunks.generation != generation || chunks.entries.is_empty() {
        for e in chunks.entries.drain(..) {
            for ent in e.entities {
                commands.entity(ent).despawn();
            }
        }
        chunks.size = size;
        chunks.generation = generation;
        chunks.tick = None;
        for cy in (0..size.1).step_by(CHUNK as usize) {
            for cx in (0..size.0).step_by(CHUNK as usize) {
                let rect = Rect::from_corners(Pos::new(cx, cy), Pos::new((cx + CHUNK - 1).min(size.0 - 1), (cy + CHUNK - 1).min(size.1 - 1)));
                chunks.entries.push(Entry { rect, hash: 0, built: None, entities: Vec::new() });
            }
        }
    }
    let look = Look { overlay: view.overlay, show_pipes: view.show_pipes };
    if chunks.tick != Some(state.tick) || chunks.look != look {
        chunks.tick = Some(state.tick);
        chunks.look = look;
        for e in &mut chunks.entries {
            e.hash = chunk_hash(state, look, e.rect);
        }
    }

    // Nearest stale chunks first.
    let eye = cam.iter().next().map_or(Vec3::ZERO, |t| t.translation());
    let mut stale: Vec<(f32, usize)> = chunks
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.built != Some(e.hash))
        .map(|(i, e)| {
            let c = Vec3::new((e.rect.min.x + e.rect.max.x + 1) as f32, 0.0, (e.rect.min.y + e.rect.max.y + 1) as f32) * TILE / 2.0;
            (c.distance_squared(eye), i)
        })
        .collect();
    stale.sort_by(|a, b| a.0.total_cmp(&b.0));
    let start = Instant::now();
    for (_, i) in stale {
        if start.elapsed() > FRAME_BUDGET {
            break;
        }
        let rect = chunks.entries[i].rect;
        let built = build_chunk(state, look, rect);
        let mut ents = Vec::new();
        for (buf, mat, shadows) in [
            (built.solid, mats.solid.clone(), true),
            (built.glass, mats.glass.clone(), true),
            (built.lights, mats.lights.clone(), false),
            (built.water, mats.water.clone(), false),
        ] {
            if buf.is_empty() {
                continue;
            }
            let mut e = commands.spawn((Mesh3d(meshes.add(buf.into_mesh())), MeshMaterial3d(mat), ChunkPart));
            if !shadows {
                e.insert(NotShadowCaster);
            }
            ents.push(e.id());
        }
        let entry = &mut chunks.entries[i];
        for old in std::mem::replace(&mut entry.entities, ents) {
            commands.entity(old).despawn();
        }
        entry.built = Some(entry.hash);
    }
}
