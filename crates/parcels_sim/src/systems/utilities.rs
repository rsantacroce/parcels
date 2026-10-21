//! Stage 2: flood-fill power and water networks and serve tiles from them.
//!
//! Each connected network shares its supply; a short network serves tiles in
//! row-major order until it runs out.

use crate::city::{Utility, UtilityLedger};
use crate::map::{Terrain, Tile, TileKind};
use crate::state::GameState;

pub fn run(state: &mut GameState) {
    for t in &mut state.map.tiles {
        t.powered = false;
        t.watered = false;
    }
    state.city.stats.power = UtilityLedger::default();
    state.city.stats.water = UtilityLedger::default();
    // Power first: pumps need power to pump water.
    settle(state, Utility::Power);
    settle(state, Utility::Water);
}

fn conducts(t: &Tile, u: Utility) -> bool {
    match u {
        Utility::Power => t.conducts_power(),
        Utility::Water => t.conducts_water(),
    }
}

fn need(state: &GameState, t: &Tile, u: Utility) -> u32 {
    match (u, t.kind) {
        (Utility::Power, TileKind::Zone(..)) => 1 + t.level as u32,
        (Utility::Power, TileKind::Building(b)) if t.is_anchor() => state.config.building(b).power_draw,
        (Utility::Water, TileKind::Zone(..)) => t.level as u32,
        _ => 0,
    }
}

/// Does this tile produce water (and so need its power before anyone else)?
fn makes_water(state: &GameState, t: &Tile) -> bool {
    t.kind.building().is_some_and(|b| state.config.building(b).water_supply > 0)
}

fn supply(state: &GameState, idx: usize, u: Utility) -> u32 {
    let t = &state.map.tiles[idx];
    let TileKind::Building(b) = t.kind else { return 0 };
    if !t.is_anchor() {
        return 0;
    }
    let stats = state.config.building(b);
    match u {
        Utility::Power => stats.power_supply,
        // Water producers pump nothing without power.
        Utility::Water if stats.water_supply > 0 && (t.powered || stats.power_draw == 0) => {
            let fp = state.map.footprint_at(state.map.pos(idx));
            let by_river = stats.river_bonus > 0
                && fp.iter().any(|p| state.map.neighbors4(state.map.idx(p)).any(|n| state.map.tiles[n].terrain == Terrain::Water));
            stats.water_supply + if by_river { stats.river_bonus } else { 0 }
        }
        Utility::Water => 0,
    }
}

fn ledger(state: &mut GameState, u: Utility) -> &mut UtilityLedger {
    let s = &mut state.city.stats;
    match u {
        Utility::Power => &mut s.power,
        Utility::Water => &mut s.water,
    }
}

fn settle(state: &mut GameState, u: Utility) {
    let n = state.map.len();
    let mut visited = vec![false; n];
    let mut stack = Vec::new();
    for start in 0..n {
        if visited[start] || !conducts(&state.map.tiles[start], u) {
            continue;
        }
        let mut members = Vec::new();
        visited[start] = true;
        stack.push(start);
        while let Some(i) = stack.pop() {
            members.push(i);
            for nb in state.map.neighbors4(i) {
                if !visited[nb] && conducts(&state.map.tiles[nb], u) {
                    visited[nb] = true;
                    stack.push(nb);
                }
            }
        }
        // Serve in row-major order regardless of traversal order.
        members.sort_unstable();
        settle_network(state, u, &members);
    }
}

fn settle_network(state: &mut GameState, u: Utility, members: &[usize]) {
    let supply_total: u32 = members.iter().map(|&i| supply(state, i, u)).sum();
    let demand_total: u32 = members.iter().map(|&i| need(state, &state.map.tiles[i], u)).sum();
    let l = ledger(state, u);
    l.supply += supply_total;
    l.demand += demand_total;

    // Hand out the supply tile by tile. Water producers go first so a short
    // network keeps its water running.
    let mut budget = supply_total;
    for pumps_pass in [true, false] {
        for &i in members {
            let t = &state.map.tiles[i];
            if makes_water(state, t) != pumps_pass {
                continue;
            }
            let d = need(state, t, u);
            let is_source = supply(state, i, u) > 0;
            let served = if d == 0 {
                supply_total > 0
            } else if budget >= d {
                budget -= d;
                true
            } else {
                ledger(state, u).unserved += d;
                false
            };
            let t = &mut state.map.tiles[i];
            match u {
                Utility::Power => t.powered = served || is_source,
                Utility::Water => t.watered = served || is_source,
            }
        }
    }
}
