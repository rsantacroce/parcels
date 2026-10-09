#!/usr/bin/env python3
"""Generate the 3D models listed in assets/catalog.json with the Meshy API.

    export MESHY_API_KEY=msy_...
    python3 tools/meshy.py --list                     # show every asset id
    python3 tools/meshy.py --print coal_plant         # print the full prompt to paste into meshy.ai
    python3 tools/meshy.py coal_plant water_tower     # generate these
    python3 tools/meshy.py --group parks              # generate one group
    python3 tools/meshy.py --all --skip-existing      # generate everything still missing
    python3 tools/meshy.py --readme                   # rebuild assets/README.md from the catalog

Each asset runs Meshy's two text-to-3D steps (preview for the shape, then refine
for textures) and saves assets/models/<group>/<id>.glb plus a .json with the
task ids and prompt used. Standard library only.
"""

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

API = "https://api.meshy.ai/openapi/v2/text-to-3d"
ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "assets" / "catalog.json"
OUT = ROOT / "assets" / "models"


def load_catalog():
    return json.loads(CATALOG.read_text())


def items(catalog):
    for group in catalog["groups"]:
        for item in group["items"]:
            yield group["id"], item


def full_prompt(catalog, item):
    return f'{catalog["style"]} {item["prompt"]}'


def request(method, url, key, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Authorization", f"Bearer {key}")
    if data:
        req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as e:
        sys.exit(f"Meshy {method} {url} failed: {e.code} {e.read().decode(errors='replace')}")


def wait(task_id, key, label):
    while True:
        t = request("GET", f"{API}/{task_id}", key)
        status = t.get("status")
        print(f"\r  {label}: {status} {t.get('progress', 0):>3}%", end="", flush=True)
        if status == "SUCCEEDED":
            print()
            return t
        if status in ("FAILED", "CANCELED"):
            print()
            sys.exit(f"  {label} {status}: {t.get('task_error', {}).get('message', '')}")
        time.sleep(5)


def generate(catalog, group, item, key, args):
    prompt = full_prompt(catalog, item)
    print(f"{item['id']} ({item['name']})")
    preview_body = {
        "mode": "preview",
        "prompt": prompt,
        "ai_model": args.model,
        "should_remesh": True,
        "topology": "triangle",
        "target_polycount": item.get("polycount", args.polycount),
        "target_formats": ["glb"],
    }
    preview_id = request("POST", API, key, preview_body)["result"]
    wait(preview_id, key, "preview")
    refine_body = {
        "mode": "refine",
        "preview_task_id": preview_id,
        "enable_pbr": False,
        "texture_prompt": f'{catalog["texture_style"]} {item["prompt"]}'[:800],
        "target_formats": ["glb"],
    }
    refine_id = request("POST", API, key, refine_body)["result"]
    task = wait(refine_id, key, "refine")

    folder = OUT / group
    folder.mkdir(parents=True, exist_ok=True)
    glb = folder / f"{item['id']}.glb"
    urllib.request.urlretrieve(task["model_urls"]["glb"], glb)
    meta = {
        "id": item["id"],
        "prompt": prompt,
        "preview_task_id": preview_id,
        "refine_task_id": refine_id,
        "footprint_m": item["footprint_m"],
        "height_m": item["height_m"],
        "generated": time.strftime("%Y-%m-%d %H:%M:%S"),
    }
    (folder / f"{item['id']}.json").write_text(json.dumps(meta, indent=2) + "\n")
    print(f"  saved {glb.relative_to(ROOT)}")


README_HEAD = """# Parcels — 3D asset list

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

> {style}

"""


def readme(catalog):
    out = [README_HEAD.format(style=catalog["style"])]
    for group in catalog["groups"]:
        out.append(f"## {group['name']}\n")
        if group.get("note"):
            out.append(f"{group['note']}\n")
        for item in group["items"]:
            w, d = item["footprint_m"]
            tiles = item["footprint_tiles"]
            size = f"{tiles}×{tiles} tiles" if tiles else "prop"
            extra = f" · {item['variants']} variants" if item.get("variants") else ""
            poly = f" · ≤{item['polycount']} tris" if item.get("polycount") else ""
            out.append(f"### {item['name']} — `{group['id']}/{item['id']}.glb`\n")
            out.append(f"{size} · footprint {w:g} × {d:g} m · height {item['height_m']:g} m{extra}{poly}\n")
            out.append(f"```text\n{full_prompt(catalog, item)}\n```\n")
    out.append("## Stays procedural (no model needed)\n")
    out.extend(f"- {line}" for line in catalog["procedural"])
    (ROOT / "assets" / "README.md").write_text("\n".join(out) + "\n")
    print("wrote assets/README.md")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("ids", nargs="*", help="asset ids to generate")
    p.add_argument("--all", action="store_true", help="generate every asset")
    p.add_argument("--group", help="generate every asset in one group (e.g. power)")
    p.add_argument("--list", action="store_true", help="list asset ids and exit")
    p.add_argument("--readme", action="store_true", help="rebuild assets/README.md and exit")
    p.add_argument("--print", metavar="ID", help="print the full prompt for one asset and exit")
    p.add_argument("--skip-existing", action="store_true", help="skip assets whose .glb already exists")
    p.add_argument("--model", default="latest", help="Meshy ai_model (default: latest)")
    p.add_argument("--polycount", type=int, default=8000, help="default target triangles (default: 8000)")
    args = p.parse_args()
    catalog = load_catalog()

    if args.readme:
        readme(catalog)
        return
    if args.list:
        for group, item in items(catalog):
            print(f"{group:12} {item['id']:20} {item['name']}")
        return
    if args.print:
        for _, item in items(catalog):
            if item["id"] == args.print:
                print(full_prompt(catalog, item))
                return
        sys.exit(f"unknown asset id: {args.print}")

    chosen = [
        (g, i) for g, i in items(catalog)
        if args.all or g == args.group or i["id"] in args.ids
    ]
    unknown = set(args.ids) - {i["id"] for _, i in chosen}
    if unknown:
        sys.exit(f"unknown asset id(s): {', '.join(sorted(unknown))} (see --list)")
    if not chosen:
        p.print_help()
        return
    key = os.environ.get("MESHY_API_KEY")
    if not key:
        sys.exit("Set MESHY_API_KEY first (meshy.ai > Settings > API keys).")
    for group, item in chosen:
        if args.skip_existing and (OUT / group / f"{item['id']}.glb").exists():
            print(f"{item['id']}: exists, skipping")
            continue
        generate(catalog, group, item, key, args)


if __name__ == "__main__":
    main()
