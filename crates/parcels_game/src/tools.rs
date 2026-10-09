//! Build tools: pick a tool, click or drag to paint, see a ghost preview with the
//! cost (or the reason it can't be built) before committing. In street view the
//! crosshair does the pointing.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::input::EguiWantsInput;
use parcels_sim::systems::apply::{plan_bulldoze, plan_place, Plan};
use parcels_sim::{
    Area, Building, Buildable, Category, CommandKind, Density, GameState, ParcelId, PlayerId, Pos, Rect, Rejection, Road, Zone,
};

use crate::audio::{Sfx, SfxQueue};
use crate::camera::{CamMode, MainCamera, RightDrag};
use crate::driver::Driver;
use crate::ui::Toasts;
use crate::view::{player_color, Overlay, View};
use crate::world::meshgen::TILE;
use crate::AppState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Inspect,
    Build(Buildable),
    Bulldoze,
}

impl Tool {
    pub fn label(self) -> String {
        match self {
            Tool::Inspect => "Inspect".into(),
            Tool::Build(b) => b.name(),
            Tool::Bulldoze => "Bulldoze".into(),
        }
    }

    fn shape(self) -> Shape {
        match self {
            Tool::Build(Buildable::Road(_) | Buildable::PowerLine | Buildable::WaterPipe) => Shape::Line,
            Tool::Build(Buildable::Building(b)) if b.size() > 1 => Shape::Single,
            Tool::Build(Buildable::Building(b)) if !matches!(b.category(), Category::Parks) && b != Building::WindTurbine => {
                Shape::Single
            }
            Tool::Inspect => Shape::Single,
            _ => Shape::Rect,
        }
    }
}

/// Hotkey letter for a tool, if it has one.
pub fn hotkey(tool: Tool) -> Option<&'static str> {
    Some(match tool {
        Tool::Inspect => "Esc",
        Tool::Bulldoze => "B",
        Tool::Build(Buildable::Road(_)) => "R",
        Tool::Build(Buildable::Zone(Zone::Residential, _)) => "Z",
        Tool::Build(Buildable::Zone(Zone::Commercial, _)) => "X",
        Tool::Build(Buildable::Zone(Zone::Industrial, _)) => "C",
        Tool::Build(Buildable::Zone(Zone::Office, _)) => "O",
        Tool::Build(Buildable::PowerLine) => "L",
        Tool::Build(Buildable::WaterPipe) => "P",
        Tool::Build(Buildable::Building(b)) => match b.category() {
            Category::Power => "G",
            Category::Water => "U",
            Category::Services => "J",
            Category::Parks => "K",
            Category::Landmarks => "N",
            _ => return None,
        },
    })
}

enum Shape {
    Single,
    Line,
    Rect,
}

pub struct Preview {
    pub tiles: Vec<Pos>,
    pub author: Option<PlayerId>,
    pub parcel: Option<ParcelId>,
    pub result: Result<Plan, String>,
    pub clipped: bool,
}

#[derive(Resource, Default)]
pub struct ToolState {
    pub tool: Tool,
    pub hover: Option<Pos>,
    pub selected: Option<Pos>,
    drag_start: Option<Pos>,
    pub preview: Option<Preview>,
    /// Last-used item per category, so category buttons reopen it.
    pub last: Vec<(Category, Buildable)>,
}

impl ToolState {
    pub fn set(&mut self, tool: Tool) {
        self.tool = tool;
        self.drag_start = None;
        if let Tool::Build(b) = tool {
            let c = b.category();
            self.last.retain(|(k, _)| *k != c);
            self.last.push((c, b));
        }
    }

    pub fn last_in(&self, c: Category) -> Option<Buildable> {
        self.last.iter().find(|(k, _)| *k == c).map(|(_, b)| *b)
    }
}

pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ToolState>().add_systems(
            Update,
            (hotkeys, track_hover, build_preview, click, draw_gizmos).chain().run_if(in_state(AppState::Playing)),
        );
    }
}

/// Every buildable in a category, in toolbar order.
pub fn items(c: Category) -> Vec<Buildable> {
    Buildable::all().into_iter().filter(|b| b.category() == c).collect()
}

/// Pressing a category key again steps to the next item in it.
fn cycle(current: Tool, c: Category) -> Tool {
    let list = items(c);
    let next = match current {
        Tool::Build(b) => list.iter().position(|&x| x == b).map(|i| list[(i + 1) % list.len()]),
        _ => None,
    };
    Tool::Build(next.unwrap_or(list[0]))
}

