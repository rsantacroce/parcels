//! In-game HUD in the paper style: a tool rail on the left with flyouts, a
//! rail of cards on the right (inbox, city mood, budget, mayor), a dashboard along the bottom
//! (minimap, date, speed, approval, money, population, RCIO), hover tooltips,
//! toasts, and the pause menu with save and load.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use parcels_sim::systems::growth::growth_block;
use parcels_sim::mayor::area_name;
use parcels_sim::{fmt_money, mood_word, Buildable, Category, CommandKind, Election, Factor, GameState, Letter, LetterState, Pos, Service, Terrain, TileKind, Zone};

use crate::audio::{Sfx, SfxQueue};
use crate::camera::{CamMode, MainCamera, Orbit, StreetRequest};
use crate::driver::{Driver, TickFeed};
use crate::icons::{self, pal, Icon};
use crate::menu::{load_list, theme};
use crate::saves;
use crate::settings::Settings;
use crate::tools::{hotkey, items, Tool, ToolState};
use crate::view::{map_color, Overlay, View};
use crate::world::meshgen::{tile_center, TILE};
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
    Election(Election),
}

#[derive(Resource)]
pub struct Hud {
    dialog: Dialog,
    flyout: Option<Flyout>,
    side: Option<Side>,
    save_name: String,
    minimap: Option<egui::TextureHandle>,
    minimap_key: Option<(u64, u64, Overlay)>,
    confirm_delete: Option<String>,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            dialog: Dialog::None,
            flyout: None,
            side: None,
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
            .add_systems(OnEnter(AppState::Playing), enter_playing)
            .add_systems(Update, (digest_feed, keys).run_if(in_state(AppState::Playing)))
            .add_systems(EguiPrimaryContextPass, hud.run_if(in_state(AppState::Playing)));
    }
}

/// `--hud city|budget|inbox|mayor|zones|views|paused`: open part of the HUD on start, for screenshots.
#[derive(Resource)]
pub struct DevHud(pub String);

fn enter_playing(mut hud: ResMut<Hud>, dev: Option<Res<DevHud>>, mut driver: Option<ResMut<Driver>>, mut ts: ResMut<ToolState>) {
    hud.dialog = Dialog::None;
    hud.minimap_key = None;
    for part in dev.iter().flat_map(|d| d.0.split(',')) {
        match part {
            "city" => hud.side = Some(Side::City),
            "budget" => hud.side = Some(Side::Budget),
            "inbox" => hud.side = Some(Side::Inbox),
            "mayor" => hud.side = Some(Side::Mayor),
            "zones" => ts.set(Tool::Build(items(Category::Zones)[0])),
            "views" => hud.flyout = Some(Flyout::DataViews),
            "paused" => {
                if let Some(d) = driver.as_mut() {
                    d.paused = true;
                }
            }
            _ => {}
        }
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
        hud.side = if hud.side == Some(Side::City) { None } else { Some(Side::City) };
    }
}

/// Turn tick reports into toasts, sounds and autosaves.
#[allow(clippy::too_many_arguments)]
fn digest_feed(
    mut feed: ResMut<TickFeed>,
    mut driver: ResMut<Driver>,
    mut hud: ResMut<Hud>,
    settings: Res<Settings>,
    mut toasts: ResMut<Toasts>,
    mut sfx: ResMut<SfxQueue>,
    time: Res<Time<Real>>,
    mut last_pop: Local<Option<u32>>,
) {
    let reports = std::mem::take(&mut feed.reports);
    let Some(state) = driver.state() else { return };
    let mut autosave = false;
    let mut election = None;
    for r in reports {
        if !driver.showcase {
            let letter = |id: &u32| state.city.mayor.letter(*id);
            for l in r.letters_new.iter().filter_map(letter) {
                toasts.info(format!("✉ New letter from {} in {}", l.from, area_name(&state.map, l.at)));
                sfx.push(Sfx::Letter);
            }
            for l in r.letters_granted.iter().filter_map(letter) {
                toasts.info(format!("✔ {} thanks you: {}", l.from, l.goal().to_lowercase()));
                sfx.push(Sfx::Cash);
            }
            for l in r.letters_missed.iter().filter_map(letter) {
                toasts.warn(format!("{} gave up waiting: {}", l.from, l.goal().to_lowercase()));
            }
            election = election.or(r.election);
        }
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
    if let Some(e) = election {
        sfx.push(if e.won { Sfx::Milestone } else { Sfx::Error });
        hud.dialog = Dialog::Election(e);
        driver.paused = true;
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

/// Which flyout hangs off the left rail.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Flyout {
    Build(Category),
    DataViews,
}

/// Which card opens from the right rail.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Inbox,
    City,
    Budget,
    Mayor,
}

const DASH_H: f32 = 164.0;
const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
/// Game years count from this calendar year.
const FIRST_YEAR: u64 = 2025;

fn overlay_icon(o: Overlay) -> Icon {
    match o {
        Overlay::None => Icon::City,
        Overlay::LandValue => Icon::Coins,
        Overlay::Power => Icon::Power,
        Overlay::Water => Icon::Water,
        Overlay::Density => Icon::People,
        Overlay::Pollution => Icon::Leaf,
        Overlay::Traffic => Icon::Car,
        Overlay::Crime => Icon::Police,
        Overlay::Services => Icon::School,
    }
}

fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(pal::PAPER)
        .stroke(egui::Stroke::new(2.0, pal::BORDER))
        .corner_radius(14)
        .inner_margin(10)
        .shadow(egui::Shadow { offset: [0, 3], blur: 10, spread: 0, color: egui::Color32::from_black_alpha(40) })
}

fn dark_tip() -> egui::Frame {
    egui::Frame::new().fill(pal::NIGHT.gamma_multiply(0.94)).corner_radius(8).inner_margin(egui::Margin::symmetric(12, 8))
}

fn caps(t: &str) -> egui::RichText {
    egui::RichText::new(t).size(11.5).strong().color(pal::INK_SOFT)
}

