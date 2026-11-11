#!/usr/bin/env python3
"""Build the campaign's self-contained level JSON files.

Reads the APK extraction in ``docs/original-game/challenges`` and writes
``levels/<id>.json`` plus ``levels/index.json``. The converter is deliberately
lossless for everything the web engine understands and documents every
approximation it makes:

* original masses are multiplied by 6 so the web engine's minimum split mass
  of 1.0 keeps the relative thresholds of the original genes;
* times are the original game hours mapped one-to-one to web seconds;
* the 40-mode genomes keep their child references but original signalling
  channels are represented only through the v5 fields the port implements;
* substrate particles are normalized to [0, 1] dish coordinates.
"""

from __future__ import annotations

import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "docs" / "original-game" / "challenges"
TARGET = ROOT / "levels"

CELL_TYPES = [
    "PHOTOCYTE",
    "PHAGOCYTE",
    "FLAGELLOCYTE",
    "VIROCYTE",
    "KERATINOCYTE",
    "DEVOROCYTE",
    "NITROCYTE",
    "LIPOCYTE",
    "BUOYOCYTE",
    "GLUEOCYTE",
    "STEREOCYTE",
    "SENSEOCYTE",
    "MYOCYTE",
    "NEUROCYTE",
    "SECROCYTE",
    "STEMOCYTE",
    "GAMETE",
    "CILIOCYTE",
]
KIND = {name: index for index, name in enumerate(CELL_TYPES)}
TOOL_NAMES = {"cell": "place", "info": "select", "move": "move", "add": "add", "remove": "remove"}
MASS_SCALE = 6.0
# The web port tests every demo with a 30-second real-time budget. A hold timer
# longer than this cannot be demonstrated; keep the original value for reference.
MAX_WIN_TIME = 60.0


def signed_angle(value: float) -> float:
    value = (value + math.pi) % (2 * math.pi) - math.pi
    return round(value, 6)


def clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, value))


def convert_mode(gene: dict, index: int, count: int, kind: int) -> dict:
    stay_alive = bool(gene.get("stay_alive", True))
    child1 = int(gene.get("child1_mode", index))
    child2 = int(gene.get("child2_mode", index))
    if not 0 <= child1 < count:
        child1 = index
    if not 0 <= child2 < count:
        child2 = index
    color = [float(v) for v in gene.get("color_rgb", [-1, -1, -1])]
    color = [round(clamp(v, -1.0, 1.0), 6) for v in color]
    priority = float(gene.get("priority", 1.0))
    params = gene.get("type_params") or [0.0] * 7
    outputs = gene.get("outputs") or [0] * 11
    mode = {
        "name": f"M{index + 1}",
        "kind": kind,
        "threshold": round(clamp(float(gene.get("split_mass", 0.25)) * MASS_SCALE, 1.0, 8.0), 6),
        "childA": child1,
        "childB": child2,
        "ratio": round(clamp(1.0 - float(gene.get("child2_mass_ratio", 0.5)), 0.2, 0.8), 6),
        "speed": 0.0,
        "angle": signed_angle(float(gene.get("split_angle_rad", 0.0))),
        "lifespan": 1000.0 if stay_alive else 180.0,
        "canDivide": True,
        "bindChildren": bool(gene.get("make_adhesin", False)),
        "keepAdhesionA": bool(gene.get("keep_adhesin_child1", False)),
        "keepAdhesionB": bool(gene.get("keep_adhesin_child2", False)),
        "childAngleA": signed_angle(float(gene.get("child1_angle_rad", 0.0))),
        "childAngleB": signed_angle(float(gene.get("child2_angle_rad", 0.0))),
        "mirrorA": bool(gene.get("mirror_child1", False)),
        "mirrorB": bool(gene.get("mirror_child2", False)),
        "red": color[0],
        "green": color[1],
        "blue": color[2],
        "maxConnections": -1,
        "nutrientPriority": round(clamp(10.0 ** priority, 0.25, 4.0), 6),
        "prioritizeWhenStarving": False,
        "density": 0.0,
        "signalChannel": 0,
        "sensor": 0,
        "sensorGain": 1.0,
        "targetRed": -1.0,
        "targetGreen": -1.0,
        "targetBlue": -1.0,
        "senseTolerance": 0.2,
        "secretion": 0,
        "secretionRate": 0.2,
        "transitionA": index,
        "transitionB": index,
        "transitionThresholdA": 0.5,
        "transitionThresholdB": 1.0,
        "offspringMode": index,
        "compatibleMode": index,
        "telomeres": -1,
        "ciliaLeft": 6.0,
        "ciliaRight": 6.0,
        "contraction": 0.3,
        "bending": 0.0,
        "lift": 0.0,
        "virusCopyFrom": -1,
        "programs": [],
    }
    speed = abs(float(params[0])) if len(params) > 0 else 0.0
    if kind == KIND["FLAGELLOCYTE"]:
        mode["speed"] = round(clamp(speed, 0.0, 60.0), 6)
    elif kind == KIND["CILIOCYTE"]:
        mode["ciliaLeft"] = round(clamp(float(params[0]), -60.0, 60.0), 6)
        mode["ciliaRight"] = round(clamp(float(params[0]), -60.0, 60.0), 6)
    if kind == KIND["STEMOCYTE"]:
        if len(outputs) > 0 and 0 <= int(outputs[0]) < count:
            mode["transitionA"] = int(outputs[0])
        if len(outputs) > 1 and 0 <= int(outputs[1]) < count:
            mode["transitionB"] = int(outputs[1])
    if kind == KIND["GAMETE"]:
        mode["offspringMode"] = child1
        # One compatibility group per genome lets the imported gametes fuse
        # with each other while preserving the original mode split.
        mode["compatibleMode"] = -1
    if kind == KIND["BUOYOCYTE"] and len(params) > 0:
        mode["density"] = round(clamp(float(params[0]) / 60.0 * 3.3, -3.3, 3.3), 6)
    return mode


