//! Balance constants. Loaded from `config/balance.ron` at game creation and then
//! stored inside `GameState`, so every machine simulates with identical numbers.
//!
//! All values are integers. Money is in cents.

use serde::{Deserialize, Serialize};

use crate::map::Buildable;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // ---- World ----
    pub map_width: u16,
    pub map_height: u16,
    pub starting_treasury: i64,
    /// Default tax rate, percent.
    pub default_tax_rate: u8,
    pub max_tax_rate: u8,
    /// Treasury may not fall below this (debt ceiling). Upkeep clamps here.
    pub debt_floor: i64,
    pub ticks_per_month: u32,
    /// Competitive game length in ticks; 0 = endless.
    pub game_length_ticks: u64,
    /// Score = treasury + total owned land value * this factor (cents per point).
    pub score_land_value_factor: i64,

    // ---- Costs (cents) ----
    pub cost: Costs,
    /// Upkeep per tick, in cents.
    pub upkeep: Costs,
    /// Bridges (road on water) multiply the road cost by this.
    pub bridge_cost_multiplier: i64,
    pub bulldoze_cost: i64,

    // ---- Utilities ----
    pub power_plant_capacity: u32,
    pub water_pump_capacity: u32,
    /// Pumps next to river water pump this much extra.
    pub water_pump_river_bonus: u32,
    /// A pump draws this much power and pumps nothing without it.
    pub pump_power_draw: u32,
    /// Default price per utility unit per tick, cents, for new players.
    pub default_utility_price: u32,

    // ---- Zones ----
    pub max_level: u8,
    pub residents_per_level: u16,
    pub commercial_jobs_per_level: u16,
    pub industrial_jobs_per_level: u16,
    /// Manhattan distance a zone may be from a road to count as accessible.
    pub road_access_radius: u8,
    /// Levels above this need water to grow.
    pub water_free_levels: u8,

    // ---- Demand ----
    /// Baseline "outside world" demand, in population units.
    pub base_residential_demand: i32,
    pub base_commercial_demand: i32,
    pub base_industrial_demand: i32,
    /// Workers wanted per 100 jobs.
    pub workers_per_100_jobs: i32,
    /// Residents needed to support one commercial job (x100).
    pub residents_per_100_commercial_jobs: i32,
    /// Residents needed to support one industrial job (x100).
    pub residents_per_100_industrial_jobs: i32,
    /// Demand lost per tax point above `neutral_tax_rate`.
    pub tax_demand_penalty: i32,
    pub neutral_tax_rate: u8,
    /// The region around the map grows: base demand rises this many percent per year.
    pub region_growth_percent_per_year: i32,
    /// Max zone tiles that may change level per zone type per tick, map wide.
    pub max_growth_per_tick: u16,
    /// Random jitter added to growth scores so ties don't always break the same way.
    pub growth_jitter: u16,
    /// Chance in 1000 that an unserved zone decays a level each tick.
    pub decay_chance_per_mille: u16,

    // ---- Land value (0..=1000) ----
    pub land_value_base: u16,
    pub land_value_max: u16,
    pub park_bonus: u16,
    pub park_radius: u8,
    pub river_bonus: u16,
    pub river_radius: u8,
    pub commercial_bonus: u16,
    pub commercial_radius: u8,
    pub road_access_bonus: u16,
    pub powered_bonus: u16,
    pub watered_bonus: u16,
    /// Land value lost per point of pollution.
    pub pollution_weight: u16,
    /// Land value lost per point of traffic.
    pub traffic_weight: u16,
    /// Each tick land value moves 1/N of the way toward its target.
    pub land_value_smoothing: u16,

    // ---- Pollution / traffic ----
    pub industrial_pollution_per_level: u16,
    pub power_plant_pollution: u16,
    pub pollution_radius: u8,
    pub traffic_radius: u8,
    /// Divides nearby population+jobs into a road tile's traffic score.
    pub traffic_divisor: u16,
    pub traffic_pollution_divisor: u16,
    /// Pollution saturates at this value.
    pub pollution_cap: u16,

    // ---- Economy ----
    /// Tax per tick = sum(occupants * land_value) * tax% / this. Industry is
    /// taxed at base land value: factories don't care about the view.
    pub tax_divisor: i64,

    // ---- AI ----
    pub ai_think_interval: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Costs {
    pub road: i64,
    pub power_line: i64,
    pub water_pipe: i64,
    pub power_plant: i64,
    pub water_pump: i64,
    pub park: i64,
    pub residential: i64,
    pub commercial: i64,
    pub industrial: i64,
}

