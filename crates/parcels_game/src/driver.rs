//! The single place that knows where commands go and where state comes from:
//! read `state()`, call `submit()`.

use bevy::prelude::*;
use parcels_sim::{CommandKind, Config, GameState, NewGame, Session, TerrainSettings, TickReport};

#[derive(Resource)]
pub struct Driver {
    pub session: Session,
    /// Bumped whenever the state is replaced wholesale (load), so views know to
    /// redraw even if the tick number didn't move.
    pub generation: u64,
    pub paused: bool,
    /// Ticks per second at 1x.
    pub speed: u32,
    /// The autoplay city behind the title screen; never saved.
    pub showcase: bool,
    /// Name this game was last saved or loaded under.
    pub save_name: Option<String>,
    /// The region plot this city lives on, if it's part of a region.
    pub region: Option<crate::region::Slot>,
}

/// Feedback from ticks applied this frame, for messages and sound.
#[derive(Resource, Default)]
pub struct TickFeed {
    pub reports: Vec<TickReport>,
}

impl Driver {
    pub fn new(state: GameState) -> Self {
        Self { session: Session::new(state), generation: 0, paused: false, speed: 4, showcase: false, save_name: None, region: None }
    }

    /// A small autoplay city, already a few years old, to look at behind the menu.
    pub fn showcase(seed: u64) -> Self {
        let mut config = Config::default();
        config.map_width = 64;
        config.map_height = 48;
        config.fire_chance_per_million = 0;
        let terrain = TerrainSettings { rivers: 1, lakes: 1, forest: 2 };
        let mut session =
            Session::new(GameState::new(&NewGame { seed, config, city_name: "Showcase".into(), terrain }));
        session.autoplay = true;
        for _ in 0..1500 {
            session.tick();
        }
        // Drop the replay log: nobody will export it, and it only grows.
        let mut d = Self::new(session.state);
        d.session.autoplay = true;
        d.showcase = true;
        d.speed = 8;
        d
    }

    pub fn state(&self) -> Option<&GameState> {
        Some(&self.session.state)
    }

    pub fn submit(&mut self, kind: CommandKind) {
        self.session.submit(kind);
    }

    pub fn replace_state(&mut self, state: GameState) {
        let autoplay = self.session.autoplay;
        self.session = Session::new(state);
        self.session.autoplay = autoplay;
        self.generation += 1;
    }

    pub fn replay(&self) -> &parcels_sim::Replay {
        &self.session.replay
    }
}

pub struct DriverPlugin;

impl Plugin for DriverPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TickFeed>()
            .insert_resource(Time::<Fixed>::from_hz(4.0))
            .add_systems(FixedUpdate, fixed_tick.run_if(resource_exists::<Driver>));
    }
}

/// The simulation clock: one sim tick per fixed step. Rendering never drives it.
fn fixed_tick(mut driver: ResMut<Driver>, mut feed: ResMut<TickFeed>, mut fixed: ResMut<Time<Fixed>>) {
    let hz = driver.speed.max(1) as f64;
    if (fixed.timestep().as_secs_f64() - 1.0 / hz).abs() > 1e-6 {
        fixed.set_timestep_hz(hz);
    }
    if driver.paused {
        return;
    }
    let r = driver.session.tick();
    feed.reports.push(r);
}
