//! What the player is looking at: overlay, and the colour ramps shared by the 3D world, the minimap and the HUD.

use bevy::prelude::*;
use parcels_sim::{GameState, Service, TileKind};

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
    Transit,
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
        Overlay::Transit,
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
            Overlay::Transit => "Buses & trains",
        }
    }

    /// Number key that selects it.
    pub fn key(self) -> &'static str {
        ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"][Overlay::ALL.iter().position(|&o| o == self).unwrap()]
    }
}

#[derive(Resource, Default)]
pub struct View {
    pub overlay: Overlay,
    /// Draw underground pipes (pipe and pump tools, water overlay).
    pub show_pipes: bool,
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
        // Full capacity reads orange, half again over it red.
        Overlay::Traffic => (heat(t.traffic as u32 * 180 / c.traffic_at_capacity.max(1)), if t.kind.is_road() { 230 } else { 50 }),
        Overlay::Crime => (heat(t.crime as u32 * 255 / 80), 190),
        Overlay::Density => (heat(state.occupants(idx) * 255 / max_occ), if t.kind.is_zone() { 210 } else { 40 }),
        Overlay::Power => utility(t.conducts_power(), t.powered),
        Overlay::Water => utility(t.conducts_water(), t.watered),
        Overlay::Services => {
            let n = [Service::Police, Service::Fire, Service::Health, Service::Education].iter().filter(|s| t.covered(**s)).count() as u32;
            (heat(255 - n * 255 / 4), 190)
        }
        Overlay::Transit => match t.kind {
            TileKind::Building(parcels_sim::Building::BusStop | parcels_sim::Building::BusDepot) => ([240, 180, 30], 240),
            TileKind::Building(parcels_sim::Building::TrainStation) => ([200, 70, 60], 240),
            _ if t.covered(Service::Transit) && t.covered(Service::Rail) => ([60, 170, 200], 190),
            _ if t.covered(Service::Rail) => ([150, 110, 210], if t.kind.is_road() { 120 } else { 170 }),
            _ if t.covered(Service::Transit) => ([60, 200, 120], if t.kind.is_road() { 120 } else { 170 }),
            _ => ([20, 20, 30], 110),
        },
    })
}

/// A flat, top-down colour per tile: used for the minimap and map previews.
pub fn map_color(state: &GameState, overlay: Overlay, idx: usize) -> [u8; 3] {
    let t = &state.map.tiles[idx];
    let base = match t.kind {
        TileKind::Empty if t.rail => [128, 104, 84],
        TileKind::Empty => match t.terrain {
            parcels_sim::Terrain::Water => [82, 134, 176],
            parcels_sim::Terrain::Forest => [72, 112, 62],
            parcels_sim::Terrain::Land => [128, 158, 88],
        },
        TileKind::Road(_) if t.terrain == parcels_sim::Terrain::Water => [150, 140, 125],
        TileKind::Road(_) => [70, 70, 76],
        TileKind::Zone(z, _) => {
            let tint = ZONE_TINT[z.index()];
            if t.level == 0 { mix([128, 158, 88], tint, 110) } else { shade(tint, 100 - 6 * t.level.min(6) as u32) }
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
