//! Main menu and multiplayer lobby.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use parcels_net::{parse_addr, HostServer, NetClient, ServerOptions, DEFAULT_PORT};
use parcels_sim::state::ai_name;
use parcels_sim::{AiStrategy, Config, Controller, GameState, NewGame, PlayerSetup, Session};

use crate::driver::{Driver, Mode};
use crate::ui::{Toasts, SAVE_DIR};
use crate::AppState;

#[derive(Resource)]
pub struct MenuState {
    pub name: String,
    pub parcels: u8,
    pub humans: u8,
    pub seed: String,
    pub port: String,
    pub address: String,
    pub host_slots: u8,
    pub error: Option<String>,
    /// Set from the command line to skip straight into a mode.
    pub auto: Option<AutoStart>,
}

#[derive(Clone, Debug)]
pub enum AutoStart {
    Solo,
    Host,
    Join,
    Load(String),
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            name: std::env::var("USER").unwrap_or_else(|_| "Mayor".into()),
            parcels: 4,
            humans: 1,
            seed: String::new(),
            port: DEFAULT_PORT.to_string(),
            address: format!("127.0.0.1:{DEFAULT_PORT}"),
            host_slots: 4,
            error: None,
            auto: None,
        }
    }
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .add_systems(OnEnter(AppState::Menu), leave_game)
            .add_systems(Update, auto_start.run_if(in_state(AppState::Menu)))
            .add_systems(Update, lobby_progress.run_if(in_state(AppState::Lobby)))
            .add_systems(EguiPrimaryContextPass, menu_ui.run_if(in_state(AppState::Menu)))
            .add_systems(EguiPrimaryContextPass, lobby_ui.run_if(in_state(AppState::Lobby)));
    }
}

pub fn load_config() -> Config {
    let candidates = ["config/balance.ron".to_string(), concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/balance.ron").to_string()];
    for path in candidates {
        if let Ok(text) = std::fs::read_to_string(&path) {
            match Config::from_ron(&text) {
                Ok(c) => return c,
                Err(e) => eprintln!("{path}: {e}; using defaults"),
            }
        }
    }
    Config::default()
}

fn seed_from(text: &str) -> u64 {
    text.trim().parse().unwrap_or_else(|_| {
        if text.trim().is_empty() {
            // Seed choice is outside the sim; once chosen it's part of the state.
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(1)
        } else {
            parcels_sim::hash::fnv1a64(text.as_bytes())
        }
    })
}

fn leave_game(mut commands: Commands, driver: Option<ResMut<Driver>>) {
    if let Some(mut d) = driver {
        match &mut d.mode {
            Mode::Host(h) => {
                h.disconnect_all();
                h.flush();
            }
            Mode::Client(c) => {
                c.disconnect();
            }
            Mode::Local(_) => {}
        }
        commands.remove_resource::<Driver>();
    }
}

fn start_solo(commands: &mut Commands, m: &MenuState) -> Driver {
    let mut players = Vec::new();
    for i in 0..m.parcels {
        if i < m.humans {
            let name = if m.humans == 1 { m.name.clone() } else { format!("{} {}", m.name, i + 1) };
            players.push(PlayerSetup { name, controller: Controller::Human });
        } else {
            let k = (i - m.humans) as usize;
            let strategy = if k % 2 == 0 { AiStrategy::UtilityBaron } else { AiStrategy::Developer };
            players.push(PlayerSetup { name: ai_name(k), controller: Controller::Ai(strategy) });
        }
    }
    let state = GameState::new(&NewGame { seed: seed_from(&m.seed), config: load_config(), players });
    commands.insert_resource(Toasts::default());
    Driver::new(Mode::Local(Session::new(state)))
}

fn start_host(m: &MenuState) -> Result<Driver, String> {
    let port: u16 = m.port.trim().parse().map_err(|_| "Port must be a number".to_string())?;
    let host = HostServer::bind(ServerOptions {
        bind: format!("0.0.0.0:{port}").parse().unwrap(),
        slots: m.host_slots,
        seed: seed_from(&m.seed),
        config: load_config(),
        host_name: Some(m.name.clone()),
    })
    .map_err(|e| format!("Can't host on port {port}: {e}"))?;
    Ok(Driver::new(Mode::Host(Box::new(host))))
}

fn start_join(m: &MenuState) -> Result<Driver, String> {
    let addr = parse_addr(&m.address).map_err(|e| e.to_string())?;
    let client = NetClient::connect(addr, &m.name).map_err(|e| format!("Can't connect: {e}"))?;
    Ok(Driver::new(Mode::Client(Box::new(client))))
}

fn start_load(path: &str) -> Result<Driver, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let state = GameState::from_ron(&text).map_err(|e| format!("{path}: {e}"))?;
    Ok(Driver::new(Mode::Local(Session::new(state))))
}

