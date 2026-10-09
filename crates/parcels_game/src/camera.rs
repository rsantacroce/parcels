//! Two ways to look at the city.
//!
//! * Overview: an orbit camera over the map. WASD / arrows / screen edge or
//!   right-drag to pan, middle-drag to rotate and tilt, Q/E to rotate, wheel to
//!   zoom toward the cursor, PageUp/PageDown to tilt.
//! * Street: walk the streets at eye height. WASD to walk, mouse to look,
//!   Shift to run, F to fly (Space/Ctrl up/down). V or Esc returns to the overview.
//!
//! On the title screen the camera slowly circles the showcase city.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use bevy_egui::input::EguiWantsInput;
use parcels_sim::{Building, GameState, Pos, Terrain, TileKind};

use crate::driver::Driver;
use crate::settings::Settings;
use crate::world::meshgen::TILE;
use crate::AppState;

#[derive(Component)]
pub struct MainCamera;

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub enum CamMode {
    Overview,
    Street { fly: bool },
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct Orbit {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
}

impl Default for Orbit {
    fn default() -> Self {
        Self { focus: Vec3::ZERO, yaw: 0.7, pitch: 0.85, dist: 400.0 }
    }
}

impl Orbit {
    pub fn eye(&self) -> Vec3 {
        self.focus + self.dist * Vec3::new(self.pitch.cos() * self.yaw.sin(), self.pitch.sin(), self.pitch.cos() * self.yaw.cos())
    }
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct Walker {
    /// Feet position.
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    bob: f32,
}

/// Was the right button dragged (pan) rather than clicked (cancel)?
#[derive(Resource, Default)]
pub struct RightDrag {
    pub travelled: f32,
}

pub const EYE: f32 = 1.7;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Orbit>()
            .init_resource::<Walker>()
            .init_resource::<RightDrag>()
            .add_systems(Startup, spawn_camera)
            .add_systems(OnEnter(AppState::Playing), frame_map)
            .add_systems(OnExit(AppState::Playing), leave_street)
            .add_systems(Update, showcase_orbit.run_if(not(in_state(AppState::Playing))))
            .add_systems(Update, (toggle_street, overview_controls, street_controls).chain().run_if(in_state(AppState::Playing)))
            .add_systems(PostUpdate, apply_camera.before(TransformSystems::Propagate));
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        MainCamera,
        CamMode::Overview,
        Projection::Perspective(PerspectiveProjection { fov: 50f32.to_radians(), near: 0.3, far: 12_000.0, ..default() }),
        DistanceFog { color: Color::srgb(0.52, 0.72, 0.92), ..default() },
        Transform::from_xyz(0.0, 300.0, 300.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

pub fn map_center(state: &GameState) -> Vec3 {
    Vec3::new(state.map.width as f32, 0.0, state.map.height as f32) * TILE / 2.0
}

fn frame_map(driver: Res<Driver>, mut orbit: ResMut<Orbit>) {
    let Some(state) = driver.state() else { return };
    // Start over the first parcel we control, with the neighbours in view.
    let (focus, span) = driver
        .controllable()
        .first()
        .and_then(|&p| state.parcels_of(p).next())
        .map(|pc| {
            let r = pc.rect;
            let c = Vec3::new((r.min.x + r.max.x + 1) as f32, 0.0, (r.min.y + r.max.y + 1) as f32) * TILE / 2.0;
            (c, r.width().max(r.height()) as f32 * TILE)
        })
        .unwrap_or((map_center(state), state.map.width.max(state.map.height) as f32 * TILE));
    *orbit = Orbit { focus, yaw: 0.6, pitch: 0.9, dist: (span * 1.1).clamp(120.0, 2500.0) };
}

fn showcase_orbit(time: Res<Time<Real>>, driver: Option<Res<Driver>>, mut orbit: ResMut<Orbit>, mut mode: Query<&mut CamMode, With<MainCamera>>) {
    for mut m in &mut mode {
        *m = CamMode::Overview;
    }
    let Some(state) = driver.as_ref().and_then(|d| d.state()) else { return };
    let span = state.map.width.max(state.map.height) as f32 * TILE;
    orbit.focus = map_center(state);
    orbit.dist = span * 0.75;
    orbit.pitch = 0.55;
    orbit.yaw += time.delta_secs() * 0.04;
}

fn max_dist(state: &GameState) -> f32 {
    state.map.width.max(state.map.height) as f32 * TILE * 1.4
}

/// Ground point under the cursor (y = 0 plane).
pub fn cursor_ground(window: &Window, camera: &Camera, cam_tf: &GlobalTransform) -> Option<Vec3> {
    let cursor = window.cursor_position()?;
    let ray = camera.viewport_to_world(cam_tf, cursor).ok()?;
    let d = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
    Some(ray.get_point(d))
}

#[allow(clippy::too_many_arguments)]
fn overview_controls(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    motion: Res<AccumulatedMouseMotion>,
    egui: Res<EguiWantsInput>,
    time: Res<Time<Real>>,
    driver: Res<Driver>,
    settings: Res<Settings>,
    window: Single<&Window, With<PrimaryWindow>>,
    cam: Single<(&Camera, &GlobalTransform, &CamMode), With<MainCamera>>,
    mut orbit: ResMut<Orbit>,
    mut drag: ResMut<RightDrag>,
) {
    let (camera, cam_tf, mode) = *cam;
    if *mode != CamMode::Overview {
        return;
    }
    let Some(state) = driver.state() else { return };
    let dt = time.delta_secs();
    let forward = Vec3::new(-orbit.yaw.sin(), 0.0, -orbit.yaw.cos());
    let right = Vec3::new(orbit.yaw.cos(), 0.0, -orbit.yaw.sin());
    let speed = orbit.dist * 1.1;
    let mut dir = Vec2::ZERO;
    let typing = egui.wants_any_keyboard_input();
    if !typing {
        if keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
            dir.y += 1.0;
        }
        if keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
            dir.y -= 1.0;
        }
        if keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
            dir.x -= 1.0;
        }
        if keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
            dir.x += 1.0;
        }
        if keys.pressed(KeyCode::KeyQ) {
            orbit.yaw -= dt * 1.4;
        }
        if keys.pressed(KeyCode::KeyE) {
            orbit.yaw += dt * 1.4;
        }
        if keys.pressed(KeyCode::PageUp) {
            orbit.pitch += dt * 0.8;
        }
        if keys.pressed(KeyCode::PageDown) {
            orbit.pitch -= dt * 0.8;
        }
    }
    let cursor = window.cursor_position();
    if let Some(c) = cursor.filter(|_| window.focused && !egui.is_pointer_over_area()) {
        let edge = 4.0;
        if c.x < edge {
            dir.x -= 1.0;
        } else if c.x > window.width() - edge {
            dir.x += 1.0;
        }
        if c.y < edge {
            dir.y += 1.0;
        } else if c.y > window.height() - edge {
            dir.y -= 1.0;
        }
    }
    let dir = dir.normalize_or_zero();
    orbit.focus += (forward * dir.y + right * dir.x) * speed * dt;

    let over_ui = egui.is_pointer_over_area();
    let px = motion.delta;
    if buttons.pressed(MouseButton::Middle) && !over_ui {
        let s = 0.005 * settings.mouse_sensitivity;
        orbit.yaw -= px.x * s;
        orbit.pitch += px.y * s * if settings.invert_y { -1.0 } else { 1.0 };
    }
    if buttons.just_pressed(MouseButton::Right) {
        drag.travelled = 0.0;
    }
    if buttons.pressed(MouseButton::Right) && !over_ui {
        drag.travelled += px.length();
        if drag.travelled > 4.0 {
            // Grab the ground: move by how far the cursor's ground point slides.
            let k = orbit.dist / window.height().max(1.0) * 1.6;
            orbit.focus += (-right * px.x + forward * px.y) * k;
        }
    }

    if scroll.delta.y != 0.0 && !over_ui {
        let notches = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        let factor = 0.88f32.powf(notches);
        let new_dist = (orbit.dist * factor).clamp(25.0, max_dist(state));
        let k = 1.0 - new_dist / orbit.dist;
        if let Some(g) = cursor_ground(&window, camera, cam_tf) {
            let f = orbit.focus;
            orbit.focus = f + (g - f) * k;
        }
        orbit.dist = new_dist;
    }
    orbit.pitch = orbit.pitch.clamp(0.18, 1.5);
    let size = Vec3::new(state.map.width as f32, 0.0, state.map.height as f32) * TILE;
    orbit.focus = orbit.focus.clamp(Vec3::ZERO, size);
    orbit.dist = orbit.dist.clamp(25.0, max_dist(state));
}