#[allow(clippy::too_many_arguments)]
fn hotkeys(keys: Res<ButtonInput<KeyCode>>, egui: Res<EguiWantsInput>, mut ts: ResMut<ToolState>, mut view: ResMut<View>, driver: Res<Driver>) {
    if egui.wants_any_keyboard_input() {
        return;
    }
    let zone_key = |ts: &ToolState, z: Zone| match ts.tool {
        // Second press switches between low and high density.
        Tool::Build(Buildable::Zone(z0, Density::Low)) if z0 == z => Tool::Build(Buildable::Zone(z, Density::High)),
        _ => Tool::Build(Buildable::Zone(z, Density::Low)),
    };
    let pick = [
        (KeyCode::Escape, Tool::Inspect),
        (KeyCode::KeyI, Tool::Inspect),
        (KeyCode::KeyB, Tool::Bulldoze),
        (
            KeyCode::KeyR,
            if ts.tool == Tool::Build(Buildable::Road(Road::Street)) {
                Tool::Build(Buildable::Road(Road::Avenue))
            } else {
                Tool::Build(Buildable::Road(Road::Street))
            },
        ),
        (KeyCode::KeyZ, zone_key(&ts, Zone::Residential)),
        (KeyCode::KeyX, zone_key(&ts, Zone::Commercial)),
        (KeyCode::KeyC, zone_key(&ts, Zone::Industrial)),
        (KeyCode::KeyO, zone_key(&ts, Zone::Office)),
        (KeyCode::KeyL, Tool::Build(Buildable::PowerLine)),
        (KeyCode::KeyP, Tool::Build(Buildable::WaterPipe)),
        (KeyCode::KeyG, cycle(ts.tool, Category::Power)),
        (KeyCode::KeyU, cycle(ts.tool, Category::Water)),
        (KeyCode::KeyJ, cycle(ts.tool, Category::Services)),
        (KeyCode::KeyK, cycle(ts.tool, Category::Parks)),
        (KeyCode::KeyN, cycle(ts.tool, Category::Landmarks)),
    ];
    for (k, t) in pick {
        if keys.just_pressed(k) {
            // Power/water cycles must skip lines and pipes, which have their own keys.
            let t = match t {
                Tool::Build(Buildable::PowerLine) if k == KeyCode::KeyG => cycle(t, Category::Power),
                Tool::Build(Buildable::WaterPipe) if k == KeyCode::KeyU => cycle(t, Category::Water),
                t => t,
            };
            ts.set(t);
            if t == Tool::Inspect && k == KeyCode::Escape {
                ts.selected = None;
            }
        }
    }
    let overlays = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
        KeyCode::Digit0,
    ];
    for (i, k) in overlays.into_iter().enumerate() {
        if keys.just_pressed(k) {
            let o = Overlay::ALL[i];
            view.overlay = if view.overlay == o { Overlay::None } else { o };
        }
    }
    view.show_pipes = matches!(ts.tool, Tool::Build(Buildable::WaterPipe))
        || matches!(ts.tool, Tool::Build(b) if b.category() == Category::Water);
    // Hot-seat: cycle which of our players we're acting as.
    if keys.just_pressed(KeyCode::Tab) {
        let mine = driver.controllable();
        if !mine.is_empty() {
            let i = view.active.and_then(|a| mine.iter().position(|&p| p == a)).map_or(0, |i| (i + 1) % mine.len());
            view.active = Some(mine[i]);
        }
    }
    if view.active.is_none_or(|a| !driver.controllable().contains(&a)) {
        view.active = driver.controllable().first().copied();
    }
}

fn track_hover(
    driver: Res<Driver>,
    window: Single<&Window, With<PrimaryWindow>>,
    cam: Single<(&Camera, &GlobalTransform, &CamMode), With<MainCamera>>,
    egui: Res<EguiWantsInput>,
    mut ts: ResMut<ToolState>,
) {
    let Some(state) = driver.state() else { return };
    let (camera, cam_tf, mode) = *cam;
    let street = matches!(mode, CamMode::Street { .. });
    if egui.is_pointer_over_area() && !street {
        ts.hover = None;
        return;
    }
    let point = if street {
        Some(Vec2::new(window.width(), window.height()) / 2.0)
    } else {
        window.cursor_position()
    };
    let max_reach = if street { 120.0 } else { f32::MAX };
    ts.hover = point
        .and_then(|c| camera.viewport_to_world(cam_tf, c).ok())
        .and_then(|ray| {
            let d = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
            (d <= max_reach).then(|| ray.get_point(d))
        })
        .and_then(|g| {
            let (x, y) = (g.x / TILE, g.z / TILE);
            (x >= 0.0 && y >= 0.0 && x < state.map.width as f32 && y < state.map.height as f32).then(|| Pos::new(x as u16, y as u16))
        });
}

