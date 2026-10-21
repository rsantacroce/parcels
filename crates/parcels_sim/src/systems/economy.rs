//! Stage 7: taxes, fares and trade in, upkeep out, treasury clamped.

use crate::map::{Road, Terrain, TileKind, Zone};
use crate::state::GameState;

pub fn run(state: &mut GameState) {
    let mut taxable = 0i64;
    let mut upkeep = 0i64;
    let mut lv_total = 0u64;
    let c = &state.config;
    for i in 0..state.map.len() {
        let t = &state.map.tiles[i];
        lv_total += t.land_value as u64;
        // Only occupied zones that are actually served pay tax.
        if t.kind.is_zone() && t.powered {
            let value = if t.kind.zone() == Some(Zone::Industrial) { c.land_value_base } else { t.land_value };
            taxable += state.occupants(i) as i64 * value as i64;
        }
        let road = match t.kind {
            TileKind::Road(Road::Street) => c.street_upkeep,
            TileKind::Road(Road::Avenue) => c.avenue_upkeep,
            _ => 0,
        };
        // Bridges cost double to maintain.
        upkeep += if t.terrain == Terrain::Water { road * 2 } else { road };
        if let TileKind::Building(b) = t.kind {
            if t.is_anchor() {
                upkeep += c.building(b).upkeep;
            }
        }
        if t.wire {
            upkeep += c.power_line_upkeep;
        }
        if t.pipe {
            upkeep += c.water_pipe_upkeep;
        }
    }

    let floor = state.config.debt_floor;
    let divisor = state.config.tax_divisor.max(1);
    let city = &mut state.city;
    // Bus fares count as income alongside taxes.
    let taxes = taxable * city.tax_rate as i64 / divisor + city.stats.fares;
    // Power and water traded with neighbours.
    let s = &mut city.stats;
    let bought = (s.power.imported + s.water.imported) as i64;
    let sold = (s.power.exported + s.water.exported) as i64;
    s.trade = (sold * state.config.export_price - bought * state.config.import_price) / 100;
    let trade = s.trade;
    city.stats.taxes = taxes;
    city.stats.upkeep = upkeep;
    city.stats.land_value_total = lv_total;
    // Never below the debt floor: you can't exploit bankruptcy into infinite
    // negative money, and building already requires cash in hand.
    city.treasury = (city.treasury + taxes + trade - upkeep).max(floor);
}

/// Total land value, for the first frame before any tick has run.
pub fn refresh_land_value(state: &mut GameState) {
    state.city.stats.land_value_total = state.map.tiles.iter().map(|t| t.land_value as u64).sum();
}
