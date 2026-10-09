# Parcels — 3D asset list

Every model the game could swap in for its procedural geometry, with a prompt
ready to paste into [Meshy](https://www.meshy.ai) (Text to 3D). This file is
generated from [`catalog.json`](catalog.json) by `python3 tools/meshy.py --readme`;
edit the catalog, not this file. The same list, with copy buttons, is on the
website's Assets page.

## How to use it

**By hand:** open Meshy, choose *Text to 3D*, paste the prompt of an asset
(each one already starts with the shared style line), generate, pick the best
preview, refine it, then download the GLB as `models/<group>/<id>.glb`.

**With the API:** get a key in Meshy's settings, then

```sh
export MESHY_API_KEY=msy_...
python3 tools/meshy.py --list
python3 tools/meshy.py coal_plant water_tower   # or --group power, or --all --skip-existing
```

## Conventions

- **Units are metres.** A map tile is 10 × 10 m. Scale each model to its footprint and height below on import.
- **Origin** at the centre of the footprint, on the ground. **Y is up**, the front (the side facing the road) points to −Z.
- Keep everything **inside its footprint**: nothing may overhang a neighbouring tile.
- **Matte, simple, chunky.** The game is flat-shaded low-poly with soft shadows; photoreal textures look out of place.
- Windows that glow at night are added by the game, so don't bake lit windows into textures.
- Cars, people and the zone sign are **white** where the game re-tints them.

## Shared style (already included in every prompt below)

> Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people.


## Residential zone

Grows by itself on green zones. Low density stops at level 3, high density goes to level 6. The building faces the road on its front (-Z) side.

### Cottage (level 1) — `residential/res_cottage.glb`

1×1 tiles · footprint 6 × 5.5 m · height 5.6 m · 3 variants

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A small one-storey suburban cottage with cream walls, a red gable roof with a short overhang, two small square windows per wall, a front door, and a small round garden tree beside it.
```

### Family house (low density, level 2) — `residential/res_house.glb`

1×1 tiles · footprint 7 × 6.6 m · height 8.6 m · 3 variants

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A two-storey detached family house, pastel rendered walls, dark brown pitched gable roof, regular grid of white-framed windows, a porch over the front door, compact box shape.
```

### Townhouse terrace (low density, level 3) — `residential/res_terrace.glb`

1×1 tiles · footprint 8.8 × 7.4 m · height 9.6 m · 2 variants

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A row of three joined three-storey townhouses with a flat roof and a slightly darker parapet, each with its own front door and tall windows, alternating cream, pale pink and sage facades.
```

### Brick walk-up (high density, levels 2-3) — `residential/res_walkup.glb`

1×1 tiles · footprint 8.4 × 8 m · height 16 m · 2 variants

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A three-to-four storey red-brick apartment walk-up filling its square lot, flat roof with a darker coping, a regular grid of tall windows on every face, a simple entrance with steps at the front.
```

### Apartment tower (high density, levels 4-6) — `residential/res_tower.glb`

1×1 tiles · footprint 7.6 × 7.6 m · height 60 m · 3 variants

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A slender residential apartment tower, pale concrete with a dense grid of windows and small balconies, a setback upper storey and a boxy rooftop plant room. Square footprint, very tall and narrow.
```

## Commercial zone

Shops up to level 3, glass towers on dense lots from level 4.

### Corner shop (levels 1-3) — `commercial/com_shop.glb`

1×1 tiles · footprint 8.8 × 7.6 m · height 11 m · 3 variants

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A small high-street shop building, one to three storeys, a wide glowing shop window across the ground floor, a coloured striped awning over it, flat roof with a box-shaped illuminated sign on top.
```

### Commercial tower (dense, levels 4-6) — `commercial/com_tower.glb`

1×1 tiles · footprint 8.4 × 8.4 m · height 62 m · 2 variants

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A modern glass commercial skyscraper on a wider two-storey podium, blue-grey metal frame with continuous horizontal glass bands on every floor, a glowing coloured box on the flat roof. Square footprint, tall.
```

## Industrial zone

### Workshop (low density) — `industrial/ind_workshop.glb`

1×1 tiles · footprint 8.8 × 7 m · height 12 m · 2 variants

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A small industrial workshop, tan corrugated-metal shed with a sawtooth roof of three skylight ridges, one brick smokestack with a red band at the top, a stack of wooden crates in the yard.
```

### Heavy factory (dense, level 3+) — `industrial/ind_factory.glb`

1×1 tiles · footprint 8.8 × 9 m · height 25 m · 2 variants

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A heavy-industry factory hall, tall grey corrugated building with a sawtooth roof, three tapering concrete smokestacks of different heights with red bands near the top, and a white cylindrical storage tank beside it.
```

## Office zone

### Office block (levels 1-3) — `office/off_block.glb`

1×1 tiles · footprint 8 × 8 m · height 20 m · 2 variants

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A mid-rise modern office block, dark steel-blue frame with ribbon windows wrapping every floor, flat roof with a thin darker coping. Square footprint, two to six storeys.
```

### Office skyscraper (levels 4-6) — `office/off_skyscraper.glb`

1×1 tiles · footprint 8 × 8 m · height 90 m · 2 variants

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A tiered glass office skyscraper: a tall square base and a narrower upper tier, glass curtain walls with a floor grid, topped either with a pointed pyramid crown or a slim antenna mast with a red warning light.
```

## Power

### Coal plant — `power/coal_plant.glb`

2×2 tiles · footprint 20 × 20 m · height 28 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A compact coal power station on a concrete pad: two grey hyperbolic cooling towers at the back, a red-brick turbine hall with tall windows at the front, one tall thin chimney, and a black coal heap.
```

### Gas plant — `power/gas_plant.glb`

2×2 tiles · footprint 20 × 20 m · height 18 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A clean modern gas-fired power plant on a concrete pad: a long light-grey turbine hall with two slim exhaust stacks on its roof, and three white round gas storage tanks in a row at the front.
```

### Wind turbine (tower and nacelle) — `power/wind_turbine_tower.glb`

1×1 tiles · footprint 4 × 4 m · height 31 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A white wind turbine tower: a tall tapering cylindrical mast on a small square concrete base with a boxy nacelle on top. No blades: the rotor is a separate model.
```

### Wind turbine rotor (animated) — `power/wind_turbine_rotor.glb`

prop · footprint 26 × 2 m · height 26 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A white three-bladed wind turbine rotor with a rounded hub, blades evenly spaced at 120 degrees, flat and facing forward. Rotor only, no tower.
```

### Solar farm — `power/solar_farm.glb`

2×2 tiles · footprint 20 × 20 m · height 2 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A small solar farm: ten rows of dark blue tilted photovoltaic panels on thin metal legs, arranged in two columns on a square patch of dry grass with gravel paths between them.
```

### Nuclear plant — `power/nuclear_plant.glb`

3×3 tiles · footprint 30 × 30 m · height 24 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A nuclear power station on a concrete pad: two large pale hyperbolic cooling towers, a white cylindrical reactor containment building with a domed top, and a grey rectangular turbine hall with a band of windows.
```

## Water

### Water pump — `water/water_pump.glb`

1×1 tiles · footprint 10 × 10 m · height 5.6 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A small water pumping station on a concrete pad: a pale blue pump house with a darker blue gable roof, and a rectangular open reservoir pool of clear blue water beside it.
```

### Water tower — `water/water_tower.glb`

1×1 tiles · footprint 7 × 7 m · height 20 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A classic water tower: a light-blue cylindrical steel tank with a shallow conical roof, raised high on four slightly splayed grey steel legs with cross-bracing.
```

### Water treatment — `water/water_treatment.glb`

2×2 tiles · footprint 20 × 20 m · height 6 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A water treatment works on a concrete pad: three round concrete settling tanks filled with blue-green water, and a white single-storey control building with a band of windows and a blue roof.
```

## Services

### Police station — `services/police_station.glb`

1×1 tiles · footprint 8 × 7.5 m · height 12 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A two-storey police station, pale blue-grey walls with a dark blue band around the top of the walls, rows of windows, a flagpole with a blue flag on the roof corner, and a blue lamp by the entrance.
```

### Fire station — `services/fire_station.glb`

1×1 tiles · footprint 8 × 7.5 m · height 12 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A two-storey red-brick fire station with a white band around the top, a large white roller door for the fire engine on the ground floor, upstairs windows, and a flagpole with a red flag on the roof corner.
```

### Clinic — `services/clinic.glb`

1×1 tiles · footprint 8 × 7.5 m · height 8.2 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A small two-storey medical clinic, clean white walls with a red band around the top, rows of windows, and a red cross sign lying on the flat roof.
```

### Hospital — `services/hospital.glb`

2×2 tiles · footprint 16 × 16 m · height 25 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A white modern hospital: a wide two-storey base building with a narrower six-storey tower rising from its centre, regular window grid, and a large red cross helipad marking on the tower roof.
```

### School — `services/school.glb`

2×2 tiles · footprint 18 × 18 m · height 9.2 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. An L-shaped two-storey red-brick school with a dark grey pitched roof, many windows, a flagpole with a blue flag, and a small sports pitch with a red running track filling the inside corner of the L.
```

### University — `services/university.glb`

3×3 tiles · footprint 28 × 28 m · height 28 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A sandstone university campus: three connected three-storey stone buildings with green-grey pitched roofs forming a U around a lawn quadrangle, and a tall square clock tower with a pyramid roof and a lit clock face.
```

## Parks

### Park — `parks/park.glb`

1×1 tiles · footprint 10 × 10 m · height 7 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A small square neighbourhood park on bright green lawn: three round and conical trees, a sandy footpath straight across the middle, and a wooden bench.
```

### Plaza — `parks/plaza.glb`

1×1 tiles · footprint 10 × 10 m · height 4 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A square paved town plaza in warm beige stone with a round shallow fountain basin in the centre and a small spout column, and a small tree in each of the four corners.
```

### Playground — `parks/playground.glb`

1×1 tiles · footprint 8 × 8 m · height 4 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A children's playground on a square of soft sand: a red climbing tower with a blue pointed roof, a yellow slide coming down from it, a grey metal swing frame, and one small tree.
```

### Sports field — `parks/sports_field.glb`

2×2 tiles · footprint 20 × 20 m · height 13 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A square grass sports pitch with white boundary and halfway lines, a small white goal at each end, and four tall floodlight masts at the corners.
```

### Stadium — `parks/stadium.glb`

3×3 tiles · footprint 28 × 28 m · height 26 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A compact open-air football stadium: four stepped grandstands with coloured seats enclosing a green pitch, light grey concrete outer walls, and four tall floodlight towers at the corners.
```

## Landmarks

### Town hall — `landmarks/town_hall.glb`

2×2 tiles · footprint 16 × 14 m · height 23 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A neoclassical town hall in pale cream stone: a two-storey block with a row of six white columns across the front portico, a round drum on the roof topped by a green copper dome and a flagpole with a red flag.
```

### Monument — `landmarks/monument.glb`

1×1 tiles · footprint 9 × 9 m · height 19 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A tall white stone obelisk monument with a pyramid tip, standing on a low square stepped plinth in the middle of a small paved square.
```

### Airport — `landmarks/airport.glb`

4×4 tiles · footprint 38 × 38 m · height 23 m

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A small regional airport: a dark asphalt runway with white centre dashes along one side, a grey apron, a long low glass-fronted terminal building, and a slim control tower with a glass cab on top.
```

## Props and street life

Small, repeated many times, so keep them very low-poly (target 300-1500 triangles).

### Broadleaf tree — `props/tree_broadleaf.glb`

prop · footprint 3 × 3 m · height 6 m · 2 variants · ≤600 tris

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A stylised round broadleaf tree with a short brown trunk and a puffy, chunky green canopy.
```

### Conifer — `props/tree_conifer.glb`

prop · footprint 2.5 × 2.5 m · height 8 m · 2 variants · ≤500 tris

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A stylised conifer tree, a stacked cone of dark green layers on a short brown trunk.
```

### Street lamp — `props/street_lamp.glb`

prop · footprint 1 × 1.5 m · height 5 m · ≤300 tris

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A simple modern street lamp: a slim dark grey pole with a short arm and a flat lamp head.
```

### Power pole — `props/power_pole.glb`

prop · footprint 2 × 0.5 m · height 9 m · ≤300 tris

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A wooden utility power pole with one cross-arm and three small insulators on top. No wires.
```

### Pylon (lines over water) — `props/pylon.glb`

prop · footprint 4 × 4 m · height 16 m · ≤1200 tris

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A small steel lattice electricity pylon with two cross-arms and insulators. No wires.
```

### Car — `props/car.glb`

prop · footprint 4.2 × 1.8 m · height 1.7 m · 3 variants · ≤1000 tris

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A small generic hatchback car, rounded toy-like shape, white body (it gets re-tinted in game), dark windows, black wheels. Facing along its length.
```

### City bus — `props/bus.glb`

prop · footprint 10 × 2.4 m · height 3 m · ≤1500 tris

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A single-deck city bus, white body with a continuous dark window band along both sides, black wheels.
```

### Pedestrian — `props/pedestrian.glb`

prop · footprint 0.5 × 0.35 m · height 1.75 m · 2 variants · ≤400 tris

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A tiny stylised standing person, simple blocky body, white shirt (re-tinted in game), dark trousers, arms at the sides.
```

### Parked airliner — `props/airplane.glb`

prop · footprint 12 × 12 m · height 5 m · ≤1500 tris

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A small white twin-engine passenger airliner with a red tail fin, landing gear down, parked.
```

### Zoned lot sign — `props/for_sale_sign.glb`

prop · footprint 1 × 0.2 m · height 1.6 m · ≤100 tris

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A small wooden post with a plain square white sign board on top (re-tinted in zone colour in game).
```

### Burnt rubble — `props/rubble.glb`

1×1 tiles · footprint 8 × 8 m · height 1.5 m · ≤800 tris

```text
Stylized low-poly 3D game asset for a cozy modern city-builder. Clean chunky shapes, slightly toy-like proportions, flat matte colours with a subtle hand-painted texture, readable from a high three-quarter camera. One isolated object, centred, standing on a flat base. No surrounding terrain, no text, no logos, no people. A low heap of charred dark grey rubble, broken concrete blocks and burnt beams on a square lot after a fire.
```

## Stays procedural (no model needed)

- Ground, river and lake water, banks and the earth skirt around the map edge
- Streets, avenues, bridges, sidewalks, zebra crossings and lane markings (they join up tile by tile)
- Power line wires and water pipes
- Window glow at night, overlays, the build cursor and parcel borders
- Fire flames and all sound (synthesised in code)
