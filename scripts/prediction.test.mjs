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
  ["frontend/net/client.ts", "client.mjs"],
  ["frontend/net/profiling.ts", "profiling.mjs"],
]) {
  const original = await readFile(new URL(`../${source}`, import.meta.url), "utf8");
  const output = ts.transpileModule(original, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  }).outputText.replaceAll('"../model"', '"./model.mjs"').replaceAll('"./protocol"', '"./protocol.mjs"').replaceAll('"./prediction"', '"./prediction.mjs"').replaceAll('"./profiling"', '"./profiling.mjs"');
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

test("v5 decoder handles dense/sparse rows and rejects malformed masks and lengths", async () => {
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
  let now = 0, next = 0, socket, closeDuringFrame = false;
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
  const client = new MultiplayerClient({ ...callbacks, onFrame() {
    now += 50;
    if (closeDuringFrame) client.close();
  } });
  try {
    client.connect("ws://local-test/ws");
    socket.onopen();
    socket.onmessage({ data: JSON.stringify({ type: "welcome", protocol: 5 }) });
    const runNext = () => {
      const [id, timer] = timers.entries().next().value;
      timers.delete(id); now += timer.delay; timer.fn();
    };
    runNext();
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
