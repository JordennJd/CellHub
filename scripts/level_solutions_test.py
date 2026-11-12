"""Content regression: regeneration preserves rules and authored solutions."""
import copy
import json
import unittest
from build_levels import build_level, SOURCE, ROOT
from level_solutions import apply_solution, solution_issues, SOLUTION_KEYS, REGISTRY


def limits_level(budget=10):
    return {
        "id": "limits",
        "genomes": [{"version": 5, "modes": [{"kind": 1, "name": "Test"}]}],
        "tools": ["place", "select"],
        "budget": budget,
        "removeMax": None,
        "allowedKinds": [],
        "maxCellsOfType": {},
        "conditions": {"user-cells": 10},
        "initial": {"cells": [], "links": [], "food": []},
        "environment": {},
        "terrain": {},
    }


def spawn_demo(count, **extra):
    return [
        {"at": 0, "op": "spawn", "x": 0.5, "y": 0.5, "genome": 0, "mode": 0, **extra}
        for _ in range(count)
    ]


def solution_record(demo, **extra):
    return {
        "solutionStatus": "verified",
        "solution": "",
        # Local genome used by the demo; apply_solution offsets its index past
        # the level's own genomes.
        "genomes": [{"version": 5, "modes": [{"kind": 1, "name": "Solution"}]}],
        "demo": demo,
        **extra,
    }


class SolutionContentTests(unittest.TestCase):
    def test_regeneration_preserves_each_original_player_limit(self):
        registry = json.loads(REGISTRY.read_text())
        for entry in json.loads((SOURCE / "index.json").read_text())["challenges"]:
            source = json.loads((SOURCE / "data" / f"{entry['id']}.json").read_text())
            rebuilt = build_level(entry, source)
            current = json.loads((ROOT / "levels" / f"{entry['id']}.json").read_text())
            for field in ["tools", "budget", "removeMax", "maxCellsOfType", "allowedKinds", "initial", "environment", "terrain", "conditions"]:
                self.assertEqual(current[field], rebuilt[field], (entry["id"], field))
            if current["tutorial"]:
                continue
            record = registry[entry["id"]]
            self.assertEqual(current["solutionStatus"], rebuilt["solutionStatus"])
            self.assertEqual(len(current["demo"]), len(rebuilt["demo"]))
            for actual, expected in zip(current["demo"], rebuilt["demo"]):
                self.assertEqual({k:v for k,v in actual.items() if k != "genome"}, {k:v for k,v in expected.items() if k != "genome"})
                if actual["op"] == "spawn":
                    self.assertEqual(current["genomes"][actual["genome"]], rebuilt["genomes"][expected["genome"]])
            if record["solutionStatus"] == "unavailable":
                self.assertEqual(current["demo"], [])
                self.assertTrue(current["solutionIssue"])

    def test_solution_application_does_not_modify_the_registry(self):
        registry = json.loads(REGISTRY.read_text())
        before = copy.deepcopy(registry)
        apply_solution({"id":"1", "genomes":[]}, registry["1"])
        apply_solution({"id":"1", "genomes":[{}]}, registry["1"])
        self.assertEqual(registry, before)

    def test_unknown_level_never_gets_a_fabricated_solution(self):
        level = apply_solution({"id":"new-level", "genomes":[]})
        self.assertEqual(level["solutionStatus"], "unavailable")
        self.assertEqual(level["demo"], [])

    def test_solution_record_cannot_override_level_rules(self):
        level = limits_level()
        record = solution_record(spawn_demo(300), budget=300)
        with self.assertRaises(ValueError):
            apply_solution(copy.deepcopy(level), record)
        for key in ["conditions", "tools", "removeMax", "allowedKinds", "initial", "environment", "terrain"]:
            bad = solution_record(spawn_demo(1))
            bad[key] = level[key]
            with self.assertRaises(ValueError, msg=key):
                apply_solution(copy.deepcopy(level), bad)

    def test_solution_record_cannot_override_test_rules(self):
        level = limits_level()
        record = solution_record(spawn_demo(1), test={"maxSeconds": 120, "unknown": 1})
        with self.assertRaises(ValueError):
            apply_solution(copy.deepcopy(level), record)

    def test_over_budget_automatic_solution_is_disabled(self):
        level = limits_level()
        rebuilt = apply_solution(copy.deepcopy(level), solution_record(spawn_demo(300)))
        self.assertEqual(rebuilt["solutionStatus"], "unavailable")
        self.assertEqual(rebuilt["demo"], [])
        self.assertIn("лимит постановки", rebuilt["solutionIssue"])
        self.assertEqual(rebuilt["budget"], level["budget"])

    def test_nonstandard_mass_tool_and_removal_limits_are_disabled(self):
        mass_level = limits_level()
        removal_level = limits_level()
        removal_level["tools"].append("remove")
        removal_level["removeMax"] = 1
        missing_tool_level = limits_level()
        missing_tool_level["tools"] = ["select"]
        for level, demo in [
            (mass_level, spawn_demo(1, mass=1.4)),
            (removal_level, [
                {"at": 0, "op": "remove", "id": 1},
                {"at": 0, "op": "remove", "id": 2},
            ]),
            (missing_tool_level, spawn_demo(1)),
        ]:
            rebuilt = apply_solution(copy.deepcopy(level), solution_record(demo))
            self.assertEqual(rebuilt["solutionStatus"], "unavailable", demo)
            self.assertEqual(rebuilt["demo"], [], demo)
            self.assertTrue(rebuilt["solutionIssue"], demo)

    def test_registered_solutions_only_contain_solution_metadata(self):
        for level_id, record in json.loads(REGISTRY.read_text()).items():
            self.assertLessEqual(set(record), SOLUTION_KEYS, level_id)
            if record["solutionStatus"] == "unavailable":
                self.assertEqual(record["demo"], [], level_id)
                self.assertTrue(record["solutionIssue"], level_id)

    def test_every_regenerated_demo_obeys_the_manifest_limits(self):
        for entry in json.loads((SOURCE / "index.json").read_text())["challenges"]:
            source = json.loads((SOURCE / "data" / f"{entry['id']}.json").read_text())
            level = build_level(entry, source)
            self.assertEqual(
                solution_issues(level, level["demo"], []), [], entry["id"]
            )


if __name__ == "__main__":
    unittest.main()
