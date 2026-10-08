//! Stage 5: traffic, pollution and land value.
//!
//! Every effect spreads by physical distance and ignores parcel borders: a park
//! lifts the neighbour's tiles too, and their factory drags yours down.

use crate::map::{Terrain, TileKind, Zone};
use crate::state::GameState;

/// Add `strength` at `center`, falling off linearly to zero just past `radius`.
fn splat(state: &GameState, field: &mut [u32], idx: usize, radius: u8, strength: u32) {
    if strength == 0 {
        return;
    }
    let r1 = radius as u32 + 1;
    for (j, d) in state.map.within(state.map.pos(idx), radius) {
        field[j] += strength * (r1 - d) / r1;
    }
}

pub fn run(state: &mut GameState) {
    let c = state.config.clone();
    let n = state.map.len();

    // Traffic: a road tile's congestion comes from the people and jobs around it.
    let mut traffic = vec![0u32; n];
    for i in 0..n {
        if state.map.tiles[i].kind == TileKind::Road {
            let near: u32 = state.map.within(state.map.pos(i), c.traffic_radius).map(|(j, _)| state.occupants(j)).sum();
            traffic[i] = near / c.traffic_divisor.max(1) as u32;
        }
    }

    // Pollution from industry, power plants and busy roads.
    let mut pollution = vec![0u32; n];
    let mut parks = vec![0u32; n];
    let mut shops = vec![0u32; n];
    let mut river = vec![0u32; n];
    for i in 0..n {
        let t = &state.map.tiles[i];
        let emitted = match t.kind {
            TileKind::Industrial => c.industrial_pollution_per_level as u32 * (t.level as u32).max(1),
            TileKind::PowerPlant => c.power_plant_pollution as u32,
            TileKind::Road => traffic[i] / c.traffic_pollution_divisor.max(1) as u32,
            _ => 0,
        };
        splat(state, &mut pollution, i, c.pollution_radius, emitted);
        match t.kind {
            TileKind::Park => splat(state, &mut parks, i, c.park_radius, c.park_bonus as u32),
            TileKind::Commercial if t.level > 0 => splat(state, &mut shops, i, c.commercial_radius, c.commercial_bonus as u32),
            _ => {}
        }
        if t.terrain == Terrain::Water {
            splat(state, &mut river, i, c.river_radius, c.river_bonus as u32);
        }
    }

    for p in &mut pollution {
        *p = (*p).min(c.pollution_cap as u32);
    }

    let road_near: Vec<bool> = (0..n)
        .map(|i| {
            state.map.tiles[i].kind == TileKind::Road || state.map.neighbors4(i).any(|j| state.map.tiles[j].kind == TileKind::Road)
        })
        .collect();

    let smoothing = c.land_value_smoothing.max(1) as i32;
    for i in 0..n {
        // Congestion nearby: the busiest adjacent road.
        let jam = state.map.neighbors4(i).map(|j| traffic[j]).max().unwrap_or(0).max(traffic[i]);
        let t = &state.map.tiles[i];
        let mut target = c.land_value_base as i32;
        // Diminishing returns on stacked amenities.
        target += parks[i].min(c.park_bonus as u32 * 2) as i32;
        target += river[i].min(c.river_bonus as u32) as i32;
        target += shops[i].min(c.commercial_bonus as u32 * 3) as i32;
        if road_near[i] {
            target += c.road_access_bonus as i32;
        }
        if t.powered {
            target += c.powered_bonus as i32;
        }
        if t.watered {
            target += c.watered_bonus as i32;
        }
        if t.kind.zone() == Some(Zone::Industrial) {
            target -= 100;
        }
        target -= (pollution[i] * c.pollution_weight as u32) as i32;
        target -= (jam * c.traffic_weight as u32) as i32;
        if t.terrain == Terrain::Water {
            target = 0;
        }
        let target = target.clamp(0, c.land_value_max as i32);

        let cur = t.land_value as i32;
        let diff = target - cur;
        let mut stepv = diff / smoothing;
        if stepv == 0 && diff != 0 {
            stepv = diff.signum();
        }
        let t = &mut state.map.tiles[i];
        t.land_value = (cur + stepv) as u16;
        t.pollution = pollution[i] as u16;
        t.traffic = traffic[i].min(u16::MAX as u32) as u16;
    }
}
