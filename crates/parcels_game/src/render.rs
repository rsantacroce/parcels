//! Draws the map into one isometric texture. The grid in `GameState` is the
//! source of truth; this is purely a view, repainted when the state or overlay
//! changes. Painting is two passes: a flat top-down ground layer (grass, roads,
//! lots, wires) reprojected onto 2:1 diamonds, then buildings as shaded boxes
//! drawn back to front. Floats are fine here — nothing flows back into the
//! simulation.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use parcels_sim::{GameState, PlayerId, Terrain, Tile, TileKind, Zone};

use crate::driver::Driver;
use crate::AppState;

/// Resolution of one tile in the flat ground layer.
const TILE_PX: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overlay {
    #[default]
    None,
    LandValue,
    Power,
    Water,
    Density,
    Pollution,
    Traffic,
    Owners,
}

impl Overlay {
    pub const ALL: [Overlay; 8] = [
        Overlay::None,
        Overlay::LandValue,
        Overlay::Power,
        Overlay::Water,
        Overlay::Density,
        Overlay::Pollution,
        Overlay::Traffic,
        Overlay::Owners,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Overlay::None => "Map",
            Overlay::LandValue => "Land value",
            Overlay::Power => "Power",
            Overlay::Water => "Water",
            Overlay::Density => "Density",
            Overlay::Pollution => "Pollution",
            Overlay::Traffic => "Traffic",
            Overlay::Owners => "Owners",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Overlay::None => "1",
            Overlay::LandValue => "2",
            Overlay::Power => "3",
            Overlay::Water => "4",
            Overlay::Density => "5",
            Overlay::Pollution => "6",
            Overlay::Traffic => "7",
            Overlay::Owners => "8",
        }
    }
}

/// What the player is looking at; changes trigger a repaint.
#[derive(Resource, Default)]
pub struct View {
    pub overlay: Overlay,
    /// The player whose parcels are shown at full brightness.
    pub active: Option<PlayerId>,
    pub show_pipes: bool,
}

#[derive(Resource)]
pub struct MapTexture {
    pub image: Handle<Image>,
    pub entity: Entity,
    painted: Option<(u64, u64, Overlay, Option<PlayerId>, bool)>,
    size: (u16, u16),
    /// Scratch top-down ground layer, reprojected into `image` each repaint.
    flat: Vec<u8>,
}

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<View>()
            .add_systems(Update, (ensure_texture, repaint).chain().run_if(in_state(AppState::Playing)))
            .add_systems(OnExit(AppState::Playing), despawn_texture);
    }
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

// ---------------------------------------------------------------------------
// Isometric projection
//
// Ground point (gx, gy), in tiles, lands on texture pixel
//   px = (gx - gy) * ISO_HW + height * ISO_HW
//   py = (gx + gy) * ISO_HH + HEADROOM
// so tile (0,0) is the top corner of the diamond and x runs down-right, y down-left.
// World space is the texture with its centre at the origin and y up.

/// Half the width / height of a tile's diamond on the texture.
const ISO_HW: f32 = 16.0;
const ISO_HH: f32 = 8.0;
/// Empty rows above the map so buildings on the back edge have room.
const HEADROOM: usize = 64;
/// Depth of the earth slab drawn under the front edges of the map.
const SLAB: usize = 6;

fn iso_size(w: u16, h: u16) -> (usize, usize) {
    let s = (w as usize + h as usize) as f32;
    ((s * ISO_HW) as usize, (s * ISO_HH) as usize + HEADROOM + SLAB)
}

/// Texture pixel of the ground origin (top corner of tile (0,0)).
fn iso_origin(h: u16) -> Vec2 {
    Vec2::new(h as f32 * ISO_HW, HEADROOM as f32)
}

/// Size of the map texture in world units.
pub fn world_size(state: &GameState) -> Vec2 {
    let (w, h) = iso_size(state.map.width, state.map.height);
    Vec2::new(w as f32, h as f32)
}

/// World position of a point on the ground, in tile units.
pub fn ground_to_world(state: &GameState, gx: f32, gy: f32) -> Vec2 {
    let o = iso_origin(state.map.height);
    let px = o.x + (gx - gy) * ISO_HW;
    let py = o.y + (gx + gy) * ISO_HH;
    let size = world_size(state);
    Vec2::new(px - size.x / 2.0, size.y / 2.0 - py)
}

