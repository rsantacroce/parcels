//! Named save games. Each save is the whole `GameState` as RON (`<slug>.ron`)
//! plus a small sidecar (`<slug>.meta.ron`) so the load screen can list saves
//! without parsing every map.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use parcels_sim::{Controller, GameState};
use serde::{Deserialize, Serialize};

pub const SAVE_DIR: &str = "saves";
pub const QUICKSAVE: &str = "quicksave";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SaveMeta {
    pub name: String,
    pub game_version: String,
    /// Unix seconds.
    pub saved_at: u64,
    pub year: u64,
    pub month: u64,
    pub map: (u16, u16),
    /// (name, human?, score in cents)
    pub players: Vec<(String, bool, i64)>,
    pub population: u32,
}

#[derive(Clone, Debug)]
pub struct SaveEntry {
    pub slug: String,
    pub meta: Option<SaveMeta>,
    pub modified: SystemTime,
}

impl SaveEntry {
    pub fn title(&self) -> String {
        self.meta.as_ref().map_or_else(|| self.slug.clone(), |m| m.name.clone())
    }

    pub fn age(&self) -> String {
        let secs = SystemTime::now().duration_since(self.modified).map(|d| d.as_secs()).unwrap_or(0);
        match secs {
            0..60 => "just now".into(),
            60..3600 => format!("{} min ago", secs / 60),
            3600..86400 => format!("{} h ago", secs / 3600),
            _ => format!("{} days ago", secs / 86400),
        }
    }
}

/// File-name-safe version of a save name.
pub fn slug(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "untitled".into() } else { s.chars().take(48).collect() }
}

fn state_path(slug: &str) -> PathBuf {
    Path::new(SAVE_DIR).join(format!("{slug}.ron"))
}

fn meta_path(slug: &str) -> PathBuf {
    Path::new(SAVE_DIR).join(format!("{slug}.meta.ron"))
}

pub fn meta_of(state: &GameState, name: &str) -> SaveMeta {
    let (year, month, _) = state.date();
    SaveMeta {
        name: name.to_string(),
        game_version: env!("CARGO_PKG_VERSION").to_string(),
        saved_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        year,
        month,
        map: (state.map.width, state.map.height),
        players: state
            .players
            .iter()
            .filter(|p| p.controller != Controller::Vacant)
            .map(|p| (p.name.clone(), p.controller == Controller::Human, p.stats.score))
            .collect(),
        population: state.global.population,
    }
}

/// Write `state` under `name`. Returns the slug used.
pub fn save(state: &GameState, name: &str) -> Result<String, String> {
    std::fs::create_dir_all(SAVE_DIR).map_err(|e| e.to_string())?;
    let slug = slug(name);
    std::fs::write(state_path(&slug), state.to_ron()).map_err(|e| e.to_string())?;
    let meta = ron::ser::to_string_pretty(&meta_of(state, name), ron::ser::PrettyConfig::default()).map_err(|e| e.to_string())?;
    std::fs::write(meta_path(&slug), meta).map_err(|e| e.to_string())?;
    Ok(slug)
}

pub fn load(slug: &str) -> Result<GameState, String> {
    let path = state_path(slug);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    GameState::from_ron(&text).map_err(|e| {
        format!("{} can't be read by this version of Parcels ({e}). It may be from an older release.", path.display())
    })
}

pub fn delete(slug: &str) -> Result<(), String> {
    std::fs::remove_file(state_path(slug)).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(meta_path(slug));
    Ok(())
}

/// Every save, newest first.
pub fn list() -> Vec<SaveEntry> {
    let Ok(dir) = std::fs::read_dir(SAVE_DIR) else { return Vec::new() };
    let mut out: Vec<SaveEntry> = dir
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let slug = name.strip_suffix(".ron")?.to_string();
            if slug.ends_with(".meta") || slug == "settings" || slug == "replay" {
                return None;
            }
            let modified = e.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
            let meta = std::fs::read_to_string(meta_path(&slug)).ok().and_then(|t| ron::from_str(&t).ok());
            Some(SaveEntry { slug, meta, modified })
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

pub fn exists(slug: &str) -> bool {
    state_path(slug).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_file_safe() {
        assert_eq!(slug("My Town #2!"), "my-town-2");
        assert_eq!(slug("   "), "untitled");
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
    }
}
