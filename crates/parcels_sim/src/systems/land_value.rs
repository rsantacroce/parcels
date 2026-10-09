//! Stage 5: service coverage, traffic, pollution, crime and land value.
//!
//! Every effect spreads by physical distance and ignores parcel borders: a park
//! lifts the neighbour's tiles too, their police patrol your side of the street,
//! and their factory drags your values down.

use crate::catalog::Service;
use crate::map::{Density, Road, Terrain, TileKind, Zone};
use crate::state::GameState;

/// Add `strength` at `center`, falling off linearly to zero just past `radius`.
fn splat(state: &GameState, field: &mut [u32], center: crate::map::Pos, radius: u8, strength: u32) {
    if strength == 0 {
        return;
    }
    let r1 = radius as u32 + 1;
    for (j, d) in state.map.within(center, radius) {
        field[j] += strength * (r1 - d) / r1;
    }
}

pub fn run(state: &mut GameState) {
    let c = state.config.clone();
    let n = state.map.len();

    // Service coverage from powered service buildings, measured from the
    // middle of the footprint.
    let mut coverage = vec![0u8; n];
    for i in 0..n {
        let t = &state.map.tiles[i];
        let TileKind::Building(b) = t.kind else { continue };
        let Some(service) = b.service() else { continue };
        if !t.is_anchor() || !t.powered {
            continue;
        }
        let center = state.map.footprint_at(state.map.pos(i)).center();
        for (j, _) in state.map.within(center, c.building(b).service_radius) {
            coverage[j] |= service.bit();
        }
    }

    // Traffic: a road tile's congestion comes from the people and jobs around it.
    let mut traffic = vec![0u32; n];
    for i in 0..n {
        if let TileKind::Road(road) = state.map.tiles[i].kind {
            let near: u32 = state.map.within(state.map.pos(i), c.traffic_radius).map(|(j, _)| state.occupants(j)).sum();
            traffic[i] = near / c.traffic_divisor.max(1) as u32;
            if road == Road::Avenue {
                traffic[i] = traffic[i] * c.avenue_traffic_percent as u32 / 100;
            }
        }
    }

    // Pollution from industry, plants and busy roads; amenities; crime pressure.
    let mut pollution = vec![0u32; n];
    let mut amenity = vec![0u32; n];
    let mut shops = vec![0u32; n];
    let mut nature = vec![0u32; n];
    let mut crowd = vec![0u32; n];
    for i in 0..n {
        let t = &state.map.tiles[i];
        let pos = state.map.pos(i);
        let emitted = match t.kind {
            TileKind::Zone(Zone::Industrial, d) => {
                let base = c.industrial_pollution_per_level as u32 * (t.level as u32).max(1);
                if d == Density::High { base * 3 / 2 } else { base }
            }
            TileKind::Building(b) if t.is_anchor() => c.building(b).pollution as u32,
            TileKind::Road(_) => traffic[i] / c.traffic_pollution_divisor.max(1) as u32,
            _ => 0,
        };
        let at = if t.kind.building().is_some() { state.map.footprint_at(pos).center() } else { pos };
        splat(state, &mut pollution, at, c.pollution_radius, emitted);
        match t.kind {
            TileKind::Building(b) if t.is_anchor() => {
                let s = c.building(b);
                if t.powered || s.power_draw == 0 {
                    splat(state, &mut amenity, at, s.land_value_radius, s.land_value_bonus as u32);
                }
            }
            TileKind::Zone(Zone::Commercial, _) if t.level > 0 => {
                splat(state, &mut shops, pos, c.commercial_radius, c.commercial_bonus as u32)
            }
            _ => {}
        }
        if t.kind.is_zone() && t.level > 0 {
            splat(state, &mut crowd, pos, c.crime_radius, state.occupants(i));
        }
        match t.terrain {
            Terrain::Water => splat(state, &mut nature, pos, c.river_radius, c.river_bonus as u32),
            Terrain::Forest => splat(state, &mut nature, pos, c.forest_radius, c.forest_bonus as u32),
            Terrain::Land => {}
        }
    }

    for p in &mut pollution {
        *p = (*p).min(c.pollution_cap as u32);
    }

    let road_near: Vec<bool> = (0..n)
        .map(|i| state.map.tiles[i].kind.is_road() || state.map.neighbors4(i).any(|j| state.map.tiles[j].kind.is_road()))
        .collect();

    let smoothing = c.land_value_smoothing.max(1) as i32;
    for i in 0..n {
        let mut crime = crowd[i] / c.crime_divisor.max(1) as u32;
        if coverage[i] & Service::Police.bit() != 0 {
            crime = crime * (100 - c.police_crime_reduction_percent.min(100) as u32) / 100;
        }
        let crime = crime.min(c.crime_cap as u32);
        // Congestion nearby: the busiest adjacent road.
        let jam = state.map.neighbors4(i).map(|j| traffic[j]).max().unwrap_or(0).max(traffic[i]);
        let t = &state.map.tiles[i];
        let mut target = c.land_value_base as i32;
        // Diminishing returns on stacked amenities.
        target += amenity[i].min(400) as i32;
        target += nature[i].min(c.river_bonus.max(c.forest_bonus) as u32) as i32;
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
        for s in [Service::Health, Service::Education] {
            if coverage[i] & s.bit() != 0 {
                target += c.service_bonus as i32;
            }
        }
        if t.kind.zone() == Some(Zone::Industrial) {
            target -= 100;
        }
        target -= (pollution[i] * c.pollution_weight as u32) as i32;
        target -= (jam * c.traffic_weight as u32) as i32;
        target -= (crime * c.crime_weight as u32) as i32;
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
        t.crime = crime as u16;
        t.coverage = coverage[i];
    }
}
