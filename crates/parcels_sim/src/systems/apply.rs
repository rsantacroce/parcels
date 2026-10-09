//! Stage 1: apply this tick's commands in a deterministic order.

use std::collections::BTreeSet;

use crate::command::{Area, Command, CommandKind, Rejection};
use crate::ids::{ParcelId, PlayerId};
use crate::map::{Buildable, Pos, Rect, Road, Terrain, Tile, TileKind};
use crate::player::Utility;
use crate::state::{GameState, TickReport};

/// The tiles a placement would actually change, and what it would cost.
/// Also used by the client for the ghost preview, so preview and reality agree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub tiles: Vec<Pos>,
    /// Footprints of buildings placed (one per anchor); empty for everything else.
    pub footprints: Vec<Rect>,
    pub cost: i64,
}

fn can_place(tile: &Tile, what: Buildable) -> bool {
    let land = tile.terrain != Terrain::Water;
    match what {
        Buildable::Road(Road::Street) => tile.kind == TileKind::Empty,
        Buildable::Road(Road::Avenue) => matches!(tile.kind, TileKind::Empty | TileKind::Road(Road::Street)),
        Buildable::PowerLine => !tile.wire && matches!(tile.kind, TileKind::Empty | TileKind::Road(_)),
        Buildable::WaterPipe => !tile.pipe && land,
        Buildable::Zone(z, d) => {
            if !land {
                return false;
            }
            match tile.kind {
                TileKind::Empty => !tile.wire,
                // Rezone an empty lot, or densify a built one of the same type.
                TileKind::Zone(z0, d0) => (z0, d0) != (z, d) && (tile.level == 0 || (z0 == z && d0 < d)),
                _ => false,
            }
        }
        Buildable::Building(_) => tile.kind == TileKind::Empty && land && !tile.wire,
    }
}

fn place_cost(state: &GameState, tile: &Tile, what: Buildable) -> i64 {
    let mut cost = match what {
        Buildable::Building(_) => 0,
        _ => state.config.cost(what),
    };
    if matches!(what, Buildable::Road(_)) && tile.terrain == Terrain::Water {
        cost *= state.config.bridge_cost_multiplier;
    }
    if tile.terrain == Terrain::Forest && what != Buildable::WaterPipe {
        cost += state.config.clear_forest_cost;
    }
    cost
}

/// Check ownership and that every tile lies in `parcel`.
fn check_parcel(state: &GameState, author: PlayerId, parcel: ParcelId, tiles: &[Pos]) -> Result<(), Rejection> {
    state.player(author).ok_or(Rejection::UnknownPlayer)?;
    let pc = state.parcel(parcel).ok_or(Rejection::UnknownParcel)?;
    if pc.owner != author {
        return Err(Rejection::NotYourParcel);
    }
    for &p in tiles {
        if !pc.rect.contains(p) || !state.map.in_bounds(p) {
            return Err(Rejection::OutsideParcel(p));
        }
    }
    Ok(())
}

fn afford(state: &GameState, author: PlayerId, plan: Plan) -> Result<Plan, Rejection> {
    if plan.tiles.is_empty() {
        return Err(Rejection::NothingToDo);
    }
    let have = state.players[author.index()].treasury;
    if plan.cost > have {
        return Err(Rejection::InsufficientFunds { need: plan.cost, have });
    }
    Ok(plan)
}

pub fn plan_place(
    state: &GameState,
    author: PlayerId,
    parcel: ParcelId,
    area: &Area,
    what: Buildable,
) -> Result<Plan, Rejection> {
    let tiles = area.tiles();
    let size = what.size();
    // A building's whole footprint must sit inside the parcel, so check it up
    // front for single placements (the common case) to give a precise reason.
    let footprints: Vec<Pos> =
        if size > 1 { tiles.iter().flat_map(|&a| Rect::square(a, size).iter().collect::<Vec<_>>()).collect() } else { Vec::new() };
    check_parcel(state, author, parcel, &tiles)?;
    if tiles.len() == 1 {
        check_parcel(state, author, parcel, &footprints)?;
    }
    let pc_rect = state.parcels[parcel.index()].rect;
    let mut seen = BTreeSet::new();
    let mut plan = Plan { tiles: Vec::new(), footprints: Vec::new(), cost: 0 };
    for anchor in tiles {
        if let Buildable::Building(b) = what {
            let fp = Rect::square(anchor, size);
            if !pc_rect.contains_rect(&fp) || fp.iter().any(|p| seen.contains(&p)) {
                continue;
            }
            if !fp.iter().all(|p| can_place(state.map.tile(p), what)) {
                continue;
            }
            plan.cost += state.config.building(b).cost;
            for p in fp.iter() {
                seen.insert(p);
                plan.cost += place_cost(state, state.map.tile(p), what);
                plan.tiles.push(p);
            }
            plan.footprints.push(fp);
        } else {
            if !seen.insert(anchor) {
                continue;
            }
            let t = state.map.tile(anchor);
            if can_place(t, what) {
                plan.cost += place_cost(state, t, what);
                plan.tiles.push(anchor);
            }
        }
    }
    afford(state, author, plan)
}