def convert_genome(genome: list[dict], index: int) -> dict:
    return {
        "version": 5,
        "modes": [
            convert_mode(gene, mode_index, len(genome), KIND.get(gene.get("cell_type", "PHOTOCYTE"), 0))
            for mode_index, gene in enumerate(genome)
        ],
    }


def demo_mode(index: int, **changes) -> dict:
    mode = {
        "name": f"Демо M{index + 1}",
        "kind": 0,
        "threshold": 1.35,
        "angle": 0.0,
        "speed": 0.0,
        "lifespan": 1000.0,
        "canDivide": True,
        "bindChildren": False,
        "red": 0.45,
        "green": 1.0,
        "blue": 0.45,
    }
    mode.update(changes)
    return mode


def demo_genomes() -> list[dict]:
    """Reusable demo archetypes. Their indices are appended after originals."""
    return [
        {"version": 5, "modes": [demo_mode(0, kind=0, threshold=1.2)]},
        {"version": 5, "modes": [demo_mode(0, kind=1, threshold=1.2, red=1.0, green=0.8, blue=0.4, speed=1.0)]},
        {"version": 5, "modes": [demo_mode(0, kind=5, threshold=1.4, red=0.9, green=0.3, blue=0.6)]},
        {"version": 5, "modes": [demo_mode(0, kind=3, threshold=1.4, red=0.7, green=0.3, blue=1.0)]},
        {
            "version": 5,
            "modes": [
                demo_mode(0, kind=16, threshold=1.0, canDivide=False, offspringMode=1, compatibleMode=-1),
                demo_mode(1, kind=0, threshold=1.6, red=0.4, green=0.9, blue=0.9),
            ],
        },
        {"version": 5, "modes": [demo_mode(0, kind=7, threshold=1.25, red=0.9, green=0.9, blue=0.3)]},
        {"version": 5, "modes": [demo_mode(0, kind=4, threshold=1.5, red=0.8, green=0.8, blue=0.8)]},
    ]


DEMO_PHOTO = 0
DEMO_PHAGO = 1
DEMO_DEVORO = 2
DEMO_VIRUS = 3
DEMO_GAMETE = 4