/// A rounded white "field" holding one stat, as in the dashboard rows.
fn stat_row(ui: &mut egui::Ui, width: f32, icon: Icon, label: &str, value: egui::RichText) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 6.0, egui::Color32::WHITE.gamma_multiply(0.75));
    p.rect_stroke(rect, 6.0, (1.0, pal::PAPER_DEEP), egui::StrokeKind::Inside);
    icons::paint_in(p, egui::Rect::from_min_size(rect.min + egui::vec2(6.0, 3.0), egui::vec2(22.0, 22.0)), 20.0, icon, pal::PAPER);
    if !label.is_empty() {
        p.text(egui::pos2(rect.left() + 34.0, rect.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(14.0), pal::INK);
    }
    let mut job = egui::text::LayoutJob::default();
    value.append_to(&mut job, ui.style(), egui::FontSelection::Default, egui::Align::Center);
    let g = ui.fonts_mut(|f| f.layout_job(job));
    ui.painter().galley(egui::pos2(rect.right() - 8.0 - g.size().x, rect.center().y - g.size().y / 2.0), g, pal::INK);
    resp
}

/// The segmented approval meter from the dashboard.
fn approval_meter(ui: &mut egui::Ui, width: f32, percent: u8) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 20.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, pal::INK.gamma_multiply(0.85));
    let n = 20;
    let inner = rect.shrink(3.0);
    let w = inner.width() / n as f32;
    let lit = (percent as usize * n + 50) / 100;
    let color = match percent {
        65.. => pal::GOOD,
        40..65 => pal::GOLD,
        _ => pal::BAD,
    };
    for i in 0..n {
        let r = egui::Rect::from_min_size(egui::pos2(inner.left() + i as f32 * w, inner.top()), egui::vec2(w - 2.0, inner.height()));
        p.rect_filled(r, 2.0, if i < lit { color } else { pal::INK_SOFT.gamma_multiply(0.7) });
    }
}

/// Small bar chart of R/C/I/O demand, centred on zero.
fn rci_chart(ui: &mut egui::Ui, demand: [i32; 4]) {
    card().inner_margin(8).show(ui, |ui| {
        ui.set_width(112.0);
        ui.vertical_centered(|ui| {
            ui.label(caps("RCIO"));
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(96.0, 76.0), egui::Sense::hover());
            let p = ui.painter();
            let mid = rect.center().y - 6.0;
            p.line_segment([egui::pos2(rect.left(), mid), egui::pos2(rect.right(), mid)], (1.0, pal::INK_SOFT));
            let bw = rect.width() / 4.0;
            for z in Zone::ALL {
                let i = z.index();
                let cx = rect.left() + bw * (i as f32 + 0.5);
                let h = (demand[i] as f32 / 1000.0).clamp(-1.0, 1.0) * 28.0;
                let bar = egui::Rect::from_two_pos(egui::pos2(cx - bw * 0.3, mid), egui::pos2(cx + bw * 0.3, mid - h.abs().max(2.0) * h.signum().max(-1.0) - if h == 0.0 { 2.0 } else { 0.0 }));
                p.rect_filled(bar, 2.0, if demand[i] >= 0 { ZONE_COLORS[i] } else { pal::BAD });
                p.text(egui::pos2(cx, rect.bottom() - 6.0), egui::Align2::CENTER_CENTER, z.letter(), egui::FontId::proportional(12.0), pal::INK);
            }
            resp.on_hover_text(format!(
                "Demand. Residential {}, commercial {}, industrial {}, office {}",
                demand[0] / 10,
                demand[1] / 10,
                demand[2] / 10,
                demand[3] / 10
            ));
        });
    });
}

