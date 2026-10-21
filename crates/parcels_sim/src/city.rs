use serde::{Deserialize, Serialize};

use crate::mayor::Mayor;

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
#[serde(default)]
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
    /// Residents travelling to work each day, and how many of them by bus.
    pub commuters: u32,
    pub bus_riders: u32,
    /// Residents whose homes have no road route to any job.
    pub stranded: u32,
    /// Average trip to work, minutes.
    pub commute_minutes: u16,
    /// Bus fares per tick, cents.
    pub fares: i64,
    /// Residents without a job, percent.
    pub jobless_percent: u8,
    /// What residents think of each part of city life, roughly -20..=+10 each.
    pub mood: [i32; Factor::COUNT],
    /// Mayor approval in tenths of a percent, 0..=1000. Drifts toward
    /// `50% + sum(mood)` so one bad month doesn't swing it wildly.
    pub approval: u16,
}

impl CityStats {
    /// Approval, whole percent.
    pub fn approval_percent(&self) -> u8 {
        (self.approval / 10) as u8
    }

    /// The factor dragging approval down the most, if any is negative.
    pub fn top_issue(&self) -> Option<Factor> {
        Factor::ALL.into_iter().filter(|f| self.mood[f.index()] < 0).min_by_key(|f| self.mood[f.index()])
    }
}

/// The parts of city life residents have an opinion on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Factor {
    Jobs,
    Utilities,
    Safety,
    Environment,
    Services,
    Taxes,
    Traffic,
    Neighbourhoods,
    Budget,
}

impl Factor {
    pub const COUNT: usize = 9;
    pub const ALL: [Factor; Factor::COUNT] = [
        Factor::Jobs,
        Factor::Utilities,
        Factor::Safety,
        Factor::Environment,
        Factor::Services,
        Factor::Taxes,
        Factor::Traffic,
        Factor::Neighbourhoods,
        Factor::Budget,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Factor::Jobs => "Jobs",
            Factor::Utilities => "Power & water",
            Factor::Safety => "Safety",
            Factor::Environment => "Clean air",
            Factor::Services => "Schools & health",
            Factor::Taxes => "Taxes",
            Factor::Traffic => "Traffic",
            Factor::Neighbourhoods => "Neighbourhoods",
            Factor::Budget => "City finances",
        }
    }

    /// What residents say when this factor is the top issue.
    pub fn complaint(self) -> &'static str {
        match self {
            Factor::Jobs => "Not enough jobs for everyone",
            Factor::Utilities => "Homes are going without power or water",
            Factor::Safety => "Crime is up; more police needed",
            Factor::Environment => "Pollution near homes",
            Factor::Services => "Too few schools and clinics",
            Factor::Taxes => "Taxes are too high",
            Factor::Traffic => "The streets are jammed",
            Factor::Neighbourhoods => "Neighbourhoods feel run-down; add parks",
            Factor::Budget => "The city is in debt",
        }
    }
}

/// A word for an approval rating, whole percent.
pub fn mood_word(percent: u8) -> &'static str {
    match percent {
        80.. => "Thriving",
        65..80 => "Happy",
        50..65 => "Content",
        35..50 => "Uneasy",
        20..35 => "Unhappy",
        _ => "Angry",
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct City {
    pub name: String,
    /// Cents.
    pub treasury: i64,
    /// Percent.
    pub tax_rate: u8,
    pub stats: CityStats,
    #[serde(default)]
    pub mayor: Mayor,
}
