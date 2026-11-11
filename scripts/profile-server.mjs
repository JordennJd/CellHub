// Local 40-player load test. Build first: cargo build --release -p cellhub-server.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const players = Number(process.env.PLAYERS ?? 40);
const population = Number(process.env.CELLS ?? 10_000);
const seconds = Number(process.env.SECONDS ?? 10);
const layout = process.env.LAYOUT ?? "grid";
assert(players > 0 && players <= 40 && population > 0 && population <= 20_000 && seconds > 0);
assert(["grid", "colony", "piles"].includes(layout), `Unknown layout: ${layout}`);
const server = spawn(process.env.SERVER_BIN ?? fileURLToPath(new URL("../target/release/cellhub-server", import.meta.url)),
  ["--bind", "127.0.0.1:0", "--max-players", String(players)],
  { env: { ...process.env, NO_COLOR: "1", RUST_LOG: "cellhub_server=info" } });
const sockets = [];
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
let output = "", address, failure;
let joined = 0, acks = 0, bytes = 0, chunks = 0;
function position(i) {
  if (layout === "colony") return { x: 650 + (i % 140) * 12, y: 650 + Math.floor(i / 140) * 12 };
  if (layout === "piles") {
    const group = Math.floor(i / 200);
    return { x: 160 + (group % 10) * 280, y: 160 + Math.floor(group / 10) * 280 };
  }
  return { x: 30 + (i % 140) * 21.5, y: 30 + Math.floor(i / 140) * 21 };
}
for (const stream of [server.stdout, server.stderr]) stream.on("data", data => {
  output += data;
  address ??= output.match(/address=(127\.0\.0\.1:\d+)/)?.[1];
});
server.on("error", error => { failure = error; });
server.on("exit", code => { failure ??= new Error(`server exited ${code}: ${output}`); });
async function until(predicate, label, timeout = 60_000) {
  const deadline = Date.now() + timeout;
  while (!predicate()) {
    if (failure) throw failure;
    if (Date.now() > deadline) throw new Error(`timeout: ${label}: ${output}`);
    await sleep(20);
  }
}
try {
  await until(() => address, "server startup");
  let room;
  for (let i = 0; i < players; i++) {
    const socket = new WebSocket(`ws://${address}/ws`);
    socket.binaryType = "arraybuffer";
    sockets.push(socket);
    socket.onopen = () => socket.send(JSON.stringify(i === 0
      ? { type: "create", name: "local load test", player: `load-${i}` }
      : { type: "join", room, player: `load-${i}` }));
    socket.onmessage = ({ data }) => {
      if (typeof data !== "string") { bytes += data.byteLength; chunks++; return; }
      const message = JSON.parse(data);
      if (message.type === "welcome") { room = message.room.id; joined++; }
      if (message.type === "ack") {
        if (message.error) failure = new Error(message.error);
        acks++;
      }
      if (message.type === "error") failure = new Error(message.message);
    };
    socket.onerror = () => { failure = new Error("WebSocket failed"); };
    socket.onclose = () => { failure ??= new Error("player disconnected"); };
    await until(() => joined === i + 1, `join ${i}`);
  }
  const genome = Array.from({ length: 8 }, () => [0, 8, 0, 0, .5, 0, 0, 240]).flat();
  for (let i = 0; i < population; i++) {
    sockets[i % players].send(JSON.stringify({ type: "spawn", seq: i + 1,
      ...position(i), mode: 0, genome }));
    if (i % 80 === 79) await until(() => acks >= i + 1, "spawn acknowledgements");
  }
  await until(() => acks === population, "all spawns");
  const samples = [];
  const initialBytes = bytes, initialChunks = chunks;
  const started = performance.now();
  for (let i = 0; i < seconds * 10; i++) {
    if (failure) throw failure;
    const response = await fetch(`http://${address}/debug/rooms`);
    assert(response.ok);
    const metrics = await response.json();
    samples.push(metrics.rooms.find(value => value.id === room));
    await sleep(100);
  }
  const elapsed = (performance.now() - started) / 1000;
  const timings = samples.map(sample => sample.tickUs / 1000).sort((a, b) => a - b);
  const percentile = key => samples.map(sample => sample[key] / 1000).sort((a, b) => a - b)[Math.floor(samples.length * .95)];
  assert(chunks > initialChunks && sockets.every(socket => socket.readyState === WebSocket.OPEN));
  console.log(JSON.stringify({ layout, players, requestedCells: population, seconds: elapsed,
    p50TickMs: timings[Math.floor(timings.length / 2)],
    p95TickMs: timings[Math.floor(timings.length * .95)], maxTickMs: timings.at(-1),
    p95StepMs: percentile("stepUs"), p95StreamMs: percentile("streamUs"),
    overrunsDuringMeasurement: samples.at(-1).overruns - samples[0].overruns,
    aggregateBytesPerSecond: (bytes - initialBytes) / elapsed,
    chunksPerPlayerPerSecond: (chunks - initialChunks) / elapsed / players,
    latest: samples.at(-1) }, null, 2));
} finally {
  for (const socket of sockets) socket.close();
  server.kill("SIGTERM");
}