fn world_to_ground(state: &GameState, p: Vec2) -> Vec2 {
    let size = world_size(state);
    let o = iso_origin(state.map.height);
    let a = (p.x + size.x / 2.0 - o.x) / ISO_HW;
    let b = (size.y / 2.0 - p.y - o.y) / ISO_HH;
    Vec2::new((a + b) / 2.0, (b - a) / 2.0)
}

/// World-space centre of a tile. Tile (0,0) is the top corner of the map.
pub fn tile_center(state: &GameState, x: u16, y: u16) -> Vec2 {
    ground_to_world(state, x as f32 + 0.5, y as f32 + 0.5)
}

/// The ground diamond covering tiles `min..=max`, shrunk by `inset` tiles
/// (negative grows it). Corners go top, right, bottom, left.
pub fn footprint(state: &GameState, min: parcels_sim::Pos, max: parcels_sim::Pos, inset: f32) -> [Vec2; 4] {
    let (x0, y0) = (min.x as f32 + inset, min.y as f32 + inset);
    let (x1, y1) = (max.x as f32 + 1.0 - inset, max.y as f32 + 1.0 - inset);
    [
        ground_to_world(state, x0, y0),
        ground_to_world(state, x1, y0),
        ground_to_world(state, x1, y1),
        ground_to_world(state, x0, y1),
    ]
}

/// The tile whose ground is under a world point (buildings don't block picking).
pub fn world_to_tile(state: &GameState, p: Vec2) -> Option<parcels_sim::Pos> {
    let g = world_to_ground(state, p);
    let (w, h) = (state.map.width as f32, state.map.height as f32);
    (g.x >= 0.0 && g.y >= 0.0 && g.x < w && g.y < h).then(|| parcels_sim::Pos::new(g.x as u16, g.y as u16))
}

fn ensure_texture(
    mut commands: Commands,
    driver: Res<Driver>,
    mut images: ResMut<Assets<Image>>,
    existing: Option<ResMut<MapTexture>>,
) {
    let Some(state) = driver.state() else { return };
    let size = (state.map.width, state.map.height);
    if let Some(tex) = &existing {
        if tex.size == size {
            return;
        }
        commands.entity(tex.entity).despawn();
    }
    let (w, h) = iso_size(size.0, size.1);
    let (w, h) = (w as u32, h as u32);
    let mut image = Image::new_fill(
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::nearest();
    let handle = images.add(image);
    let entity = commands
        .spawn((
            Sprite { image: handle.clone(), custom_size: Some(Vec2::new(w as f32, h as f32)), ..default() },
            Transform::from_xyz(0.0, 0.0, 0.0),
        ))
        .id();
    commands.insert_resource(MapTexture { image: handle, entity, painted: None, size, flat: Vec::new() });
}

fn despawn_texture(mut commands: Commands, tex: Option<Res<MapTexture>>) {
    if let Some(tex) = tex {
        commands.entity(tex.entity).despawn();
        commands.remove_resource::<MapTexture>();
    }
}

fn repaint(driver: Res<Driver>, view: Res<View>, tex: Option<ResMut<MapTexture>>, mut images: ResMut<Assets<Image>>) {
    let (Some(state), Some(mut tex)) = (driver.state(), tex) else { return };
    let key = (state.tick, driver.generation, view.overlay, view.active, view.show_pipes);
    if tex.painted == Some(key) {
        return;
    }
    let Some(mut image) = images.get_mut(&tex.image) else { return };
    let Some(data) = image.data.as_mut() else { return };
    paint(state, &view, &mut tex.flat, data);
    tex.painted = Some(key);
}

// ---------------------------------------------------------------------------
// Software painter

struct Canvas<'a> {
    buf: &'a mut [u8],
    stride: usize,
}

impl Canvas<'_> {
    #[inline]
    fn put(&mut self, x: usize, y: usize, c: [u8; 3]) {
        let i = (y * self.stride + x) * 4;
        self.buf[i..i + 3].copy_from_slice(&c);
        self.buf[i + 3] = 255;
    }

    fn rect(&mut self, ox: usize, oy: usize, x0: usize, y0: usize, w: usize, h: usize, c: [u8; 3]) {
        for y in y0..(y0 + h).min(TILE_PX) {
            for x in x0..(x0 + w).min(TILE_PX) {
                self.put(ox + x, oy + y, c);
            }
        }
    }

    fn get(&self, x: usize, y: usize) -> [u8; 3] {
        let i = (y * self.stride + x) * 4;
        [self.buf[i], self.buf[i + 1], self.buf[i + 2]]
    }
}

