#!/usr/bin/env python3
"""Re-sync ``levels/*.json`` initial cells/food with the original substrates.

``build_levels.py`` regenerates a whole manifest (and therefore replaces
hand-authored demos).  This script only rebuilds ``initial`` from
``docs/original-game/challenges/data/<id>.json`` while keeping every other
field of the existing manifest intact.

Run it after changing the substrate coordinate conversion::

    python3 scripts/sync_level_substrates.py

Use ``--check`` to fail when a manifest is out of sync without writing files.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from build_levels import SOURCE, TARGET, build_level  # noqa: E402


def synced_initial(entry: dict) -> tuple[dict, dict]:
    path = SOURCE / "data" / f"{entry['id']}.json"
    data = json.loads(path.read_text())
    return data, build_level(entry, data)["initial"]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="report drift without writing")
    args = parser.parse_args()

    index = json.loads((SOURCE / "index.json").read_text())
    stale: list[str] = []
    for entry in index["challenges"]:
        level_path = TARGET / f"{entry['id']}.json"
        level = json.loads(level_path.read_text())
        _, initial = synced_initial(entry)
        if level.get("initial") == initial:
            continue
        stale.append(entry["id"])
        if not args.check:
            level["initial"] = initial
            level_path.write_text(
                json.dumps(level, ensure_ascii=False, separators=(",", ":")) + "\n"
            )

    if stale:
        verb = "out of sync" if args.check else "updated"
        print(f"{len(stale)} level(s) {verb}: {', '.join(stale)}")
        if args.check:
            raise SystemExit(1)
    else:
        print("all levels already match the original substrate data")


if __name__ == "__main__":
    main()
