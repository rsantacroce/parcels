//! In-game HUD: date and speed, treasury, RCI demand, taxes, utility trade,
//! players and scores, tile inspector, overlays, toasts.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use parcels_sim::{
    fmt_money, AiStrategy, Buildable, CommandKind, Controller, GameState, PlayerId, TileKind, Utility,
};

use crate::audio::{Sfx, SfxQueue};
use crate::driver::{Driver, TickFeed};
use crate::render::{player_color, Overlay, View};
use crate::tools::{Tool, ToolState};
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

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Toasts>()
            .add_systems(Update, (digest_feed, keys).run_if(in_state(AppState::Playing)))
            .add_systems(EguiPrimaryContextPass, hud.run_if(in_state(AppState::Playing)));
    }
}

pub const SAVE_DIR: &str = "saves";

pub fn save_game(driver: &Driver, toasts: &mut Toasts) {
    let Some(state) = driver.state() else { return };
    if !driver.is_authority() {
        toasts.warn("Only the host can save");
        return;
    }
    let _ = std::fs::create_dir_all(SAVE_DIR);
    let path = format!("{SAVE_DIR}/quicksave.ron");
    match std::fs::write(&path, state.to_ron()) {
        Ok(()) => toasts.info(format!("Saved to {path}")),
        Err(e) => toasts.warn(format!("Save failed: {e}")),
    }
}

pub fn load_game(driver: &mut Driver, toasts: &mut Toasts) {
    if !driver.is_authority() {
        toasts.warn("Only the host can load");
        return;
    }
    let path = format!("{SAVE_DIR}/quicksave.ron");
    match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| GameState::from_ron(&t).map_err(|e| e.to_string())) {
        Ok(state) => {
            driver.replace_state(state);
            toasts.info(format!("Loaded {path}"));
        }
        Err(e) => toasts.warn(format!("Load failed: {e}")),
    }
}

fn export_replay(driver: &Driver, toasts: &mut Toasts) {
    let Some(replay) = driver.replay() else {
        toasts.warn("Replays are recorded by the host");
        return;
    };
    let _ = std::fs::create_dir_all(SAVE_DIR);
    let path = format!("{SAVE_DIR}/replay.ron");
    match ron::ser::to_string(replay).map_err(|e| e.to_string()).and_then(|t| std::fs::write(&path, t).map_err(|e| e.to_string())) {
        Ok(()) => toasts.info(format!("Replay ({} commands) written to {path}", replay.commands.len())),
        Err(e) => toasts.warn(format!("Replay export failed: {e}")),
    }
}

fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    egui: Res<bevy_egui::input::EguiWantsInput>,
    mut driver: ResMut<Driver>,
    mut toasts: ResMut<Toasts>,
    mut sfx: ResMut<SfxQueue>,
) {
    if egui.wants_any_keyboard_input() {
        return;
    }
    if keys.just_pressed(KeyCode::Space) && driver.is_authority() {
        driver.paused = !driver.paused;
    }
    if keys.just_pressed(KeyCode::Equal) && driver.is_authority() {
        driver.speed = (driver.speed * 2).min(32);
    }
    if keys.just_pressed(KeyCode::Minus) && driver.is_authority() {
        driver.speed = (driver.speed / 2).max(1);
    }
    if keys.just_pressed(KeyCode::F5) {
        save_game(&driver, &mut toasts);
    }
    if keys.just_pressed(KeyCode::F9) {
        load_game(&mut driver, &mut toasts);
    }
    if keys.just_pressed(KeyCode::KeyM) {
        sfx.muted = !sfx.muted;
        toasts.info(if sfx.muted { "Sound off" } else { "Sound on" });
    }
}

