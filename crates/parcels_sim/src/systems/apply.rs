//! Stage 1: apply this tick's commands in a deterministic order.

use crate::command::{Area, Command, CommandKind, Rejection};
use crate::ids::{ParcelId, PlayerId};
use crate::map::{Buildable, Pos, Terrain, Tile, TileKind};
use crate::player::Utility;
use crate::state::{GameState, TickReport};

/// The tiles a placement would actually change, and what it would cost.
/// Also used by the client for the ghost preview, so preview and reality agree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub tiles: Vec<Pos>,
    pub cost: i64,
}

fn can_place(tile: &Tile, what: Buildable) -> bool {
    match what {
        Buildable::Road => tile.kind == TileKind::Empty,
        Buildable::PowerLine => !tile.wire && matches!(tile.kind, TileKind::Empty | TileKind::Road),
        Buildable::WaterPipe => !tile.pipe && tile.terrain == Terrain::Land,
        _ => tile.kind == TileKind::Empty && tile.terrain == Terrain::Land && !tile.wire,
    }
}

fn place_cost(state: &GameState, tile: &Tile, what: Buildable) -> i64 {
    let base = state.config.cost.of(what);
    if what == Buildable::Road && tile.terrain == Terrain::Water {
        base * state.config.bridge_cost_multiplier
    } else {
        base
    }
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

pub fn plan_place(
    state: &GameState,
    author: PlayerId,
    parcel: ParcelId,
    area: &Area,
    what: Buildable,
) -> Result<Plan, Rejection> {
    let tiles = area.tiles();
    check_parcel(state, author, parcel, &tiles)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut plan = Plan { tiles: Vec::new(), cost: 0 };
    for p in tiles {
        if !seen.insert(p) {
            continue;
        }
        let t = state.map.tile(p);
        if can_place(t, what) {
            plan.cost += place_cost(state, t, what);
            plan.tiles.push(p);
        }
    }
    if plan.tiles.is_empty() {
        return Err(Rejection::NothingToDo);
    }
    let have = state.players[author.index()].treasury;
    if plan.cost > have {
        return Err(Rejection::InsufficientFunds { need: plan.cost, have });
    }
    Ok(plan)
}

pub fn plan_bulldoze(state: &GameState, author: PlayerId, parcel: ParcelId, area: &Area) -> Result<Plan, Rejection> {
    let tiles = area.tiles();
    check_parcel(state, author, parcel, &tiles)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut plan = Plan { tiles: Vec::new(), cost: 0 };
    for p in tiles {
        if seen.insert(p) && !state.map.tile(p).is_bare() {
            plan.cost += state.config.bulldoze_cost;
            plan.tiles.push(p);
        }
    }
    if plan.tiles.is_empty() {
        return Err(Rejection::NothingToDo);
    }
    let have = state.players[author.index()].treasury;
    if plan.cost > have {
        return Err(Rejection::InsufficientFunds { need: plan.cost, have });
    }
    Ok(plan)
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
                match what {
                    Buildable::PowerLine => t.wire = true,
                    Buildable::WaterPipe => t.pipe = true,
                    other => {
                        t.kind = other.surface().expect("surface buildable");
                        t.level = 0;
                    }
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
