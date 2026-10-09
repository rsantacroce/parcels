//! Everything a player can place that isn't a road, wire, pipe or zone: plants,
//! services, parks and landmarks. Identity, footprint and category live in code;
//! the balance numbers (`BuildingStats`) live in `Config` so they can be tuned
//! from `config/balance.ron` and travel inside every save.

use serde::{Deserialize, Serialize};

/// Toolbar grouping. Also used by the AI to reason about what it has built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Category {
    Transport,
    Zones,
    Power,
    Water,
    Services,
    Parks,
    Landmarks,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Transport,
        Category::Zones,
        Category::Power,
        Category::Water,
        Category::Services,
        Category::Parks,
        Category::Landmarks,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::Transport => "Transport",
            Category::Zones => "Zones",
            Category::Power => "Power",
            Category::Water => "Water",
            Category::Services => "Services",
            Category::Parks => "Parks",
            Category::Landmarks => "Landmarks",
        }
    }
}

/// City services. A powered service building covers every tile within its
/// radius, on either side of any parcel border.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Service {
    /// Cuts crime.
    Police = 0,
    /// Stops fires.
    Fire = 1,
    /// Needed for the tallest buildings; raises land value.
    Health = 2,
    /// Needed for mid-rise housing and offices; raises land value.
    Education = 3,
}

impl Service {
    pub const ALL: [Service; 4] = [Service::Police, Service::Fire, Service::Health, Service::Education];

    pub fn bit(self) -> u8 {
        1 << self as u8
    }

