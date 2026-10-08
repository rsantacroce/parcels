# Parcels — art asset brief

This file lists every sprite the isometric renderer needs. It is written so you
can paste it, whole or one section at a time, into an image-generation model.
Each asset has a file name, a canvas size and a description.

The game currently draws everything procedurally (`crates/parcels_game/src/render.rs`),
and those shapes and colours are the reference for these sprites. Once the PNGs
exist under `assets/sprites/`, the renderer can swap its procedural boxes for
them one category at a time.

---

## 1. Global style (prepend to every prompt)

> Isometric pixel art for a cozy SimCity-2000-style city builder. Strict 2:1
> dimetric projection: tile diamonds are exactly twice as wide as they are tall,
> and every edge of a ground diamond is a clean 2-pixel-across, 1-pixel-down
> staircase line. Hard-edged pixel art: no anti-aliasing, no blur, no gradients
> across more than a few shades, no text, no logos, no drop shadows on the
> background. Light comes from the upper left: top faces are brightest, faces
> pointing down-left are mid-tone (~84% brightness), faces pointing down-right
> are darkest (~64%). 1px darker outline only where a shape meets the sky.
> Transparent background (PNG with alpha). Muted, warm, slightly desaturated
> palette. One object per image, centred, with nothing cropped.

### Grid and canvas rules

| Thing | Size at authoring scale (4×) | In-game size (1×) |
| --- | --- | --- |
| One tile's ground diamond | 128 × 64 px | 32 × 16 px |
| Canvas width for any 1-tile sprite | 128 px | 32 px |
| Canvas height | 64 px + the object's height | |

- Author everything at **4×** (a 128×64 diamond). The game downsamples with
  nearest-neighbour, so stick to a 4×4 pixel grid where you can.
- **Anchor:** the diamond's bottom vertex sits at the bottom-centre of the canvas
  (x = 64, y = canvas height − 1). The diamond fills the bottom 64 rows, and
  anything taller grows upward from it.
- A building must stay inside the vertical column above its own diamond. Nothing
  may stick out left, right or below the diamond, or depth sorting breaks.
- **Screen directions of grid neighbours** (needed for roads, wires and pipes):

  | Grid direction | Screen direction | Mask bit |
  | --- | --- | --- |
  | North (y − 1) | up-right | 1 |
  | East (x + 1) | down-right | 2 |
  | South (y + 1) | down-left | 4 |
  | West (x − 1) | up-left | 8 |

  Connected pieces are named by the sum of their bits, `00`–`15`. For example,
  `05` = north + south = a straight running up-right to down-left, and
  `15` = a four-way crossing.

### Reference palette (hex)

| Use | Colours |
| --- | --- |
| Grass | `#347038` `#3C7A40` `#427E44` |
| River water | `#2C60B0`, sparkle `#5A96DC` |
| Earth slab | `#70543A` (left face), `#4A3726` (right face), grass lip `#466E37` |
| Asphalt / lane paint | `#48484E` / `#D2C878` |
| Residential tint / walls / roof | `#78C86E` / `#E1D7BE` / `#AA4637` |
| Commercial tint / body / glass | `#6EA0EB` / `#4A6EA6` / `#AAD2F5` |
| Industrial tint / body | `#E1B950` / `#96825F` |
| Window glass (homes, towers) | `#5A78A0` `#6E96C8` |
| Concrete pad | `#78766F` |
| Power line | `#EBCD3C` on brown poles `#5A3C1E` |
| Water pipe | `#50AAFA` |

---

## 2. Terrain — `assets/sprites/terrain/` (128 × 64)

Ground diamonds that fill the whole 128×64 diamond and leave the corners transparent.

| File | Prompt |
| --- | --- |
| `grass_0.png` … `grass_3.png` | Flat isometric grass tile with subtle, tileable pixel noise. Four variants that differ only in where small tufts and flowers sit, so a big field doesn't repeat visibly. Every variant must tile seamlessly with the others. |
| `water_0.png` … `water_3.png` | Flat isometric river-water tile, deep blue with a few small light sparkles. Four animation frames (shimmer), seamlessly tileable. |
| `shore_00.png` … `shore_15.png` | Grass tile with a river bank cut into it. The mask bits mark which neighbours are water, and the bank (thin sand plus darker earth) faces those sides. `00` is unused. |
| `edge_left.png` | 128 × 88 px. The front-left side of the map's earth slab under one tile: a vertical wall of soil 24 px tall hanging from the tile's lower-left edge, with a thin grass lip at the top. Mid-tone (left face). |
| `edge_right.png` | Same as above for the lower-right edge, in the darker right-face shade. |
| `edge_left_water.png`, `edge_right_water.png` | The same slab pieces where the river reaches the map edge: a cross-section of blue water over dark mud. |

