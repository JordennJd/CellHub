import assert from "node:assert/strict";
import { test } from "node:test";
import { defaultGenome, newMode, multicellularGenome, flatten, inflate, parseGenome, serializeGenome, removeMode } from "../frontend/model.ts";

test("archetype settings and all adhesion flags survive encoding and saving", () => {
  const modes = multicellularGenome();
  modes.push({ ...newMode(3), bindChildren: true, keepAdhesionA: true, keepAdhesionB: true });
  assert.deepEqual(inflate(flatten(modes)), modes);
  assert.deepEqual(parseGenome(serializeGenome(modes)), modes);
  assert.equal(modes[1].canDivide, false);
  assert.equal(modes[2].keepAdhesionA, true);
  assert.equal(modes[2].keepAdhesionB, false);
  assert.equal(modes[2].bindChildren, false);
});

test("legacy eight-mode saves migrate without adding bonds", () => {
  const legacy = defaultGenome().map(({ canDivide, bindChildren, keepAdhesionA, keepAdhesionB, ...m }) => m);
  const loaded = parseGenome(JSON.stringify(legacy));
  assert.deepEqual(loaded, defaultGenome());
  assert.deepEqual(inflate(new Float32Array(legacy.flatMap(Object.values))), loaded);
});

test("genomes accept 1–40 archetypes and reject broken references and flags", () => {
  for (const n of [1, 3, 40]) {
    const g = Array.from({ length: n }, (_, i) => newMode(i));
    assert.equal(parseGenome(serializeGenome(g)).length, n);
    assert.equal(inflate(flatten(g)).length, n);
  }
  for (const invalid of [[], Array.from({ length: 41 }, (_, i) => newMode(i)),
    [{ ...newMode(0), childA: 1 }], [{ ...newMode(0), childB: -1 }],
    [{ ...newMode(0), kind: 0.5 }], [{ ...newMode(0), bindChildren: 1 }],
    [{ ...newMode(0), canDivide: undefined }], [{ ...newMode(0), speed: NaN }]]) {
    assert.throws(() => parseGenome(JSON.stringify({ version: 2, modes: invalid })));
  }
  assert.throws(() => parseGenome('{"version":3,"modes":[]}'));
});

test("deleting an archetype remaps incoming references and permits cycles", () => {
  const g = multicellularGenome();
  const removed = removeMode(g, 1);
  assert.equal(removed.length, 2);
  assert.deepEqual(removed.map((m) => [m.childA, m.childB]), [[1, 1], [1, 0]]);
  const last = removeMode(g, 2);
  assert.deepEqual(last.map((m) => [m.childA, m.childB]), [[1, 1], [1, 1]]);
  const only = [newMode(0)];
  assert.equal(removeMode(only, 0), only);
  assert.equal(g.length, 3);
  assert.deepEqual(parseGenome(serializeGenome(removed)), removed);
});

test("stress populations stay within the world without clamping or duplicate positions", async () => {
  const { initialPopulation, POPULATION_LIMIT, WORLD } = await import("../frontend/model.ts");
  for (const count of [1, 60, 5000, POPULATION_LIMIT]) {
    const points = initialPopulation(count);
    assert.equal(points.length, count);
    assert.equal(new Set(points.map(({x,y}) => `${x}/${y}`)).size, count);
    assert(points.every(({x,y,mode}) => x > 12 && x < WORLD - 12 && y > 12 && y < WORLD - 12 && mode >= 0 && mode < 6));
  }
  assert.equal(initialPopulation(POPULATION_LIMIT + 1).length, POPULATION_LIMIT);
  assert.equal(initialPopulation(2.9).length, 2);
  for (const count of [0, -1, NaN, Infinity]) assert.deepEqual(initialPopulation(count), []);
});
