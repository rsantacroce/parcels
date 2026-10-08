//! Simple rule-based neighbours. The AI is just another command source: it reads
//! the state and returns commands, exactly like a human clicking. In multiplayer
//! only the host runs it, and its commands travel the same stream as everyone's.
//!
//! Deterministic: depends only on the state passed in.

use crate::command::{Area, CommandKind};
use crate::ids::PlayerId;
use crate::map::{Buildable, Pos, Rect, Terrain, TileKind, Zone};
use crate::player::{AiStrategy, Controller, Parcel, Utility};
use crate::state::GameState;
use crate::systems::apply::{plan_bulldoze, plan_place};

const RESERVE: i64 = 1_000_00;

/// Should this player think on this tick? Players are staggered.
pub fn thinks_now(state: &GameState, player: PlayerId) -> bool {
    let interval = state.config.ai_think_interval.max(1) as u64;
    (state.tick + player.0 as u64) % interval == 0
}

/// Commands an AI player wants to issue this tick (usually zero or one).
pub fn plan(state: &GameState, player: PlayerId) -> Vec<CommandKind> {
    let Some(p) = state.player(player) else { return Vec::new() };
    let Controller::Ai(strategy) = p.controller else { return Vec::new() };
    if state.is_over() || !thinks_now(state, player) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for parcel in state.parcels_of(player) {
        if let Some(cmd) = next_build(state, player, strategy, parcel) {
            out.push(cmd);
            break;
        }
    }
    out.extend(policy(state, player, strategy));
    out
}

fn road_rows(r: &Rect) -> Vec<u16> {
    (r.min.y + 1..r.max.y).step_by(3).collect()
}

fn spine_x(r: &Rect) -> u16 {
    r.min.x + 1
}

fn try_place(state: &GameState, who: PlayerId, parcel: &Parcel, tiles: Vec<Pos>, what: Buildable) -> Option<CommandKind> {
    if tiles.is_empty() {
        return None;
    }
    let area = Area::Tiles(tiles);
    let plan = plan_place(state, who, parcel.id, &area, what).ok()?;
    let treasury = state.players[who.index()].treasury;
    (treasury - plan.cost >= RESERVE).then_some(CommandKind::Place { parcel: parcel.id, area, what })
}

