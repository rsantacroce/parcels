//! Turns a chunk of the tile grid into triangle soup: ground, roads, buildings,
//! trees, poles. Pure CPU code with no Bevy systems, so it is easy to test and
//! cheap to rerun when a chunk changes. Everything is vertex-coloured; windows
//! and lamps go into separate buffers so night can light them up.
//!
//! Units are metres. A tile is `TILE` metres square; tile (x, y) covers world
//! X in [x*TILE, (x+1)*TILE) and Z in [y*TILE, (y+1)*TILE), with Y up.

use bevy::prelude::*;
use parcels_sim::{Building, Density, GameState, Pos, Rect, Road, Terrain, Tile, TileKind, Zone};

use crate::view::{mix, overlay_tint, shade, Overlay, ZONE_TINT};

pub const TILE: f32 = 10.0;
pub const WATER_Y: f32 = -0.6;
const BED_Y: f32 = -2.2;
const SIDEWALK: f32 = 1.8;
const CURB: f32 = 0.15;

/// sRGB bytes to linear vertex colour.
pub fn lin(c: [u8; 3]) -> [f32; 4] {
    let f = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    [f(c[0]), f(c[1]), f(c[2]), 1.0]
}

/// Cosmetic hash. Render-only; never used by the sim.
pub fn noise(x: u32, y: u32, salt: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77) ^ salt.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

#[derive(Default, Clone)]
pub struct MeshBuf {
    pub pos: Vec<[f32; 3]>,
    pub nrm: Vec<[f32; 3]>,
    pub col: Vec<[f32; 4]>,
    pub idx: Vec<u32>,
}

impl MeshBuf {
    pub fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    /// A flat quad. Winding is fixed up so the face points along `n`.
    pub fn quad(&mut self, v: [Vec3; 4], n: Vec3, color: [u8; 3]) {
        let c = lin(color);
        let base = self.pos.len() as u32;
        for p in v {
            self.pos.push(p.to_array());
            self.nrm.push(n.to_array());
            self.col.push(c);
        }
        let facing = (v[1] - v[0]).cross(v[2] - v[0]).dot(n) >= 0.0;
        if facing {
            self.idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        } else {
            self.idx.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
        }
    }

    pub fn tri(&mut self, v: [Vec3; 3], color: [u8; 3]) {
        let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize_or_zero();
        let c = lin(color);
        let base = self.pos.len() as u32;
        for p in v {
            self.pos.push(p.to_array());
            self.nrm.push(n.to_array());
            self.col.push(c);
        }
        self.idx.extend([base, base + 1, base + 2]);
    }

    /// Horizontal rectangle at height `y`, facing up.
    pub fn flat(&mut self, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, color: [u8; 3]) {
        self.quad(
            [Vec3::new(x0, y, z0), Vec3::new(x0, y, z1), Vec3::new(x1, y, z1), Vec3::new(x1, y, z0)],
            Vec3::Y,
            color,
        );
    }

    /// Axis-aligned box without a bottom. `top` colours the lid.
    pub fn cuboid(&mut self, min: Vec3, max: Vec3, side: [u8; 3], top: [u8; 3]) {
        let (a, b) = (min, max);
        self.flat(a.x, a.z, b.x, b.z, b.y, top);
        self.quad([Vec3::new(a.x, a.y, b.z), Vec3::new(b.x, a.y, b.z), Vec3::new(b.x, b.y, b.z), Vec3::new(a.x, b.y, b.z)], Vec3::Z, side);
        self.quad([Vec3::new(a.x, a.y, a.z), Vec3::new(a.x, b.y, a.z), Vec3::new(b.x, b.y, a.z), Vec3::new(b.x, a.y, a.z)], -Vec3::Z, side);
        self.quad([Vec3::new(b.x, a.y, a.z), Vec3::new(b.x, b.y, a.z), Vec3::new(b.x, b.y, b.z), Vec3::new(b.x, a.y, b.z)], Vec3::X, side);
        self.quad([Vec3::new(a.x, a.y, a.z), Vec3::new(a.x, a.y, b.z), Vec3::new(a.x, b.y, b.z), Vec3::new(a.x, b.y, a.z)], -Vec3::X, side);
    }

    pub fn block(&mut self, min: Vec3, max: Vec3, color: [u8; 3]) {
        self.cuboid(min, max, color, color);
    }

    /// Gable roof over the box `min..max` (y from eave to ridge). The ridge runs
    /// along X if `along_x`, else along Z. `wall` fills the triangular ends.
    pub fn gable(&mut self, min: Vec3, max: Vec3, along_x: bool, roof: [u8; 3], wall: [u8; 3]) {
        let (y0, y1) = (min.y, max.y);
        if along_x {
            let zm = (min.z + max.z) / 2.0;
            let up_s = Vec3::new(0.0, max.z - zm, y1 - y0).normalize();
            self.quad([Vec3::new(min.x, y0, max.z), Vec3::new(max.x, y0, max.z), Vec3::new(max.x, y1, zm), Vec3::new(min.x, y1, zm)], Vec3::new(0.0, up_s.y, up_s.z).normalize(), roof);
            self.quad([Vec3::new(min.x, y0, min.z), Vec3::new(min.x, y1, zm), Vec3::new(max.x, y1, zm), Vec3::new(max.x, y0, min.z)], Vec3::new(0.0, up_s.y, -up_s.z).normalize(), roof);
            self.tri([Vec3::new(min.x, y0, min.z), Vec3::new(min.x, y0, max.z), Vec3::new(min.x, y1, zm)], wall);
            self.tri([Vec3::new(max.x, y0, max.z), Vec3::new(max.x, y0, min.z), Vec3::new(max.x, y1, zm)], wall);
        } else {
            let xm = (min.x + max.x) / 2.0;
            let up_s = Vec3::new(max.x - xm, y1 - y0, 0.0).normalize();
            self.quad([Vec3::new(max.x, y0, min.z), Vec3::new(xm, y1, min.z), Vec3::new(xm, y1, max.z), Vec3::new(max.x, y0, max.z)], Vec3::new(up_s.y, up_s.x, 0.0).normalize(), roof);
            self.quad([Vec3::new(min.x, y0, min.z), Vec3::new(min.x, y0, max.z), Vec3::new(xm, y1, max.z), Vec3::new(xm, y1, min.z)], Vec3::new(-up_s.y, up_s.x, 0.0).normalize(), roof);
            self.tri([Vec3::new(min.x, y0, max.z), Vec3::new(max.x, y0, max.z), Vec3::new(xm, y1, max.z)], wall);
            self.tri([Vec3::new(max.x, y0, min.z), Vec3::new(min.x, y0, min.z), Vec3::new(xm, y1, min.z)], wall);
        }
    }

    /// Four-sided pyramid (hip roof with a point, spire).
    pub fn pyramid(&mut self, min: Vec3, max: Vec3, color: [u8; 3]) {
        let apex = Vec3::new((min.x + max.x) / 2.0, max.y, (min.z + max.z) / 2.0);
        let y = min.y;
        let c = [Vec3::new(min.x, y, min.z), Vec3::new(max.x, y, min.z), Vec3::new(max.x, y, max.z), Vec3::new(min.x, y, max.z)];
        for i in 0..4 {
            self.tri([c[(i + 1) % 4], c[i], apex], color);
        }
    }

    /// Upright prism approximating a cylinder, with a lid.
    pub fn cylinder(&mut self, base: Vec3, r: f32, h: f32, seg: usize, side: [u8; 3], top: [u8; 3]) {
        self.frustum(base, r, r, h, seg, side, top);
    }

    /// Like `cylinder` but tapering to `r1` at the top (cooling towers, trunks).
    pub fn frustum(&mut self, base: Vec3, r0: f32, r1: f32, h: f32, seg: usize, side: [u8; 3], top: [u8; 3]) {
        let ring = |r: f32, y: f32| -> Vec<Vec3> {
            (0..seg)
                .map(|i| {
                    let a = i as f32 / seg as f32 * std::f32::consts::TAU;
                    base + Vec3::new(a.cos() * r, y, a.sin() * r)
                })
                .collect()
        };
        let (lo, hi) = (ring(r0, 0.0), ring(r1, h));
        for i in 0..seg {
            let j = (i + 1) % seg;
            let mid = ((lo[i] + lo[j]) / 2.0 - base).with_y(0.0).normalize_or_zero();
            self.quad([lo[i], hi[i], hi[j], lo[j]], mid, side);
        }
        let center = base + Vec3::Y * h;
        for i in 0..seg {
            let j = (i + 1) % seg;
            self.tri([hi[j], hi[i], center], top);
        }
    }

    /// Cone (tree crowns, spires).
    pub fn cone(&mut self, base: Vec3, r: f32, h: f32, seg: usize, color: [u8; 3]) {
        let apex = base + Vec3::Y * h;
        for i in 0..seg {
            let a0 = i as f32 / seg as f32 * std::f32::consts::TAU;
            let a1 = (i + 1) as f32 / seg as f32 * std::f32::consts::TAU;
            let p0 = base + Vec3::new(a0.cos() * r, 0.0, a0.sin() * r);
            let p1 = base + Vec3::new(a1.cos() * r, 0.0, a1.sin() * r);
            self.tri([p1, p0, apex], color);
        }
    }