impl Costs {
    pub fn of(&self, b: Buildable) -> i64 {
        match b {
            Buildable::Road => self.road,
            Buildable::PowerLine => self.power_line,
            Buildable::WaterPipe => self.water_pipe,
            Buildable::PowerPlant => self.power_plant,
            Buildable::WaterPump => self.water_pump,
            Buildable::Park => self.park,
            Buildable::Residential => self.residential,
            Buildable::Commercial => self.commercial,
            Buildable::Industrial => self.industrial,
        }
    }
}

impl Default for Costs {
    fn default() -> Self {
        Self {
            road: 10_00,
            power_line: 5_00,
            water_pipe: 5_00,
            power_plant: 3_000_00,
            water_pump: 1_500_00,
            park: 50_00,
            residential: 10_00,
            commercial: 10_00,
            industrial: 10_00,
        }
    }
}

impl Config {
    pub fn default_upkeep() -> Costs {
        Costs {
            road: 1,
            power_line: 1,
            water_pipe: 1,
            power_plant: 1_00,
            water_pump: 50,
            park: 2,
            residential: 0,
            commercial: 0,
            industrial: 0,
        }
    }

    /// Parse a RON config. Missing fields take defaults.
    pub fn from_ron(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }

    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).expect("config serializes")
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            map_width: 64,
            map_height: 48,
            starting_treasury: 20_000_00,
            default_tax_rate: 7,
            max_tax_rate: 20,
            debt_floor: -5_000_00,
            ticks_per_month: 30,
            game_length_ticks: 30 * 12 * 10,
            score_land_value_factor: 20,

            cost: Costs::default(),
            upkeep: Self::default_upkeep(),
            bridge_cost_multiplier: 4,
            bulldoze_cost: 1_00,

            power_plant_capacity: 120,
            water_pump_capacity: 80,
            water_pump_river_bonus: 60,
            pump_power_draw: 6,
            default_utility_price: 4,

            max_level: 4,
            residents_per_level: 12,
            commercial_jobs_per_level: 8,
            industrial_jobs_per_level: 10,
            road_access_radius: 2,
            water_free_levels: 1,

            base_residential_demand: 80,
            base_commercial_demand: 20,
            base_industrial_demand: 80,
            workers_per_100_jobs: 130,
            residents_per_100_commercial_jobs: 300,
            residents_per_100_industrial_jobs: 220,
            tax_demand_penalty: 4,
            neutral_tax_rate: 7,
            region_growth_percent_per_year: 30,
            max_growth_per_tick: 4,
            growth_jitter: 60,
            decay_chance_per_mille: 40,

            land_value_base: 200,
            land_value_max: 1000,
            park_bonus: 140,
            park_radius: 4,
            river_bonus: 120,
            river_radius: 3,
            commercial_bonus: 40,
            commercial_radius: 3,
            road_access_bonus: 60,
            powered_bonus: 60,
            watered_bonus: 60,
            pollution_weight: 1,
            traffic_weight: 1,
            land_value_smoothing: 6,

            industrial_pollution_per_level: 10,
            power_plant_pollution: 50,
            pollution_radius: 5,
            traffic_radius: 3,
            traffic_divisor: 20,
            traffic_pollution_divisor: 3,
            pollution_cap: 300,

            tax_divisor: 1_500,

            ai_think_interval: 8,
        }
    }
}
