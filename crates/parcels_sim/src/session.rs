//! A local command queue around `GameState`: collect commands from any source
//! (local input, AI, network), stamp them, step, and keep a replay log.
//!
//! Single-player and the network host both drive the sim through this.

use crate::ai;
use crate::command::{Command, CommandKind};
use crate::ids::PlayerId;
use crate::player::Controller;
use crate::replay::Replay;
use crate::state::{GameState, TickReport};

pub struct Session {
    pub state: GameState,
    pending: Vec<Command>,
    /// Next sequence number per author (index 255 = system).
    next_seq: [u32; 256],
    pub replay: Replay,
}

impl Session {
    pub fn new(state: GameState) -> Self {
        Self { replay: Replay::new(state.clone()), state, pending: Vec::new(), next_seq: [0; 256] }
    }

    /// Queue a command to execute on the next tick. Returns the stamped command.
    pub fn submit(&mut self, author: PlayerId, kind: CommandKind) -> Command {
        let seq = &mut self.next_seq[author.0 as usize];
        let cmd = Command { tick: self.state.tick, author, seq: *seq, kind };
        *seq += 1;
        self.pending.push(cmd.clone());
        cmd
    }

    /// Commands every AI-controlled player wants to issue this tick.
    pub fn ai_commands(&mut self) {
        let ais: Vec<PlayerId> =
            self.state.players.iter().filter(|p| matches!(p.controller, Controller::Ai(_))).map(|p| p.id).collect();
        for id in ais {
            for kind in ai::plan(&self.state, id) {
                self.submit(id, kind);
            }
        }
    }

    /// Commands queued for the upcoming tick.
    pub fn take_pending(&mut self) -> Vec<Command> {
        let tick = self.state.tick;
        let mut out = std::mem::take(&mut self.pending);
        for c in &mut out {
            c.tick = tick;
        }
        out
    }

    /// Run AI, then step with everything queued.
    pub fn tick(&mut self) -> TickReport {
        self.ai_commands();
        let cmds = self.take_pending();
        self.step_with(cmds)
    }

    /// Step with an explicit, already-ordered command list (lockstep clients).
    pub fn step_with(&mut self, cmds: Vec<Command>) -> TickReport {
        let report = self.state.step(&cmds);
        self.replay.commands.extend(cmds);
        self.replay.final_tick = self.state.tick;
        report
    }
}