fn speed_button(ui: &mut egui::Ui, icon: Icon, on: bool, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(44.0, 32.0), egui::Sense::click());
    let p = ui.painter();
    let fill = if on { pal::ACCENT_SOFT } else if resp.hovered() { pal::PAPER } else { pal::PAPER_DIM };
    p.rect_filled(rect, 8.0, fill);
    p.rect_stroke(rect, 8.0, (if on { 2.0 } else { 1.0 }, if on { pal::BORDER } else { pal::PAPER_DEEP }), egui::StrokeKind::Inside);
    icons::paint_in(p, rect, 18.0, icon, fill);
    resp.on_hover_text(tip).clicked()
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
    let city = &state.city;
    let s = &city.stats;
    let tpm = state.config.ticks_per_month as i64;

    // Keep the flyout in step with hotkeys.
    match ts.tool {
        Tool::Build(b) => hud.flyout = Some(Flyout::Build(b.category())),
        _ if matches!(hud.flyout, Some(Flyout::Build(_))) => hud.flyout = None,
        _ => {}
    }

    let mut root = egui::Ui::new(
        ctx.clone(),
        "hud".into(),
        egui::UiBuilder::new().layer_id(egui::LayerId::background()).max_rect(ctx.viewport_rect()),
    );

    if in_street {
        street_overlay(&ctx, &state, &ts, &mode, &mut street);
    } else {
        // ---------------- Bottom dashboard ----------------
        egui::Panel::bottom("dash")
            .exact_size(DASH_H)
            .frame(egui::Frame::new().fill(pal::PAPER).stroke(egui::Stroke::new(2.0, pal::BORDER)).inner_margin(egui::Margin::symmetric(12, 10)))
            .show(&mut root, |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(12.0, 6.0);
                    minimap(ui, &state, &mut hud, &view, &mut orbit, driver.generation);

                    ui.vertical(|ui| {
                        ui.set_width(210.0);
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new(&city.name).size(20.0).strong().color(pal::INK));
                        });
                        let (y, m, d) = state.date();
                        let date = format!("{} {}, {}", MONTHS[(m - 1) as usize], d, FIRST_YEAR + y - 1);
                        stat_row(ui, 210.0, Icon::Calendar, "", egui::RichText::new(date).size(16.0).strong()).on_hover_text(format!(
                            "Year {y} of the city · {:02}:{:02}",
                            sky.hour as u32,
                            (sky.hour.fract() * 60.0) as u32
                        ));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if speed_button(ui, Icon::Pause, driver.paused, "Pause (Space)") {
                                driver.paused = !driver.paused;
                            }
                            for (icon, sp, tip) in [(Icon::Play, 4, "Normal speed"), (Icon::Fast, 8, "Fast"), (Icon::Faster, 32, "Fastest")] {
                                if speed_button(ui, icon, !driver.paused && driver.speed == sp, tip) {
                                    driver.speed = sp;
                                    driver.paused = false;
                                }
                            }
                        });
                    });

                    ui.vertical(|ui| {
                        ui.set_width(300.0);
                        let pct = s.approval_percent();
                        ui.vertical_centered(|ui| ui.label(caps(&format!("MAYOR APPROVAL · {}", mood_word(pct).to_uppercase()))));
                        ui.horizontal(|ui| {
                            approval_meter(ui, 230.0, pct);
                            ui.label(egui::RichText::new(format!("{pct}%")).size(18.0).strong().color(pal::INK));
                        })
                        .response
                        .on_hover_text("Open the City card for the full breakdown");
                        let net = (s.taxes - s.upkeep) * tpm;
                        let money = egui::RichText::new(format!(
                            "{}   {}{}/mo",
                            short_money(city.treasury),
                            if net >= 0 { "+" } else { "−" },
                            short_money(net.abs())
                        ))
                        .size(16.0)
                        .strong()
                        .color(if city.treasury < 0 { pal::BAD } else { pal::INK });
                        stat_row(ui, 300.0, Icon::Money, "", money)
                            .on_hover_text(format!("Treasury {} · taxes {}/mo · upkeep {}/mo", fmt_money(city.treasury), fmt_money(s.taxes * tpm), fmt_money(s.upkeep * tpm)));
                        stat_row(ui, 300.0, Icon::People, "", egui::RichText::new(s.population.to_string()).size(16.0).strong())
                            .on_hover_text("Residents");
                    });

                    ui.vertical(|ui| {
                        ui.set_width(260.0);
                        let months = months_until(&state, city.mayor.next_election);
                        let need = state.config.reelection_percent;
                        let ec = if s.approval_percent() >= need { pal::GOOD } else { pal::BAD };
                        let when = if months <= 12 { format!("{months} mo") } else { calendar(&state, city.mayor.next_election) };
                        if stat_row(ui, 260.0, Icon::Ballot, "Election", egui::RichText::new(when).size(15.0).strong().color(ec))
                            .on_hover_text(format!(
                                "Next election {} ({months} months). You need {need}% approval to win; you have {}%.",
                                calendar(&state, city.mayor.next_election),
                                s.approval_percent()
                            ))
                            .interact(egui::Sense::click())
                            .clicked()
                        {
                            hud.side = Some(Side::Mayor);
                        }
                        let jc = if s.jobless_percent > 10 { pal::BAD } else { pal::INK };
                        stat_row(ui, 260.0, Icon::Jobs, "Jobless", egui::RichText::new(format!("{}%", s.jobless_percent)).size(15.0).strong().color(jc));
                        let short = s.power.unserved + s.water.unserved;
                        let (uc, ut) = if short > 0 { (pal::BAD, format!("{short} short")) } else { (pal::GOOD, "OK".to_string()) };
                        stat_row(ui, 260.0, Icon::Power, "Power & water", egui::RichText::new(ut).size(15.0).strong().color(uc)).on_hover_text(format!(
                            "Power {} / {} · water {} / {}",
                            s.power.demand, s.power.supply, s.water.demand, s.water.supply
                        ));
                        let issue = match s.top_issue() {
                            Some(f) => egui::RichText::new(f.name()).size(15.0).strong().color(pal::WARN),
                            None => egui::RichText::new("None").size(15.0).strong().color(pal::GOOD),
                        };
                        let r = stat_row(ui, 260.0, Icon::Warning, "Top issue", issue);
                        if let Some(f) = s.top_issue() {
                            r.on_hover_text(f.complaint());
                        }
                    });

                    rci_chart(ui, s.demand);
                });
            });

        // ---------------- Left tool rail ----------------
        egui::Area::new("rail_left".into()).anchor(egui::Align2::LEFT_TOP, egui::vec2(10.0, 10.0)).show(&ctx, |ui| {
            card().inner_margin(egui::Margin::symmetric(4, 6)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if icons::rail_button(ui, Icon::Inspect, "Inspect", ts.tool == Tool::Inspect).on_hover_text("Esc").clicked() {
                    ts.set(Tool::Inspect);
                    hud.flyout = None;
                }
                for (c, label) in [
                    (Category::Zones, "Zoning"),
                    (Category::Transport, "Roads"),
                    (Category::Power, "Power"),
                    (Category::Water, "Water"),
                    (Category::Services, "Services"),
                    (Category::Parks, "Parks"),
                    (Category::Landmarks, "Landmarks"),
                ] {
                    let on = hud.flyout == Some(Flyout::Build(c));
                    if icons::rail_button(ui, Icon::for_category(c), label, on).clicked() {
                        if on {
                            hud.flyout = None;
                            ts.set(Tool::Inspect);
                        } else {
                            hud.flyout = Some(Flyout::Build(c));
                            let pick = ts.last_in(c).unwrap_or(items(c)[0]);
                            ts.set(Tool::Build(pick));
                        }
                    }
                }
                if icons::rail_button(ui, Icon::Bulldoze, "Demolish", ts.tool == Tool::Bulldoze).on_hover_text("B").clicked() {
                    ts.set(Tool::Bulldoze);
                    hud.flyout = None;
                }
                let on = hud.flyout == Some(Flyout::DataViews) || view.overlay != Overlay::None;
                if icons::rail_button(ui, Icon::DataViews, "Data views", on).on_hover_text("1–9").clicked() {
                    hud.flyout = if hud.flyout == Some(Flyout::DataViews) { None } else { Some(Flyout::DataViews) };
                }
            });
        });

        // ---------------- Flyout ----------------
        if let Some(f) = hud.flyout {
            egui::Area::new("flyout".into()).anchor(egui::Align2::LEFT_TOP, egui::vec2(98.0, 10.0)).show(&ctx, |ui| {
                card().show(ui, |ui| {
                    ui.set_width(270.0);
                    match f {
                        Flyout::Build(c) => {
                            ui.horizontal(|ui| {
                                icons::inline(ui, Icon::for_category(c), 22.0);
                                ui.label(egui::RichText::new(c.name()).size(17.0).strong());
                            });
                            ui.add_space(4.0);
                            for b in items(c) {
                                let cost = state.config.cost(b);
                                let per = if matches!(b, Buildable::Building(_)) { "" } else { "/tile" };
                                let key = hotkey(Tool::Build(b)).map(|k| format!("  [{k}]")).unwrap_or_default();
                                let r = icons::menu_button(ui, Icon::for_buildable(b), &b.name(), 270.0, 36.0, ts.tool == Tool::Build(b));
                                ui.painter().text(
                                    r.rect.right_center() - egui::vec2(10.0, 0.0),
                                    egui::Align2::RIGHT_CENTER,
                                    format!("{}{per}", fmt_money(cost).trim_end_matches(".00")),
                                    egui::FontId::proportional(12.5),
                                    pal::INK_SOFT,
                                );
                                if r.on_hover_text(format!("{}{key}", item_tooltip(&state, b))).clicked() {
                                    ts.set(Tool::Build(b));
                                }
                            }
                            ui.add_space(2.0);
                            ui.label(egui::RichText::new(tool_hint(ts.tool)).small().color(pal::INK_SOFT));
                        }
                        Flyout::DataViews => {
                            ui.horizontal(|ui| {
                                icons::inline(ui, Icon::DataViews, 22.0);
                                ui.label(egui::RichText::new("Data views").size(17.0).strong());
                            });
                            ui.add_space(4.0);
                            for o in Overlay::ALL {
                                let r = icons::menu_button(ui, overlay_icon(o), o.label(), 270.0, 34.0, view.overlay == o);
                                ui.painter().text(
                                    r.rect.right_center() - egui::vec2(10.0, 0.0),
                                    egui::Align2::RIGHT_CENTER,
                                    o.key(),
                                    egui::FontId::monospace(12.0),
                                    pal::INK_SOFT,
                                );
                                if r.clicked() {
                                    view.overlay = o;
                                }
                            }
                        }
                    }
                });
            });
        }

        // ---------------- Right rail ----------------
        egui::Area::new("rail_right".into()).anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 10.0)).show(&ctx, |ui| {
            card().inner_margin(egui::Margin::symmetric(4, 6)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let unread = city.mayor.open().count();
                for (side, icon, label, tip) in [
                    (Side::Inbox, Icon::Mail, "Inbox", "Letters from residents"),
                    (Side::City, Icon::City, "City", "How residents feel (`)"),
                    (Side::Budget, Icon::Budget, "Budget", "Taxes, upkeep, utilities"),
                    (Side::Mayor, Icon::Mayor, "Mayor", "Your record and the next election"),
                ] {
                    let on = hud.side == Some(side);
                    let r = icons::rail_button(ui, icon, label, on);
                    if side == Side::Inbox && unread > 0 {
                        badge(ui, r.rect, unread);
                    }
                    if r.on_hover_text(tip).clicked() {
                        hud.side = if on { None } else { Some(side) };
                    }
                }
                let (label, tip) = if orbit.iso { ("Iso view", "Isometric. Click for the free 3D camera (T)") } else { ("3D view", "Free camera. Click for isometric (T)") };
                if icons::rail_button(ui, Icon::Cube, label, false).on_hover_text(tip).clicked() {
                    orbit.iso = !orbit.iso;
                    if orbit.iso {
                        orbit.iso_yaw = crate::camera::snap_quarter(orbit.yaw);
                    }
                }
                if icons::rail_button(ui, Icon::Walk, "Street", false).on_hover_text("Walk the streets (V)").clicked() {
                    street.write(StreetRequest(ts.selected.or(ts.hover)));
                }
                let sound = if settings.muted { Icon::Muted } else { Icon::Sound };
                if icons::rail_button(ui, sound, "Sound", false).on_hover_text("M").clicked() {
                    settings.muted = !settings.muted;
                }
                if icons::rail_button(ui, Icon::Help, "Help", hud.dialog == Dialog::Help).on_hover_text("H").clicked() {
                    hud.dialog = if hud.dialog == Dialog::Help { Dialog::None } else { Dialog::Help };
                }
                if icons::rail_button(ui, Icon::Gear, "Game", hud.dialog == Dialog::Pause).on_hover_text("F10").clicked() {
                    hud.dialog = Dialog::Pause;
                }
            });
        });

        // ---------------- Side cards ----------------
        let selected = if ts.tool == Tool::Inspect { ts.selected } else { None };
        if hud.side.is_some() || selected.is_some() {
            egui::Area::new("side".into()).anchor(egui::Align2::RIGHT_TOP, egui::vec2(-98.0, 10.0)).show(&ctx, |ui| {
                ui.set_width(320.0);
                match hud.side {
                    Some(Side::City) => {
                        card().show(ui, |ui| city_card(ui, &state));
                    }
                    Some(Side::Budget) => {
                        card().show(ui, |ui| budget_card(ui, &state, &mut actions));
                    }
                    Some(Side::Inbox) => {
                        card().show(ui, |ui| {
                            if let Some(at) = inbox_card(ui, &state) {
                                orbit.focus = tile_center(at).with_y(0.0);
                                ts.set(Tool::Inspect);
                                ts.selected = Some(at);
                            }
                        });
                    }
                    Some(Side::Mayor) => {
                        card().show(ui, |ui| mayor_card(ui, &state));
                    }
                    None => {}
                }
                if let Some(at) = selected {
                    ui.add_space(8.0);
                    card().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            icons::inline(ui, Icon::Inspect, 20.0);
                            ui.label(egui::RichText::new("Inspect").size(16.0).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✕").clicked() {
                                    ts.selected = None;
                                }
                            });
                        });
                        inspector(ui, &state, at);
                    });
                }
            });
        }

        // ---------------- Hover tooltip ----------------
        if ts.tool == Tool::Inspect && !ctx.is_pointer_over_egui() {
            if let (Some(h), Some(at)) = (ts.hover, ctx.pointer_hover_pos()) {
                if let Some(lines) = hover_lines(&state, h) {
                    egui::Area::new("hover_tip".into()).fixed_pos(at + egui::vec2(18.0, 14.0)).interactable(false).order(egui::Order::Tooltip).show(&ctx, |ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        dark_tip().show(ui, |ui| {
                            for (i, l) in lines.iter().enumerate() {
                                let t = egui::RichText::new(l).color(egui::Color32::WHITE).size(14.0);
                                ui.label(if i == 0 { t.strong().size(15.0) } else { t });
                            }
                        });
                    });
                }
            }
        }
    }

    // ---------------- Ghost preview tooltip ----------------
    if let Some(p) = &ts.preview {
        let at = if in_street { Some(ctx.viewport_rect().center() + egui::vec2(16.0, 16.0)) } else { ctx.pointer_hover_pos().map(|p| p + egui::vec2(18.0, 14.0)) };
        if let Some(at) = at {
            egui::Area::new("preview".into()).fixed_pos(at).interactable(false).order(egui::Order::Tooltip).show(&ctx, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                dark_tip().show(ui, |ui| match &p.result {
                    Ok(plan) => {
                        let n = if plan.footprints.is_empty() { plan.tiles.len() } else { plan.footprints.len() };
                        ui.label(egui::RichText::new(format!("{} × {}", n, ts.tool.label())).color(egui::Color32::WHITE).strong());
                        let afford = plan.cost <= city.treasury;
                        ui.label(egui::RichText::new(fmt_money(plan.cost)).color(if afford { egui::Color32::from_rgb(170, 230, 140) } else { egui::Color32::from_rgb(255, 150, 130) }));
                        if p.clipped {
                            ui.label(egui::RichText::new("Clipped to the map").small().color(egui::Color32::LIGHT_GRAY));
                        }
                    }
                    Err(why) => {
                        ui.label(egui::RichText::new(why).color(egui::Color32::from_rgb(255, 150, 130)));
                    }
                });
            });
        }
    }

    // ---------------- Toasts ----------------
    let lift = if in_street { 70.0 } else { DASH_H + 14.0 };
    egui::Area::new("toasts".into()).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -lift)).interactable(false).show(&ctx, |ui| {
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            for t in toasts.0.iter().rev() {
                let c = if t.warn { egui::Color32::from_rgb(255, 170, 150) } else { egui::Color32::WHITE };
                dark_tip().corner_radius(10).show(ui, |ui| {
                    ui.label(egui::RichText::new(&t.text).color(c).size(15.0).strong());
                });
            }
        });
    });

    // ---------------- Paused pill ----------------
    if driver.paused && hud.dialog == Dialog::None {
        egui::Area::new("paused".into()).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 12.0)).show(&ctx, |ui| {
            egui::Frame::new()
                .fill(pal::ACCENT_SOFT)
                .stroke(egui::Stroke::new(2.5, pal::BORDER))
                .corner_radius(22)
                .inner_margin(egui::Margin::symmetric(14, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        icons::inline(ui, Icon::Pause, 20.0);
                        ui.label(egui::RichText::new("PAUSED").size(20.0).strong().color(pal::INK).extra_letter_spacing(2.0));
                        ui.add_space(6.0);
                        if icons::menu_button(ui, Icon::Play, "Resume", 104.0, 30.0, false).clicked() {
                            driver.paused = false;
                        }
                    });
                });
        });
    }

    dialogs(&ctx, &state, &mut driver, &mut hud, &mut toasts, &mut next);

    for kind in actions {
        driver.submit(kind);
    }
    Ok(())
}

