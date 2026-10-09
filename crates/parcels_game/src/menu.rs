//! Title screen, its sub-screens (new game, load, play with a friend,
//! settings) and the multiplayer lobby. Behind the title an AI-only showcase
//! city grows while the camera circles it.

use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use parcels_net::{lan_ip, parse_addr, HostServer, NetClient, ServerOptions, DEFAULT_PORT};
use parcels_sim::state::ai_name;
use parcels_sim::{fmt_money, AiStrategy, Config, Controller, GameState, Map, NewGame, PlayerSetup, Session, TerrainSettings};

use crate::driver::{Driver, Mode};
use crate::saves;
use crate::settings::{Crowds, DayCycle, Settings};
use crate::ui::{color32, Toasts};
use crate::view::{map_color, Overlay};
use crate::AppState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Title,
    NewGame,
    Load,
    Friends,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MapSize {
    Small,
    Medium,
    Large,
    Huge,
    Custom,
}

impl MapSize {
    const ALL: [MapSize; 5] = [MapSize::Small, MapSize::Medium, MapSize::Large, MapSize::Huge, MapSize::Custom];

    fn dims(self) -> Option<(u16, u16)> {
        match self {
            MapSize::Small => Some((64, 48)),
            MapSize::Medium => Some((96, 72)),
            MapSize::Large => Some((128, 96)),
            MapSize::Huge => Some((192, 144)),
            MapSize::Custom => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            MapSize::Small => "Small",
            MapSize::Medium => "Medium",
            MapSize::Large => "Large",
            MapSize::Huge => "Huge",
            MapSize::Custom => "Custom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AiMix {
    Mixed,
    Barons,
    Developers,
}

/// Everything the new-game and host screens let you choose.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GameSetup {
    pub size: MapSize,
    pub width: u16,
    pub height: u16,
    pub terrain: TerrainSettings,
    pub parcels: u8,
    pub humans: u8,
    pub ai: AiMix,
    /// Years; 0 = endless.
    pub years: u8,
    /// Starting money in whole dollars.
    pub money: i64,
    pub fires: bool,
    pub seed: String,
}

impl Default for GameSetup {
    fn default() -> Self {
        Self {
            size: MapSize::Medium,
            width: 96,
            height: 72,
            terrain: TerrainSettings::default(),
            parcels: 4,
            humans: 1,
            ai: AiMix::Mixed,
            years: 10,
            money: 20_000,
            fires: true,
            seed: String::new(),
        }
    }
}

impl GameSetup {
    pub fn dims(&self) -> (u16, u16) {
        self.size.dims().unwrap_or((self.width, self.height))
    }

    pub fn config(&self) -> Config {
        let mut c = load_config();
        let (w, h) = self.dims();
        c.map_width = w;
        c.map_height = h;
        c.game_length_ticks = self.years as u64 * 12 * c.ticks_per_month as u64;
        c.starting_treasury = self.money * 100;
        if !self.fires {
            c.fire_chance_per_million = 0;
        }
        c
    }

    fn ai_strategy(&self, k: usize) -> AiStrategy {
        match self.ai {
            AiMix::Barons => AiStrategy::UtilityBaron,
            AiMix::Developers => AiStrategy::Developer,
            AiMix::Mixed if k % 2 == 0 => AiStrategy::UtilityBaron,
            AiMix::Mixed => AiStrategy::Developer,
        }
    }

