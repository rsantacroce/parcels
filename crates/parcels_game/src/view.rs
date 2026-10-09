//! What the player is looking at: overlay, whose neighbourhood is "active", and
//! the colour ramps shared by the 3D world, the minimap and the HUD.

use bevy::prelude::*;
use parcels_sim::{GameState, PlayerId, Service, TileKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Overlay {
    #[default]
    None,
    LandValue,
    Power,
    Water,
    Density,
    Pollution,
    Traffic,
    Crime,
    Services,
    Owners,
}

impl Overlay {
    pub const ALL: [Overlay; 10] = [
        Overlay::None,
        Overlay::LandValue,
        Overlay::Power,
        Overlay::Water,
        Overlay::Density,
        Overlay::Pollution,
        Overlay::Traffic,
        Overlay::Crime,
        Overlay::Services,
        Overlay::Owners,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Overlay::None => "City",
            Overlay::LandValue => "Land value",
            Overlay::Power => "Power",
            Overlay::Water => "Water",
            Overlay::Density => "Density",
            Overlay::Pollution => "Pollution",
            Overlay::Traffic => "Traffic",
            Overlay::Crime => "Crime",
            Overlay::Services => "Services",
            Overlay::Owners => "Owners",
        }
    }

    /// Number key that selects it (0 = the tenth).
    pub fn key(self) -> &'static str {
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"][Overlay::ALL.iter().position(|&o| o == self).unwrap()]
    }
}

#[derive(Resource, Default)]
pub struct View {
    pub overlay: Overlay,
    /// The player whose parcels we act for (hot-seat switches this).
    pub active: Option<PlayerId>,
    /// Draw underground pipes (pipe and pump tools, water overlay).
    pub show_pipes: bool,
}

pub fn player_color(id: PlayerId) -> [u8; 3] {
    const C: [[u8; 3]; 8] = [
        [235, 87, 87],
        [80, 160, 245],
        [245, 190, 60],
        [170, 110, 235],
        [70, 205, 160],
        [245, 130, 190],
        [150, 200, 70],
        [240, 140, 60],
    ];
    C[id.index() % C.len()]
}

pub fn mix(a: [u8; 3], b: [u8; 3], t: u32) -> [u8; 3] {
    let f = |x: u8, y: u8| ((x as u32 * (256 - t.min(256)) + y as u32 * t.min(256)) >> 8) as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}

pub fn shade(c: [u8; 3], pct: u32) -> [u8; 3] {
    let f = |x: u8| (x as u32 * pct / 100).min(255) as u8;
    [f(c[0]), f(c[1]), f(c[2])]
}

/// Heat ramp for 0..=255: blue → green → yellow → red.
pub fn heat(v: u32) -> [u8; 3] {
    let v = v.min(255);
    if v < 85 {
        mix([40, 80, 200], [60, 190, 90], v * 3)
    } else if v < 170 {
        mix([60, 190, 90], [240, 220, 60], (v - 85) * 3)
    } else {
        mix([240, 220, 60], [220, 50, 40], (v - 170) * 3)
    }
}

pub const ZONE_TINT: [[u8; 3]; 4] = [[120, 200, 110], [110, 160, 235], [225, 185, 80], [150, 130, 230]];

/// Overlay colour and strength (0..=256) for a tile, if the overlay colours it.
pub fn overlay_tint(state: &GameState, overlay: Overlay, idx: usize) -> Option<([u8; 3], u32)> {
    let t = &state.map.tiles[idx];
    let c = &state.config;
    let max_occ = (c.high_density_max_level as u32 * c.residents_per_level as u32).max(1);
    let utility = |conducts: bool, served: bool| match (conducts, served) {
        (true, true) => ([60, 210, 90], 190),
        (true, false) => ([230, 50, 40], 200),
        (false, true) => ([60, 210, 90], 70),
        (false, false) => ([20, 20, 30], 150),
    };
    Some(match overlay {
        Overlay::None => return None,
        Overlay::LandValue => (heat(255 - t.land_value as u32 * 255 / c.land_value_max.max(1) as u32), 200),
        Overlay::Pollution => (heat(t.pollution as u32 * 255 / 200), 200),
        Overlay::Traffic => (heat(t.traffic as u32 * 255 / 60), if t.kind.is_road() { 230 } else { 50 }),
        Overlay::Crime => (heat(t.crime as u32 * 255 / 80), 190),
        Overlay::Density => (heat(state.occupants(idx) * 255 / max_occ), if t.kind.is_zone() { 210 } else { 40 }),
        Overlay::Power => utility(t.conducts_power(), t.powered),
        Overlay::Water => utility(t.conducts_water(), t.watered),
        Overlay::Services => {
            let n = Service::ALL.iter().filter(|s| t.covered(**s)).count() as u32;
            (heat(255 - n * 255 / 4), 190)
        }
        Overlay::Owners => (player_color(state.owner_of_idx(idx)?), 150),
    })
}

/// A flat, top-down colour per tile: used for the minimap and map previews.
pub fn map_color(state: &GameState, overlay: Overlay, idx: usize) -> [u8; 3] {
    let t = &state.map.tiles[idx];
    let base = match t.kind {
        TileKind::Empty => match t.terrain {
            parcels_sim::Terrain::Water => [50, 100, 175],
            parcels_sim::Terrain::Forest => [38, 92, 44],
            parcels_sim::Terrain::Land => [86, 140, 74],
        },
        TileKind::Road(_) if t.terrain == parcels_sim::Terrain::Water => [150, 140, 125],
        TileKind::Road(_) => [70, 70, 76],
        TileKind::Zone(z, _) => {
            let tint = ZONE_TINT[z.index()];
            if t.level == 0 { mix([86, 140, 74], tint, 110) } else { shade(tint, 100 - 6 * t.level.min(6) as u32) }
        }
        TileKind::Building(b) => match b.category() {
            parcels_sim::Category::Parks => [60, 150, 70],
            parcels_sim::Category::Power => [200, 180, 70],
            parcels_sim::Category::Water => [70, 160, 220],
            parcels_sim::Category::Services => [220, 220, 230],
            _ => [210, 150, 200],
        },
    };
    match overlay_tint(state, overlay, idx) {
        Some((c, a)) => mix(base, c, a),
        None => base,
    }
}