/// Turn tick reports and network log lines into toasts and sounds.
fn digest_feed(
    mut feed: ResMut<TickFeed>,
    mut driver: ResMut<Driver>,
    view: Res<View>,
    mut toasts: ResMut<Toasts>,
    mut sfx: ResMut<SfxQueue>,
    time: Res<Time<Real>>,
) {
    for line in driver.log() {
        sfx.push(Sfx::Join);
        toasts.info(line);
    }
    let mine = driver.controllable();
    let reports = std::mem::take(&mut feed.reports);
    let Some(state) = driver.state() else { return };
    for r in reports {
        for (cmd, why) in &r.rejected {
            if mine.contains(&cmd.author) {
                toasts.warn(format!("{}: {why}", state.players[cmd.author.index()].name));
                sfx.push(Sfx::Error);
            }
        }
        if r.month_ended {
            if let Some(p) = view.active.and_then(|a| state.player(a)) {
                let s = &p.stats;
                if s.taxes + s.trade - s.upkeep > 0 {
                    sfx.push(Sfx::Cash);
                }
            }
        }
        if r.game_over {
            sfx.push(Sfx::GameOver);
        }
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

fn color32(id: PlayerId) -> egui::Color32 {
    let [r, g, b] = player_color(id);
    egui::Color32::from_rgb(r, g, b)
}

fn demand_bar(ui: &mut egui::Ui, label: &str, v: i32, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).strong().color(color));
        let (rect, _) = ui.allocate_exact_size(egui::vec2(90.0, 12.0), egui::Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 2.0, egui::Color32::from_gray(40));
        let mid = rect.center().x;
        let w = (v as f32 / 1000.0).clamp(-1.0, 1.0) * rect.width() / 2.0;
        let bar = if w >= 0.0 {
            egui::Rect::from_min_max(egui::pos2(mid, rect.top()), egui::pos2(mid + w, rect.bottom()))
        } else {
            egui::Rect::from_min_max(egui::pos2(mid + w, rect.top()), egui::pos2(mid, rect.bottom()))
        };
        p.rect_filled(bar, 2.0, if v >= 0 { color } else { egui::Color32::from_rgb(200, 60, 50) });
        p.line_segment([egui::pos2(mid, rect.top()), egui::pos2(mid, rect.bottom())], (1.0, egui::Color32::GRAY));
    })
    .response
    .on_hover_text("Demand for this zone in your neighbourhood (map-wide balance shifted by your tax rate)");
}

