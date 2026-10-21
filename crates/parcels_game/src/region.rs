//! Regions on disk: a grid of plots, each empty or holding one city. A region
//! is a folder under `saves/regions/<slug>/` with `region.ron` (the grid and a
//! summary of every city, so the region map never has to load a whole city)
//! and one `x<col>y<row>.ron` save per founded city.

use std::path::PathBuf;

use parcels_sim::{region, GameState, Links, Neighbour, NewGame, Side, TerrainSettings};
use serde::{Deserialize, Serialize};

use crate::menu::load_config;
use crate::saves::{slug, SAVE_DIR};
use crate::view::{map_color, Overlay};

/// Thumbnails are at most this many pixels wide.
const THUMB_W: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Region {
    pub name: String,
    pub seed: u64,
    pub cols: u8,
    pub rows: u8,
    /// Every city's map size.
    pub map: (u16, u16),
    /// Starting money per city, whole dollars.
    pub money: i64,
    pub fires: bool,
    pub eras: bool,
    pub terrain: TerrainSettings,
    /// Row-major, `cols * rows` long.
    pub plots: Vec<Option<PlotCity>>,
}

/// A founded city as the region map shows it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlotCity {
    pub name: String,
    pub summary: Neighbour,
    pub era: u8,
    pub treasury: i64,
    pub year: u64,
    pub approval: u8,
    pub links: [Links; 4],
    /// RGB, `thumb_size.0 * thumb_size.1` pixels.
    pub thumb: Vec<u8>,
    pub thumb_size: (u16, u16),
}

/// Which plot of which region a game in progress belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub region: String,
    pub x: u8,
    pub y: u8,
}

fn root() -> PathBuf {
    PathBuf::from(SAVE_DIR).join("regions")
}

impl Region {
    pub fn new(name: &str, seed: u64, cols: u8, rows: u8, map: (u16, u16)) -> Self {
        Self {
            name: name.trim().to_string(),
            seed,
            cols,
            rows,
            map,
            money: 20_000,
            fires: true,
            eras: true,
            terrain: TerrainSettings::default(),
            plots: vec![None; cols as usize * rows as usize],
        }
    }

    pub fn slug(&self) -> String {
        slug(&self.name)
    }

    fn dir(slug: &str) -> PathBuf {
        root().join(slug)
    }

    fn city_path(slug: &str, x: u8, y: u8) -> PathBuf {
        Self::dir(slug).join(format!("x{x}y{y}.ron"))
    }

