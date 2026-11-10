import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import { test } from "node:test";
import ts from "typescript";
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

test("real WASM multicellular nutrition sustains specialists and keratin blocks infection", () => {
  const sim = new Simulation(42);
  const mode = (kind, child = 0) => [kind, 8, child, child, .5, 0, 0, 1000, 0, 0, 0, 0];
  try {
    sim.configure(2, 2, 0, 1, false, false);
    const organism = new Float32Array([
      0, 1, 1, 2, .5, 0, 0, 1000, 1, 1, 0, 0,
      ...mode(0, 1), ...mode(2, 2),
    ]);
    sim.spawn(500, 500, 0, organism);
    const alone = sim.spawn(1500, 1500, 0, new Float32Array(mode(2)));
    for (let i = 0; i < 15; i++) sim.advance(120);
    const cells = sim.snapshot();
    assert.equal(cells.length, 24);
    assert.equal(sim.links().length, 2);
    assert(![cells[0], cells[12]].includes(alone), "unfed specialist starves");
    const flagRow = cells[4] === 2 ? 0 : 12;
    assert(cells[flagRow + 5] > 1, "attached specialist receives photosynthetic energy");

    sim.reset(); sim.configure(2, 2, 0, 1, false, false);
    const shielded = new Float32Array([
      0, 1, 1, 2, .5, 0, 0, 1000, 1, 1, 0, 0,
      ...mode(4, 1), ...mode(0, 2),
    ]);
    sim.spawn(500, 500, 0, shielded); sim.advance(15);
    const pair = sim.snapshot();
    assert.equal(pair.length, 24);
    sim.spawn(pair[13], pair[14], 0, new Float32Array(mode(3)));
    sim.advance(1);
    assert.equal(sim.stats()[4], 0, "bonded keratin prevents infection");
    sim.remove(2); sim.advance(1);
    assert.equal(sim.stats()[4], 1, "removing the shield exposes its former neighbor");
  } finally { sim.free(); }
});

test("saved world JSON restores exact state and deterministic continuation", () => {
  const original = new Simulation(73421), restored = new Simulation(2);
  try {
    original.configure(1.5, 2, 0.1, 1.2, false, true);
    original.spawn(500, 500, 0, genes());
    original.advance(120);
    const json = JSON.stringify({version: 1, values: Array.from(original.export_state())});
    restored.restore_state(new Float64Array(JSON.parse(json).values));
    assert.deepEqual(restored.export_state(), original.export_state());
    for (let i = 0; i < 4; i++) { original.advance(120); restored.advance(120); }
    assert.deepEqual(restored.export_state(), original.export_state());
    const before = restored.export_state();
    for (const invalid of [[], [99], Array.from(before.slice(0, -1)), [...before, 1]]) {
      assert.throws(() => restored.restore_state(new Float64Array(invalid)));
      assert.deepEqual(restored.export_state(), before);
    }
    const nonfinite = before.slice(); nonfinite[8] = NaN;
    assert.throws(() => restored.restore_state(nonfinite));
    assert.deepEqual(restored.export_state(), before);
  } finally { original.free(); restored.free(); }
});

test("laboratory tools and substrate settings work through the real WASM ABI", () => {
  const sim = new Simulation(73421);
  try {
    sim.configure(2, 3, 0, 2, false, true, true, 1, -1);
    assert.deepEqual(Array.from(sim.environment()), [2, 3, 0, 2, 0, 1, 1, 1, -1]);
    const before = sim.export_state();
    assert.throws(() => sim.configure(1, 1, 0, 0, true, false));
    assert.deepEqual(sim.export_state(), before);
    const id = sim.spawn(500, 500, 0, genes());
    sim.advance(10);
    const mass = sim.snapshot()[5];
    sim.move_cell(id, 650, 700);
    sim.boost(id);
    assert.equal(sim.snapshot()[1], 650);
    assert.equal(sim.snapshot()[2], 700);
    assert(Math.abs(sim.snapshot()[5] - mass - 1) < 0.00001);
    const terrain = sim.terrain(), tick = sim.stats()[0];
    sim.sterilize();
    assert.equal(sim.stats()[1], 0);
    assert.equal(sim.stats()[0], tick);
    assert.deepEqual(sim.terrain(), terrain);
    const another = sim.spawn(300, 400, 0, genes());
    sim.remove(another);
    assert.throws(() => sim.remove(another));
    assert.equal(sim.snapshot().length, 0);
    sim.reset();
    assert.equal(sim.stats()[0], 0);
    assert.deepEqual(Array.from(sim.environment()), [1, 1, 1, 1, 1, 0, 0, 0, 0]);
  } finally { sim.free(); }
});