## 3. Roads — `assets/sprites/roads/` (128 × 64)

| File | Prompt |
| --- | --- |
| `road_00.png` … `road_15.png` | Flat isometric asphalt road decal on a transparent diamond. Grey asphalt about 75% of the tile wide, a thin kerb, dashed yellow centre line on straights. The mask says which neighbouring roads it joins: arms reach the tile edge in those directions. `00` is a lone square of tarmac, `05`/`10` are straights, two adjacent bits make a bend, three a T, `15` a crossroads with a plain centre. |
| `bridge_05.png`, `bridge_10.png` | 128 × 80. Short concrete road bridge over water, with railings and one visible pier, along each straight axis. The deck sits about 4 px above the water. |
| `bridge_other.png` | A generic square bridge platform, used for bends and junctions over water. |

## 4. Utilities — `assets/sprites/utilities/`

| File | Size | Prompt |
| --- | --- | --- |
| `pole_00.png` … `pole_15.png` | 128 × 112 | Wooden power-line pole about 40 px tall (4×) standing in the back-right quarter of the tile, with sagging yellow-black wires running to the tile edges in the mask directions. `00` is a bare pole. Transparent everywhere except the pole and wires. |
| `pylon_00.png` … `pylon_15.png` | 128 × 128 | Same as the poles, but a small steel lattice pylon, used when the line crosses water. |
| `pipe_00.png` … `pipe_15.png` | 128 × 64 | Underground water pipe shown as a cut-away: a light-blue pipe with darker joints lying flat in the front-left quarter of the tile, running to the tile edges in the mask directions. Drawn semi-transparent (≈70% alpha), because it only appears in the water overlay. |

## 5. Zones — `assets/sprites/zones/`

Empty zoned lots are ground decals. Buildings are separate sprites drawn on top
of the decal. For a level with several variants, the game picks one per tile at
random, so give each variant a clearly different silhouette.

### Lots (128 × 64)

| File | Prompt |
| --- | --- |
| `lot_res.png` | Flat isometric empty building lot: grass tinted soft green, dotted light-green boundary line just inside the diamond edge. |
| `lot_com.png` | Same, tinted soft blue with a blue dotted boundary. |
| `lot_ind.png` | Same, tinted ochre/yellow with an ochre dotted boundary. |
| `yard_res.png`, `yard_com.png`, `yard_ind.png` | A built-on lot: paved yard 75% of the diamond, slightly tinted with the zone colour, and the same dotted border. |

### Residential (green zone)

| File | Canvas | Prompt |
| --- | --- | --- |
| `res_1_a.png`, `res_1_b.png`, `res_1_c.png` | 128 × 112 | A single small detached suburban house: cream walls, red hipped or gabled roof, two small blue windows per visible wall, a tiny front door facing down-left. Variants: (a) hipped roof, (b) gabled roof with a chimney, (c) L-shaped with a small garden tree. Footprint about half the tile, centred. |
| `res_2_a.png`, `res_2_b.png` | 128 × 112 | Two small terraced houses on one lot: one at the back-left, one at the front-right, both cream with red roofs, and a hedge between them. |
| `res_3_a.png`, `res_3_b.png` | 128 × 136 | Four-storey brick-and-render apartment block filling about 70% of the tile. Regular grid of blue windows (4 per face per floor), flat grey roof with a small rooftop box. Variant b has balconies. |
| `res_4_a.png`, `res_4_b.png` | 128 × 200 | Eight-storey residential tower, beige/pink concrete, dense window grid, flat roof with a water tank. Variant b is slightly slimmer with a setback near the top. |

### Commercial (blue zone)

| File | Canvas | Prompt |
| --- | --- | --- |
| `com_1_a.png`, `com_1_b.png` | 128 × 120 | Small one-storey corner shop: blue-grey walls, a striped awning on the down-left face, a large shop window, and a flat roof with a sign box. |
| `com_2_a.png`, `com_2_b.png` | 128 × 152 | Three-storey office or retail building with horizontal ribbon windows (light blue glass bands) and a pale roof. |
| `com_3_a.png`, `com_3_b.png` | 128 × 200 | Six-storey glass office: dark blue body, continuous glass bands, a thin antenna on the roof. |
| `com_4_a.png`, `com_4_b.png` | 128 × 240 | Eight-to-nine-storey glass skyscraper, deep blue with bright window bands, a roof antenna and red aircraft-warning light. Variant b has a stepped crown. Must stay within 240 px total height. |

