//! The tile grid: a dense row-major array. This is the single source of truth for
//! the world; the renderer only ever reads it.

use serde::{Deserialize, Serialize};

use crate::ids::ParcelId;
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Terrain {
    Land,
    /// Fixed river water. Only roads (bridges) and power lines can go on it.
    Water,
}

/// What occupies a tile's surface. Power lines and water pipes are overlays
/// (`Tile::wire`, `Tile::pipe`) so they can share a tile with roads and zones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TileKind {
    Empty,
    Road,
    PowerPlant,
    WaterPump,
    Park,
    Residential,
    Commercial,
    Industrial,
}

impl TileKind {
    pub fn is_zone(self) -> bool {
        matches!(self, TileKind::Residential | TileKind::Commercial | TileKind::Industrial)
    }

    pub fn zone(self) -> Option<Zone> {
        match self {
            TileKind::Residential => Some(Zone::Residential),
            TileKind::Commercial => Some(Zone::Commercial),
            TileKind::Industrial => Some(Zone::Industrial),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Zone {
    Residential = 0,
    Commercial = 1,
    Industrial = 2,
}

impl Zone {
    pub const ALL: [Zone; 3] = [Zone::Residential, Zone::Commercial, Zone::Industrial];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn buildable(self) -> Buildable {
        match self {
            Zone::Residential => Buildable::Residential,
            Zone::Commercial => Buildable::Commercial,
            Zone::Industrial => Buildable::Industrial,
        }
    }

    pub fn kind(self) -> TileKind {
        match self {
            Zone::Residential => TileKind::Residential,
            Zone::Commercial => TileKind::Commercial,
            Zone::Industrial => TileKind::Industrial,
        }
    }
}

/// Things a player can place. Each maps to a surface kind or an overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Buildable {
    Road,
    PowerLine,
    WaterPipe,
    PowerPlant,
    WaterPump,
    Park,
    Residential,
    Commercial,
    Industrial,
}

impl Buildable {
    pub const ALL: [Buildable; 9] = [
        Buildable::Road,
        Buildable::Residential,
        Buildable::Commercial,
        Buildable::Industrial,
        Buildable::PowerLine,
        Buildable::WaterPipe,
        Buildable::PowerPlant,
        Buildable::WaterPump,
        Buildable::Park,
    ];

    pub fn surface(self) -> Option<TileKind> {
        Some(match self {
            Buildable::Road => TileKind::Road,
            Buildable::PowerPlant => TileKind::PowerPlant,
            Buildable::WaterPump => TileKind::WaterPump,
            Buildable::Park => TileKind::Park,
            Buildable::Residential => TileKind::Residential,
            Buildable::Commercial => TileKind::Commercial,
            Buildable::Industrial => TileKind::Industrial,
            Buildable::PowerLine | Buildable::WaterPipe => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Buildable::Road => "Road",
            Buildable::PowerLine => "Power line",
            Buildable::WaterPipe => "Water pipe",
            Buildable::PowerPlant => "Power plant",
            Buildable::WaterPump => "Water pump",
            Buildable::Park => "Park",
            Buildable::Residential => "Residential",
            Buildable::Commercial => "Commercial",
            Buildable::Industrial => "Industrial",
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
    pub powered: bool,
    pub watered: bool,
    /// Zone development level, 0 = zoned but empty.
    pub level: u8,
    /// 0..=config.land_value_max
    pub land_value: u16,
    pub pollution: u16,
    pub traffic: u16,
}

impl Tile {
    fn new(parcel: ParcelId) -> Self {
        Self {
            terrain: Terrain::Land,
            kind: TileKind::Empty,
            wire: false,
            pipe: false,
            parcel,
            powered: false,
            watered: false,
            level: 0,
            land_value: 0,
            pollution: 0,
            traffic: 0,
        }
    }

    pub fn conducts_power(&self) -> bool {
        self.wire || matches!(self.kind, TileKind::PowerPlant | TileKind::WaterPump) || self.kind.is_zone()
    }

    pub fn conducts_water(&self) -> bool {
        self.pipe || self.kind == TileKind::WaterPump || self.kind.is_zone()
    }

    pub fn is_bare(&self) -> bool {
        self.kind == TileKind::Empty && !self.wire && !self.pipe
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
        Self {
            min: Pos::new(a.x.min(b.x), a.y.min(b.y)),
            max: Pos::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    pub fn contains(&self, p: Pos) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
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

    /// Row-major iteration: deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = Pos> + '_ {
        (self.min.y..=self.max.y).flat_map(move |y| (self.min.x..=self.max.x).map(move |x| Pos::new(x, y)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Map {
    pub width: u16,
    pub height: u16,
    pub tiles: Vec<Tile>,
}

impl Map {
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

    /// Carve a meandering river from top to bottom, seeded.
    pub fn carve_river(&mut self, seed: u64) {
        let mut rng = Rng::new(seed ^ 0x5249_5645_52);
        let w = self.width as i32;
        // Keep the river away from the very edges and roughly central.
        let mut x = w / 3 + rng.below((w / 3).max(1) as u32) as i32;
        for y in 0..self.height {
            let width = 1 + rng.below(2) as i32;
            for dx in 0..width {
                let xx = (x + dx).clamp(0, w - 1) as u16;
                self.tile_mut(Pos::new(xx, y)).terrain = Terrain::Water;
            }
            match rng.below(5) {
                0 => x -= 1,
                1 => x += 1,
                _ => {}
            }
            x = x.clamp(2, w - 4);
        }
    }
}
