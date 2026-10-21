//! Eras: the city moves from a small town to a modern metropolis. Each era
//! unlocks more of the catalogue. An era arrives when the city reaches its
//! population or the calendar reaches its year, whichever comes first, and once
//! reached it is never lost.

use crate::catalog::Building;
use crate::map::{Buildable, Density, Road, Zone};
use crate::state::GameState;

pub const FIRST: u8 = 1;
pub const LAST: u8 = 4;

pub fn name(era: u8) -> &'static str {
    match era {
        0 | 1 => "Town",
        2 => "City",
        3 => "Metropolis",
        _ => "Modern",
    }
}

/// One line on what an era brings, for the announcement.
pub fn blurb(era: u8) -> &'static str {
    match era {
        0 | 1 => "Streets, small zones, coal and wind, and the basic services.",
        2 => "Avenues, offices, cleaner power, water treatment and civic landmarks.",
        3 => "Dense zones, hospitals, universities and the stadium.",
        _ => "Nuclear power and the airport.",
    }
}

impl Buildable {
    /// The era this becomes available in.
    pub fn era(self) -> u8 {
        use Building::*;
        match self {
            Buildable::Road(Road::Street) | Buildable::PowerLine | Buildable::WaterPipe => 1,
            Buildable::Road(Road::Avenue) => 2,
            Buildable::Zone(_, Density::High) => 3,
            Buildable::Zone(Zone::Office, Density::Low) => 2,
            Buildable::Zone(..) => 1,
            Buildable::Building(b) => match b {
                BusStop | BusDepot | CoalPlant | WindTurbine | WaterPump | WaterTower | PoliceStation | FireStation | Clinic
                | School | Park | Plaza | Playground => 1,
                GasPlant | SolarFarm | WaterTreatment | SportsField | TownHall | Monument => 2,
                Hospital | University | Stadium => 3,
                NuclearPlant | Airport => 4,
            },
        }
    }
}

/// Is `what` available to this city yet?
pub fn unlocked(state: &GameState, what: Buildable) -> bool {
    !state.config.eras || what.era() <= state.city.era
}

/// Everything that first becomes available in `era`.
pub fn unlocks(era: u8) -> Vec<Buildable> {
    Buildable::all().into_iter().filter(|b| b.era() == era).collect()
}

/// The era a city with this population, in this year (1-based), has reached.
pub fn reached(state: &GameState) -> u8 {
    let c = &state.config;
    if !c.eras {
        return LAST;
    }
    let (year, _, _) = state.date();
    let pop = state.city.stats.population;
    let mut era = FIRST;
    for i in 0..(LAST - FIRST) as usize {
        if pop >= c.era_population[i] || year > c.era_years[i] as u64 {
            era = FIRST + 1 + i as u8;
        }
    }
    era.max(state.city.era)
}