    /// A thin bar between two points (wires, railings), square in section.
    pub fn bar(&mut self, a: Vec3, b: Vec3, w: f32, color: [u8; 3]) {
        let d = (b - a).normalize_or_zero();
        let side = if d.y.abs() > 0.9 { Vec3::X } else { d.cross(Vec3::Y).normalize() };
        let up = side.cross(d).normalize();
        let (s, u) = (side * w / 2.0, up * w / 2.0);
        self.quad([a + u - s, a + u + s, b + u + s, b + u - s], up, color);
        self.quad([a - u - s, b - u - s, b - u + s, a - u + s], -up, color);
        self.quad([a - s - u, a - s + u, b - s + u, b - s - u], -side, color);
        self.quad([a + s - u, b + s - u, b + s + u, a + s + u], side, color);
    }

    pub fn into_mesh(self) -> Mesh {
        use bevy::asset::RenderAssetUsages;
        use bevy::mesh::{Indices, PrimitiveTopology};
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.pos)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.nrm)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.col)
            .with_inserted_indices(Indices::U32(self.idx))
    }
}

/// Everything one chunk draws, split by material.
#[derive(Default)]
pub struct ChunkMeshes {
    pub solid: MeshBuf,
    /// Windows that stay dark.
    pub glass: MeshBuf,
    /// Windows and lamps that glow at night.
    pub lights: MeshBuf,
    /// Translucent water surface.
    pub water: MeshBuf,
}

/// Options that change how the world is drawn (not what it is).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Look {
    pub overlay: Overlay,
    pub show_pipes: bool,
}

/// Wall, glass and accent palettes per building variant.
const WALLS: [[u8; 3]; 8] = [
    [232, 222, 200],
    [210, 196, 170],
    [196, 214, 222],
    [214, 222, 190],
    [236, 206, 190],
    [180, 120, 96],
    [222, 218, 210],
    [170, 160, 150],
];
const ROOFS: [[u8; 3]; 5] = [[168, 72, 56], [110, 84, 70], [88, 96, 110], [140, 60, 50], [70, 76, 86]];
const GLASS: [u8; 3] = [92, 120, 150];
const LIT: [u8; 3] = [255, 214, 140];
/// Windows that light up at night: glassy by day, warm by night (emissive).
const WINDOW: [u8; 3] = [120, 140, 160];
const CONCRETE: [u8; 3] = [168, 166, 160];
const ASPHALT: [u8; 3] = [64, 64, 70];
const SIDEWALK_C: [u8; 3] = [178, 176, 170];
const GRASS: [u8; 3] = [86, 140, 74];
const TRUNK: [u8; 3] = [104, 76, 52];

struct Ctx<'a> {
    state: &'a GameState,
    look: Look,
    m: &'a mut ChunkMeshes,
    /// When an overlay is on, buildings fade toward grey so the ground reads.
    fade: bool,
}

impl Ctx<'_> {
    fn c(&self, color: [u8; 3]) -> [u8; 3] {
        if self.fade { mix(color, [200, 200, 205], 170) } else { color }
    }

    fn tile_at(&self, x: i32, y: i32) -> Option<&Tile> {
        tile_at(self.state, x, y)
    }
}

fn tile_at(state: &GameState, x: i32, y: i32) -> Option<&Tile> {
    (x >= 0 && y >= 0).then(|| state.map.get(Pos::new(x as u16, y as u16))).flatten()
}

/// Build the meshes for the tiles in `area`.
pub fn build_chunk(state: &GameState, look: Look, area: Rect) -> ChunkMeshes {
    let mut m = ChunkMeshes::default();
    let mut cx = Ctx { state, look, m: &mut m, fade: look.overlay != Overlay::None };
    for p in area.iter() {
        let idx = state.map.idx(p);
        ground(&mut cx, p, idx);
        let t = &state.map.tiles[idx];
        match t.kind {
            TileKind::Empty if t.terrain == Terrain::Forest => forest(&mut cx, p),
            TileKind::Road(r) => road(&mut cx, p, t, r),
            TileKind::Zone(z, d) => zone(&mut cx, p, t, z, d),
            TileKind::Building(b) if t.is_anchor() => building(&mut cx, p, b, t),
            _ => {}
        }
        if t.wire {
            wire(&mut cx, p);
        }
        if t.pipe && (look.show_pipes || look.overlay == Overlay::Water) {
            pipe(&mut cx, p);
        }
    }
    edge_skirt(&mut cx, area);
    m
}

fn origin(p: Pos) -> Vec3 {
    Vec3::new(p.x as f32 * TILE, 0.0, p.y as f32 * TILE)
}

pub fn tile_center(p: Pos) -> Vec3 {
    origin(p) + Vec3::new(TILE / 2.0, 0.0, TILE / 2.0)
}

// ---------------------------------------------------------------------------
// Ground

fn ground_color(cx: &Ctx, p: Pos, idx: usize, t: &Tile) -> [u8; 3] {
    let n = noise(p.x as u32, p.y as u32, 1) % 12;
    let grass = [GRASS[0] - 6 + n as u8, GRASS[1] - 6 + n as u8, GRASS[2] - 4 + (n / 2) as u8];
    let base = match t.kind {
        TileKind::Zone(z, _) => {
            if t.burning > 0 {
                [70, 66, 62]
            } else if t.level == 0 {
                mix(grass, ZONE_TINT[z.index()], 90)
            } else {
                mix(grass, [150, 150, 140], 60)
            }
        }
        TileKind::Building(b) => match b {
            Building::Park | Building::Playground | Building::SportsField => grass,
            Building::Plaza | Building::Monument | Building::TownHall => [196, 190, 176],
            Building::Airport | Building::Stadium => [120, 122, 118],
            _ => [140, 138, 130],
        },
        _ if t.terrain == Terrain::Forest => shade(grass, 82),
        _ => grass,
    };
    match overlay_tint(cx.state, cx.look.overlay, idx) {
        Some((c, a)) => mix(base, c, a),
        None => base,
    }
}

fn ground(cx: &mut Ctx, p: Pos, idx: usize) {
    let t = &cx.state.map.tiles[idx];
    let o = origin(p);
    if t.terrain == Terrain::Water {
        // River bed, banks against land neighbours, and the translucent surface.
        cx.m.solid.flat(o.x, o.z, o.x + TILE, o.z + TILE, BED_Y, [74, 84, 70]);
        let mut surface = [44, 104, 168];
        if let Some((c, a)) = overlay_tint(cx.state, cx.look.overlay, idx) {
            surface = mix(surface, c, a);
        }
        cx.m.water.flat(o.x, o.z, o.x + TILE, o.z + TILE, WATER_Y, surface);
        for (dx, dy) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
            let land = cx.tile_at(p.x as i32 + dx, p.y as i32 + dy).is_some_and(|n| n.terrain != Terrain::Water);
            if land {
                bank(cx, o, dx, dy);
            }
        }
        return;
    }
    let color = ground_color(cx, p, idx, t);
    cx.m.solid.flat(o.x, o.z, o.x + TILE, o.z + TILE, 0.0, color);
}

/// Vertical earth face on the water side of a tile edge.
fn bank(cx: &mut Ctx, o: Vec3, dx: i32, dy: i32) {
    let (x0, z0, x1, z1) = (o.x, o.z, o.x + TILE, o.z + TILE);
    let earth = [110, 90, 66];
    let (a, b, n) = match (dx, dy) {
        (0, -1) => (Vec3::new(x0, 0.0, z0), Vec3::new(x1, 0.0, z0), Vec3::Z),
        (0, 1) => (Vec3::new(x0, 0.0, z1), Vec3::new(x1, 0.0, z1), -Vec3::Z),
        (-1, 0) => (Vec3::new(x0, 0.0, z0), Vec3::new(x0, 0.0, z1), Vec3::X),
        _ => (Vec3::new(x1, 0.0, z0), Vec3::new(x1, 0.0, z1), -Vec3::X),
    };
    cx.m.solid.quad([a, b, b.with_y(BED_Y), a.with_y(BED_Y)], n, earth);
}

/// Earth slab under the map edge so the world looks like a diorama.
fn edge_skirt(cx: &mut Ctx, area: Rect) {
    let (w, h) = (cx.state.map.width, cx.state.map.height);
    let depth = -6.0;
    for p in area.iter() {
        let o = origin(p);
        let top = if cx.state.map.tile(p).terrain == Terrain::Water { BED_Y } else { 0.0 };
        let mut side = |a: Vec3, b: Vec3, n: Vec3| {
            cx.m.solid.quad([a.with_y(top), b.with_y(top), b.with_y(depth), a.with_y(depth)], n, [96, 74, 52]);
        };
        if p.y == 0 {
            side(o, o + Vec3::X * TILE, -Vec3::Z);
        }
        if p.y == h - 1 {
            side(o + Vec3::Z * TILE, o + Vec3::new(TILE, 0.0, TILE), Vec3::Z);
        }
        if p.x == 0 {
            side(o, o + Vec3::Z * TILE, -Vec3::X);
        }
        if p.x == w - 1 {
            side(o + Vec3::X * TILE, o + Vec3::new(TILE, 0.0, TILE), Vec3::X);
        }
    }
}