    pub fn players(&self, name: &str) -> Vec<PlayerSetup> {
        (0..self.parcels)
            .map(|i| {
                if i < self.humans {
                    let name = if self.humans == 1 { name.to_string() } else { format!("{name} {}", i + 1) };
                    PlayerSetup { name, controller: Controller::Human }
                } else {
                    let k = (i - self.humans) as usize;
                    PlayerSetup { name: ai_name(k), controller: Controller::Ai(self.ai_strategy(k)) }
                }
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub enum AutoStart {
    Solo,
    Host,
    Join,
    Load(String),
}

#[derive(Resource)]
pub struct MenuState {
    pub screen: Screen,
    pub setup: GameSetup,
    pub port: String,
    pub address: String,
    pub host_slots: u8,
    pub error: Option<String>,
    /// Set from the command line to skip straight into a mode.
    pub auto: Option<AutoStart>,
    confirm_delete: Option<String>,
    preview: Option<(u64, egui::TextureHandle)>,
    showcase: Option<Mutex<Receiver<Driver>>>,
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            screen: Screen::Title,
            setup: GameSetup::default(),
            port: DEFAULT_PORT.to_string(),
            address: format!("127.0.0.1:{DEFAULT_PORT}"),
            host_slots: 4,
            error: None,
            auto: None,
            confirm_delete: None,
            preview: None,
            showcase: None,
        }
    }
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .add_systems(OnEnter(AppState::Menu), leave_game)
            .add_systems(Update, (auto_start, receive_showcase).run_if(in_state(AppState::Menu)))
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

fn wall_seed() -> u64 {
    // Seed choice is outside the sim; once chosen it's part of the state.
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(1)
}

fn seed_from(text: &str) -> u64 {
    text.trim().parse().unwrap_or_else(|_| if text.trim().is_empty() { wall_seed() } else { parcels_sim::hash::fnv1a64(text.as_bytes()) })
}

/// Shared look for every egui screen. Cheap to call every frame.
pub fn theme(ctx: &egui::Context, settings: &Settings) {
    if (ctx.zoom_factor() - settings.ui_scale).abs() > 0.01 {
        ctx.set_zoom_factor(settings.ui_scale);
    }
    const MARK: egui::Vec2 = egui::vec2(9.0, 4.0);
    if ctx.global_style().spacing.button_padding == MARK {
        return;
    }
    ctx.all_styles_mut(|style| {
        style.visuals = egui::Visuals::dark();
        let v = &mut style.visuals;
        v.window_corner_radius = 10.into();
        v.menu_corner_radius = 8.into();
        v.window_fill = egui::Color32::from_rgb(24, 28, 37);
        v.panel_fill = egui::Color32::from_rgb(20, 24, 31);
        v.extreme_bg_color = egui::Color32::from_rgb(14, 17, 22);
        v.selection.bg_fill = egui::Color32::from_rgb(46, 130, 120);
        v.hyperlink_color = egui::Color32::from_rgb(110, 200, 190);
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = 6.into();
        }
        v.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(38, 44, 56);
        v.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(52, 62, 78);
        style.spacing.button_padding = MARK;
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        for (ts, f) in style.text_styles.iter_mut() {
            f.size = match ts {
                egui::TextStyle::Heading => 20.0,
                egui::TextStyle::Body | egui::TextStyle::Button => 14.0,
                egui::TextStyle::Small => 11.5,
                egui::TextStyle::Monospace => 13.5,
                _ => f.size,
            };
        }
    });
}

/// Grow a fresh showcase city on a worker thread so the window opens at once.
fn start_showcase(m: &mut MenuState) {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(Driver::showcase(wall_seed()));
    });
    m.showcase = Some(Mutex::new(rx));
}

fn receive_showcase(mut commands: Commands, mut m: ResMut<MenuState>, driver: Option<Res<Driver>>) {
    let ready = m.showcase.as_ref().and_then(|rx| rx.lock().ok()?.try_recv().ok());
    if let Some(d) = ready {
        m.showcase = None;
        if driver.is_none() {
            commands.insert_resource(d);
        }
    }
}

fn leave_game(mut commands: Commands, driver: Option<ResMut<Driver>>, mut m: ResMut<MenuState>) {
    if let Some(mut d) = driver {
        if d.showcase {
            return;
        }
        match &mut d.mode {
            Mode::Host(h) => {
                h.disconnect_all();
                h.flush();
            }
            Mode::Client(c) => c.disconnect(),
            Mode::Local(_) => {}
        }
        commands.remove_resource::<Driver>();
    }
    if m.showcase.is_none() {
        start_showcase(&mut m);
    }
}

fn start_solo(m: &MenuState, name: &str) -> Driver {
    let s = &m.setup;
    let state = GameState::new(&NewGame { seed: seed_from(&s.seed), config: s.config(), players: s.players(name), terrain: s.terrain });
    Driver::new(Mode::Local(Session::new(state)))
}

fn host_options(m: &MenuState, name: &str) -> Result<ServerOptions, String> {
    let port: u16 = m.port.trim().parse().map_err(|_| "Port must be a number".to_string())?;
    Ok(ServerOptions {
        bind: format!("0.0.0.0:{port}").parse().unwrap(),
        slots: m.host_slots,
        seed: seed_from(&m.setup.seed),
        config: m.setup.config(),
        terrain: m.setup.terrain,
        host_name: Some(name.to_string()),
    })
}

fn start_host(m: &MenuState, name: &str, from_save: Option<GameState>) -> Result<Driver, String> {
    let opts = host_options(m, name)?;
    let port = opts.bind.port();
    let mut host = HostServer::bind(opts).map_err(|e| format!("Can't host on port {port}: {e}"))?;
    if let Some(state) = from_save {
        host.load(state);
    }
    Ok(Driver::new(Mode::Host(Box::new(host))))
}

fn start_join(m: &MenuState, name: &str) -> Result<Driver, String> {
    let addr = parse_addr(&m.address).map_err(|e| e.to_string())?;
    let client = NetClient::connect(addr, name).map_err(|e| format!("Can't connect: {e}"))?;
    Ok(Driver::new(Mode::Client(Box::new(client))))
}

fn start_load(slug_or_path: &str) -> Result<Driver, String> {
    let state = if slug_or_path.ends_with(".ron") {
        let text = std::fs::read_to_string(slug_or_path).map_err(|e| format!("{slug_or_path}: {e}"))?;
        GameState::from_ron(&text).map_err(|e| format!("{slug_or_path}: {e}"))?
    } else {
        saves::load(slug_or_path)?
    };
    let mut d = Driver::new(Mode::Local(Session::new(state)));
    d.save_name = saves::list().into_iter().find(|e| e.slug == slug_or_path).map(|e| e.title());
    Ok(d)
}

fn auto_start(
    mut commands: Commands,
    mut m: ResMut<MenuState>,
    settings: Res<Settings>,
    mut next: ResMut<NextState<AppState>>,
) {
    let Some(auto) = m.auto.take() else { return };
    let name = settings.name.clone();
    let result = match auto {
        AutoStart::Solo => Ok((start_solo(&m, &name), AppState::Playing)),
        AutoStart::Host => start_host(&m, &name, None).map(|d| (d, AppState::Lobby)),
        AutoStart::Join => start_join(&m, &name).map(|d| (d, AppState::Lobby)),
        AutoStart::Load(p) => start_load(&p).map(|d| (d, AppState::Playing)),
    };
    match result {
        Ok((d, s)) => {
            commands.insert_resource(d);
            commands.insert_resource(Toasts::default());
            next.set(s);
        }
        Err(e) => m.error = Some(e),
    }
}

/// The list of saves with load and delete buttons. `on_load` gets the slug.
pub fn load_list(ui: &mut egui::Ui, confirm_delete: &mut Option<String>, mut on_load: impl FnMut(&str)) {
    let entries = saves::list();
    if entries.is_empty() {
        ui.weak("No saved games yet. Save from the in-game menu (F10) or quick-save with F5.");
        return;
    }
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        for e in &entries {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(e.title()).strong().size(15.0));
                        match &e.meta {
                            Some(meta) => {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Year {} month {} · {}×{} map · population {} · saved {}",
                                        meta.year, meta.month, meta.map.0, meta.map.1, meta.population, e.age()
                                    ))
                                    .small(),
                                );
                                let who: Vec<String> = meta.players.iter().map(|(n, h, _)| if *h { format!("{n} 👤") } else { n.clone() }).collect();
                                ui.label(egui::RichText::new(who.join(", ")).small().weak());
                            }
                            None => {
                                ui.label(egui::RichText::new(format!("saved {}", e.age())).small().weak());
                            }
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if confirm_delete.as_deref() == Some(&e.slug) {
                            if ui.button(egui::RichText::new("Really delete").color(egui::Color32::LIGHT_RED)).clicked() {
                                let _ = saves::delete(&e.slug);
                                *confirm_delete = None;
                            }
                            if ui.button("Keep").clicked() {
                                *confirm_delete = None;
                            }
                        } else {
                            if ui.button("🗑").on_hover_text("Delete").clicked() {
                                *confirm_delete = Some(e.slug.clone());
                            }
                            if ui.button(egui::RichText::new("▶ Load").strong()).clicked() {
                                on_load(&e.slug);
                            }
                        }
                    });
                });
            });
        }
    });
}

