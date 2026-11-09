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
      /finite numbers/g,
    );
    const invalid = genes();
    invalid[1] = Infinity;
    assert.throws(() => sim.spawn(100, 100, 0, invalid), /finite numbers/g);
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


test("real WASM preserves variable genomes and transfers integer bond endpoints", () => {
  const sim = new Simulation(42);
  try {
    const g = new Float32Array([
      0, 1, 1, 2, 0.5, 0, 0, 1000, 1, 1, 0, 0,
      2, 8, 1, 1, 0.5, 30, 0, 1000, 0, 0, 0, 0,
      1, 8, 2, 0, 0.5, 0, 0, 1000, 1, 0, 1, 0,
    ]);
    const id = sim.spawn(300, 300, 0, g);
    assert.deepEqual(sim.genome(id), g);
    sim.advance(1);
    assert.equal(sim.stats()[1], 2);
    const links = sim.links();
    assert(links instanceof Uint32Array);
    assert.deepEqual(Array.from(links), [2, 3]);
    const transferred = structuredClone(links, { transfer: [links.buffer] });
    assert.equal(links.byteLength, 0);
    assert.deepEqual(Array.from(transferred), [2, 3]);
    assert.equal(sim.cell_mode(2), 1);
    assert.equal(sim.cell_mode(3), 2);
    for (let i = 0; i < 10; i++) sim.advance(30);
    assert.equal(sim.stats()[1], 2);
    assert.deepEqual(Array.from(sim.links()), [2, 3]);
    const rows = sim.snapshot();
    assert(Math.abs(Math.hypot(rows[1] - rows[11], rows[2] - rows[12]) - rows[3] - rows[13] - 2) < 0.01);
    const bad = g.slice();
    bad[2] = 40;
    assert.throws(() => sim.spawn(100, 100, 0, bad), /child reference/);
    bad[2] = 1;
    bad[9] = 0.5;
    assert.throws(() => sim.edit(2, 0, bad), /adhesion flag/);
    assert.deepEqual(Array.from(sim.links()), [2, 3]);
    sim.reset();
    assert.equal(sim.links().length, 0);
  } finally { sim.free(); }
});