fn mix(a: [u8; 3], b: [u8; 3], t: u32) -> [u8; 3] {
    let f = |x: u8, y: u8| ((x as u32 * (256 - t) + y as u32 * t) >> 8) as u8;
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}

fn shade(c: [u8; 3], pct: u32) -> [u8; 3] {
    [(c[0] as u32 * pct / 100) as u8, (c[1] as u32 * pct / 100) as u8, (c[2] as u32 * pct / 100) as u8]
}

/// Cosmetic per-pixel noise. Render-only; never used by the sim.
fn noise(x: usize, y: usize) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// Heat ramp for 0..=255: blue → green → yellow → red.
fn heat(v: u32) -> [u8; 3] {
    let v = v.min(255);
    if v < 85 {
        mix([40, 80, 200], [60, 190, 90], v * 3)
    } else if v < 170 {
        mix([60, 190, 90], [240, 220, 60], (v - 85) * 3)
    } else {
        mix([240, 220, 60], [220, 50, 40], (v - 170) * 3)
    }
}

const ZONE_TINT: [[u8; 3]; 3] = [[120, 200, 110], [110, 160, 235], [225, 185, 80]];

/// How a tile's pixels are recoloured for the current overlay and active player.
#[derive(Clone, Copy, Default)]
struct Filter {
    tint: Option<([u8; 3], u32)>,
    dim: bool,
}

impl Filter {
    fn of(state: &GameState, view: &View, idx: usize) -> Self {
        let t = &state.map.tiles[idx];
        let max_lv = state.config.land_value_max.max(1) as u32;
        let max_occ = (state.config.max_level as u32 * state.config.residents_per_level as u32).max(1);
        let owner = state.owner_of_idx(idx);
        let tint = match view.overlay {
            Overlay::None => None,
            Overlay::LandValue => Some((heat(255 - t.land_value as u32 * 255 / max_lv), 150)),
            Overlay::Pollution => Some((heat(t.pollution as u32 * 255 / 200), 150)),
            Overlay::Traffic => Some((heat(t.traffic as u32 * 255 / 60), if t.kind == TileKind::Road { 200 } else { 60 })),
            Overlay::Density => Some((heat(state.occupants(idx) * 255 / max_occ), if t.kind.is_zone() { 170 } else { 40 })),
            Overlay::Power => utility_tint(t.conducts_power(), t.powered),
            Overlay::Water => utility_tint(t.conducts_water(), t.watered),
            Overlay::Owners => owner.map(|o| (player_color(o), 110)),
        };
        let dim = view.active.is_some() && owner != view.active && view.overlay == Overlay::None;
        Filter { tint, dim }
    }

    fn apply(self, mut c: [u8; 3]) -> [u8; 3] {
        if let Some((tc, a)) = self.tint {
            c = mix(c, tc, a);
        }
        if self.dim {
            c = shade(c, 82);
        }
        c
    }

    fn is_identity(self) -> bool {
        self.tint.is_none() && !self.dim
    }
}

/// Paint the whole map into `out`, an RGBA buffer of `iso_size` pixels.
/// `flat` is scratch space for the top-down ground layer.
pub fn paint(state: &GameState, view: &View, flat: &mut Vec<u8>, out: &mut [u8]) {
    flat.resize(state.map.len() * TILE_PX * TILE_PX * 4, 0);
    paint_flat(state, view, flat);
    project_ground(state, flat, out);
    paint_objects(state, view, out);
}

/// Top-down ground layer: everything that lies flat on a tile.
fn paint_flat(state: &GameState, view: &View, buf: &mut [u8]) {
    let map = &state.map;
    let stride = map.width as usize * TILE_PX;
    let mut cv = Canvas { buf, stride };
    for idx in 0..map.len() {
        let pos = map.pos(idx);
        let (ox, oy) = (pos.x as usize * TILE_PX, pos.y as usize * TILE_PX);
        let t = &map.tiles[idx];
        draw_ground(&mut cv, state, idx, t, ox, oy, view.show_pipes || view.overlay == Overlay::Water);
        let filter = Filter::of(state, view, idx);
        if !filter.is_identity() {
            for y in 0..TILE_PX {
                for x in 0..TILE_PX {
                    let c = cv.get(ox + x, oy + y);
                    cv.put(ox + x, oy + y, filter.apply(c));
                }
            }
        }
    }
}

