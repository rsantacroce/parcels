//! In-game HUD: top bar (date, speed, money, demand), bottom build toolbar,
//! city panel with inspector, minimap, toasts, and the pause menu with save
//! and load.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use parcels_sim::systems::growth::growth_block;
use parcels_sim::{fmt_money, Buildable, Category, CommandKind, GameState, Service, Terrain, TileKind, Zone};

use crate::audio::{Sfx, SfxQueue};
use crate::camera::{CamMode, MainCamera, Orbit, StreetRequest};
use crate::driver::{Driver, TickFeed};
use crate::menu::{load_list, theme};
use crate::saves;
use crate::settings::Settings;
use crate::tools::{hotkey, items, Tool, ToolState};
use crate::view::{map_color, Overlay, View};
use crate::world::meshgen::TILE;
use crate::world::scene::Sky;
use crate::AppState;

#[derive(Default)]
pub struct Toast {
    pub text: String,
    pub warn: bool,
    pub age: f32,
}

#[derive(Resource, Default)]
pub struct Toasts(pub Vec<Toast>);

impl Toasts {
    pub fn info(&mut self, s: impl Into<String>) {
        self.0.push(Toast { text: s.into(), warn: false, age: 0.0 });
    }
    pub fn warn(&mut self, s: impl Into<String>) {
        self.0.push(Toast { text: s.into(), warn: true, age: 0.0 });
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Dialog {
    #[default]
    None,
    Pause,
    Save,
    Load,
    Help,
}

#[derive(Resource)]
pub struct Hud {
    dialog: Dialog,
    panel: bool,
    category: Option<Category>,
    save_name: String,
    minimap: Option<egui::TextureHandle>,
    minimap_key: Option<(u64, u64, Overlay)>,
    confirm_delete: Option<String>,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            dialog: Dialog::None,
            panel: true,
            category: None,
            save_name: String::new(),
            minimap: None,
            minimap_key: None,
            confirm_delete: None,
        }
    }
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Toasts>()
            .init_resource::<Hud>()
            .add_systems(OnEnter(AppState::Playing), |mut hud: ResMut<Hud>| {
                hud.dialog = Dialog::None;
                hud.minimap_key = None;
            })
            .add_systems(Update, (digest_feed, keys).run_if(in_state(AppState::Playing)))
            .add_systems(EguiPrimaryContextPass, hud.run_if(in_state(AppState::Playing)));
    }
}

pub fn quick_save(driver: &mut Driver, toasts: &mut Toasts) {
    let Some(state) = driver.state() else { return };
    match saves::save(state, "Quicksave") {
        Ok(_) => toasts.info("Quick-saved"),
        Err(e) => toasts.warn(format!("Save failed: {e}")),
    }
}

pub fn load_slug(driver: &mut Driver, slug: &str, toasts: &mut Toasts) -> bool {
    match saves::load(slug) {
        Ok(state) => {
            driver.replace_state(state);
            driver.save_name = saves::list().into_iter().find(|e| e.slug == slug).map(|e| e.title());
            toasts.info(format!("Loaded {}", driver.save_name.clone().unwrap_or_else(|| slug.into())));
            true
        }
        Err(e) => {
            toasts.warn(e);
            false
        }
    }
}

fn export_replay(driver: &Driver, toasts: &mut Toasts) {
    let replay = driver.replay();
    let _ = std::fs::create_dir_all(saves::SAVE_DIR);
    let path = format!("{}/replay.ron", saves::SAVE_DIR);
    match ron::ser::to_string(replay).map_err(|e| e.to_string()).and_then(|t| std::fs::write(&path, t).map_err(|e| e.to_string())) {
        Ok(()) => toasts.info(format!("Replay ({} commands) written to {path}", replay.commands.len())),
        Err(e) => toasts.warn(format!("Replay export failed: {e}")),
    }
}

