//! Parcels simulation: deterministic, tick-based, integer-only, Bevy-free.
//!
//! Determinism rules for this crate:
//! - no floating point in any state or computation;
//! - no wall-clock time, no unseeded randomness (see [`rng`]);
//! - no `HashMap`/`HashSet` iteration (use `Vec`, `BTreeMap`, or sorted output).

pub mod ai;
pub mod catalog;
pub mod command;
pub mod config;
pub mod hash;
pub mod ids;
pub mod map;
pub mod player;
pub mod replay;
pub mod rng;
pub mod session;
pub mod state;
pub mod systems;

pub use catalog::{Building, BuildingStats, Category, Service};
pub use command::{Area, Command, CommandKind, Rejection};
pub use config::Config;
pub use ids::{ParcelId, PlayerId};
pub use map::{Buildable, Density, Map, Pos, Rect, Road, Terrain, TerrainSettings, Tile, TileKind, Zone};
pub use player::{AiStrategy, Controller, Parcel, Player, PlayerStats, Utility};
pub use replay::Replay;
pub use session::Session;
pub use state::{GameState, NewGame, PlayerSetup, TickReport};

/// Format cents as dollars, e.g. `-$1,234.50`.
pub fn fmt_money(cents: i64) -> String {
    let neg = cents < 0;
    let c = cents.unsigned_abs();
    let dollars = (c / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in dollars.chars().enumerate() {
        if i > 0 && (dollars.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}${}.{:02}", if neg { "-" } else { "" }, grouped, c % 100)
}

#[cfg(test)]
mod tests {
    #[test]
    fn money_format() {
        assert_eq!(super::fmt_money(123456789), "$1,234,567.89");
        assert_eq!(super::fmt_money(-5), "-$0.05");
        assert_eq!(super::fmt_money(100000), "$1,000.00");
    }
}
