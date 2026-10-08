//! A replay is the starting state plus every command. Re-running it must give the
//! same final hash on any machine: that's the determinism contract.

use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::state::GameState;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Replay {
    pub initial: GameState,
    pub commands: Vec<Command>,
    pub final_tick: u64,
}

impl Replay {
    pub fn new(initial: GameState) -> Self {
        let final_tick = initial.tick;
        Self { initial, commands: Vec::new(), final_tick }
    }

    /// Re-simulate up to (not past) `tick`.
    pub fn run_until(&self, tick: u64) -> GameState {
        let mut r = self.clone();
        r.final_tick = tick.min(self.final_tick);
        r.run(|_| {})
    }

    /// Re-simulate from the initial state, returning the final state.
    /// `on_tick` sees the state after each tick (e.g. to record hashes).
    pub fn run(&self, mut on_tick: impl FnMut(&GameState)) -> GameState {
        let mut state = self.initial.clone();
        let mut i = 0;
        while state.tick < self.final_tick {
            let start = i;
            while i < self.commands.len() && self.commands[i].tick == state.tick {
                i += 1;
            }
            state.step(&self.commands[start..i]);
            on_tick(&state);
        }
        state
    }
}