#[allow(clippy::too_many_arguments)]
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    egui: Res<bevy_egui::input::EguiWantsInput>,
    mut driver: ResMut<Driver>,
    mut toasts: ResMut<Toasts>,
    mut settings: ResMut<Settings>,
    mut hud: ResMut<Hud>,
) {
    if egui.wants_any_keyboard_input() {
        return;
    }
    if keys.just_pressed(KeyCode::Space) {
        driver.paused = !driver.paused;
    }
    if keys.just_pressed(KeyCode::Equal) {
        driver.speed = (driver.speed * 2).min(32);
    }
    if keys.just_pressed(KeyCode::Minus) {
        driver.speed = (driver.speed / 2).max(1);
    }
    if keys.just_pressed(KeyCode::F5) {
        quick_save(&mut driver, &mut toasts);
    }
    if keys.just_pressed(KeyCode::F9) {
        load_slug(&mut driver, saves::QUICKSAVE, &mut toasts);
    }
    if keys.just_pressed(KeyCode::KeyM) {
        settings.muted = !settings.muted;
        toasts.info(if settings.muted { "Sound off" } else { "Sound on" });
    }
    if keys.just_pressed(KeyCode::F1) || keys.just_pressed(KeyCode::KeyH) {
        hud.dialog = if hud.dialog == Dialog::Help { Dialog::None } else { Dialog::Help };
    }
    if keys.just_pressed(KeyCode::F10) {
        hud.dialog = if hud.dialog == Dialog::Pause { Dialog::None } else { Dialog::Pause };
    }
    if keys.just_pressed(KeyCode::Escape) && hud.dialog != Dialog::None {
        hud.dialog = Dialog::None;
    }
    if keys.just_pressed(KeyCode::Backquote) {
        hud.panel = !hud.panel;
    }
}

/// Turn tick reports into toasts, sounds and autosaves.
#[allow(clippy::too_many_arguments)]
fn digest_feed(
    mut feed: ResMut<TickFeed>,
    driver: Res<Driver>,
    settings: Res<Settings>,
    mut toasts: ResMut<Toasts>,
    mut sfx: ResMut<SfxQueue>,
    time: Res<Time<Real>>,
    mut last_pop: Local<Option<u32>>,
) {
    let reports = std::mem::take(&mut feed.reports);
    let Some(state) = driver.state() else { return };
    let mut autosave = false;
    for r in reports {
        for (_, why) in &r.rejected {
            toasts.warn(why.to_string());
            sfx.push(Sfx::Error);
        }
        if let Some(p) = r.fires.first() {
            toasts.warn(format!("🔥 Fire at ({}, {})! A fire station nearby would have stopped it.", p.x, p.y));
            sfx.push(Sfx::Fire);
        }
        if r.month_ended {
            let s = &state.city.stats;
            if s.taxes - s.upkeep > 0 {
                sfx.push(Sfx::Cash);
            }
            // Celebrate round numbers, but not the ones a loaded save already passed.
            let pop = s.population;
            let before = last_pop.replace(pop).unwrap_or(pop);
            if let Some(&m) = MILESTONES.iter().rev().find(|&&m| before < m && pop >= m) {
                if !driver.showcase {
                    toasts.info(format!("🎉 {} now has {m} residents!", state.city.name));
                    sfx.push(Sfx::Milestone);
                }
            }
            if state.tick % (state.config.ticks_per_month as u64 * 12) == 0 {
                autosave = true;
            }
        }
    }
    if autosave && settings.autosave && !driver.showcase {
        // Write on a worker thread so a big map doesn't hitch the frame.
        let snapshot = state.clone();
        std::thread::spawn(move || {
            let _ = saves::save(&snapshot, "Autosave");
        });
        toasts.info("Autosaved");
    }
    let dt = time.delta_secs();
    for t in &mut toasts.0 {
        t.age += dt;
    }
    toasts.0.retain(|t| t.age < 6.0);
    if toasts.0.len() > 6 {
        let n = toasts.0.len() - 6;
        toasts.0.drain(..n);
    }
}

const MILESTONES: [u32; 9] = [100, 500, 1_000, 2_500, 5_000, 10_000, 25_000, 50_000, 100_000];

const ZONE_COLORS: [egui::Color32; 4] = [
    egui::Color32::from_rgb(120, 210, 110),
    egui::Color32::from_rgb(110, 160, 240),
    egui::Color32::from_rgb(230, 190, 80),
    egui::Color32::from_rgb(170, 140, 240),
];