fn setup_hash(s: &GameSetup) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::hash::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// A top-down thumbnail of the map these settings would generate.
fn preview(ctx: &egui::Context, m: &mut MenuState, name: &str) -> Option<egui::TextureHandle> {
    let mut s = m.setup.clone();
    if s.seed.trim().is_empty() {
        // Show something stable until a seed is chosen.
        s.seed = "preview".into();
    }
    let key = setup_hash(&s);
    if m.preview.as_ref().is_none_or(|(k, _)| *k != key) {
        let state = GameState::new(&NewGame { seed: seed_from(&s.seed), config: s.config(), players: s.players(name), terrain: s.terrain });
        let (w, h) = (state.map.width as usize, state.map.height as usize);
        let mut rgba = Vec::with_capacity(w * h * 4);
        for i in 0..w * h {
            let mut c = map_color(&state, Overlay::None, i);
            // Parcel borders.
            let p = state.map.pos(i);
            let edge = state.parcels.iter().any(|pc| {
                pc.rect.contains(p) && (p.x == pc.rect.min.x || p.y == pc.rect.min.y || p.x == pc.rect.max.x || p.y == pc.rect.max.y)
            });
            if edge {
                let o = state.owner_at(p).map(crate::view::player_color).unwrap_or([255, 255, 255]);
                c = crate::view::mix(c, o, 200);
            }
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
        let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
        let tex = ctx.load_texture("map-preview", img, egui::TextureOptions::NEAREST);
        m.preview = Some((key, tex));
    }
    m.preview.as_ref().map(|(_, t)| t.clone())
}

fn setup_editor(ui: &mut egui::Ui, s: &mut GameSetup, multiplayer: bool) {
    egui::Grid::new(("setup", multiplayer)).num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
        ui.label("Map size");
        ui.horizontal(|ui| {
            for size in MapSize::ALL {
                let label = match size.dims() {
                    Some((w, h)) => format!("{} {w}×{h}", size.label()),
                    None => size.label().to_string(),
                };
                ui.selectable_value(&mut s.size, size, label);
            }
        });
        ui.end_row();
        if s.size == MapSize::Custom {
            ui.label("");
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut s.width, Map::MIN_SIZE..=Map::MAX_SIZE).text("wide"));
                ui.add(egui::Slider::new(&mut s.height, Map::MIN_SIZE..=Map::MAX_SIZE).text("deep"));
            });
            ui.end_row();
        }
        ui.label("Rivers");
        ui.horizontal(|ui| {
            for (n, l) in [(0, "None"), (1, "One"), (2, "Two")] {
                ui.selectable_value(&mut s.terrain.rivers, n, l);
            }
        });
        ui.end_row();
        ui.label("Lakes");
        ui.horizontal(|ui| {
            for (n, l) in [(0, "None"), (1, "Few"), (2, "Some"), (4, "Many")] {
                ui.selectable_value(&mut s.terrain.lakes, n, l);
            }
        });
        ui.end_row();
        ui.label("Woods");
        ui.horizontal(|ui| {
            for (n, l) in [(0, "None"), (1, "Few"), (2, "Some"), (3, "Lots")] {
                ui.selectable_value(&mut s.terrain.forest, n, l);
            }
        });
        ui.end_row();
        if !multiplayer {
            ui.label("Neighbourhoods");
            ui.add(egui::Slider::new(&mut s.parcels, 1..=8).text("parcels"));
            ui.end_row();
            ui.label("Human players");
            let max_h = s.parcels;
            s.humans = s.humans.clamp(1, max_h);
            ui.add(egui::Slider::new(&mut s.humans, 1..=max_h).text("on this computer (hot-seat)"));
            ui.end_row();
        }
        ui.label("AI neighbours");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut s.ai, AiMix::Mixed, "Mixed");
            ui.selectable_value(&mut s.ai, AiMix::Barons, "Utility barons");
            ui.selectable_value(&mut s.ai, AiMix::Developers, "Developers");
        });
        ui.end_row();
        ui.label("Game length");
        ui.horizontal(|ui| {
            for (y, l) in [(5, "5 years"), (10, "10 years"), (20, "20 years"), (0, "Endless")] {
                ui.selectable_value(&mut s.years, y, l);
            }
        });
        ui.end_row();
        ui.label("Starting money");
        ui.horizontal(|ui| {
            for (v, l) in [(40_000, "Easy $40k"), (20_000, "Normal $20k"), (10_000, "Hard $10k")] {
                ui.selectable_value(&mut s.money, v, l);
            }
        });
        ui.end_row();
        ui.label("Fires");
        ui.checkbox(&mut s.fires, "Buildings without a fire station nearby can burn");
        ui.end_row();
        ui.label("Map seed");
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut s.seed).hint_text("random").desired_width(160.0));
            if ui.button("🎲").on_hover_text("Roll a new map").clicked() {
                s.seed = (wall_seed() ^ (s.seed.len() as u64 * 7919)).wrapping_mul(2654435761).to_string();
            }
        });
        ui.end_row();
    });
}

