import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import { test } from "node:test";
import ts from "typescript";
import { build } from "esbuild";
import init, { Simulation } from "../frontend/wasm/simulation.js";

const modelSource = await readFile(new URL("../frontend/model.ts", import.meta.url), "utf8");
const modelJs = ts.transpileModule(modelSource, {compilerOptions: {
  target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext,
}}).outputText;
const model = await import(`data:text/javascript;base64,${Buffer.from(modelJs).toString("base64")}`);

const organismsBundle = await build({
  entryPoints: [new URL("../frontend/organisms.ts", import.meta.url).pathname],
  bundle: true, write: false, format: "esm", platform: "node", logLevel: "silent",
});
const organisms = await import(`data:text/javascript;base64,${Buffer.from(organismsBundle.outputFiles[0].text).toString("base64")}`);

await init({
  module_or_path: await readFile(
    new URL("../frontend/wasm/simulation_bg.wasm", import.meta.url),
  ),
});
const genes = () =>
  new Float32Array(
    Array.from({ length: 8 }, () => [0, 2.4, 0, 0, 0.5, 12, 0.8, 240]).flat(),
  );

test("demo organism catalog is valid and filtered by allowed cell types", () => {
  const list = organisms.DEMO_ORGANISMS;
  assert(list.length >= 6);
  assert.equal(new Set(list.map(o => o.id)).size, list.length);
  assert(!list.some(o => o.id === "swimmer" || o.id === "photo-lipo"));
  for (const organism of list) {
    assert(!("origin" in organism) && !("universality" in organism));
    const genome = organism.genome();
    assert(organism.span >= 1 && organism.span <= 8, `${organism.id} has an invalid span`);
    assert(genome.length >= 1, `${organism.id} has no modes`);
    assert(organism.initialMode >= 0 && organism.initialMode < genome.length);
    assert(genome[organism.initialMode].canDivide, `${organism.id} cannot grow`);
    for (const [index, mode] of genome.entries()) {
      assert(mode.name, `${organism.id} slot ${index} has no name`);
      assert(mode.kind >= 0 && mode.kind < model.TYPES.length);
      for (const ref of [mode.childA, mode.childB, mode.transitionA, mode.transitionB, mode.offspringMode, mode.compatibleMode])
        assert(ref >= 0 && ref < genome.length, `${organism.id} slot ${index} has a broken reference`);
    }
    assert.deepEqual(model.flatten(model.inflate(model.flatten(genome))), model.flatten(genome));
  }
  assert(organisms.demoOrganisms([0]).some(o => o.id === "filament"));
  assert(organisms.demoOrganisms([0, 1, 2, 6]).some(o => o.id === "tutorial"));
  assert(!organisms.demoOrganisms([0, 1, 2, 6]).some(o => o.id === "smart-phagocyte"));
});

const organismCatalog = (organism) =>
  new Float32Array([1, 1, organism.span, 1, ...model.flatten(organism.genome())]);

test("catalog self-retaining B daughters preserve their heading", () => {
  for (const organism of organisms.DEMO_ORGANISMS) {
    const genome = organism.genome();
    for (const [index, mode] of genome.entries()) {
      if (!mode.keepAdhesionB || mode.childB !== index || mode.childA === index) continue;
      assert(
        Math.abs(Math.abs(mode.childAngleB) - Math.PI) < 1e-5,
        `${organism.id} slot ${index} rotates its retained daughter away from the heading`,
      );
    }
  }
});

