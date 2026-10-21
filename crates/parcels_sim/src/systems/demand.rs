//! Stage 3: R/C/I/O demand.
//!
//! The raw balance of residents and jobs, shifted by the tax rate: high taxes
//! make the city less attractive to newcomers, and so does an unpopular mayor.

use crate::map::{TileKind, Zone};
use crate::state::GameState;

pub fn run(state: &mut GameState) {
    // Occupants per zone, plus public-sector jobs.
    let mut occ = [0u32; Zone::COUNT];
    let mut public = 0u32;
    let mut developed = 0u32;
    let mut bonus = [0i32; Zone::COUNT];
    for i in 0..state.map.len() {
        let t = &state.map.tiles[i];
        if t.kind != TileKind::Empty {
            developed += 1;
        }
        match t.kind {
            TileKind::Zone(z, _) => occ[z.index()] += state.occupants(i),
            TileKind::Building(b) if t.is_anchor() => {
                let s = state.config.building(b);
                if t.powered || s.power_draw == 0 {
                    public += s.jobs as u32;
                    for z in 0..Zone::COUNT {
                        bonus[z] += s.demand_bonus[z];
                    }
                }
            }
            _ => {}
        }
    }
    let c = &state.config;
    let total = |z: Zone| occ[z.index()] as i32;
    let (tp, tc, ti, to) = (total(Zone::Residential), total(Zone::Commercial), total(Zone::Industrial), total(Zone::Office));
    let tpub = public as i32;

    // The wider region keeps growing, so a saturated city still has some pull.
    let years = (state.tick / (c.ticks_per_month as u64 * 12)) as i32;
    let region = 100 + years * c.region_growth_percent_per_year;
    let base = [c.base_residential_demand, c.base_commercial_demand, c.base_industrial_demand, c.base_office_demand]
        .map(|b| b * region / 100);
    let raw = [
        base[0] + bonus[0] + (tc + ti + to + tpub) * c.workers_per_100_jobs / 100 - tp,
        base[1] + bonus[1] + tp * 100 / c.residents_per_100_commercial_jobs.max(1) - tc,
        base[2] + bonus[2] + tp * 100 / c.residents_per_100_industrial_jobs.max(1) - ti,
        base[3] + bonus[3] + tp * 100 / c.residents_per_100_office_jobs.max(1) - to,
    ];
    // Neighbours reachable by road add their pull.
    let regional = crate::region::demand(state);
    let raw = [0, 1, 2, 3].map(|z| raw[z] + regional[z]);
    // Normalise against current size so a tiny town and a big one read alike.
    let scale = [tp + 100, tc + 50, ti + 50, to + 50];
    let mut norm = [0i32; Zone::COUNT];
    for z in 0..Zone::COUNT {
        norm[z] = (raw[z] * 1000 / scale[z]).clamp(-1000, 1000);
    }

    let neutral = c.neutral_tax_rate as i32;
    let tax_shift = (state.city.tax_rate as i32 - neutral) * c.tax_demand_penalty * 10;
    // A popular mayor draws people in; an unpopular one drives them off.
    let mood_shift = (state.city.stats.approval as i32 - 500) * c.approval_demand_weight / 100;
    let s = &mut state.city.stats;
    s.population = tp as u32;
    s.commercial_jobs = tc as u32;
    s.industrial_jobs = ti as u32;
    s.office_jobs = to as u32;
    s.public_jobs = public;
    s.developed_tiles = developed;
    s.raw_demand = raw;
    for z in 0..Zone::COUNT {
        let mood = if z == Zone::Residential.index() { mood_shift } else { 0 };
        s.demand[z] = (norm[z] - tax_shift + mood).clamp(-1000, 1000);
    }
}
