//! Commands are the only way the world changes. Saves, replays and autoplay all
//! reduce to "this list of commands, applied at these ticks".

use serde::{Deserialize, Serialize};

use crate::map::{Buildable, Pos, Rect};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    /// Tick on which the command executes.
    pub tick: u64,
    /// Sequence number; commands in one tick apply in this order.
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
    Place { area: Area, what: Buildable },
    Bulldoze { area: Area },
    SetTaxRate { rate: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rejection {
    WrongTick { expected: u64 },
    OutsideMap(Pos),
    NothingToDo,
    InsufficientFunds { need: i64, have: i64 },
    InvalidTaxRate { max: u8 },
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rejection::WrongTick { expected } => write!(f, "command arrived for the wrong tick (expected {expected})"),
            Rejection::OutsideMap(p) => write!(f, "tile ({}, {}) is off the map", p.x, p.y),
            Rejection::NothingToDo => write!(f, "nothing can be built there"),
            Rejection::InsufficientFunds { need, have } => {
                write!(f, "not enough money: need {}, have {}", crate::fmt_money(*need), crate::fmt_money(*have))
            }
            Rejection::InvalidTaxRate { max } => write!(f, "tax rate must be 0..={max}%"),
        }
    }
}
