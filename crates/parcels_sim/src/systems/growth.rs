//! Stage 4: zones grow or decay.
//!
//! Map-wide demand is a limited pool of new residents and jobs. Every eligible
//! zone tile on the map competes for it, ranked by land value plus its owner's
//! demand, so attractive neighbourhoods — on either side of a border — fill first.

use crate::map::Zone;
use crate::rng::Rng;
use crate::state::{GameState, TickReport};

/// Does any road lie within Manhattan `radius` of each tile?
fn road_access(state: &GameState, radius: u8) -> Vec<bool> {
    let map = &state.map;
    let mut access = vec![false; map.len()];
    for i in 0..map.len() {
        if map.tiles[i].kind == crate::map::TileKind::Road {
            for (j, _) in map.within(map.pos(i), radius) {
                access[j] = true;
            }
        }
    }
    access
}

pub fn run(state: &mut GameState, report: &mut TickReport) {
    let access = road_access(state, state.config.road_access_radius);
    let mut rng = Rng::for_tick(state.seed, state.tick, 0x6752_4f57);
    let c = state.config.clone();

    for zone in Zone::ALL {
        let z = zone.index();
        let per = match zone {
            Zone::Residential => c.residents_per_level,
            Zone::Commercial => c.commercial_jobs_per_level,
            Zone::Industrial => c.industrial_jobs_per_level,
        } as i32;
        let raw = state.global.raw_demand[z];
        let cap = c.max_growth_per_tick as i32;
        let pool = if raw > 0 { ((raw + per - 1) / per).min(cap) } else { -((-raw / per).min(cap)) };

        let mut grow: Vec<(i32, usize)> = Vec::new();
        let mut shrink: Vec<(i32, usize)> = Vec::new();
        let mut decay: Vec<usize> = Vec::new();

        for i in 0..state.map.len() {
            let t = &state.map.tiles[i];
            if t.kind != zone.kind() {
                continue;
            }
            let Some(owner) = state.owner_of_idx(i) else { continue };
            let pdemand = state.players[owner.index()].stats.demand[z];
            let jitter = rng.below(c.growth_jitter as u32 + 1) as i32;

            let sustained = t.powered && access[i] && (t.level <= c.water_free_levels || t.watered);
            if !sustained {
                if t.level > 0 && rng.chance(c.decay_chance_per_mille) {
                    decay.push(i);
                }
                continue;
            }
            // Punishing taxes drive people out even when the city wants to grow.
            if pdemand < -500 && t.level > 0 && rng.chance(c.decay_chance_per_mille / 2) {
                decay.push(i);
                continue;
            }
            let score = t.land_value as i32 + pdemand / 4 + jitter;
            let water_ok = t.level < c.water_free_levels || t.watered;
            if pdemand > 0 && t.level < c.max_level && water_ok {
                grow.push((score, i));
            }
            if t.level > 0 {
                shrink.push((score, i));
            }
        }

        for i in decay {
            state.map.tiles[i].level -= 1;
            report.decayed += 1;
        }
        if pool > 0 {
            grow.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            for &(_, i) in grow.iter().take(pool as usize) {
                state.map.tiles[i].level += 1;
                report.grew += 1;
            }
        } else if pool < 0 {
            shrink.sort_unstable();
            for &(_, i) in shrink.iter().take((-pool) as usize) {
                let t = &mut state.map.tiles[i];
                // May already have decayed above.
                if t.level > 0 {
                    t.level -= 1;
                    report.decayed += 1;
                }
            }
        }
    }
}