fn tool_hint(tool: Tool) -> &'static str {
    match tool {
        Tool::Build(Buildable::Road(_) | Buildable::PowerLine | Buildable::WaterPipe) => "Drag to lay a line. Right-click cancels.",
        Tool::Build(Buildable::Building(b)) if b.size() > 1 => "Click to place; the footprint centres on the cursor.",
        Tool::Build(Buildable::Building(_)) => "Click to place, or drag to place several.",
        _ => "Drag to paint an area. Right-click cancels.",
    }
}

/// Money in a short form for the dashboard: $950, $12.4K, $2.9M.
fn short_money(cents: i64) -> String {
    let d = cents / 100;
    let a = d.abs();
    let sign = if d < 0 { "-" } else { "" };
    if a >= 1_000_000 {
        format!("{sign}${:.1}M", a as f64 / 1e6)
    } else if a >= 10_000 {
        format!("{sign}${:.1}K", a as f64 / 1e3)
    } else {
        format!("{sign}${a}")
    }
}

fn street_overlay(ctx: &egui::Context, state: &GameState, ts: &ToolState, mode: &CamMode, street: &mut MessageWriter<StreetRequest>) {
    let center = ctx.viewport_rect().center();
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, "crosshair".into()));
    painter.circle_stroke(center, 5.0, (1.5, egui::Color32::from_white_alpha(200)));
    egui::Area::new("street_back".into()).anchor(egui::Align2::LEFT_TOP, egui::vec2(10.0, 10.0)).show(ctx, |ui| {
        card().inner_margin(6).show(ui, |ui| {
            if icons::menu_button(ui, Icon::City, "Back to the map", 190.0, 36.0, false).on_hover_text("V or Esc").clicked() {
                street.write(StreetRequest(None));
            }
        });
    });
    egui::Area::new("street_hint".into()).anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -16.0)).interactable(false).show(ctx, |ui| {
        dark_tip().show(ui, |ui| {
            let fly = matches!(mode, CamMode::Street { fly: true });
            ui.label(
                egui::RichText::new(format!(
                    "{} · WASD move · Shift run · F {} · B/Z/R… tools work at the crosshair",
                    if fly { "Flying" } else { "Walking" },
                    if fly { "walk" } else { "fly" }
                ))
                .color(egui::Color32::WHITE),
            );
            if let Some(h) = ts.hover {
                let t = state.map.tile(h);
                ui.label(egui::RichText::new(format!("Looking at {} ({}, {})", t.kind.name(), h.x, h.y)).color(egui::Color32::LIGHT_GRAY));
            }
        });
    });
}

