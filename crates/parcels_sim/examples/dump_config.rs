//! Print the default balance config as RON: `cargo run -p parcels_sim --example dump_config > config/balance.ron`
fn main() {
    println!("// Parcels balance constants. Money is in cents; land value is 0..=1000.");
    println!("// Edit and restart (new games only: saves carry their own copy).");
    println!("{}", parcels_sim::Config::default().to_ron());
}
