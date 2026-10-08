//! Stage 3: RCI demand.
//!
//! Workers, jobs and shoppers move freely across the map, so the raw balance is
//! map wide. Each player then sees that demand shifted by their own tax rate:
//! a high-tax neighbourhood is simply less attractive than the one next door.

use crate::map::Zone;
use crate::state::GameState;

pub fn run(state: &mut GameState) {
    let np = state.players.len();
    let mut pop = vec![0u32; np];
    let mut com = vec![0u32; np];
    let mut ind = vec![0u32; np];
    let mut developed = vec![0u32; np];
    for i in 0..state.map.len() {
        let Some(o) = state.owner_of_idx(i) else { continue };
        let t = &state.map.tiles[i];
        if t.kind != crate::map::TileKind::Empty {
            developed[o.index()] += 1;
        }
        let occ = state.occupants(i);
        match t.kind.zone() {
            Some(Zone::Residential) => pop[o.index()] += occ,
            Some(Zone::Commercial) => com[o.index()] += occ,
            Some(Zone::Industrial) => ind[o.index()] += occ,
            None => {}
        }
    }
    let c = &state.config;
    let total_pop: u32 = pop.iter().sum();
    let total_com: u32 = com.iter().sum();
    let total_ind: u32 = ind.iter().sum();
    let (tp, tc, ti) = (total_pop as i32, total_com as i32, total_ind as i32);

    // The wider region keeps growing, so a saturated map still has some pull.
    let years = (state.tick / (c.ticks_per_month as u64 * 12)) as i32;
    let region = 100 + years * c.region_growth_percent_per_year;
    let (br, bc, bi) = (
        c.base_residential_demand * region / 100,
        c.base_commercial_demand * region / 100,
        c.base_industrial_demand * region / 100,
    );
    let raw_r = br + (tc + ti) * c.workers_per_100_jobs / 100 - tp;
    let raw_c = bc + tp * 100 / c.residents_per_100_commercial_jobs.max(1) - tc;
    let raw_i = bi + tp * 100 / c.residents_per_100_industrial_jobs.max(1) - ti;
    let raw = [raw_r, raw_c, raw_i];
    // Normalise against current size so a tiny town and a big one read alike.
    let scale = [tp + 100, tc + 50, ti + 50];
    let mut norm = [0i32; 3];
    for z in 0..3 {
        norm[z] = (raw[z] * 1000 / scale[z]).clamp(-1000, 1000);
    }

    let neutral = c.neutral_tax_rate as i32;
    let penalty = c.tax_demand_penalty;
    for (p, player) in state.players.iter_mut().enumerate() {
        let tax_shift = (player.tax_rate as i32 - neutral) * penalty * 10;
        let s = &mut player.stats;
        s.population = pop[p];
        s.commercial_jobs = com[p];
        s.industrial_jobs = ind[p];
        s.developed_tiles = developed[p];
        for z in 0..3 {
            s.demand[z] = (norm[z] - tax_shift).clamp(-1000, 1000);
        }
    }

    let g = &mut state.global;
    g.population = total_pop;
    g.commercial_jobs = total_com;
    g.industrial_jobs = total_ind;
    g.raw_demand = raw;
    g.demand = norm;
}
