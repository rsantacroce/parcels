//! Stage 3: R/C/I/O demand.
//!
//! Workers, jobs and shoppers move freely across the map, so the raw balance is
//! map wide. Each player then sees that demand shifted by their own tax rate:
//! a high-tax neighbourhood is simply less attractive than the one next door.

use crate::map::{TileKind, Zone};
use crate::state::GameState;

pub fn run(state: &mut GameState) {
    let np = state.players.len();
    // Occupants per player per zone, plus public-sector jobs.
    let mut occ = vec![[0u32; Zone::COUNT]; np];
    let mut public = vec![0u32; np];
    let mut developed = vec![0u32; np];
    let mut bonus = [0i32; Zone::COUNT];
    for i in 0..state.map.len() {
        let Some(o) = state.owner_of_idx(i) else { continue };
        let t = &state.map.tiles[i];
        if t.kind != TileKind::Empty {
            developed[o.index()] += 1;
        }
        match t.kind {
            TileKind::Zone(z, _) => occ[o.index()][z.index()] += state.occupants(i),
            TileKind::Building(b) if t.is_anchor() => {
                let s = state.config.building(b);
                if t.powered || s.power_draw == 0 {
                    public[o.index()] += s.jobs as u32;
                    for z in 0..Zone::COUNT {
                        bonus[z] += s.demand_bonus[z];
                    }
                }
            }
            _ => {}
        }
    }
    let c = &state.config;
    let total = |z: Zone| occ.iter().map(|o| o[z.index()]).sum::<u32>() as i32;
    let (tp, tc, ti, to) = (total(Zone::Residential), total(Zone::Commercial), total(Zone::Industrial), total(Zone::Office));
    let tpub: i32 = public.iter().sum::<u32>() as i32;

    // The wider region keeps growing, so a saturated map still has some pull.
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
    // Normalise against current size so a tiny town and a big one read alike.
    let scale = [tp + 100, tc + 50, ti + 50, to + 50];
    let mut norm = [0i32; Zone::COUNT];
    for z in 0..Zone::COUNT {
        norm[z] = (raw[z] * 1000 / scale[z]).clamp(-1000, 1000);
    }

    let neutral = c.neutral_tax_rate as i32;
    let penalty = c.tax_demand_penalty;
    for (p, player) in state.players.iter_mut().enumerate() {
        let tax_shift = (player.tax_rate as i32 - neutral) * penalty * 10;
        let s = &mut player.stats;
        s.population = occ[p][Zone::Residential.index()];
        s.commercial_jobs = occ[p][Zone::Commercial.index()];
        s.industrial_jobs = occ[p][Zone::Industrial.index()];
        s.office_jobs = occ[p][Zone::Office.index()];
        s.public_jobs = public[p];
        s.developed_tiles = developed[p];
        for z in 0..Zone::COUNT {
            s.demand[z] = (norm[z] - tax_shift).clamp(-1000, 1000);
        }
    }

    let g = &mut state.global;
    g.population = tp as u32;
    g.commercial_jobs = tc as u32;
    g.industrial_jobs = ti as u32;
    g.office_jobs = to as u32;
    g.public_jobs = tpub as u32;
    g.raw_demand = raw;
    g.demand = norm;
}