fn tree(m: &mut MeshBuf, at: Vec3, size: f32, variant: u32, fade: bool) {
    let leaf = [[52, 112, 50], [44, 98, 46], [70, 124, 52], [38, 90, 58]][(variant % 4) as usize];
    let leaf = if fade { mix(leaf, [200, 200, 205], 170) } else { leaf };
    m.frustum(at, 0.25 * size, 0.18 * size, 2.0 * size, 5, TRUNK, TRUNK);
    if variant % 3 == 0 {
        // Conifer.
        m.cone(at + Vec3::Y * 1.2 * size, 1.6 * size, 5.0 * size, 6, leaf);
    } else {
        // Broadleaf: a squat cone over an upturned one.
        let mid = at + Vec3::Y * 2.0 * size;
        m.cone(mid + Vec3::Y * 0.8 * size, 1.9 * size, 3.0 * size, 7, leaf);
        m.frustum(mid, 1.0 * size, 1.9 * size, 0.8 * size, 7, shade(leaf, 90), leaf);
    }
}

fn forest(cx: &mut Ctx, p: Pos) {
    let o = origin(p);
    let n = 3 + noise(p.x as u32, p.y as u32, 7) % 3;
    for i in 0..n {
        let h = noise(p.x as u32, p.y as u32, 100 + i);
        let at = o + Vec3::new(1.5 + (h % 70) as f32 / 10.0, 0.0, 1.5 + (h / 70 % 70) as f32 / 10.0);
        tree(&mut cx.m.solid, at, 0.8 + (h % 5) as f32 * 0.12, h, cx.fade);
    }
}

// ---------------------------------------------------------------------------
// Roads, wires, pipes

fn road(cx: &mut Ctx, p: Pos, t: &Tile, kind: Road) {
    let o = origin(p);
    let state = cx.state;
    let is_road = |dx: i32, dy: i32| tile_at(state, p.x as i32 + dx, p.y as i32 + dy).is_some_and(|n| n.kind.is_road());
    let (n, s, w, e) = (is_road(0, -1), is_road(0, 1), is_road(-1, 0), is_road(1, 0));
    let bridge = t.terrain == Terrain::Water;
    let y = if bridge { 0.4 } else { 0.03 };
    let asphalt = cx.c(if kind == Road::Avenue { [58, 58, 64] } else { ASPHALT });
    cx.m.solid.flat(o.x, o.z, o.x + TILE, o.z + TILE, y, asphalt);
    let (x0, z0, x1, z1) = (o.x, o.z, o.x + TILE, o.z + TILE);
    if bridge {
        // Deck sides and piers down into the water.
        cx.m.solid.cuboid(Vec3::new(x0 + 3.5, BED_Y, z0 + 3.5), Vec3::new(x1 - 3.5, y - 0.4, z1 - 3.5), CONCRETE, CONCRETE);
        if !n || !s {
            for z in [z0, z1 - 0.3] {
                if (z == z0 && !n) || (z != z0 && !s) {
                    cx.m.solid.block(Vec3::new(x0, y, z), Vec3::new(x1, y + 1.0, z + 0.3), [200, 200, 196]);
                }
            }
        }
        if !w || !e {
            for x in [x0, x1 - 0.3] {
                if (x == x0 && !w) || (x != x0 && !e) {
                    cx.m.solid.block(Vec3::new(x, y, z0), Vec3::new(x + 0.3, y + 1.0, z1), [200, 200, 196]);
                }
            }
        }
    } else {
        // Raised sidewalks on every side that doesn't continue the road.
        let walk = cx.c(SIDEWALK_C);
        let sw = SIDEWALK;
        if !n {
            cx.m.solid.cuboid(Vec3::new(x0, 0.0, z0), Vec3::new(x1, CURB, z0 + sw), walk, walk);
        }
        if !s {
            cx.m.solid.cuboid(Vec3::new(x0, 0.0, z1 - sw), Vec3::new(x1, CURB, z1), walk, walk);
        }
        if !w {
            cx.m.solid.cuboid(Vec3::new(x0, 0.0, z0), Vec3::new(x0 + sw, CURB, z1), walk, walk);
        }
        if !e {
            cx.m.solid.cuboid(Vec3::new(x1 - sw, 0.0, z0), Vec3::new(x1, CURB, z1), walk, walk);
        }
        // Corner pieces where two roads meet but the diagonal doesn't continue.
        for (dx, dz, a, b) in [(-1, -1, w, n), (1, -1, e, n), (-1, 1, w, s), (1, 1, e, s)] {
            if a && b && !is_road(dx, dz) {
                let cx0 = if dx < 0 { x0 } else { x1 - sw };
                let cz0 = if dz < 0 { z0 } else { z1 - sw };
                cx.m.solid.cuboid(Vec3::new(cx0, 0.0, cz0), Vec3::new(cx0 + sw, CURB, cz0 + sw), walk, walk);
            }
        }
        // A street lamp on a corner of each straight piece.
        let straight = (n || s) != (w || e);
        if straight && (p.x + p.y) % 2 == 0 {
            let at = if n || s { Vec3::new(x0 + 0.6, CURB, z0 + TILE / 2.0) } else { Vec3::new(x0 + TILE / 2.0, CURB, z0 + 0.6) };
            if (n || s) && !w || (w || e) && !n {
                lamp(cx, at, if n || s { Vec3::X } else { Vec3::Z });
            }
        }
    }
    // Lane markings.
    let paint = cx.c([225, 205, 110]);
    let white = cx.c([230, 230, 225]);
    let ym = y + 0.01;
    let (cxm, czm) = (x0 + TILE / 2.0, z0 + TILE / 2.0);
    let vertical = (n || s) && !(w || e);
    let horizontal = (w || e) && !(n || s);
    if kind == Road::Avenue {
        // Planted median.
        let med = cx.c([96, 140, 80]);
        if vertical {
            cx.m.solid.cuboid(Vec3::new(cxm - 0.6, y, z0), Vec3::new(cxm + 0.6, y + 0.18, z1), CONCRETE, med);
        } else if horizontal {
            cx.m.solid.cuboid(Vec3::new(x0, y, czm - 0.6), Vec3::new(x1, y + 0.18, czm + 0.6), CONCRETE, med);
        }
        for off in [-2.4f32, 2.4] {
            if vertical {
                for k in 0..2 {
                    let z = z0 + 1.0 + k as f32 * 5.0;
                    cx.m.solid.flat(cxm + off - 0.08, z, cxm + off + 0.08, z + 2.5, ym, white);
                }
            } else if horizontal {
                for k in 0..2 {
                    let x = x0 + 1.0 + k as f32 * 5.0;
                    cx.m.solid.flat(x, czm + off - 0.08, x + 2.5, czm + off + 0.08, ym, white);
                }
            }
        }
    } else if vertical {
        for k in 0..2 {
            let z = z0 + 1.0 + k as f32 * 5.0;
            cx.m.solid.flat(cxm - 0.1, z, cxm + 0.1, z + 2.6, ym, paint);
        }
    } else if horizontal {
        for k in 0..2 {
            let x = x0 + 1.0 + k as f32 * 5.0;
            cx.m.solid.flat(x, czm - 0.1, x + 2.6, czm + 0.1, ym, paint);
        }
    } else if [n, s, w, e].iter().filter(|&&c| c).count() >= 3 {
        // Zebra crossings on each arm of a junction.
        for (on, dir) in [(n, 0), (s, 1), (w, 2), (e, 3)] {
            if !on {
                continue;
            }
            for k in 0..5 {
                let off = 2.2 + k as f32 * 1.3;
                match dir {
                    0 => cx.m.solid.flat(x0 + off, z0 + 0.4, x0 + off + 0.7, z0 + 2.0, ym, white),
                    1 => cx.m.solid.flat(x0 + off, z1 - 2.0, x0 + off + 0.7, z1 - 0.4, ym, white),
                    2 => cx.m.solid.flat(x0 + 0.4, z0 + off, x0 + 2.0, z0 + off + 0.7, ym, white),
                    _ => cx.m.solid.flat(x1 - 2.0, z0 + off, x1 - 0.4, z0 + off + 0.7, ym, white),
                }
            }
        }
    }
}

fn lamp(cx: &mut Ctx, at: Vec3, toward: Vec3) {
    let pole = [70, 72, 78];
    cx.m.solid.block(at - Vec3::new(0.1, 0.0, 0.1), at + Vec3::new(0.1, 6.0, 0.1), pole);
    let arm = at + Vec3::Y * 6.0 + toward * 1.4;
    cx.m.solid.bar(at + Vec3::Y * 5.9, arm, 0.12, pole);
    cx.m.lights.block(arm - Vec3::new(0.3, 0.25, 0.3), arm + Vec3::new(0.3, -0.05, 0.3), LIT);
}