fn demand_bar(ui: &mut egui::Ui, label: &str, v: i32, color: egui::Color32) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 26.0), egui::Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 2.0, egui::Color32::from_gray(35));
        let mid = rect.center().y;
        let h = (v as f32 / 1000.0).clamp(-1.0, 1.0) * rect.height() / 2.0;
        let bar = if h >= 0.0 {
            egui::Rect::from_min_max(egui::pos2(rect.left(), mid - h), egui::pos2(rect.right(), mid))
        } else {
            egui::Rect::from_min_max(egui::pos2(rect.left(), mid), egui::pos2(rect.right(), mid - h))
        };
        p.rect_filled(bar, 2.0, if v >= 0 { color } else { egui::Color32::from_rgb(200, 60, 50) });
        p.line_segment([egui::pos2(rect.left(), mid), egui::pos2(rect.right(), mid)], (1.0, egui::Color32::GRAY));
        ui.label(egui::RichText::new(label).small().strong().color(color));
    })
    .response
    .on_hover_text(format!("{label} demand: {}", v / 10));
}

fn category_icon(c: Category) -> &'static str {
    match c {
        Category::Transport => "🚗",
        Category::Zones => "🏠",
        Category::Power => "⚡",
        Category::Water => "💧",
        Category::Services => "🚒",
        Category::Parks => "🌳",
        Category::Landmarks => "⭐",
    }
}

