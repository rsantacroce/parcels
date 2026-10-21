//! A local command queue around `GameState`: collect commands from the player or
//! autoplay, stamp them, step, and keep a replay log.

use crate::ai;
use crate::command::{Command, CommandKind};
use crate::replay::Replay;
use crate::state::{GameState, TickReport};

pub struct Session {
    pub state: GameState,
    pending: Vec<Command>,
    next_seq: u32,
    pub replay: Replay,
    /// Let the autoplay builder run the city (title screen, tuning runs).
    pub autoplay: bool,
}

impl Session {
    pub fn new(state: GameState) -> Self {
        Self { replay: Replay::new(state.clone()), state, pending: Vec::new(), next_seq: 0, autoplay: false }
    }

    /// Queue a command to execute on the next tick. Returns the stamped command.
    pub fn submit(&mut self, kind: CommandKind) -> Command {
        let cmd = Command { tick: self.state.tick, seq: self.next_seq, kind };
        self.next_seq += 1;
        self.pending.push(cmd.clone());
        cmd
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

    /// Run autoplay if enabled, then step with everything queued.
    pub fn tick(&mut self) -> TickReport {
        if self.autoplay {
            for kind in ai::plan(&self.state) {
                self.submit(kind);
            }
        }
        let cmds = self.take_pending();
        let report = self.state.step(&cmds);
        self.replay.commands.extend(cmds);
        self.replay.final_tick = self.state.tick;
        report
    }
}
