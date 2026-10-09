//! Stage 6: taxes in, upkeep out, utility trades paid, treasuries clamped.

use crate::map::{Road, Terrain, TileKind, Zone};
use crate::state::GameState;

pub fn run(state: &mut GameState) {
    let np = state.players.len();
    let mut taxable = vec![0i64; np];
    let mut upkeep = vec![0i64; np];
    let mut lv_total = vec![0u64; np];
    let c = &state.config;
    for i in 0..state.map.len() {
        let Some(o) = state.owner_of_idx(i) else { continue };
        let o = o.index();
        let t = &state.map.tiles[i];
        lv_total[o] += t.land_value as u64;
        // Only occupied zones that are actually served pay tax.
        if t.kind.is_zone() && t.powered {
            let value = if t.kind.zone() == Some(Zone::Industrial) { c.land_value_base } else { t.land_value };
            taxable[o] += state.occupants(i) as i64 * value as i64;
        }
        let road = match t.kind {
            TileKind::Road(Road::Street) => c.street_upkeep,
            TileKind::Road(Road::Avenue) => c.avenue_upkeep,
            _ => 0,
        };
        // Bridges cost double to maintain.
        upkeep[o] += if t.terrain == Terrain::Water { road * 2 } else { road };
        if let TileKind::Building(b) = t.kind {
            if t.is_anchor() {
                upkeep[o] += c.building(b).upkeep;
            }
        }
        if t.wire {
            upkeep[o] += c.power_line_upkeep;
        }
        if t.pipe {
            upkeep[o] += c.water_pipe_upkeep;
        }
    }

    let mut trade = vec![0i64; np];
    for t in &state.trades {
        let amount = t.units as i64 * t.price as i64;
        trade[t.buyer.index()] -= amount;
        trade[t.seller.index()] += amount;
    }

    let floor = state.config.debt_floor;
    let divisor = state.config.tax_divisor.max(1);
    for (p, player) in state.players.iter_mut().enumerate() {
        let taxes = taxable[p] * player.tax_rate as i64 / divisor;
        let s = &mut player.stats;
        s.taxes = taxes;
        s.upkeep = upkeep[p];
        s.trade = trade[p];
        s.land_value_total = lv_total[p];
        // Never below the debt floor: you can't exploit bankruptcy into infinite
        // negative money, and building already requires cash in hand.
        player.treasury = (player.treasury + taxes - upkeep[p] + trade[p]).max(floor);
    }
    refresh_scores(state);
}

/// Competitive score: cash plus the value of the land you hold.
pub fn refresh_scores(state: &mut GameState) {
    let factor = state.config.score_land_value_factor;
    if state.players.iter().all(|p| p.stats.land_value_total == 0) {
        let np = state.players.len();
        let mut lv = vec![0u64; np];
        for i in 0..state.map.len() {
            if let Some(o) = state.owner_of_idx(i) {
                lv[o.index()] += state.map.tiles[i].land_value as u64;
            }
        }
        for (p, player) in state.players.iter_mut().enumerate() {
            player.stats.land_value_total = lv[p];
        }
    }
    for p in &mut state.players {
        p.stats.score = p.treasury + p.stats.land_value_total as i64 * factor;
    }
}
