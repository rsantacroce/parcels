//! Build tools: pick a tool, click or drag to paint, see a ghost preview with the
//! cost (or the reason it can't be built) before committing.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::input::EguiWantsInput;
use parcels_sim::systems::apply::{plan_bulldoze, plan_place, Plan};
use parcels_sim::{Area, Buildable, CommandKind, GameState, ParcelId, PlayerId, Pos, Rect, Rejection};

use crate::audio::{Sfx, SfxQueue};
use crate::camera::MainCamera;
use crate::driver::Driver;
use crate::render::{footprint, player_color, world_to_tile, View};
use crate::ui::Toasts;
use crate::AppState;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Inspect,
    Build(Buildable),
    Bulldoze,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Inspect => "Inspect",
            Tool::Build(b) => b.name(),
            Tool::Bulldoze => "Bulldoze",
        }
    }

    pub fn hotkey(self) -> &'static str {
        match self {
            Tool::Inspect => "Esc",
            Tool::Bulldoze => "B",
            Tool::Build(b) => match b {
                Buildable::Road => "R",
                Buildable::Residential => "Z",
                Buildable::Commercial => "X",
                Buildable::Industrial => "C",
                Buildable::PowerLine => "L",
                Buildable::WaterPipe => "P",
                Buildable::PowerPlant => "G",
                Buildable::WaterPump => "U",
                Buildable::Park => "K",
            },
        }
    }

    pub fn all() -> Vec<Tool> {
        let mut v = vec![Tool::Inspect];
        v.extend(Buildable::ALL.iter().map(|&b| Tool::Build(b)));
        v.push(Tool::Bulldoze);
        v
    }

    fn shape(self) -> Shape {
        match self {
            Tool::Build(Buildable::Road | Buildable::PowerLine | Buildable::WaterPipe) => Shape::Line,
            Tool::Build(Buildable::PowerPlant | Buildable::WaterPump) | Tool::Inspect => Shape::Single,
            _ => Shape::Rect,
        }
    }
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

