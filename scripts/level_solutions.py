"""Apply the separately authored, tested campaign solutions during regeneration."""
from __future__ import annotations
import copy
import json
from pathlib import Path

REGISTRY = Path(__file__).resolve().parent.parent / "levels" / "solutions" / "index.json"


def apply_solution(level: dict, record: dict | None = None) -> dict:
    if record is None:
        records = json.loads(REGISTRY.read_text()) if REGISTRY.exists() else {}
        record = records.get(level["id"])
    record = copy.deepcopy(record or {
        "solutionStatus": "unavailable",
        "solutionIssue": "Честное решение этого уровня пока не подтверждено в текущем движке.",
        "solution": "", "genomes": [], "demo": [],
    })
    offset = len(level["genomes"])
    level["genomes"].extend(record.pop("genomes", []))
    actions = record.get("demo", [])
    for action in actions:
        if action["op"] == "spawn":
            action["genome"] += offset
    level.update(record)
    return level
