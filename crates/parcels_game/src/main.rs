//! Parcels — Bevy client.
//!
//!   parcels                                   title screen
//!   parcels --new [--map 128x96] [--city Name] straight into a new city
//!   parcels --load quicksave | saves/x.ron
//!   parcels --verify-replay saves/replay.ron  (headless determinism check)
//!   common: --name Rob --seed 42
//!   dev:    --shot out.png [--street] [--hour 21] [--wait 4] [--screen new|load|settings]
//!           [--hud city,budget,inbox,mayor,region,zones,views,traffic,buses,paused]
//!           [--region slug [--plot x,y]]  open a region map, or a city in it

mod audio;
mod camera;
mod driver;
mod icons;
mod menu;
mod region;
mod region_ui;
mod saves;
mod settings;
mod tools;
mod ui;
mod view;
mod world;

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy_egui::EguiPlugin;

use menu::{AutoStart, MapSize, MenuState};
use settings::Settings;

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Menu,
    /// The region map, between cities.
    Region,
    Playing,
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).filter(|v| !v.starts_with("--")).cloned()
}

fn verify_replay(path: &str) -> ! {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let replay: parcels_sim::Replay = ron::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
    let a = replay.run(|_| {}).hash();
    let b = replay.run(|_| {}).hash();
    println!("{} commands over {} ticks -> hash {a:016x}", replay.commands.len(), replay.final_tick);
    if a == b {
        println!("deterministic: OK");
        std::process::exit(0);
    }
    println!("MISMATCH: second run gave {b:016x}");
    std::process::exit(1);
}

/// `--shot`: wait for the world to finish building, take a screenshot, quit.
#[derive(Resource)]
struct DevShot {
    path: String,
    wait: f32,
    street: bool,
    hour: Option<f32>,
    stage: u8,
    timer: f32,
}

fn dev_shot(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut shot: ResMut<DevShot>,
    chunks: Res<world::chunks::Chunks>,
    mut sky: ResMut<world::scene::Sky>,
    mut street: MessageWriter<camera::StreetRequest>,
    mut exit: MessageWriter<AppExit>,
    state: Res<State<AppState>>,
    driver: Option<Res<driver::Driver>>,
) {
    if let Some(h) = shot.hour {
        sky.hour = h;
    }
    let on_region = *state.get() == AppState::Region;
    if driver.as_ref().and_then(|d| d.state()).is_none() && !on_region {
        return;
    }
    shot.timer += time.delta_secs();
    match shot.stage {
        0 if shot.street && *state.get() == AppState::Playing && shot.timer > 1.0 => {
            street.write(camera::StreetRequest(None));
            shot.stage = 1;
        }
        0 if !shot.street => shot.stage = 1,
        1 if shot.timer > shot.wait && (on_region || chunks.progress() >= 1.0) => {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(shot.path.clone()));
            shot.stage = 2;
            shot.timer = 0.0;
        }
        2 if shot.timer > 1.5 => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(p) = arg(&args, "--verify-replay") {
        verify_replay(&p);
    }

    let mut settings = Settings::load();
    let mut menu = MenuState::default();
    if let Some(n) = arg(&args, "--name") {
        settings.name = n;
    }
    if let Some(s) = arg(&args, "--seed") {
        menu.setup.seed = s;
    }
    if let Some(c) = arg(&args, "--city") {
        menu.setup.city_name = c;
    }
    if let Some((w, h)) = arg(&args, "--map").and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.parse::<u16>().ok()?, h.parse::<u16>().ok()?))
    }) {
        menu.setup.size = MapSize::Custom;
        menu.setup.width = w.clamp(parcels_sim::Map::MIN_SIZE, parcels_sim::Map::MAX_SIZE);
        menu.setup.height = h.clamp(parcels_sim::Map::MIN_SIZE, parcels_sim::Map::MAX_SIZE);
    }
    if let Some(s) = arg(&args, "--screen") {
        menu.screen = match s.as_str() {
            "new" => menu::Screen::NewGame,
            "load" => menu::Screen::Load,
            "settings" => menu::Screen::Settings,
            _ => menu::Screen::Title,
        };
    }
    if let Some(slug) = arg(&args, "--region") {
        let plot = arg(&args, "--plot").and_then(|v| {
            let (x, y) = v.split_once(',')?;
            Some((x.parse().ok()?, y.parse().ok()?))
        });
        menu.auto = Some(AutoStart::Region(slug, plot));
    } else if args.iter().any(|a| a == "--new") {
        menu.auto = Some(AutoStart::New);
    } else if let Some(p) = arg(&args, "--load") {
        menu.auto = Some(AutoStart::Load(p));
    }

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window { title: "Parcels".into(), resolution: (1440, 900).into(), ..default() }),
        ..default()
    }))
    .add_plugins(EguiPlugin::default())
    .init_state::<AppState>()
    .add_message::<camera::StreetRequest>()
    .insert_resource(ClearColor(Color::srgb(world::scene::SKY[0], world::scene::SKY[1], world::scene::SKY[2])))
    .insert_resource(settings)
    .insert_resource(menu)
    .add_plugins((
        driver::DriverPlugin,
        world::WorldPlugin,
        camera::CameraPlugin,
        tools::ToolsPlugin,
        ui::UiPlugin,
        menu::MenuPlugin,
        region_ui::RegionPlugin,
        audio::AudioPlugin,
    ));
    app.init_resource::<view::View>();
    if let Some(h) = arg(&args, "--hud") {
        app.insert_resource(ui::DevHud(h));
    }
    if let Some(path) = arg(&args, "--shot") {
        app.insert_resource(DevShot {
            path,
            wait: arg(&args, "--wait").and_then(|v| v.parse().ok()).unwrap_or(3.0),
            street: args.iter().any(|a| a == "--street"),
            hour: arg(&args, "--hour").and_then(|v| v.parse().ok()),
            stage: 0,
            timer: 0.0,
        })
        .add_systems(Update, dev_shot);
    }
    app.run();
}
