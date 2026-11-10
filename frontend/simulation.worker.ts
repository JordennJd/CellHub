/// <reference lib="webworker" />
import init, { Simulation } from "./wasm/simulation.js";
import type { Command } from "./model";
const ctx = self as unknown as DedicatedWorkerGlobalScope;
let sim: Simulation;
let speed = 0;
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
          ctx.postMessage({ type: "spawned", id: sim.spawn(c.x, c.y, c.mode, c.genome) });
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
          if (![0, 0.25, 1, 2, 4].includes(c.value)) throw new Error("Недопустимая скорость симуляции");
          speed = c.value;
          debt = 0;
          break;
        case "configure": {
          const s = c.settings;
          sim.configure(s.light, s.food, s.radiation, s.viscosity, s.aging, s.killAtEdge,
            s.onlyPointMutations, s.gravity, s.density);
          changed = true;
          break;
        }
        case "remove":
          sim.remove(c.id);
          changed = true;
          break;
        case "boost":
          sim.boost(c.id);
          changed = true;
          break;
        case "move":
          sim.move_cell(c.id, c.x, c.y);
          changed = true;
          break;
        case "sterilize":
          sim.sterilize();
          changed = true;
          break;
        case "export":
          ctx.postMessage({ type: "export", requestId: c.requestId,
            state: JSON.stringify({ version: 1, values: Array.from(sim.export_state()) }) });
          break;
        case "restore": {
          if (c.state.length > 100_000_000) throw new Error("Файл эксперимента слишком большой");
          const state = JSON.parse(c.state);
          if (state?.version !== 1 || !Array.isArray(state.values)
            || state.values.length > 10_000_000
            || state.values.some((v: unknown) => typeof v !== "number" || !Number.isFinite(v))) {
            throw new Error("Некорректный формат эксперимента");
          }
          sim.restore_state(new Float64Array(state.values));
          speed = 0;
          debt = 0;
          const [light, food, radiation, viscosity, aging, killAtEdge, onlyPointMutations, gravity, density] = sim.environment();
          ctx.postMessage({ type: "restored", requestId: c.requestId, settings: {
            light, food, radiation, viscosity, aging: !!aging, killAtEdge: !!killAtEdge,
            onlyPointMutations: !!onlyPointMutations, gravity, density,
          } });
          changed = true;
          break;
        }
        case "preview": {
          if (!Number.isFinite(c.steps)) throw new Error("Некорректный возраст предпросмотра");
          // Preview owns a separate worker; never share this instance with a live laboratory.
          sim.reset();
          sim.configure(1, 1, 0, 1, true, false);
          speed = 0;
          debt = 0;
          sim.spawn(600, 600, c.mode, c.genome);
          let remaining = Math.min(1800, Math.max(0, Math.floor(c.steps)));
          while (remaining > 0) {
            const chunk = Math.min(120, remaining);
            sim.advance(chunk);
            remaining -= chunk;
          }
          changed = true;
          break;
        }
        case "reset":
          sim.reset();
          speed = 0;
          debt = 0;
          ctx.postMessage({ type: "reset", requestId: c.requestId });
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
      ctx.postMessage({ type: "error", message: String(e), command: c.type,
        requestId: "requestId" in c ? c.requestId : undefined });
    }
  }
  return changed;
}
try {
  await init();
  sim = new Simulation(73421);
  sim.set_circular_dish(true);
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