test("chaos seeds every curated organism as a bonded chain", () => {
  for (const organism of organisms.DEMO_ORGANISMS) {
    const sim = new Simulation(42);
    try {
      sim.start_chaos_with_organisms(42, model.flatten(model.chaosGenome()), organismCatalog(organism), 6144);
      const cells = sim.snapshot(), links = sim.links();
      const ids = Array.from({ length: cells.length / model.STRIDE }, (_, i) => cells[i * model.STRIDE]);
      const recent = ids.slice(-organism.span);
      assert.equal(recent.length, organism.span, `${organism.id} did not seed its full chain`);
      const modes = organism.genome().length;
      for (let i = 0; i < recent.length; i++)
        assert.equal(sim.cell_mode(recent[i]), Math.min(i, modes - 1),
          `${organism.id} chain has the wrong mode order`);
      const bonds = new Set(Array.from({ length: links.length / 2 }, (_, i) => `${links[i * 2]}/${links[i * 2 + 1]}`));
      for (let i = 1; i < recent.length; i++)
        assert(bonds.has(`${recent[i - 1]}/${recent[i]}`), `${organism.id} chain is not bonded`);
      sim.advance(60);
      assert(sim.stats()[1] >= 1, `${organism.id} died immediately`);
    } finally {
      sim.free();
    }
  }
});

test("catalog virocyte infects a mature neighbour and the infection splits", () => {
  const sim = new Simulation(42);
  try {
    sim.configure(1, 1, 0, 1, false, false);
    const viroid = organisms.DEMO_ORGANISMS.find(o => o.id === "virocyte").genome();
    const host = sim.spawn(500, 500, 0, genes());
    sim.boost(host);
    sim.advance(6);
    const before = sim.stats()[1];
    sim.spawn(516, 500, 0, model.flatten(viroid));
    sim.advance(2);
    assert.equal(sim.stats()[4], 1, "virocyte infects an unprotected neighbour");
    sim.advance(10);
    assert(sim.stats()[1] > before, "infected host becomes a fast-splitting virion");
  } finally { sim.free(); }
});

test("tutorial phagocyte survives repeated self-division without losing its bond", () => {
  const sim = new Simulation(42);
  try {
    sim.set_circular_dish(true);
    // Full light and food, no radiation, drift or aging, so the pair divides predictably.
    sim.configure(1, 1, 0, 1, false, false, true, 0, 0, 1);
    sim.set_division_event_tracking(true);
    const tutorial = organisms.DEMO_ORGANISMS.find(o => o.id === "tutorial");
    const founder = sim.spawn(1536, 1536, 0, model.flatten(tutorial.genome()));
    let flagellocyte = null, phagocyte = null, selfDivisions = 0;
    for (let batch = 0; batch < 200 && selfDivisions < 2; batch++) {
      sim.advance(15);
      const events = sim.take_division_events();
      for (let i = 0; i < events.length; i += 8) {
        const parent = events[i + 1], a = events[i + 2], b = events[i + 3];
        if (parent === founder && flagellocyte === null) {
          flagellocyte = a;
          phagocyte = b;
        } else if (parent === phagocyte) {
          phagocyte = b;
          selfDivisions++;
        }
      }
    }
    assert.equal(selfDivisions, 2, "the phagocyte did not divide twice");
    const snapshot = sim.snapshot();
    const alive = new Set();
    for (let i = 0; i < snapshot.length; i += model.STRIDE) alive.add(snapshot[i]);
    assert(alive.has(flagellocyte), "the flagellocyte died");
    assert(alive.has(phagocyte), "the retained phagocyte died");
    assert.equal(sim.cell_mode(phagocyte), 2);
    const links = sim.links(), bonds = new Set();
    for (let i = 0; i < links.length; i += 2) {
      bonds.add(`${links[i]}/${links[i + 1]}`);
      bonds.add(`${links[i + 1]}/${links[i]}`);
    }
    assert(
      bonds.has(`${flagellocyte}/${phagocyte}`),
      "the retained phagocyte lost its bond to the flagellocyte",
    );
  } finally {
    sim.free();
  }
});