/// The few lines shown when hovering a built tile with the inspect tool.
fn hover_lines(state: &GameState, at: parcels_sim::Pos) -> Option<Vec<String>> {
    let t = state.map.tile(at);
    let idx = state.map.idx(at);
    let mut lines = Vec::new();
    match t.kind {
        TileKind::Empty | TileKind::Road(_) => return None,
        TileKind::Zone(z, d) => {
            let dense = if d == parcels_sim::Density::High { "Dense " } else { "" };
            lines.push(format!("{dense}{} · level {}", z.name(), t.level));
            let who = if z == Zone::Residential { "Residents" } else { "Jobs" };
            lines.push(format!("{who}: {}", state.occupants(idx)));
            lines.push(match growth_block(&state.config, t) {
                Some(why) => format!("Growth: {why}"),
                None if !t.powered => "Growth: no power".into(),
                None => "Growth: OK".into(),
            });
        }
        TileKind::Building(b) => {
            let s = state.config.building(b);
            lines.push(b.name().to_string());
            if s.jobs > 0 {
                lines.push(format!("Jobs: {}", s.jobs));
            }
            if s.power_draw > 0 {
                lines.push(if t.powered { "Running".into() } else { "No power: not running".into() });
            }
        }
    }
    lines.push(format!("Land value {} · crime {} · pollution {}", t.land_value, t.crime, t.pollution));
    Some(lines)
}

