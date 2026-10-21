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
power, water, services, parks, landmarks, B bulldoze, Esc inspect. 1–9 and 0 switch data views
(land value, power, water, density, pollution, traffic, crime, services, buses).
Space pauses, +/- speed, F5/F9 quick-save/load,
F10 menu, H help, ` toggles the side panel.

**What you can build** (balance in `config/balance.ron`):

| Category | Items |
| --- | --- |
| Transport | street, avenue (faster, carries more), bridges over water, rail track (level crossings, rail bridges), subway line, bus stop, bus depot, train station, subway station |
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

**Being mayor.** Residents write letters about whatever bothers them most: a school
near their home, a park, lower taxes, cleaner air, quieter streets. Each letter
asks for one concrete thing and gives you a year. Granting it earns goodwill that
lifts approval for a while; letting it lapse costs some. Every four years the city
votes, and you need 50% approval to win. Lose and you can stay on as caretaker.
Approval also feeds back into growth: a popular mayor draws newcomers, an
unpopular one drives them away.

**Regions.** From the title screen, *Regions* creates a grid of plots (2×2 to
4×4), each its own city map. Found a city on any plot and play it; the others
stay as you left them. Neighbours connect where something crosses the map edge
facing them: a road lets residents commute to the neighbour's open jobs (and its
job seekers drive in) and adds demand for homes, shops and factories; a power
line or water pipe buys the neighbour's spare supply when you're short, or
sells yours when it is. Signs at the map edges name the cities beyond, and the
Region card shows what crosses each border. Each city sees its neighbours as
they were when last played, so the simulation stays deterministic.

**Eras.** A new city starts as a **Town** with streets, small zones, coal and
wind, buses and the basic services. The **City** era (1,500 residents, or year 4)
adds avenues, offices, gas and solar power, water treatment, the town hall,
monument and sports field. **Metropolis** (6,000, or year 7) brings dense zones,
hospitals, universities and the stadium; **Modern** (15,000, or year 11) nuclear
power and the airport. Whichever comes first, population or calendar, and an era
is never lost. Turn eras off when founding a city to have everything from day one.

**Getting around.** Every few ticks the city works out its commute: residents
travel the cheapest way along the roads to the nearest jobs that still have
room, and each road's traffic is the number of trips that pass over it. Avenues
are quicker and carry more, so routes favour them. Homes cut off from every job
count as out of work. Bus stops go beside a road; a powered bus depot on the
same road network makes them work, and trips that start and end near working
stops partly go by bus, which takes cars off the road and earns fares. Rail track can cross roads at level crossings and water on bridges. A train
station must touch the track and a road, and works once its line has another
station or runs off the map to a neighbouring city. Homes near a working
station send half their commuters by train, to jobs near another station or to
the neighbour down the line, before anyone takes the road. The subway runs underground beneath anything on land; its stations sit on the
line and carry 60% of nearby commuters to jobs near another station, ahead of
trains and cars. The very top level of a tower needs a subway station nearby.
The Traffic view (7)
colours every road by load, and the Transit view (0) shows where stops and
stations reach, and the tunnels.

## Layout

| Crate | What it does |
| --- | --- |
| `parcels_sim` | Headless simulation. No Bevy, no floats, no wall clock, no hash-map iteration. `GameState`, `City`, `Command`, the building catalog, terrain generation, the nine-stage tick pipeline, eras, autoplay, replays. |
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
  traffic, spread by distance) → commute (residents route along the roads to
  jobs with room for them; traffic is the trips each road carries; trips between
  two bus stops partly ride the bus) → economy (taxes, fares and upkeep, clamped at a debt
  floor) → status (resident mood per factor; approval drifts toward the total plus
  goodwill) → mayor (letters granted or lapsed, new letters, elections).
- **Determinism guards:** replay, snapshot and arrival-order tests, plus a test
  that fails if `f32`, `f64`, `HashMap`, `Instant` and similar show up in sim
  source.

- **Rendering:** the grid is drawn as 16×16-tile chunks of vertex-coloured meshes.
  Each tick a chunk's visible state is hashed and only changed chunks are rebuilt,
  nearest the camera first, within a frame budget. Windows and lamps live in their
  own mesh so night can light them. Cars, people, turbine rotors and flames are
  cosmetic and never touch the sim.

## Not built yet (deliberately)

Curved roads, per-car traffic simulation, terrain elevation, public transport. Saves from v0.1 can't be loaded
(the tile format changed).