fn next_build(state: &GameState, who: PlayerId, strategy: AiStrategy, parcel: &Parcel) -> Option<CommandKind> {
    let r = parcel.rect;
    let map = &state.map;
    let stats = &state.players[who.index()].stats;
    let rows = road_rows(&r);
    let sx = spine_x(&r);

    // 1. The spine and the first two streets. Both run edge to edge so they meet
    //    the neighbours' roads, joining everyone into one network.
    let spine: Vec<Pos> = (r.min.y..=r.max.y).map(|y| Pos::new(sx, y)).collect();
    let streets = |n: usize| -> Vec<Pos> {
        rows.iter().take(n).flat_map(|&y| (r.min.x..=r.max.x).map(move |x| Pos::new(x, y))).collect()
    };
    if let Some(c) = try_place(state, who, parcel, spine.clone(), Buildable::Road) {
        return Some(c);
    }
    if let Some(c) = try_place(state, who, parcel, streets(2), Buildable::Road) {
        return Some(c);
    }

    // Utilities ride along every road, so any zone on a street is connected and
    // the edge-to-edge streets link the network to the neighbours.
    let roads: Vec<Pos> = r.iter().filter(|&p| map.tile(p).kind == TileKind::Road).collect();
    let count = |k: TileKind| r.iter().filter(|&p| map.tile(p).kind == k).count();

    // 2. Utilities: lines and pipes ride along the roads, plants sit beside the spine.
    let wants_plant = match strategy {
        AiStrategy::UtilityBaron => count(TileKind::PowerPlant) == 0 || stats.power.supply < stats.power.demand + 100,
        AiStrategy::Developer => stats.power.unserved > 0 && stats.power.bought == 0,
    };
    let wants_pump = match strategy {
        AiStrategy::UtilityBaron => count(TileKind::WaterPump) == 0 || stats.water.supply < stats.water.demand + 60,
        AiStrategy::Developer => stats.water.unserved > 0 && stats.water.bought == 0 && stats.population > 60,
    };
    // Barons overbuild utilities to sell; developers keep a token plant and buy.
    let utility_cap = match strategy {
        AiStrategy::UtilityBaron => 8,
        AiStrategy::Developer => 2,
    };
    let beside_spine = |y_from_bottom: bool| -> Vec<Pos> {
        let mut ys: Vec<u16> = (r.min.y..=r.max.y).collect();
        if y_from_bottom {
            ys.reverse();
        }
        ys.into_iter()
            .map(|y| Pos::new(r.min.x, y))
            .filter(|&p| map.tile(p).is_bare() && map.tile(p).terrain == Terrain::Land)
            .filter(|&p| map.tile(Pos::new(sx, p.y)).kind == TileKind::Road && !rows.contains(&p.y))
            .take(1)
            .collect()
    };
    if wants_plant && count(TileKind::PowerPlant) < utility_cap {
        if let Some(c) = try_place(state, who, parcel, beside_spine(false), Buildable::PowerPlant) {
            return Some(c);
        }
    }
    let unwired: Vec<Pos> = roads.iter().copied().filter(|&p| !map.tile(p).wire).collect();
    if let Some(c) = try_place(state, who, parcel, unwired, Buildable::PowerLine) {
        return Some(c);
    }
    if wants_pump && count(TileKind::WaterPump) < utility_cap {
        if let Some(c) = try_place(state, who, parcel, beside_spine(true), Buildable::WaterPump) {
            return Some(c);
        }
    }
    if count(TileKind::WaterPump) > 0 || stats.water.bought > 0 {
        let unpiped: Vec<Pos> = roads.iter().copied().filter(|&p| !map.tile(p).pipe).collect();
        if let Some(c) = try_place(state, who, parcel, unpiped, Buildable::WaterPipe) {
            return Some(c);
        }
    }

    // 3. Clear out dead industry next to housing once rich (cheap land value fix).
    // 4. Zone whatever is most in demand, if there isn't already empty zoning waiting.
    let mut demand: Vec<(i32, Zone)> = Zone::ALL.iter().map(|&z| (stats.demand[z.index()], z)).collect();
    if strategy == AiStrategy::Developer {
        demand[Zone::Residential.index()].0 += 150;
    }
    demand.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (d, zone) in demand {
        if d <= 0 {
            break;
        }
        let waiting = r.iter().filter(|&p| map.tile(p).kind == zone.kind() && map.tile(p).level == 0).count();
        if waiting >= 6 {
            continue;
        }
        let mut spots: Vec<Pos> = r
            .iter()
            .filter(|&p| p.x > r.min.x)
            .filter(|&p| {
                let t = map.tile(p);
                t.kind == TileKind::Empty && t.terrain == Terrain::Land && !t.wire
            })
            .filter(|&p| map.neighbors4(map.idx(p)).any(|j| map.tiles[j].kind == TileKind::Road))
            .collect();
        // Industry goes to the far end of the parcel, away from homes.
        if zone == Zone::Industrial {
            spots.reverse();
        }
        spots.truncate(8);
        if let Some(c) = try_place(state, who, parcel, spots, zone.buildable()) {
            return Some(c);
        }
    }

    // 5. More streets only once there's nowhere left to zone.
    let free_frontage = r
        .iter()
        .filter(|&p| p.x > r.min.x && map.tile(p).kind == TileKind::Empty && map.tile(p).terrain == Terrain::Land)
        .filter(|&p| map.neighbors4(map.idx(p)).any(|j| map.tiles[j].kind == TileKind::Road))
        .count();
    for n in (3..=rows.len()).filter(|_| free_frontage < 4) {
        if let Some(c) = try_place(state, who, parcel, streets(n), Buildable::Road) {
            return Some(c);
        }
    }

    // 6. Bulldoze abandoned industry that is poisoning homes, then add parks.
    let dead_ind: Vec<Pos> = r
        .iter()
        .filter(|&p| map.tile(p).kind == TileKind::Industrial && map.tile(p).level == 0 && map.tile(p).pollution > 80)
        .take(4)
        .collect();
    if !dead_ind.is_empty() && state.players[who.index()].treasury > 10_000_00 {
        let area = Area::Tiles(dead_ind);
        if plan_bulldoze(state, who, parcel.id, &area).is_ok() {
            return Some(CommandKind::Bulldoze { parcel: parcel.id, area });
        }
    }
    if state.players[who.index()].treasury > 12_000_00 {
        let spot: Vec<Pos> = r
            .iter()
            .filter(|&p| map.tile(p).is_bare() && map.tile(p).terrain == Terrain::Land)
            .filter(|&p| map.within(p, 2).any(|(j, _)| map.tiles[j].kind == TileKind::Residential))
            .filter(|&p| !map.within(p, 3).any(|(j, _)| map.tiles[j].kind == TileKind::Park))
            .take(1)
            .collect();
        if let Some(c) = try_place(state, who, parcel, spot, Buildable::Park) {
            return Some(c);
        }
    }
    None
}

fn policy(state: &GameState, who: PlayerId, strategy: AiStrategy) -> Vec<CommandKind> {
    let p = &state.players[who.index()];
    let s = &p.stats;
    let mut out = Vec::new();
    let (low, high) = match strategy {
        AiStrategy::UtilityBaron => (6, 11),
        AiStrategy::Developer => (4, 9),
    };
    let tax = if p.treasury < 3_000_00 { (p.tax_rate + 1).min(high) } else if p.treasury > 25_000_00 { p.tax_rate.saturating_sub(1).max(low) } else { p.tax_rate };
    if tax != p.tax_rate {
        out.push(CommandKind::SetTaxRate { rate: tax });
    }
    let base = state.config.default_utility_price;
    let markup = if strategy == AiStrategy::UtilityBaron { 2 } else { 0 };
    for (u, ledger, price) in [(Utility::Power, &s.power, p.power_price), (Utility::Water, &s.water, p.water_price)] {
        let has_surplus = ledger.supply > ledger.own_use;
        if has_surplus && price.is_none() {
            out.push(CommandKind::SetUtilityPrice { utility: u, price: Some(base + markup) });
        }
    }
    out
}
