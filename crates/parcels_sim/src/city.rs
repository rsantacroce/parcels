use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Utility {
    Power,
    Water,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtilityLedger {
    pub supply: u32,
    pub demand: u32,
    pub unserved: u32,
}

/// Derived numbers, refreshed every tick. Part of the state (and its hash) so a
/// replay reproduces exactly what the HUD showed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CityStats {
    pub population: u32,
    pub commercial_jobs: u32,
    pub industrial_jobs: u32,
    pub office_jobs: u32,
    /// Jobs in powered plants, services and landmarks.
    pub public_jobs: u32,
    /// Raw R/C/I/O demand in people/jobs (before tax adjustments).
    pub raw_demand: [i32; 4],
    /// Per-mille R/C/I/O demand after taxes, -1000..=1000.
    pub demand: [i32; 4],
    pub taxes: i64,
    pub upkeep: i64,
    pub power: UtilityLedger,
    pub water: UtilityLedger,
    pub land_value_total: u64,
    pub developed_tiles: u32,
    /// Buildings lost to fire since the city was founded.
    pub fires: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct City {
    pub name: String,
    /// Cents.
    pub treasury: i64,
    /// Percent.
    pub tax_rate: u8,
    pub stats: CityStats,
}
