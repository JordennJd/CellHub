/// <reference lib="webworker" />
import init, { Simulation } from "./wasm/simulation.js";
import { chaosOrganismsBuffer } from "./organisms";
import { TICK_RATE, type Command, type WorldCommand } from "./model";
const ctx = self as unknown as DedicatedWorkerGlobalScope;
let sim: Simulation;
let speed = 0;
let debt = 0;
let last = performance.now();
let previewRequestId: number | undefined;
type ScheduledCommand = WorldCommand & { atTick: number };
let scheduled: ScheduledCommand[] = [];
// Curated organism catalog; resent after restore because checkpoints omit it.
let chaosOrganisms = chaosOrganismsBuffer();
const MAX_SCHEDULED_COMMANDS = 1024;
const scheduledTypes = new Set(["spawn", "edit", "configure", "remove", "boost", "move", "sterilize", "chaos-configure"]);
const validTick = (tick: unknown): tick is number =>
  typeof tick === "number" && Number.isInteger(tick) && tick >= 0 && tick <= 0xffffffff;

function frame() {
  const cells = sim.snapshot(),
    links = sim.links(),
    terrain = sim.terrain(),
    stats = sim.stats(),
    nitrogen = sim.nitrogen_reserves(),
    nitrates = sim.nitrates(),
    details = sim.cell_details(),
    coveredFood = sim.covered_food(),
    level = sim.level_status(),
    divisions = sim.take_division_events();
  const info = sim.chaos_info(), history = sim.chaos_history(), events = sim.chaos_events();
  ctx.postMessage({
    type: "frame", cells, links, terrain, stats, nitrogen, nitrates, details, coveredFood, level, divisions,
    worldSize: sim.world_size(), worldWidth: sim.world_width(), circular: sim.circular_dish(), chaos: info.length ? { info, history, events } : undefined,
    ...(previewRequestId === undefined ? {} : { previewRequestId }),
  }, [
    info.buffer, history.buffer, events.buffer,
    cells.buffer,
    links.buffer,
    terrain.buffer,
    stats.buffer,
    nitrogen.buffer,
    nitrates.buffer,
    details.buffer,
    coveredFood.buffer,
    divisions.buffer,
  ]);
  previewRequestId = undefined;
}
const pending: Command[] = [];
ctx.onmessage = ({ data }: MessageEvent<Command>) => {
  pending.push(data);
};
function sortScheduled(commands: ScheduledCommand[]) {
  return commands.sort((a, b) => a.atTick - b.atTick);
}

function settingsData() {
  const [light, food, radiation, viscosity, aging, killAtEdge, onlyPointMutations, gravity, density, nitrogen] = sim.environment();
  return {
    light, food, radiation, viscosity, aging: !!aging, killAtEdge: !!killAtEdge,
    onlyPointMutations: !!onlyPointMutations, gravity, density, nitrogen,
  };
}

function readScheduled(raw: unknown, tick: number): ScheduledCommand[] {
  if (raw === undefined) return [];
  if (!Array.isArray(raw) || raw.length > MAX_SCHEDULED_COMMANDS) throw new Error("Некорректная очередь команд");
  return sortScheduled(raw.map(item => {
    if (!item || !scheduledTypes.has(item.type) || !validTick(item.atTick) || item.atTick <= tick)
      throw new Error("Некорректный тик отложенной команды");
    if (item.type === "spawn" || item.type === "edit") {
      if (!Array.isArray(item.genome) || item.genome.length > 1_200_000
        || !item.genome.every(Number.isFinite)) throw new Error("Некорректный геном отложенной команды");
      return { ...item, genome: new Float32Array(item.genome) };
    }
    return item;
  }));
}

// atTick is the number of completed steps before a mutation. Commands on the
// same boundary run in receipt order, regardless of how steps are batched.
function advance(steps: number) {
  const target = sim.tick_number() + steps;
  let applied = 0;
  for (const c of scheduled) {
    if (c.atTick > target) break;
    sim.advance(c.atTick - sim.tick_number());
    apply(c);
    applied++;
  }
  if (applied) scheduled.splice(0, applied);
  sim.advance(target - sim.tick_number());
}

function commands() {
  let changed = false;
  for (const c of pending.splice(0)) {
    if ("atTick" in c && c.atTick !== undefined) {
      const tick = sim.tick_number();
      if (!scheduledTypes.has(c.type) || !validTick(c.atTick) || c.atTick < tick
        || (c.atTick > tick && scheduled.length >= MAX_SCHEDULED_COMMANDS)) {
        ctx.postMessage({ type: "error", command: c.type, message: "Недопустимый тик команды или очередь переполнена" });
        continue;
      }
      if (c.atTick > tick) {
        scheduled.push({ ...c, atTick: c.atTick });
        sortScheduled(scheduled);
        continue;
      }
    }
    changed = apply(c) || changed;
  }
  return changed;
}

