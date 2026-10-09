//! Stage 4: zones grow, decay or burn.
//!
//! Map-wide demand is a limited pool of new residents and jobs. Every eligible
//! zone tile on the map competes for it, ranked by land value plus its owner's
//! demand, so attractive neighbourhoods — on either side of a border — fill first.
//!
//! What a tile may grow into depends on what's around it: water past the first
//! level, a school for mid-rise housing and offices, a hospital or clinic for the
//! tallest towers, and good land for dense zones to outgrow the low-density cap.

use crate::catalog::Service;
use crate::config::Config;
use crate::map::{Density, Tile, TileKind, Zone};
use crate::rng::Rng;
use crate::state::{GameState, TickReport};

/// Does any road lie within Manhattan `radius` of each tile?
fn road_access(state: &GameState, radius: u8) -> Vec<bool> {
    let map = &state.map;
    let mut access = vec![false; map.len()];
    for i in 0..map.len() {
        if map.tiles[i].kind.is_road() {
            for (j, _) in map.within(map.pos(i), radius) {
                access[j] = true;
            }
        }
    }
    access
}

/// Why a tile can't reach its next level, if it can't. Shown in the inspector.
pub fn growth_block(c: &Config, t: &Tile) -> Option<&'static str> {
    let TileKind::Zone(zone, density) = t.kind else { return None };
    let next = t.level + 1;
    if t.burning > 0 {
        return Some("rebuilding after a fire");
    }
    if next > c.max_level(density) {
        return Some(if density == Density::Low { "fully grown (low density)" } else { "fully grown" });
    }
    if next > c.water_free_levels && !t.watered {
        return Some("needs water to grow");
    }
    if density == Density::High && next > c.low_density_max_level && t.land_value < c.dense_min_land_value {
        return Some("land too cheap for towers");
    }
    let needs_school = match zone {
        Zone::Residential => next > c.education_free_levels,
        Zone::Office => next > 1,
        _ => false,
    };
    if needs_school && !t.covered(Service::Education) {
        return Some("needs a school nearby");
    }
    if next > c.health_free_levels && !t.covered(Service::Health) {
        return Some("needs a clinic or hospital nearby");
    }
    None
}

pub fn run(state: &mut GameState, report: &mut TickReport) {
    let access = road_access(state, state.config.road_access_radius);
    let mut rng = Rng::for_tick(state.seed, state.tick, 0x6752_4f57);
    let c = state.config.clone();

    for t in &mut state.map.tiles {
        t.burning = t.burning.saturating_sub(1);
    }

    for zone in Zone::ALL {
        let z = zone.index();
        let per = c.per_level(zone) as i32;
        let raw = state.global.raw_demand[z];
        let cap = c.max_growth_per_tick as i32;
        let pool = if raw > 0 { ((raw + per - 1) / per).min(cap) } else { -((-raw / per).min(cap)) };

        let mut grow: Vec<(i32, usize)> = Vec::new();
        let mut shrink: Vec<(i32, usize)> = Vec::new();
        let mut decay: Vec<usize> = Vec::new();

        for i in 0..state.map.len() {
            let t = &state.map.tiles[i];
            if t.kind.zone() != Some(zone) {
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
            if pdemand > 0 && growth_block(&c, t).is_none() {
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

    fires(state, report);
}

/// Built-up lots without a fire station nearby occasionally burn down.
fn fires(state: &mut GameState, report: &mut TickReport) {
    let chance = state.config.fire_chance_per_million;
    if chance == 0 {
        return;
    }
    let mut rng = Rng::for_tick(state.seed, state.tick, 0x4649_5245);
    for i in 0..state.map.len() {
        let t = &state.map.tiles[i];
        if !t.kind.is_zone() || t.level < 2 || t.covered(Service::Fire) {
            continue;
        }
        if rng.below(1_000_000) < chance {
            let rubble = state.config.fire_rubble_ticks;
            let owner = state.owner_of_idx(i);
            let t = &mut state.map.tiles[i];
            t.level = 0;
            t.burning = rubble.max(1);
            report.fires.push(state.map.pos(i));
            if let Some(o) = owner {
                state.players[o.index()].stats.fires += 1;
            }
        }
    }
}