/// Reproject the flat layer onto diamonds, with an earth slab under the front edges.
fn project_ground(state: &GameState, flat: &[u8], out: &mut [u8]) {
    let (mw, mh) = (state.map.width as f32, state.map.height as f32);
    let (w, h) = iso_size(state.map.width, state.map.height);
    let o = iso_origin(state.map.height);
    let fstride = state.map.width as usize * TILE_PX;
    let ground = |px: f32, py: f32| -> Option<(usize, usize)> {
        let a = (px - o.x) / ISO_HW;
        let b = (py - o.y) / ISO_HH;
        let (gx, gy) = ((a + b) / 2.0, (b - a) / 2.0);
        (gx >= 0.0 && gy >= 0.0 && gx < mw && gy < mh)
            .then(|| (((gx * TILE_PX as f32) as usize).min(fstride - 1), (gy * TILE_PX as f32) as usize))
    };
    for py in 0..h {
        for px in 0..w {
            let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
            let i = (py * w + px) * 4;
            let c = if let Some((sx, sy)) = ground(fx, fy) {
                let j = (sy * fstride + sx) * 4;
                Some([flat[j], flat[j + 1], flat[j + 2]])
            } else {
                // Under the front edges: walk up to find the ground this slab hangs from.
                (1..=SLAB).find_map(|d| ground(fx, fy - d as f32).map(|g| (d, g))).map(|(d, (sx, sy))| {
                    let idx = state.map.idx(parcels_sim::Pos::new((sx / TILE_PX) as u16, (sy / TILE_PX) as u16));
                    let wet = state.map.tiles[idx].terrain == Terrain::Water;
                    let left = fx < o.x + (mw - mh) * ISO_HW;
                    let base = match (wet, d) {
                        (true, _) => [36, 80, 150],
                        (false, 1) => [70, 110, 55],
                        _ => [112, 84, 58],
                    };
                    shade(base, if left { 85 } else { 65 })
                })
            };
            match c {
                Some(c) => {
                    out[i..i + 3].copy_from_slice(&c);
                    out[i + 3] = 255;
                }
                None => out[i..i + 4].copy_from_slice(&[0, 0, 0, 0]),
            }
        }
    }
}

fn utility_tint(conducts: bool, served: bool) -> Option<([u8; 3], u32)> {
    Some(match (conducts, served) {
        (true, true) => ([60, 210, 90], 150),
        (true, false) => ([230, 50, 40], 170),
        (false, true) => ([60, 210, 90], 60),
        (false, false) => ([20, 20, 30], 140),
    })
}

fn neighbor(state: &GameState, idx: usize, dx: i32, dy: i32) -> Option<&Tile> {
    let p = state.map.pos(idx);
    let (x, y) = (p.x as i32 + dx, p.y as i32 + dy);
    if x < 0 || y < 0 {
        return None;
    }
    state.map.get(parcels_sim::Pos::new(x as u16, y as u16))
}

const DIRS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];

