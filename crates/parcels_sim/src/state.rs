use serde::{Deserialize, Serialize};

use crate::city::{City, CityStats};
use crate::command::Command;
use crate::config::Config;
use crate::map::{Map, Pos, TerrainSettings};
use crate::systems;

/// Everything needed to reproduce the game. A save file is just this, serialized.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameState {
    /// The next tick to be simulated.
    pub tick: u64,
    pub seed: u64,
    pub config: Config,
    pub map: Map,
    pub city: City,
}

/// What the caller learns from a tick: useful for UI feedback and sound.
#[derive(Clone, Debug, Default)]
pub struct TickReport {
    pub tick: u64,
    pub applied: Vec<Command>,
    pub rejected: Vec<(Command, crate::command::Rejection)>,
    /// Tiles whose level went up / down this tick.
    pub grew: u32,
    pub decayed: u32,
    /// Tiles that caught fire this tick.
    pub fires: Vec<Pos>,
    pub month_ended: bool,
}

#[derive(Clone, Debug)]
pub struct NewGame {
    pub seed: u64,
    pub config: Config,
    pub city_name: String,
    pub terrain: TerrainSettings,
}

impl NewGame {
    pub fn new(seed: u64, config: Config, city_name: &str) -> Self {
        Self { seed, config, city_name: city_name.to_string(), terrain: TerrainSettings::default() }
    }
}

impl GameState {
    pub fn new(setup: &NewGame) -> Self {
        let config = setup.config.clone();
        let mut map = Map::new(config.map_width, config.map_height);
        map.generate(setup.seed, &setup.terrain);
        for t in &mut map.tiles {
            t.land_value = config.land_value_base;
        }
        let city = City {
            name: setup.city_name.clone(),
            treasury: config.starting_treasury,
            tax_rate: config.default_tax_rate,
            stats: CityStats { approval: 500, ..CityStats::default() },
        };
        let mut state = Self { tick: 0, seed: setup.seed, config, map, city };
        // Settle derived values so the very first frame shows sensible numbers.
        systems::land_value::run(&mut state);
        systems::economy::refresh_land_value(&mut state);
        state
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

    /// Advance one tick, applying `commands` (which must all be stamped with this
    /// tick). The pipeline order is fixed: commands, utilities, demand, growth,
    /// land value, economy, status.
    pub fn step(&mut self, commands: &[Command]) -> TickReport {
        let mut report = TickReport { tick: self.tick, ..Default::default() };

        systems::apply::run(self, commands, &mut report);
        systems::utilities::run(self);
        systems::demand::run(self);
        systems::growth::run(self, &mut report);
        systems::land_value::run(self);
        systems::economy::run(self);
        systems::status::run(self);

        self.tick += 1;
        report.month_ended = self.tick % self.config.ticks_per_month as u64 == 0;
        report
    }

    /// Stable 64-bit hash of the complete state, for replay checks.
    pub fn hash(&self) -> u64 {
        let bytes = self.to_bytes();
        crate::hash::fnv1a64(&bytes)
    }

    /// Compact binary form.
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