fn line(a: Pos, b: Pos) -> Vec<Pos> {
    let mut v = Vec::new();
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    for x in x0..=x1 {
        v.push(Pos::new(x, a.y));
    }
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    for y in y0..=y1 {
        if y != a.y {
            v.push(Pos::new(b.x, y));
        }
    }
    v
}

/// Anchor (min corner) that centres a `size` footprint on `at`.
fn centred(state: &GameState, at: Pos, size: u16) -> Pos {
    let back = (size - 1) / 2;
    let x = at.x.saturating_sub(back).min(state.map.width.saturating_sub(size));
    let y = at.y.saturating_sub(back).min(state.map.height.saturating_sub(size));
    Pos::new(x, y)
}

fn make_preview(state: &GameState, mine: &[PlayerId], tool: Tool, start: Pos, end: Pos) -> Option<Preview> {
    if tool == Tool::Inspect {
        return None;
    }
    let tiles = match tool.shape() {
        Shape::Single => match tool {
            Tool::Build(b) => vec![centred(state, end, b.size())],
            _ => vec![end],
        },
        Shape::Line => line(start, end),
        Shape::Rect => Rect::from_corners(start, end).iter().collect(),
    };
    let anchor = if matches!(tool.shape(), Shape::Single) { tiles[0] } else { start };
    let parcel = state.map.get(anchor).map(|t| t.parcel)?;
    let pc = state.parcel(parcel)?;
    let owner = pc.owner;
    if !mine.contains(&owner) {
        let who = &state.players[owner.index()].name;
        return Some(Preview {
            tiles,
            author: None,
            parcel: Some(parcel),
            result: Err(format!("Parcel {} belongs to {who}", parcel.0 + 1)),
            clipped: false,
        });
    }
    // Drags that wander over the fence are clipped to the parcel they started in.
    let before = tiles.len();
    let tiles: Vec<Pos> = tiles.into_iter().filter(|&p| pc.rect.contains(p)).collect();
    let clipped = tiles.len() != before;
    let area = Area::Tiles(tiles.clone());
    let result = match tool {
        Tool::Build(b) => plan_place(state, owner, parcel, &area, b),
        Tool::Bulldoze => plan_bulldoze(state, owner, parcel, &area),
        Tool::Inspect => unreachable!(),
    };
    let result = result.map_err(|r: Rejection| match r {
        Rejection::NothingToDo => match tool {
            Tool::Bulldoze => "Nothing to bulldoze here".to_string(),
            Tool::Build(Buildable::Building(b)) if b.size() > 1 => {
                format!("{} needs {}×{} tiles of clear land", b.name(), b.size(), b.size())
            }
            Tool::Build(b) => format!("{} can't go here", b.name()),
            _ => r.to_string(),
        },
        Rejection::OutsideParcel(_) if matches!(tool, Tool::Build(Buildable::Building(_))) => {
            "Doesn't fit inside your parcel".to_string()
        }
        other => other.to_string(),
    });
    Some(Preview { tiles, author: Some(owner), parcel: Some(parcel), result, clipped })
}

fn build_preview(driver: Res<Driver>, mut ts: ResMut<ToolState>) {
    let Some(state) = driver.state() else { return };
    let mine = driver.controllable();
    let preview = ts.hover.and_then(|h| make_preview(state, &mine, ts.tool, ts.drag_start.unwrap_or(h), h));
    ts.preview = preview;
}