### Industrial (yellow zone)

| File | Canvas | Prompt |
| --- | --- | --- |
| `ind_1.png` | 128 × 136 | Small workshop: tan corrugated shed covering the front 3/4 of the lot, sawtooth roof with skylights facing down-left, and one brick smokestack at the back. |
| `ind_2.png` | 128 × 144 | The same factory, larger, with two smokestacks of different heights. |
| `ind_3.png` | 128 × 152 | Bigger factory with three smokestacks, a loading dock with a small truck, and drums. |
| `ind_4.png` | 128 × 168 | A taller heavy-industry hall with three stacks and a cylindrical storage tank. |
| `smoke_0.png` … `smoke_3.png` | 32 × 48 | Small grey puffy smoke plume rising from a chimney top, 4 animation frames looping. Transparent background. |

## 6. Services — `assets/sprites/services/`

| File | Canvas | Prompt |
| --- | --- | --- |
| `power_plant.png` | 128 × 160 | Coal power plant on a concrete pad filling one tile: two grey hyperbolic cooling towers at the back (the left one taller, with dark open tops), and a red-brick turbine hall at the front with a yellow hazard stripe. |
| `water_pump.png` | 128 × 104 | Water pumping station on a concrete pad: a rectangular blue reservoir pool in the front-left half, and a small grey pump house with a blue hipped roof at the back-right. |
| `park_a.png`, `park_b.png` | 128 × 120 | Small neighbourhood park on bright lawn: 2–3 round or conical trees, a sandy footpath, a bench. Variant b adds a tiny fountain. |

## 7. Status badges — `assets/sprites/badges/` (20 × 28)

They float above a building and must stay readable at 1× (5 × 7 px).

| File | Prompt |
| --- | --- |
| `no_power.png` | Tiny warning sign: black frame, red fill, white lightning bolt. |
| `no_water.png` | Tiny warning sign: black frame, light-blue fill, white water drop. |

## 8. UI icons — `assets/sprites/ui/` (64 × 64, flat front view, not isometric)

Simple, chunky pixel-art icons on a transparent background, in the same palette
as the sprites, readable at 24 px.

| File | Icon |
| --- | --- |
| `tool_inspect.png` | Magnifying glass |
| `tool_road.png` | Short road segment with a dashed line |
| `tool_residential.png` | Small house with a green "R" ground tint (no letter, just colour) |
| `tool_commercial.png` | Small shop with an awning, blue tint |
| `tool_industrial.png` | Factory with a chimney, yellow tint |
| `tool_power_line.png` | Power pole with wires |
| `tool_water_pipe.png` | Blue pipe elbow |
| `tool_power_plant.png` | Cooling tower with a lightning bolt |
| `tool_water_pump.png` | Water drop over a pump |
| `tool_park.png` | Tree |
| `tool_bulldoze.png` | Yellow bulldozer |
| `overlay_map.png` | Folded map |
| `overlay_land_value.png` | Coin stack |
| `overlay_power.png` | Lightning bolt |
| `overlay_water.png` | Water drop |
| `overlay_density.png` | Group of three people |
| `overlay_pollution.png` | Smoke cloud |
| `overlay_traffic.png` | Small car |
| `overlay_owners.png` | Flag |

---

## 9. Not needed

- **Cursor, hover, selection, build preview and parcel borders** are gizmo
  lines drawn by the game.
- **Overlay colouring** (land value, pollution and so on) is a tint the game
  applies to sprites.
- **Sound** is synthesized in code (`audio.rs`).

## 10. Checklist before handing assets back

- [ ] PNG, transparent background, no anti-aliased edges.
- [ ] Every 1-tile sprite is 128 px wide, and its diamond's bottom vertex sits at the bottom-centre pixel.
- [ ] Nothing pokes outside the column above its own diamond.
- [ ] Light from the upper left on every sprite.
- [ ] All 16 mask variants exist for `road_`, `shore_`, `pole_`, `pylon_` and `pipe_`.
- [ ] File names exactly as listed.