fn draw_ground(cv: &mut Canvas, state: &GameState, idx: usize, t: &Tile, ox: usize, oy: usize, pipes: bool) {
    // Ground.
    for y in 0..TILE_PX {
        for x in 0..TILE_PX {
            let n = noise(ox + x, oy + y);
            let c = if t.terrain == Terrain::Water {
                if n % 23 == 0 { [90, 150, 220] } else { [44, 96, 176] }
            } else {
                let v = (n % 14) as u8;
                [52 + v, 112 + v, 56 + v / 2]
            };
            cv.put(ox + x, oy + y, c);
        }
    }
    match t.kind {
        TileKind::Empty => {}
        TileKind::Road => draw_road(cv, state, idx, t, ox, oy),
        TileKind::Park => {
            cv.rect(ox, oy, 1, 1, 14, 14, [78, 150, 70]);
            cv.rect(ox, oy, 3, 9, 9, 2, [170, 150, 110]);
        }
        TileKind::PowerPlant => cv.rect(ox, oy, 0, 0, 16, 16, [120, 118, 112]),
        TileKind::WaterPump => {
            cv.rect(ox, oy, 0, 0, 16, 16, [120, 118, 112]);
            cv.rect(ox, oy, 2, 6, 9, 8, [70, 110, 150]);
            cv.rect(ox, oy, 3, 7, 7, 6, [70, 160, 230]);
        }
        TileKind::Residential | TileKind::Commercial | TileKind::Industrial => draw_lot(cv, t, ox, oy),
    }
    if t.wire {
        let c = [235, 205, 60];
        let (cx, cy) = (11, 4);
        for (dx, dy) in DIRS {
            let joins = neighbor(state, idx, dx, dy).is_some_and(|n| n.conducts_power());
            if joins {
                match (dx, dy) {
                    (0, -1) => cv.rect(ox, oy, cx, 0, 1, cy, c),
                    (0, 1) => cv.rect(ox, oy, cx, cy, 1, TILE_PX - cy, c),
                    (-1, 0) => cv.rect(ox, oy, 0, cy, cx, 1, c),
                    _ => cv.rect(ox, oy, cx, cy, TILE_PX - cx, 1, c),
                }
            }
        }
        cv.rect(ox, oy, cx - 1, cy - 1, 3, 3, [90, 60, 30]);
    }
    if t.pipe && pipes {
        let c = [80, 170, 250];
        let (cx, cy) = (4, 11);
        for (dx, dy) in DIRS {
            if neighbor(state, idx, dx, dy).is_some_and(|n| n.conducts_water()) {
                match (dx, dy) {
                    (0, -1) => cv.rect(ox, oy, cx, 0, 2, cy, c),
                    (0, 1) => cv.rect(ox, oy, cx, cy, 2, TILE_PX - cy, c),
                    (-1, 0) => cv.rect(ox, oy, 0, cy, cx, 2, c),
                    _ => cv.rect(ox, oy, cx, cy, TILE_PX - cx, 2, c),
                }
            }
        }
        cv.rect(ox, oy, cx - 1, cy - 1, 4, 4, c);
    }
}

fn draw_road(cv: &mut Canvas, state: &GameState, idx: usize, t: &Tile, ox: usize, oy: usize) {
    let asphalt = [72, 72, 78];
    if t.terrain == Terrain::Water {
        cv.rect(ox, oy, 0, 2, 16, 12, [120, 110, 100]);
    }
    let is_road = |dx, dy| neighbor(state, idx, dx, dy).is_some_and(|n| n.kind == TileKind::Road);
    let (n, w, e, s) = (is_road(0, -1), is_road(-1, 0), is_road(1, 0), is_road(0, 1));
    // Body: centre square plus arms toward connected roads.
    cv.rect(ox, oy, 2, 2, 12, 12, asphalt);
    if n {
        cv.rect(ox, oy, 2, 0, 12, 2, asphalt);
    }
    if s {
        cv.rect(ox, oy, 2, 14, 12, 2, asphalt);
    }
    if w {
        cv.rect(ox, oy, 0, 2, 2, 12, asphalt);
    }
    if e {
        cv.rect(ox, oy, 14, 2, 2, 12, asphalt);
    }
    let line = [210, 200, 120];
    let horizontal = (w || e) && !(n || s);
    let vertical = (n || s) && !(w || e);
    if horizontal {
        for x in (1..16).step_by(4) {
            cv.rect(ox, oy, x, 7, 2, 1, line);
        }
    } else if vertical {
        for y in (1..16).step_by(4) {
            cv.rect(ox, oy, 7, y, 1, 2, line);
        }
    } else {
        cv.rect(ox, oy, 7, 7, 2, 2, [95, 95, 100]);
    }
}

fn draw_lot(cv: &mut Canvas, t: &Tile, ox: usize, oy: usize) {
    let tint = ZONE_TINT[t.kind.zone().unwrap().index()];
    // Zoned lot: tinted ground with a dotted border.
    cv.rect(ox, oy, 0, 0, 16, 16, mix([60, 110, 60], tint, 110));
    let border = shade(tint, 70);
    for i in (0..16).step_by(2) {
        cv.rect(ox, oy, i, 0, 1, 1, border);
        cv.rect(ox, oy, i, 15, 1, 1, border);
        cv.rect(ox, oy, 0, i, 1, 1, border);
        cv.rect(ox, oy, 15, i, 1, 1, border);
    }
    if t.level > 0 {
        // Paved yard under the building.
        cv.rect(ox, oy, 2, 2, 12, 12, mix([150, 150, 140], tint, 40));
    }
}

// ---------------------------------------------------------------------------
// 3D pass: buildings as boxes, painted back to front

#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    Top,
    /// Faces +y (front-left on screen).
    Left,
    /// Faces +x (front-right on screen).
    Right,
}