fn item_tooltip(state: &GameState, b: Buildable) -> String {
    let c = &state.config;
    let per_mo = |v: i64| fmt_money(v * c.ticks_per_month as i64);
    match b {
        Buildable::Building(bb) => {
            let s = c.building(bb);
            let mut lines = vec![bb.blurb().to_string(), format!("{}×{} tiles · upkeep {}/mo", bb.size(), bb.size(), per_mo(s.upkeep))];
            if s.power_supply > 0 {
                lines.push(format!("Power: {} units", s.power_supply));
            }
            if s.water_supply > 0 {
                lines.push(format!("Water: {} units (+{} by the river)", s.water_supply, s.river_bonus));
            }
            if s.power_draw > 0 {
                lines.push(format!("Needs {} power", s.power_draw));
            }
            if let Some(svc) = bb.service() {
                lines.push(format!("{} coverage, radius {}", svc.name(), s.service_radius));
            }
            if s.land_value_bonus > 0 {
                lines.push(format!("Land value +{} within {}", s.land_value_bonus, s.land_value_radius));
            }
            if s.pollution > 0 {
                lines.push(format!("Pollution {}", s.pollution));
            }
            if s.jobs > 0 {
                lines.push(format!("{} jobs", s.jobs));
            }
            lines.join("\n")
        }
        Buildable::Zone(z, d) => {
            let cap = c.max_level(d);
            let need = match z {
                Zone::Residential => "Homes. Past level 2 they need a school nearby.",
                Zone::Commercial => "Shops. Like busy neighbours and people nearby.",
                Zone::Industrial => "Factories: jobs and pollution. Keep away from homes.",
                Zone::Office => "Clean jobs. Need a school to grow past level 1.",
            };
            format!("{need}\nGrows to level {cap}.{}", if d == parcels_sim::Density::High { " Towers need land value and a hospital." } else { "" })
        }
        Buildable::Road(r) => match r {
            parcels_sim::Road::Street => "Two-lane street. Zones within two tiles of a road can grow.".into(),
            parcels_sim::Road::Avenue => "Four lanes with a median: about half the congestion. Drag over streets to upgrade.".into(),
        },
        Buildable::PowerLine => "Carries power between plants and zones. Can run over roads and water.".into(),
        Buildable::WaterPipe => "Underground water main. Zones pass water on to their neighbours.".into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn hud(
    mut contexts: EguiContexts,
    mut driver: ResMut<Driver>,
    mut view: ResMut<View>,
    mut ts: ResMut<ToolState>,
    mut toasts: ResMut<Toasts>,
    mut next: ResMut<NextState<AppState>>,
    mut settings: ResMut<Settings>,
    mut hud: ResMut<Hud>,
    mut orbit: ResMut<Orbit>,
    sky: Res<Sky>,
    mode: Single<&CamMode, With<MainCamera>>,
    mut street: MessageWriter<StreetRequest>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    theme(&ctx, &settings);
    let Some(state) = driver.state().cloned() else {
        return Ok(());
    };
    let in_street = matches!(**mode, CamMode::Street { .. });
    let mut actions: Vec<CommandKind> = Vec::new();

    let mut root = egui::Ui::new(
        ctx.clone(),
        "hud".into(),
        egui::UiBuilder::new().layer_id(egui::LayerId::background()).max_rect(ctx.viewport_rect()),
    );

    // ---------------- Top bar ----------------
    egui::Panel::top("top").show(&mut root, |ui| {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if ui.button("☰").on_hover_text("Menu (F10)").clicked() {
                hud.dialog = Dialog::Pause;
            }
            let (y, m, d) = state.date();
            let hour = sky.hour as u32;
            ui.label(egui::RichText::new(format!("Y{y} · M{m:02} · D{d:02}")).monospace().strong())
                .on_hover_text(format!("Tick {} · time of day {:02}:{:02}", state.tick, hour, ((sky.hour.fract()) * 60.0) as u32));
            ui.separator();
            let label = if driver.paused { "▶" } else { "⏸" };
            if ui.button(label).on_hover_text("Pause (Space)").clicked() {
                driver.paused = !driver.paused;
            }
            for (name, s) in [("1×", 4), ("2×", 8), ("4×", 16), ("8×", 32)] {
                if ui.selectable_label(driver.speed == s && !driver.paused, name).clicked() {
                    driver.speed = s;
                    driver.paused = false;
                }
            }
            ui.separator();
            let city = &state.city;
            let s = &city.stats;
            let tpm = state.config.ticks_per_month as i64;
            let net = (s.taxes - s.upkeep) * tpm;
            ui.label(egui::RichText::new(&city.name).strong());
            ui.label(egui::RichText::new(fmt_money(city.treasury)).monospace().strong().size(16.0));
            let c = if net >= 0 { egui::Color32::LIGHT_GREEN } else { egui::Color32::LIGHT_RED };
            ui.colored_label(c, format!("{}{}/mo", if net >= 0 { "+" } else { "" }, fmt_money(net)))
                .on_hover_text(format!("Per month: taxes {}  upkeep -{}", fmt_money(s.taxes * tpm), fmt_money(s.upkeep * tpm)));
            ui.separator();
            ui.label(format!("👥 {}", s.population)).on_hover_text("Residents");
            for z in Zone::ALL {
                demand_bar(ui, z.letter(), s.demand[z.index()], ZONE_COLORS[z.index()]);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(if settings.muted { "🔇" } else { "🔊" }).on_hover_text("Sound (M)").clicked() {
                    settings.muted = !settings.muted;
                }
                if ui.selectable_label(hud.panel, "📊").on_hover_text("City panel (`)").clicked() {
                    hud.panel = !hud.panel;
                }
                if ui.selectable_label(in_street, "🚶 Street").on_hover_text("Walk the streets (V)").clicked() {
                    street.write(StreetRequest(ts.selected.or(ts.hover)));
                }
                egui::ComboBox::from_id_salt("overlay").selected_text(format!("👁 {}", view.overlay.label())).show_ui(ui, |ui| {
                    for o in Overlay::ALL {
                        ui.selectable_value(&mut view.overlay, o, format!("{}  {}", o.key(), o.label()));
                    }
                });
            });
        });
        ui.add_space(2.0);
    });

    // ---------------- Bottom toolbar ----------------
    if !in_street || ts.tool != Tool::Inspect {
        egui::Panel::bottom("toolbar").show(&mut root, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let big = |t: &str| egui::RichText::new(t).size(15.0);
                if ui.selectable_label(ts.tool == Tool::Inspect, big("🔍 Inspect")).on_hover_text("Esc").clicked() {
                    ts.set(Tool::Inspect);
                    hud.category = None;
                }
                for c in Category::ALL {
                    let on = hud.category == Some(c);
                    if ui.selectable_label(on, big(&format!("{} {}", category_icon(c), c.name()))).clicked() {
                        hud.category = if on { None } else { Some(c) };
                        if let Some(c) = hud.category {
                            let pick = ts.last_in(c).unwrap_or(items(c)[0]);
                            ts.set(Tool::Build(pick));
                        }
                    }
                }
                if ui.selectable_label(ts.tool == Tool::Bulldoze, big("🔨 Bulldoze")).on_hover_text("B").clicked() {
                    ts.set(Tool::Bulldoze);
                    hud.category = None;
                }
            });
            // Keep the open category in step with hotkeys.
            if let Tool::Build(b) = ts.tool {
                hud.category = Some(b.category());
            }
            if let Some(c) = hud.category {
                ui.add_space(2.0);
                ui.horizontal_wrapped(|ui| {
                    for b in items(c) {
                        let cost = state.config.cost(b);
                        let per = if matches!(b, Buildable::Building(_)) { "" } else { "/tile" };
                        let key = hotkey(Tool::Build(b)).map(|k| format!(" [{k}]")).unwrap_or_default();
                        let text = format!("{}  {}{per}", b.name(), fmt_money(cost).trim_end_matches(".00"));
                        let resp = ui.selectable_label(ts.tool == Tool::Build(b), text).on_hover_text(format!("{}{key}", item_tooltip(&state, b)));
                        if resp.clicked() {
                            ts.set(Tool::Build(b));
                        }
                    }
                });
            }
            ui.horizontal(|ui| {
                let hint = match ts.tool {
                    Tool::Build(Buildable::Road(_) | Buildable::PowerLine | Buildable::WaterPipe) => "Drag to lay a line.",
                    Tool::Build(Buildable::Building(b)) if b.size() > 1 => "Click to place. The footprint centres on the cursor.",
                    Tool::Build(Buildable::Building(_)) => "Click to place, or drag to place several.",
                    Tool::Inspect => "Click a tile to inspect it. Right-drag pans, middle-drag rotates, wheel zooms.",
                    _ => "Drag to paint an area.",
                };
                ui.label(egui::RichText::new(hint).small().weak());
                ui.label(egui::RichText::new("Right-click cancels · H help").small().weak());
            });
            ui.add_space(2.0);
        });
    }

    // ---------------- Right panel ----------------
    if hud.panel && !in_street {
        egui::Panel::right("info").exact_size(300.0).show(&mut root, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                city_panel(ui, &state, &mut actions);
                ui.add_space(6.0);
                inspector(ui, &state, ts.selected.or(ts.hover));
            });
        });
    }

    // ---------------- Minimap ----------------
    if !in_street {
        minimap(&ctx, &state, &mut hud, &view, &mut orbit, driver.generation);
    }

    // ---------------- Street view overlay ----------------
    if in_street {
        let center = ctx.viewport_rect().center();
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, "crosshair".into()));
        painter.circle_stroke(center, 5.0, (1.5, egui::Color32::from_white_alpha(200)));
        egui::Area::new("street_hint".into()).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -16.0)).interactable(false).show(&ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let fly = matches!(**mode, CamMode::Street { fly: true });
                ui.label(format!(
                    "{} · WASD move · Shift run · F {} · B/Z/R… tools work at the crosshair · V or Esc: back to the map",
                    if fly { "Flying" } else { "Walking" },
                    if fly { "walk" } else { "fly" }
                ));
                if let Some(h) = ts.hover {
                    let t = state.map.tile(h);
                    ui.label(egui::RichText::new(format!("Looking at {} ({}, {})", t.kind.name(), h.x, h.y)).weak());
                }
            });
        });
    }

    // ---------------- Ghost preview tooltip ----------------
    if let Some(p) = &ts.preview {
        let at = if in_street { Some(ctx.viewport_rect().center() + egui::vec2(16.0, 16.0)) } else { ctx.pointer_hover_pos().map(|p| p + egui::vec2(18.0, 14.0)) };
        if let Some(at) = at {
            egui::Area::new("preview".into()).fixed_pos(at).interactable(false).order(egui::Order::Tooltip).show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| match &p.result {
                    Ok(plan) => {
                        let n = if plan.footprints.is_empty() { plan.tiles.len() } else { plan.footprints.len() };
                        ui.label(format!("{} × {}: {}", n, ts.tool.label(), fmt_money(plan.cost)));
                        if p.clipped {
                            ui.small("Clipped to the map");
                        }
                    }
                    Err(why) => {
                        ui.colored_label(egui::Color32::LIGHT_RED, why);
                    }
                });
            });
        }
    }

    // ---------------- Toasts ----------------
    egui::Area::new("toasts".into()).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 52.0)).interactable(false).show(&ctx, |ui| {
        for t in &toasts.0 {
            let c = if t.warn { egui::Color32::from_rgb(255, 150, 130) } else { egui::Color32::from_rgb(220, 232, 255) };
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.colored_label(c, &t.text);
            });
        }
    });

    if driver.paused && hud.dialog == Dialog::None {
        egui::Area::new("paused".into()).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 52.0)).interactable(false).show(&ctx, |ui| {
            ui.label(egui::RichText::new("PAUSED").size(22.0).strong().color(egui::Color32::YELLOW));
        });
    }

    dialogs(&ctx, &state, &mut driver, &mut hud, &mut toasts, &mut next);

    for kind in actions {
        driver.submit(kind);
    }
    Ok(())
}