/// Can someone on foot stand on this tile?
pub fn walkable(state: &GameState, p: Pos) -> bool {
    let Some(t) = state.map.get(p) else { return false };
    match t.kind {
        TileKind::Road(_) => true,
        TileKind::Zone(..) => t.level == 0 || t.burning > 0,
        TileKind::Building(b) => {
            matches!(b, Building::Park | Building::Plaza | Building::Playground | Building::SportsField | Building::SolarFarm)
        }
        TileKind::Empty => t.terrain != Terrain::Water,
    }
}

fn tile_of(v: Vec3) -> Option<Pos> {
    (v.x >= 0.0 && v.z >= 0.0).then(|| Pos::new((v.x / TILE) as u16, (v.z / TILE) as u16))
}

fn ground_height(state: &GameState, v: Vec3) -> f32 {
    match tile_of(v).and_then(|p| state.map.get(p)) {
        Some(t) if t.kind.is_road() && t.terrain == Terrain::Water => 0.4,
        Some(t) if t.kind.is_road() => {
            // Sidewalks are a kerb higher than the asphalt.
            let fx = (v.x / TILE).fract() * TILE;
            let fz = (v.z / TILE).fract() * TILE;
            if fx.min(TILE - fx).min(fz).min(TILE - fz) < 1.8 { 0.15 } else { 0.03 }
        }
        _ => 0.0,
    }
}

