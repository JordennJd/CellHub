"""Apply the separately authored, tested campaign solutions during regeneration.

A solution is untrusted content, not a patch for the level rules.  It may only
carry the solution metadata, the extra genomes and the demo track.  Everything
that defines the challenge (budget, tools, conditions, initial scene, ...) comes
from the original substrate and must never be overridden by the registry.

The demo track is also checked against the same limits as the Rust runtime.  An
invalid record is not applied: the level is regenerated as ``unavailable`` with
an explicit issue, so a too-generous automatic solution cannot silently reach
the game.
"""
from __future__ import annotations
import copy
import json
from pathlib import Path

REGISTRY = Path(__file__).resolve().parent.parent / "levels" / "solutions" / "index.json"

# Only these keys describe a solution.  Any other key would let the registry
# redefine the challenge itself (for example `budget: 300` next to a 300-spawn
# demo), which is exactly how an "automatic solution" could outgrow its limits.
# `test` only tunes the harness that plays the demo (time limit and seed); it is
# not a player-facing rule.
SOLUTION_KEYS = {"solutionStatus", "solutionIssue", "solution", "genomes", "demo", "test"}
TEST_KEYS = {"maxSeconds", "seed", "winTimeOriginal"}

# Same order as `CELL_TYPES` in `scripts/build_levels.py`.
CELL_TYPES = [
    "photocyte",
    "phagocyte",
    "flagellocyte",
    "virocyte",
    "keratinocyte",
    "devorocyte",
    "nitrocyte",
    "lipocyte",
    "buoyocyte",
    "glueocyte",
    "stereocyte",
    "senseocyte",
    "myocyte",
    "neurocyte",
    "secrocyte",
    "stemocyte",
    "gamete",
    "ciliocyte",
]
TOOL_FOR_OP = {"spawn": "place", "remove": "remove", "boost": "add", "move": "move"}


def unavailable(issue: str) -> dict:
    return {
        "solutionStatus": "unavailable",
        "solutionIssue": issue,
        "solution": "",
        "genomes": [],
        "demo": [],
    }


def _is_number(value) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def _genome_issues(level: dict, genome: dict, label: str) -> list[str]:
    issues: list[str] = []
    modes = genome.get("modes") if isinstance(genome, dict) else None
    if not isinstance(modes, list) or not modes:
        return [f"{label}: геном пуст или повреждён"]
    allowed = level.get("allowedKinds") or []
    counts: dict[int, int] = {}
    for mode in modes:
        if not isinstance(mode, dict):
            issues.append(f"{label}: режим генома повреждён")
            continue
        kind = mode.get("kind", 0)
        if not isinstance(kind, int) or not 0 <= kind < len(CELL_TYPES):
            issues.append(f"{label}: неизвестный тип клетки {kind}")
            continue
        counts[kind] = counts.get(kind, 0) + 1
        if allowed and kind not in allowed:
            issues.append(
                f"{label}: тип {CELL_TYPES[kind]} недоступен в испытании"
            )
    for name, maximum in (level.get("maxCellsOfType") or {}).items():
        key = name.lower().replace("-", "")
        if key not in CELL_TYPES:
            continue
        used = counts.get(CELL_TYPES.index(key), 0)
        if used > int(maximum):
            issues.append(
                f"{label}: режимов {name} {used}, допускается не более {maximum}"
            )
    return issues


