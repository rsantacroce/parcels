//! The mayor's office: letters from residents and elections every few years.
//!
//! Residents write in about whatever bothers them most. A letter asks for one
//! concrete thing (a school near their home, lower taxes, cleaner air) and has
//! a deadline. Granting it earns goodwill, which lifts approval for a while;
//! ignoring it costs some. Elections are won on approval alone.

use serde::{Deserialize, Serialize};

use crate::catalog::{Category, Service};
use crate::city::Utility;
use crate::map::{Map, Pos, TileKind, Zone};
use crate::state::GameState;

/// How far a park may be from a letter writer's home to count as "near".
pub const PARK_RADIUS: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ask {
    /// The writer's home covered by this service.
    Service(Service),
    /// A park, plaza, playground or sports field near the writer's home.
    Park,
    /// The tax rate at or below this percent.
    TaxesAtMost(u8),
    /// No homes short of this utility.
    Supply(Utility),
    /// The jobless share at or below this percent.
    JoblessAtMost(u8),
    /// Pollution at the writer's home at or below this.
    CleanAir(u16),
    /// Traffic on the streets beside the writer's home at or below this.
    QuietStreets(u16),
    /// The treasury out of the red.
    OutOfDebt,
}

impl Ask {
    /// Is this about the writer's own street (so it fails if their home goes)?
    pub fn is_local(self) -> bool {
        matches!(self, Ask::Service(_) | Ask::Park | Ask::CleanAir(_) | Ask::QuietStreets(_))
    }