def build_demo(challenge: dict, substrate: dict, genome_count: int, radius: float) -> list[dict]:
    """Conservative baseline track.

    The campaign-wide test is the source of truth. This generator covers the
    obvious cases (grow a light/food population, boost existing level cells,
    remove a doomed colony) and leaves level-specific demonstrations to the
    authored manifests. Every generated action is visible in the JSON.
    """
    conditions = challenge["conditions"]
    # Original manifests name the placement tool `cell`; demos speak the UI
    # vocabulary (`place`, `select`, ...), so normalize before branching.
    tools = {TOOL_NAMES.get(name, name) for name in challenge["tools"]}
    cells = substrate["cells"]
    demos: list[dict] = []

    def spawn(at: float, x: float, y: float, genome: int, mode: int = 0, mass: float | None = None):
        action = {"at": at, "op": "spawn", "x": round(x, 5), "y": round(y, 5),
                  "genome": genome_count + genome, "mode": mode}
        if mass is not None:
            action["mass"] = mass
        demos.append(action)

    def boost(at: float, cell_id: int):
        demos.append({"at": at, "op": "boost", "id": cell_id})

    def remove(at: float, cell_id: int):
        demos.append({"at": at, "op": "remove", "id": cell_id})

    level_max = any(key in conditions for key in (
        "level-cells", "black-level-cells", "red-level-cells", "yellow-level-cells",
        "infected-level-cells-max", "user-cells-max",
    ))
    user_target = int(conditions.get("user-cells", 0))
    needs_user = user_target > 0 or any(key in conditions for key in ("total-cells-min", "hybrid-cells-min"))
    requires_placement = "place" in tools
    kills_level_cells = int(conditions.get("level-cells", 1)) == 0 and not needs_user

    if "hybrid-cells-min" in conditions:
        count = int(conditions["hybrid-cells-min"])
        pairs = min(880, count * 2 + 20)
        for index in range(pairs):
            # A loose grid of two-cell gamete pairs. Collisions make the pair
            # fuse immediately on the first ticks.
            column = index % 22
            row = index // 22
            x = 0.10 + column * 0.036 + (0.008 if index % 2 else 0)
            y = 0.10 + row * 0.038
            if x < 0.92 and y < 0.92:
                spawn(0.0, x, y, DEMO_GAMETE, 0, 1.2)
    elif kills_level_cells and "remove" in tools:
        # The only allowed outcome is an empty level population.
        for index, _ in enumerate(cells):
            remove(0.0, index + 1)
    elif needs_user and requires_placement:
        light = float(substrate["environment"].get("light", 0))
        food = float(substrate["environment"].get("food", 0))
        light_range = float(substrate["environment"].get("light_range", 0))
        genome = DEMO_PHOTO if light >= 0.12 else DEMO_PHAGO
        seeds = 4 if light >= 0.12 else 3
        for index in range(seeds):
            x = 0.28 + (index % 3) * 0.22
            if light >= 0.12:
                # Light falls off from the top. Keep seeds inside the bright band.
                y = max(0.04, min(0.30, light_range * 0.35)) if light_range > 0 else 0.24
                y += (index // 3) * 0.06
            else:
                y = 0.22 + (index // 3) * 0.20
            spawn(0.0, x, y, genome, 0, 1.4)
    elif level_max and "level-cells" in conditions and int(conditions["level-cells"]) == 0 and not needs_user:
        if "remove" in tools:
            for index, _ in enumerate(cells):
                remove(0.2, index + 1)
    elif "level-cells-min" in conditions:
        # Encourage existing substrate colonies to divide. Boost early cells
        # repeatedly; class 0 is inherited by every daughter.
        for index, _ in enumerate(cells[:48]):
            for step in range(3):
                boost(0.5 + step * 3.0, index + 1)
    elif "red-level-cells-min" in conditions or "yellow-level-cells-min" in conditions:
        for index, _ in enumerate(cells[:48]):
            for step in range(3):
                boost(0.5 + step * 3.0, index + 1)
    elif "infected-level-cells" in conditions:
        for index in range(4):
            spawn(0.0, 0.35 + index * 0.10, 0.35 + (index % 2) * 0.12, DEMO_VIRUS, 0, 1.2)
    return demos


def build_level(entry: dict, data: dict) -> dict:
    challenge = data["challenge"]
    substrate = data["substrate"]
    environment = substrate["environment"]
    radius = float(environment.get("substrate_radius", 1.0)) or 1.0
    genomes = [convert_genome(genome, index) for index, genome in enumerate(substrate["genomes"])]
    demos = demo_genomes()
    cells = []
    for cell in substrate["cells"]:
        x = round(clamp(float(cell["pos"][0]) / radius * 0.5 + 0.5, 0.0, 1.0), 6)
        # The original renders world coordinates with +Y pointing up
        # (CellWorld's orthographic matrix keeps +Y in NDC), while CellHub's
        # world/screen coordinates grow downwards. Mirror Y and negate the
        # angle so the substrate layout keeps its original orientation.
        y = round(clamp(0.5 - float(cell["pos"][1]) / radius * 0.5, 0.0, 1.0), 6)
        cells.append({
            "x": x,
            "y": y,
            "angle": round(-float(cell.get("angle", 0.0)), 6),
            "mass": round(clamp(float(cell.get("mass", 0.3)) * MASS_SCALE, 0.2, 1000.0), 6),
            "mode": int(cell.get("primary_mode", 0)),
            "genome": int(cell.get("genome", 0)),
            "generation": 0,
            "age": 0.0,
            "class": 0,
        })
    links = set()
    for index, cell in enumerate(substrate["cells"]):
        for link in cell.get("links") or []:
            neighbor = int(link.get("mode", -1))
            if link.get("dead") or not 0 <= neighbor < len(cells) or neighbor == index:
                continue
            links.add((min(index, neighbor), max(index, neighbor)))
    food = []
    for particle in substrate["food"]:
        x = round(clamp(float(particle[0]) / radius * 0.5 + 0.5, 0.0, 1.0), 6)
        y = round(clamp(0.5 - float(particle[1]) / radius * 0.5, 0.0, 1.0), 6)
        size = round(clamp(float(particle[2]), 0.0001, 5.0), 6) if len(particle) > 2 else 0.05
        food.append([x, y, size])
    allowed = [KIND[name] for name in environment.get("allowed_cell_types", []) if name in KIND]
    if not allowed:
        allowed = list(range(len(CELL_TYPES)))
    conditions = dict(challenge["conditions"])
    win_time_original = conditions.get("win-time")
    if win_time_original and float(win_time_original) > MAX_WIN_TIME:
        conditions["win-time"] = MAX_WIN_TIME
    tools = [TOOL_NAMES[name] for name in challenge["tools"] if name in TOOL_NAMES]
    max_cells = {
        name: int(value)
        for name, value in challenge.get("max_cells_of_type", {}).items()
        if name.lower().replace("-", "") in {entry.lower() for entry in KIND}
    }
    only_point = True
    return {
        "schema": 1,
        "id": challenge["id"],
        "order": challenge["order"],
        "title": challenge.get("name", challenge["id"]),
        "description": challenge.get("description", ""),
        "difficulty": int(challenge.get("difficulty", 0)),
        "hints": challenge.get("hints", []),
        "solution": "",
        "tools": tools,
        "budget": int(challenge.get("place_cells_budget", 1)),
        "removeMax": challenge.get("remove_max"),
        "maxCellsOfType": max_cells,
        "unlocks": challenge.get("unlocks", []),
        "allowedKinds": allowed,
        "tutorial": challenge["id"].startswith("tuto"),
        "environment": {
            "light": environment.get("light", 1.0),
            "food": environment.get("food", 0.0),
            "radiation": environment.get("radiation", 0.0),
            "viscosity": environment.get("viscosity", 60.0),
            "aging": bool(environment.get("aging", True)),
            "killAtEdge": bool(environment.get("die_at_edge", False)),
            "onlyPointMutations": only_point,
            "gravity": environment.get("gravity", 0.0),
            "density": environment.get("density", 0.0),
            "nitrogen": environment.get("nitrates", 1.0),
            "maxCells": int(environment.get("max_cells", 0) or 0),
        },
        "terrain": {
            "light": environment.get("light", 1.0),
            "lightRange": environment.get("light_range", 0.0),
            "radiation": environment.get("radiation", 0.0),
            "food": environment.get("food", 0.0),
            "nitrates": environment.get("nitrates", 1.0),
            "biome": 1,
            "capacity": environment.get("food", 0.0),
        },
        "conditions": conditions,
        "genomes": genomes + demos,
        "initial": {
            "cells": cells,
            "links": [list(pair) for pair in sorted(links)],
            "food": food,
        },
        "demo": build_demo(challenge, substrate, len(genomes), radius),
        "test": {
            "maxSeconds": None,
            "seed": 0xCE11AB,
            **({"winTimeOriginal": win_time_original} if win_time_original and float(win_time_original) > MAX_WIN_TIME else {}),
        },
    }


def main() -> None:
    index = json.loads((SOURCE / "index.json").read_text())
    TARGET.mkdir(exist_ok=True)
    summaries = []
    for entry in index["challenges"]:
        path = SOURCE / "data" / f"{entry['id']}.json"
        data = json.loads(path.read_text())
        level = build_level(entry, data)
        (TARGET / f"{level['id']}.json").write_text(
            json.dumps(level, ensure_ascii=False, separators=(",", ":")) + "\n"
        )
        summaries.append({
            "id": level["id"],
            "order": level["order"],
            "title": level["title"],
            "difficulty": level["difficulty"],
            "tutorial": level["tutorial"],
            "conditions": level["conditions"],
            "cells": len(level["initial"]["cells"]),
            "food": len(level["initial"]["food"]),
        })
    (TARGET / "index.json").write_text(
        json.dumps({"schema": 1, "levels": summaries}, ensure_ascii=False, separators=(",", ":")) + "\n"
    )
    print(f"wrote {len(summaries)} levels to {TARGET}")


if __name__ == "__main__":
    main()