fn hotkeys(keys: Res<ButtonInput<KeyCode>>, egui: Res<EguiWantsInput>, mut ts: ResMut<ToolState>, mut view: ResMut<View>, driver: Res<Driver>) {
    if egui.wants_any_keyboard_input() {
        return;
    }
    let map = [
        (KeyCode::Escape, Tool::Inspect),
        (KeyCode::KeyI, Tool::Inspect),
        (KeyCode::KeyB, Tool::Bulldoze),
        (KeyCode::KeyR, Tool::Build(Buildable::Road)),
        (KeyCode::KeyZ, Tool::Build(Buildable::Residential)),
        (KeyCode::KeyX, Tool::Build(Buildable::Commercial)),
        (KeyCode::KeyC, Tool::Build(Buildable::Industrial)),
        (KeyCode::KeyL, Tool::Build(Buildable::PowerLine)),
        (KeyCode::KeyP, Tool::Build(Buildable::WaterPipe)),
        (KeyCode::KeyG, Tool::Build(Buildable::PowerPlant)),
        (KeyCode::KeyU, Tool::Build(Buildable::WaterPump)),
        (KeyCode::KeyK, Tool::Build(Buildable::Park)),
    ];
    for (k, t) in map {
        if keys.just_pressed(k) {
            ts.tool = t;
            ts.drag_start = None;
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
    ];
    for (i, k) in overlays.into_iter().enumerate() {
        if keys.just_pressed(k) {
            view.overlay = crate::render::Overlay::ALL[i];
        }
    }
    view.show_pipes = ts.tool == Tool::Build(Buildable::WaterPipe) || ts.tool == Tool::Build(Buildable::WaterPump);
    // Hot-seat: cycle which of our players we're looking at / acting as.
    if keys.just_pressed(KeyCode::Tab) {
        let mine = driver.controllable();
        if !mine.is_empty() {
            let i = view.active.and_then(|a| mine.iter().position(|&p| p == a)).map_or(0, |i| (i + 1) % mine.len());
            view.active = Some(mine[i]);
        }
    }
    if view.active.is_none() || !driver.controllable().contains(&view.active.unwrap()) {
        view.active = driver.controllable().first().copied();
    }
}

fn track_hover(
    driver: Res<Driver>,
    window: Single<&Window, With<PrimaryWindow>>,
    cam: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    egui: Res<EguiWantsInput>,
    mut ts: ResMut<ToolState>,
) {
    let Some(state) = driver.state() else { return };
    if egui.is_pointer_over_area() {
        ts.hover = None;
        return;
    }
    ts.hover = window
        .cursor_position()
        .and_then(|c| cam.0.viewport_to_world_2d(cam.1, c).ok())
        .and_then(|w| world_to_tile(state, w));
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

fn make_preview(state: &GameState, mine: &[PlayerId], tool: Tool, start: Pos, end: Pos) -> Option<Preview> {
    if tool == Tool::Inspect {
        return None;
    }
    let tiles = match tool.shape() {
        Shape::Single => vec![end],
        Shape::Line => line(start, end),
        Shape::Rect => Rect::from_corners(start, end).iter().collect(),
    };
    let anchor = if matches!(tool.shape(), Shape::Single) { end } else { start };
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
            Tool::Build(b) => format!("{} can't go here", b.name()),
            _ => r.to_string(),
        },
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
    mut driver: ResMut<Driver>,
    mut ts: ResMut<ToolState>,
    mut view: ResMut<View>,
    mut toasts: ResMut<Toasts>,
    mut sfx: ResMut<SfxQueue>,
) {
    if buttons.just_pressed(MouseButton::Right) {
        if ts.drag_start.is_some() {
            ts.drag_start = None;
        } else {
            ts.tool = Tool::Inspect;
        }
        return;
    }
    if buttons.just_pressed(MouseButton::Left) && !egui.is_pointer_over_area() {
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
                let area = Area::Tiles(plan.tiles.clone());
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

fn outline(gizmos: &mut Gizmos, corners: [Vec2; 4], color: Color) {
    gizmos.linestrip_2d(corners.into_iter().chain([corners[0]]), color);
}

fn draw_gizmos(mut gizmos: Gizmos, driver: Res<Driver>, ts: Res<ToolState>, view: Res<View>) {
    let Some(state) = driver.state() else { return };
    // Parcel borders in each owner's colour; ours drawn doubled.
    for pc in &state.parcels {
        let [r, g, b] = player_color(pc.owner);
        let color = Color::srgba_u8(r, g, b, 220);
        outline(&mut gizmos, footprint(state, pc.rect.min, pc.rect.max, 0.03), color);
        if Some(pc.owner) == view.active {
            outline(&mut gizmos, footprint(state, pc.rect.min, pc.rect.max, 0.15), color);
        }
    }
    if let Some(p) = &ts.preview {
        let color = match &p.result {
            Ok(_) => Color::srgba(0.3, 1.0, 0.4, 0.9),
            Err(_) => Color::srgba(1.0, 0.25, 0.2, 0.9),
        };
        let tiles: &[Pos] = match &p.result {
            Ok(plan) => &plan.tiles,
            Err(_) => &p.tiles,
        };
        for &t in tiles.iter().take(4096) {
            outline(&mut gizmos, footprint(state, t, t, 0.1), color);
        }
    }
    if let Some(h) = ts.hover {
        outline(&mut gizmos, footprint(state, h, h, 0.0), Color::WHITE);
    }
    if let Some(s) = ts.selected {
        outline(&mut gizmos, footprint(state, s, s, -0.08), Color::srgb(1.0, 0.9, 0.2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parcels_sim::{Config, Controller, NewGame, PlayerSetup, Terrain};

    fn game() -> GameState {
        let mut config = Config::default();
        config.map_width = 32;
        config.map_height = 16;
        let players = vec![
            PlayerSetup { name: "Me".into(), controller: Controller::Human },
            PlayerSetup { name: "Them".into(), controller: Controller::Human },
        ];
        let mut s = GameState::new(&NewGame { seed: 1, config, players });
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
        let p = make_preview(&s, &[PlayerId(0)], Tool::Build(Buildable::Road), Pos::new(1, 1), Pos::new(4, 1)).unwrap();
        let plan = p.result.unwrap();
        assert_eq!(plan.tiles.len(), 4);
        assert_eq!(plan.cost, 4 * s.config.cost.road);
        assert!(!p.clipped);
    }

    #[test]
    fn preview_in_neighbours_parcel_explains_why() {
        let s = game();
        let p = make_preview(&s, &[PlayerId(0)], Tool::Build(Buildable::Road), Pos::new(20, 1), Pos::new(22, 1)).unwrap();
        let why = p.result.unwrap_err();
        assert!(why.contains("Them"), "{why}");
    }

    #[test]
    fn drag_over_the_fence_is_clipped() {
        let s = game();
        let p = make_preview(&s, &[PlayerId(0)], Tool::Build(Buildable::Residential), Pos::new(13, 1), Pos::new(18, 2)).unwrap();
        assert!(p.clipped);
        assert!(p.result.unwrap().tiles.iter().all(|t| t.x < 16));
    }

    #[test]
    fn bulldozing_nothing_says_so() {
        let s = game();
        let p = make_preview(&s, &[PlayerId(0)], Tool::Bulldoze, Pos::new(1, 1), Pos::new(2, 2)).unwrap();
        assert_eq!(p.result.unwrap_err(), "Nothing to bulldoze here");
    }
}
