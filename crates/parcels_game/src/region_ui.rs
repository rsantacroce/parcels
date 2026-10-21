//! The region map: every plot of the region as a card, founded cities as
//! thumbnails with what links them, and a side card to found or play a city.

use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use parcels_sim::{era, fmt_money, CommandKind, Side};

use crate::driver::Driver;
use crate::icons::{self, pal, Icon};
use crate::menu::theme;
use crate::region::{Region, Slot};
use crate::settings::Settings;
use crate::ui::Toasts;
use crate::AppState;

/// The region being looked at.
#[derive(Resource)]
pub struct RegionView {
    pub region: Region,
    pub selected: Option<(u8, u8)>,
    pub new_name: String,
    pub error: Option<String>,
    thumbs: BTreeMap<(u8, u8), egui::TextureHandle>,
}

impl RegionView {
    pub fn new(region: Region) -> Self {
        Self { region, selected: None, new_name: String::new(), error: None, thumbs: BTreeMap::new() }
    }

    pub fn open(slug: &str) -> Result<Self, String> {
        Region::load(slug).map(Self::new)
    }
}

pub struct RegionPlugin;

impl Plugin for RegionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Region), enter_region)
            .add_systems(EguiPrimaryContextPass, region_ui.run_if(in_state(AppState::Region)));
    }
}

/// The city on screen belongs to the game, not the region map.
fn enter_region(mut commands: Commands, driver: Option<Res<Driver>>) {
    if driver.is_some() {
        commands.remove_resource::<Driver>();
    }
}

/// Start playing plot (x, y): found a city there if it's empty, otherwise load
/// it, and hand it the latest news from its neighbours.
pub fn open_plot(region: &mut Region, x: u8, y: u8, name: &str) -> Result<Driver, String> {
    let founding = region.plot(x, y).is_none();
    let state = if founding { region.found(x, y, name) } else { region.load_city(x, y)? };
    if founding {
        region.store(x, y, &state)?;
    }
    let mut d = Driver::new(state);
    d.save_name = Some(d.session.state.city.name.clone());
    d.region = Some(Slot { region: region.slug(), x, y });
    d.submit(CommandKind::SetNeighbours { neighbours: region.neighbours_of(x, y) });
    Ok(d)
}

fn thumb(ctx: &egui::Context, view: &mut RegionView, x: u8, y: u8) -> Option<egui::TextureHandle> {
    if let Some(t) = view.thumbs.get(&(x, y)) {
        return Some(t.clone());
    }
    let city = view.region.plot(x, y)?;
    let (w, h) = (city.thumb_size.0 as usize, city.thumb_size.1 as usize);
    if w * h * 3 != city.thumb.len() {
        return None;
    }
    let img = egui::ColorImage::from_rgb([w, h], &city.thumb);
    let tex = ctx.load_texture(format!("plot-{x}-{y}"), img, egui::TextureOptions::NEAREST);
    view.thumbs.insert((x, y), tex.clone());
    Some(tex)
}