/// Light from the upper left.
fn lit(face: Face, c: [u8; 3]) -> [u8; 3] {
    match face {
        Face::Top => mix(c, [255, 255, 255], 30),
        Face::Left => shade(c, 84),
        Face::Right => shade(c, 64),
    }
}

fn solid(c: [u8; 3]) -> impl Fn(Face, f32, f32) -> [u8; 3] {
    move |f, _, _| lit(f, c)
}

/// Facade with a grid of windows; `cols` per face, a floor every `floor` px.
fn windows(wall: [u8; 3], glass: [u8; 3], roof: [u8; 3], cols: f32, floor: f32) -> impl Fn(Face, f32, f32) -> [u8; 3] {
    move |f, s, z| {
        if f == Face::Top {
            return lit(f, roof);
        }
        let col = (s * cols).fract();
        let row = z % floor;
        let pane = (0.3..0.75).contains(&col) && row >= 1.0 && row < floor - 1.0 && z >= 2.0;
        lit(f, if pane { glass } else { wall })
    }
}

struct Iso<'a> {
    buf: &'a mut [u8],
    w: usize,
    h: usize,
    origin: Vec2,
    filter: Filter,
    /// Tile being drawn; block coordinates are local to it (0..1).
    at: (f32, f32),
    /// Highest point drawn on this tile, for placing badges.
    top: f32,
}

impl Iso<'_> {
    fn put(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        self.buf[i..i + 3].copy_from_slice(&self.filter.apply(c));
        self.buf[i + 3] = 255;
    }

    fn screen(&self, gx: f32, gy: f32, z: f32) -> Vec2 {
        Vec2::new(self.origin.x + (gx - gy) * ISO_HW, self.origin.y + (gx + gy) * ISO_HH - z)
    }

    /// A box over local footprint (u0,v0)-(u1,v1) from height z0 to z1 (pixels).
    /// `paint` gets the face, the position across it (0..1; for the top, u) and
    /// the height above z0 (for the top, v).
    fn block(&mut self, (u0, v0): (f32, f32), (u1, v1): (f32, f32), z0: f32, z1: f32, paint: impl Fn(Face, f32, f32) -> [u8; 3]) {
        let (gx0, gy0) = (self.at.0 + u0, self.at.1 + v0);
        let (gx1, gy1) = (self.at.0 + u1, self.at.1 + v1);
        let (ox, oy) = (self.origin.x, self.origin.y);
        let left = self.screen(gx0, gy1, 0.0).x.floor() as i32;
        let right = self.screen(gx1, gy0, 0.0).x.ceil() as i32;
        let top = self.screen(gx0, gy0, z1).y.floor() as i32;
        let bottom = self.screen(gx1, gy1, z0).y.ceil() as i32;
        for py in top..bottom {
            for px in left..right {
                let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                let a = (fx - ox) / ISO_HW;
                // Top face.
                let b = (fy + z1 - oy) / ISO_HH;
                let (gx, gy) = ((a + b) / 2.0, (b - a) / 2.0);
                if gx >= gx0 && gx < gx1 && gy >= gy0 && gy < gy1 {
                    self.put(px, py, paint(Face::Top, (gx - gx0) / (gx1 - gx0), (gy - gy0) / (gy1 - gy0)));
                    continue;
                }
                // Left face lies in the plane gy = gy1.
                let t = a + gy1;
                if t >= gx0 && t < gx1 {
                    let z = (t + gy1) * ISO_HH + oy - fy;
                    if z >= z0 && z < z1 {
                        self.put(px, py, paint(Face::Left, (t - gx0) / (gx1 - gx0), z - z0));
                        continue;
                    }
                }
                // Right face lies in the plane gx = gx1.
                let s = gx1 - a;
                if s >= gy0 && s < gy1 {
                    let z = (gx1 + s) * ISO_HH + oy - fy;
                    if z >= z0 && z < z1 {
                        self.put(px, py, paint(Face::Right, (s - gy0) / (gy1 - gy0), z - z0));
                    }
                }
            }
        }
        self.top = self.top.max(z1);
    }

    /// A hipped roof as 1px steps shrinking toward the ridge.
    fn roof(&mut self, (u0, v0): (f32, f32), (u1, v1): (f32, f32), z: f32, height: u32, c: [u8; 3]) {
        let step = (u1 - u0).min(v1 - v0) / 2.0 / height as f32;
        for k in 0..height {
            let i = k as f32 * step;
            let zk = z + k as f32;
            self.block((u0 + i, v0 + i), (u1 - i, v1 - i), zk, zk + 1.0, solid(c));
        }
    }

    fn house(&mut self, lo: (f32, f32), hi: (f32, f32), wall: f32, roof: u32) {
        self.block(lo, hi, 0.0, wall, windows([225, 215, 190], [90, 120, 160], [225, 215, 190], 2.0, wall));
        self.roof((lo.0 - 0.03, lo.1 - 0.03), (hi.0 + 0.03, hi.1 + 0.03), wall, roof, [170, 70, 55]);
    }

    fn tree(&mut self, u: f32, v: f32, size: f32) {
        self.block((u - 0.03, v - 0.03), (u + 0.03, v + 0.03), 0.0, 3.0, solid([110, 80, 50]));
        let layers = (8.0 * size) as u32;
        for k in 0..layers {
            let r = 0.17 * size * (1.0 - k as f32 / layers as f32) + 0.02;
            let z = 3.0 + k as f32;
            let c = if k % 3 == 0 { [40, 110, 45] } else { [34, 95, 40] };
            self.block((u - r, v - r), (u + r, v + r), z, z + 1.0, solid(c));
        }
    }

    /// A small warning sign floating over the tile.
    fn badge(&mut self, color: [u8; 3]) {
        let p = self.screen(self.at.0 + 0.5, self.at.1 + 0.5, self.top + 4.0);
        let (x, y) = (p.x as i32 - 2, p.y as i32 - 6);
        for dy in 0..7 {
            for dx in 0..5 {
                let edge = dx == 0 || dx == 4 || dy == 0 || dy == 4 || dy == 6;
                let c = if edge { [30, 30, 30] } else { color };
                self.put(x + dx, y + dy, c);
            }
        }
    }
}

