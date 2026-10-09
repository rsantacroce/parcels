//! Balance constants. Loaded from `config/balance.ron` at game creation and then
//! stored inside `GameState`, so every machine simulates with identical numbers.
//!
//! All values are integers. Money is in cents.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::catalog::{Building, BuildingStats};
use crate::map::{Buildable, Density, Road, Zone};

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

    // ---- Costs (cents) and upkeep (cents per tick) ----
    pub street_cost: i64,
    pub avenue_cost: i64,
    pub power_line_cost: i64,
    pub water_pipe_cost: i64,
    pub zone_cost: i64,
    pub dense_zone_cost: i64,
    /// Bridges (road on water) multiply the road cost by this.
    pub bridge_cost_multiplier: i64,
    pub bulldoze_cost: i64,
    /// Extra for building on, or bulldozing, woodland.
    pub clear_forest_cost: i64,
    pub street_upkeep: i64,
    pub avenue_upkeep: i64,
    pub power_line_upkeep: i64,
    pub water_pipe_upkeep: i64,
    /// Overrides for building balance. Anything missing uses the built-in default.
    pub buildings: BTreeMap<Building, BuildingStats>,
    /// Default price per utility unit per tick, cents, offered by the AI.
    pub default_utility_price: u32,

    // ---- Zones ----
    pub low_density_max_level: u8,
    pub high_density_max_level: u8,
    pub residents_per_level: u16,
    pub commercial_jobs_per_level: u16,
    pub industrial_jobs_per_level: u16,
    pub office_jobs_per_level: u16,
    /// Manhattan distance a zone may be from a road to count as accessible.
    pub road_access_radius: u8,
    /// Levels above this need water to grow.
    pub water_free_levels: u8,
    /// Housing above this level needs a school or university nearby.
    pub education_free_levels: u8,
    /// Any zone above this level needs a clinic or hospital nearby.
    pub health_free_levels: u8,
    /// Dense zones only grow past the low-density cap on land worth this much.
    pub dense_min_land_value: u16,

    // ---- Demand ----
    /// Baseline "outside world" demand, in population units.
    pub base_residential_demand: i32,
    pub base_commercial_demand: i32,
    pub base_industrial_demand: i32,
    pub base_office_demand: i32,
    /// Workers wanted per 100 jobs.
    pub workers_per_100_jobs: i32,
    /// Residents needed to support one commercial job (x100).
    pub residents_per_100_commercial_jobs: i32,
    /// Residents needed to support one industrial job (x100).
    pub residents_per_100_industrial_jobs: i32,
    /// Residents needed to support one office job (x100).
    pub residents_per_100_office_jobs: i32,
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
    pub river_bonus: u16,
    pub river_radius: u8,
    pub forest_bonus: u16,
    pub forest_radius: u8,
    pub commercial_bonus: u16,
    pub commercial_radius: u8,
    pub road_access_bonus: u16,
    pub powered_bonus: u16,
    pub watered_bonus: u16,
    /// Bonus for each of health and education coverage.
    pub service_bonus: u16,
    /// Land value lost per point of pollution.
    pub pollution_weight: u16,
    /// Land value lost per point of traffic.
    pub traffic_weight: u16,
    /// Land value lost per point of crime.
    pub crime_weight: u16,
    /// Each tick land value moves 1/N of the way toward its target.
    pub land_value_smoothing: u16,

    // ---- Pollution / traffic / crime / fire ----
    pub industrial_pollution_per_level: u16,
    pub pollution_radius: u8,
    pub traffic_radius: u8,
    /// Divides nearby population+jobs into a road tile's traffic score.
    pub traffic_divisor: u16,
    /// Avenues carry traffic at this percentage of a street's congestion.
    pub avenue_traffic_percent: u16,
    pub traffic_pollution_divisor: u16,
    /// Pollution saturates at this value.
    pub pollution_cap: u16,
    pub crime_radius: u8,
    /// Divides nearby occupants into crime.
    pub crime_divisor: u16,
    /// Police coverage removes this percentage of crime.
    pub police_crime_reduction_percent: u16,
    pub crime_cap: u16,
    /// Per tick, per developed (level 2+) zone tile without fire cover. 0 = no fires.
    pub fire_chance_per_million: u32,
    /// Ticks a burnt lot stays rubble before it can regrow.
    pub fire_rubble_ticks: u8,

    // ---- Economy ----
    /// Tax per tick = sum(occupants * land_value) * tax% / this. Industry is
    /// taxed at base land value: factories don't care about the view.
    pub tax_divisor: i64,

    // ---- AI ----
    pub ai_think_interval: u32,
}

