//! Parcels — Bevy client.
//!
//!   parcels                         main menu
//!   parcels --solo [--ai 3]         straight into a game vs AI neighbours
//!   parcels --host [--port 5757] [--slots 4]
//!   parcels --join 192.168.1.20[:5757]
//!   parcels --load saves/quicksave.ron
//!   parcels --verify-replay saves/replay.ron    (headless determinism check)
//!   common: --name Rob --seed 42

mod audio;
mod camera;
mod driver;
mod menu;
mod render;
mod tools;
mod ui;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

use menu::{AutoStart, MenuState};

#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Menu,
    Lobby,
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(p) = arg(&args, "--verify-replay") {
        verify_replay(&p);
    }

    let mut menu = MenuState::default();
    if let Some(n) = arg(&args, "--name") {
        menu.name = n;
    }
    if let Some(s) = arg(&args, "--seed") {
        menu.seed = s;
    }
    if let Some(n) = arg(&args, "--ai").and_then(|v| v.parse::<u8>().ok()) {
        menu.parcels = (n + 1).clamp(1, 8);
    }
    if let Some(n) = arg(&args, "--slots").and_then(|v| v.parse::<u8>().ok()) {
        menu.host_slots = n.clamp(2, 8);
    }
    if let Some(p) = arg(&args, "--port") {
        menu.port = p;
    }
    if args.iter().any(|a| a == "--solo") {
        menu.auto = Some(AutoStart::Solo);
    } else if args.iter().any(|a| a == "--host") {
        menu.auto = Some(AutoStart::Host);
    } else if args.iter().any(|a| a == "--join") {
        if let Some(a) = arg(&args, "--join") {
            menu.address = a;
        }
        menu.auto = Some(AutoStart::Join);
    } else if let Some(p) = arg(&args, "--load") {
        menu.auto = Some(AutoStart::Load(p));
    }

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Parcels".into(), resolution: (1440, 900).into(), ..default() }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .init_state::<AppState>()
        .insert_resource(ClearColor(Color::srgb(0.08, 0.09, 0.11)))
        .insert_resource(menu)
        .add_plugins((
            driver::DriverPlugin,
            render::RenderPlugin,
            camera::CameraPlugin,
            tools::ToolsPlugin,
            ui::UiPlugin,
            menu::MenuPlugin,
            audio::AudioPlugin,
        ))
        .run();
}
