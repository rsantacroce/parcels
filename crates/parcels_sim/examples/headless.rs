//! Let autoplay build a city with no graphics and print a yearly summary.
//! `cargo run -p parcels_sim --example headless --release -- [seed] [years] [width height]`

use parcels_sim::{fmt_money, Config, GameState, NewGame, Session};

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(7);
    let years: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);
    let mut config = std::fs::read_to_string("config/balance.ron")
        .ok()
        .and_then(|t| Config::from_ron(&t).ok())
        .unwrap_or_default();
    if let (Some(w), Some(h)) = (args.next().and_then(|s| s.parse().ok()), args.next().and_then(|s| s.parse().ok())) {
        config.map_width = w;
        config.map_height = h;
    }
    let mut s = Session::new(GameState::new(&NewGame::new(seed, config, "Autoville")));
    s.autoplay = true;
    let tpy = s.state.config.ticks_per_month as u64 * 12;
    let trace = std::env::var("AI_TRACE").is_ok();
    while s.state.tick < years * tpy {
        let r = s.tick();
        let c = &s.state.city;
        if trace {
            for cmd in &r.applied {
                let what = match &cmd.kind {
                    parcels_sim::CommandKind::Place { area, what } => format!("place {} x{}", what.name(), area.tiles().len()),
                    k => format!("{k:?}"),
                };
                let st = &c.stats;
                println!("t{:>5} {:>12} pop{:>5} pw {}/{} un{} | {what}", cmd.tick, fmt_money(c.treasury), st.population, st.power.supply, st.power.demand, st.power.unserved);
            }
        }
        if s.state.tick % tpy == 0 || s.state.tick == 60 {
            let st = &c.stats;
            println!(
                "year {:>2} {:>12} tax{:>2}% pop{:>5} C{:>4} I{:>4} O{:>4} +{}/-{} pw {}/{} un{} wt {}/{} un{} lv{} demand {:?}",
                s.state.tick / tpy, fmt_money(c.treasury), c.tax_rate, st.population, st.commercial_jobs, st.industrial_jobs,
                st.office_jobs, st.taxes, st.upkeep, st.power.supply, st.power.demand, st.power.unserved,
                st.water.supply, st.water.demand, st.water.unserved,
                st.land_value_total / s.state.map.len() as u64, st.demand
            );
        }
    }
    diag(&s.state);
    // Optional: save a mid-game snapshot for eyeballing the client.
    if let (Ok(at), Ok(path)) = (std::env::var("SAVE_AT"), std::env::var("SAVE_TO")) {
        let at: u64 = at.parse().unwrap();
        std::fs::write(&path, s.replay.run_until(at).to_ron()).unwrap();
        println!("saved tick {at} to {path}");
    }
    if let Ok(path) = std::env::var("REPLAY_TO") {
        std::fs::write(&path, ron::ser::to_string(&s.replay).unwrap()).unwrap();
    }
    println!("final hash {:016x}", s.state.hash());
}

#[allow(dead_code)]
pub fn diag(s: &GameState) {
    use parcels_sim::{Category, Terrain, TileKind, Zone};
    for z in Zone::ALL {
        let mut levels = [0u32; 8];
        let (mut unp, mut unw) = (0, 0);
        for t in &s.map.tiles {
            if t.kind.zone() == Some(z) {
                levels[t.level as usize] += 1;
                if !t.powered { unp += 1 }
                if !t.watered { unw += 1 }
            }
        }
        println!("{z:?}: levels {:?} unpowered {unp} unwatered {unw}", &levels[..7]);
    }
    let mut built = std::collections::BTreeMap::new();
    for t in s.map.tiles.iter().filter(|t| t.is_anchor()) {
        if let TileKind::Building(b) = t.kind {
            *built.entry(b.name()).or_insert(0) += 1;
        }
    }
    println!("buildings {built:?}");
    let empty = s.map.tiles.iter().filter(|t| t.kind == TileKind::Empty).count();
    println!("empty tiles {empty}, roads {}", s.map.tiles.iter().filter(|t| t.kind.is_road()).count());
    for y in 0..s.map.height {
        let row: String = (0..s.map.width).map(|x| {
            let t = s.map.tile(parcels_sim::Pos::new(x, y));
            match t.kind {
                TileKind::Empty => match t.terrain {
                    Terrain::Water => '~',
                    Terrain::Forest => '^',
                    Terrain::Land => if t.wire { '+' } else { '.' },
                },
                TileKind::Road(_) => '#',
                TileKind::Building(b) => match b.category() {
                    Category::Power => 'P',
                    Category::Water => 'W',
                    Category::Services => 'S',
                    Category::Parks => 'T',
                    _ => 'L',
                },
                TileKind::Zone(z, _) => {
                    let base = [b'0', b'a', b'A', b'o'][z.index()];
                    if t.powered { (base + t.level) as char } else { ['r', 'c', 'i', 'o'][z.index()] }
                }
            }
        }).collect();
        println!("{row}");
    }
}
