//! Export every buildable item and balance constant as JSON for the website's
//! Parcelspedia, straight from `config/balance.ron`, so the numbers there are
//! the numbers the game plays with.
//!
//! `cargo run -p parcels_sim --example pedia --release > ../parcels-site/pedia.json`

use parcels_sim::{Building, Config, Zone};
use serde_json::{json, Value};

fn main() {
    let config = std::fs::read_to_string("config/balance.ron")
        .ok()
        .and_then(|t| Config::from_ron(&t).ok())
        .unwrap_or_default();

    let buildings: Vec<Value> = Building::ALL
        .iter()
        .map(|&b| {
            json!({
                "id": format!("{b:?}"),
                "name": b.name(),
                "category": b.category().name(),
                "size": b.size(),
                "blurb": b.blurb(),
                "service": b.service().map(|s| s.name()),
                "conducts": b.conducts(),
                "stats": config.building(b),
            })
        })
        .collect();

    let zones: Vec<Value> = Zone::ALL
        .iter()
        .map(|&z| json!({ "id": format!("{z:?}"), "name": z.name(), "letter": z.letter(), "per_level": config.per_level(z) }))
        .collect();

    // The building table is exported above with names; drop it from the raw config.
    let mut raw = serde_json::to_value(Config { buildings: Default::default(), ..config.clone() }).unwrap();
    raw.as_object_mut().unwrap().remove("buildings");

    let out = json!({
        "game_version": env!("CARGO_PKG_VERSION"),
        "config": raw,
        "zones": zones,
        "buildings": buildings,
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
