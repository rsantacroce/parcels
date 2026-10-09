//! Dedicated headless host. No graphics, no Bevy: just the simulation and the
//! command relay.
//!
//! parcels-server [--port 5757] [--slots 4] [--wait 1] [--seed N] [--tps 4]
//!                [--map 128x96] [--config config/balance.ron] [--load save.ron]
//!
//! Waits in the lobby until `--wait` players have connected, then starts; empty
//! seats are run by the AI and later joiners take them over.

use std::time::{Duration, Instant};

use parcels_net::{HostServer, ServerOptions, DEFAULT_PORT};
use parcels_sim::{Config, GameState, Map};

fn arg<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok())
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let port: u16 = arg(&args, "--port").unwrap_or(DEFAULT_PORT);
    let slots: u8 = arg::<u8>(&args, "--slots").unwrap_or(4).clamp(1, 8);
    let wait: usize = arg(&args, "--wait").unwrap_or(1);
    let tps: u32 = arg::<u32>(&args, "--tps").unwrap_or(4).max(1);
    let seed: u64 = arg(&args, "--seed").unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
    });
    let config_path: String = arg(&args, "--config").unwrap_or_else(|| "config/balance.ron".into());
    let mut config = match std::fs::read_to_string(&config_path) {
        Ok(t) => Config::from_ron(&t).map_err(std::io::Error::other)?,
        Err(_) => Config::default(),
    };
    if let Some(size) = arg::<String>(&args, "--map") {
        let (w, h) = size.split_once('x').ok_or_else(|| std::io::Error::other("--map wants WxH, e.g. 128x96"))?;
        let dim = |v: &str| v.parse::<u16>().map(|n| n.clamp(Map::MIN_SIZE, Map::MAX_SIZE)).map_err(std::io::Error::other);
        config.map_width = dim(w)?;
        config.map_height = dim(h)?;
    }

    let mut host = HostServer::bind(ServerOptions {
        bind: format!("0.0.0.0:{port}").parse().unwrap(),
        slots,
        seed,
        config,
        terrain: Default::default(),
        host_name: None,
    })?;
    if let Some(path) = arg::<String>(&args, "--load") {
        let text = std::fs::read_to_string(&path)?;
        host.load(GameState::from_ron(&text).map_err(std::io::Error::other)?);
    } else {
        println!("Lobby open: waiting for {wait} player(s); {slots} parcels, seed {seed}.");
    }

    let tick_len = Duration::from_secs(1) / tps;
    let mut last = Instant::now();
    let mut next_tick = Instant::now();
    loop {
        let now = Instant::now();
        host.poll(now - last);
        last = now;
        if !host.started() && host.connected() >= wait {
            host.start();
            next_tick = Instant::now();
        }
        if host.started() {
            while Instant::now() >= next_tick {
                if let Some(r) = host.tick() {
                    if r.month_ended {
                        let s = host.state().unwrap();
                        let (y, m, _) = s.date();
                        let summary: Vec<String> = s
                            .players
                            .iter()
                            .map(|p| format!("{}: {} pop {}", p.name, parcels_sim::fmt_money(p.treasury), p.stats.population))
                            .collect();
                        println!("Y{y} M{m:02} | {}", summary.join(" | "));
                    }
                    if r.game_over {
                        let s = host.state().unwrap();
                        println!("Game over. Ranking:");
                        for (i, (p, score)) in s.outcome.as_ref().unwrap().ranking.iter().enumerate() {
                            println!("  {}. {} {}", i + 1, s.players[p.index()].name, parcels_sim::fmt_money(*score));
                        }
                    }
                }
                next_tick += tick_len;
            }
        }
        host.flush();
        std::thread::sleep(Duration::from_millis(5));
    }
}
