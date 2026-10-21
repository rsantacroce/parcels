# Parcels

A single-player city builder in Rust + Bevy 0.19.1, in the spirit of classic
SimCity: lay streets, zone land, run power and water, add services, and watch the
city grow. The simulation is deterministic, so saves and replays reproduce exactly.

**Website, manual and downloads:** https://rsantacroce.github.io/parcels-site/ ·
[latest release](https://github.com/rsantacroce/parcels/releases/latest)

Pre-built binaries for Windows, macOS (Apple Silicon) and Linux are attached to
each release. Unpack the archive and run `parcels` from inside its folder, so
it finds `config/balance.ron`. On macOS, clear the download quarantine first:
`xattr -dr com.apple.quarantine parcels-*`.

## Run

```sh
cargo run --release --bin parcels                    # title screen
cargo run --release --bin parcels -- --new --map 128x96 --city "New Harbor"
cargo run --release --bin parcels -- --load quicksave
cargo run --release --bin parcels -- --verify-replay saves/replay.ron
cargo test --workspace --release
```

The title screen (with an autoplay city growing behind it) offers **Continue**, **New game**
(city name, map size 64×48 up to 256×256, rivers, lakes, woods, starting money,
fires, seed, with a live map preview), **Load game** (named saves with date, map,
population and treasury) and **Settings**
(volume, mouse, shadows, day/night speed, cars and people, interface size, autosave).

**Controls.** The map opens in an isometric view: WASD/arrows/screen edge or
right-drag to pan, Q/E to turn a quarter, wheel to zoom. T switches to a free 3D
camera (middle-drag or Q/E to rotate, PageUp/PageDown to tilt) and back. **V** drops you into street view:
walk with WASD and the mouse, Shift to run, F to fly, V/Esc to come back; build tools
work at the crosshair. Tools: R street (again: avenue), Z/X/C/O residential,
commercial, industrial, office (again: dense), L power line, P pipe, G/U/J/K/N cycle
power, water, services, parks, landmarks, B bulldoze, Esc inspect. 1–9 overlays
(land value, power, water, density, pollution, traffic, crime, services).
Space pauses, +/- speed, F5/F9 quick-save/load,
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

Services only work while powered. The inspector tells you why a building isn't growing (no water, no school, land too
cheap for towers…).

There is no time limit or score: the game is a sandbox. Residents rate nine parts
of city life (jobs, power and water, safety, clean air, schools and health, taxes,
traffic, neighbourhoods, finances); together they set the mayor's approval, and the
worst one is shown as the city's top issue. Milestones are announced as the
population grows.

## Layout

| Crate | What it does |
| --- | --- |
| `parcels_sim` | Headless simulation. No Bevy, no floats, no wall clock, no hash-map iteration. `GameState`, `City`, `Command`, the building catalog, terrain generation, the six-stage tick pipeline, autoplay, replays. |
| `parcels_game` | Bevy client: 3D world built from chunked meshes (`world/`), orbit and street-level cameras, day/night, cosmetic cars and pedestrians, build tools, egui HUD and minimap, title screen, saves, settings, synthesized audio. |

Balance constants live in `config/balance.ron` (money in cents, land value
0..=1000). They're copied into each new game's state, so a save always replays
with the rules it was played under. `cargo run -p parcels_sim --example headless
--release` lets autoplay build a city and prints yearly stats and an ASCII map, which is
handy when tuning.

## Assets

The world is drawn procedurally today. [`assets/`](assets/README.md) lists every
3D model that could replace that geometry, with sizes and a prompt for
[Meshy](https://www.meshy.ai) text-to-3D. `python3 tools/meshy.py` generates them
through the Meshy API (`MESHY_API_KEY`), and `--readme` rebuilds the list from
`assets/catalog.json`.

## Website data

`cargo run -p parcels_sim --example pedia --release > ../parcels-site/pedia.json`
exports every item and balance constant for the website's Parcelspedia. Rerun it
whenever `config/balance.ron` or the building catalog changes.

## Releasing

Push a `v*` tag. `.github/workflows/release.yml` builds Windows, macOS and Linux
archives and attaches them to the GitHub release. To add binaries to an existing
tag, run the workflow by hand: `gh workflow run release.yml -f tag=v0.2.0`.

## Design notes

- **Commands are the only way to change the world.** Player input, autoplay and
  replays all produce the same `Command` values. `Session` stamps them with tick
  and sequence, and the sim applies them in sequence order.
- **Tick pipeline:** apply commands → utilities (flood-fill each power and water
  network and serve tiles from its supply) → RCI demand (shifted by the tax rate)
  → growth (every eligible zone competes for one pool of newcomers, ranked by
  land value) → land value (parks, river, shops, utilities, minus pollution and
  traffic, spread by distance) → economy (taxes and upkeep, clamped at a debt
  floor) → status (resident mood per factor; approval drifts toward the total).
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
