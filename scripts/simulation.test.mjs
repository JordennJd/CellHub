import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import { test } from "node:test";
import init, { Simulation } from "../frontend/wasm/simulation.js";

await init({
  module_or_path: await readFile(
    new URL("../frontend/wasm/simulation_bg.wasm", import.meta.url),
  ),
});
const genes = () =>
  new Float32Array(
    Array.from({ length: 8 }, () => [0, 2.4, 0, 0, 0.5, 12, 0.8, 240]).flat(),
  );

test("real WASM rejects invalid commands without corrupting the world", () => {
  const sim = new Simulation(42);
  try {
    assert.throws(() => sim.spawn(NaN, 100, 0, genes()), /Invalid spawn/);
    assert.throws(() => sim.spawn(100, 100, 8, genes()), /Invalid spawn/);
    assert.throws(
      () => sim.spawn(100, 100, 0, new Float32Array(3)),
      /64 finite/,
    );
    const invalid = genes();
    invalid[1] = Infinity;
    assert.throws(() => sim.spawn(100, 100, 0, invalid), /64 finite/);
    const id = sim.spawn(100, 100, 0, genes());
    assert.throws(() => sim.edit(999, 0, genes()), /no longer exists/);
    assert.equal(sim.stats()[1], 1);
    assert.equal(sim.snapshot()[0], id);
  } finally {
    sim.free();
  }
});

test("real WASM lifecycle produces transferred snapshots and reproducible descendants", () => {
  const a = new Simulation(73421),
    b = new Simulation(73421);
  try {
    for (const sim of [a, b]) {
      sim.spawn(300, 300, 0, genes());
      for (let i = 0; i < 12; i++) sim.advance(120);
    }
    const snapshot = a.snapshot();
    assert(snapshot instanceof Float32Array);
    assert(snapshot.length > 10);
    assert.equal(snapshot.length % 10, 0);
    assert.deepEqual(snapshot, b.snapshot());
    assert.equal(a.terrain().length, 48 * 48 * 4);
    assert.equal(a.stats()[0], 1440);
    const transferred = structuredClone(snapshot, {
      transfer: [snapshot.buffer],
    });
    assert.equal(snapshot.byteLength, 0);
    assert(transferred.length > 10);
    assert(a.snapshot().length > 10);
    a.reset();
    assert.equal(a.stats()[1], 0);
    assert.equal(a.stats()[0], 0);
  } finally {
    a.free();
    b.free();
  }
});
