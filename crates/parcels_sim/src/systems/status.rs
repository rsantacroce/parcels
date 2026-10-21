//! Stage 7: how residents feel. Each factor is scored from what the homes
//! around the city actually experience, weighted by how many people live there,
//! and the approval rating drifts toward the total.

use crate::catalog::Service;
use crate::city::Factor;
use crate::map::Zone;
use crate::state::GameState;

pub fn run(state: &mut GameState) {
    let c = &state.config;
    let s = &state.city.stats;
    let pop = s.population as i64;

    // Averages over residents: crime, pollution, land value, nearby traffic and
    // service coverage where people live.
    let (mut crime, mut pollution, mut land, mut traffic, mut served, mut people) = (0i64, 0i64, 0i64, 0i64, 0i64, 0i64);
    for i in 0..state.map.len() {
        let t = &state.map.tiles[i];
        if t.kind.zone() != Some(Zone::Residential) || t.level == 0 {
            continue;
        }
        let n = state.occupants(i) as i64;
        people += n;
        crime += t.crime as i64 * n;
        pollution += t.pollution as i64 * n;
        land += t.land_value as i64 * n;
        let jam = state.map.neighbors4(i).map(|j| state.map.tiles[j].traffic).max().unwrap_or(0);
        traffic += jam as i64 * n;
        let covered = [Service::Education, Service::Health].iter().filter(|&&sv| t.covered(sv)).count() as i64;
        served += covered * n;
    }
    let avg = |v: i64| if people > 0 { v / people } else { 0 };

    let jobs = (s.commercial_jobs + s.industrial_jobs + s.office_jobs + s.public_jobs) as i64;
    let workers_wanted = jobs * c.workers_per_100_jobs as i64 / 100;
    let jobless = if pop > 0 { ((pop - workers_wanted).max(0) * 100 / pop).min(100) } else { 0 };

    let need = (s.power.demand + s.water.demand) as i64;
    let short = (s.power.unserved + s.water.unserved) as i64;
    let utilities = if need > 0 { 5 - short * 25 / need } else { 0 };

    let mut mood = [0i32; Factor::COUNT];
    let mut set = |f: Factor, v: i64, lo: i64, hi: i64| mood[f.index()] = v.clamp(lo, hi) as i32;
    if people > 0 {
        set(Factor::Jobs, 10 - jobless / 2, -20, 10);
        set(Factor::Utilities, utilities, -20, 5);
        set(Factor::Safety, 8 - avg(crime) / 5, -20, 8);
        set(Factor::Environment, 8 - avg(pollution) / 6, -20, 8);
        // Half the people covered by both a school and a clinic reads as neutral.
        set(Factor::Services, avg(served * 10) - 10, -10, 10);
        set(Factor::Traffic, 5 - avg(traffic) / 4, -15, 5);
        set(Factor::Neighbourhoods, (avg(land) - c.land_value_base as i64) / 20, -10, 10);
    }
    set(Factor::Taxes, (c.neutral_tax_rate as i64 - state.city.tax_rate as i64) * 3, -20, 10);
    set(Factor::Budget, if state.city.treasury < 0 { -10 } else { 0 }, -10, 0);

    let target = ((500 + mood.iter().sum::<i32>() * 10).clamp(0, 1000)) as i32;
    let s = &mut state.city.stats;
    let cur = s.approval as i32;
    let diff = target - cur;
    // Close a fortieth of the gap each tick, at least a tenth of a point.
    let step = if diff == 0 { 0 } else if diff.abs() < 40 { diff.signum() } else { diff / 40 };
    s.approval = (cur + step) as u16;
    s.jobless_percent = jobless as u8;
    s.mood = mood;
}