fn auto_start(mut commands: Commands, mut m: ResMut<MenuState>, mut next: ResMut<NextState<AppState>>) {
    let Some(auto) = m.auto.take() else { return };
    let result = match auto {
        AutoStart::Solo => Ok((start_solo(&mut commands, &m), AppState::Playing)),
        AutoStart::Host => start_host(&m).map(|d| (d, AppState::Lobby)),
        AutoStart::Join => start_join(&m).map(|d| (d, AppState::Lobby)),
        AutoStart::Load(p) => start_load(&p).map(|d| (d, AppState::Playing)),
    };
    match result {
        Ok((d, s)) => {
            commands.insert_resource(d);
            next.set(s);
        }
        Err(e) => m.error = Some(e),
    }
}

fn menu_ui(mut contexts: EguiContexts, mut commands: Commands, mut m: ResMut<MenuState>, mut next: ResMut<NextState<AppState>>) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut go: Option<Result<(Driver, AppState), String>> = None;
    egui::Window::new("Parcels").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
        ui.label(egui::RichText::new("A neighbourhood city builder: one shared map, separate budgets.").italics());
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Your name");
            ui.text_edit_singleline(&mut m.name);
        });
        ui.horizontal(|ui| {
            ui.label("Map seed");
            ui.add(egui::TextEdit::singleline(&mut m.seed).hint_text("random"));
        });
        ui.separator();
        ui.heading("Single player / hot-seat");
        ui.add(egui::Slider::new(&mut m.parcels, 1..=8).text("parcels"));
        let max_h = m.parcels;
        m.humans = m.humans.clamp(1, max_h);
        ui.add(egui::Slider::new(&mut m.humans, 1..=max_h).text("human players (rest are AI)"));
        if ui.button("Start game").clicked() {
            go = Some(Ok((start_solo(&mut commands, &m), AppState::Playing)));
        }
        let save = format!("{SAVE_DIR}/quicksave.ron");
        if std::path::Path::new(&save).exists() && ui.button("Continue saved game").clicked() {
            go = Some(start_load(&save).map(|d| (d, AppState::Playing)));
        }
        ui.separator();
        ui.heading("Multiplayer");
        ui.horizontal(|ui| {
            ui.label("Port");
            ui.add(egui::TextEdit::singleline(&mut m.port).desired_width(60.0));
            ui.add(egui::Slider::new(&mut m.host_slots, 2..=8).text("parcels"));
            if ui.button("Host").clicked() {
                go = Some(start_host(&m).map(|d| (d, AppState::Lobby)));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Address");
            ui.add(egui::TextEdit::singleline(&mut m.address).desired_width(160.0));
            if ui.button("Join").clicked() {
                go = Some(start_join(&m).map(|d| (d, AppState::Lobby)));
            }
        });
        if let Some(e) = &m.error {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        }
    });
    match go {
        Some(Ok((d, s))) => {
            m.error = None;
            commands.insert_resource(d);
            next.set(s);
        }
        Some(Err(e)) => m.error = Some(e),
        None => {}
    }
    Ok(())
}

fn lobby_progress(mut driver: ResMut<Driver>, mut next: ResMut<NextState<AppState>>, mut m: ResMut<MenuState>) {
    let ready = match &driver.mode {
        Mode::Client(c) => {
            if c.is_disconnected() {
                m.error = Some(format!("Disconnected: {}", c.disconnect_reason().unwrap_or_default()));
                next.set(AppState::Menu);
                return;
            }
            c.state.is_some()
        }
        Mode::Host(h) => h.started(),
        Mode::Local(_) => true,
    };
    if ready {
        driver.generation += 1;
        next.set(AppState::Playing);
    }
}

fn lobby_ui(mut contexts: EguiContexts, mut driver: ResMut<Driver>, mut next: ResMut<NextState<AppState>>) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui::Window::new("Lobby").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
        match &mut driver.mode {
            Mode::Host(h) => {
                if let Some(a) = h.local_addr() {
                    ui.label(format!("Hosting on port {}. Friends join with your IP and that port.", a.port()));
                }
                ui.label(format!("{} parcels; empty seats are played by the AI and can be taken over later.", h.slots()));
                ui.separator();
                for e in h.lobby() {
                    ui.label(format!("• {}{}", e.name, if e.is_host { " (host)" } else { "" }));
                }
                ui.separator();
                if ui.button("Start game").clicked() {
                    h.start();
                }
            }
            Mode::Client(c) => {
                if !c.is_connected() {
                    ui.label("Connecting…");
                } else if c.lobby.is_empty() {
                    ui.label("Connected. Waiting for the host…");
                } else {
                    ui.label(format!("Waiting for the host to start ({} parcels):", c.slots));
                    for e in &c.lobby {
                        ui.label(format!("• {}{}", e.name, if e.is_host { " (host)" } else { "" }));
                    }
                }
                for line in c.log.iter().rev().take(3) {
                    ui.weak(line);
                }
            }
            Mode::Local(_) => {}
        }
        if ui.button("Cancel").clicked() {
            next.set(AppState::Menu);
        }
    });
    Ok(())
}