impl Config {
    /// Balance numbers for a building: the override from the config file if any,
    /// otherwise the built-in default.
    pub fn building(&self, b: Building) -> BuildingStats {
        self.buildings.get(&b).copied().unwrap_or_else(|| b.default_stats())
    }

    /// Cost per tile (zones, roads, overlays) or per building.
    pub fn cost(&self, b: Buildable) -> i64 {
        match b {
            Buildable::Road(Road::Street) => self.street_cost,
            Buildable::Road(Road::Avenue) => self.avenue_cost,
            Buildable::PowerLine => self.power_line_cost,
            Buildable::WaterPipe => self.water_pipe_cost,
            Buildable::Zone(_, Density::Low) => self.zone_cost,
            Buildable::Zone(_, Density::High) => self.dense_zone_cost,
            Buildable::Building(b) => self.building(b).cost,
        }
    }

    pub fn max_level(&self, d: Density) -> u8 {
        match d {
            Density::Low => self.low_density_max_level,
            Density::High => self.high_density_max_level,
        }
    }

    pub fn per_level(&self, z: Zone) -> u16 {
        match z {
            Zone::Residential => self.residents_per_level,
            Zone::Commercial => self.commercial_jobs_per_level,
            Zone::Industrial => self.industrial_jobs_per_level,
            Zone::Office => self.office_jobs_per_level,
        }
    }

    /// Parse a RON config. Missing fields take defaults.
    pub fn from_ron(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }

    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).expect("config serializes")
    }

    /// Every building's stats spelled out, for writing a complete config file.
    pub fn with_all_buildings(mut self) -> Self {
        for b in Building::ALL {
            let s = self.building(b);
            self.buildings.insert(b, s);
        }
        self
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

            street_cost: 10_00,
            avenue_cost: 25_00,
            power_line_cost: 5_00,
            water_pipe_cost: 5_00,
            zone_cost: 10_00,
            dense_zone_cost: 25_00,
            bridge_cost_multiplier: 4,
            bulldoze_cost: 1_00,
            clear_forest_cost: 3_00,
            street_upkeep: 1,
            avenue_upkeep: 2,
            power_line_upkeep: 1,
            water_pipe_upkeep: 1,
            buildings: BTreeMap::new(),
            default_utility_price: 4,

            low_density_max_level: 3,
            high_density_max_level: 6,
            residents_per_level: 12,
            commercial_jobs_per_level: 8,
            industrial_jobs_per_level: 10,
            office_jobs_per_level: 10,
            road_access_radius: 2,
            water_free_levels: 1,
            education_free_levels: 2,
            health_free_levels: 4,
            dense_min_land_value: 380,

            base_residential_demand: 80,
            base_commercial_demand: 20,
            base_industrial_demand: 80,
            base_office_demand: 10,
            workers_per_100_jobs: 130,
            residents_per_100_commercial_jobs: 300,
            residents_per_100_industrial_jobs: 220,
            residents_per_100_office_jobs: 450,
            tax_demand_penalty: 4,
            neutral_tax_rate: 7,
            region_growth_percent_per_year: 30,
            max_growth_per_tick: 4,
            growth_jitter: 60,
            decay_chance_per_mille: 40,

            land_value_base: 200,
            land_value_max: 1000,
            river_bonus: 120,
            river_radius: 3,
            forest_bonus: 50,
            forest_radius: 2,
            commercial_bonus: 40,
            commercial_radius: 3,
            road_access_bonus: 60,
            powered_bonus: 60,
            watered_bonus: 60,
            service_bonus: 30,
            pollution_weight: 1,
            traffic_weight: 1,
            crime_weight: 1,
            land_value_smoothing: 6,

            industrial_pollution_per_level: 10,
            pollution_radius: 5,
            traffic_radius: 3,
            traffic_divisor: 20,
            avenue_traffic_percent: 45,
            traffic_pollution_divisor: 3,
            pollution_cap: 300,
            crime_radius: 3,
            crime_divisor: 12,
            police_crime_reduction_percent: 80,
            crime_cap: 200,
            fire_chance_per_million: 15,
            fire_rubble_ticks: 30,

            tax_divisor: 1_500,

            ai_think_interval: 8,
        }
    }
}
