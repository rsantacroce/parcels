use serde::{Deserialize, Serialize};

use crate::command::{Command, Rejection};
use crate::config::Config;
use crate::ids::{ParcelId, PlayerId};
use crate::map::{Map, Pos, Rect, TerrainSettings};
use crate::player::{AiStrategy, Controller, Parcel, Player, PlayerStats, Utility};
use crate::systems;

/// A utility sale settled during the utilities stage and paid in the economy stage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub utility: Utility,
    pub seller: PlayerId,
    pub buyer: PlayerId,
    pub units: u32,
    pub price: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalStats {
    pub population: u32,
    pub commercial_jobs: u32,
    pub industrial_jobs: u32,
    pub office_jobs: u32,
    pub public_jobs: u32,
    /// Raw R/C/I/O demand in people/jobs, map wide (before tax adjustments).
    pub raw_demand: [i32; 4],
    /// Normalised per-mille demand, map wide.
    pub demand: [i32; 4],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    /// Players sorted best first, with final scores.
    pub ranking: Vec<(PlayerId, i64)>,
}

/// Everything needed to reproduce the game. A save file and a late-join snapshot
/// are both just this, serialized.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameState {
    /// The next tick to be simulated.
    pub tick: u64,
    pub seed: u64,
    pub config: Config,
    pub map: Map,
    pub parcels: Vec<Parcel>,
    pub players: Vec<Player>,
    pub trades: Vec<Trade>,
    pub global: GlobalStats,
    pub outcome: Option<Outcome>,
}

/// What the caller learns from a tick: useful for UI feedback and sound.
#[derive(Clone, Debug, Default)]
pub struct TickReport {
    pub tick: u64,
    pub applied: Vec<Command>,
    pub rejected: Vec<(Command, Rejection)>,
    /// Tiles whose level went up / down this tick.
    pub grew: u32,
    pub decayed: u32,
    /// Tiles that caught fire this tick.
    pub fires: Vec<Pos>,
    pub month_ended: bool,
    pub game_over: bool,
}

#[derive(Clone, Debug)]
pub struct PlayerSetup {
    pub name: String,
    pub controller: Controller,
}

#[derive(Clone, Debug)]
pub struct NewGame {
    pub seed: u64,
    pub config: Config,
    pub players: Vec<PlayerSetup>,
    pub terrain: TerrainSettings,
}

impl NewGame {
    /// One human versus `ai` computer neighbours.
    pub fn solo(seed: u64, config: Config, human: &str, ai: usize) -> Self {
        let mut players = vec![PlayerSetup { name: human.to_string(), controller: Controller::Human }];
        for i in 0..ai {
            let strategy = if i % 2 == 0 { AiStrategy::UtilityBaron } else { AiStrategy::Developer };
            players.push(PlayerSetup { name: ai_name(i), controller: Controller::Ai(strategy) });
        }
        Self { seed, config, players, terrain: TerrainSettings::default() }
    }
}

pub fn ai_name(i: usize) -> String {
    const NAMES: [&str; 8] = ["Ada", "Bo", "Cyd", "Dee", "Eli", "Fen", "Gus", "Hal"];
    format!("{} (AI)", NAMES[i % NAMES.len()])
}

/// Grid of `cols x rows` parcels for `n` players.
fn parcel_grid(n: usize) -> (u16, u16) {
    match n {
        0 | 1 => (1, 1),
        2 => (2, 1),
        3 => (3, 1),
        4 => (2, 2),
        5 | 6 => (3, 2),
        _ => (4, 2),
    }
}

pub const MAX_PLAYERS: usize = 8;