    pub fn name(self) -> &'static str {
        match self {
            Service::Police => "Police",
            Service::Fire => "Fire",
            Service::Health => "Health",
            Service::Education => "Education",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Building {
    // Power
    CoalPlant,
    GasPlant,
    WindTurbine,
    SolarFarm,
    NuclearPlant,
    // Water
    WaterPump,
    WaterTower,
    WaterTreatment,
    // Services
    PoliceStation,
    FireStation,
    Clinic,
    Hospital,
    School,
    University,
    // Parks
    Park,
    Plaza,
    Playground,
    SportsField,
    Stadium,
    // Landmarks
    TownHall,
    Monument,
    Airport,
}

impl Building {
    pub const ALL: [Building; 22] = [
        Building::CoalPlant,
        Building::GasPlant,
        Building::WindTurbine,
        Building::SolarFarm,
        Building::NuclearPlant,
        Building::WaterPump,
        Building::WaterTower,
        Building::WaterTreatment,
        Building::PoliceStation,
        Building::FireStation,
        Building::Clinic,
        Building::Hospital,
        Building::School,
        Building::University,
        Building::Park,
        Building::Plaza,
        Building::Playground,
        Building::SportsField,
        Building::Stadium,
        Building::TownHall,
        Building::Monument,
        Building::Airport,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Building::CoalPlant => "Coal plant",
            Building::GasPlant => "Gas plant",
            Building::WindTurbine => "Wind turbine",
            Building::SolarFarm => "Solar farm",
            Building::NuclearPlant => "Nuclear plant",
            Building::WaterPump => "Water pump",
            Building::WaterTower => "Water tower",
            Building::WaterTreatment => "Water treatment",
            Building::PoliceStation => "Police station",
            Building::FireStation => "Fire station",
            Building::Clinic => "Clinic",
            Building::Hospital => "Hospital",
            Building::School => "School",
            Building::University => "University",
            Building::Park => "Park",
            Building::Plaza => "Plaza",
            Building::Playground => "Playground",
            Building::SportsField => "Sports field",
            Building::Stadium => "Stadium",
            Building::TownHall => "Town hall",
            Building::Monument => "Monument",
            Building::Airport => "Airport",
        }
    }

    pub fn category(self) -> Category {
        use Building::*;
        match self {
            CoalPlant | GasPlant | WindTurbine | SolarFarm | NuclearPlant => Category::Power,
            WaterPump | WaterTower | WaterTreatment => Category::Water,
            PoliceStation | FireStation | Clinic | Hospital | School | University => Category::Services,
            Park | Plaza | Playground | SportsField | Stadium => Category::Parks,
            TownHall | Monument | Airport => Category::Landmarks,
        }
    }

    /// Square footprint edge, in tiles. The anchor is the footprint's min corner.
    pub fn size(self) -> u16 {
        use Building::*;
        match self {
            WindTurbine | WaterPump | WaterTower | PoliceStation | FireStation | Clinic | Park | Plaza | Playground
            | Monument => 1,
            CoalPlant | GasPlant | SolarFarm | WaterTreatment | Hospital | School | SportsField | TownHall => 2,
            NuclearPlant | University | Stadium => 3,
            Airport => 4,
        }
    }

    pub fn service(self) -> Option<Service> {
        match self {
            Building::PoliceStation => Some(Service::Police),
            Building::FireStation => Some(Service::Fire),
            Building::Clinic | Building::Hospital => Some(Service::Health),
            Building::School | Building::University => Some(Service::Education),
            _ => None,
        }
    }

    /// Joins power and water networks (like a zone does). Open spaces don't.
    pub fn conducts(self) -> bool {
        !matches!(self, Building::Park | Building::Plaza | Building::Playground | Building::Monument)
    }

    /// One-line description for tooltips.
    pub fn blurb(self) -> &'static str {
        use Building::*;
        match self {
            CoalPlant => "Cheap, plentiful power. Very dirty.",
            GasPlant => "Cleaner than coal, pricier to build and run.",
            WindTurbine => "A little clean power on a single tile.",
            SolarFarm => "Clean power, modest output.",
            NuclearPlant => "Enormous clean output at an enormous price.",
            WaterPump => "Pumps water; much more beside the river. Needs power.",
            WaterTower => "Small water supply anywhere. Needs power.",
            WaterTreatment => "Large water supply. Needs lots of power.",
            PoliceStation => "Cuts crime nearby.",
            FireStation => "Protects nearby buildings from fire.",
            Clinic => "Health coverage for a small area.",
            Hospital => "Wide health coverage. Lets towers grow tall.",
            School => "Education: lets housing past level 2 and offices grow.",
            University => "Wide education coverage and a land value boost.",
            Park => "Trees and paths. Lifts land value nearby.",
            Plaza => "Paved square. Lifts land value, good beside shops.",
            Playground => "Families love it.",
            SportsField => "Big land value boost over a wide area.",
            Stadium => "A landmark that lifts the whole district.",
            TownHall => "Civic pride: attracts residents to the region.",
            Monument => "A statue people travel to see.",
            Airport => "Brings business and industry to the region. Noisy.",
        }
    }

