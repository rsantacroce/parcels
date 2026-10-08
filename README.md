# Parcels

A co-op/competitive neighbourhood city builder in Rust + Bevy 0.19.1. One shared,
deterministic simulation; the map is split into parcels, each with its own owner
and treasury. Roads, power and water cross borders, so neighbours trade and
affect one another's land value.

## Run

```sh
cargo run --release --bin parcels                    # main menu
cargo run --release --bin parcels -- --solo --ai 3   # you vs 3 AI neighbours
cargo run --release --bin parcels -- --host --slots 4 --name Rob
cargo run --release --bin parcels -- --join 192.168.1.20:5757 --name Ana
cargo run --release --bin parcels-server -- --slots 4 --wait 2   # headless dedicated host
cargo run --release --bin parcels -- --verify-replay saves/replay.ron
cargo test --workspace --release
```

**Controls:** WASD/arrows/screen edge/middle-drag to pan, wheel to zoom. Tools:
R road, Z/X/C residential/commercial/industrial, L power line, P pipe, G power
plant, U pump, K park, B bulldoze, Esc inspect. Drag to paint; right-click
cancels. 1–8 switch overlays (land value, power, water, density, pollution,
traffic, owners). Space pauses, +/- change speed, Tab switches between your
parcels in hot-seat, F5/F9 save/load, M mutes sound.

**Winning:** the highest score (treasury plus land value of the parcels you own)
when the time limit runs out (10 game years by default).

## Layout

| Crate | What it does |
| --- | --- |
| `parcels_sim` | Headless simulation. No Bevy, no floats, no wall clock, no hash-map iteration. `GameState`, `Command`, the six-stage tick pipeline, AI, replays. |
| `parcels_net` | Lockstep command relay over renet. Contains the `parcels-server` binary. |
| `parcels_game` | Bevy client: isometric map renderer, camera, build tools, egui HUD, menu/lobby, synthesized audio. |

Balance constants live in `config/balance.ron` (money in cents, land value
0..=1000). They're copied into each new game's state, so a save always replays
with the rules it was played under. `cargo run -p parcels_sim --example headless
--release` runs an all-AI game and prints yearly stats and an ASCII map, which is
handy when tuning.

## Design notes

- **Commands are the only way to change the world.** Local input, the AI, the
  network and replays all produce the same `Command` values. `Session` stamps
  them with tick and sequence, and the sim applies them sorted by (author, seq).
- **Who controls a parcel is a field** (`Controller::Human | Ai | Vacant`).
  Single-player, hot-seat, AI and online play differ only in where commands come
  from.
- **Tick pipeline:** apply commands → utilities (flood-fill networks across
  borders; each owner uses its own supply first, then buys from neighbours who
  set a price, cheapest seller first) → RCI demand (map-wide, shifted per player
  by their tax rate) → growth (every eligible zone on the map competes for one
  shared pool of newcomers, ranked by land value) → land value (parks, river,
  shops, utilities, minus pollution and traffic, spread by distance) → economy
  (taxes, upkeep and trade payments, clamped at a debt floor).
- **Networking:** the host owns the clock. Clients send command requests; the
  host assigns author, tick and order, steps its own sim, and broadcasts the
  exact list it applied. Joiners get one postcard snapshot and then follow the
  stream. Clients report a state hash every 30 ticks, and a mismatch triggers a
  resync. A late joiner takes over an empty or AI parcel; when a player leaves,
  the AI takes their parcel back.
- **Determinism guards:** replay, snapshot and arrival-order tests, plus a test
  that fails if `f32`, `f64`, `HashMap`, `Instant` and similar show up in sim
  source.

## Not built yet (deliberately)

Curved roads, per-car traffic, terrain editing, disasters, client-side
prediction, accounts and matchmaking. The stretch goals in `prompt.md`
(negotiated deals, claimable parcels, animated sprites) build directly on the
command and controller model.