fn region_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut view: ResMut<RegionView>,
    settings: Res<Settings>,
    mut next: ResMut<NextState<AppState>>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    theme(&ctx, &settings);
    let mut go: Option<Result<Driver, String>> = None;
    let mut back = false;

    let mut root = egui::Ui::new(ctx.clone(), "region".into(), egui::UiBuilder::new().layer_id(egui::LayerId::background()).max_rect(ctx.viewport_rect()));
    egui::CentralPanel::default().frame(egui::Frame::new().fill(pal::PAPER_DIM).inner_margin(20)).show(&mut root, |ui| {
        let r = &view.region;
        ui.horizontal(|ui| {
            icons::inline(ui, Icon::Map, 34.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(&r.name).size(26.0).strong());
                let cities = r.plots.iter().flatten().count();
                ui.label(
                    egui::RichText::new(format!(
                        "Region · {cities} of {} plots settled · {} residents",
                        r.plots.len(),
                        r.population()
                    ))
                    .color(pal::INK_SOFT),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icons::menu_button(ui, Icon::Quit, "Title screen", 170.0, 38.0, false).clicked() {
                    back = true;
                }
            });
        });
        ui.add_space(10.0);
        let (cols, rows) = (view.region.cols as f32, view.region.rows as f32);
        let aspect = view.region.map.1 as f32 / view.region.map.0 as f32;

        ui.horizontal_top(|ui| {
            // ---- The grid ----
            let avail = ui.available_size() - egui::vec2(360.0, 0.0);
            let gap = 14.0;
            let cell_w = ((avail.x - gap * (cols - 1.0)) / cols).min((avail.y - gap * (rows - 1.0)) / rows / aspect).max(80.0);
            let cell = egui::vec2(cell_w, cell_w * aspect);
            let (area, _) = ui.allocate_exact_size(egui::vec2(cell.x * cols + gap * (cols - 1.0), cell.y * rows + gap * (rows - 1.0)), egui::Sense::hover());
            let at = |x: u8, y: u8| egui::Rect::from_min_size(area.min + egui::vec2(x as f32 * (cell.x + gap), y as f32 * (cell.y + gap)), cell);

            for y in 0..view.region.rows {
                for x in 0..view.region.cols {
                    let rect = at(x, y);
                    let resp = ui.interact(rect, ui.id().with(("plot", x, y)), egui::Sense::click());
                    let selected = view.selected == Some((x, y));
                    let tex = thumb(&ctx, &mut view, x, y);
                    let p = ui.painter();
                    p.rect_filled(rect.translate(egui::vec2(0.0, 3.0)), 12.0, pal::PAPER_DEEP);
                    match (view.region.plot(x, y), tex) {
                        (Some(city), Some(tex)) => {
                            p.image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                            let chip = egui::Rect::from_min_size(rect.left_bottom() + egui::vec2(8.0, -42.0), egui::vec2(rect.width() - 16.0, 34.0));
                            p.rect_filled(chip, 8.0, pal::NIGHT.gamma_multiply(0.85));
                            p.text(chip.left_center() + egui::vec2(10.0, -7.0), egui::Align2::LEFT_CENTER, &city.name, egui::FontId::proportional(15.0), egui::Color32::WHITE);
                            p.text(
                                chip.left_center() + egui::vec2(10.0, 8.0),
                                egui::Align2::LEFT_CENTER,
                                format!("{} residents · {} era", city.summary.population, era::name(city.era)),
                                egui::FontId::proportional(11.5),
                                egui::Color32::from_gray(220),
                            );
                        }
                        _ => {
                            let fill = if resp.hovered() { egui::Color32::from_rgb(170, 190, 130) } else { egui::Color32::from_rgb(150, 172, 112) };
                            p.rect_filled(rect, 12.0, fill);
                            p.text(rect.center() - egui::vec2(0.0, 8.0), egui::Align2::CENTER_CENTER, "Open land", egui::FontId::proportional(16.0), pal::PAPER);
                            p.text(rect.center() + egui::vec2(0.0, 12.0), egui::Align2::CENTER_CENTER, "click to found a city", egui::FontId::proportional(13.0), pal::PAPER);
                        }
                    }
                    let stroke = if selected { egui::Stroke::new(4.0, pal::ACCENT) } else if resp.hovered() { egui::Stroke::new(2.5, pal::BORDER) } else { egui::Stroke::new(1.5, pal::INK_SOFT) };
                    p.rect_stroke(rect, 12.0, stroke, egui::StrokeKind::Inside);
                    if resp.clicked() {
                        view.selected = Some((x, y));
                        view.error = None;
                    }
                }
            }

            // What crosses each border between two plots.
            let p = ui.painter();
            for y in 0..view.region.rows {
                for x in 0..view.region.cols {
                    for side in [Side::East, Side::South] {
                        let Some((nx, ny)) = view.region.beside(x, y, side) else { continue };
                        let a = view.region.plot(x, y).map(|c| c.links[side.index()]).unwrap_or_default();
                        let b = view.region.plot(nx, ny).map(|c| c.links[side.opposite().index()]).unwrap_or_default();
                        let icons: Vec<Icon> = [
                            (a.road || b.road, Icon::Road),
                            (a.rail || b.rail, Icon::Rail),
                            (a.power || b.power, Icon::PowerLine),
                            (a.water || b.water, Icon::Pipe),
                        ]
                            .into_iter()
                            .filter_map(|(on, i)| on.then_some(i))
                            .collect();
                        if icons.is_empty() {
                            continue;
                        }
                        let (ra, rb) = (at(x, y), at(nx, ny));
                        let mid = if side == Side::East {
                            egui::pos2((ra.right() + rb.left()) / 2.0, ra.center().y)
                        } else {
                            egui::pos2(ra.center().x, (ra.bottom() + rb.top()) / 2.0)
                        };
                        let n = icons.len() as f32;
                        let size = 26.0;
                        let span = if side == Side::East { egui::vec2(0.0, size + 4.0) } else { egui::vec2(size + 4.0, 0.0) };
                        for (k, icon) in icons.into_iter().enumerate() {
                            let c = mid + span * (k as f32 - (n - 1.0) / 2.0);
                            p.circle_filled(c, size / 2.0 + 3.0, pal::PAPER);
                            p.circle_stroke(c, size / 2.0 + 3.0, (1.5, pal::BORDER));
                            icons::paint_in(p, egui::Rect::from_center_size(c, egui::vec2(size, size)), size - 6.0, icon, pal::PAPER);
                        }
                    }
                }
            }

            ui.add_space(16.0);

            // ---- The selected plot ----
            egui::Frame::new()
                .fill(pal::PAPER)
                .stroke(egui::Stroke::new(2.0, pal::BORDER))
                .corner_radius(14)
                .inner_margin(14)
                .show(ui, |ui| ui.vertical(|ui| {
                    ui.set_width(300.0);
                    let Some((x, y)) = view.selected else {
                        ui.label(egui::RichText::new("Pick a plot").size(18.0).strong());
                        ui.label("Each plot is its own city map. Found a city on open land, or open one you've built.");
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(
                                "Neighbouring cities connect where a road, power line or water pipe reaches the edge of the map facing them: \
                                 people commute and shop across the border, and spare power and water can be sold.",
                            )
                            .small()
                            .color(pal::INK_SOFT),
                        );
                        return;
                    };
                    let neighbours = view.region.neighbours_of(x, y);
                    match view.region.plot(x, y).cloned() {
                        Some(city) => {
                            ui.horizontal(|ui| {
                                icons::inline(ui, Icon::City, 24.0);
                                ui.label(egui::RichText::new(&city.name).size(20.0).strong());
                            });
                            egui::Grid::new("plot_city").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                                for (k, v) in [
                                    ("Residents", city.summary.population.to_string()),
                                    ("Era", era::name(city.era).to_string()),
                                    ("Year", city.year.to_string()),
                                    ("Approval", format!("{}%", city.approval)),
                                    ("Treasury", fmt_money(city.treasury)),
                                    ("Open jobs", city.summary.open_jobs.to_string()),
                                    ("Spare power / water", format!("{} / {}", city.summary.power_spare, city.summary.water_spare)),
                                ] {
                                    ui.label(k);
                                    ui.label(egui::RichText::new(v).strong());
                                    ui.end_row();
                                }
                            });
                            ui.add_space(8.0);
                            if icons::menu_button(ui, Icon::Play, "Play this city", 300.0, 42.0, true).clicked() {
                                go = Some(open_plot(&mut view.region, x, y, ""));
                            }
                        }
                        None => {
                            ui.horizontal(|ui| {
                                icons::inline(ui, Icon::Parks, 24.0);
                                ui.label(egui::RichText::new("Open land").size(20.0).strong());
                            });
                            ui.label(format!("A {}×{} plot.", view.region.map.0, view.region.map.1));
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label("Name");
                                ui.add(egui::TextEdit::singleline(&mut view.new_name).hint_text("New City").desired_width(220.0));
                            });
                            ui.add_space(6.0);
                            if icons::menu_button(ui, Icon::Sparkle, "Found a city here", 300.0, 42.0, true).clicked() {
                                let name = view.new_name.clone();
                                go = Some(open_plot(&mut view.region, x, y, &name));
                            }
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("NEIGHBOURS").size(11.5).strong().color(pal::INK_SOFT));
                    let mut any = false;
                    for side in Side::ALL {
                        if let Some(n) = &neighbours[side.index()] {
                            any = true;
                            ui.label(format!("{} to the {} · {} residents", n.name, side.name(), n.population));
                        }
                    }
                    if !any {
                        ui.label(egui::RichText::new("None yet.").color(pal::INK_SOFT));
                    }
                    if let Some(e) = &view.error {
                        ui.colored_label(pal::BAD, e);
                    }
                }));
        });
    });

    if back {
        commands.remove_resource::<RegionView>();
        next.set(AppState::Menu);
    }
    match go {
        Some(Ok(d)) => {
            commands.insert_resource(d);
            commands.insert_resource(Toasts::default());
            next.set(AppState::Playing);
        }
        Some(Err(e)) => view.error = Some(e),
        None => {}
    }
    Ok(())
}
