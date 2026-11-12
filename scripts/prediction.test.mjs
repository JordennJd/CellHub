import assert from "node:assert/strict";
import { test, after } from "node:test";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import ts from "typescript";

// Exercise the actual client modules without requiring a browser or server.
const directory = await mkdtemp(join(tmpdir(), "cellhub-prediction-tests-"));
for (const [source, target] of [
  ["frontend/model.ts", "model.mjs"],
  ["frontend/net/protocol.ts", "protocol.mjs"],
  ["frontend/net/prediction.ts", "prediction.mjs"],
  ["frontend/net/contacts.ts", "contacts.mjs"],
  ["frontend/net/client.ts", "client.mjs"],
  ["frontend/net/profiling.ts", "profiling.mjs"],
]) {
  const original = await readFile(new URL(`../${source}`, import.meta.url), "utf8");
  const output = ts.transpileModule(original, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText.replaceAll('"../model"', '"./model.mjs"').replaceAll('"./protocol"', '"./protocol.mjs"').replaceAll('"./prediction"', '"./prediction.mjs"').replaceAll('"./profiling"', '"./profiling.mjs"').replaceAll('"./contacts"', '"./contacts.mjs"');
  await writeFile(join(directory, target), output);
}
after(() => rm(directory, { recursive: true, force: true }));
const { PredictionWorld } = await import(pathToFileURL(join(directory, "prediction.mjs")));
const { CELL_STRIDE, RENDER_STRIDE } = await import(pathToFileURL(join(directory, "protocol.mjs")));

test("network bond prediction keeps either anchored endpoint fixed and skips two anchors", () => {
  for (const [anchorA, anchorB] of [[true, false], [false, true], [true, true]]) {
    const cells = new Float32Array(2 * CELL_STRIDE);
    for (const [index, anchored] of [anchorA, anchorB].entries()) {
      const start = index * CELL_STRIDE;
      cells.set([index + 1, 100 + index * 60, 100, 10, 0, index + 1], start);
      cells[start + 11] = 1; // Bonded; authoritative propulsion speed is zero.
      cells[start + 27] = Number(anchored); // details[12] after eight signals.
    }
    const world = new PredictionWorld();
    world.applyChunk({
      tick: 1, population: 2, births: 0, deaths: 0, infections: 0,
      chunkX: 0, chunkY: 0, cells, links: new Uint32Array([1, 2]),
      terrain: new Float32Array(),
    }, 1000);
    world.tick(1 / 30, 1033);
    const frame = world.frame().cells;
    const positions = new Map(Array.from({ length: 2 }, (_, index) => {
      const start = index * RENDER_STRIDE;
      return [frame[start], [frame[start + 1], frame[start + 2]]];
    }));
    if (anchorA) assert.deepEqual(positions.get(1), [100, 100], "first anchor stays fixed");
    if (anchorB) assert.deepEqual(positions.get(2), [160, 100], "second anchor stays fixed");
    if (!anchorA) assert.deepEqual(positions.get(1), [138, 100], "free first endpoint takes the full correction");
    if (!anchorB) assert.deepEqual(positions.get(2), [122, 100], "free second endpoint takes the full correction");
  }
});

test("local placement ghost appears immediately and yields to the authoritative cell", () => {
  const cells = new Float32Array(2 * CELL_STRIDE);
  cells.set([1, 100, 100, 10, 0, 1], 0);
  cells.set([2, 160, 100, 10, 0, 1], CELL_STRIDE);
  const world = new PredictionWorld();
  world.applyChunk({
    tick: 1, population: 2, births: 0, deaths: 0, infections: 0,
    chunkX: 0, chunkY: 0, cells, links: new Uint32Array([1, 2]),
    terrain: new Float32Array(),
  }, 1000);
  const genome = new Float32Array(2 + 102);
  genome[2] = 0; // Photocyte.
  genome[7] = 0.5; // Mode speed.
  genome[18] = -1; // Default archetype colour.
  world.predictSpawn(7, 50, 60, 0, genome, 3, 1100);
  const idsOf = frame => Array.from(frame.cells)
    .filter((_, index) => index % RENDER_STRIDE === 0);
  assert(idsOf(world.frame()).includes(-1), "the pending spawn is visible before its ack");
  world.applyAck(7, 9);
  const next = new Float32Array(3 * CELL_STRIDE);
  next.set(cells);
  next.set([9, 50, 60, 10, 0, 1], 2 * CELL_STRIDE);
  world.applyChunk({
    tick: 2, population: 3, births: 1, deaths: 0, infections: 0,
    chunkX: 0, chunkY: 0, cells: next,
    links: new Uint32Array([1, 2]), terrain: new Float32Array(),
  }, 1200);
  const ids = idsOf(world.frame());
  assert(!ids.includes(-1), "the ghost is gone once the real cell arrives");
  assert(ids.includes(9));
});

test("v6 decoder handles dense/sparse rows and rejects malformed masks and lengths", async () => {
  const { decodeChunk } = await import(pathToFileURL(join(directory, "protocol.mjs")));
  for (const sparse of [false, true]) {
    const values = new Float32Array(CELL_STRIDE);
    values[0] = 123;
    values[1] = -0;
    values[2] = 100.25;
    values[25] = -1;
    const bits = new Uint32Array(values.buffer);
    let mask = 0;
    for (let i = 0; i < CELL_STRIDE; i++) if (bits[i]) mask |= 1 << i;
    const count = sparse ? 4 : CELL_STRIDE;
    const bytes = new ArrayBuffer(31 + (sparse ? 4 : 0) + count * 4);
    const view = new DataView(bytes);
    view.setUint8(0, 2);
    view.setUint16(23, 1, true);
    view.setUint8(29, Number(sparse));
    let offset = 31;
    if (sparse) { view.setUint32(offset, mask, true); offset += 4; }
    for (let i = 0; i < CELL_STRIDE; i++) {
      if (!sparse || bits[i]) { view.setUint32(offset, bits[i], true); offset += 4; }
    }
    const decoded = decodeChunk(bytes);
    assert(decoded);
    assert.deepEqual(new Uint32Array(decoded.cells.buffer), bits);
    assert.equal(decodeChunk(bytes.slice(0, -1)), null);
    assert.equal(decodeChunk(new Uint8Array([...new Uint8Array(bytes), 0]).buffer), null);
    if (sparse) {
      view.setUint32(31, 0xf0000000, true);
      assert.equal(decodeChunk(bytes), null);
    }
  }
});

test("large colonies bound prediction work and retain authoritative linked positions", () => {
  const cells = new Float32Array(2 * CELL_STRIDE);
  cells.set([1, 100, 100, 10, 0, 1], 0);
  cells.set([2, 160, 100, 10, 0, 1], CELL_STRIDE);
  for (const i of [0, CELL_STRIDE]) { cells[i + 10] = 60; cells[i + 11] = 1; }
  const links = new Uint32Array(12_001 * 2);
  // Distinct link keys ensure that the budget sees every edge.
  for (let i = 0; i < 12_001; i++) { links[i * 2] = 1; links[i * 2 + 1] = i + 2; }
  const world = new PredictionWorld();
  world.applyChunk({ tick: 1, population: 2, births: 2, deaths: 0, infections: 0,
    chunkX: 1, chunkY: 0, cells, links, terrain: new Float32Array() }, 0);
  world.tick(.1, 100);
  assert.equal(world.frame().cells[1], 100);
  assert.equal(world.frame().cells[RENDER_STRIDE + 1], 160);
});


test("slow client frames leave idle time and closing during a frame cancels scheduling", async () => {
  const { MultiplayerClient } = await import(pathToFileURL(join(directory, "client.mjs")));
  const originals = new Map(["window", "WebSocket", "location", "performance"].map(key =>
    [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  let now = 0, next = 0, socket, closeDuringFrame = false, rendered;
  const timers = new Map();
  const setGlobal = (key, value) => Object.defineProperty(globalThis, key, { configurable: true, writable: true, value });
  setGlobal("performance", { now: () => now });
  setGlobal("location", { search: "" });
  setGlobal("window", {
    setTimeout(fn, delay) { const id = ++next; timers.set(id, { fn, delay }); return id; },
    clearTimeout(id) { timers.delete(id); },
  });
  setGlobal("WebSocket", class {
    static OPEN = 1;
    readyState = 1;
    constructor() { socket = this; }
    send() {}
    close() {}
  });
  const callbacks = Object.fromEntries(["onPhase", "onRooms", "onWelcome", "onInspect", "onError",
    "onNotice", "onWorldReset", "onPlayerCount", "onLeaderboard", "onDisconnected"].map(key => [key, () => {}]));
  const client = new MultiplayerClient({ ...callbacks, onFrame(frame) {
    rendered = frame;
    now += 50;
    if (closeDuringFrame) client.close();
  } });
  try {
    client.connect("ws://local-test/ws");
    socket.onopen();
    socket.onmessage({ data: JSON.stringify({ type: "welcome", protocol: 6 }) });
    const runNext = () => {
      const [id, timer] = timers.entries().next().value;
      timers.delete(id); now += timer.delay; timer.fn();
    };
    // A whole burst for the same region must be coalesced before decoding.
    for (let tick = 1; tick <= 100; tick++) {
      const buffer = new ArrayBuffer(31 + CELL_STRIDE * 4), view = new DataView(buffer);
      view.setUint8(0, 2); view.setUint32(1, tick, true); view.setUint32(5, 1, true);
      view.setUint16(23, 1, true);
      [1, tick + 100, 100, 10, 2, 1].forEach((v,i) => view.setFloat32(31 + i * 4, v, true));
      socket.onmessage({data:buffer});
    }
    assert.equal(client.pendingChunks.size, 1, 'packet backlog is bounded by region count');
    runNext();
    assert.equal(rendered.cells[1], 200, 'the first display frame uses the newest packet');
    assert.equal(rendered.stats[0], 100);
    assert.equal(client.pendingChunks.size, 0);
    assert.equal(timers.size, 1);
    assert(Math.abs(timers.values().next().value.delay - 50) < 1e-9);
    closeDuringFrame = true;
    runNext();
    assert.equal(timers.size, 0);
  } finally {
    client.close();
    for (const [key, descriptor] of originals) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  }
});

function scene(rows, tick = 1, chunkX = 0, links = [], chunkY = 0) {
  const cells = new Float32Array(rows.length * CELL_STRIDE);
  rows.forEach((row, i) => {
    const p = i * CELL_STRIDE;
    cells.set([row.id, row.x, row.y ?? 100, row.radius ?? 10, 2, row.mass ?? 1], p);
    cells[p + 9] = row.angle ?? 0;
    cells[p + 10] = row.speed ?? 0;
    cells[p + 11] = Number(links.includes(row.id));
    cells[p + 27] = Number(row.anchored ?? false);
  });
  return { tick, population: rows.length, births: rows.length, deaths: 0, infections: 0,
    chunkX, chunkY, cells, links: Uint32Array.from(links), terrain: new Float32Array() };
}
/** Deliver every region of one authoritative tick, as the server cycle does. */
function coverTick(world, tick, rowsAt = () => [], now = tick * 100) {
  for (let y = 0; y < 6; y++) for (let x = 0; x < 6; x++)
    world.applyChunk(scene(rowsAt(x, y) ?? [], tick, x, [], y), now);
}
const positions = world => {
  const result = new Map(), cells = world.frame().cells;
  for (let i = 0; i < cells.length; i += RENDER_STRIDE) result.set(cells[i], { x: cells[i + 1], y: cells[i + 2] });
  return result;
};

test('predicted flagellocytes stop at an anchored cell, including complete swept crossings', () => {
  for (const speed of [60, 1000]) for (const blocker of [125, 115]) {
    const world = new PredictionWorld();
    world.applyChunk(scene([{ id: 1, x: 100, speed }, { id: 2, x: blocker, anchored: true }]), 0);
    world.tick(.2, 200);
    const p = positions(world);
    assert(p.get(1).x <= blocker - 20 + .001, 'a fast cell cannot tunnel through the blocker');
    assert.equal(p.get(2).x, blocker, 'the anchored blocker does not move');
  }
});

test('unanchored contacts share displacement by mass and preserve cell order', () => {
  const world = new PredictionWorld();
  world.applyChunk(scene([{ id: 1, x: 100, speed: 1000 }, { id: 2, x: 125, mass: 3 }]), 0);
  world.tick(.2, 200);
  const p = positions(world);
  assert(p.get(2).x - p.get(1).x >= 19.999);
  assert(Math.abs(p.get(1).x + 3 * p.get(2).x - (100 + 200 + 3 * 125)) < .001, 'contact preserves centre of mass');
});

test('swept contacts include neighbours across spatial bucket and network chunk borders', () => {
  const world = new PredictionWorld();
  world.applyChunk(scene([{ id: 1, x: 498, speed: 1000 }], 1, 0), 0);
  world.applyChunk(scene([{ id: 2, x: 523, anchored: true }], 1, 1), 0);
  world.tick(.2, 200);
  assert(positions(world).get(1).x <= 503.001);
});

test('receipt clocks avoid drift on slow or uneven frames and cap disconnect extrapolation', () => {
  const slow = new PredictionWorld(), fast = new PredictionWorld();
  for (const w of [slow, fast]) w.applyChunk(scene([{ id: 1, x: 100, speed: 60 }]), 0);
  slow.tick(.25, 250);
  for (let t = 10; t <= 250; t += 10) fast.tick(.01, t);
  assert(Math.abs(positions(slow).get(1).x - positions(fast).get(1).x) < .001);
  slow.tick(30, 30250);
  assert.equal(positions(slow).get(1).x, 142, 'prediction stops after one sync cycle plus one tick without authority');
  slow.applyChunk(scene([{ id: 1, x: 500, speed: 60 }], 1000), 30250);
  slow.tick(.01, 30260);
  assert(Math.abs(positions(slow).get(1).x - 500.6) < .001, 'new authority immediately discards elapsed time');
});

test('prediction never divides, ages or creates descendants; server births and pruning remain authoritative', () => {
  const world = new PredictionWorld();
  world.applyChunk(scene([{ id: 1, x: 100, speed: 60 }]), 0);
  for (let i = 1; i <= 300; i++) world.tick(1 / 30, i * 1000 / 30);
  assert.equal(world.cellCount, 1);
  assert.equal(world.frame().cells[6], 0, 'age is server-owned');
  assert.equal(world.frame().cells[7], 0, 'generation is server-owned');
  coverTick(world, 8, (x, y) => x === 0 && y === 0 ? [{ id: 2, x: 100 }, { id: 3, x: 120 }] : [], 10000);
  assert.deepEqual([...positions(world).keys()], [2, 3], 'old parent is pruned when a complete snapshot omits it');
});

test('a parent phantom cannot survive a complete snapshot, but partial regions cannot prove death', () => {
  const world = new PredictionWorld();
  world.applyChunk(scene([{ id: 1, x: 100, speed: 60 }], 1), 0);
  world.applyChunk(scene([{ id: 2, x: 100 }, { id: 3, x: 120 }], 2), 100);
  assert(positions(world).has(1), 'one refreshed region cannot confirm that the parent is gone');
  coverTick(world, 2, (x, y) => x === 0 && y === 0 ? [{ id: 2, x: 100 }, { id: 3, x: 120 }] : [], 200);
  assert.deepEqual([...positions(world).keys()], [2, 3], 'the parent disappears together with its daughters');
});

test('cells crossing region borders survive a complete snapshot', () => {
  const world = new PredictionWorld();
  world.applyChunk(scene([{ id: 1, x: 498 }], 1, 0), 0);
  coverTick(world, 2, (x, y) => x === 1 && y === 0 ? [{ id: 1, x: 523 }] : [], 100);
  assert(positions(world).has(1), 'moving between regions is not death');
});

test('out-of-date chunks never rewind moved cells or global stats', () => {
  const world = new PredictionWorld();
  world.applyChunk(scene([{ id: 1, x: 600 }], 10, 1), 0);
  world.applyChunk(scene([{ id: 1, x: 400 }], 9, 0), 10);
  assert.equal(positions(world).get(1).x, 600);
  assert.equal(world.frame().stats[0], 10);
  world.applyChunk(scene([{ id: 1, x: 590 }], 8, 1), 20);
  assert.equal(positions(world).get(1).x, 600);
});

test('crowded prediction exhausts a bounded contact budget without advancing through blockers', () => {
  const world = new PredictionWorld();
  const rows = Array.from({length:7000}, (_, i) => ({ id:i + 1, x:100, speed:60 }));
  world.applyChunk(scene(rows), 0);
  world.tick(.2, 200);
  const cells = world.frame().cells;
  assert.equal(world.cellCount, 7000);
  for (let i = 0; i < cells.length; i += RENDER_STRIDE) assert.equal(cells[i + 1], 100);
});

test('7000 distributed cells predict within a measured, bounded frame workload', t => {
  const world = new PredictionWorld();
  const rows = Array.from({length:7000}, (_, i) => ({id:i + 1, x:30 + i % 100 * 29, y:30 + Math.floor(i / 100) * 29, speed:30}));
  const samples = [];
  for (let i = 0; i < 20; i++) {
    world.applyChunk(scene(rows, i + 1), i * 33);
    const started = performance.now();
    world.tick(.033, i * 33 + 33);
    world.frame();
    samples.push(performance.now() - started);
  }
  samples.sort((a,b) => a-b);
  t.diagnostic(`7000 cells, prediction + frame: p50=${samples[10].toFixed(2)}ms p95=${samples[19].toFixed(2)}ms`);
  assert.equal(world.cellCount, 7000);
});

test('spawn acknowledgement replaces exactly one temporary cell and reset clears sync history', async () => {
  const { defaultGenome, flatten } = await import(pathToFileURL(join(directory, 'model.mjs')));
  const world = new PredictionWorld();
  world.predictSpawn(1, 100, 100, 0, flatten(defaultGenome()), 9, 0);
  assert.equal(world.cellCount, 1);
  assert([...positions(world).keys()][0] < 0);
  world.applyAck(1, 44);
  assert.equal(world.cellCount, 0, "accepted temporary parent never waits for an ID that may have divided");
  world.applyChunk(scene([{id:44, x:101}], 50), 100);
  assert.deepEqual([...positions(world).keys()], [44]);
  world.predictSpawn(2, 100, 100, 0, flatten(defaultGenome()), 9, 100);
  world.applyAck(2, undefined, 'rejected');
  assert.equal(world.cellCount, 1);
  world.applyReset();
  world.applyChunk(scene([{id:1, x:150}], 1), 200);
  assert.deepEqual([...positions(world).keys()], [1]);
});

test('moving a bond into a newer region survives replacement of its old region', () => {
  const world = new PredictionWorld();
  world.applyChunk(scene([{id:1,x:100},{id:2,x:125}], 1, 0, [1,2]), 0);
  world.applyChunk(scene([{id:1,x:600},{id:2,x:625}], 2, 1, [1,2]), 20);
  world.applyChunk(scene([], 3, 0), 30);
  assert.deepEqual(Array.from(world.frame().links), [1,2]);
});