fn set_cursor(cursor: &mut CursorOptions, grab: bool) {
    cursor.grab_mode = if grab { CursorGrabMode::Locked } else { CursorGrabMode::None };
    cursor.visible = !grab;
}

#[allow(clippy::too_many_arguments)]
fn toggle_street(
    keys: Res<ButtonInput<KeyCode>>,
    egui: Res<EguiWantsInput>,
    driver: Res<Driver>,
    mut orbit: ResMut<Orbit>,
    mut walker: ResMut<Walker>,
    mut mode: Single<&mut CamMode, With<MainCamera>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut request: MessageReader<StreetRequest>,
) {
    let Some(state) = driver.state() else { return };
    let pressed_v = keys.just_pressed(KeyCode::KeyV) && !egui.wants_any_keyboard_input();
    let asked = request.read().last().map(|r| r.0);
    let want = match (**mode, asked, pressed_v) {
        (_, Some(at), _) => Some(at),
        (CamMode::Overview, None, true) => Some(None),
        (CamMode::Street { .. }, None, true) => {
            leave(&mut orbit, &walker, &mut mode, &mut cursor);
            return;
        }
        _ => None,
    };
    if let Some(at) = want {
        let start = at.or_else(|| tile_of(orbit.focus)).unwrap_or(Pos::new(0, 0));
        // Nearest road tile to where we're looking, else nearest walkable one.
        let best = state
            .map
            .bounds()
            .iter()
            .filter(|&p| state.map.tile(p).kind.is_road())
            .min_by_key(|p| p.manhattan(start))
            .or_else(|| state.map.bounds().iter().filter(|&p| walkable(state, p)).min_by_key(|p| p.manhattan(start)));
        let Some(p) = best else { return };
        let t = state.map.tile(p);
        let along_x = t.kind.is_road()
            && [(1i32, 0i32), (-1, 0)].iter().any(|(dx, _)| {
                let x = p.x as i32 + dx;
                x >= 0 && state.map.get(Pos::new(x as u16, p.y)).is_some_and(|n| n.kind.is_road())
            });
        // Stand on the sidewalk-side of the lane, looking down the street.
        let c = Vec3::new((p.x as f32 + 0.5) * TILE, 0.0, (p.y as f32 + 0.5) * TILE);
        walker.pos = c + if along_x { Vec3::Z * 3.0 } else { Vec3::X * 3.0 };
        walker.yaw = if along_x { -std::f32::consts::FRAC_PI_2 } else { 0.0 };
        walker.pitch = 0.05;
        **mode = CamMode::Street { fly: false };
        set_cursor(&mut cursor, true);
    } else if matches!(**mode, CamMode::Street { .. }) && keys.just_pressed(KeyCode::Escape) {
        leave(&mut orbit, &walker, &mut mode, &mut cursor);
    }
}