fn wire(cx: &mut Ctx, p: Pos) {
    let o = origin(p);
    let pole_at = o + Vec3::new(1.0, 0.0, 1.0);
    let top = pole_at + Vec3::Y * 9.0;
    let wood = cx.c([96, 70, 44]);
    cx.m.solid.block(pole_at - Vec3::new(0.15, 0.0, 0.15), top + Vec3::new(0.15, 0.0, 0.15), wood);
    cx.m.solid.block(top + Vec3::new(-1.0, -0.6, -0.08), top + Vec3::new(1.0, -0.4, 0.08), wood);
    let cable = [40, 40, 40];
    for (dx, dy) in [(1, 0), (0, 1)] {
        let joins = cx.tile_at(p.x as i32 + dx, p.y as i32 + dy).is_some_and(|n| n.wire);
        if joins {
            let next = top + Vec3::new(dx as f32 * TILE, 0.0, dy as f32 * TILE);
            for side in [-0.8f32, 0.8] {
                let off = if dx != 0 { Vec3::Z * side * 0.1 } else { Vec3::X * side };
                cx.m.solid.bar(top - Vec3::Y * 0.5 + off, next - Vec3::Y * 0.5 + off, 0.05, cable);
            }
        }
    }
}

fn pipe(cx: &mut Ctx, p: Pos) {
    let c = tile_center(p) + Vec3::Y * 0.3;
    let blue = [80, 170, 250];
    cx.m.lights.block(c - Vec3::splat(0.5), c + Vec3::splat(0.5), blue);
    for (dx, dy) in [(1, 0), (0, 1)] {
        if cx.tile_at(p.x as i32 + dx, p.y as i32 + dy).is_some_and(|n| n.conducts_water()) {
            let next = c + Vec3::new(dx as f32 * TILE, 0.0, dy as f32 * TILE);
            cx.m.lights.bar(c, next, 0.5, blue);
        }
    }
}

// ---------------------------------------------------------------------------
// Zones: the buildings that grow by themselves

/// Which way the lot faces: toward an adjacent road if there is one.
fn facing(cx: &Ctx, p: Pos) -> Vec3 {
    for (dx, dy) in [(0, 1), (1, 0), (0, -1), (-1, 0)] {
        if cx.tile_at(p.x as i32 + dx, p.y as i32 + dy).is_some_and(|n| n.kind.is_road()) {
            return Vec3::new(dx as f32, 0.0, dy as f32);
        }
    }
    Vec3::Z
}

/// Glass bands, one per floor, on all four faces of a box.
fn floors(m: &mut ChunkMeshes, min: Vec3, max: Vec3, floor_h: f32, seed: u32, from: f32) {
    let inset = 0.6;
    let mut y = min.y + from;
    let mut k = 0;
    while y + floor_h * 0.7 <= max.y {
        let (y0, y1) = (y + floor_h * 0.25, y + floor_h * 0.75);
        let d = 0.03;
        // Split each face into panes ~1.6 m wide; a random share are lit at night.
        for (a0, a1, face) in [(min.x + inset, max.x - inset, 0u32), (min.z + inset, max.z - inset, 1)] {
            let n = ((a1 - a0) / 2.4).round().max(1.0) as u32;
            let step = (a1 - a0) / n as f32;
            for i in 0..n {
                let (u0, u1) = (a0 + i as f32 * step + 0.4, a0 + (i + 1) as f32 * step - 0.4);
                for side in 0..2u32 {
                    let lit = noise(seed ^ face ^ (side << 4), k * 31 + i, 3) % 5 < 2;
                    let buf = if lit { &mut m.lights } else { &mut m.glass };
                    let col = if lit { WINDOW } else { GLASS };
                    if face == 0 {
                        let (z, n) = if side == 0 { (max.z + d, Vec3::Z) } else { (min.z - d, -Vec3::Z) };
                        buf.quad([Vec3::new(u0, y0, z), Vec3::new(u1, y0, z), Vec3::new(u1, y1, z), Vec3::new(u0, y1, z)], n, col);
                    } else {
                        let (x, n) = if side == 0 { (max.x + d, Vec3::X) } else { (min.x - d, -Vec3::X) };
                        buf.quad([Vec3::new(x, y0, u0), Vec3::new(x, y0, u1), Vec3::new(x, y1, u1), Vec3::new(x, y1, u0)], n, col);
                    }
                }
            }
        }
        y += floor_h;
        k += 1;
    }
}

/// Mirror a lot layout so buildings sit at the back and face the road.
fn lot(o: Vec3, face: Vec3, u0: f32, v0: f32, u1: f32, v1: f32) -> (Vec3, Vec3) {
    // (u, v) in metres within the tile, v measured from the road side inward.
    let (a, b) = match (face.x as i32, face.z as i32) {
        (0, 1) => ((u0, TILE - v1), (u1, TILE - v0)),
        (0, -1) => ((u0, v0), (u1, v1)),
        (1, 0) => ((TILE - v1, u0), (TILE - v0, u1)),
        _ => ((v0, u0), (v1, u1)),
    };
    (o + Vec3::new(a.0, 0.0, a.1), o + Vec3::new(b.0, 0.0, b.1))
}

fn zone(cx: &mut Ctx, p: Pos, t: &Tile, z: Zone, d: Density) {
    let o = origin(p);
    if t.burning > 0 {
        rubble(cx, p);
        return;
    }
    if t.level == 0 {
        // A "for sale" sign in the zone colour.
        let c = tile_center(p) + facing(cx, p) * 3.5;
        cx.m.solid.block(c + Vec3::new(-0.05, 0.0, -0.05), c + Vec3::new(0.05, 1.4, 0.05), [120, 100, 80]);
        cx.m.solid.block(c + Vec3::new(-0.5, 1.0, -0.5), c + Vec3::new(0.5, 1.6, 0.5), ZONE_TINT[z.index()]);
        return;
    }
    let face = facing(cx, p);
    let v = noise(p.x as u32, p.y as u32, 11);
    let seed = noise(p.x as u32, p.y as u32, 12);
    let lv = t.level;
    match z {
        Zone::Residential => residential(cx, o, face, lv, d, v, seed),
        Zone::Commercial => commercial(cx, o, face, lv, d, v, seed),
        Zone::Industrial => industrial(cx, o, face, lv, d, v),
        Zone::Office => office(cx, o, face, lv, v, seed),
    }
}

fn rubble(cx: &mut Ctx, p: Pos) {
    let o = origin(p);
    for i in 0..6 {
        let h = noise(p.x as u32, p.y as u32, 200 + i);
        let at = o + Vec3::new(2.0 + (h % 60) as f32 / 10.0, 0.0, 2.0 + (h / 60 % 60) as f32 / 10.0);
        let s = 0.4 + (h % 7) as f32 * 0.15;
        cx.m.solid.block(at, at + Vec3::new(s * 1.6, s, s), [56 + (h % 30) as u8, 50, 46]);
    }
}

fn residential(cx: &mut Ctx, o: Vec3, face: Vec3, lv: u8, d: Density, v: u32, seed: u32) {
    let wall = cx.c(WALLS[(v % 8) as usize]);
    let roof = cx.c(ROOFS[(v / 8 % 5) as usize]);
    let along_x = face.z != 0.0;
    match (d, lv) {
        (_, 1) => {
            // Cottage with a gable roof and a garden tree.
            let (a, b) = lot(o, face, 2.0, 2.5, 8.0, 8.0);
            cx.m.solid.block(a, b.with_y(3.2), wall);
            cx.m.solid.gable(a.with_y(3.2) - Vec3::new(0.3, 0.0, 0.3), b.with_y(5.6) + Vec3::new(0.3, 0.0, 0.3), along_x, roof, wall);
            floors(cx.m, a, b.with_y(3.2), 3.2, seed, 0.0);
            let (t, _) = lot(o, face, 8.6, 1.0, 9.0, 1.4);
            tree(&mut cx.m.solid, t, 0.6, v, cx.fade);
        }
        (Density::Low, 2) => {
            let (a, b) = lot(o, face, 1.5, 2.0, 8.5, 8.6);
            cx.m.solid.block(a, b.with_y(6.0), wall);
            cx.m.solid.gable(a.with_y(6.0) - Vec3::new(0.3, 0.0, 0.3), b.with_y(8.6) + Vec3::new(0.3, 0.0, 0.3), along_x, roof, wall);
            floors(cx.m, a, b.with_y(6.0), 3.0, seed, 0.0);
        }
        (Density::Low, _) => {
            // Terrace of townhouses.
            let (a, b) = lot(o, face, 0.6, 1.6, 9.4, 9.0);
            cx.m.solid.block(a, b.with_y(9.0), wall);
            cx.m.solid.block(a.with_y(9.0), b.with_y(9.6), shade(wall, 85));
            floors(cx.m, a, b.with_y(9.0), 3.0, seed, 0.0);
        }
        (Density::High, 2 | 3) => {
            // Brick walk-up with a flat roof.
            let brick = cx.c([[170, 96, 72], [150, 110, 90], [190, 150, 120], [130, 90, 80]][(v % 4) as usize]);
            let h = lv as f32 * 4.0 + 4.0;
            let (a, b) = lot(o, face, 0.8, 1.2, 9.2, 9.2);
            cx.m.solid.block(a, b.with_y(h), brick);
            cx.m.solid.block(a.with_y(h), b.with_y(h + 0.5), shade(brick, 80));
            floors(cx.m, a, b.with_y(h), 3.0, seed, 0.2);
        }
        (Density::High, _) => {
            // Apartment tower: taller with each level, with a setback and roof box.
            let floors_n = [0, 0, 0, 0, 8, 13, 19][lv.min(6) as usize] as f32 + (v % 3) as f32;
            let h = floors_n * 3.1;
            let body = cx.c([[220, 214, 200], [200, 206, 214], [226, 210, 190], [190, 196, 190]][(v % 4) as usize]);
            let (a, b) = lot(o, face, 1.2, 1.2, 8.8, 8.8);
            cx.m.solid.block(a, b.with_y(h), body);
            floors(cx.m, a, b.with_y(h), 3.1, seed, 0.3);
            let inset = Vec3::new(1.6, 0.0, 1.6);
            cx.m.solid.block((a + inset).with_y(h), (b - inset).with_y(h + 3.0), shade(body, 85));
            cx.m.solid.block((a + inset * 2.0).with_y(h + 3.0), (b - inset * 2.0).with_y(h + 4.2), CONCRETE);
        }
    }
}