fn dialogs(ctx: &egui::Context, state: &GameState, driver: &mut Driver, hud: &mut Hud, toasts: &mut Toasts, next: &mut NextState<AppState>) {
    let center = |title: &str| egui::Window::new(title.to_string()).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO);
    match hud.dialog {
        Dialog::None => {}
        Dialog::Pause => {
            center("Paused").show(ctx, |ui| {
                ui.set_width(240.0);
                let wide = |ui: &mut egui::Ui, t: &str| ui.add_sized([240.0, 30.0], egui::Button::new(t)).clicked();
                if wide(ui, "Resume") {
                    hud.dialog = Dialog::None;
                }
                if wide(ui, "Save game…") {
                    hud.save_name = driver.save_name.clone().unwrap_or_else(|| default_save_name(state));
                    hud.dialog = Dialog::Save;
                }
                if wide(ui, "Load game…") {
                    hud.dialog = Dialog::Load;
                }
                if wide(ui, "Export replay") {
                    export_replay(driver, toasts);
                }
                if wide(ui, "Controls") {
                    hud.dialog = Dialog::Help;
                }
                ui.separator();
                if wide(ui, "Quit to title") {
                    hud.dialog = Dialog::None;
                    next.set(AppState::Menu);
                }
            });
        }
        Dialog::Save => {
            center("Save game").show(ctx, |ui| {
                ui.set_width(360.0);
                ui.horizontal(|ui| {
                    ui.label("Name");
                    let r = ui.add(egui::TextEdit::singleline(&mut hud.save_name).desired_width(240.0));
                    r.request_focus();
                });
                let slug = saves::slug(&hud.save_name);
                if saves::exists(&slug) {
                    ui.colored_label(egui::Color32::from_rgb(240, 200, 120), "A save with this name exists and will be replaced.");
                }
                ui.horizontal(|ui| {
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.button("💾 Save").clicked() || enter {
                        match saves::save(state, &hud.save_name) {
                            Ok(_) => {
                                toasts.info(format!("Saved \"{}\"", hud.save_name));
                                driver.save_name = Some(hud.save_name.clone());
                                hud.dialog = Dialog::None;
                            }
                            Err(e) => toasts.warn(format!("Save failed: {e}")),
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        hud.dialog = Dialog::Pause;
                    }
                });
            });
        }
        Dialog::Load => {
            center("Load game").show(ctx, |ui| {
                ui.set_width(520.0);
                let mut chosen = None;
                load_list(ui, &mut hud.confirm_delete, |slug| chosen = Some(slug.to_string()));
                if let Some(slug) = chosen {
                    if load_slug(driver, &slug, toasts) {
                        hud.dialog = Dialog::None;
                    }
                }
                if ui.button("Cancel").clicked() {
                    hud.dialog = Dialog::Pause;
                }
            });
        }
        Dialog::Help => {
            center("Controls").show(ctx, |ui| {
                ui.set_width(440.0);
                egui::Grid::new("help").num_columns(2).striped(true).show(ui, |ui| {
                    for (k, v) in [
                        ("WASD / arrows / edges", "Pan the map"),
                        ("Right-drag", "Pan"),
                        ("Middle-drag, Q / E", "Rotate · tilt"),
                        ("Wheel", "Zoom toward the cursor"),
                        ("V", "Street view: walk the city (F to fly)"),
                        ("R", "Street, press again for avenue"),
                        ("Z X C O", "Residential, commercial, industrial, office; again for dense"),
                        ("L / P", "Power line / water pipe"),
                        ("G U J K N", "Cycle power, water, services, parks, landmarks"),
                        ("B / Esc", "Bulldoze / inspect"),
                        ("1 – 9", "Overlays (press again to clear)"),
                        ("Space, + / −", "Pause, speed"),
                        ("F5 / F9", "Quick-save / quick-load"),
                        ("`", "Toggle the side panel"),
                        ("F10", "Menu"),
                    ] {
                        ui.label(egui::RichText::new(k).monospace().strong());
                        ui.label(v);
                        ui.end_row();
                    }
                });
                if ui.button("Close").clicked() {
                    hud.dialog = Dialog::None;
                }
            });
        }
    }
}