#[allow(clippy::too_many_arguments)]
fn hud(
    mut contexts: EguiContexts,
    mut driver: ResMut<Driver>,
    mut view: ResMut<View>,
    mut ts: ResMut<ToolState>,
    mut toasts: ResMut<Toasts>,
    mut next: ResMut<NextState<AppState>>,
    mut sfx: ResMut<SfxQueue>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let Some(state) = driver.state().cloned() else {
        return Ok(());
    };
    let active = view.active.filter(|a| state.player(*a).is_some());
    let mine = driver.controllable();
    let mut actions: Vec<(PlayerId, CommandKind)> = Vec::new();
    let mut system_actions: Vec<CommandKind> = Vec::new();

    let mut root = egui::Ui::new(
        ctx.clone(),
        "hud".into(),
        egui::UiBuilder::new().layer_id(egui::LayerId::background()).max_rect(ctx.viewport_rect()),
    );

    // ---------------- Top bar ----------------
    egui::Panel::top("top").show(&mut root, |ui| {
        ui.horizontal_wrapped(|ui| {
            let (y, m, d) = state.date();
            ui.label(egui::RichText::new(format!("Year {y}  Month {m:02}  Day {d:02}")).monospace().strong());
            ui.label(egui::RichText::new(format!("tick {}", state.tick)).weak().small());
            if state.config.game_length_ticks > 0 {
                let left = state.config.game_length_ticks.saturating_sub(state.tick);
                let months = left / state.config.ticks_per_month as u64;
                ui.label(format!("· {} mo left", months)).on_hover_text("Highest score (treasury + land value) when time runs out wins");
            }
            ui.separator();
            if driver.is_authority() {
                let label = if driver.paused { "▶ Resume" } else { "⏸ Pause" };
                if ui.button(label).on_hover_text("Space").clicked() {
                    driver.paused = !driver.paused;
                }
                for (name, s) in [("1x", 4), ("2x", 8), ("4x", 16), ("8x", 32)] {
                    if ui.selectable_label(driver.speed == s && !driver.paused, name).clicked() {
                        driver.speed = s;
                        driver.paused = false;
                    }
                }
            } else {
                ui.label("Speed set by host");
            }
            ui.separator();
            if let Some(p) = active.and_then(|a| state.player(a)) {
                let s = &p.stats;
                let net = s.taxes + s.trade - s.upkeep;
                ui.colored_label(color32(p.id), egui::RichText::new(&p.name).strong());
                ui.label(egui::RichText::new(fmt_money(p.treasury)).monospace().strong());
                let per_month = net * state.config.ticks_per_month as i64;
                let c = if net >= 0 { egui::Color32::LIGHT_GREEN } else { egui::Color32::LIGHT_RED };
                ui.colored_label(c, format!("{}{}/mo", if net >= 0 { "+" } else { "" }, fmt_money(per_month))).on_hover_text(format!(
                    "Per month: taxes {}  upkeep -{}  trade {}",
                    fmt_money(s.taxes * 30),
                    fmt_money(s.upkeep * 30),
                    fmt_money(s.trade * 30)
                ));
                ui.separator();
                demand_bar(ui, "R", s.demand[0], egui::Color32::from_rgb(120, 210, 110));
                demand_bar(ui, "C", s.demand[1], egui::Color32::from_rgb(110, 160, 240));
                demand_bar(ui, "I", s.demand[2], egui::Color32::from_rgb(230, 190, 80));
            }
            ui.separator();
            egui::ComboBox::from_id_salt("overlay").selected_text(format!("View: {}", view.overlay.label())).show_ui(ui, |ui| {
                for o in Overlay::ALL {
                    ui.selectable_value(&mut view.overlay, o, format!("{}  {}", o.key(), o.label()));
                }
            });
            ui.separator();
            if driver.is_authority() {
                if ui.button("Save").on_hover_text("F5").clicked() {
                    save_game(&driver, &mut toasts);
                }
                if ui.button("Load").on_hover_text("F9").clicked() {
                    load_game(&mut driver, &mut toasts);
                }
                if ui.button("Replay").on_hover_text("Export every command since the game began").clicked() {
                    export_replay(&driver, &mut toasts);
                }
            }
            if ui.button(if sfx.muted { "🔇" } else { "🔊" }).on_hover_text("M").clicked() {
                sfx.muted = !sfx.muted;
            }
            if ui.button("Menu").clicked() {
                next.set(AppState::Menu);
            }
        });
    });

    // ---------------- Tool palette ----------------
    egui::Panel::left("tools").exact_size(150.0).show(&mut root, |ui| {
        ui.heading("Build");
        for tool in Tool::all() {
            let cost = match tool {
                Tool::Build(b) => format!(" {}", fmt_money(state.config.cost.of(b))),
                Tool::Bulldoze => format!(" {}", fmt_money(state.config.bulldoze_cost)),
                Tool::Inspect => String::new(),
            };
            let text = egui::RichText::new(format!("{}  {}", tool.label(), cost.trim_end_matches(".00")));
            let resp = ui.selectable_label(ts.tool == tool, text).on_hover_text(format!("Hotkey: {}", tool.hotkey()));
            if resp.clicked() {
                ts.tool = tool;
            }
        }
        ui.separator();
        ui.small(match ts.tool {
            Tool::Build(Buildable::Road | Buildable::PowerLine | Buildable::WaterPipe) => "Drag to lay a line.",
            Tool::Build(Buildable::PowerPlant | Buildable::WaterPump) => "Click to place.",
            Tool::Inspect => "Click a tile to inspect it.",
            _ => "Drag to paint an area.",
        });
        ui.small("Right-click cancels.");
        ui.separator();
        ui.small("Zones conduct power and water to their neighbours. Lines ride on roads; pipes go anywhere on land.");
        ui.separator();
        ui.small("WASD / edge / middle-drag: pan\nWheel: zoom\n1-8: overlays\nTab: switch parcel owner\nSpace: pause  +/-: speed");
    });

    // ---------------- Right panel ----------------
    egui::Panel::right("info").exact_size(300.0).show(&mut root, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(p) = active.and_then(|a| state.player(a)) {
                ui.heading("Your neighbourhood");
                let s = &p.stats;
                egui::Grid::new("stats").num_columns(2).striped(true).show(ui, |ui| {
                    ui.label("Population");
                    ui.label(s.population.to_string());
                    ui.end_row();
                    ui.label("Jobs (C / I)");
                    ui.label(format!("{} / {}", s.commercial_jobs, s.industrial_jobs));
                    ui.end_row();
                    ui.label("Taxes / mo");
                    ui.label(fmt_money(s.taxes * state.config.ticks_per_month as i64));
                    ui.end_row();
                    ui.label("Upkeep / mo");
                    ui.label(fmt_money(-s.upkeep * state.config.ticks_per_month as i64));
                    ui.end_row();
                    ui.label("Trade / mo");
                    ui.label(fmt_money(s.trade * state.config.ticks_per_month as i64));
                    ui.end_row();
                    ui.label("Score");
                    ui.label(egui::RichText::new(fmt_money(s.score)).strong());
                    ui.end_row();
                });
                let mut tax = p.tax_rate;
                ui.horizontal(|ui| {
                    ui.label("Tax rate");
                    if ui.add(egui::Slider::new(&mut tax, 0..=state.config.max_tax_rate).suffix("%")).changed() {
                        actions.push((p.id, CommandKind::SetTaxRate { rate: tax }));
                    }
                });
                ui.separator();
                for (u, l, price) in [(Utility::Power, &s.power, p.power_price), (Utility::Water, &s.water, p.water_price)] {
                    let name = if u == Utility::Power { "⚡ Power" } else { "💧 Water" };
                    ui.label(egui::RichText::new(name).strong());
                    ui.label(format!("supply {}  demand {}  own use {}", l.supply, l.demand, l.own_use));
                    let short = l.unserved > 0;
                    ui.colored_label(
                        if short { egui::Color32::LIGHT_RED } else { egui::Color32::GRAY },
                        format!("bought {}  sold {}  unserved {}", l.bought, l.sold, l.unserved),
                    );
                    let mut selling = price.is_some();
                    let mut cents = price.unwrap_or(state.config.default_utility_price);
                    ui.horizontal(|ui| {
                        let a = ui.checkbox(&mut selling, "Sell surplus at").on_hover_text(
                            "Neighbours on the same network who are short buy your spare capacity, cheapest seller first",
                        );
                        let b = ui.add_enabled(selling, egui::DragValue::new(&mut cents).range(0..=100).suffix("¢/unit"));
                        if a.changed() || b.changed() {
                            actions.push((p.id, CommandKind::SetUtilityPrice { utility: u, price: selling.then_some(cents) }));
                        }
                    });
                }
                let trades: Vec<_> = state.trades.iter().filter(|t| t.buyer == p.id || t.seller == p.id).collect();
                if !trades.is_empty() {
                    ui.label(egui::RichText::new("Trades this tick").strong());
                    for t in trades {
                        let (dir, other) = if t.buyer == p.id { ("buy from", t.seller) } else { ("sell to", t.buyer) };
                        ui.label(format!(
                            "{} {} {:?} {} @ {}¢",
                            dir,
                            state.players[other.index()].name,
                            t.utility,
                            t.units,
                            t.price
                        ));
                    }
                }
                ui.separator();
            }

            ui.heading("Players");
            let mut ranking: Vec<_> = state.players.iter().filter(|p| p.controller != Controller::Vacant).collect();
            ranking.sort_by(|a, b| b.stats.score.cmp(&a.stats.score));
            egui::Grid::new("players").num_columns(4).striped(true).show(ui, |ui| {
                ui.label("");
                ui.label("Treasury");
                ui.label("Pop");
                ui.label("Score");
                ui.end_row();
                for p in ranking {
                    let tag = match p.controller {
                        Controller::Human if mine.contains(&p.id) => " (you)",
                        _ => "",
                    };
                    let short: String = p.name.chars().take(16).collect();
                    let name = egui::RichText::new(format!("{short}{tag}")).color(color32(p.id));
                    if ui.selectable_label(Some(p.id) == active, name).clicked() && mine.contains(&p.id) {
                        view.active = Some(p.id);
                    }
                    ui.label(fmt_money(p.treasury));
                    ui.label(p.stats.population.to_string());
                    ui.label(fmt_money(p.stats.score));
                    ui.end_row();
                }
            });
            if !driver.is_networked() {
                ui.collapsing("Who controls each parcel", |ui| {
                    ui.small("Hand parcels to the AI or take them over yourself (hot-seat).");
                    for p in &state.players {
                        ui.horizontal(|ui| {
                            ui.colored_label(color32(p.id), format!("Parcel {}", p.id.0 + 1));
                            let mut c = p.controller.clone();
                            egui::ComboBox::from_id_salt(("ctl", p.id.0))
                                .selected_text(match c {
                                    Controller::Human => "Human",
                                    Controller::Ai(AiStrategy::UtilityBaron) => "AI: utility baron",
                                    Controller::Ai(AiStrategy::Developer) => "AI: developer",
                                    Controller::Vacant => "Empty",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut c, Controller::Human, "Human");
                                    ui.selectable_value(&mut c, Controller::Ai(AiStrategy::UtilityBaron), "AI: utility baron");
                                    ui.selectable_value(&mut c, Controller::Ai(AiStrategy::Developer), "AI: developer");
                                    ui.selectable_value(&mut c, Controller::Vacant, "Empty");
                                });
                            if c != p.controller {
                                let base = p.name.trim_end_matches(" (AI)").to_string();
                                let name = if matches!(c, Controller::Ai(_)) { format!("{base} (AI)") } else { base };
                                system_actions.push(CommandKind::SetController { player: p.id, controller: c, name });
                            }
                        });
                    }
                });
            }
            ui.separator();
            inspector(ui, &state, ts.selected.or(ts.hover));
        });
    });

    // ---------------- Ghost preview tooltip ----------------
    if let (Some(p), Some(pointer)) = (&ts.preview, ctx.pointer_hover_pos()) {
        egui::Area::new("preview".into()).fixed_pos(pointer + egui::vec2(18.0, 14.0)).interactable(false).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| match &p.result {
                Ok(plan) => {
                    ui.label(format!("{} tile(s): {}", plan.tiles.len(), fmt_money(plan.cost)));
                    if p.clipped {
                        ui.small("Clipped to your parcel");
                    }
                }
                Err(why) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, why);
                }
            });
        });
    }

    // ---------------- Toasts ----------------
    egui::Area::new("toasts".into()).anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(160.0, -12.0)).interactable(false).show(ctx, |ui| {
        for t in &toasts.0 {
            let c = if t.warn { egui::Color32::from_rgb(255, 140, 120) } else { egui::Color32::from_rgb(220, 230, 255) };
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.colored_label(c, &t.text);
            });
        }
    });

    // ---------------- Game over ----------------
    if let Some(out) = &state.outcome {
        egui::Window::new("Time's up!").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
            ui.label("Final standings (treasury + land value):");
            for (i, (pid, score)) in out.ranking.iter().enumerate() {
                let p = &state.players[pid.index()];
                let medal = ["🥇", "🥈", "🥉"].get(i).copied().unwrap_or("  ");
                ui.colored_label(color32(*pid), format!("{medal} {}  {}", p.name, fmt_money(*score)));
            }
            if ui.button("Back to menu").clicked() {
                next.set(AppState::Menu);
            }
        });
    }
    if driver.paused {
        egui::Area::new("paused".into()).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0)).interactable(false).show(ctx, |ui| {
            ui.label(egui::RichText::new("PAUSED").size(22.0).strong().color(egui::Color32::YELLOW));
        });
    }

    for (author, kind) in actions {
        driver.submit(author, kind);
    }
    for kind in system_actions {
        let _ = driver.submit_system(kind);
    }
    Ok(())
}