fn paint_objects(state: &GameState, view: &View, buf: &mut [u8]) {
    let (w, h) = iso_size(state.map.width, state.map.height);
    let mut iso = Iso { buf, w, h, origin: iso_origin(state.map.height), filter: Filter::default(), at: (0.0, 0.0), top: 0.0 };
    let (mw, mh) = (state.map.width as usize, state.map.height as usize);
    // Back to front: each diagonal x + y = d only overlaps the ones after it.
    for d in 0..mw + mh - 1 {
        for x in d.saturating_sub(mh - 1)..=d.min(mw - 1) {
            let pos = parcels_sim::Pos::new(x as u16, (d - x) as u16);
            let idx = state.map.idx(pos);
            let t = &state.map.tiles[idx];
            iso.filter = Filter::of(state, view, idx);
            iso.at = (pos.x as f32, pos.y as f32);
            iso.top = 0.0;
            draw_object(&mut iso, t);
            if t.kind.is_zone() && !t.powered {
                iso.badge([250, 70, 50]);
            } else if t.kind.is_zone() && t.level >= state.config.water_free_levels && !t.watered {
                iso.badge([80, 170, 250]);
            }
        }
    }
}

fn draw_object(iso: &mut Iso, t: &Tile) {
    match t.kind {
        TileKind::Empty | TileKind::Road => {}
        TileKind::Park => {
            iso.tree(0.27, 0.25, 1.0);
            iso.tree(0.72, 0.32, 0.8);
            iso.tree(0.3, 0.72, 0.9);
        }
        TileKind::PowerPlant => {
            let tower = |f: Face, _: f32, _: f32| if f == Face::Top { [60, 60, 60] } else { lit(f, [175, 175, 170]) };
            iso.block((0.08, 0.08), (0.42, 0.42), 0.0, 20.0, tower);
            iso.block((0.55, 0.1), (0.85, 0.4), 0.0, 15.0, tower);
            iso.block((0.08, 0.52), (0.92, 0.92), 0.0, 9.0, |f, _, z| {
                lit(f, if f != Face::Top && (4.0..6.0).contains(&z) { [250, 210, 60] } else { [130, 62, 50] })
            });
        }
        TileKind::WaterPump => {
            iso.block((0.66, 0.12), (0.92, 0.42), 0.0, 6.0, solid([90, 90, 100]));
            iso.roof((0.64, 0.1), (0.94, 0.44), 6.0, 3, [70, 110, 150]);
        }
        TileKind::Residential | TileKind::Commercial | TileKind::Industrial => draw_building(iso, t),
    }
}