impl GameState {
    pub fn new(setup: &NewGame) -> Self {
        let config = setup.config.clone();
        let mut map = Map::new(config.map_width, config.map_height);
        map.generate(setup.seed, &setup.terrain);

        let n = setup.players.len().clamp(1, MAX_PLAYERS);
        let (cols, rows) = parcel_grid(n);
        let mut parcels = Vec::new();
        let mut players = Vec::new();
        for row in 0..rows {
            for col in 0..cols {
                let i = parcels.len();
                let x0 = col * map.width / cols;
                let x1 = (col + 1) * map.width / cols - 1;
                let y0 = row * map.height / rows;
                let y1 = (row + 1) * map.height / rows - 1;
                let rect = Rect::from_corners(Pos::new(x0, y0), Pos::new(x1, y1));
                let pid = PlayerId(i as u8);
                parcels.push(Parcel { id: ParcelId(i as u8), owner: pid, rect });
                for p in rect.iter() {
                    map.tile_mut(p).parcel = ParcelId(i as u8);
                }
                let (name, controller) = match setup.players.get(i) {
                    Some(s) => (s.name.clone(), s.controller.clone()),
                    None => (format!("Vacant lot {}", i + 1), Controller::Vacant),
                };
                players.push(Player {
                    id: pid,
                    name,
                    controller,
                    treasury: config.starting_treasury,
                    tax_rate: config.default_tax_rate,
                    power_price: None,
                    water_price: None,
                    stats: PlayerStats::default(),
                });
            }
        }

        for t in &mut map.tiles {
            t.land_value = config.land_value_base;
        }

        let mut state = Self {
            tick: 0,
            seed: setup.seed,
            config,
            map,
            parcels,
            players,
            trades: Vec::new(),
            global: GlobalStats::default(),
            outcome: None,
        };
        // Settle derived values so the very first frame shows sensible numbers.
        systems::land_value::run(&mut state);
        systems::economy::refresh_scores(&mut state);
        state
    }

    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.players.get(id.index())
    }

    pub fn player_mut(&mut self, id: PlayerId) -> Option<&mut Player> {
        self.players.get_mut(id.index())
    }

    pub fn parcel(&self, id: ParcelId) -> Option<&Parcel> {
        self.parcels.get(id.index())
    }

    /// Owner of the tile at `p`, if any.
    pub fn owner_at(&self, p: Pos) -> Option<PlayerId> {
        let t = self.map.get(p)?;
        self.parcel(t.parcel).map(|pc| pc.owner)
    }

    pub fn owner_of_idx(&self, idx: usize) -> Option<PlayerId> {
        self.parcel(self.map.tiles[idx].parcel).map(|pc| pc.owner)
    }

    pub fn parcels_of(&self, player: PlayerId) -> impl Iterator<Item = &Parcel> {
        self.parcels.iter().filter(move |p| p.owner == player)
    }

    /// Population / jobs housed by a zone tile.
    pub fn occupants(&self, idx: usize) -> u32 {
        let t = &self.map.tiles[idx];
        t.kind.zone().map_or(0, |z| t.level as u32 * self.config.per_level(z) as u32)
    }

    pub fn month(&self) -> u64 {
        self.tick / self.config.ticks_per_month as u64
    }

    /// (year starting at 1, month 1..=12, day 1..=ticks_per_month)
    pub fn date(&self) -> (u64, u64, u64) {
        let tpm = self.config.ticks_per_month as u64;
        let m = self.tick / tpm;
        (m / 12 + 1, m % 12 + 1, self.tick % tpm + 1)
    }

    pub fn is_over(&self) -> bool {
        self.outcome.is_some()
    }

    /// Advance one tick, applying `commands` (which must all be stamped with this
    /// tick). The pipeline order is fixed: commands, utilities, demand, growth,
    /// land value, economy.
    pub fn step(&mut self, commands: &[Command]) -> TickReport {
        let mut report = TickReport { tick: self.tick, ..Default::default() };
        if self.is_over() {
            for c in commands {
                report.rejected.push((c.clone(), Rejection::GameOver));
            }
            return report;
        }

        systems::apply::run(self, commands, &mut report);
        systems::utilities::run(self);
        systems::demand::run(self);
        systems::growth::run(self, &mut report);
        systems::land_value::run(self);
        systems::economy::run(self);

        self.tick += 1;
        report.month_ended = self.tick % self.config.ticks_per_month as u64 == 0;

        if self.config.game_length_ticks > 0 && self.tick >= self.config.game_length_ticks {
            let mut ranking: Vec<(PlayerId, i64)> = self
                .players
                .iter()
                .filter(|p| p.controller != Controller::Vacant)
                .map(|p| (p.id, p.stats.score))
                .collect();
            ranking.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            self.outcome = Some(Outcome { ranking });
            report.game_over = true;
        }
        report
    }

    /// Stable 64-bit hash of the complete state, for desync detection.
    pub fn hash(&self) -> u64 {
        let bytes = self.to_bytes();
        crate::hash::fnv1a64(&bytes)
    }

    /// Compact binary form (network snapshots).
    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("game state serializes")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(bytes)
    }

    /// Human-readable form (save files).
    pub fn to_ron(&self) -> String {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default().compact_arrays(true))
            .expect("game state serializes")
    }

    pub fn from_ron(text: &str) -> Result<Self, ron::error::SpannedError> {
        ron::from_str(text)
    }
}
