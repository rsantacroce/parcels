//! Stage 6: taxes in, upkeep out, utility trades paid, treasuries clamped.

use crate::map::{Terrain, TileKind};
use crate::state::GameState;

pub fn run(state: &mut GameState) {
    let np = state.players.len();
    let mut taxable = vec![0i64; np];
    let mut upkeep = vec![0i64; np];
    let mut lv_total = vec![0u64; np];
    let u = &state.config.upkeep;
    for i in 0..state.map.len() {
        let Some(o) = state.owner_of_idx(i) else { continue };
        let o = o.index();
        let t = &state.map.tiles[i];
        lv_total[o] += t.land_value as u64;
        // Only occupied zones that are actually served pay tax.
        if t.kind.is_zone() && t.powered {
            let value = if t.kind == TileKind::Industrial { state.config.land_value_base } else { t.land_value };
            taxable[o] += state.occupants(i) as i64 * value as i64;
        }
        upkeep[o] += match t.kind {
            TileKind::Empty => 0,
            TileKind::Road if t.terrain == Terrain::Water => u.road * 2,
            TileKind::Road => u.road,
            TileKind::PowerPlant => u.power_plant,
            TileKind::WaterPump => u.water_pump,
            TileKind::Park => u.park,
            TileKind::Residential => u.residential,
            TileKind::Commercial => u.commercial,
            TileKind::Industrial => u.industrial,
        };
        if t.wire {
            upkeep[o] += u.power_line;
        }
        if t.pipe {
            upkeep[o] += u.water_pipe;
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