    pub fn default_stats(self) -> BuildingStats {
        use Building::*;
        let s = BuildingStats::default();
        match self {
            CoalPlant => BuildingStats { cost: 3_000_00, upkeep: 1_00, power_supply: 240, pollution: 60, jobs: 20, ..s },
            GasPlant => BuildingStats { cost: 5_500_00, upkeep: 1_40, power_supply: 220, pollution: 25, jobs: 15, ..s },
            WindTurbine => BuildingStats { cost: 600_00, upkeep: 15, power_supply: 18, ..s },
            SolarFarm => BuildingStats { cost: 4_000_00, upkeep: 30, power_supply: 90, jobs: 4, ..s },
            NuclearPlant => BuildingStats { cost: 25_000_00, upkeep: 4_00, power_supply: 1200, pollution: 10, jobs: 80, ..s },
            WaterPump => BuildingStats { cost: 1_500_00, upkeep: 50, water_supply: 80, river_bonus: 60, power_draw: 6, ..s },
            WaterTower => BuildingStats { cost: 900_00, upkeep: 25, water_supply: 40, power_draw: 2, ..s },
            WaterTreatment => {
                BuildingStats { cost: 6_000_00, upkeep: 1_50, water_supply: 320, river_bonus: 120, power_draw: 20, jobs: 25, ..s }
            }
            PoliceStation => BuildingStats { cost: 2_500_00, upkeep: 80, power_draw: 4, service_radius: 10, jobs: 15, ..s },
            FireStation => BuildingStats { cost: 2_500_00, upkeep: 80, power_draw: 4, service_radius: 10, jobs: 15, ..s },
            Clinic => BuildingStats {
                cost: 2_000_00,
                upkeep: 60,
                power_draw: 4,
                service_radius: 7,
                jobs: 10,
                land_value_bonus: 20,
                land_value_radius: 3,
                ..s
            },
            Hospital => BuildingStats {
                cost: 8_000_00,
                upkeep: 2_00,
                power_draw: 12,
                service_radius: 16,
                jobs: 60,
                land_value_bonus: 40,
                land_value_radius: 5,
                ..s
            },
            School => BuildingStats { cost: 4_000_00, upkeep: 1_00, power_draw: 8, service_radius: 10, jobs: 30, ..s },
            University => BuildingStats {
                cost: 15_000_00,
                upkeep: 3_00,
                power_draw: 20,
                service_radius: 20,
                jobs: 120,
                land_value_bonus: 120,
                land_value_radius: 8,
                ..s
            },
            Park => BuildingStats { cost: 50_00, upkeep: 2, land_value_bonus: 140, land_value_radius: 4, ..s },
            Plaza => BuildingStats { cost: 80_00, upkeep: 3, land_value_bonus: 90, land_value_radius: 3, ..s },
            Playground => BuildingStats { cost: 120_00, upkeep: 4, land_value_bonus: 110, land_value_radius: 3, ..s },
            SportsField => {
                BuildingStats { cost: 1_500_00, upkeep: 30, power_draw: 2, land_value_bonus: 170, land_value_radius: 6, ..s }
            }
            Stadium => BuildingStats {
                cost: 20_000_00,
                upkeep: 3_00,
                power_draw: 30,
                land_value_bonus: 260,
                land_value_radius: 10,
                jobs: 50,
                demand_bonus: [0, 40, 0, 0],
                ..s
            },
            TownHall => BuildingStats {
                cost: 10_000_00,
                upkeep: 1_00,
                power_draw: 10,
                land_value_bonus: 120,
                land_value_radius: 12,
                jobs: 40,
                demand_bonus: [80, 0, 0, 20],
                ..s
            },
            Monument => BuildingStats { cost: 6_000_00, upkeep: 20, land_value_bonus: 150, land_value_radius: 6, ..s },
            Airport => BuildingStats {
                cost: 40_000_00,
                upkeep: 5_00,
                power_draw: 40,
                pollution: 40,
                jobs: 150,
                demand_bonus: [0, 80, 80, 80],
                ..s
            },
        }
    }
}

/// Balance numbers for one building type. Integers only; money in cents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildingStats {
    pub cost: i64,
    /// Per tick.
    pub upkeep: i64,
    pub power_supply: u32,
    pub water_supply: u32,
    /// Extra water when any footprint tile touches the river.
    pub river_bonus: u32,
    /// Power needed to run at all (water producers and services).
    pub power_draw: u32,
    pub pollution: u16,
    /// Manhattan radius of service coverage (services only).
    pub service_radius: u8,
    pub land_value_bonus: u16,
    pub land_value_radius: u8,
    /// Public-sector jobs, which count toward residential demand.
    pub jobs: u16,
    /// Added to the region's raw R/C/I/O demand while the building is powered
    /// (or always, if it draws nothing).
    pub demand_bonus: [i32; 4],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_building_has_a_category_and_sane_stats() {
        for b in Building::ALL {
            let s = b.default_stats();
            assert!(s.cost > 0, "{b:?}");
            assert!((1..=4).contains(&b.size()), "{b:?}");
            assert_eq!(b.service().is_some(), s.service_radius > 0, "{b:?}");
            assert!(Category::ALL.contains(&b.category()));
        }
    }
}