#[allow(clippy::too_many_arguments)]
fn click(
    buttons: Res<ButtonInput<MouseButton>>,
    egui: Res<EguiWantsInput>,
    drag: Res<RightDrag>,
    mode: Single<&CamMode, With<MainCamera>>,
    mut driver: ResMut<Driver>,
    mut ts: ResMut<ToolState>,
    mut view: ResMut<View>,
    mut toasts: ResMut<Toasts>,
    mut sfx: ResMut<SfxQueue>,
) {
    let street = matches!(**mode, CamMode::Street { .. });
    let ui = egui.is_pointer_over_area() && !street;
    if buttons.just_released(MouseButton::Right) && drag.travelled <= 4.0 && !ui {
        if ts.drag_start.is_some() {
            ts.drag_start = None;
        } else {
            ts.set(Tool::Inspect);
        }
        return;
    }
    if buttons.just_pressed(MouseButton::Left) && !ui {
        if let Some(h) = ts.hover {
            if ts.tool == Tool::Inspect {
                ts.selected = Some(h);
            } else {
                ts.drag_start = Some(h);
            }
        }
    }
    if buttons.just_released(MouseButton::Left) && ts.drag_start.is_some() {
        ts.drag_start = None;
        let Some(p) = ts.preview.take() else { return };
        match (&p.result, p.author, p.parcel) {
            (Ok(plan), Some(author), Some(parcel)) => {
                let tiles = if plan.footprints.is_empty() { plan.tiles.clone() } else { plan.footprints.iter().map(|f| f.min).collect() };
                let area = Area::Tiles(tiles);
                let kind = match ts.tool {
                    Tool::Build(what) => CommandKind::Place { parcel, area, what },
                    _ => CommandKind::Bulldoze { parcel, area },
                };
                driver.submit(author, kind);
                view.active = Some(author);
                sfx.push(if ts.tool == Tool::Bulldoze { Sfx::Bulldoze } else { Sfx::Place });
            }
            (Err(why), _, _) => {
                toasts.warn(why.clone());
                sfx.push(Sfx::Error);
            }
            _ => {}
        }
    }
}

fn outline(gizmos: &mut Gizmos, r: Rect, inset: f32, y: f32, color: Color) {
    let (x0, z0) = (r.min.x as f32 * TILE + inset, r.min.y as f32 * TILE + inset);
    let (x1, z1) = ((r.max.x + 1) as f32 * TILE - inset, (r.max.y + 1) as f32 * TILE - inset);
    gizmos.linestrip(
        [Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), Vec3::new(x1, y, z1), Vec3::new(x0, y, z1), Vec3::new(x0, y, z0)],
        color,
    );
}

/// Rough height of a building, for the ghost preview box.
fn ghost_height(b: Buildable) -> f32 {
    match b {
        Buildable::Building(b) => match b {
            Building::WindTurbine => 40.0,
            Building::CoalPlant | Building::NuclearPlant => 25.0,
            Building::Hospital | Building::University | Building::TownHall => 22.0,
            Building::Park | Building::Plaza | Building::Playground | Building::SportsField => 4.0,
            _ => 12.0,
        },
        _ => 1.0,
    }
}