def solution_issues(level: dict, demo: list, genomes: list) -> list[str]:
    """Return every limit violation in an already-expanded demo track."""
    issues: list[str] = []
    tools = set(level.get("tools") or [])
    budget = int(level.get("budget") or 0)
    remove_max = level.get("removeMax")
    all_genomes = list(level.get("genomes") or []) + list(genomes)
    placed = 0
    removed = 0
    previous = 0.0
    for index, action in enumerate(demo, 1):
        at = action.get("at") if isinstance(action, dict) else None
        op = action.get("op") if isinstance(action, dict) else None
        if not _is_number(at) or at < previous:
            issues.append(f"Действие {index}: некорректное время")
        else:
            previous = float(at)
        if op not in TOOL_FOR_OP:
            issues.append(f"Действие {index}: неизвестная операция {op!r}")
            continue
        tool = TOOL_FOR_OP[op]
        if tool not in tools:
            issues.append(f"Действие {index}: инструмент «{tool}» недоступен")
        if op == "spawn":
            if placed >= budget:
                issues.append(
                    f"Действие {index}: исчерпан лимит постановки клеток ({budget})"
                )
            mass = action.get("mass")
            if mass is not None and mass != 1:
                issues.append(
                    f"Действие {index}: начальная масса клетки должна быть 1"
                )
            x, y = action.get("x"), action.get("y")
            if not _is_number(x) or not _is_number(y) or not (
                0.0 <= x <= 1.0 and 0.0 <= y <= 1.0
            ):
                issues.append(f"Действие {index}: некорректные координаты")
            genome_index = action.get("genome")
            genome = (
                all_genomes[genome_index]
                if isinstance(genome_index, int) and 0 <= genome_index < len(all_genomes)
                else None
            )
            if genome is None:
                issues.append(f"Действие {index}: отсутствующий геном")
            else:
                issues.extend(
                    _genome_issues(level, genome, f"Действие {index}")
                )
                modes = genome.get("modes") or []
                mode = action.get("mode")
                if not isinstance(mode, int) or not 0 <= mode < len(modes):
                    issues.append(f"Действие {index}: отсутствующий режим")
            placed += 1
        elif op == "remove":
            if remove_max is not None and removed >= int(remove_max):
                issues.append(
                    f"Действие {index}: исчерпан лимит удалений ({remove_max})"
                )
            if action.get("id") is None:
                issues.append(f"Действие {index}: не указана клетка")
            removed += 1
        elif op == "boost":
            if action.get("id") is None:
                issues.append(f"Действие {index}: не указана клетка")
        elif op == "move":
            if action.get("id") is None:
                issues.append(f"Действие {index}: не указана клетка")
            x, y = action.get("x"), action.get("y")
            if not _is_number(x) or not _is_number(y) or not (
                0.0 <= x <= 1.0 and 0.0 <= y <= 1.0
            ):
                issues.append(f"Действие {index}: некорректное перемещение")
    return issues


def apply_solution(level: dict, record: dict | None = None) -> dict:
    if record is None:
        records = json.loads(REGISTRY.read_text()) if REGISTRY.exists() else {}
        record = records.get(level["id"])
    record = copy.deepcopy(record or unavailable(
        "Честное решение этого уровня пока не подтверждено в текущем движке."
    ))
    unknown = set(record) - SOLUTION_KEYS
    if unknown:
        raise ValueError(
            f"Решение уровня {level.get('id', '?')} пытается изменить правила испытания: "
            + ", ".join(sorted(unknown))
        )
    test = record.get("test")
    if test is not None:
        if not isinstance(test, dict):
            raise ValueError(
                f"Решение уровня {level.get('id', '?')}: test должен быть объектом"
            )
        unknown_test = set(test) - TEST_KEYS
        if unknown_test:
            raise ValueError(
                f"Решение уровня {level.get('id', '?')} пытается изменить параметры проверки: "
                + ", ".join(sorted(unknown_test))
            )

    genomes = copy.deepcopy(record.get("genomes") or [])
    demo = copy.deepcopy(record.get("demo") or [])
    if not isinstance(genomes, list) or not isinstance(demo, list):
        record = unavailable("Автоматическое решение повреждено: genomes/demo должны быть массивами.")
        genomes = []
        demo = []
    offset = len(level.get("genomes") or [])
    for action in demo:
        if isinstance(action, dict) and action.get("op") == "spawn":
            if isinstance(action.get("genome"), int):
                action["genome"] += offset

    status = record.get("solutionStatus")
    issues = solution_issues(level, demo, genomes)
    if status not in (None, "verified", "unavailable"):
        issues.insert(0, f"Неизвестный статус решения {status!r}")
    if demo and status != "verified":
        issues.insert(0, "Решение с автоматическими действиями должно иметь статус verified")
    if status == "unavailable" and demo:
        issues.insert(0, "Неподтверждённое решение не может содержать автоматических действий")
    if issues:
        record = unavailable(
            "Автоматическое решение не соответствует правилам испытания: "
            + issues[0]
            + (" (и ещё " + str(len(issues) - 1) + ")" if len(issues) > 1 else "")
        )
        genomes = []
        demo = []

    level["genomes"].extend(genomes)
    record.pop("genomes", None)
    record["demo"] = demo
    # `record` contains only solution metadata now; level rules are untouched.
    level.update(record)
    return level