pub fn default_save_name(state: &GameState) -> String {
    let (y, m, _) = state.date();
    format!("{} year {y} month {m}", state.city.name)
}

fn city_panel(ui: &mut egui::Ui, state: &GameState, actions: &mut Vec<CommandKind>) {
    let tpm = state.config.ticks_per_month as i64;
    let city = &state.city;
    ui.heading(&city.name);
    let s = &city.stats;
    egui::Grid::new("stats").num_columns(2).striped(true).show(ui, |ui| {
        for (k, v) in [
            ("Population", s.population.to_string()),
            ("Jobs: shops / industry", format!("{} / {}", s.commercial_jobs, s.industrial_jobs)),
            ("Jobs: offices / public", format!("{} / {}", s.office_jobs, s.public_jobs)),
            ("Taxes / mo", fmt_money(s.taxes * tpm)),
            ("Upkeep / mo", fmt_money(-s.upkeep * tpm)),
            ("Buildings lost to fire", s.fires.to_string()),
        ] {
            ui.label(k);
            ui.label(v);
            ui.end_row();
        }
    });
    let mut tax = city.tax_rate;
    ui.horizontal(|ui| {
        ui.label("Tax rate");
        if ui.add(egui::Slider::new(&mut tax, 0..=state.config.max_tax_rate).suffix("%")).changed() {
            actions.push(CommandKind::SetTaxRate { rate: tax });
        }
    });
    egui::CollapsingHeader::new("Power & water").default_open(true).show(ui, |ui| {
        for (name, l) in [("⚡ Power", &s.power), ("💧 Water", &s.water)] {
            ui.label(egui::RichText::new(name).strong());
            ui.colored_label(
                if l.unserved > 0 { egui::Color32::LIGHT_RED } else { egui::Color32::GRAY },
                format!("supply {}  demand {}  unserved {}", l.supply, l.demand, l.unserved),
            );
        }
    });
}