    pub fn exists(name: &str) -> bool {
        Self::dir(&slug(name)).join("region.ron").exists()
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = Self::dir(&self.slug());
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default().compact_arrays(true)).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("region.ron"), text).map_err(|e| e.to_string())
    }

    pub fn load(slug: &str) -> Result<Self, String> {
        let path = Self::dir(slug).join("region.ron");
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        ron::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Every region on disk, by name.
    pub fn list() -> Vec<(String, Region)> {
        let Ok(dir) = std::fs::read_dir(root()) else { return Vec::new() };
        let mut out: Vec<(String, Region)> = dir
            .flatten()
            .filter_map(|e| {
                let slug = e.file_name().to_string_lossy().to_string();
                Region::load(&slug).ok().map(|r| (slug, r))
            })
            .collect();
        out.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        out
    }

    pub fn plot(&self, x: u8, y: u8) -> Option<&PlotCity> {
        self.plots.get(y as usize * self.cols as usize + x as usize)?.as_ref()
    }

    /// The plot beyond `side` of (x, y), if it's on the grid.
    pub fn beside(&self, x: u8, y: u8, side: Side) -> Option<(u8, u8)> {
        let (dx, dy) = side.offset();
        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
        (nx >= 0 && ny >= 0 && nx < self.cols as i32 && ny < self.rows as i32).then_some((nx as u8, ny as u8))
    }

    /// What the city at (x, y) should know about its neighbours.
    pub fn neighbours_of(&self, x: u8, y: u8) -> [Option<Neighbour>; 4] {
        Side::ALL.map(|side| {
            let (nx, ny) = self.beside(x, y, side)?;
            self.plot(nx, ny).map(|c| c.summary.clone())
        })
    }

    pub fn population(&self) -> u32 {
        self.plots.iter().flatten().map(|c| c.summary.population).sum()
    }

    /// A brand-new city on an empty plot. Each plot has its own land, seeded
    /// from the region.
    pub fn found(&self, x: u8, y: u8, name: &str) -> GameState {
        let mut config = load_config();
        config.map_width = self.map.0;
        config.map_height = self.map.1;
        config.starting_treasury = self.money * 100;
        config.eras = self.eras;
        if !self.fires {
            config.fire_chance_per_million = 0;
        }
        let seed = self.seed ^ (x as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64 + 1).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
        let name = if name.trim().is_empty() { "New City" } else { name.trim() };
        GameState::new(&NewGame { seed, config, city_name: name.to_string(), terrain: self.terrain })
    }

    pub fn load_city(&self, x: u8, y: u8) -> Result<GameState, String> {
        let path = Self::city_path(&self.slug(), x, y);
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        GameState::from_ron(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Write a city's save and refresh its plot on the region map.
    pub fn store(&mut self, x: u8, y: u8, state: &GameState) -> Result<(), String> {
        let path = Self::city_path(&self.slug(), x, y);
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&path, state.to_ron()).map_err(|e| e.to_string())?;
        let i = y as usize * self.cols as usize + x as usize;
        self.plots[i] = Some(PlotCity::of(state));
        self.save()
    }
}

impl PlotCity {
    pub fn of(state: &GameState) -> Self {
        let (w, h) = (state.map.width as usize, state.map.height as usize);
        let step = w.div_ceil(THUMB_W).max(1);
        let (tw, th) = (w.div_ceil(step), h.div_ceil(step));
        let mut thumb = Vec::with_capacity(tw * th * 3);
        for ty in 0..th {
            for tx in 0..tw {
                let (x, y) = ((tx * step).min(w - 1), (ty * step).min(h - 1));
                thumb.extend_from_slice(&map_color(state, Overlay::None, y * w + x));
            }
        }
        Self {
            name: state.city.name.clone(),
            summary: region::summary(state),
            era: state.city.era,
            treasury: state.city.treasury,
            year: state.date().0,
            approval: state.city.stats.approval_percent(),
            links: region::links(state),
            thumb,
            thumb_size: (tw as u16, th as u16),
        }
    }
}

/// Save the game in progress back into its region, if it belongs to one.
pub fn store_slot(slot: &Slot, state: &GameState) -> Result<(), String> {
    let mut r = Region::load(&slot.region)?;
    r.store(slot.x, slot.y, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `PARCELS_DEMO_DIR=/some/dir cargo test -p parcels_game -- --ignored demo_region`
    /// writes a small region with three autoplay cities there, for screenshots.
    #[test]
    #[ignore]
    fn demo_region() {
        let dir = std::env::var("PARCELS_DEMO_DIR").expect("PARCELS_DEMO_DIR");
        std::env::set_current_dir(dir).unwrap();
        let mut r = Region::new("Demo Valley", 77, 3, 2, (64, 48));
        for (x, y, name, years) in [(0u8, 0u8, "Westbrook", 6u64), (1, 0, "Millford", 4), (1, 1, "Southgate", 2)] {
            let mut s = parcels_sim::Session::new(r.found(x, y, name));
            s.autoplay = true;
            s.submit(parcels_sim::CommandKind::SetNeighbours { neighbours: r.neighbours_of(x, y) });
            let ticks = years * 12 * s.state.config.ticks_per_month as u64;
            for _ in 0..ticks {
                s.tick();
            }
            r.store(x, y, &s.state).unwrap();
        }
    }

    #[test]
    fn neighbours_come_from_the_plots_around() {
        let mut r = Region::new("Test", 1, 3, 2, (32, 24));
        let state = r.found(1, 0, "Middle");
        r.plots[1] = Some(PlotCity::of(&state));
        // (1, 1) has the city to its north and nothing elsewhere.
        let n = r.neighbours_of(1, 1);
        assert_eq!(n[Side::North.index()].as_ref().unwrap().name, "Middle");
        assert!(n[Side::East.index()].is_none() && n[Side::South.index()].is_none() && n[Side::West.index()].is_none());
        // Off the grid there is nothing.
        assert_eq!(r.beside(0, 0, Side::West), None);
        assert_eq!(r.beside(2, 1, Side::East), None);
        // Each plot gets its own land.
        assert_ne!(r.found(0, 0, "A").seed, r.found(2, 1, "B").seed);
    }
}
