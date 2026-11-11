// Build WASM and the native profile example before running this test.
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import init, { Simulation } from "../frontend/wasm/simulation.js";
await init({ module_or_path: await readFile(new URL("../frontend/wasm/simulation_bg.wasm", import.meta.url)) });
const population = 1000, ticks = 240;
const genes = new Float32Array(Array.from({ length: 8 }, () => [0, 2.4, 0, 0, .5, 12, .8, 240]).flat());
const simulation = new Simulation(73421);
try {
  for (let i = 0; i < population; i++) {
    // Match Rust's float32 arithmetic at each operation.
    const x = Math.fround(30 + Math.fround((i % 140) * 21.5));
    const y = Math.fround(30 + Math.fround(Math.floor(i / 140) * 21));
    simulation.spawn(x, y, 0, genes);
  }
  for (let i = 0; i < ticks; i++) simulation.advance(1);
  const wasm = simulation.checksum();
  for (const threads of [1, 2, 4]) {
    const output = execFileSync(fileURLToPath(new URL("../target/release/examples/profile", import.meta.url)),
      [String(population), String(ticks)], { encoding: "utf8", env: { ...process.env, RAYON_NUM_THREADS: String(threads) } });
    assert.equal(output.match(/checksum=(\w+)/)?.[1], wasm, `native ${threads} threads vs WASM`);
    console.log(output.trim());
  }
  console.log(`WASM/native parity: ${wasm}`);
} finally { simulation.free(); }