fn draw_building(iso: &mut Iso, t: &Tile) {
    let lv = t.level as f32;
    if t.level == 0 {
        return;
    }
    match t.kind.zone().unwrap() {
        Zone::Residential => match t.level {
            1 => iso.house((0.25, 0.28), (0.75, 0.74), 6.0, 5),
            2 => {
                iso.house((0.1, 0.12), (0.46, 0.48), 6.0, 4);
                iso.house((0.52, 0.5), (0.9, 0.88), 7.0, 4);
            }
            3 => {
                iso.block((0.14, 0.14), (0.86, 0.86), 0.0, 16.0, windows([200, 180, 150], [90, 120, 160], [150, 140, 130], 4.0, 4.0));
                iso.block((0.4, 0.4), (0.6, 0.6), 16.0, 19.0, solid([160, 160, 160]));
            }
            _ => {
                iso.block((0.16, 0.16), (0.84, 0.84), 0.0, 32.0, windows([190, 160, 140], [110, 150, 200], [140, 130, 125], 4.0, 4.0));
                iso.block((0.35, 0.35), (0.62, 0.62), 32.0, 36.0, solid([160, 160, 160]));
            }
        },
        Zone::Commercial => {
            let height = 6.0 + lv * 8.0;
            let body = mix([90, 120, 170], [60, 90, 150], (t.level as u32) * 40);
            iso.block((0.12, 0.12), (0.88, 0.88), 0.0, height, move |f, _, z| {
                if f == Face::Top {
                    return lit(f, [200, 205, 215]);
                }
                lit(f, if z >= 2.0 && z % 3.0 < 1.5 { [170, 210, 245] } else { body })
            });
            if t.level >= 3 {
                iso.block((0.48, 0.48), (0.54, 0.54), height, height + 8.0, solid([200, 200, 210]));
            }
        }
        Zone::Industrial => {
            let body = [150, 130, 95];
            for s in 0..t.level.min(3) {
                let u = 0.14 + s as f32 * 0.26;
                let tall = 16.0 + s as f32 * 3.0;
                iso.block((u, 0.06), (u + 0.1, 0.16), 0.0, tall, move |f, _, z| {
                    lit(f, if z >= tall - 2.0 { [210, 210, 210] } else { [110, 100, 95] })
                });
            }
            let shed = if t.level == 4 { 11.0 } else { 7.0 };
            iso.block((0.06, 0.22), (0.94, 0.94), 0.0, shed, solid(body));
            // Sawtooth roof.
            for k in 0..3 {
                let v = 0.26 + k as f32 * 0.23;
                iso.block((0.08, v), (0.92, v + 0.1), shed, shed + 3.0, |f, _, _| {
                    lit(f, if f == Face::Left { [150, 190, 215] } else { shade(body, 85) })
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parcels_sim::{Config, NewGame, Session};

    #[test]
    fn tile_world_roundtrip() {
        let s = GameState::new(&NewGame::solo(1, Config::default(), "x", 3));
        for (x, y) in [(0, 0), (63, 47), (10, 30)] {
            let c = tile_center(&s, x, y);
            assert_eq!(world_to_tile(&s, c), Some(parcels_sim::Pos::new(x, y)));
        }
        assert_eq!(world_to_tile(&s, Vec2::new(1e6, 0.0)), None);
        // The corners of the texture are outside the diamond.
        let half = world_size(&s) / 2.0;
        assert_eq!(world_to_tile(&s, Vec2::new(-half.x + 2.0, half.y - 2.0)), None);
    }

    #[test]
    fn paints_a_grown_city_with_every_overlay() {
        let mut session = Session::new(GameState::new(&NewGame::solo(2, Config::default(), "x", 3)));
        for _ in 0..400 {
            session.tick();
        }
        let s = &session.state;
        let (w, h) = iso_size(s.map.width, s.map.height);
        let mut buf = vec![0u8; w * h * 4];
        let mut flat = Vec::new();
        for o in Overlay::ALL {
            let view = View { overlay: o, active: Some(PlayerId(0)), show_pipes: true };
            paint(s, &view, &mut flat, &mut buf);
        }
        // Every tile centre is covered by opaque ground.
        for (x, y) in [(0, 0), (63, 0), (0, 47), (63, 47), (30, 20)] {
            let p = tile_center(s, x, y) * Vec2::new(1.0, -1.0) + Vec2::new(w as f32, h as f32) / 2.0;
            assert_eq!(buf[(p.y as usize * w + p.x as usize) * 4 + 3], 255, "tile ({x},{y})");
        }
    }
}