test("v3 daughter orientation, mirroring and RGB survive the real WASM save ABI", () => {
  const sim = new Simulation(42), restored = new Simulation(1);
  try {
    const g = new Float32Array([-3, 1,
      0, 1, 0, 0, 0.5, 0, 0.2, 1000, 1, 0, 0, 0, 0.3, -0.4, 1, 0, 0.2, 0.6, 1,
    ]);
    sim.configure(1, 1, 0, 1, true, false, true, 0.5, -0.5);
    const id = sim.spawn(500, 500, 0, g);
    assert.deepEqual(sim.genome(id), g);
    sim.boost(id);
    sim.advance(14);
    assert.equal(sim.stats()[1], 1);
    sim.advance(1);
    const frame = sim.snapshot();
    assert.equal(frame.length, 24);
    assert.equal(frame[11], 1);
    assert.equal(frame[23], 0);
    assert.equal(frame[10], 0x3399ff);
    assert.equal(frame[22], 0x3399ff);
    assert(Math.abs(frame[21] - frame[9] - Math.PI + 0.1) < 0.00001);
    restored.restore_state(sim.export_state());
    assert.deepEqual(restored.snapshot(), frame);
    assert.deepEqual(restored.environment(), sim.environment());
    for (let i = 0; i < 4; i++) { sim.advance(120); restored.advance(120); }
    assert.deepEqual(restored.export_state(), sim.export_state());
    const bad = g.slice(); bad[2 + 14] = 0.5;
    assert.throws(() => sim.spawn(500, 500, 0, bad), /mirror flag/);
    bad[2 + 14] = 1; bad[2 + 16] = -1;
    assert.throws(() => sim.spawn(500, 500, 0, bad), /color/);
  } finally { sim.free(); restored.free(); }
});

test("worker acknowledges ordered resets and correlates failed restore commands", async () => {
  const workerSource = await readFile(new URL("../frontend/simulation.worker.ts", import.meta.url), "utf8");
  const wasm = await readFile(new URL("../frontend/wasm/simulation_bg.wasm", import.meta.url));
  const workerJs = ts.transpileModule(workerSource, {compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext,
  }}).outputText
    .replace('"./wasm/simulation.js"', JSON.stringify(new URL("../frontend/wasm/simulation.js", import.meta.url).href))
    .replace("await init();", `await init({module_or_path:new Uint8Array(${JSON.stringify(Array.from(wasm))})});`);
  const previousSelf = globalThis.self, previousInterval = globalThis.setInterval;
  const messages = [];
  let tick;
  const context = {postMessage: message => messages.push(message), onmessage: null};
  globalThis.self = context;
  globalThis.setInterval = callback => { tick = callback; return 0; };
  try {
    await import(`data:text/javascript;base64,${Buffer.from(workerJs).toString("base64")}`);
    assert(messages.some(message => message.type === "ready"));
    messages.length = 0;
    const send = data => context.onmessage({data});
    send({type:"spawn", x:500, y:500, mode:0, genome:genes()});
    send({type:"reset", requestId:"reset-one"});
    send({type:"spawn", x:500, y:500, mode:0, genome:genes()});
    send({type:"reset", requestId:"reset-two"});
    tick();
    assert.deepEqual(messages.map(message => message.type), ["spawned", "reset", "spawned", "reset", "frame"]);
    assert.deepEqual(messages.filter(message => message.type === "reset").map(message => message.requestId), ["reset-one", "reset-two"]);
    assert.equal(messages.at(-1).stats[1], 0);
    messages.length = 0;
    send({type:"restore", requestId:"load-invalid", state:'{"version":99,"values":[]}'});
    tick();
    assert.equal(messages[0].type, "error");
    assert.equal(messages[0].command, "restore");
    assert.equal(messages[0].requestId, "load-invalid");
    send({type:"export", requestId:"save-one"});
    tick();
    const exported = messages.find(message => message.type === "export");
    assert.equal(exported.requestId, "save-one");
    send({type:"restore", requestId:"load-valid", state:exported.state});
    tick();
    const restored = messages.find(message => message.type === "restored");
    assert.equal(restored.requestId, "load-valid");
    assert.equal(restored.settings.viscosity, 1);
  } finally {
    globalThis.self = previousSelf;
    globalThis.setInterval = previousInterval;
  }
});

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
    assert(snapshot.length > 12);
    assert.equal(snapshot.length % 12, 0);
    assert.deepEqual(snapshot, b.snapshot());
    assert.equal(a.terrain().length, 48 * 48 * 4);
    assert.equal(a.stats()[0], 1440);
    const transferred = structuredClone(snapshot, {
      transfer: [snapshot.buffer],
    });
    assert.equal(snapshot.byteLength, 0);
    assert(transferred.length > 12);
    assert(a.snapshot().length > 12);
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
    assert.deepEqual(Array.from(sim.genome(id)), [-3, 3, ...Array.from({length:3}, (_,i)=>[...g.slice(i*12,i*12+12),0,0,0,0,-1,-1,-1]).flat()]);
    sim.advance(14);
    assert.equal(sim.stats()[1], 1);
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
    assert(Math.abs(Math.hypot(rows[1] - rows[13], rows[2] - rows[14]) - rows[3] - rows[15] - 2) < 0.01);
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