test("rectangular WASM worlds preserve wide coordinates and exact checkpoints", () => {
  const sim = new Simulation(42), restored = new Simulation(1);
  try {
    sim.set_rectangular_dish(6144, 3072);
    sim.spawn(6000, 100, 0, genes());
    assert.equal(sim.world_width(), 6144);
    assert.equal(sim.world_size(), 3072);
    assert.equal(sim.terrain().length, 96 * 48 * 4);
    const state = sim.export_state();
    restored.restore_state(state);
    assert.deepEqual(restored.export_state(), state);
    assert(restored.snapshot()[1] > 5900);
    sim.start_chaos(42, model.flatten(model.chaosGenome()), 12288);
    assert.equal(sim.world_width(), 12288);
    assert.equal(sim.world_size(), 3072);
    restored.restore_state(sim.export_state());
    assert.deepEqual(restored.export_state(), sim.export_state());
  } finally { sim.free(); restored.free(); }
});

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

test("all eighteen types and eight signal channels survive the real WASM ABI and checkpoints", () => {
  const sim = new Simulation(41), restored = new Simulation(1);
  try {
    sim.configure(2, 2, 0, 1, false, false);
    for (let kind = 0; kind < model.TYPES.length; kind++) {
      const mode = { ...model.newMode(0), kind, speed: 0, canDivide: false };
      if (kind === 13) mode.programs = Array.from({ length: 8 }, (_, channel) => ({
        ...model.newProgram(), target: channel, scale: (channel - 3) * 0.25,
      }));
      const id = sim.spawn(300 + kind * 120, 1400, 0, model.flatten([mode]));
      assert.deepEqual(sim.genome(id), model.flatten([mode]));
    }
    sim.advance(1);
    const details = sim.cell_details();
    assert.equal(details.length, 18 * model.CELL_DETAILS_STRIDE);
    for (let channel = 0; channel < 8; channel++)
      assert.equal(details[13 * model.CELL_DETAILS_STRIDE + channel], (channel - 3) * 0.25);
    restored.restore_state(sim.export_state());
    assert.deepEqual(restored.cell_details(), details);
    for (let tick = 0; tick < 60; tick++) {
      sim.advance(1);
      restored.advance(1);
      assert.equal(restored.checksum(), sim.checksum(), `checkpoint continuation at tick ${tick}`);
    }
    assert.equal(sim.covered_food().length, 48 * 48);
  } finally { sim.free(); restored.free(); }
});

test("gamete hybridization and stem differentiation execute through the real WASM entry points", () => {
  const sim = new Simulation(42);
  try {
    sim.configure(2, 2, 0, 1, false, false);
    const gametes = [
      { ...model.newMode(0), kind: 16, speed: 0, compatibleMode: 0, offspringMode: 1, telomeres: 3 },
      { ...model.newMode(1), kind: 0, speed: 0, canDivide: false },
    ];
    sim.spawn(600, 600, 0, model.flatten(gametes));
    sim.spawn(612, 600, 0, model.flatten(gametes));
    sim.advance(15);
    assert.equal(sim.stats()[1], 1);
    const id = sim.snapshot()[0];
    assert.equal(sim.genome(id).length, 2 + 4 * model.MODE_STRIDE);
    assert.equal(sim.cell_details()[10], 3);
    sim.reset();
    sim.configure(2, 2, 0, 1, false, false);
    const stem = [
      { ...model.newMode(0), kind: 15, speed: 0, transitionA: 1,
        programs: [{ ...model.newProgram(), target: 16 }] },
      { ...model.newMode(1), kind: 1, speed: 0, canDivide: false },
    ];
    const founder = sim.spawn(600, 600, 0, model.flatten(stem));
    sim.advance(15);
    assert.equal(sim.cell_mode(founder), 1);
    assert.equal(sim.stats()[2], 1);
  } finally { sim.free(); }
});

