// End-to-end authority test: public room, chunk streaming, second player,
// spawn acknowledgement and host-only reset. The server binds an ephemeral
// port and is stopped in `finally`.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

const binary = fileURLToPath(new URL(`../target/debug/cellhub-server${process.platform === "win32" ? ".exe" : ""}`, import.meta.url));
if (!existsSync(binary))
  throw new Error("Build the server first: bash scripts/rust.sh build -p cellhub-server");

const server = spawn(binary, [
  "--bind", "127.0.0.1:0",
  "--static", "/tmp/cellhub-empty-static",
], {
  stdio: ["ignore", "pipe", "pipe"],
  env: { ...process.env, RUST_LOG: "cellhub_server=info", NO_COLOR: "1" },
});
let output = "";
let address = null;
const addressReady = new Promise((resolve, reject) => {
  const timer = setTimeout(() => reject(new Error(`server did not start:\n${output}`)), 10_000);
  const read = (chunk) => {
    output += String(chunk);
    const match = output.match(/address=(127\.0\.0\.1:\d+)/);
    if (match && !address) {
      address = match[1];
      clearTimeout(timer);
      resolve(address);
    }
  };
  server.stdout.on("data", read);
  server.stderr.on("data", read);
  server.once("exit", (code) => reject(new Error(`server exited early (${code}):\n${output}`)));
});
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const CELL_STRIDE = 28;
const TERRAIN_STRIDE = 6;
const waitFor = async (predicate, label, timeout = 10_000) => {
  const started = Date.now();
  while (Date.now() - started < timeout) {
    const value = predicate();
    if (value) return value;
    await delay(20);
  }
  throw new Error(`timeout: ${label}`);
};

function decodeChunk(buffer) {
  const view = new DataView(buffer);
  assert.equal(view.getUint8(0), 2, "binary chunk tag");
  const cellCount = view.getUint16(23, true);
  const linkCount = view.getUint16(25, true);
  const tileCount = view.getUint16(27, true);
  const cells = new Float32Array(cellCount * CELL_STRIDE);
  let offset = 31;
  const sparse = view.getUint8(29) === 1;
  for (let row = 0; row < cellCount; row++) {
    const mask = sparse ? view.getUint32(offset, true) : (1 << CELL_STRIDE) - 1;
    if (sparse) offset += 4;
    for (let i = 0; i < CELL_STRIDE; i++) {
      if (mask & (1 << i)) {
        cells[row * CELL_STRIDE + i] = view.getFloat32(offset, true);
        offset += 4;
      }
    }
  }
  const terrainOffset = offset + linkCount * 8;
  assert.equal(buffer.byteLength, terrainOffset + tileCount * TERRAIN_STRIDE * 4);
  return {
    tick: view.getUint32(1, true),
    population: view.getUint32(5, true),
    chunkX: view.getUint8(21),
    chunkY: view.getUint8(22),
    cells,
    terrainTiles: tileCount,
    firstTileNitrates: tileCount > 0 ? view.getFloat32(terrainOffset + 4 * 4, true) : undefined,
  };
}

function cellRow(chunk, id) {
  for (let i = 0; i < chunk.cells.length; i += CELL_STRIDE) {
    if (chunk.cells[i] === id) return chunk.cells.subarray(i, i + CELL_STRIDE);
  }
}

class Client {
  constructor(url) {
    this.url = url;
    this.messages = [];
    this.chunks = [];
    this.errors = [];
    this.seqs = new Map();
    this.ws = null;
  }
  async connect() {
    this.ws = new WebSocket(this.url);
    this.ws.binaryType = "arraybuffer";
    await new Promise((resolve, reject) => {
      this.ws.onopen = resolve;
      this.ws.onerror = reject;
    });
    this.ws.onmessage = ({ data }) => {
      if (typeof data === "string") {
        const message = JSON.parse(data);
        this.messages.push(message);
        if (message.type === "ack" || message.type === "inspect") {
          const pending = this.seqs.get(message.seq);
          if (pending) {
            this.seqs.delete(message.seq);
            pending(message);
          }
        }
        if (message.type === "error") {
          this.errors.push(message.message);
          const pending = this.seqs.get(message.seq);
          if (pending) {
            this.seqs.delete(message.seq);
            pending(message);
          }
        }
      } else {
        this.chunks.push(decodeChunk(data));
      }
    };
  }
  send(message) {
    this.ws.send(JSON.stringify(message));
  }
  command(message, seq) {
    return new Promise((resolve) => {
      this.seqs.set(seq, resolve);
      this.send({ ...message, seq });
    });
  }
  close() {
    this.ws?.close();
  }
}

