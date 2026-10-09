//! The 3D city: chunked meshes built from the tile grid, sun and sky, street
//! life and animated props. It draws whatever `Driver` holds, so the title
//! screen's showcase city and a real game use the same code.

pub mod chunks;
pub mod life;
pub mod meshgen;
pub mod props;
pub mod scene;

use bevy::prelude::*;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<chunks::Chunks>()
            .init_resource::<scene::Sky>()
            .init_resource::<life::LifeRng>()
            .init_resource::<props::Props>()
            .add_systems(Startup, (scene::setup, life::setup, props::setup))
            .add_systems(
                Update,
                (scene::update_sky, chunks::sync_chunks, props::sync_props, props::animate, life::populate, life::drive),
            );
    }
}