fn inspector(ui: &mut egui::Ui, state: &GameState, at: Option<parcels_sim::Pos>) {
    ui.heading("Tile");
    let Some(pos) = at else {
        ui.weak("Hover a tile");
        return;
    };
    let t = state.map.tile(pos);
    let idx = state.map.idx(pos);
    let owner = state.owner_at(pos).and_then(|o| state.player(o));
    egui::Grid::new("tile").num_columns(2).show(ui, |ui| {
        ui.label("Position");
        ui.label(format!("{}, {}", pos.x, pos.y));
        ui.end_row();
        ui.label("Parcel");
        match owner {
            Some(o) => ui.colored_label(color32(o.id), format!("{} — {}", t.parcel.0 + 1, o.name)),
            None => ui.label("-"),
        };
        ui.end_row();
        ui.label("Kind");
        let mut kind = match t.kind {
            TileKind::Empty if t.terrain == parcels_sim::Terrain::Water => "River".to_string(),
            TileKind::Road if t.terrain == parcels_sim::Terrain::Water => "Bridge".to_string(),
            k => format!("{k:?}"),
        };
        if t.kind.is_zone() {
            kind.push_str(&format!(" (level {})", t.level));
        }
        if t.wire {
            kind.push_str(" + power line");
        }
        if t.pipe {
            kind.push_str(" + pipe");
        }
        ui.label(kind);
        ui.end_row();
        if t.kind.is_zone() {
            ui.label(if t.kind == TileKind::Residential { "Residents" } else { "Jobs" });
            ui.label(state.occupants(idx).to_string());
            ui.end_row();
        }
        ui.label("Power / water");
        ui.label(format!("{} / {}", if t.powered { "yes" } else { "no" }, if t.watered { "yes" } else { "no" }));
        ui.end_row();
        ui.label("Land value");
        ui.label(t.land_value.to_string());
        ui.end_row();
        ui.label("Pollution");
        ui.label(t.pollution.to_string());
        ui.end_row();
        ui.label("Traffic");
        ui.label(t.traffic.to_string());
        ui.end_row();
    });
}