fn city_card(ui: &mut egui::Ui, state: &GameState) {
    let s = &state.city.stats;
    let pct = s.approval_percent();
    ui.horizontal(|ui| {
        icons::inline(ui, Icon::City, 24.0);
        ui.label(egui::RichText::new(&state.city.name).size(18.0).strong());
    });
    ui.horizontal(|ui| {
        icons::inline(ui, Icon::Mood, 18.0);
        ui.label(egui::RichText::new(format!("{} · {pct}% approve", mood_word(pct))).strong());
    });
    approval_meter(ui, 300.0, pct);
    ui.add_space(4.0);
    match s.top_issue() {
        Some(f) => {
            ui.horizontal(|ui| {
                icons::inline(ui, Icon::Warning, 18.0);
                ui.label(egui::RichText::new(f.complaint()).color(pal::WARN).strong());
            });
        }
        None => {
            ui.label(egui::RichText::new("No complaints right now.").color(pal::GOOD));
        }
    }
    ui.add_space(6.0);
    ui.label(caps("WHAT RESIDENTS THINK"));
    for f in Factor::ALL {
        let v = s.mood[f.index()];
        ui.horizontal(|ui| {
            icons::inline(ui, Icon::for_factor(f), 18.0);
            ui.allocate_ui_with_layout(egui::vec2(124.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(124.0);
                ui.label(f.name());
            });
            let (rect, _) = ui.allocate_exact_size(egui::vec2(150.0, 12.0), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(rect, 3.0, pal::PAPER_DEEP);
            let mid = rect.left() + rect.width() * 2.0 / 3.0;
            let scale = rect.width() / 30.0;
            let end = mid + v as f32 * scale;
            let bar = egui::Rect::from_x_y_ranges(mid.min(end)..=mid.max(end), rect.y_range());
            p.rect_filled(bar, 2.0, if v >= 0 { pal::GOOD } else { pal::BAD });
            p.line_segment([egui::pos2(mid, rect.top() - 2.0), egui::pos2(mid, rect.bottom() + 2.0)], (1.0, pal::INK_SOFT));
        });
    }
    ui.add_space(6.0);
    ui.label(caps("PEOPLE AND JOBS"));
    egui::Grid::new("city_stats").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
        for (k, v) in [
            ("Population", s.population.to_string()),
            ("Jobless", format!("{}%", s.jobless_percent)),
            ("Shop / industry jobs", format!("{} / {}", s.commercial_jobs, s.industrial_jobs)),
            ("Office / public jobs", format!("{} / {}", s.office_jobs, s.public_jobs)),
            ("Buildings lost to fire", s.fires.to_string()),
        ] {
            ui.label(k);
            ui.label(egui::RichText::new(v).strong());
            ui.end_row();
        }
    });
}