    /// Two asks are the same kind of request (one open letter per kind).
    pub fn same_kind(self, o: Ask) -> bool {
        match (self, o) {
            (Ask::Service(a), Ask::Service(b)) => a == b,
            (Ask::Supply(a), Ask::Supply(b)) => a == b,
            _ => std::mem::discriminant(&self) == std::mem::discriminant(&o),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LetterState {
    Open,
    Granted,
    Missed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Letter {
    pub id: u32,
    pub from: String,
    pub ask: Ask,
    /// Where the writer lives.
    pub at: Pos,
    pub sent: u64,
    /// Tick the request expires.
    pub due: u64,
    pub state: LetterState,
}

impl Letter {
    /// What the resident wrote.
    pub fn text(&self) -> String {
        match self.ask {
            Ask::Service(Service::Education) => "Our kids have no school nearby. Please build one close to us.".into(),
            Ask::Service(Service::Health) => "The nearest clinic is too far away. We need one in our neighbourhood.".into(),
            Ask::Service(Service::Police) => "Break-ins are up on our street. Please send the police.".into(),
            Ask::Service(Service::Fire) => "If a fire starts here, nobody will come. We need a fire station nearby.".into(),
            Ask::Service(Service::Transit) => "We sit in traffic every morning. A bus stop near us would change everything.".into(),
            Ask::Service(Service::Rail) => "A train station near us would get us to work in no time.".into(),
            Ask::Park => "There's nowhere green to walk. Could we have a park near us?".into(),
            Ask::TaxesAtMost(n) => format!("Taxes are squeezing every household. Bring them down to {n}% or less."),
            Ask::Supply(Utility::Power) => "Our lights keep going out. The city needs more power.".into(),
            Ask::Supply(Utility::Water) => "The taps run dry every evening. Please build more water supply.".into(),
            Ask::JoblessAtMost(n) => format!("Too many of us are out of work. Get joblessness down to {n}%."),
            Ask::CleanAir(_) => "The air here smells of smoke. Please clean it up.".into(),
            Ask::QuietStreets(_) => "It takes forever to get anywhere from our street. Fix the traffic.".into(),
            Ask::OutOfDebt => "The city is in debt. Balance the books before it gets worse.".into(),
        }
    }

    /// The goal in a few words, for the inbox list.
    pub fn goal(&self) -> String {
        match self.ask {
            Ask::Service(Service::Transit) => "A working bus stop near their home".into(),
            Ask::Service(s) => format!("{} coverage at their home", s.name()),
            Ask::Park => "A park within 4 tiles".into(),
            Ask::TaxesAtMost(n) => format!("Taxes at {n}% or less"),
            Ask::Supply(Utility::Power) => "No homes without power".into(),
            Ask::Supply(Utility::Water) => "No homes without water".into(),
            Ask::JoblessAtMost(n) => format!("Jobless at {n}% or less"),
            Ask::CleanAir(n) => format!("Pollution at their home {n} or less"),
            Ask::QuietStreets(n) => format!("Traffic beside their home {n} or less"),
            Ask::OutOfDebt => "Treasury above $0".into(),
        }
    }

    /// Is the request fulfilled right now?
    pub fn met(&self, state: &GameState) -> bool {
        let map = &state.map;
        let s = &state.city.stats;
        match self.ask {
            Ask::Service(sv) => map.tile(self.at).covered(sv),
            Ask::Park => near_park(map, self.at),
            Ask::TaxesAtMost(n) => state.city.tax_rate <= n,
            Ask::Supply(Utility::Power) => s.power.unserved == 0,
            Ask::Supply(Utility::Water) => s.water.unserved == 0,
            Ask::JoblessAtMost(n) => s.jobless_percent <= n,
            Ask::CleanAir(n) => map.tile(self.at).pollution <= n,
            Ask::QuietStreets(n) => street_traffic(map, self.at) <= n,
            Ask::OutOfDebt => state.city.treasury >= 0,
        }
    }
}

pub fn near_park(map: &Map, at: Pos) -> bool {
    map.within(at, PARK_RADIUS).any(|(j, _)| matches!(map.tiles[j].kind, TileKind::Building(b) if b.category() == Category::Parks))
}

/// The busiest road next to a tile.
pub fn street_traffic(map: &Map, at: Pos) -> u16 {
    map.neighbors4(map.idx(at)).filter(|&j| map.tiles[j].kind.is_road()).map(|j| map.tiles[j].traffic).max().unwrap_or(0)
}

pub fn is_home(map: &Map, at: Pos) -> bool {
    let t = map.tile(at);
    t.kind.zone() == Some(Zone::Residential) && t.level > 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Election {
    pub tick: u64,
    /// Approval on polling day, whole percent.
    pub approval: u8,
    pub won: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mayor {
    /// Tick of the next election (0 = not scheduled yet; set on the first tick).
    pub next_election: u64,
    pub elections: Vec<Election>,
    /// Extra approval, tenths of a percent, from answered or ignored letters.
    /// Fades month by month.
    pub goodwill: i32,
    /// Open letters plus the most recently answered ones, oldest first.
    pub letters: Vec<Letter>,
    pub next_letter: u32,
    pub letters_granted: u32,
    pub letters_missed: u32,
}

impl Mayor {
    /// Terms won, counting the one the mayor started in.
    pub fn term(&self) -> u32 {
        1 + self.elections.iter().filter(|e| e.won).count() as u32
    }

    pub fn open(&self) -> impl Iterator<Item = &Letter> {
        self.letters.iter().filter(|l| l.state == LetterState::Open)
    }

    pub fn letter(&self, id: u32) -> Option<&Letter> {
        self.letters.iter().find(|l| l.id == id)
    }
}

const FIRST: [&str; 24] = [
    "Ada", "Bea", "Carlos", "Dmitri", "Esi", "Farah", "Gus", "Hana", "Ivo", "June", "Kofi", "Lena", "Mateo", "Nora", "Omar", "Priya",
    "Quinn", "Rosa", "Sven", "Tomás", "Uma", "Wen", "Yusuf", "Zoe",
];
const LAST: [&str; 24] = [
    "Abbott", "Baker", "Costa", "Diaz", "Evans", "Fischer", "Garcia", "Hughes", "Ito", "Jensen", "Kowalski", "Lund", "Moreau",
    "Nakamura", "Okafor", "Patel", "Quist", "Rossi", "Silva", "Tanaka", "Novak", "Weber", "Young", "Zhou",
];

pub fn resident_name(a: u32, b: u32) -> String {
    format!("{} {}", FIRST[a as usize % FIRST.len()], LAST[b as usize % LAST.len()])
}

/// "the north-east", "the centre", … for a tile, relative to the map.
pub fn area_name(map: &Map, at: Pos) -> &'static str {
    let (cx, cy) = (map.width as i32 / 2, map.height as i32 / 2);
    let (dx, dy) = (at.x as i32 - cx, at.y as i32 - cy);
    let (fx, fy) = (map.width as i32 / 6, map.height as i32 / 6);
    let ns = if dy < -fy { 1 } else if dy > fy { 2 } else { 0 };
    let ew = if dx < -fx { 1 } else if dx > fx { 2 } else { 0 };
    match (ns, ew) {
        (0, 0) => "the centre",
        (1, 0) => "the north side",
        (2, 0) => "the south side",
        (0, 1) => "the west side",
        (0, 2) => "the east side",
        (1, 1) => "the north-west",
        (1, 2) => "the north-east",
        (2, 1) => "the south-west",
        _ => "the south-east",
    }
}