fn draw_gizmos(mut gizmos: Gizmos, driver: Res<Driver>, ts: Res<ToolState>, view: Res<View>) {
    let Some(state) = driver.state() else { return };
    // Parcel borders in each owner's colour; ours drawn doubled.
    for pc in &state.parcels {
        let [r, g, b] = player_color(pc.owner);
        let color = Color::srgba_u8(r, g, b, 230);
        outline(&mut gizmos, pc.rect, 0.3, 0.5, color);
        if Some(pc.owner) == view.active {
            outline(&mut gizmos, pc.rect, 1.2, 0.5, color);
        }
    }
    if let Some(p) = &ts.preview {
        let color = match &p.result {
            Ok(_) => Color::srgba(0.3, 1.0, 0.4, 0.95),
            Err(_) => Color::srgba(1.0, 0.25, 0.2, 0.95),
        };
        match (&p.result, ts.tool) {
            (Ok(plan), Tool::Build(b @ Buildable::Building(_))) => {
                for fp in &plan.footprints {
                    let size = Vec3::new(fp.width() as f32 * TILE - 1.0, ghost_height(b), fp.height() as f32 * TILE - 1.0);
                    let c = Vec3::new((fp.min.x + fp.max.x + 1) as f32, 0.0, (fp.min.y + fp.max.y + 1) as f32) * TILE / 2.0;
                    gizmos.cube(Transform::from_translation(c + Vec3::Y * size.y / 2.0).with_scale(size), color);
                }
            }
            (Err(_), Tool::Build(b @ Buildable::Building(bb))) => {
                let fp = Rect::square(p.tiles.first().copied().unwrap_or(Pos::new(0, 0)), bb.size());
                let size = Vec3::new(fp.width() as f32 * TILE - 1.0, ghost_height(b), fp.height() as f32 * TILE - 1.0);
                let c = Vec3::new((fp.min.x + fp.max.x + 1) as f32, 0.0, (fp.min.y + fp.max.y + 1) as f32) * TILE / 2.0;
                gizmos.cube(Transform::from_translation(c + Vec3::Y * size.y / 2.0).with_scale(size), color);
            }
            (result, _) => {
                let tiles: &[Pos] = match result {
                    Ok(plan) => &plan.tiles,
                    Err(_) => &p.tiles,
                };
                for &t in tiles.iter().take(4096) {
                    outline(&mut gizmos, Rect { min: t, max: t }, 0.8, 0.35, color);
                }
            }
        }
    }
    if let Some(h) = ts.hover {
        outline(&mut gizmos, state.map.footprint_at(h), 0.0, 0.3, Color::WHITE);
    }
    if let Some(s) = ts.selected.filter(|&s| state.map.in_bounds(s)) {
        outline(&mut gizmos, state.map.footprint_at(s), -0.6, 0.3, Color::srgb(1.0, 0.9, 0.2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parcels_sim::{Config, Controller, NewGame, PlayerSetup, Terrain, TerrainSettings};

    fn game() -> GameState {
        let mut config = Config::default();
        config.map_width = 32;
        config.map_height = 16;
        let players = vec![
            PlayerSetup { name: "Me".into(), controller: Controller::Human },
            PlayerSetup { name: "Them".into(), controller: Controller::Human },
        ];
        let mut s = GameState::new(&NewGame { seed: 1, config, players, terrain: TerrainSettings::default() });
        for t in &mut s.map.tiles {
            t.terrain = Terrain::Land;
        }
        s
    }

    #[test]
    fn line_is_l_shaped_and_contiguous() {
        let l = line(Pos::new(2, 2), Pos::new(5, 6));
        assert_eq!(l.len(), 4 + 4);
        assert_eq!(l[0], Pos::new(2, 2));
        assert_eq!(*l.last().unwrap(), Pos::new(5, 6));
        for w in l.windows(2) {
            assert_eq!(w[0].manhattan(w[1]), 1);
        }
    }

    #[test]
    fn preview_in_own_parcel_has_cost() {
        let s = game();
        let p = make_preview(&s, &[PlayerId(0)], Tool::Build(Buildable::Road(Road::Street)), Pos::new(1, 1), Pos::new(4, 1)).unwrap();
        let plan = p.result.unwrap();
        assert_eq!(plan.tiles.len(), 4);
        assert_eq!(plan.cost, 4 * s.config.street_cost);
        assert!(!p.clipped);
    }

    #[test]
    fn preview_in_neighbours_parcel_explains_why() {
        let s = game();
        let p = make_preview(&s, &[PlayerId(0)], Tool::Build(Buildable::Road(Road::Street)), Pos::new(20, 1), Pos::new(22, 1)).unwrap();
        let why = p.result.unwrap_err();
        assert!(why.contains("Them"), "{why}");
    }

    #[test]
    fn drag_over_the_fence_is_clipped() {
        let s = game();
        let tool = Tool::Build(Buildable::Zone(Zone::Residential, Density::Low));
        let p = make_preview(&s, &[PlayerId(0)], tool, Pos::new(13, 1), Pos::new(18, 2)).unwrap();
        assert!(p.clipped);
        assert!(p.result.unwrap().tiles.iter().all(|t| t.x < 16));
    }

    #[test]
    fn bulldozing_nothing_says_so() {
        let s = game();
        let p = make_preview(&s, &[PlayerId(0)], Tool::Bulldoze, Pos::new(1, 1), Pos::new(2, 2)).unwrap();
        assert_eq!(p.result.unwrap_err(), "Nothing to bulldoze here");
    }

    #[test]
    fn big_buildings_centre_on_the_cursor_and_explain_misfits() {
        let s = game();
        let tool = Tool::Build(Buildable::Building(Building::Stadium));
        let p = make_preview(&s, &[PlayerId(0)], tool, Pos::new(5, 5), Pos::new(5, 5)).unwrap();
        assert_eq!(p.result.unwrap().footprints[0], Rect::square(Pos::new(4, 4), 3));
        // Straddling the border.
        let p = make_preview(&s, &[PlayerId(0)], tool, Pos::new(15, 5), Pos::new(15, 5)).unwrap();
        assert_eq!(p.result.unwrap_err(), "Doesn't fit inside your parcel");
    }

    #[test]
    fn category_keys_cycle_through_their_items() {
        let first = cycle(Tool::Inspect, Category::Services);
        assert_eq!(first, Tool::Build(Buildable::Building(Building::PoliceStation)));
        assert_eq!(cycle(first, Category::Services), Tool::Build(Buildable::Building(Building::FireStation)));
    }
}