fn commercial(cx: &mut Ctx, o: Vec3, face: Vec3, lv: u8, d: Density, v: u32, seed: u32) {
    let accent = cx.c([[200, 60, 60], [60, 120, 200], [230, 170, 40], [60, 160, 110], [170, 80, 170]][(v % 5) as usize]);
    let wall = cx.c(WALLS[(v % 8) as usize]);
    if d == Density::Low || lv <= 3 {
        let h = 4.2 + (lv as f32 - 1.0) * 3.4;
        let (a, b) = lot(o, face, 0.6, 1.0, 9.4, 8.6);
        cx.m.solid.block(a, b.with_y(h), wall);
        // Shop window band and an awning facing the street.
        let (sa, sb) = lot(o, face, 1.2, 0.9, 8.8, 0.95);
        cx.m.lights.block(sa.with_y(0.3), sb.with_y(2.8), LIT);
        let (aa, ab) = lot(o, face, 0.8, 0.0, 9.2, 1.0);
        cx.m.solid.block(aa.with_y(2.9), ab.with_y(3.2), accent);
        if lv >= 2 {
            floors(cx.m, a.with_y(3.6), b.with_y(h), 3.2, seed, 0.0);
        }
        // Rooftop sign.
        let (ra, rb) = lot(o, face, 3.0, 2.0, 7.0, 2.4);
        cx.m.lights.block(ra.with_y(h), rb.with_y(h + 1.4), shade(accent, 120));
        return;
    }
    // Glass tower.
    let h = [0.0, 0.0, 0.0, 0.0, 30.0, 44.0, 62.0][lv.min(6) as usize] + (v % 4) as f32 * 3.0;
    let frame = cx.c([[80, 96, 120], [60, 70, 84], [120, 130, 140]][(v % 3) as usize]);
    let (a, b) = lot(o, face, 0.8, 0.8, 9.2, 9.2);
    cx.m.solid.block(a, b.with_y(h), frame);
    floors(cx.m, a, b.with_y(h), 3.6, seed, 0.0);
    let (pa, pb) = lot(o, face, 0.0, 0.0, 10.0, 10.0);
    cx.m.solid.block(pa.with_y(0.0) + Vec3::new(0.2, 0.0, 0.2), pb.with_y(4.5) - Vec3::new(0.2, 0.0, 0.2), shade(frame, 110));
    cx.m.lights.block(a.with_y(h) + Vec3::new(3.0, 0.0, 3.0), b.with_y(h + 2.0) - Vec3::new(3.0, 0.0, 3.0), accent);
}

fn industrial(cx: &mut Ctx, o: Vec3, face: Vec3, lv: u8, d: Density, v: u32) {
    let body = cx.c([[150, 132, 100], [128, 128, 120], [160, 120, 90], [110, 120, 130]][(v % 4) as usize]);
    let roof = cx.c([120, 124, 130]);
    let big = d == Density::High && lv >= 3;
    let h = if big { 9.0 + lv as f32 } else { 4.5 + lv as f32 * 1.2 };
    let (a, b) = lot(o, face, 0.6, 2.4, 9.4, 9.4);
    cx.m.solid.cuboid(a, b.with_y(h), body, roof);
    // Sawtooth skylights.
    let along_x = face.z != 0.0;
    for k in 0..3 {
        let f = 0.15 + k as f32 * 0.28;
        let (sa, sb) = if along_x {
            (Vec3::new(a.x + 0.4, h, a.z + (b.z - a.z) * f), Vec3::new(b.x - 0.4, h + 1.4, a.z + (b.z - a.z) * (f + 0.2)))
        } else {
            (Vec3::new(a.x + (b.x - a.x) * f, h, a.z + 0.4), Vec3::new(a.x + (b.x - a.x) * (f + 0.2), h + 1.4, b.z - 0.4))
        };
        cx.m.solid.gable(sa, sb, !along_x, roof, body);
    }
    // Yard with crates, and chimneys/silos as it grows.
    let (ya, _) = lot(o, face, 1.0, 0.4, 2.2, 1.6);
    cx.m.solid.block(ya, ya + Vec3::new(1.2, 1.0, 1.2), [150, 110, 70]);
    let stacks = if big { lv.min(4) as usize } else { (lv as usize).saturating_sub(1) };
    for s in 0..stacks {
        let (sa, _) = lot(o, face, 2.0 + s as f32 * 2.2, 5.0, 2.5, 5.5);
        let tall = h + 6.0 + s as f32 * 2.0 + if big { 6.0 } else { 0.0 };
        cx.m.solid.frustum(sa, 0.7, 0.5, tall, 8, cx.c([150, 140, 132]), [60, 56, 54]);
        cx.m.solid.cylinder(sa + Vec3::Y * (tall - 1.0), 0.56, 0.6, 8, cx.c([200, 60, 50]), [60, 56, 54]);
    }
    if big {
        let (ta, _) = lot(o, face, 7.5, 0.3, 8.0, 0.8);
        cx.m.solid.cylinder(ta + Vec3::new(0.0, 0.0, 0.0), 1.4, 5.0, 10, cx.c([200, 200, 196]), [170, 170, 166]);
    }
}

fn office(cx: &mut Ctx, o: Vec3, face: Vec3, lv: u8, v: u32, seed: u32) {
    let frame = cx.c([[70, 84, 110], [96, 100, 108], [140, 150, 160], [52, 60, 72]][(v % 4) as usize]);
    let h = match lv {
        1 => 8.0,
        2 => 13.0,
        3 => 20.0,
        4 => 36.0,
        5 => 54.0,
        _ => 78.0,
    } + (v % 4) as f32 * 2.5;
    let (a, b) = lot(o, face, 1.0, 1.0, 9.0, 9.0);
    if lv >= 4 {
        // Tiered skyscraper with a crown.
        let tier = |k: f32| Vec3::new(k, 0.0, k);
        let h1 = h * 0.62;
        cx.m.solid.block(a, b.with_y(h1), frame);
        floors(cx.m, a, b.with_y(h1), 3.4, seed, 0.0);
        cx.m.solid.block((a + tier(1.0)).with_y(h1), (b - tier(1.0)).with_y(h), frame);
        floors(cx.m, (a + tier(1.0)).with_y(h1), (b - tier(1.0)).with_y(h), 3.4, seed ^ 7, 0.0);
        if v % 2 == 0 {
            cx.m.solid.pyramid((a + tier(1.0)).with_y(h), (b - tier(1.0)).with_y(h + 6.0), shade(frame, 120));
        } else {
            cx.m.solid.block((a + tier(2.4)).with_y(h), (b - tier(2.4)).with_y(h + 3.0), shade(frame, 120));
            let mast = (a + b) / 2.0;
            cx.m.solid.block(mast.with_y(h + 3.0) - Vec3::new(0.15, 0.0, 0.15), mast.with_y(h + 12.0) + Vec3::new(0.15, 0.0, 0.15), [200, 200, 200]);
            cx.m.lights.block(mast.with_y(h + 12.0) - Vec3::splat(0.3), mast.with_y(h + 12.4) + Vec3::splat(0.3), [255, 60, 50]);
        }
    } else {
        cx.m.solid.block(a, b.with_y(h), frame);
        floors(cx.m, a, b.with_y(h), 3.4, seed, 0.0);
        cx.m.solid.block(a.with_y(h), b.with_y(h + 0.6), shade(frame, 80));
    }
}

// ---------------------------------------------------------------------------
// Placed buildings