function apply(c: Command) {
  let changed = false;
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
      case "chaos-configure":
        if (!Number.isInteger(c.rate) || c.rate < 1 || c.rate > 60) throw new Error("Недопустимая частота самозарождения");
        sim.configure_chaos(c.enabled, c.rate, c.viable);
        changed = true;
        break;
      case "configure": {
        const s = c.settings;
        sim.configure(s.light, s.food, s.radiation, s.viscosity, s.aging, s.killAtEdge,
          s.onlyPointMutations, s.gravity, s.density, s.nitrogen);
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
          state: JSON.stringify({
            version: 1,
            values: Array.from(sim.export_state()),
            scheduled: scheduled.map(c => "genome" in c ? { ...c, genome: Array.from(c.genome) } : c),
          }) });
        break;
      case "restore": {
        if (c.state.length > 100_000_000) throw new Error("Файл эксперимента слишком большой");
        const state = JSON.parse(c.state);
        if (state?.version !== 1 || !Array.isArray(state.values)
          || state.values.length > 10_000_000
          || state.values.some((v: unknown) => typeof v !== "number" || !Number.isFinite(v))) {
          throw new Error("Некорректный формат эксперимента");
        }
        const nextScheduled = readScheduled(state.scheduled, state.values[3]);
        sim.restore_state(new Float64Array(state.values));
        sim.use_rectangular_geometry();
        sim.set_chaos_organisms(chaosOrganisms);
        previewRequestId = undefined;
        scheduled = nextScheduled;
        speed = 0;
        debt = 0;
        ctx.postMessage({ type: "restored", requestId: c.requestId, settings: settingsData() });
        changed = true;
        break;
      }
      case "preview": {
        if (!Number.isFinite(c.steps)) throw new Error("Некорректный возраст предпросмотра");
        // Preview owns a separate worker; never share this instance with a live laboratory.
        previewRequestId = undefined;
        sim.reset();
        scheduled = [];
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
        previewRequestId = "requestId" in c && typeof c.requestId === "number" ? c.requestId : undefined;
        changed = true;
        break;
      }
      case "reset":
        if (c.level !== undefined) {
          if (typeof c.level !== "string" || c.level.length > 20_000_000)
            throw new Error("Некорректный файл уровня");
          sim.load_level_json(c.level);
        } else if (c.chaos) {
          if (!Number.isInteger(c.chaos.seed) || c.chaos.seed < 0 || c.chaos.seed > 0xffffffff) throw new Error("Некорректный seed");
          if (c.chaos.organisms?.length) chaosOrganisms = c.chaos.organisms;
          sim.start_chaos_with_organisms(c.chaos.seed, c.chaos.genome, chaosOrganisms, c.bounds?.width ?? 3072);
        } else { sim.reset(); if (c.bounds) sim.set_rectangular_dish(c.bounds.width, c.bounds.height); }
        previewRequestId = undefined;
        scheduled = [];
        speed = 0;
        debt = 0;
        ctx.postMessage({ type: "reset", requestId: c.requestId, settings: settingsData() });
        changed = true;
        break;
      case "demo":
        if (c.restart) sim.reset();
        sim.start_level_demo();
        scheduled = [];
        speed = 4;
        debt = 0;
        changed = true;
        break;
      case "stopDemo":
        sim.stop_level_demo();
        speed = 0;
        debt = 0;
        changed = true;
        break;
      case "step":
        if (speed === 0) {
          advance(1);
          changed = true;
        }
        break;
    }
  } catch (e) {
    ctx.postMessage({ type: "error", message: String(e), command: c.type,
      requestId: "requestId" in c ? c.requestId : undefined });
  }
  return changed;
}

async function createSimulation() {
  const threads = ctx.crossOriginIsolated && ctx.name !== "cellhub-preview"
    ? Math.min(4, ctx.navigator.hardwareConcurrency || 1) : 1;
  if (threads > 1) {
    try {
      const threaded = await import("./wasm/threaded/index.js");
      await threaded.default();
      // A failed nested worker can leave Rayon's startup promise pending.
      let timer: number | undefined;
      try {
        await Promise.race([
          threaded.initThreadPool(threads),
          new Promise<never>((_, reject) => {
            timer = ctx.setTimeout(() => reject(new Error("WASM thread pool startup timed out")), 10_000);
          }),
        ]);
      } finally {
        ctx.clearTimeout(timer);
      }
      threaded.enable_parallel_simulation();
      return { sim: new threaded.Simulation(73421), threaded: true };
    } catch (e) {
      console.warn("CellHub: WASM threads unavailable", e);
    }
  }
  await init();
  return { sim: new Simulation(73421), threaded: false };
}

try {
  const initialized = await createSimulation();
  sim = initialized.sim;
  sim.use_rectangular_geometry();
  sim.set_division_event_tracking(true);
  ctx.postMessage({ type: "ready", threaded: initialized.threaded });
  let needFrame = true;
  setInterval(() => {
    const now = performance.now();
    needFrame = commands() || needFrame;
    debt += Math.min((now - last) / 1000, 0.25) * speed;
    last = now;
    const steps = Math.min(40, Math.floor(debt * TICK_RATE));
    if (steps) {
      advance(steps);
      debt -= steps / TICK_RATE;
      needFrame = true;
    }
    if (needFrame) {
      frame();
      needFrame = false;
    }
  }, 1000 / TICK_RATE);
} catch (e) {
  ctx.postMessage({ type: "error", message: `WASM: ${String(e)}` });
}
