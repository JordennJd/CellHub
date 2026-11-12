"""Content regression: regeneration preserves rules and authored solutions."""
import copy
import json
import unittest
from build_levels import build_level, SOURCE, ROOT
from level_solutions import apply_solution, REGISTRY


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


if __name__ == "__main__":
    unittest.main()