let first;
let second;
let third;
try {
  const host = await addressReady;
  const url = `ws://${host}/ws`;
  first = new Client(url);
  await first.connect();
  first.send({ type: "list" });
  await waitFor(() => first.messages.some((m) => m.type === "rooms"), "room list");
  first.send({ type: "create", name: "Интеграционный тест", player: "Первый" });
  const welcome = await waitFor(
    () => first.messages.find((m) => m.type === "welcome"),
    "welcome",
  );
  assert.equal(welcome.protocol, 4);
  assert.equal(welcome.host, true);
  const room = welcome.room.id;

  const genome = Array.from({ length: 1 }, () => [
    0, 2.4, 0, 0, 0.5, 0, 0.8, 1000, 1, 0, 0, 0,
  ]).flat();
  const ack = await first.command(
    { type: "spawn", x: 300, y: 300, mode: 0, genome },
    1,
  );
  assert(ack.cell_id > 0, "authoritative spawn returns a cell id");
  const cellId = ack.cell_id;

  await waitFor(() => first.chunks.length >= 40, "chunk stream", 15_000);
  const seen = new Set(first.chunks.map((chunk) => chunk.chunkY * 6 + chunk.chunkX));
  assert.equal(seen.size, 36, "a full sequential chunk cycle is delivered");
  const withCell = first.chunks.find((chunk) => cellRow(chunk, cellId));
  assert(withCell, "the authoritative cell appears in its chunk");
  assert(withCell.population >= 1);
  assert(withCell.firstTileNitrates > 0, "the chunk includes substrate nitrates");

  second = new Client(url);
  await second.connect();
  second.send({ type: "join", room, player: "Второй" });
  const welcome2 = await waitFor(
    () => second.messages.find((m) => m.type === "welcome"),
    "second welcome",
  );
  assert.equal(welcome2.room.id, room);
  assert.equal(welcome2.host, false);
  assert.equal(welcome2.room.players, 2);
  await waitFor(() => second.chunks.length >= 3, "second client chunks");

  const inspected = await second.command({ type: "inspect", id: cellId }, 4);
  // Legacy input is normalized to the shared v5 genome ABI by the simulation.
  assert.deepEqual(inspected.genome.slice(0, 2), [-5, 1]);
  assert.equal(inspected.genome.length, 104);
  genome.forEach((value, index) => assert(Math.abs(inspected.genome[index + 2] - value) < 1e-6,
    `shared genome field ${index} survives legacy normalization`));
  assert.deepEqual(inspected.genome.slice(14, 24), [0, 0, 0, 0, -1, -1, -1, -1, 1, 0]);
  // The previous main used type 6 for Lipocyte and three signal fields.
  const otherGenome = [-4, 1,
    6, 2.4, 0, 0, 0.5, 0, 0.8, 1000, 0, 0, 0, 0,
    0, 0, 0, 0, 1, 0, 0, 0, 0, 4.5,
  ];
  const secondAck = await second.command(
    { type: "spawn", x: 700, y: 700, mode: 0, genome: otherGenome },
    5,
  );
  assert(secondAck.cell_id > 0 && secondAck.cell_id !== cellId);
  const otherChunk = await waitFor(
    () => first.chunks.find((chunk) => cellRow(chunk, secondAck.cell_id)),
    "second player's cell reaches the first client",
    15_000,
  );
  const otherRow = cellRow(otherChunk, secondAck.cell_id);
  assert.equal(otherRow[4], 7, "the previous main Lipocyte ID migrates to the current ID");
  assert.equal(otherRow[12], 0xff0000, "the selected color is serialized separately from speed");
  assert(otherRow[14] > 0 && otherRow[14] <= 1, "the cell reserve is included in the chunk");

  const before = second.messages.filter((m) => m.type === "world_reset").length;
  const resetAck = await first.command({ type: "reset" }, 2);
  assert.equal(resetAck.error, undefined);
  await waitFor(
    () =>
      second.messages.filter((m) => m.type === "world_reset").length > before,
    "world reset broadcast",
  );

  const denied = await second.command({ type: "reset" }, 3);
  assert.match(denied.message, /создатель комнаты/);

  // A created room is a fresh scenario, not a view of an existing world.
  third = new Client(url);
  await third.connect();
  third.send({ type: "create", name: "Новый сценарий", player: "Третий" });
  const welcome3 = await waitFor(
    () => third.messages.find((m) => m.type === "welcome"),
    "third welcome",
  );
  assert.notEqual(welcome3.room.id, room, "a new room gets a new id");
  assert.equal(welcome3.room.population, 0, "a new room starts empty");
  await waitFor(() => third.chunks.length >= 40, "third room chunk stream", 15_000);
  assert(
    third.chunks.every((chunk) => !cellRow(chunk, cellId) && !cellRow(chunk, secondAck.cell_id)),
    "existing cells do not leak into a new room",
  );

  assert.equal(first.errors.length, 0);
  console.log(
    "PASS: public rooms, isolated new scenario, 36-chunk cycle, authoritative spawn, join and host reset",
  );
} finally {
  first?.close();
  second?.close();
  third?.close();
  server.kill("SIGTERM");
  await delay(150);
}
