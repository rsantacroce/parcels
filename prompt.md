# Build Prompt — "Parcels": a co-op/competitive neighborhood city builder (Rust + Bevy)

Oct 7, 2026 · @Rob

## What you're building

Build *Parcels*, a tile-based city-builder in the spirit of SimCity 2000, where one shared map is divided into owned parcels and each player develops their own neighborhood from a separate budget.

The twist that defines the project: the map is one simulation, but control is split. In single-player you run several neighborhoods yourself or against AI. In multiplayer, 2–8 players each own a slice of the same map and build on it live. Roads, power lines, and water pipes cross parcel borders, so neighbors are economically entangled — a player who builds a power plant can sell surplus to the one who didn't, and land value bleeds across the fence. The result is part co-op, part competition: you grow your own patch, but the patch next door changes what yours is worth.

Keep the fantasy small and legible. A neighborhood is tens-to-hundreds of tiles, not a metropolis. That scope keeps the simulation understandable, the multiplayer state small enough to sync, and the project finishable.

## The neighborhood model

The core unit is the *parcel*: a contiguous block of tiles owned by exactly one player. Partition the map into parcels at the start (fixed districts) for v1 — simpler to reason about and to network than parcels claimed during play.

Ownership rules:

- A player may only place or bulldoze tiles inside parcels they own.
- Each player has an independent treasury. Income and expenses are computed per owner, not globally.
- Tiles know their owner, but simulation effects (traffic, pollution, land value) ignore the border — they spread by physical adjacency, not by ownership.

This split is what makes the design work: edits are conflict-free (no two players touch the same tile), but consequences are shared (everything physical flows across borders).

Shared infrastructure crossing borders:

- Roads connect into one network regardless of who built each segment; traffic routes across parcels.
- Power and water form networks spanning the whole map. A plant or pump in one parcel can serve neighbors.
- Neighbor trade: surplus power or water is offered to adjacent parcels that are short, at a price. The buyer pays the source's owner each tick. This is the main economic link between players.

Cooperation and competition then fall out naturally:

- Cooperate: one player builds utilities and sells capacity while another specializes in dense housing that needs it; a shared road spine lifts everyone's land value.
- Compete: land value is relative. Attractive development next door pulls residents and businesses toward the border, while a neighbor's pollution and traffic can sink your values. Players compete for a limited pool of incoming population and jobs.

Pick one scoring rule for v1: highest treasury plus total land value at a time limit (competitive), or a combined population target (co-op). Keep the score visible so the competition reads.

## Gameplay loop

Moment to moment, the player zones land, lays infrastructure, and watches the neighborhood respond, adjusting spending as the treasury rises or falls.

The session loop:

1. Survey the map and decide what your parcel needs — housing, jobs, utilities, roads.
2. Spend from your treasury to build. Every tile has a build cost and an ongoing upkeep.
3. The simulation ticks: zones fill or empty on demand, population and jobs shift, utilities are consumed, taxes come in, upkeep goes out.
4. Land value updates from what's nearby — yours and your neighbors'. Your income next tick reflects it.
5. React: raise or cut taxes, expand, bulldoze what's failing, or strike a utility-trade deal with a neighbor.

Single-player and multiplayer run the *same* loop on the *same* map; the only difference is who issues the commands for each parcel.

- Single-player: you control one parcel and leave the rest to AI or empty, or you control several parcels yourself — just select a parcel to act on it.
- Multiplayer: each human controls their own parcel(s); the simulation runs identically and stays in lockstep across machines.

Design so that "who owns this parcel" is just a field — a human, an AI, or you. Then single-player, hot-seat, and networked multiplayer become the same code with different command sources. This is the key architectural decision, and it is what lets single and multiplayer share one map.

## Tech stack and hard constraints

Language and engine: Rust with Bevy, targeting the current release (0.19 at time of writing — check for newer). Pin the exact version in `Cargo.toml` and follow that version's migration guide; Bevy's API changes between releases, so ignore tutorials written for older versions.

Crates to consider:

- `bevy` — ECS, rendering, input, windowing.
- Networking, when you reach it: `bevy_replicon`, `lightyear`, or `renet`. Replicon and lightyear are higher-level (replication, rollback); renet is a lower-level reliable-UDP transport. Choose one when you start multiplayer, not before.
- `serde` — serialize game state and commands (saves and network messages).
- `ron` or `toml` — human-readable config and save files.

Four non-negotiable constraints, decided now because they are expensive to retrofit:

1. **The simulation is deterministic and tick-based.** It advances in discrete ticks on a fixed timestep (e.g. Bevy's `FixedUpdate`). Given the same starting state and the same commands, every machine produces an identical result. Rendering interpolates between ticks for smoothness but never drives simulation state.
2. **No floating-point in simulation state.** Floats diverge across machines and break determinism. Use integers or fixed-point for money, population, land value, and utility flow. Keep floats for rendering only.
3. **No wall-clock time or unseeded RNG in the sim.** A seeded PRNG advanced once per tick is fine; `rand::thread_rng()` is not.
4. **Commands, not state, are the unit of change.** Every player action is a `Command` value (`PlaceTile`, `Bulldoze`, `SetTaxRate`, …) tagged with parcel, tick, and author. The sim applies commands at a tick. This makes saves, replays, undo, AI, and networking all the same mechanism.

Separate the simulation crate from the rendering/Bevy crate. A headless sim that compiles without Bevy is easier to test, and it lets you run an authoritative server with no graphics.

## World and data model

Represent the map as a fixed-size grid of tiles. A dense array indexed by (x, y) is simplest and fastest; store it in a Bevy `Resource`, not as one entity per tile — millions of entities are wasteful for a grid.

The authoritative state (everything needed to reproduce the game) is small and serializable:

| Thing | Shape | Notes |
| --- | --- | --- |
| `GameState` | resource | tick counter, RNG seed, map, parcels, players |
| `Map` | grid of `Tile` | fixed width × height |
| `Tile` | struct | kind, owner id, utility flags, land value |
| `Parcel` | struct | id, owner id, its tiles or a rectangle |
| `Player` | struct | id, treasury (integer), tax rate, human/AI |
| `Command` | enum | the player actions, each with parcel + tick |

Zone and building kinds for v1: residential, commercial, industrial (the SimCity trio), plus road, power line, water pipe, power plant, water pump, and park. That is enough for a real economy without drowning in content.

Keep everything in the sim state `Clone` + `Serialize`. A save file is `GameState` serialized; a network snapshot for late-joiners is the same bytes. Bevy entities and components are the *view* of this state — spawn sprites from the grid on change, but treat the grid as the single source of truth.

## Simulation systems

Each tick runs the same ordered pipeline of systems, every one reading the previous one's output. The order matters: land value from this tick feeds demand next tick, and that feedback loop is what makes a city-builder feel alive rather than static.

&#91;embedded content: tick pipeline · 6 stages, loops each tick\]

The systems, in order:

1. **Apply commands** — execute every command queued for this tick (place, bulldoze, set tax), mutating the grid. Use a deterministic order: sort by author, then by sequence.
2. **Utilities** — flood-fill power and water from sources along connected networks and mark which tiles are served. Compute each owner's surplus or deficit, then settle neighbor trades.
3. **Demand (RCI)** — compute residential, commercial, and industrial demand per player from jobs, population, tax rate, and available served land. This is the RCI balance SimCity players know.
4. **Growth and decay** — zoned tiles that are served and in demand grow (gain population, jobs, density); unserved or low-demand tiles decay. Bound the change per tick so it stays gradual.
5. **Land value** — recompute each tile from desirable neighbors (parks, served utilities, low pollution) and undesirable ones (industry, traffic, pollution), spreading across parcel borders by distance.
6. **Economy** — collect taxes (population × land value × tax rate), subtract upkeep, apply utility-trade payments, update each treasury. Clamp to prevent negative-money exploits.

Tune with a small set of constants in a config file (`ron` or `toml`) so you can balance without recompiling. Start with simple linear rules; make them richer once the loop is fun.

## Multiplayer architecture

Run one authoritative simulation and keep every client in lockstep by sharing *commands*, not world state. Because the sim is deterministic (per the constraints above), every machine that applies the same commands at the same ticks computes the same world — so you send only the small stream of player commands over the network, never the map.

&#91;embedded content: authoritative command flow · clients ↔ server\]

The model:

- One machine is the **host/server** — a dedicated headless build, or one player's game. It owns the official tick clock and the canonical ordering of commands.
- Clients send their commands to the server. The server stamps each with the tick it will execute on and broadcasts the ordered command list for that tick to everyone.
- Every client (and the server) applies that same list on that tick. The simulation stays identical everywhere.
- A new or reconnecting player gets a full `GameState` snapshot (serialized once), then follows the command stream from that tick onward.

Why parcels make this easy: ownership removes edit conflicts. The server's only validation is "does this command's author own that parcel, and can they afford it?" There is no merge logic and no fighting over the same tile — the hard part of shared-world multiplayer is designed away.

Keep it simple for v1:

- Lockstep (everyone applies a tick's commands together) is the easiest correct model and fine for a slow builder where a few hundred milliseconds of input delay is invisible. Start here.
- Add client-side prediction later, only if it feels laggy: show the player's own build instantly, reconcile when the server confirms.
- Use a networking crate for transport and replication rather than hand-rolling sockets.

Guard against divergence by hashing `GameState` every N ticks and comparing across clients. A mismatch means a determinism bug — usually a stray float or unordered iteration — so catch these early.

## Build in phases

Build single-player fully before touching the network, and make each milestone a playable vertical slice rather than a hidden layer.

1. **M0 — Scaffold.** Bevy app opens a window, renders a grid of tiles from the `Map` resource, camera pan and zoom work. No simulation yet. Proves the engine setup and render path.
2. **M1 — Build and bulldoze.** Click to place and remove roads and zones on a grid you own. Commands are created and applied; the grid updates; one treasury, costs deducted. No growth yet.
3. **M2 — The simulation loop.** Add the tick pipeline: utilities, demand, growth, land value, economy. A single neighborhood now grows, decays, and earns. Tune constants until it is fun to watch. This is the real game — most effort lives here.
4. **M3 — Parcels and multiple budgets.** Partition the map into parcels with owners and separate treasuries. Add cross-border infrastructure and neighbor utility-trade. Control several parcels yourself, or hand some to a simple AI. The neighborhood design is now real, still single-process.
5. **M4 — Determinism pass.** Remove any floats, wall-clock, or unordered iteration from the sim. Add state hashing and a replay test (same commands → same hash). Do this before networking, not after — it is much harder to debug over the wire.
6. **M5 — Networked lockstep.** Add a networking crate. The server orders commands and broadcasts per tick; clients apply in lockstep. Two players build on one shared map live. Snapshot for late join.
7. **M6 — Polish.** Scoring and win conditions, save/load (serialize `GameState`), sound, better art, data-layer overlays, a lobby. Ship it to friends.

If time is short, M0–M3 alone is a complete, enjoyable single-player neighborhood builder. Networking is the ambitious half — treat it as a separate project you have set yourself up to reach.

## UI, camera, and controls

Go top-down, not isometric, for v1. Top-down is far easier in Bevy's 2D renderer and lets you ship M0 fast; isometric (the SimCity 2000 look) is prettier but costs you depth-sorting and trickier input picking. Start top-down and switch later only if you want the look.

Camera: pan with WASD or edge-scroll, zoom with the scroll wheel, clamp to the map bounds. A simple orthographic 2D camera is enough.

Build tools: a toolbar to pick what to place — road, R/C/I zone, power, water, park, bulldozer. Click or click-drag to paint tiles in parcels you own; show a ghost preview and the cost before committing, and block placement outside your parcels with a reason.

HUD: current treasury, tick and clock, tax controls, and the RCI demand indicator. In multiplayer, a small panel per player showing treasury and score.

Data overlays (toggle keys): land value, power coverage, water coverage, population density, pollution. These are how a city-builder becomes legible — reuse the grid render and just recolor tiles by the chosen layer. Cheap to build, huge for playability.

For UI, Bevy's built-in UI works, or add `bevy_egui` for fast tool panels and debug controls while prototyping.

## Scope guards (what not to build yet)

Protect the project from the feature creep that kills city-builder clones; everything here is a deliberate *later*, not a never.

- No curved roads, highways, or subways in v1. One road type on the grid is enough for a working traffic and connectivity model.
- No detailed traffic simulation (per-car pathfinding). Model congestion as a tile property derived from nearby density, not individual vehicles.
- No terrain editing, elevation, or water management beyond flat land plus fixed water tiles.
- No tech tree, eras, or disasters. SimCity's charm can wait; the economy loop is the thing to nail first.
- No fancy art. Colored tiles and simple sprites are fine through M5; art is the cheapest thing to add last.
- No dedicated-server infrastructure, accounts, or matchmaking. Direct host/connect by IP or a short code is plenty for playing with friends.
- No premature optimization. A few hundred tiles ticking a simple pipeline is nothing for Rust; profile only if you actually drop frames.

## Stretch goals (once the core is fun)

- Richer neighbor economics: let players explicitly sell power, water, or services to each other at negotiated prices, or form shared-budget alliances.
- Claimable parcels: start with a small owned area and buy adjacent land as you grow, so the map carves up dynamically.
- AI opponents with simple strategies (utility baron, dense-housing developer) for solo play and to fill empty parcels online.
- Spectate and drop-in: friends join an in-progress game via snapshot and take over an empty or AI parcel.
- Isometric render and animated sprites for the SimCity 2000 feel.
- Mod-friendly content: load zone types, buildings, and balance constants from data files so others can tweak the game.
- Persistence: save and resume long games, or keep a persistent neighborhood that friends return to over days.

## How to use this prompt

Drive the build one milestone at a time; don't ask for the whole game in a single step. This document is the overall brief — feed it as context, then work milestone by milestone from the build plan.

A good way to work with a coding assistant:

1. Share this whole document as the project brief, then say: "Let's start with M0. Set up a Rust + Bevy project (pin the current Bevy version), open a window, and render a grid from a `Map` resource with a pannable camera." Get it compiling and running before moving on.
2. After each milestone works, commit, then ask for the next one by name. Keep the determinism and command-pattern constraints in front of the assistant at every step — remind it if it reaches for a float in sim state or a global mutable instead of a command.
3. When Bevy APIs don't match what the assistant writes (they will drift), paste the compiler error and the Bevy version, and ask it to fix against that version's API rather than guess.
4. Ask for tests on the sim crate early — especially the M4 replay/determinism test. A headless, tested sim is what makes the networking milestone tractable.

Prompt hygiene that helps: ask for small, compiling increments; require the simulation crate to stay free of Bevy types; and have the assistant flag any place it introduces nondeterminism so you can catch it. When you get stuck on a Bevy specific, trust the current [Bevy Book](https://bevy.org) and the release's migration guide over any older tutorial.

Treat M0–M3 as the real milestone of ambition for a first project. A fun single-player neighborhood builder in Rust and Bevy is already something most people only talk about — multiplayer is the victory lap.