fn building(cx: &mut Ctx, p: Pos, b: Building, t: &Tile) {
    let size = b.size() as f32 * TILE;
    let o = origin(p);
    let c = o + Vec3::new(size / 2.0, 0.0, size / 2.0);
    let v = noise(p.x as u32, p.y as u32, 31);
    let seed = noise(p.x as u32, p.y as u32, 32);
    let pad = |cx: &mut Ctx, inset: f32, col: [u8; 3]| {
        cx.m.solid.flat(o.x + inset, o.z + inset, o.x + size - inset, o.z + size - inset, 0.04, col);
    };
    let k = |c: [u8; 3], cx: &Ctx| cx.c(c);
    match b {
        Building::CoalPlant => {
            pad(cx, 0.5, [110, 108, 104]);
            let hall = k([140, 70, 56], cx);
            cx.m.solid.cuboid(o + Vec3::new(2.0, 0.0, 10.5), o + Vec3::new(18.0, 12.0, 18.0), hall, [90, 90, 96]);
            floors(cx.m, o + Vec3::new(2.0, 0.0, 10.5), o + Vec3::new(18.0, 12.0, 18.0), 4.0, seed, 1.0);
            for (i, x) in [5.0f32, 13.0].iter().enumerate() {
                cx.m.solid.frustum(o + Vec3::new(*x, 0.0, 5.0), 3.6, 2.4, 16.0 + i as f32 * 2.0, 12, k([196, 194, 188], cx), [70, 70, 70]);
            }
            cx.m.solid.frustum(o + Vec3::new(17.0, 0.0, 3.0), 0.9, 0.7, 28.0, 8, k([170, 160, 150], cx), [40, 40, 40]);
            cx.m.solid.block(o + Vec3::new(1.0, 0.0, 1.0), o + Vec3::new(3.0, 3.0, 3.5), [40, 40, 44]);
        }
        Building::GasPlant => {
            pad(cx, 0.5, [120, 122, 120]);
            cx.m.solid.cuboid(o + Vec3::new(1.5, 0.0, 9.0), o + Vec3::new(18.5, 9.0, 18.0), k([200, 204, 210], cx), [120, 126, 136]);
            floors(cx.m, o + Vec3::new(1.5, 0.0, 9.0), o + Vec3::new(18.5, 9.0, 18.0), 4.0, seed, 1.0);
            for x in [4.0f32, 9.0, 14.0] {
                cx.m.solid.cylinder(o + Vec3::new(x, 0.0, 4.5), 2.0, 6.0, 12, k([230, 230, 226], cx), [190, 190, 186]);
            }
            for x in [6.0f32, 12.0] {
                cx.m.solid.cylinder(o + Vec3::new(x, 9.0, 13.5), 0.6, 9.0, 8, k([180, 180, 176], cx), [60, 60, 60]);
            }
        }
        Building::WindTurbine => {
            // Tower only; the rotor is an animated prop.
            cx.m.solid.frustum(c, 1.0, 0.45, 30.0, 10, k([236, 236, 232], cx), [236, 236, 232]);
            cx.m.solid.block(c + Vec3::new(-0.8, 29.4, -1.6), c + Vec3::new(0.8, 31.2, 1.0), k([236, 236, 232], cx));
            cx.m.solid.block(c - Vec3::new(2.0, 0.0, 2.0), c + Vec3::new(2.0, 0.3, 2.0), CONCRETE);
        }
        Building::SolarFarm => {
            pad(cx, 0.3, [140, 150, 110]);
            for row in 0..5 {
                for col in 0..2 {
                    let a = o + Vec3::new(1.2 + col as f32 * 9.0, 0.0, 1.2 + row as f32 * 3.7);
                    cx.m.solid.block(a + Vec3::new(3.6, 0.0, 1.2), a + Vec3::new(4.0, 1.2, 1.6), [120, 120, 120]);
                    cx.m.glass.quad(
                        [a + Vec3::new(0.0, 0.8, 0.0), a + Vec3::new(8.0, 0.8, 0.0), a + Vec3::new(8.0, 1.9, 2.6), a + Vec3::new(0.0, 1.9, 2.6)],
                        Vec3::new(0.0, 0.92, -0.4).normalize(),
                        [40, 60, 110],
                    );
                }
            }
        }
        Building::NuclearPlant => {
            pad(cx, 0.5, [150, 150, 146]);
            for (x, z) in [(7.0f32, 8.0f32), (20.0, 8.0)] {
                cx.m.solid.frustum(o + Vec3::new(x, 0.0, z), 6.0, 3.8, 14.0, 16, k([210, 208, 202], cx), [80, 80, 80]);
                cx.m.solid.frustum(o + Vec3::new(x, 14.0, z), 3.8, 4.6, 10.0, 16, k([210, 208, 202], cx), [80, 80, 80]);
            }
            cx.m.solid.cylinder(o + Vec3::new(9.0, 0.0, 22.0), 4.5, 12.0, 16, k([230, 228, 220], cx), [230, 228, 220]);
            cx.m.solid.frustum(o + Vec3::new(9.0, 12.0, 22.0), 4.5, 0.5, 4.5, 16, k([230, 228, 220], cx), [230, 228, 220]);
            cx.m.solid.cuboid(o + Vec3::new(16.0, 0.0, 17.0), o + Vec3::new(28.0, 10.0, 28.0), k([180, 186, 196], cx), [110, 116, 124]);
            floors(cx.m, o + Vec3::new(16.0, 0.0, 17.0), o + Vec3::new(28.0, 10.0, 28.0), 3.5, seed, 0.5);
        }
        Building::WaterPump => {
            pad(cx, 0.5, CONCRETE);
            cx.m.solid.cuboid(o + Vec3::new(2.0, 0.0, 2.0), o + Vec3::new(7.0, 4.0, 6.0), k([150, 170, 190], cx), [80, 100, 130]);
            cx.m.solid.gable(o + Vec3::new(1.7, 4.0, 1.7), o + Vec3::new(7.3, 5.6, 6.3), true, k([70, 100, 140], cx), k([150, 170, 190], cx));
            cx.m.water.flat(o.x + 1.0, o.z + 7.0, o.x + 9.0, o.z + 9.2, 0.12, [60, 140, 210]);
        }
        Building::WaterTower => {
            let legs = k([150, 150, 150], cx);
            for (dx, dz) in [(-2.0f32, -2.0f32), (2.0, -2.0), (-2.0, 2.0), (2.0, 2.0)] {
                cx.m.solid.bar(c + Vec3::new(dx * 1.2, 0.0, dz * 1.2), c + Vec3::new(dx * 0.8, 14.0, dz * 0.8), 0.35, legs);
            }
            cx.m.solid.cylinder(c + Vec3::Y * 13.0, 3.6, 5.0, 14, k([120, 170, 210], cx), [120, 170, 210]);
            cx.m.solid.cone(c + Vec3::Y * 18.0, 3.8, 2.4, 14, k([90, 120, 160], cx));
        }
        Building::WaterTreatment => {
            pad(cx, 0.5, [140, 140, 136]);
            for (x, z) in [(5.5f32, 5.5f32), (14.5, 5.5), (5.5, 14.5)] {
                cx.m.solid.cylinder(o + Vec3::new(x, 0.0, z), 4.0, 1.4, 16, CONCRETE, CONCRETE);
                cx.m.water.flat(o.x + x - 2.8, o.z + z - 2.8, o.x + x + 2.8, o.z + z + 2.8, 1.45, [70, 150, 190]);
            }
            cx.m.solid.cuboid(o + Vec3::new(11.0, 0.0, 11.0), o + Vec3::new(19.0, 6.0, 19.0), k([200, 210, 220], cx), [100, 120, 140]);
            floors(cx.m, o + Vec3::new(11.0, 0.0, 11.0), o + Vec3::new(19.0, 6.0, 19.0), 3.0, seed, 0.0);
        }
        Building::PoliceStation | Building::FireStation | Building::Clinic => {
            let (wall, band) = match b {
                Building::PoliceStation => ([200, 206, 220], [40, 70, 160]),
                Building::FireStation => ([190, 90, 76], [230, 230, 220]),
                _ => ([236, 236, 236], [210, 50, 50]),
            };
            pad(cx, 0.3, CONCRETE);
            let face = facing(cx, p);
            let (a, bb) = lot(o, face, 1.0, 1.5, 9.0, 9.0);
            cx.m.solid.block(a, bb.with_y(7.0), k(wall, cx));
            floors(cx.m, a, bb.with_y(7.0), 3.4, seed, 0.2);
            cx.m.solid.block(a.with_y(7.0) - Vec3::new(0.1, 0.0, 0.1), bb.with_y(7.8) + Vec3::new(0.1, 0.0, 0.1), k(band, cx));
            if b == Building::FireStation {
                let (da, db) = lot(o, face, 2.0, 1.4, 8.0, 1.52);
                cx.m.solid.block(da, db.with_y(4.0), [220, 220, 210]);
            }
            if b == Building::Clinic {
                let top = (a + bb) / 2.0 + Vec3::Y * 7.8;
                cx.m.lights.block(top - Vec3::new(1.2, 0.0, 0.3), top + Vec3::new(1.2, 0.4, 0.3), [220, 40, 40]);
                cx.m.lights.block(top - Vec3::new(0.3, 0.0, 1.2), top + Vec3::new(0.3, 0.4, 1.2), [220, 40, 40]);
            } else {
                let mast = a.with_y(7.8) + Vec3::new(0.8, 0.0, 0.8);
                cx.m.solid.block(mast, mast + Vec3::new(0.2, 4.0, 0.2), [180, 180, 180]);
                cx.m.solid.block(mast + Vec3::new(0.2, 2.6, 0.0), mast + Vec3::new(1.8, 3.8, 0.1), k(band, cx));
            }
        }
        Building::Hospital => {
            pad(cx, 0.4, CONCRETE);
            let white = k([238, 238, 236], cx);
            cx.m.solid.block(o + Vec3::new(2.0, 0.0, 2.0), o + Vec3::new(18.0, 8.0, 18.0), white);
            floors(cx.m, o + Vec3::new(2.0, 0.0, 2.0), o + Vec3::new(18.0, 8.0, 18.0), 3.6, seed, 0.2);
            cx.m.solid.block(o + Vec3::new(6.0, 8.0, 6.0), o + Vec3::new(14.0, 24.0, 14.0), white);
            floors(cx.m, o + Vec3::new(6.0, 8.0, 6.0), o + Vec3::new(14.0, 24.0, 14.0), 3.6, seed ^ 3, 0.0);
            let top = o + Vec3::new(10.0, 24.0, 10.0);
            cx.m.lights.block(top - Vec3::new(2.4, 0.0, 0.6), top + Vec3::new(2.4, 0.5, 0.6), [220, 40, 40]);
            cx.m.lights.block(top - Vec3::new(0.6, 0.0, 2.4), top + Vec3::new(0.6, 0.5, 2.4), [220, 40, 40]);
        }
        Building::School => {
            pad(cx, 0.3, [176, 160, 130]);
            let brick = k([186, 104, 80], cx);
            cx.m.solid.block(o + Vec3::new(1.0, 0.0, 1.0), o + Vec3::new(19.0, 7.0, 7.5), brick);
            cx.m.solid.block(o + Vec3::new(1.0, 0.0, 7.5), o + Vec3::new(7.5, 7.0, 19.0), brick);
            floors(cx.m, o + Vec3::new(1.0, 0.0, 1.0), o + Vec3::new(19.0, 7.0, 7.5), 3.4, seed, 0.2);
            floors(cx.m, o + Vec3::new(1.0, 0.0, 7.5), o + Vec3::new(7.5, 7.0, 19.0), 3.4, seed ^ 9, 0.2);
            cx.m.solid.gable(o + Vec3::new(0.7, 7.0, 0.7), o + Vec3::new(19.3, 9.2, 7.8), true, k([96, 96, 104], cx), brick);
            // Playing field with a track.
            cx.m.solid.flat(o.x + 9.0, o.z + 9.0, o.x + 19.0, o.z + 19.0, 0.05, [170, 90, 70]);
            cx.m.solid.flat(o.x + 10.0, o.z + 10.0, o.x + 18.0, o.z + 18.0, 0.07, [90, 150, 80]);
            let pole = o + Vec3::new(8.5, 0.0, 8.5);
            cx.m.solid.block(pole, pole + Vec3::new(0.12, 8.0, 0.12), [200, 200, 200]);
            cx.m.solid.block(pole + Vec3::new(0.12, 6.6, 0.0), pole + Vec3::new(2.0, 7.8, 0.05), [60, 90, 200]);
        }
        Building::University => {
            pad(cx, 0.3, [196, 188, 170]);
            let stone = k([214, 200, 172], cx);
            cx.m.solid.flat(o.x + 8.0, o.z + 8.0, o.x + 22.0, o.z + 22.0, 0.07, [96, 150, 84]);
            for (a, b) in [
                (Vec3::new(1.0, 0.0, 1.0), Vec3::new(29.0, 10.0, 7.0)),
                (Vec3::new(1.0, 0.0, 23.0), Vec3::new(29.0, 10.0, 29.0)),
                (Vec3::new(1.0, 0.0, 7.0), Vec3::new(7.0, 10.0, 23.0)),
            ] {
                cx.m.solid.block(o + a, o + b, stone);
                floors(cx.m, o + a, o + b, 3.6, seed ^ (a.z as u32), 0.4);
                cx.m.solid.gable(o + a.with_y(10.0) - Vec3::splat(0.3).with_y(0.0), o + b.with_y(13.0) + Vec3::splat(0.3).with_y(0.0), (b.x - a.x) > (b.z - a.z), k([90, 110, 100], cx), stone);
            }
            let tower = o + Vec3::new(24.0, 0.0, 15.0);
            cx.m.solid.block(tower - Vec3::new(2.5, 0.0, 2.5), tower + Vec3::new(2.5, 22.0, 2.5), stone);
            cx.m.solid.pyramid(tower.with_y(22.0) - Vec3::new(2.8, 0.0, 2.8), tower.with_y(28.0) + Vec3::new(2.8, 0.0, 2.8), k([90, 110, 100], cx));
            cx.m.lights.block(tower + Vec3::new(-1.0, 17.0, -2.55), tower + Vec3::new(1.0, 19.0, -2.5), [250, 240, 200]);
        }
        Building::Park => {
            for i in 0..3 {
                let h = noise(p.x as u32, p.y as u32, 300 + i);
                let at = o + Vec3::new(2.0 + (h % 60) as f32 / 10.0, 0.0, 2.0 + (h / 60 % 60) as f32 / 10.0);
                tree(&mut cx.m.solid, at, 0.8 + (h % 4) as f32 * 0.1, h, cx.fade);
            }
            cx.m.solid.flat(o.x + 4.4, o.z, o.x + 5.6, o.z + TILE, 0.05, [196, 180, 140]);
            cx.m.solid.block(o + Vec3::new(6.4, 0.0, 4.6), o + Vec3::new(7.8, 0.5, 5.2), [120, 84, 52]);
        }
        Building::Plaza => {
            cx.m.solid.flat(o.x + 0.2, o.z + 0.2, o.x + 9.8, o.z + 9.8, 0.06, [208, 196, 172]);
            cx.m.solid.cylinder(c, 2.4, 0.6, 14, CONCRETE, CONCRETE);
            cx.m.water.flat(c.x - 1.9, c.z - 1.9, c.x + 1.9, c.z + 1.9, 0.62, [90, 170, 220]);
            cx.m.solid.cylinder(c + Vec3::Y * 0.6, 0.3, 1.8, 8, CONCRETE, CONCRETE);
            for (dx, dz) in [(-3.6f32, -3.6f32), (3.6, -3.6), (-3.6, 3.6), (3.6, 3.6)] {
                tree(&mut cx.m.solid, c + Vec3::new(dx, 0.0, dz), 0.55, v, cx.fade);
            }
        }
        Building::Playground => {
            cx.m.solid.flat(o.x + 1.0, o.z + 1.0, o.x + 9.0, o.z + 9.0, 0.05, [214, 190, 130]);
            cx.m.solid.block(o + Vec3::new(2.0, 0.0, 2.0), o + Vec3::new(4.5, 2.4, 4.5), [220, 70, 60]);
            cx.m.solid.pyramid(o + Vec3::new(1.8, 2.4, 1.8), o + Vec3::new(4.7, 3.8, 4.7), [60, 120, 210]);
            cx.m.solid.quad([o + Vec3::new(4.5, 2.2, 2.4), o + Vec3::new(4.5, 2.2, 4.1), o + Vec3::new(7.5, 0.2, 4.1), o + Vec3::new(7.5, 0.2, 2.4)], Vec3::new(0.55, 0.83, 0.0), [240, 200, 60]);
            cx.m.solid.bar(o + Vec3::new(5.5, 0.0, 6.5), o + Vec3::new(5.5, 2.6, 6.5), 0.15, [80, 80, 90]);
            cx.m.solid.bar(o + Vec3::new(8.5, 0.0, 6.5), o + Vec3::new(8.5, 2.6, 6.5), 0.15, [80, 80, 90]);
            cx.m.solid.bar(o + Vec3::new(5.5, 2.6, 6.5), o + Vec3::new(8.5, 2.6, 6.5), 0.15, [80, 80, 90]);
            tree(&mut cx.m.solid, o + Vec3::new(8.0, 0.0, 2.0), 0.7, v, cx.fade);
        }
        Building::SportsField => {
            cx.m.solid.flat(o.x + 1.0, o.z + 1.0, o.x + 19.0, o.z + 19.0, 0.05, [70, 140, 70]);
            let line = [235, 235, 230];
            for (a, b) in [((2.0, 2.0), (18.0, 2.3)), ((2.0, 17.7), (18.0, 18.0)), ((2.0, 2.0), (2.3, 18.0)), ((17.7, 2.0), (18.0, 18.0)), ((2.0, 9.85), (18.0, 10.15))] {
                cx.m.solid.flat(o.x + a.0, o.z + a.1, o.x + b.0, o.z + b.1, 0.07, line);
            }
            for z in [3.0f32, 17.0] {
                cx.m.solid.bar(o + Vec3::new(8.5, 0.0, z), o + Vec3::new(8.5, 2.4, z), 0.15, line);
                cx.m.solid.bar(o + Vec3::new(11.5, 0.0, z), o + Vec3::new(11.5, 2.4, z), 0.15, line);
                cx.m.solid.bar(o + Vec3::new(8.5, 2.4, z), o + Vec3::new(11.5, 2.4, z), 0.15, line);
            }
            for (x, z) in [(0.5f32, 0.5f32), (19.5, 0.5), (0.5, 19.5), (19.5, 19.5)] {
                cx.m.solid.block(o + Vec3::new(x - 0.15, 0.0, z - 0.15), o + Vec3::new(x + 0.15, 12.0, z + 0.15), [160, 160, 160]);
                cx.m.lights.block(o + Vec3::new(x - 0.6, 12.0, z - 0.6), o + Vec3::new(x + 0.6, 12.8, z + 0.6), [255, 250, 220]);
            }
        }
        Building::Stadium => {
            cx.m.solid.flat(o.x + 8.0, o.z + 6.0, o.x + 22.0, o.z + 24.0, 0.06, [70, 150, 70]);
            let stand = k([196, 196, 204], cx);
            let seats = k([[60, 90, 200], [200, 60, 60], [230, 180, 40]][(v % 3) as usize], cx);
            for tier in 0..4 {
                let i = tier as f32 * 1.6;
                let y = tier as f32 * 3.0;
                for (a, b) in [
                    (Vec3::new(2.0 + i, y, 2.0 + i), Vec3::new(28.0 - i, y + 3.0, 5.0 + i)),
                    (Vec3::new(2.0 + i, y, 25.0 - i), Vec3::new(28.0 - i, y + 3.0, 28.0 - i)),
                    (Vec3::new(2.0 + i, y, 5.0 + i), Vec3::new(5.0 + i, y + 3.0, 25.0 - i)),
                    (Vec3::new(25.0 - i, y, 5.0 + i), Vec3::new(28.0 - i, y + 3.0, 25.0 - i)),
                ] {
                    cx.m.solid.cuboid(o + a, o + b, stand, seats);
                }
            }
            for (x, z) in [(2.0f32, 2.0f32), (28.0, 2.0), (2.0, 28.0), (28.0, 28.0)] {
                cx.m.solid.block(o + Vec3::new(x - 0.3, 0.0, z - 0.3), o + Vec3::new(x + 0.3, 24.0, z + 0.3), [170, 170, 170]);
                cx.m.lights.block(o + Vec3::new(x - 1.2, 24.0, z - 1.2), o + Vec3::new(x + 1.2, 25.6, z + 1.2), [255, 250, 220]);
            }
        }
        Building::TownHall => {
            pad(cx, 0.3, [214, 206, 190]);
            let stone = k([226, 218, 200], cx);
            cx.m.solid.block(o + Vec3::new(2.0, 0.0, 4.0), o + Vec3::new(18.0, 10.0, 16.0), stone);
            floors(cx.m, o + Vec3::new(2.0, 0.0, 4.0), o + Vec3::new(18.0, 10.0, 16.0), 4.5, seed, 0.6);
            for x in 0..6 {
                let cx0 = o + Vec3::new(3.5 + x as f32 * 2.6, 0.0, 3.0);
                cx.m.solid.cylinder(cx0, 0.45, 9.0, 8, k([240, 236, 226], cx), stone);
            }
            cx.m.solid.block(o + Vec3::new(2.5, 9.0, 2.5), o + Vec3::new(17.5, 10.2, 4.2), stone);
            cx.m.solid.cylinder(o + Vec3::new(10.0, 10.0, 10.0), 3.6, 4.0, 16, stone, stone);
            cx.m.solid.frustum(o + Vec3::new(10.0, 14.0, 10.0), 3.8, 0.4, 5.0, 16, k([110, 150, 140], cx), [110, 150, 140]);
            cx.m.solid.block(o + Vec3::new(9.9, 19.0, 9.9), o + Vec3::new(10.1, 23.0, 10.1), [200, 200, 200]);
            cx.m.solid.block(o + Vec3::new(10.1, 21.6, 10.0), o + Vec3::new(11.8, 22.8, 10.05), [200, 60, 60]);
        }
        Building::Monument => {
            cx.m.solid.flat(o.x + 0.5, o.z + 0.5, o.x + 9.5, o.z + 9.5, 0.05, [208, 200, 184]);
            cx.m.solid.block(c - Vec3::new(2.4, 0.0, 2.4), c + Vec3::new(2.4, 1.2, 2.4), [200, 196, 186]);
            cx.m.solid.block(c - Vec3::new(1.2, -1.2, 1.2), c + Vec3::new(1.2, 16.0, 1.2), k([232, 228, 218], cx));
            cx.m.solid.pyramid(c.with_y(16.0) - Vec3::new(1.2, 0.0, 1.2), c.with_y(19.0) + Vec3::new(1.2, 0.0, 1.2), k([232, 228, 218], cx));
        }
        Building::Airport => {
            cx.m.solid.flat(o.x + 1.0, o.z + 4.0, o.x + 39.0, o.z + 10.0, 0.06, [72, 72, 78]);
            for k2 in 0..9 {
                let x = o.x + 3.0 + k2 as f32 * 4.0;
                cx.m.solid.flat(x, o.z + 6.85, x + 2.0, o.z + 7.15, 0.08, [235, 235, 230]);
            }
            cx.m.solid.flat(o.x + 4.0, o.z + 14.0, o.x + 36.0, o.z + 26.0, 0.06, [110, 112, 112]);
            let term = k([210, 216, 226], cx);
            cx.m.solid.cuboid(o + Vec3::new(4.0, 0.0, 28.0), o + Vec3::new(30.0, 9.0, 38.0), term, [170, 176, 186]);
            floors(cx.m, o + Vec3::new(4.0, 0.0, 28.0), o + Vec3::new(30.0, 9.0, 38.0), 4.4, seed, 0.6);
            let tower = o + Vec3::new(35.0, 0.0, 33.0);
            cx.m.solid.cylinder(tower, 1.4, 20.0, 10, term, term);
            cx.m.glass.frustum(tower + Vec3::Y * 20.0, 2.6, 3.0, 3.0, 10, [60, 90, 120], [80, 80, 90]);
            for (x, z) in [(12.0f32, 20.0f32), (26.0, 19.0)] {
                plane(cx, o + Vec3::new(x, 0.1, z));
            }
        }
    }
    // Unpowered services and plants get a red beacon so the problem reads in 3D.
    let s = cx.state.config.building(b);
    if s.power_draw > 0 && !t.powered {
        let at = c + Vec3::Y * 12.0;
        cx.m.lights.block(at - Vec3::splat(0.8), at + Vec3::splat(0.8), [255, 40, 30]);
    }
}

