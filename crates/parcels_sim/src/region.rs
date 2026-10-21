//! Neighbouring cities in a region. Each city is its own map; the four map
//! edges face the plots next to it. What a city knows about a neighbour is a
//! snapshot taken when that neighbour was last played, delivered by the
//! `SetNeighbours` command so replays see exactly what the player saw.
//!
//! Neighbours only matter where something crosses the edge: a road lets people
//! commute and shop across the border, a power line or water pipe lets the two
//! cities trade spare supply.

use serde::{Deserialize, Serialize};

use crate::map::{Map, Pos, Zone};
use crate::state::GameState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Side {
    North = 0,
    East = 1,
    South = 2,
    West = 3,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::North, Side::East, Side::South, Side::West];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Side::North => "north",
            Side::East => "east",
            Side::South => "south",
            Side::West => "west",
        }
    }

    /// Plot offset of the neighbour on this side (y grows southward).
    pub fn offset(self) -> (i32, i32) {
        match self {
            Side::North => (0, -1),
            Side::East => (1, 0),
            Side::South => (0, 1),
            Side::West => (-1, 0),
        }
    }

    pub fn opposite(self) -> Side {
        Side::ALL[(self.index() + 2) % 4]
    }

    /// The tiles along this edge of the map, in row-major order.
    pub fn edge(self, map: &Map) -> Vec<Pos> {
        let (w, h) = (map.width, map.height);
        match self {
            Side::North => (0..w).map(|x| Pos::new(x, 0)).collect(),
            Side::South => (0..w).map(|x| Pos::new(x, h - 1)).collect(),
            Side::West => (0..h).map(|y| Pos::new(0, y)).collect(),
            Side::East => (0..h).map(|y| Pos::new(w - 1, y)).collect(),
        }
    }

    /// Which edges a tile lies on.
    pub fn of(map: &Map, p: Pos) -> impl Iterator<Item = Side> {
        let (w, h) = (map.width, map.height);
        [(Side::North, p.y == 0), (Side::East, p.x + 1 == w), (Side::South, p.y + 1 == h), (Side::West, p.x == 0)]
            .into_iter()
            .filter_map(|(s, on)| on.then_some(s))
    }
}

/// A neighbouring city as last seen.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Neighbour {
    pub name: String,
    pub population: u32,
    /// Workers its jobs could still take.
    pub open_jobs: u32,
    /// Its residents looking for work.
    pub job_seekers: u32,
    pub power_spare: u32,
    pub power_short: u32,
    pub water_spare: u32,
    pub water_short: u32,
}

/// What crosses one edge of the map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Links {
    pub road: bool,
    pub power: bool,
    pub water: bool,
}

impl Links {
    pub fn any(self) -> bool {
        self.road || self.power || self.water
    }
}

/// What crosses each edge that has a neighbour beyond it.
pub fn links(state: &GameState) -> [Links; 4] {
    let mut out = [Links::default(); 4];
    for side in Side::ALL {
        if state.neighbours[side.index()].is_none() {
            continue;
        }
        let l = &mut out[side.index()];
        for p in side.edge(&state.map) {
            let t = state.map.tile(p);
            l.road |= t.kind.is_road();
            l.power |= t.wire;
            l.water |= t.pipe;
        }
    }
    out
}

/// This city as its neighbours will see it.
pub fn summary(state: &GameState) -> Neighbour {
    let s = &state.city.stats;
    let c = &state.config;
    let jobs = s.commercial_jobs + s.industrial_jobs + s.office_jobs + s.public_jobs;
    let workers_wanted = jobs as u64 * c.workers_per_100_jobs.max(0) as u64 / 100;
    let local = |l: &crate::city::UtilityLedger| l.supply.saturating_sub(l.imported);
    Neighbour {
        name: state.city.name.clone(),
        population: s.population,
        open_jobs: workers_wanted.saturating_sub(s.population as u64).min(u32::MAX as u64) as u32,
        job_seekers: s.population * s.jobless_percent as u32 / 100,
        power_spare: local(&s.power).saturating_sub(s.power.demand + s.power.exported),
        power_short: s.power.unserved,
        water_spare: local(&s.water).saturating_sub(s.water.demand + s.water.exported),
        water_short: s.water.unserved,
    }
}

/// Raw R/C/I/O demand from neighbours reachable by road.
pub fn demand(state: &GameState) -> [i32; Zone::COUNT] {
    let c = &state.config;
    let pct = c.region_demand_percent;
    let mut d = [0i32; Zone::COUNT];
    for (side, l) in Side::ALL.into_iter().zip(links(state)) {
        let Some(n) = &state.neighbours[side.index()] else { continue };
        if !l.road {
            continue;
        }
        // Homes here for their jobs, shops here for their shoppers, factories
        // here for their job seekers.
        d[Zone::Residential.index()] += n.open_jobs as i32 * pct / 100;
        d[Zone::Commercial.index()] += n.population as i32 * pct / c.residents_per_100_commercial_jobs.max(1);
        d[Zone::Industrial.index()] += n.job_seekers as i32 * pct / 100;
    }
    d
}