pub fn plan_bulldoze(state: &GameState, author: PlayerId, parcel: ParcelId, area: &Area) -> Result<Plan, Rejection> {
    let tiles = area.tiles();
    check_parcel(state, author, parcel, &tiles)?;
    let mut seen = BTreeSet::new();
    let mut plan = Plan { tiles: Vec::new(), footprints: Vec::new(), cost: 0 };
    for p in tiles {
        // Touching any part of a building takes the whole thing down.
        for q in state.map.footprint_at(p).iter() {
            let t = state.map.tile(q);
            if !seen.insert(q) || t.is_bare() {
                continue;
            }
            plan.cost += state.config.bulldoze_cost;
            if t.terrain == Terrain::Forest {
                plan.cost += state.config.clear_forest_cost;
            }
            plan.tiles.push(q);
        }
    }
    afford(state, author, plan)
}

fn apply_one(state: &mut GameState, cmd: &Command) -> Result<(), Rejection> {
    if cmd.tick != state.tick {
        return Err(Rejection::WrongTick { expected: state.tick });
    }
    if cmd.kind.is_system() != (cmd.author == PlayerId::SYSTEM) {
        return Err(if cmd.kind.is_system() { Rejection::NotSystem } else { Rejection::UnknownPlayer });
    }
    match &cmd.kind {
        CommandKind::Place { parcel, area, what } => {
            let plan = plan_place(state, cmd.author, *parcel, area, *what)?;
            for &p in &plan.tiles {
                let t = state.map.tile_mut(p);
                if t.terrain == Terrain::Forest && *what != Buildable::WaterPipe {
                    t.terrain = Terrain::Land;
                }
                match what {
                    Buildable::PowerLine => t.wire = true,
                    Buildable::WaterPipe => t.pipe = true,
                    Buildable::Zone(z, d) => {
                        // Densifying keeps what's already built; rezoning starts over.
                        if t.kind.zone() != Some(*z) {
                            t.level = 0;
                        }
                        t.kind = crate::map::TileKind::Zone(*z, *d);
                    }
                    other => {
                        t.kind = other.surface().expect("surface buildable");
                        t.level = 0;
                        t.part = 0;
                    }
                }
            }
            for fp in &plan.footprints {
                for (i, p) in fp.iter().enumerate() {
                    state.map.tile_mut(p).part = i as u8;
                }
            }
            state.players[cmd.author.index()].treasury -= plan.cost;
        }
        CommandKind::Bulldoze { parcel, area } => {
            let plan = plan_bulldoze(state, cmd.author, *parcel, area)?;
            for &p in &plan.tiles {
                let t = state.map.tile_mut(p);
                t.kind = TileKind::Empty;
                t.wire = false;
                t.pipe = false;
                t.level = 0;
                t.part = 0;
                t.burning = 0;
                if t.terrain == Terrain::Forest {
                    t.terrain = Terrain::Land;
                }
            }
            state.players[cmd.author.index()].treasury -= plan.cost;
        }
        CommandKind::SetTaxRate { rate } => {
            let max = state.config.max_tax_rate;
            if *rate > max {
                return Err(Rejection::InvalidTaxRate { max });
            }
            state.player_mut(cmd.author).ok_or(Rejection::UnknownPlayer)?.tax_rate = *rate;
        }
        CommandKind::SetUtilityPrice { utility, price } => {
            let p = state.player_mut(cmd.author).ok_or(Rejection::UnknownPlayer)?;
            match utility {
                Utility::Power => p.power_price = *price,
                Utility::Water => p.water_price = *price,
            }
        }
        CommandKind::SetController { player, controller, name } => {
            let p = state.player_mut(*player).ok_or(Rejection::UnknownPlayer)?;
            p.controller = controller.clone();
            p.name = name.clone();
        }
    }
    Ok(())
}

pub fn run(state: &mut GameState, commands: &[Command], report: &mut TickReport) {
    // Deterministic order regardless of arrival order: author, then sequence.
    let mut ordered: Vec<&Command> = commands.iter().collect();
    ordered.sort_by_key(|c| (c.author, c.seq));
    for cmd in ordered {
        match apply_one(state, cmd) {
            Ok(()) => report.applied.push(cmd.clone()),
            Err(why) => report.rejected.push((cmd.clone(), why)),
        }
    }
}