fn budget_card(ui: &mut egui::Ui, state: &GameState, actions: &mut Vec<CommandKind>) {
    let tpm = state.config.ticks_per_month as i64;
    let city = &state.city;
    let s = &city.stats;
    ui.horizontal(|ui| {
        icons::inline(ui, Icon::Budget, 24.0);
        ui.label(egui::RichText::new("Budget").size(18.0).strong());
    });
    egui::Grid::new("budget").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
        let net = (s.taxes - s.upkeep) * tpm;
        for (k, v, c) in [
            ("Treasury", fmt_money(city.treasury), if city.treasury < 0 { pal::BAD } else { pal::INK }),
            ("Taxes / month", fmt_money(s.taxes * tpm), pal::GOOD),
            ("Upkeep / month", fmt_money(-s.upkeep * tpm), pal::BAD),
            ("Net / month", fmt_money(net), if net >= 0 { pal::GOOD } else { pal::BAD }),
        ] {
            ui.label(k);
            ui.label(egui::RichText::new(v).strong().color(c));
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    let mut tax = city.tax_rate;
    ui.horizontal(|ui| {
        icons::inline(ui, Icon::Coins, 18.0);
        ui.label("Tax rate");
        if ui.add(egui::Slider::new(&mut tax, 0..=state.config.max_tax_rate).suffix("%")).changed() {
            actions.push(CommandKind::SetTaxRate { rate: tax });
        }
    });
    ui.label(
        egui::RichText::new(format!("Above {}% newcomers stay away and residents grumble.", state.config.neutral_tax_rate)).small().color(pal::INK_SOFT),
    );
    ui.add_space(6.0);
    ui.label(caps("UTILITIES"));
    for (icon, name, l) in [(Icon::Power, "Power", &s.power), (Icon::Water, "Water", &s.water)] {
        ui.horizontal(|ui| {
            icons::inline(ui, icon, 18.0);
            ui.allocate_ui_with_layout(egui::vec2(60.0, 18.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(60.0);
                ui.label(name);
            });
            let frac = if l.supply == 0 { if l.demand == 0 { 0.0 } else { 1.0 } } else { (l.demand as f32 / l.supply as f32).min(1.0) };
            let c = if l.unserved > 0 { pal::BAD } else { pal::GOOD };
            ui.add(egui::ProgressBar::new(frac).desired_width(150.0).fill(c).text(format!("{} / {}", l.demand, l.supply)));
        });
        if l.unserved > 0 {
            ui.label(egui::RichText::new(format!("{} units short: build more {}", l.unserved, name.to_lowercase())).small().color(pal::BAD));
        }
    }
}

fn dialogs(ctx: &egui::Context, state: &GameState, driver: &mut Driver, hud: &mut Hud, toasts: &mut Toasts, next: &mut NextState<AppState>) {
    let center = |title: &str| egui::Window::new(title.to_string()).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO);
    match hud.dialog {
        Dialog::None => {}
        Dialog::Pause => {
            center("Game").show(ctx, |ui| {
                ui.set_width(240.0);
                ui.spacing_mut().item_spacing.y = 8.0;
                let wide = |ui: &mut egui::Ui, icon: Icon, t: &str| icons::menu_button(ui, icon, t, 240.0, 38.0, false).clicked();
                if wide(ui, Icon::Play, "Resume") {
                    hud.dialog = Dialog::None;
                }
                if wide(ui, Icon::Save, "Save city…") {
                    hud.save_name = driver.save_name.clone().unwrap_or_else(|| default_save_name(state));
                    hud.dialog = Dialog::Save;
                }
                if wide(ui, Icon::Folder, "Load city…") {
                    hud.dialog = Dialog::Load;
                }
                if wide(ui, Icon::Replay, "Export replay") {
                    export_replay(driver, toasts);
                }
                if wide(ui, Icon::Help, "Controls") {
                    hud.dialog = Dialog::Help;
                }
                ui.separator();
                if wide(ui, Icon::Quit, "Quit to title") {
                    hud.dialog = Dialog::None;
                    next.set(AppState::Menu);
                }
            });
        }
        Dialog::Save => {
            center("Save city").show(ctx, |ui| {
                ui.set_width(360.0);
                ui.horizontal(|ui| {
                    ui.label("Name");
                    let r = ui.add(egui::TextEdit::singleline(&mut hud.save_name).desired_width(240.0));
                    r.request_focus();
                });
                let slug = saves::slug(&hud.save_name);
                if saves::exists(&slug) {
                    ui.colored_label(pal::WARN, "A save with this name exists and will be replaced.");
                }
                ui.horizontal(|ui| {
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if icons::menu_button(ui, Icon::Save, "Save", 110.0, 34.0, true).clicked() || enter {
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
            center("Load city").show(ctx, |ui| {
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
        Dialog::Election(e) => {
            let title = if e.won { "Re-elected!" } else { "Voted out" };
            center(title).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.horizontal(|ui| {
                    icons::inline(ui, if e.won { Icon::Ballot } else { Icon::Warning }, 44.0);
                    ui.vertical(|ui| {
                        let m = &state.city.mayor;
                        if e.won {
                            ui.label(egui::RichText::new(format!("{} voted to keep you, {}% in favour.", state.city.name, e.approval)).size(16.0).strong());
                            ui.label(format!("Your term {} starts now. Next election {}.", m.term(), calendar(state, m.next_election)));
                        } else {
                            ui.label(egui::RichText::new(format!("Only {}% voted for you. {} has chosen someone else.", e.approval, state.city.name)).size(16.0).strong());
                            ui.label("You can stay on as caretaker and win them back, or start over.");
                        }
                    });
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let label = if e.won { "Back to work" } else { "Stay on as caretaker" };
                    if icons::menu_button(ui, Icon::Play, label, 220.0, 36.0, true).clicked() {
                        hud.dialog = Dialog::None;
                        driver.paused = false;
                    }
                    if !e.won && icons::menu_button(ui, Icon::Quit, "Quit to title", 150.0, 36.0, false).clicked() {
                        hud.dialog = Dialog::None;
                        next.set(AppState::Menu);
                    }
                });
            });
        }
        Dialog::Help => {
            center("Controls").show(ctx, |ui| {
                ui.set_width(440.0);
                egui::Grid::new("help").num_columns(2).striped(true).show(ui, |ui| {
                    for (k, v) in [
                        ("WASD / arrows / edges", "Pan the map"),
                        ("Right-drag", "Pan"),
                        ("T", "Isometric view / free 3D camera"),
                        ("Q / E", "Turn a quarter (isometric) or rotate"),
                        ("Middle-drag, PgUp/PgDn", "Rotate · tilt (3D camera)"),
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

/// Whole months from now until `tick`.
fn months_until(state: &GameState, tick: u64) -> u64 {
    tick.saturating_sub(state.tick).div_ceil(state.config.ticks_per_month as u64)
}

/// "MAR 2029" for a tick.
fn calendar(state: &GameState, tick: u64) -> String {
    let m = tick / state.config.ticks_per_month as u64;
    format!("{} {}", MONTHS[(m % 12) as usize], FIRST_YEAR + m / 12)
}

/// A red count bubble on the top-right of a rail button.
fn badge(ui: &egui::Ui, rect: egui::Rect, n: usize) {
    let c = egui::pos2(rect.center().x + 18.0, rect.top() + 8.0);
    let p = ui.painter();
    p.circle_filled(c, 10.0, pal::BAD);
    p.circle_stroke(c, 10.0, (1.5, pal::PAPER));
    p.text(c, egui::Align2::CENTER_CENTER, n.to_string(), egui::FontId::proportional(12.0), egui::Color32::WHITE);
}

/// Open letters, then recently answered ones. Returns a tile to look at when
/// the player clicks "Show".
fn inbox_card(ui: &mut egui::Ui, state: &GameState) -> Option<Pos> {
    let m = &state.city.mayor;
    let mut show = None;
    ui.horizontal(|ui| {
        icons::inline(ui, Icon::Mail, 24.0);
        ui.label(egui::RichText::new("Inbox").size(18.0).strong());
    });
    let open: Vec<&Letter> = m.open().collect();
    if open.is_empty() {
        ui.label(egui::RichText::new("No letters waiting. Residents write in about whatever bothers them most.").color(pal::INK_SOFT));
    }
    for l in open.iter().rev() {
        egui::Frame::new().fill(egui::Color32::WHITE.gamma_multiply(0.75)).stroke((1.0, pal::PAPER_DEEP)).corner_radius(8).inner_margin(8).show(ui, |ui| {
            ui.set_width(296.0);
            ui.horizontal(|ui| {
                icons::inline(ui, Icon::for_ask(l.ask), 22.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&l.from).strong());
                    ui.label(egui::RichText::new(area_name(&state.map, l.at)).small().color(pal::INK_SOFT));
                });
            });
            ui.label(egui::RichText::new(format!("“{}”", l.text())).italics());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(l.goal()).small().strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if l.ask.is_local() && ui.small_button("Show").on_hover_text("Look at their home").clicked() {
                        show = Some(l.at);
                    }
                    let left = months_until(state, l.due);
                    ui.label(egui::RichText::new(format!("{left} mo left")).small().color(if left <= 2 { pal::BAD } else { pal::INK_SOFT }));
                });
            });
        });
        ui.add_space(4.0);
    }
    let answered: Vec<&Letter> = m.letters.iter().filter(|l| l.state != LetterState::Open).collect();
    if !answered.is_empty() {
        ui.add_space(4.0);
        ui.label(caps("ANSWERED"));
        for l in answered.iter().rev() {
            ui.horizontal(|ui| {
                icons::inline(ui, if l.state == LetterState::Granted { Icon::Check } else { Icon::Cross }, 16.0);
                ui.label(egui::RichText::new(format!("{}: {}", l.from, l.goal().to_lowercase())).small());
            });
        }
    }
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(format!(
            "Granting a request lifts approval by about {}%; letting one lapse costs {}%. The effect fades over the months.",
            state.config.letter_reward / 10,
            state.config.letter_penalty / 10
        ))
        .small()
        .color(pal::INK_SOFT),
    );
    show
}

fn mayor_card(ui: &mut egui::Ui, state: &GameState) {
    let m = &state.city.mayor;
    let s = &state.city.stats;
    let pct = s.approval_percent();
    let need = state.config.reelection_percent;
    ui.set_width(300.0);
    ui.horizontal(|ui| {
        icons::inline(ui, Icon::Mayor, 24.0);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(format!("Mayor of {}", state.city.name)).size(18.0).strong());
            ui.label(egui::RichText::new(format!("Term {}", m.term())).color(pal::INK_SOFT));
        });
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        icons::inline(ui, Icon::Ballot, 18.0);
        let months = months_until(state, m.next_election);
        ui.label(egui::RichText::new(format!("Election {} · {months} months", calendar(state, m.next_election))).strong());
    });
    let r = ui.horizontal(|ui| {
        approval_meter(ui, 250.0, pct);
        ui.label(egui::RichText::new(format!("{pct}%")).strong());
    });
    // Mark the line the mayor has to clear.
    let meter = r.response.rect;
    let x = meter.left() + 3.0 + (250.0 - 6.0) * need as f32 / 100.0;
    ui.painter().line_segment([egui::pos2(x, meter.top() - 3.0), egui::pos2(x, meter.top() + 23.0)], (2.5, pal::ACCENT));
    let (txt, c) = if pct >= need { (format!("You'd win if the vote were today ({need}% needed)."), pal::GOOD) } else { (format!("You'd lose if the vote were today: {need}% needed."), pal::BAD) };
    ui.label(egui::RichText::new(txt).color(c));
    ui.add_space(6.0);
    ui.label(caps("GOODWILL"));
    let gw = m.goodwill;
    let word = match gw {
        30.. => "Residents feel heard",
        1..30 => "A little goodwill",
        0 => "Neutral",
        -29..0 => "Some letters went unanswered",
        _ => "Residents feel ignored",
    };
    ui.horizontal(|ui| {
        icons::inline(ui, Icon::Mail, 18.0);
        ui.label(format!("{word} ({}{:.1}% approval)", if gw >= 0 { "+" } else { "−" }, gw.abs() as f32 / 10.0));
    });
    ui.label(egui::RichText::new(format!("Requests granted: {} · missed: {}", m.letters_granted, m.letters_missed)).small().color(pal::INK_SOFT));
    if !m.elections.is_empty() {
        ui.add_space(6.0);
        ui.label(caps("PAST ELECTIONS"));
        for e in m.elections.iter().rev().take(6) {
            ui.horizontal(|ui| {
                icons::inline(ui, if e.won { Icon::Check } else { Icon::Cross }, 16.0);
                ui.label(format!("{} · {}% · {}", calendar(state, e.tick), e.approval, if e.won { "won" } else { "lost" }));
            });
        }
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Approval also pulls people in: a popular mayor raises demand for homes, an unpopular one drives residents away.")
            .small()
            .color(pal::INK_SOFT),
    );
}

pub fn default_save_name(state: &GameState) -> String {
    let (y, m, _) = state.date();
    format!("{} year {y} month {m}", state.city.name)
}

fn inspector(ui: &mut egui::Ui, state: &GameState, pos: parcels_sim::Pos) {
    {
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
                    Some(why) => ui.colored_label(pal::WARN, why),
                    None if !t.powered => ui.colored_label(pal::BAD, "no power"),
                    None => ui.colored_label(pal::GOOD, "can grow when there's demand"),
                };
                ui.end_row();
            }
            if let TileKind::Building(b) = t.kind {
                let s = state.config.building(b);
                if s.power_draw > 0 {
                    ui.label("Running");
                    if t.powered {
                        ui.colored_label(pal::GOOD, "yes");
                    } else {
                        ui.colored_label(pal::BAD, "no power");
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
    }
}

fn minimap(ui: &mut egui::Ui, state: &GameState, hud: &mut Hud, view: &View, orbit: &mut Orbit, generation: u64) {
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
            None => hud.minimap = Some(ui.ctx().load_texture("minimap", img, egui::TextureOptions::NEAREST)),
        }
    }
    let Some(tex) = &hud.minimap else { return };
    let (w, h) = (state.map.width as f32, state.map.height as f32);
    let scale = (176.0 / w).min(128.0 / h);
    let size = egui::vec2(w * scale, h * scale);
    egui::Frame::new().stroke(egui::Stroke::new(2.0, pal::PAPER_DEEP)).corner_radius(6).inner_margin(2).show(ui, |ui| {
        let resp = ui.add(egui::Image::new((tex.id(), size)).corner_radius(4).sense(egui::Sense::click_and_drag()));
        let r = resp.rect;
        let to_screen = |v: Vec3| r.min + egui::vec2(v.x / TILE * scale, v.z / TILE * scale);
        let p = ui.painter_at(r);
        // Where the camera is and which way it looks.
        let f = to_screen(orbit.focus);
        let e = to_screen(orbit.eye());
        p.line_segment([e, f], (1.5, egui::Color32::WHITE));
        p.circle_filled(f, 3.0, egui::Color32::WHITE);
        if (resp.clicked() || resp.dragged()) && resp.interact_pointer_pos().is_some() {
            let at = resp.interact_pointer_pos().unwrap() - r.min;
            orbit.focus = Vec3::new(at.x / scale, 0.0, at.y / scale) * TILE;
        }
    });
}
