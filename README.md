# Parcels

A co-op/competitive neighbourhood city builder in Rust + Bevy 0.19.1. One shared,
deterministic simulation; the map is split into parcels, each with its own owner
and treasury. Roads, power and water cross borders, so neighbours trade and
affect one another's land value.

**Website, manual and downloads:** https://rsantacroce.github.io/parcels-site/ ·
[latest release](https://github.com/rsantacroce/parcels/releases/latest)

Pre-built binaries for Windows, macOS (Apple Silicon) and Linux are attached to
each release. Unpack the archive and run `parcels` from inside its folder, so
it finds `config/balance.ron`. On macOS, clear the download quarantine first:
`xattr -dr com.apple.quarantine parcels-*`.

## Run

```sh
cargo run --release --bin parcels                    # title screen
cargo run --release --bin parcels -- --solo --ai 3 --map 128x96
cargo run --release --bin parcels -- --host --slots 4 --name Rob
cargo run --release --bin parcels -- --join 192.168.1.20:5757 --name Ana
cargo run --release --bin parcels -- --load quicksave
cargo run --release --bin parcels-server -- --slots 4 --wait 2 --map 192x144   # headless host
cargo run --release --bin parcels -- --verify-replay saves/replay.ron
cargo test --workspace --release
```

The title screen (with an AI city growing behind it) offers **Continue**, **New game**
(map size 64×48 up to 256×256, rivers, lakes, woods, neighbours, hot-seat players,
game length, starting money, fires, seed, with a live map preview), **Load game**
(named saves with date, map and players), **Play with a friend** (host a new or saved
game, shows the address to share; join by address; hot-seat) and **Settings**
(volume, mouse, shadows, day/night speed, cars and people, interface size, autosave).

**Controls.** Map: WASD/arrows/screen edge or right-drag to pan, middle-drag or Q/E
to rotate, wheel to zoom, PageUp/PageDown to tilt. **V** drops you into street view:
walk with WASD and the mouse, Shift to run, F to fly, V/Esc to come back; build tools
work at the crosshair. Tools: R street (again: avenue), Z/X/C/O residential,
commercial, industrial, office (again: dense), L power line, P pipe, G/U/J/K/N cycle
power, water, services, parks, landmarks, B bulldoze, Esc inspect. 1–0 overlays
(land value, power, water, density, pollution, traffic, crime, services, owners).
Space pauses, +/- speed, Tab switches parcel in hot-seat, F5/F9 quick-save/load,
F10 menu, H help, ` toggles the side panel.

**What you can build** (balance in `config/balance.ron`):

| Category | Items |
| --- | --- |
| Transport | street, avenue (half the congestion), bridges over water |
| Zones | residential, commercial, industrial, office; low or high density |
| Power | power line, coal, gas, wind turbine, solar farm, nuclear (1×1 to 3×3) |
| Water | pipe, pump (bonus by the river), water tower, treatment plant |
| Services | police (crime), fire station (fires), clinic, hospital (tall towers), school, university (mid-rise housing and offices) |
| Parks | park, plaza, playground, sports field, stadium |
| Landmarks | town hall, monument, airport (4×4; boosts regional demand) |

Services only work while powered and cover tiles on both sides of a parcel border.
The inspector tells you why a building isn't growing (no water, no school, land too
cheap for towers…).

**Winning:** the highest score (treasury plus land value of the parcels you own)
when the time limit runs out (10 game years by default).

## Layout

| Crate | What it does |
| --- | --- |
| `parcels_sim` | Headless simulation. No Bevy, no floats, no wall clock, no hash-map iteration. `GameState`, `Command`, the building catalog, terrain generation, the six-stage tick pipeline, AI, replays. |
| `parcels_net` | Lockstep command relay over renet. Contains the `parcels-server` binary. |
| `parcels_game` | Bevy client: 3D world built from chunked meshes (`world/`), orbit and street-level cameras, day/night, cosmetic cars and pedestrians, build tools, egui HUD and minimap, title screen, saves, settings, synthesized audio. |

Balance constants live in `config/balance.ron` (money in cents, land value
0..=1000). They're copied into each new game's state, so a save always replays
with the rules it was played under. `cargo run -p parcels_sim --example headless
--release` runs an all-AI game and prints yearly stats and an ASCII map, which is
handy when tuning.

## Assets

The world is drawn procedurally today. [`assets/`](assets/README.md) lists every
3D model that could replace that geometry, with sizes and a prompt for
[Meshy](https://www.meshy.ai) text-to-3D. `python3 tools/meshy.py` generates them
through the Meshy API (`MESHY_API_KEY`), and `--readme` rebuilds the list from
`assets/catalog.json`.

## Releasing

Push a `v*` tag. `.github/workflows/release.yml` builds Windows, macOS and Linux
archives and attaches them to the GitHub release.

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

- **Rendering:** the grid is drawn as 16×16-tile chunks of vertex-coloured meshes.
  Each tick a chunk's visible state is hashed and only changed chunks are rebuilt,
  nearest the camera first, within a frame budget. Windows and lamps live in their
  own mesh so night can light them. Cars, people, turbine rotors and flames are
  cosmetic and never touch the sim.

## Not built yet (deliberately)

Curved roads, per-car traffic simulation, terrain elevation, public transport,
client-side prediction, accounts and matchmaking. Saves from v0.1 can't be loaded
(the tile format changed).
