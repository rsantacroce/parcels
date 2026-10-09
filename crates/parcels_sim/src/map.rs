//! The tile grid: a dense row-major array. This is the single source of truth for
//! the world; the renderer only ever reads it.

use serde::{Deserialize, Serialize};

use crate::catalog::{Building, Category, Service};
use crate::ids::ParcelId;
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Terrain {
    Land,
    /// Fixed river or lake water. Only roads (bridges) and power lines can go on it.
    Water,
    /// Woodland. Lifts land value nearby; building on it clears it first, at a cost.
    Forest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Road {
    Street,
    /// Four lanes: twice the cost, half the congestion.
    Avenue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Density {
    Low,
    High,
}

/// What occupies a tile's surface. Power lines and water pipes are overlays
/// (`Tile::wire`, `Tile::pipe`) so they can share a tile with roads and zones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TileKind {
    Empty,
    Road(Road),
    Zone(Zone, Density),
    /// One tile of a (possibly multi-tile) building; see `Tile::part`.
    Building(Building),
}

impl TileKind {
    pub fn is_zone(self) -> bool {
        matches!(self, TileKind::Zone(..))
    }

    pub fn zone(self) -> Option<Zone> {
        match self {
            TileKind::Zone(z, _) => Some(z),
            _ => None,
        }
    }

    pub fn density(self) -> Option<Density> {
        match self {
            TileKind::Zone(_, d) => Some(d),
            _ => None,
        }
    }

    pub fn is_road(self) -> bool {
        matches!(self, TileKind::Road(_))
    }

    pub fn building(self) -> Option<Building> {
        match self {
            TileKind::Building(b) => Some(b),
            _ => None,
        }
    }

    pub fn name(self) -> String {
        match self {
            TileKind::Empty => "Empty".into(),
            TileKind::Road(Road::Street) => "Street".into(),
            TileKind::Road(Road::Avenue) => "Avenue".into(),
            TileKind::Zone(z, Density::Low) => format!("{} (low density)", z.name()),
            TileKind::Zone(z, Density::High) => format!("{} (high density)", z.name()),
            TileKind::Building(b) => b.name().into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Zone {
    Residential = 0,
    Commercial = 1,
    Industrial = 2,
    /// Clean white-collar jobs. Needs education and good land.
    Office = 3,
}

impl Zone {
    pub const ALL: [Zone; 4] = [Zone::Residential, Zone::Commercial, Zone::Industrial, Zone::Office];
    pub const COUNT: usize = 4;

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Zone::Residential => "Residential",
            Zone::Commercial => "Commercial",
            Zone::Industrial => "Industrial",
            Zone::Office => "Office",
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            Zone::Residential => "R",
            Zone::Commercial => "C",
            Zone::Industrial => "I",
            Zone::Office => "O",
        }
    }
}

/// Things a player can place. Each maps to a surface kind or an overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Buildable {
    Road(Road),
    PowerLine,
    WaterPipe,
    Zone(Zone, Density),
    Building(Building),
}

impl Buildable {
    /// Every buildable, in toolbar order.
    pub fn all() -> Vec<Buildable> {
        let mut v = vec![Buildable::Road(Road::Street), Buildable::Road(Road::Avenue), Buildable::PowerLine, Buildable::WaterPipe];
        for z in Zone::ALL {
            v.push(Buildable::Zone(z, Density::Low));
            v.push(Buildable::Zone(z, Density::High));
        }
        v.extend(Building::ALL.iter().map(|&b| Buildable::Building(b)));
        v
    }

    pub fn category(self) -> Category {
        match self {
            Buildable::Road(_) => Category::Transport,
            Buildable::PowerLine => Category::Power,
            Buildable::WaterPipe => Category::Water,
            Buildable::Zone(..) => Category::Zones,
            Buildable::Building(b) => b.category(),
        }
    }

    pub fn surface(self) -> Option<TileKind> {
        match self {
            Buildable::Road(r) => Some(TileKind::Road(r)),
            Buildable::Zone(z, d) => Some(TileKind::Zone(z, d)),
            Buildable::Building(b) => Some(TileKind::Building(b)),
            Buildable::PowerLine | Buildable::WaterPipe => None,
        }
    }

    /// Footprint edge in tiles (1 for everything but big buildings).
    pub fn size(self) -> u16 {
        match self {
            Buildable::Building(b) => b.size(),
            _ => 1,
        }
    }

    pub fn name(self) -> String {
        match self {
            Buildable::Road(Road::Street) => "Street".into(),
            Buildable::Road(Road::Avenue) => "Avenue".into(),
            Buildable::PowerLine => "Power line".into(),
            Buildable::WaterPipe => "Water pipe".into(),
            Buildable::Zone(z, Density::Low) => z.name().into(),
            Buildable::Zone(z, Density::High) => format!("Dense {}", z.name().to_lowercase()),
            Buildable::Building(b) => b.name().into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: Terrain,
    pub kind: TileKind,
    /// Power line overlay.
    pub wire: bool,
    /// Water pipe overlay (underground).
    pub pipe: bool,
    /// Owning parcel. The owner is `parcels[parcel].owner`, so handing a parcel to
    /// someone else is a single field change.
    pub parcel: ParcelId,
    /// Index of this tile inside its building's footprint, row-major from the
    /// anchor (min corner) = 0. Always 0 for everything else.
    pub part: u8,
    pub powered: bool,
    pub watered: bool,
    /// Zone development level, 0 = zoned but empty.
    pub level: u8,
    /// 0..=config.land_value_max
    pub land_value: u16,
    pub pollution: u16,
    pub traffic: u16,
    pub crime: u16,
    /// Bitmask of `Service::bit()` for services covering this tile.
    pub coverage: u8,
    /// Ticks left until a burnt-out lot can be rebuilt; 0 = not burning.
    pub burning: u8,
}

impl Tile {
    fn new(parcel: ParcelId) -> Self {
        Self {
            terrain: Terrain::Land,
            kind: TileKind::Empty,
            wire: false,
            pipe: false,
            parcel,
            part: 0,
            powered: false,
            watered: false,
            level: 0,
            land_value: 0,
            pollution: 0,
            traffic: 0,
            crime: 0,
            coverage: 0,
            burning: 0,
        }
    }

    pub fn conducts_power(&self) -> bool {
        match self.kind {
            TileKind::Zone(..) => true,
            TileKind::Building(b) => b.conducts(),
            _ => self.wire,
        }
    }

    pub fn conducts_water(&self) -> bool {
        match self.kind {
            TileKind::Zone(..) => true,
            TileKind::Building(b) => b.conducts(),
            _ => self.pipe,
        }
    }

    /// Nothing built and nothing growing: grass, water or sand.
    pub fn is_bare(&self) -> bool {
        self.kind == TileKind::Empty && !self.wire && !self.pipe && self.terrain != Terrain::Forest
    }

    /// The anchor tile of a building, or any non-building tile.
    pub fn is_anchor(&self) -> bool {
        self.part == 0
    }

    pub fn covered(&self, s: Service) -> bool {
        self.coverage & s.bit() != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Pos {
    pub x: u16,
    pub y: u16,
}

impl Pos {
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }

    pub fn manhattan(self, o: Pos) -> u32 {
        (self.x.abs_diff(o.x) + self.y.abs_diff(o.y)) as u32
    }
}

/// Inclusive tile rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub min: Pos,
    pub max: Pos,
}

impl Rect {
    pub fn from_corners(a: Pos, b: Pos) -> Self {
        Self { min: Pos::new(a.x.min(b.x), a.y.min(b.y)), max: Pos::new(a.x.max(b.x), a.y.max(b.y)) }
    }

    /// A `size`×`size` square with its min corner at `anchor`.
    pub fn square(anchor: Pos, size: u16) -> Self {
        let s = size.max(1) - 1;
        Self { min: anchor, max: Pos::new(anchor.x.saturating_add(s), anchor.y.saturating_add(s)) }
    }

    pub fn contains(&self, p: Pos) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    pub fn contains_rect(&self, r: &Rect) -> bool {
        self.contains(r.min) && self.contains(r.max)
    }

    pub fn width(&self) -> u16 {
        self.max.x - self.min.x + 1
    }

    pub fn height(&self) -> u16 {
        self.max.y - self.min.y + 1
    }

    pub fn area(&self) -> u32 {
        self.width() as u32 * self.height() as u32
    }

    pub fn center(&self) -> Pos {
        Pos::new((self.min.x + self.max.x) / 2, (self.min.y + self.max.y) / 2)
    }

    /// Row-major iteration: deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = Pos> + '_ {
        (self.min.y..=self.max.y).flat_map(move |y| (self.min.x..=self.max.x).map(move |x| Pos::new(x, y)))
    }
}

/// Knobs for generating the land at the start of a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TerrainSettings {
    /// 0..=2 rivers. The first runs north–south, the second east–west.
    pub rivers: u8,
    /// Lakes per 64×64 area, 0..=4.
    pub lakes: u8,
    /// Woodland amount, 0 (none) ..= 3 (lots).
    pub forest: u8,
}

impl Default for TerrainSettings {
    fn default() -> Self {
        Self { rivers: 1, lakes: 1, forest: 2 }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Map {
    pub width: u16,
    pub height: u16,
    pub tiles: Vec<Tile>,
}

impl Map {
    /// Sizes the new-game screen offers. The sim itself accepts anything.
    pub const MIN_SIZE: u16 = 32;
    pub const MAX_SIZE: u16 = 256;

    pub fn new(width: u16, height: u16) -> Self {
        Self { width, height, tiles: vec![Tile::new(ParcelId::NONE); width as usize * height as usize] }
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn in_bounds(&self, p: Pos) -> bool {
        p.x < self.width && p.y < self.height
    }

    pub fn idx(&self, p: Pos) -> usize {
        p.y as usize * self.width as usize + p.x as usize
    }

    pub fn pos(&self, idx: usize) -> Pos {
        Pos::new((idx % self.width as usize) as u16, (idx / self.width as usize) as u16)
    }

    pub fn get(&self, p: Pos) -> Option<&Tile> {
        self.in_bounds(p).then(|| &self.tiles[self.idx(p)])
    }

    pub fn tile(&self, p: Pos) -> &Tile {
        &self.tiles[self.idx(p)]
    }

    pub fn tile_mut(&mut self, p: Pos) -> &mut Tile {
        let i = self.idx(p);
        &mut self.tiles[i]
    }

    pub fn bounds(&self) -> Rect {
        Rect { min: Pos::new(0, 0), max: Pos::new(self.width - 1, self.height - 1) }
    }

    /// Footprint of the building covering `p`, or just `p` itself.
    pub fn footprint_at(&self, p: Pos) -> Rect {
        let t = self.tile(p);
        match t.kind {
            TileKind::Building(b) => {
                let s = b.size();
                let (dx, dy) = (t.part as u16 % s, t.part as u16 / s);
                Rect::square(Pos::new(p.x - dx, p.y - dy), s)
            }
            _ => Rect { min: p, max: p },
        }
    }

    /// 4-neighbours of a tile index, in fixed N, W, E, S order.
    pub fn neighbors4(&self, idx: usize) -> impl Iterator<Item = usize> {
        let w = self.width as usize;
        let h = self.height as usize;
        let x = idx % w;
        let y = idx / w;
        let n = (y > 0).then(|| idx - w);
        let west = (x > 0).then(|| idx - 1);
        let e = (x + 1 < w).then(|| idx + 1);
        let s = (y + 1 < h).then(|| idx + w);
        [n, west, e, s].into_iter().flatten()
    }

    /// All tile indices within Manhattan distance `r` of `center`, with the distance.
    /// Row-major order.
    pub fn within(&self, center: Pos, r: u8) -> impl Iterator<Item = (usize, u32)> + '_ {
        let r = r as i32;
        let (cx, cy) = (center.x as i32, center.y as i32);
        let (w, h) = (self.width as i32, self.height as i32);
        (cy - r..=cy + r).flat_map(move |y| {
            (cx - r..=cx + r).filter_map(move |x| {
                if x < 0 || y < 0 || x >= w || y >= h {
                    return None;
                }
                let d = (x - cx).unsigned_abs() + (y - cy).unsigned_abs();
                (d as i32 <= r).then(|| ((y * w + x) as usize, d))
            })
        })
    }

    /// Lay out rivers, lakes and woods. Seeded and integer-only.
    pub fn generate(&mut self, seed: u64, terrain: &TerrainSettings) {
        for i in 0..terrain.rivers.min(2) {
            self.carve_river(seed.wrapping_add(i as u64), i == 1);
        }
        let area = self.width as u32 * self.height as u32;
        let mut rng = Rng::new(seed ^ 0x4c41_4b45);
        let lakes = (terrain.lakes.min(4) as u32 * area).div_ceil(64 * 64);
        for _ in 0..lakes {
            let r = 2 + rng.below(4) as i32;
            let c = self.random_inner(&mut rng, r as u16 + 2);
            self.blob(&mut rng, c, r, Terrain::Water, 1000);
        }
        let mut rng = Rng::new(seed ^ 0x464f_5245_5354);
        let woods = (terrain.forest.min(3) as u32 * area * 3).div_ceil(64 * 64);
        for _ in 0..woods {
            let r = 2 + rng.below(5) as i32;
            let c = self.random_inner(&mut rng, 0);
            self.blob(&mut rng, c, r, Terrain::Forest, 750);
        }
    }

    fn random_inner(&self, rng: &mut Rng, margin: u16) -> Pos {
        let mx = margin.min(self.width / 3);
        let my = margin.min(self.height / 3);
        Pos::new(mx + rng.below((self.width - 2 * mx) as u32) as u16, my + rng.below((self.height - 2 * my) as u32) as u16)
    }

    /// Roughly round patch of `terrain` (only over land), each tile kept with
    /// probability `fill` per mille, with a ragged edge.
    fn blob(&mut self, rng: &mut Rng, c: Pos, r: i32, terrain: Terrain, fill: u16) {
        let (cx, cy) = (c.x as i32, c.y as i32);
        for y in cy - r - 1..=cy + r + 1 {
            for x in cx - r - 1..=cx + r + 1 {
                if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                    continue;
                }
                let d2 = (x - cx) * (x - cx) + (y - cy) * (y - cy);
                let edge = r * r + rng.below((2 * r + 1) as u32) as i32 - r;
                if d2 <= edge && rng.chance(fill) {
                    let t = self.tile_mut(Pos::new(x as u16, y as u16));
                    if t.terrain == Terrain::Land {
                        t.terrain = terrain;
                    }
                }
            }
        }
    }

    /// Carve a meandering river across the map, seeded. `east_west` turns it sideways.
    pub fn carve_river(&mut self, seed: u64, east_west: bool) {
        let mut rng = Rng::new(seed ^ 0x5249_5645_52);
        let (across, along) = if east_west { (self.height as i32, self.width) } else { (self.width as i32, self.height) };
        // Keep the river away from the very edges and roughly central.
        let mut x = across / 3 + rng.below((across / 3).max(1) as u32) as i32;
        for y in 0..along {
            let width = 1 + rng.below(2) as i32;
            for dx in 0..width {
                let xx = (x + dx).clamp(0, across - 1) as u16;
                let p = if east_west { Pos::new(y, xx) } else { Pos::new(xx, y) };
                self.tile_mut(p).terrain = Terrain::Water;
            }
            match rng.below(5) {
                0 => x -= 1,
                1 => x += 1,
                _ => {}
            }
            x = x.clamp(2, across - 4);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprint_roundtrip() {
        let mut m = Map::new(32, 32);
        let r = Rect::square(Pos::new(5, 7), 3);
        for (i, p) in r.iter().enumerate() {
            let t = m.tile_mut(p);
            t.kind = TileKind::Building(Building::Stadium);
            t.part = i as u8;
        }
        for p in r.iter() {
            assert_eq!(m.footprint_at(p), r);
        }
    }

    #[test]
    fn generation_is_seeded() {
        let s = TerrainSettings { rivers: 2, lakes: 3, forest: 3 };
        let mut a = Map::new(96, 64);
        let mut b = Map::new(96, 64);
        a.generate(9, &s);
        b.generate(9, &s);
        assert_eq!(a, b);
        assert!(a.tiles.iter().any(|t| t.terrain == Terrain::Forest));
        assert!(a.tiles.iter().any(|t| t.terrain == Terrain::Water));
        let mut c = Map::new(96, 64);
        c.generate(10, &s);
        assert_ne!(a, c);
    }
}
