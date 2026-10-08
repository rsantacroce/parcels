use serde::{Deserialize, Serialize};

use crate::ids::PlayerId;
use crate::map::Rect;
use crate::ids::ParcelId;

/// Who issues commands for a player. This is the only thing that differs between
/// single-player, hot-seat, AI and networked play.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Controller {
    Human,
    Ai(AiStrategy),
    /// Nobody: the parcel sits idle until someone drops in.
    Vacant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiStrategy {
    /// Builds plenty of power and water and sells the surplus.
    UtilityBaron,
    /// Packs in housing and buys utilities from neighbours.
    Developer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Utility {
    Power,
    Water,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtilityLedger {
    pub supply: u32,
    pub demand: u32,
    /// Units consumed from own plants.
    pub own_use: u32,
    pub bought: u32,
    pub sold: u32,
    pub unserved: u32,
}

/// Derived per-player numbers, refreshed every tick. Part of the state (and its
/// hash) so the HUD of every client shows identical values.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerStats {
    pub population: u32,
    pub commercial_jobs: u32,
    pub industrial_jobs: u32,
    /// Per-mille RCI demand, -1000..=1000, as seen by this player.
    pub demand: [i32; 3],
    pub taxes: i64,
    pub upkeep: i64,
    /// Net utility trade cash flow (positive = earned).
    pub trade: i64,
    pub power: UtilityLedger,
    pub water: UtilityLedger,
    pub land_value_total: u64,
    pub developed_tiles: u32,
    pub score: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub controller: Controller,
    /// Cents.
    pub treasury: i64,
    /// Percent.
    pub tax_rate: u8,
    /// Price per unit per tick (cents) offered to neighbours; `None` = not selling.
    pub power_price: Option<u32>,
    pub water_price: Option<u32>,
    pub stats: PlayerStats,
}

impl Player {
    pub fn price(&self, u: Utility) -> Option<u32> {
        match u {
            Utility::Power => self.power_price,
            Utility::Water => self.water_price,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parcel {
    pub id: ParcelId,
    pub owner: PlayerId,
    pub rect: Rect,
}
