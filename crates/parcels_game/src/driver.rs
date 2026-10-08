//! The single place that knows where commands go and where state comes from.
//!
//! Single-player, hot-seat, host and client all look the same to the rest of the
//! game: read `state()`, call `submit()`. Only the command source differs.

use std::time::Duration;

use bevy::prelude::*;
use parcels_net::{HostServer, NetClient};
use parcels_sim::{CommandKind, Controller, GameState, PlayerId, Rejection, Session, TickReport};

pub enum Mode {
    /// Everything in-process: solo, hot-seat, or versus AI.
    Local(Session),
    /// We run the authoritative sim and relay commands to clients.
    Host(Box<HostServer>),
    /// We mirror a remote host.
    Client(Box<NetClient>),
}

#[derive(Resource)]
pub struct Driver {
    pub mode: Mode,
    /// Bumped whenever the state is replaced wholesale (load, snapshot), so views
    /// know to redraw even if the tick number didn't move.
    pub generation: u64,
    pub paused: bool,
    /// Ticks per second at 1x.
    pub speed: u32,
}

/// Feedback from ticks applied this frame, for messages and sound.
#[derive(Resource, Default)]
pub struct TickFeed {
    pub reports: Vec<TickReport>,
}

impl Driver {
    pub fn new(mode: Mode) -> Self {
        Self { mode, generation: 0, paused: false, speed: 4 }
    }

    pub fn state(&self) -> Option<&GameState> {
        match &self.mode {
            Mode::Local(s) => Some(&s.state),
            Mode::Host(h) => h.state(),
            Mode::Client(c) => c.state.as_ref(),
        }
    }

    pub fn is_networked(&self) -> bool {
        !matches!(self.mode, Mode::Local(_))
    }

    /// Can this machine pause, change speed, save, load?
    pub fn is_authority(&self) -> bool {
        !matches!(self.mode, Mode::Client(_))
    }

    /// Players whose commands this machine may issue.
    pub fn controllable(&self) -> Vec<PlayerId> {
        match &self.mode {
            Mode::Local(s) => s.state.players.iter().filter(|p| p.controller == Controller::Human).map(|p| p.id).collect(),
            Mode::Host(h) => h.local_player.into_iter().collect(),
            Mode::Client(c) => c.me.into_iter().collect(),
        }
    }

    pub fn submit(&mut self, author: PlayerId, kind: CommandKind) {
        match &mut self.mode {
            Mode::Local(s) => {
                s.submit(author, kind);
            }
            Mode::Host(h) => h.submit_local(kind),
            Mode::Client(c) => c.submit(kind),
        }
    }

    /// Local-only system command (e.g. hand a parcel to the AI).
    pub fn submit_system(&mut self, kind: CommandKind) -> Result<(), Rejection> {
        match &mut self.mode {
            Mode::Local(s) => {
                s.submit(PlayerId::SYSTEM, kind);
                Ok(())
            }
            _ => Err(Rejection::NotSystem),
        }
    }

    /// Advance the authoritative sim one tick (local / host).
    pub fn tick(&mut self) -> Option<TickReport> {
        match &mut self.mode {
            Mode::Local(s) => Some(s.tick()),
            Mode::Host(h) => h.tick(),
            Mode::Client(_) => None,
        }
    }

    /// Network pump; clients also apply whatever ticks have arrived.
    pub fn poll(&mut self, dt: Duration, feed: &mut TickFeed) {
        match &mut self.mode {
            Mode::Local(_) => {}
            Mode::Host(h) => {
                h.poll(dt);
                h.flush();
            }
            Mode::Client(c) => {
                let had_state = c.state.as_ref().map(|s| s.tick);
                c.poll(dt);
                if c.state.as_ref().map(|s| s.tick) != had_state {
                    self.generation += 1;
                }
                // Catch up quickly when behind, but don't freeze a frame doing it.
                for _ in 0..32 {
                    match c.step() {
                        Some(r) => feed.reports.push(r),
                        None => break,
                    }
                }
                c.flush();
            }
        }
    }

    pub fn replace_state(&mut self, state: GameState) {
        match &mut self.mode {
            Mode::Local(s) => *s = Session::new(state),
            Mode::Host(h) => h.load(state),
            Mode::Client(_) => return,
        }
        self.generation += 1;
    }

    pub fn log(&mut self) -> Vec<String> {
        match &mut self.mode {
            Mode::Local(_) => Vec::new(),
            Mode::Host(h) => std::mem::take(&mut h.log),
            Mode::Client(c) => std::mem::take(&mut c.log),
        }
    }

    pub fn replay(&self) -> Option<&parcels_sim::Replay> {
        match &self.mode {
            Mode::Local(s) => Some(&s.replay),
            Mode::Host(h) => h.session.as_ref().map(|s| &s.replay),
            Mode::Client(_) => None,
        }
    }
}

pub struct DriverPlugin;

impl Plugin for DriverPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TickFeed>()
            .insert_resource(Time::<Fixed>::from_hz(4.0))
            .add_systems(PreUpdate, poll_network.run_if(resource_exists::<Driver>))
            .add_systems(FixedUpdate, fixed_tick.run_if(resource_exists::<Driver>));
    }
}

fn poll_network(mut driver: ResMut<Driver>, mut feed: ResMut<TickFeed>, time: Res<Time<Real>>) {
    driver.poll(time.delta(), &mut feed);
}

/// The simulation clock: one sim tick per fixed step. Rendering never drives it.
fn fixed_tick(mut driver: ResMut<Driver>, mut feed: ResMut<TickFeed>, mut fixed: ResMut<Time<Fixed>>) {
    let hz = driver.speed.max(1) as f64;
    if (fixed.timestep().as_secs_f64() - 1.0 / hz).abs() > 1e-6 {
        fixed.set_timestep_hz(hz);
    }
    if driver.paused || driver.state().is_some_and(|s| s.is_over()) {
        return;
    }
    if let Some(r) = driver.tick() {
        feed.reports.push(r);
    }
}
