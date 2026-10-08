//! Orthographic camera over the isometric map: WASD/arrows/edge-scroll/middle-drag to pan,
//! wheel to zoom toward the cursor, clamped to the map.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::input::EguiWantsInput;

use crate::driver::Driver;
use crate::render::world_size;
use crate::AppState;

#[derive(Component)]
pub struct MainCamera;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(OnEnter(AppState::Playing), frame_map)
            .add_systems(Update, (pan_zoom, clamp).chain().run_if(in_state(AppState::Playing)));
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera2d, MainCamera, Projection::Orthographic(OrthographicProjection::default_2d())));
}

fn frame_map(
    driver: Res<Driver>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cam: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let Some(state) = driver.state() else { return };
    let map = world_size(state);
    // Start zoomed in on the first parcel we control, with the map in context.
    let target = driver
        .controllable()
        .first()
        .and_then(|&p| state.parcels_of(p).next())
        .map(|pc| {
            let c = pc.rect.min;
            let d = pc.rect.max;
            crate::render::tile_center(state, (c.x + d.x) / 2, (c.y + d.y) / 2)
        })
        .unwrap_or(Vec2::ZERO);
    cam.0.translation = target.extend(cam.0.translation.z);
    if let Projection::Orthographic(o) = &mut *cam.1 {
        let fit = (map.x / window.width()).max(map.y / window.height());
        o.scale = (fit * 0.75).clamp(0.25, 4.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn pan_zoom(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    motion: Res<AccumulatedMouseMotion>,
    egui: Res<EguiWantsInput>,
    time: Res<Time<Real>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cam: Single<(&mut Transform, &mut Projection, &Camera, &GlobalTransform), With<MainCamera>>,
) {
    let (ref mut tf, ref mut proj, camera, gtf) = *cam;
    let Projection::Orthographic(o) = &mut **proj else { return };
    let dt = time.delta_secs();
    let speed = 600.0 * o.scale;
    let mut dir = Vec2::ZERO;
    if !egui.wants_any_keyboard_input() {
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
    }
    let cursor = window.cursor_position();
    if let Some(c) = cursor.filter(|_| window.focused && !egui.is_pointer_over_area()) {
        let edge = 6.0;
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
    tf.translation += (dir.normalize_or_zero() * speed * dt).extend(0.0);

    if buttons.pressed(MouseButton::Middle) && !egui.is_pointer_over_area() {
        tf.translation.x -= motion.delta.x * o.scale;
        tf.translation.y += motion.delta.y * o.scale;
    }

    if scroll.delta.y != 0.0 && !egui.is_pointer_over_area() {
        let notches = match scroll.unit {
            MouseScrollUnit::Line => scroll.delta.y,
            MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
        };
        let before = cursor.and_then(|c| camera.viewport_to_world_2d(gtf, c).ok());
        let new_scale = (o.scale * 0.9f32.powf(notches)).clamp(0.2, 4.0);
        // Keep the point under the cursor fixed while zooming.
        if let Some(b) = before {
            let center = tf.translation.truncate();
            let offset = b - center;
            let moved = offset * (1.0 - new_scale / o.scale);
            tf.translation += moved.extend(0.0);
        }
        o.scale = new_scale;
    }
}

fn clamp(driver: Res<Driver>, mut cam: Single<&mut Transform, With<MainCamera>>) {
    let Some(state) = driver.state() else { return };
    let half = world_size(state) / 2.0;
    cam.translation.x = cam.translation.x.clamp(-half.x, half.x);
    cam.translation.y = cam.translation.y.clamp(-half.y, half.y);
}
