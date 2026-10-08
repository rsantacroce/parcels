//! Commands are the only way the world changes. Saves, replays, AI and networking
//! all reduce to "this list of commands, applied at these ticks".

use serde::{Deserialize, Serialize};

use crate::ids::{ParcelId, PlayerId};
use crate::map::{Buildable, Pos, Rect};
use crate::player::{Controller, Utility};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    /// Tick on which the command executes.
    pub tick: u64,
    pub author: PlayerId,
    /// Per-author sequence number; breaks ordering ties deterministically.
    pub seq: u32,
    pub kind: CommandKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Area {
    Rect(Rect),
    /// Arbitrary tiles, e.g. an L-shaped road drag.
    Tiles(Vec<Pos>),
}

impl Area {
    pub fn tiles(&self) -> Vec<Pos> {
        match self {
            Area::Rect(r) => r.iter().collect(),
            Area::Tiles(t) => t.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandKind {
    Place { parcel: ParcelId, area: Area, what: Buildable },
    Bulldoze { parcel: ParcelId, area: Area },
    SetTaxRate { rate: u8 },
    /// Offer surplus to neighbours at `price` cents/unit/tick, or stop selling.
    SetUtilityPrice { utility: Utility, price: Option<u32> },
    /// System only: change who drives a player (join, leave, AI takeover).
    SetController { player: PlayerId, controller: Controller, name: String },
}

impl CommandKind {
    pub fn is_system(&self) -> bool {
        matches!(self, CommandKind::SetController { .. })
    }

    pub fn parcel(&self) -> Option<ParcelId> {
        match self {
            CommandKind::Place { parcel, .. } | CommandKind::Bulldoze { parcel, .. } => Some(*parcel),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rejection {
    WrongTick { expected: u64 },
    UnknownPlayer,
    UnknownParcel,
    NotYourParcel,
    OutsideParcel(Pos),
    NothingToDo,
    InsufficientFunds { need: i64, have: i64 },
    InvalidTaxRate { max: u8 },
    NotSystem,
    GameOver,
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rejection::WrongTick { expected } => write!(f, "command arrived for the wrong tick (expected {expected})"),
            Rejection::UnknownPlayer => write!(f, "unknown player"),
            Rejection::UnknownParcel => write!(f, "unknown parcel"),
            Rejection::NotYourParcel => write!(f, "you don't own that parcel"),
            Rejection::OutsideParcel(p) => write!(f, "tile ({}, {}) is outside your parcel", p.x, p.y),
            Rejection::NothingToDo => write!(f, "nothing can be built there"),
            Rejection::InsufficientFunds { need, have } => {
                write!(f, "not enough money: need {}, have {}", crate::fmt_money(*need), crate::fmt_money(*have))
            }
            Rejection::InvalidTaxRate { max } => write!(f, "tax rate must be 0..={max}%"),
            Rejection::NotSystem => write!(f, "only the host may do that"),
            Rejection::GameOver => write!(f, "the game is over"),
        }
    }
}