#[allow(clippy::too_many_arguments)]
fn menu_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut m: ResMut<MenuState>,
    mut settings: ResMut<Settings>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    driver: Option<Res<Driver>>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    theme(&ctx, &settings);
    let mut go: Option<Result<(Driver, AppState), String>> = None;
    // Only saves this version wrote (they have metadata) are offered to continue.
    let latest = saves::list().into_iter().find(|e| e.meta.is_some());

    // Title column.
    egui::Area::new("title".into()).anchor(egui::Align2::LEFT_TOP, egui::vec2(48.0, 48.0)).show(&ctx, |ui| {
        egui::Frame::new().fill(egui::Color32::from_rgba_unmultiplied(14, 18, 26, 215)).corner_radius(14).inner_margin(26).show(ui, |ui| {
            ui.set_width(270.0);
            ui.label(egui::RichText::new("PARCELS").size(48.0).strong().color(egui::Color32::from_rgb(240, 236, 220)));
            ui.label(egui::RichText::new("One shared map. Separate budgets.\nBuild your neighbourhood; mind the one next door.").italics().color(egui::Color32::from_gray(190)));
            ui.add_space(18.0);
            let big = |ui: &mut egui::Ui, t: &str, on: bool| {
                ui.add_sized([270.0, 40.0], egui::Button::new(egui::RichText::new(t).size(17.0)).selected(on)).clicked()
            };
            if let Some(e) = &latest {
                if big(ui, &format!("▶  Continue  ·  {}", e.title()), false) {
                    go = Some(start_load(&e.slug).map(|d| (d, AppState::Playing)));
                }
            }
            let screen = m.screen;
            if big(ui, "✨  New game", screen == Screen::NewGame) {
                m.screen = Screen::NewGame;
            }
            if big(ui, "📂  Load game", screen == Screen::Load) {
                m.screen = Screen::Load;
            }
            if big(ui, "👥  Play with a friend", screen == Screen::Friends) {
                m.screen = Screen::Friends;
            }
            if big(ui, "⚙  Settings", screen == Screen::Settings) {
                m.screen = Screen::Settings;
            }
            if big(ui, "✖  Quit", false) {
                exit.write(AppExit::Success);
            }
            ui.add_space(10.0);
            if let Some(e) = &m.error {
                ui.colored_label(egui::Color32::LIGHT_RED, e);
            }
            if driver.is_none() {
                ui.label(egui::RichText::new("Growing a city to look at…").small().weak());
            }
        });
    });
    egui::Area::new("version".into()).anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -8.0)).interactable(false).show(&ctx, |ui| {
        ui.label(egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).small().weak());
    });

    // The open screen, as a card to the right of the buttons.
    let card = |title: &str| {
        egui::Window::new(title.to_string())
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(400.0, 48.0))
            .default_width(620.0)
    };
    let name = settings.name.clone();
    match m.screen {
        Screen::Title => {}
        Screen::NewGame => {
            card("New game").show(&ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Mayor");
                    if ui.text_edit_singleline(&mut settings.name).lost_focus() {
                        let _ = settings.save();
                    }
                });
                ui.separator();
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        setup_editor(ui, &mut m.setup, false);
                    });
                    if let Some(tex) = preview(&ctx, &mut m, &name) {
                        let s = tex.size_vec2();
                        let k = 220.0 / s.x.max(s.y);
                        ui.vertical(|ui| {
                            ui.image((tex.id(), s * k));
                            let (w, h) = m.setup.dims();
                            ui.small(format!("{w}×{h} tiles · {} parcels", m.setup.parcels));
                        });
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.add_sized([180.0, 36.0], egui::Button::new(egui::RichText::new("Start building").strong().size(16.0))).clicked() {
                        go = Some(Ok((start_solo(&m, &name), AppState::Playing)));
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "{} you{} vs {} AI · {}",
                            m.setup.humans,
                            if m.setup.humans > 1 { " (hot-seat, Tab switches)" } else { "" },
                            m.setup.parcels - m.setup.humans,
                            fmt_money(m.setup.money * 100).trim_end_matches(".00")
                        ))
                        .weak(),
                    );
                });
            });
        }
        Screen::Load => {
            card("Load game").show(&ctx, |ui| {
                let mut chosen = None;
                let mut confirm = m.confirm_delete.take();
                load_list(ui, &mut confirm, |slug| chosen = Some(slug.to_string()));
                m.confirm_delete = confirm;
                if let Some(slug) = chosen {
                    go = Some(start_load(&slug).map(|d| (d, AppState::Playing)));
                }
                ui.separator();
                ui.small("Tip: to continue a save with friends, load it here, or host it from \"Play with a friend\".");
            });
        }
        Screen::Friends => {
            card("Play with a friend").show(&ctx, |ui| {
                ui.columns(2, |cols| {
                    let ui = &mut cols[0];
                    ui.heading("Host");
                    ui.label("Start a game others can join over your network.");
                    egui::Grid::new("host").num_columns(2).show(ui, |ui| {
                        ui.label("Players");
                        let mut slots = m.host_slots;
                        ui.add(egui::Slider::new(&mut slots, 2..=8).text("parcels"));
                        m.host_slots = slots;
                        ui.end_row();
                        ui.label("Port");
                        ui.add(egui::TextEdit::singleline(&mut m.port).desired_width(70.0));
                        ui.end_row();
                    });
                    ui.collapsing("Map & rules", |ui| {
                        setup_editor(ui, &mut m.setup, true);
                    });
                    if let Some(ip) = lan_ip() {
                        ui.label(egui::RichText::new(format!("Friends will connect to {ip}:{}", m.port.trim())).weak());
                    }
                    if ui.add_sized([200.0, 34.0], egui::Button::new(egui::RichText::new("Host new game").strong())).clicked() {
                        go = Some(start_host(&m, &name, None).map(|d| (d, AppState::Lobby)));
                    }
                    ui.add_space(4.0);
                    ui.menu_button("Host a saved game…", |ui| {
                        for e in saves::list().into_iter().take(12) {
                            if ui.button(e.title()).clicked() {
                                go = Some(saves::load(&e.slug).and_then(|s| start_host(&m, &name, Some(s))).map(|d| (d, AppState::Lobby)));
                                ui.close();
                            }
                        }
                    });
                    ui.small("Empty seats are played by the AI; friends who join later take one over.");

                    let ui = &mut cols[1];
                    ui.heading("Join");
                    ui.label("Ask the host for their address.");
                    ui.horizontal(|ui| {
                        ui.label("Address");
                        ui.add(egui::TextEdit::singleline(&mut m.address).desired_width(170.0));
                    });
                    if ui.add_sized([200.0, 34.0], egui::Button::new(egui::RichText::new("Join game").strong())).clicked() {
                        go = Some(start_join(&m, &name).map(|d| (d, AppState::Lobby)));
                    }
                    ui.add_space(10.0);
                    ui.heading("Same computer");
                    ui.label("Take turns on one keyboard: start a new game with 2+ human players and press Tab to switch.");
                    if ui.button("Set up hot-seat").clicked() {
                        m.setup.humans = m.setup.humans.max(2);
                        m.setup.parcels = m.setup.parcels.max(m.setup.humans);
                        m.screen = Screen::NewGame;
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Your name");
                    if ui.text_edit_singleline(&mut settings.name).lost_focus() {
                        let _ = settings.save();
                    }
                });
            });
        }
        Screen::Settings => {
            card("Settings").show(&ctx, |ui| {
                let before = settings.clone();
                egui::Grid::new("settings").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut settings.name);
                    ui.end_row();
                    ui.label("Volume");
                    ui.horizontal(|ui| {
                        ui.add(egui::Slider::new(&mut settings.volume, 0.0..=1.0).show_value(false));
                        ui.checkbox(&mut settings.muted, "Mute");
                    });
                    ui.end_row();
                    ui.label("Mouse sensitivity");
                    ui.horizontal(|ui| {
                        ui.add(egui::Slider::new(&mut settings.mouse_sensitivity, 0.2..=3.0));
                        ui.checkbox(&mut settings.invert_y, "Invert Y");
                    });
                    ui.end_row();
                    ui.label("Shadows");
                    ui.checkbox(&mut settings.shadows, "Sun casts shadows");
                    ui.end_row();
                    ui.label("Day and night");
                    egui::ComboBox::from_id_salt("day").selected_text(settings.day_cycle.label()).show_ui(ui, |ui| {
                        for d in DayCycle::ALL {
                            ui.selectable_value(&mut settings.day_cycle, d, d.label());
                        }
                    });
                    ui.end_row();
                    ui.label("Cars & people");
                    ui.horizontal(|ui| {
                        for c in Crowds::ALL {
                            ui.selectable_value(&mut settings.crowds, c, c.label());
                        }
                    });
                    ui.end_row();
                    ui.label("Interface size");
                    ui.add(egui::Slider::new(&mut settings.ui_scale, 0.75..=1.75).step_by(0.05));
                    ui.end_row();
                    ui.label("Autosave");
                    ui.checkbox(&mut settings.autosave, "Every game year");
                    ui.end_row();
                });
                if *settings != before {
                    if let Err(e) = settings.save() {
                        m.error = Some(format!("Couldn't save settings: {e}"));
                    }
                }
            });
        }
    }

    match go {
        Some(Ok((d, s))) => {
            m.error = None;
            m.screen = Screen::Title;
            commands.insert_resource(d);
            commands.insert_resource(Toasts::default());
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

fn lobby_ui(mut contexts: EguiContexts, mut driver: ResMut<Driver>, settings: Res<Settings>, mut next: ResMut<NextState<AppState>>) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    theme(&ctx, &settings);
    egui::Window::new("Lobby").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(&ctx, |ui| {
        ui.set_width(420.0);
        match &mut driver.mode {
            Mode::Host(h) => {
                let port = h.local_addr().map(|a| a.port()).unwrap_or(DEFAULT_PORT);
                ui.label("Tell your friends to choose Play with a friend → Join and enter:");
                let addr = match lan_ip() {
                    Some(ip) => format!("{ip}:{port}"),
                    None => format!("<your IP>:{port}"),
                };
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&addr).monospace().size(22.0).strong());
                    if ui.button("📋 Copy").clicked() {
                        ui.ctx().copy_text(addr.clone());
                    }
                });
                ui.small("Over the internet, forward this UDP port on your router and share your public IP.");
                ui.separator();
                ui.label(format!("{} parcels; empty seats are played by the AI and can be taken over later.", h.slots()));
                for (i, e) in h.lobby().iter().enumerate() {
                    ui.horizontal(|ui| {
                        crate::ui::swatch(ui, color32(parcels_sim::PlayerId(i as u8)));
                        ui.label(format!("{}{}", e.name, if e.is_host { " (host)" } else { "" }));
                    });
                }
                ui.separator();
                if ui.add_sized([160.0, 34.0], egui::Button::new(egui::RichText::new("Start game").strong())).clicked() {
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
                    for (i, e) in c.lobby.iter().enumerate() {
                        ui.horizontal(|ui| {
                            crate::ui::swatch(ui, color32(parcels_sim::PlayerId(i as u8)));
                            ui.label(format!("{}{}", e.name, if e.is_host { " (host)" } else { "" }));
                        });
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