fn inspector(ui: &mut egui::Ui, state: &GameState, at: Option<parcels_sim::Pos>) {
    egui::CollapsingHeader::new("Tile").default_open(true).show(ui, |ui| {
        let Some(pos) = at else {
            ui.weak("Hover or click a tile");
            return;
        };
        let pos = if state.map.tile(pos).kind.building().is_some() { state.map.footprint_at(pos).min } else { pos };
        let t = state.map.tile(pos);
        let idx = state.map.idx(pos);
        egui::Grid::new("tile").num_columns(2).show(ui, |ui| {
            ui.label("Where");
            ui.label(format!("({}, {})", pos.x, pos.y));
            ui.end_row();
            ui.label("What");
            let mut kind = match (t.kind, t.terrain) {
                (TileKind::Empty, Terrain::Water) => "River".to_string(),
                (TileKind::Empty, Terrain::Forest) => "Woodland".to_string(),
                (TileKind::Road(_), Terrain::Water) => format!("{} bridge", t.kind.name()),
                (k, _) => k.name(),
            };
            if t.kind.is_zone() {
                kind.push_str(&format!(", level {}", t.level));
            }
            if t.wire {
                kind.push_str(" + power line");
            }
            if t.pipe {
                kind.push_str(" + pipe");
            }
            ui.label(kind);
            ui.end_row();
            if let Some(z) = t.kind.zone() {
                ui.label(if z == Zone::Residential { "Residents" } else { "Jobs" });
                ui.label(state.occupants(idx).to_string());
                ui.end_row();
                ui.label("Growth");
                match growth_block(&state.config, t) {
                    Some(why) => ui.colored_label(egui::Color32::from_rgb(240, 200, 120), why),
                    None if !t.powered => ui.colored_label(egui::Color32::LIGHT_RED, "no power"),
                    None => ui.colored_label(egui::Color32::LIGHT_GREEN, "can grow when there's demand"),
                };
                ui.end_row();
            }
            if let TileKind::Building(b) = t.kind {
                let s = state.config.building(b);
                if s.power_draw > 0 {
                    ui.label("Running");
                    if t.powered {
                        ui.colored_label(egui::Color32::LIGHT_GREEN, "yes");
                    } else {
                        ui.colored_label(egui::Color32::LIGHT_RED, "no power");
                    }
                    ui.end_row();
                }
            }
            ui.label("Power / water");
            ui.label(format!("{} / {}", if t.powered { "✔" } else { "✘" }, if t.watered { "✔" } else { "✘" }));
            ui.end_row();
            ui.label("Services");
            let cov: Vec<&str> = Service::ALL.iter().filter(|s| t.covered(**s)).map(|s| s.name()).collect();
            ui.label(if cov.is_empty() { "none".to_string() } else { cov.join(", ") });
            ui.end_row();
            for (k, v) in [("Land value", t.land_value), ("Pollution", t.pollution), ("Traffic", t.traffic), ("Crime", t.crime)] {
                ui.label(k);
                ui.label(v.to_string());
                ui.end_row();
            }
        });
    });
}