test("laboratory tools and substrate settings work through the real WASM ABI", () => {
  const sim = new Simulation(73421);
  try {
    sim.configure(2, 3, 0, 2, false, true, true, 1, -1, 0);
    assert.deepEqual(Array.from(sim.environment()), [2, 3, 0, 2, 0, 1, 1, 1, -1, 0]);
    assert.equal(sim.nitrates().length, 48 * 48);
    assert(sim.nitrates().every(value => value === 0));
    const before = sim.export_state();
    assert.throws(() => sim.configure(1, 1, 0, 0, true, false));
    assert.deepEqual(sim.export_state(), before);
    const id = sim.spawn(500, 500, 0, genes());
    sim.advance(10);
    assert(sim.nitrogen_reserves()[0] < 1);
    const mass = sim.snapshot()[5];
    sim.move_cell(id, 650, 700);
    sim.boost(id);
    assert.equal(sim.nitrogen_reserves()[0], 1);
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
    assert.deepEqual(Array.from(sim.environment()), [1, 1, 1, 1, 1, 0, 0, 0, 0, 1]);
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
    assert.deepEqual(sim.genome(id), model.flatten(model.inflate(g)));
    sim.boost(id);
    sim.advance(4);
    assert.equal(sim.stats()[1], 1);
    sim.advance(1);
    const frame = sim.snapshot();
    assert.equal(frame.length, 24);
    assert.equal(frame[11], 1);
    assert.equal(frame[23], 0);
    assert.equal(frame[10], 0x3399ff);
    assert.equal(frame[22], 0x3399ff);
    assert(Math.abs(frame[21] - frame[9] - Math.PI + 0.7) < 0.00001);
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
  const organismsUrl = `data:text/javascript;base64,${Buffer.from(organismsBundle.outputFiles[0].text).toString("base64")}`;
  const modelUrl = `data:text/javascript;base64,${Buffer.from(modelJs).toString("base64")}`;
  const workerJs = ts.transpileModule(workerSource, {compilerOptions: {
    target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext,
  }}).outputText
    .replace('"./wasm/simulation.js"', JSON.stringify(new URL("../frontend/wasm/simulation.js", import.meta.url).href))
    .replace('"./organisms"', JSON.stringify(organismsUrl))
    .replace('"./model"', JSON.stringify(modelUrl))
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

    // Save before commands targeted to a future boundary. Replaying the loaded
    // queue must apply same-tick commands in their original receipt order.
    send({type:"spawn", x:500, y:500, mode:0, genome:genes(), atTick:2});
    send({type:"boost", id:1, atTick:2});
    send({type:"export", requestId:"before-scheduled"});
    tick();
    const checkpoint = messages.find(message => message.requestId === "before-scheduled");
    for (const replay of [false, true]) {
      if (replay) {
        send({type:"restore", requestId:"load-scheduled", state:checkpoint.state});
        tick();
      }
      send({type:"step"});
      tick();
      assert.equal(messages.at(-1).stats[1], 0);
      send({type:"step"});
      tick();
      assert.equal(messages.at(-1).stats[0], 2);
      assert.equal(messages.at(-1).cells[5], 2);
      send({type:"export", requestId:`after-${replay}`});
      tick();
    }
    const finalState = replay => messages.find(message => message.requestId === `after-${replay}`).state;
    assert.equal(finalState(false), finalState(true));

    // The real worker routes chaos resets and serializes future config
    // commands with the same queue as sandbox commands.
    const catalog = organisms.chaosOrganismsBuffer();
    send({type:"reset", requestId:"chaos-start", chaos:{seed:42, genome:model.flatten(model.chaosGenome()), organisms:catalog}});
    tick();
    assert.equal(messages.at(-1).worldSize, 3072);
    assert.equal(messages.at(-1).chaos.info[5], 6);
    assert.equal(messages.at(-1).chaos.info[9], organisms.DEMO_ORGANISMS.length);
    send({type:"chaos-configure", enabled:false, rate:10, viable:false, atTick:2});
    send({type:"export", requestId:"chaos-queue"});
    tick();
    const chaosSave = messages.find(message => message.requestId === "chaos-queue").state;
    for (const replay of [false, true]) {
      if (replay) { send({type:"restore", requestId:"chaos-reload", state:chaosSave}); tick(); }
      send({type:"step"}); send({type:"step"}); tick();
      assert.equal(messages.at(-1).chaos.info[2], 0);
      assert.equal(messages.at(-1).chaos.info[9], organisms.DEMO_ORGANISMS.length);
      send({type:"export", requestId:`chaos-after-${replay}`}); tick();
    }
    assert.equal(messages.find(m => m.requestId === "chaos-after-false").state,
      messages.find(m => m.requestId === "chaos-after-true").state);
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

test("chaos WASM mixes curated organisms with mandatory single cells and checkpoints", () => {
  const sim = new Simulation(7), restored = new Simulation(99);
  try {
    const catalog = organisms.chaosOrganismsBuffer();
    sim.start_chaos_with_organisms(73421, model.flatten(model.chaosGenome()), catalog, 3072);
    assert.equal(sim.world_size(), 3072);
    assert.equal(sim.terrain().length, 48 * 48 * 4);
    assert(sim.stats()[1] >= 130, "initial soup is too small");
    assert.equal(sim.chaos_info()[5], 6);
    assert.equal(sim.chaos_info()[9], organisms.DEMO_ORGANISMS.length);
    assert(sim.links().length > 0, "initial soup has no multicellular organisms");

    sim.sterilize();
    sim.advance(59);
    assert.equal(sim.stats()[1], 0);
    sim.advance(1);
    assert.equal(sim.stats()[1], 1);
    assert([0, 1, 2, 3, 5].includes(sim.snapshot()[4]), "first single is not a mandatory role");
    assert.equal(sim.genome(sim.snapshot()[0]).length, 2 + 102, "single should have one mode");
    // Remember the newest id just before the organism arrives: its cells are
    // allocated contiguously from that point, while singles may divide on the
    // same tick and append higher ids.
    sim.advance(89);
    const before = sim.snapshot();
    const newest = Math.max(...Array.from({ length: before.length / model.STRIDE }, (_, i) => before[i * model.STRIDE]));
    sim.advance(1);
    assert.equal(sim.chaos_info()[8], 1);
    const events = sim.chaos_events();
    assert.deepEqual(Array.from(events.slice(-3, -1)), [150, 4]);
    const span = events.at(-1);
    assert(span >= 1 && span <= 5);
    const cells = sim.snapshot(), links = sim.links();
    const recent = Array.from({ length: span }, (_, i) => newest + 1 + i);
    const modes = (sim.genome(recent[0]).length - 2) / model.MODE_STRIDE;
    for (let i = 0; i < recent.length; i++)
      assert.equal(sim.cell_mode(recent[i]), Math.min(i, modes - 1));
    const bonds = new Set(Array.from({ length: links.length / 2 }, (_, i) => `${links[i * 2]}/${links[i * 2 + 1]}`));
    for (let i = 1; i < recent.length; i++)
      assert(bonds.has(`${recent[i - 1]}/${recent[i]}`), "organism cells are not bonded in a chain");

    restored.restore_state(sim.export_state());
    restored.set_chaos_organisms(catalog);
    assert.deepEqual(restored.export_state(), sim.export_state());
    sim.configure_chaos(false, 10, false);
    restored.configure_chaos(false, 10, false);
    for (let i = 0; i < 10; i++) { sim.advance(120); restored.advance(120); }
    assert.deepEqual(sim.export_state(), restored.export_state());
    assert.throws(() => sim.configure_chaos(true, 0, true));
    assert.throws(() => sim.set_chaos_organisms(new Float32Array([99, 1])));
    assert.throws(() => sim.set_chaos_organisms(new Float32Array([1, 1, 2, 1, 1, 1])));
    assert.throws(() => sim.set_chaos_pool(new Float32Array()));
  } finally { sim.free(); restored.free(); }
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
    assert.deepEqual(sim.genome(id), model.flatten(model.inflate(g)));
    sim.advance(4);
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
    for (let i = 0; i < 10; i++) sim.advance(10);
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