fn plane(cx: &mut Ctx, at: Vec3) {
    let white = [236, 236, 240];
    cx.m.solid.block(at + Vec3::new(-6.0, 1.2, -0.8), at + Vec3::new(6.0, 2.8, 0.8), white);
    cx.m.solid.block(at + Vec3::new(-1.5, 1.8, -6.0), at + Vec3::new(1.5, 2.1, 6.0), white);
    cx.m.solid.block(at + Vec3::new(-6.0, 2.8, -0.15), at + Vec3::new(-4.5, 5.0, 0.15), [200, 60, 60]);
    cx.m.solid.block(at + Vec3::new(-0.3, 0.0, -0.3), at + Vec3::new(0.3, 1.2, 0.3), [40, 40, 40]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use parcels_sim::{Config, NewGame, Session};

    #[test]
    fn grown_city_builds_valid_meshes_for_every_overlay() {
        let mut session = Session::new(GameState::new(&NewGame::solo(2, Config::default(), "x", 3)));
        for _ in 0..600 {
            session.tick();
        }
        let s = &session.state;
        for overlay in Overlay::ALL {
            let m = build_chunk(s, Look { overlay, show_pipes: true }, s.map.bounds());
            for buf in [&m.solid, &m.glass, &m.lights, &m.water] {
                assert_eq!(buf.pos.len(), buf.nrm.len());
                assert_eq!(buf.pos.len(), buf.col.len());
                assert_eq!(buf.idx.len() % 3, 0);
                assert!(buf.idx.iter().all(|&i| (i as usize) < buf.pos.len()));
                assert!(buf.pos.iter().flatten().all(|v| v.is_finite()));
            }
            assert!(!m.solid.is_empty());
        }
    }

    #[test]
    fn every_building_type_draws_something() {
        let mut s = GameState::new(&NewGame::solo(2, Config::default(), "x", 0));
        let mut x = 0;
        for b in Building::ALL {
            let fp = Rect::square(Pos::new(x, 2), b.size());
            for (i, p) in fp.iter().enumerate() {
                let t = s.map.tile_mut(p);
                t.terrain = Terrain::Land;
                t.kind = TileKind::Building(b);
                t.part = i as u8;
            }
            let m = build_chunk(&s, Look::default(), fp);
            assert!(m.solid.pos.len() > 30, "{b:?} drew only {} vertices", m.solid.pos.len());
            x += b.size() + 1;
            if x > 56 {
                break;
            }
        }
    }

    #[test]
    fn quad_winding_faces_its_normal() {
        let mut m = MeshBuf::default();
        m.cuboid(Vec3::ZERO, Vec3::ONE, [1, 2, 3], [1, 2, 3]);
        for tri in m.idx.chunks(3) {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| Vec3::from_array(m.pos[i as usize]));
            let n = Vec3::from_array(m.nrm[tri[0] as usize]);
            assert!((b - a).cross(c - a).dot(n) > 0.0);
        }
    }
}