fn minimap(ctx: &egui::Context, state: &GameState, hud: &mut Hud, view: &View, orbit: &mut Orbit, generation: u64) {
    let key = (state.tick / 10, generation, view.overlay);
    if hud.minimap_key != Some(key) || hud.minimap.is_none() {
        hud.minimap_key = Some(key);
        let (w, h) = (state.map.width as usize, state.map.height as usize);
        let mut rgba = Vec::with_capacity(w * h * 4);
        for i in 0..w * h {
            let c = map_color(state, view.overlay, i);
            rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
        let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
        match &mut hud.minimap {
            Some(tex) => tex.set(img, egui::TextureOptions::NEAREST),
            None => hud.minimap = Some(ctx.load_texture("minimap", img, egui::TextureOptions::NEAREST)),
        }
    }
    let Some(tex) = &hud.minimap else { return };
    let (w, h) = (state.map.width as f32, state.map.height as f32);
    let scale = (200.0 / w.max(h)).min(4.0);
    let size = egui::vec2(w * scale, h * scale);
    egui::Area::new("minimap".into()).anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(10.0, -96.0)).show(ctx, |ui| {
        egui::Frame::popup(ui.style()).inner_margin(4.0).show(ui, |ui| {
            let resp = ui.add(egui::Image::new((tex.id(), size)).sense(egui::Sense::click_and_drag()));
            let r = resp.rect;
            let to_screen = |v: Vec3| r.min + egui::vec2(v.x / TILE * scale, v.z / TILE * scale);
            let p = ui.painter();
            // Where the camera is and which way it looks.
            let f = to_screen(orbit.focus);
            let eye = orbit.eye();
            let e = to_screen(eye);
            p.line_segment([e, f], (1.5, egui::Color32::WHITE));
            p.circle_filled(f, 3.0, egui::Color32::WHITE);
            if (resp.clicked() || resp.dragged()) && resp.interact_pointer_pos().is_some() {
                let at = resp.interact_pointer_pos().unwrap() - r.min;
                orbit.focus = Vec3::new(at.x / scale, 0.0, at.y / scale) * TILE;
            }
        });
    });
}