fn leave(orbit: &mut Orbit, walker: &Walker, mode: &mut CamMode, cursor: &mut CursorOptions) {
    orbit.focus = walker.pos.with_y(0.0);
    orbit.dist = orbit.dist.min(220.0);
    orbit.yaw = walker.yaw;
    *mode = CamMode::Overview;
    set_cursor(cursor, false);
}

fn leave_street(mut mode: Query<&mut CamMode, With<MainCamera>>, mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    for mut m in &mut mode {
        *m = CamMode::Overview;
    }
    for mut c in &mut cursor {
        set_cursor(&mut c, false);
    }
}

/// Ask to drop into street view (at a tile, or near the camera focus).
#[derive(Message, Clone, Copy)]
pub struct StreetRequest(pub Option<Pos>);

#[allow(clippy::too_many_arguments)]
fn street_controls(
    keys: Res<ButtonInput<KeyCode>>,
    motion: Res<AccumulatedMouseMotion>,
    time: Res<Time<Real>>,
    driver: Res<Driver>,
    settings: Res<Settings>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut mode: Single<&mut CamMode, With<MainCamera>>,
    mut walker: ResMut<Walker>,
) {
    let CamMode::Street { fly } = **mode else { return };
    let Some(state) = driver.state() else { return };
    let dt = time.delta_secs().min(0.1);
    if window.focused {
        let s = 0.0022 * settings.mouse_sensitivity;
        walker.yaw -= motion.delta.x * s;
        walker.pitch -= motion.delta.y * s * if settings.invert_y { -1.0 } else { 1.0 };
        walker.pitch = walker.pitch.clamp(-1.45, 1.45);
    }
    if keys.just_pressed(KeyCode::KeyF) {
        **mode = CamMode::Street { fly: !fly };
        if fly {
            walker.pos.y = 0.0;
        }
    }
    let forward = Vec3::new(-walker.yaw.sin(), 0.0, -walker.yaw.cos());
    let right = Vec3::new(walker.yaw.cos(), 0.0, -walker.yaw.sin());
    let mut dir = Vec3::ZERO;
    if keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
        dir += forward;
    }
    if keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
        dir -= forward;
    }
    if keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
        dir -= right;
    }
    if keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
        dir += right;
    }
    let run = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let speed = if fly { if run { 90.0 } else { 30.0 } } else if run { 12.0 } else { 4.5 };
    let step = dir.normalize_or_zero() * speed * dt;
    let size = Vec3::new(state.map.width as f32, 0.0, state.map.height as f32) * TILE;
    if fly {
        walker.pos += step;
        if keys.pressed(KeyCode::Space) {
            walker.pos.y += speed * dt;
        }
        if keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
            walker.pos.y -= speed * dt;
        }
        walker.pos.y = walker.pos.y.clamp(0.0, 600.0);
        walker.pos = walker.pos.clamp(Vec3::new(-50.0, 0.0, -50.0), size + Vec3::new(50.0, 600.0, 50.0));
        return;
    }
    // Slide along walls: try each axis on its own.
    let r = 0.35;
    let free = |v: Vec3| {
        [Vec3::new(r, 0.0, r), Vec3::new(-r, 0.0, r), Vec3::new(r, 0.0, -r), Vec3::new(-r, 0.0, -r)]
            .iter()
            .all(|o| tile_of(v + *o).is_some_and(|p| walkable(state, p)))
    };
    let mut p = walker.pos;
    if free(p + Vec3::X * step.x) {
        p.x += step.x;
    }
    if free(p + Vec3::Z * step.z) {
        p.z += step.z;
    }
    let target_y = ground_height(state, p);
    p.y += (target_y - p.y) * (dt * 12.0).min(1.0);
    if step.length_squared() > 0.0 {
        walker.bob += step.length() * 1.6;
    }
    walker.pos = p;
}

fn apply_camera(orbit: Res<Orbit>, walker: Res<Walker>, mut cam: Query<(&mut Transform, &CamMode), With<MainCamera>>) {
    for (mut tf, mode) in &mut cam {
        *tf = match mode {
            CamMode::Overview => Transform::from_translation(orbit.eye()).looking_at(orbit.focus, Vec3::Y),
            CamMode::Street { fly } => {
                let bob = if *fly { 0.0 } else { (walker.bob).sin() * 0.04 };
                Transform::from_translation(walker.pos + Vec3::Y * (EYE + bob))
                    .with_rotation(Quat::from_euler(EulerRot::YXZ, walker.yaw, walker.pitch, 0.0))
            }
        };
    }
}
