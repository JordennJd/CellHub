/// <reference lib="webworker" />
import init, { Simulation } from "./wasm/simulation.js";
import type { Command } from "./model";
const ctx = self as unknown as DedicatedWorkerGlobalScope;
let sim: Simulation;
let speed = 1;
let debt = 0;
let last = performance.now();
function frame() {
  const cells = sim.snapshot(),
    links = sim.links(),
    terrain = sim.terrain(),
    stats = sim.stats();
  ctx.postMessage({ type: "frame", cells, links, terrain, stats }, [
    cells.buffer,
    links.buffer,
    terrain.buffer,
    stats.buffer,
  ]);
}
const pending: Command[] = [];
ctx.onmessage = ({ data }: MessageEvent<Command>) => {
  pending.push(data);
};
function commands() {
  let changed = false;
  for (const c of pending.splice(0)) {
    try {
      switch (c.type) {
        case "spawn":
          sim.spawn(c.x, c.y, c.mode, c.genome);
          changed = true;
          break;
        case "edit":
          sim.edit(c.id, c.mode, c.genome);
          changed = true;
          break;
        case "inspect":
          ctx.postMessage({
            type: "genome",
            id: c.id,
            genome: sim.genome(c.id),
            mode: sim.cell_mode(c.id),
          });
          break;
        case "speed":
          speed = c.value;
          debt = 0;
          break;
        case "reset":
          sim.reset();
          debt = 0;
          changed = true;
          break;
        case "step":
          if (speed === 0) {
            sim.advance(1);
            changed = true;
          }
          break;
      }
    } catch (e) {
      ctx.postMessage({ type: "error", message: String(e) });
    }
  }
  return changed;
}
try {
  await init();
  sim = new Simulation(73421);
  ctx.postMessage({ type: "ready" });
  let needFrame = true;
  setInterval(() => {
    const now = performance.now();
    needFrame = commands() || needFrame;
    debt += Math.min((now - last) / 1000, 0.25) * speed;
    last = now;
    const steps = Math.min(120, Math.floor(debt * 30));
    if (steps) {
      sim.advance(steps);
      debt -= steps / 30;
      needFrame = true;
    }
    if (needFrame) {
      frame();
      needFrame = false;
    }
  }, 1000 / 30);
} catch (e) {
  ctx.postMessage({ type: "error", message: `WASM: ${String(e)}` });
}
