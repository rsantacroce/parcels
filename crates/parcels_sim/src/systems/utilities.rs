//! Stage 2: flood-fill power and water networks, serve tiles, settle neighbour trades.
//!
//! Networks ignore parcel borders. Within each connected network every owner
//! first uses their own supply; owners who are short then buy surplus from
//! neighbours who have set a price, cheapest seller first.

use crate::map::{Terrain, Tile, TileKind};
use crate::player::{Utility, UtilityLedger};
use crate::state::{GameState, Trade};

pub fn run(state: &mut GameState) {
    for t in &mut state.map.tiles {
        t.powered = false;
        t.watered = false;
    }
    for p in &mut state.players {
        p.stats.power = UtilityLedger::default();
        p.stats.water = UtilityLedger::default();
    }
    state.trades.clear();
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

fn ledger(state: &mut GameState, owner: usize, u: Utility) -> &mut UtilityLedger {
    let s = &mut state.players[owner].stats;
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
    let np = state.players.len();
    let mut supply_by = vec![0u32; np];
    let mut demand_by = vec![0u32; np];
    for &i in members {
        let Some(o) = state.owner_of_idx(i) else { continue };
        supply_by[o.index()] += supply(state, i, u);
        demand_by[o.index()] += need(state, &state.map.tiles[i], u);
    }
    let total_supply: u32 = supply_by.iter().sum();
    if total_supply == 0 {
        for &i in members {
            if let Some(o) = state.owner_of_idx(i) {
                let d = need(state, &state.map.tiles[i], u);
                ledger(state, o.index(), u).unserved += d;
                ledger(state, o.index(), u).demand += d;
            }
        }
        return;
    }

    let mut surplus = vec![0u32; np];
    let mut short = vec![0u32; np];
    let mut granted = vec![0u32; np];
    for p in 0..np {
        let own = supply_by[p].min(demand_by[p]);
        surplus[p] = supply_by[p] - own;
        short[p] = demand_by[p] - own;
        granted[p] = own;
        let l = ledger(state, p, u);
        l.supply += supply_by[p];
        l.demand += demand_by[p];
        l.own_use += own;
    }

    // Sellers: cheapest first, then by id. Buyers: by id, must be solvent.
    let mut sellers: Vec<(u32, usize)> = (0..np)
        .filter(|&p| surplus[p] > 0)
        .filter_map(|p| state.players[p].price(u).map(|price| (price, p)))
        .collect();
    sellers.sort_unstable();
    for b in 0..np {
        if short[b] == 0 || state.players[b].treasury <= 0 {
            continue;
        }
        for &(price, s) in &sellers {
            if s == b || short[b] == 0 {
                continue;
            }
            let units = short[b].min(surplus[s]);
            if units == 0 {
                continue;
            }
            short[b] -= units;
            surplus[s] -= units;
            granted[b] += units;
            ledger(state, b, u).bought += units;
            ledger(state, s, u).sold += units;
            let (seller, buyer) = (state.players[s].id, state.players[b].id);
            match state.trades.iter_mut().find(|t| t.utility == u && t.seller == seller && t.buyer == buyer) {
                Some(t) => t.units += units,
                None => state.trades.push(Trade { utility: u, seller, buyer, units, price }),
            }
        }
    }

    // Hand out each owner's granted units tile by tile. Water producers go first
    // so a short network keeps its water running.
    let mut budget = granted;
    for pumps_pass in [true, false] {
        for &i in members {
            let t = &state.map.tiles[i];
            if makes_water(state, t) != pumps_pass {
                continue;
            }
            let Some(o) = state.owner_of_idx(i) else { continue };
            let d = need(state, t, u);
            let is_source = supply(state, i, u) > 0;
            let served = if d == 0 {
                true
            } else if budget[o.index()] >= d {
                budget[o.index()] -= d;
                true
            } else {
                ledger(state, o.index(), u).unserved += d;
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
