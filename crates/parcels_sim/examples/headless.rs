//! Run an all-AI game with no graphics and print a yearly summary.
//! `cargo run -p parcels_sim --example headless --release -- [seed] [players]`

use parcels_sim::{fmt_money, AiStrategy, Config, Controller, GameState, NewGame, PlayerSetup, Session};

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(7);
    let n: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(4);
    let config = std::fs::read_to_string("config/balance.ron")
        .ok()
        .and_then(|t| Config::from_ron(&t).ok())
        .unwrap_or_default();
    let players = (0..n)
        .map(|i| PlayerSetup {
            name: parcels_sim::state::ai_name(i),
            controller: Controller::Ai(if i % 2 == 0 { AiStrategy::UtilityBaron } else { AiStrategy::Developer }),
        })
        .collect();
    let mut s = Session::new(GameState::new(&NewGame { seed, config, players }));
    let tpy = s.state.config.ticks_per_month as u64 * 12;
    while !s.state.is_over() {
        s.tick();
        if s.state.tick % tpy == 0 || s.state.tick == 60 {
            println!("--- year {} (tick {}) pop {} jobs C{} I{} demand {:?}", s.state.tick / tpy, s.state.tick,
                s.state.global.population, s.state.global.commercial_jobs, s.state.global.industrial_jobs, s.state.global.demand);
            for p in &s.state.players {
                let st = &p.stats;
                println!(
                    "  {:10} {:>12} tax{:>2}% pop{:>5} C{:>4} I{:>4} +{}/-{}/{:+} pw {}/{} b{} s{} un{} wt {}/{} b{} s{} un{} lv{} score {}",
                    p.name, fmt_money(p.treasury), p.tax_rate, st.population, st.commercial_jobs, st.industrial_jobs,
                    st.taxes, st.upkeep, st.trade, st.power.supply, st.power.demand, st.power.bought, st.power.sold, st.power.unserved,
                    st.water.supply, st.water.demand, st.water.bought, st.water.sold, st.water.unserved,
                    st.land_value_total / (s.state.parcels[0].rect.area() as u64), fmt_money(st.score)
                );
            }
        }
    }
    diag(&s.state);
    // Optional: save a mid-game snapshot with player 0 handed to a human, for
    // eyeballing the client: `--save-at <tick> <path>` via env vars.
    if let (Ok(at), Ok(path)) = (std::env::var("SAVE_AT"), std::env::var("SAVE_TO")) {
        let at: u64 = at.parse().unwrap();
        let mut st = s.replay.run_until(at);
        st.players[0].controller = Controller::Human;
        st.players[0].name = "You".into();
        std::fs::write(&path, st.to_ron()).unwrap();
        println!("saved tick {at} to {path}");
    }
    if let Ok(path) = std::env::var("REPLAY_TO") {
        std::fs::write(&path, ron::ser::to_string(&s.replay).unwrap()).unwrap();
    }
    println!("final hash {:016x}", s.state.hash());
    println!("ranking {:?}", s.state.outcome.unwrap().ranking);
}

#[allow(dead_code)]
pub fn diag(s: &GameState) {
    use parcels_sim::{TileKind, Zone};
    for z in Zone::ALL {
        let mut levels = [0u32; 8];
        let (mut unp, mut unw) = (0, 0);
        for t in &s.map.tiles {
            if t.kind == z.kind() {
                levels[t.level as usize] += 1;
                if !t.powered { unp += 1 }
                if !t.watered { unw += 1 }
            }
        }
        println!("{z:?}: levels {:?} unpowered {unp} unwatered {unw}", &levels[..5]);
    }
    let empty_frontage = s.map.tiles.iter().filter(|t| t.kind == TileKind::Empty).count();
    println!("empty tiles {empty_frontage}, roads {}", s.map.tiles.iter().filter(|t| t.kind == TileKind::Road).count());
    // ascii map
    for y in 0..s.map.height {
        let row: String = (0..s.map.width).map(|x| {
            let t = s.map.tile(parcels_sim::Pos::new(x, y));
            match t.kind {
                TileKind::Empty if t.terrain == parcels_sim::Terrain::Water => '~',
                TileKind::Empty => if t.wire {'+'} else {'.'},
                TileKind::Road => '#',
                TileKind::PowerPlant => 'P',
                TileKind::WaterPump => 'W',
                TileKind::Park => 'T',
                TileKind::Residential => if t.powered {(b'0' + t.level) as char} else {'r'},
                TileKind::Commercial => if t.powered {(b'a' + t.level) as char} else {'c'},
                TileKind::Industrial => if t.powered {(b'A' + t.level) as char} else {'i'},
            }
        }).collect();
        println!("{row}");
    }
}
