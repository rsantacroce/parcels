//! Player preferences, kept in `saves/settings.ron` between runs.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::saves::SAVE_DIR;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DayCycle {
    /// Always midday.
    Off,
    /// A full day every 20 minutes.
    Slow,
    /// Every 8 minutes.
    #[default]
    Normal,
    /// Every 2 minutes.
    Fast,
}

impl DayCycle {
    pub const ALL: [DayCycle; 4] = [DayCycle::Off, DayCycle::Slow, DayCycle::Normal, DayCycle::Fast];

    pub fn label(self) -> &'static str {
        match self {
            DayCycle::Off => "Always day",
            DayCycle::Slow => "Slow (20 min)",
            DayCycle::Normal => "Normal (8 min)",
            DayCycle::Fast => "Fast (2 min)",
        }
    }

    /// Real seconds per in-world day; `None` = frozen at noon.
    pub fn seconds(self) -> Option<f32> {
        match self {
            DayCycle::Off => None,
            DayCycle::Slow => Some(1200.0),
            DayCycle::Normal => Some(480.0),
            DayCycle::Fast => Some(120.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Crowds {
    Off,
    Some,
    #[default]
    Lots,
}

impl Crowds {
    pub const ALL: [Crowds; 3] = [Crowds::Off, Crowds::Some, Crowds::Lots];

    pub fn label(self) -> &'static str {
        match self {
            Crowds::Off => "Off",
            Crowds::Some => "Some",
            Crowds::Lots => "Lots",
        }
    }

    pub fn factor(self) -> f32 {
        match self {
            Crowds::Off => 0.0,
            Crowds::Some => 0.4,
            Crowds::Lots => 1.0,
        }
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub name: String,
    /// 0..=1.
    pub volume: f32,
    pub muted: bool,
    pub mouse_sensitivity: f32,
    pub invert_y: bool,
    pub shadows: bool,
    pub day_cycle: DayCycle,
    /// Cars and pedestrians.
    pub crowds: Crowds,
    pub ui_scale: f32,
    /// Write `autosave.ron` every game year.
    pub autosave: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: std::env::var("USER").unwrap_or_else(|_| "Mayor".into()),
            volume: 0.7,
            muted: false,
            mouse_sensitivity: 1.0,
            invert_y: false,
            shadows: true,
            day_cycle: DayCycle::Normal,
            crowds: Crowds::Lots,
            ui_scale: 1.0,
            autosave: true,
        }
    }
}

fn path() -> String {
    format!("{SAVE_DIR}/settings.ron")
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(path()).ok().and_then(|t| ron::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        std::fs::create_dir_all(SAVE_DIR).map_err(|e| e.to_string())?;
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).map_err(|e| e.to_string())?;
        std::fs::write(path(), text).map_err(|e| e.to_string())
    }
}
