import assert from "node:assert/strict";
import { test, beforeEach, after } from "node:test";
import { mkdtemp, readFile, readdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import ts from "typescript";

// Compile the real modules into an isolated temporary directory for Node.
const directory = await mkdtemp(join(tmpdir(), "cellhub-game-tests-"));
for (const [source, target] of [["frontend/model.ts", "model.mjs"], ["frontend/game/storage.ts", "storage.mjs"], ["frontend/game/challenges.ts", "challenges.mjs"]]) {
  const original = await readFile(new URL(`../${source}`, import.meta.url), "utf8");
  const output = ts.transpileModule(original, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText
    .replaceAll('"../model"', '"./model.mjs"');
  await writeFile(join(directory, target), output);
}
after(() => rm(directory, { recursive: true, force: true }));
const storage = await import(pathToFileURL(join(directory, "storage.mjs")));
const { newMode, STRIDE } = await import(pathToFileURL(join(directory, "model.mjs")));
const { listCompleted, completeChallenge, TUTORIALS, evaluateTutorialStep, completeTutorial, listCompletedTutorials } = await import(pathToFileURL(join(directory, "challenges.mjs")));
const data = new Map();
const key = "cellhub.library.v1";
const genome = () => ({ genome: [newMode(0)], initialMode: 0 });
globalThis.localStorage = { getItem: k => data.get(k) ?? null, setItem: (k, v) => data.set(k, v), removeItem: k => data.delete(k) };
beforeEach(() => data.clear());

test("independent named entries survive export/import, rename and removal", () => {
  const a = storage.saveEntry("genome", "Первая", genome());
  const b = storage.saveEntry("genome", "Вторая", genome());
  storage.renameEntry(a.id, "Свет");
  assert.equal(storage.listEntries().length, 2);
  const imported = storage.importEntry(storage.exportEntry(b));
  assert.notEqual(imported.id, b.id);
  assert.equal(imported.name, "Вторая (2)");
  storage.removeEntry(b.id);
  assert.deepEqual(new Set(storage.listEntries().map(e => e.name)), new Set(["Свет", "Вторая (2)"]));
});

test("legacy migration is additive and a deleted migrated copy does not reappear", () => {
  const raw = JSON.stringify({ version: 2, modes: [newMode(0)] });
  data.set("cellhub.genome.v2", raw);
  const [entry] = storage.listEntries();
  assert.equal(entry.name, "Прежний геном");
  assert.equal(data.get("cellhub.genome.v2"), raw);
  storage.removeEntry(entry.id);
  assert.deepEqual(storage.listEntries(), []);
  assert.equal(data.get("cellhub.genome.v2"), raw);
});

test("corrupt libraries and invalid saves cannot overwrite existing bytes", () => {
  data.set(key, "broken");
  assert.throws(() => storage.saveEntry("genome", "Новая", genome()), /повреждена/);
  assert.equal(data.get(key), "broken");
  data.clear();
  storage.saveEntry("genome", "Хорошая", genome());
  const before = data.get(key);
  assert.throws(() => storage.saveEntry("genome", "Плохая", { ...genome(), initialMode: 10 }));
  assert.throws(() => storage.importEntry('{"format":"cellhub-library","version":9}'));
  assert.equal(data.get(key), before);
});

test("duplicate names, broken references, malformed environments and oversized imports are rejected", () => {
  storage.saveEntry("genome", "Имя", genome());
  assert.throws(() => storage.saveEntry("genome", " имя ", genome()), /существует/);
  assert.throws(() => storage.saveEntry("genome", "Сломанный", { genome: [{ ...newMode(0), childA: 2 }], initialMode: 0 }));
  assert.throws(() => storage.saveEntry("plate", "Чашка", { ...genome(), state: "{}", environment: {}, inserted: 0 }));
  assert.throws(() => storage.parseEntry(" ".repeat(20 * 1024 * 1024 + 1)), /большой/);
});

test("world metadata and distinct starting mode survive a round trip", () => {
  const plate = { genome: [newMode(0), newMode(1)], initialMode: 1, state: '{"version":1,"values":[1,123]}',
    environment: { light: 1, food: 1, radiation: 0, viscosity: 1, aging: true, killAtEdge: false, onlyPointMutations: false, gravity: 0, density: 0, nitrogen: 1 }, inserted: 3,
    slotNames: ["Рост", "Движение"], genomeNames: { "0,2.4,0,0": ["Старая клетка"], "1,3,1,0": ["Новый фагоцит", "Двигатель"] } };
  const saved = storage.saveEntry("plate", "Образец", plate);
  assert.deepEqual(storage.parseEntry(storage.exportEntry(saved)).data, plate);
  const chaos = storage.saveEntry("plate", "Мир хаоса", { ...plate, mode: "chaos" });
  assert.equal(storage.parseEntry(storage.exportEntry(chaos)).data.mode, "chaos");
  assert.throws(() => storage.validateData("plate", { ...plate, mode: "unknown" }));
  for (const environment of [{ ...plate.environment, light: 5.01 }, { ...plate.environment, food: -1 }, { ...plate.environment, viscosity: 0 }, { ...plate.environment, viscosity: 5.1 },
    { ...plate.environment, onlyPointMutations: 1 }, { ...plate.environment, gravity: -0.1 }, { ...plate.environment, gravity: 1.01 },
    { ...plate.environment, density: -3.31 }, { ...plate.environment, density: 3.31 }, { ...plate.environment, nitrogen: -1 }])
    assert.throws(() => storage.validateData("plate", { ...plate, environment }));
  const { onlyPointMutations, gravity, density, nitrogen, ...oldEnvironment } = plate.environment;
  assert.deepEqual(storage.validateData("plate", { ...plate, environment: oldEnvironment }).environment, plate.environment,
    "old saves gain neutral environment defaults without losing existing settings");
  for (const state of ['{}', '{"version":2,"values":[1]}', '{"version":1,"values":[null]}'])
    assert.throws(() => storage.validateData("plate", { ...plate, state }));
  assert.throws(() => storage.validateData("plate", { ...plate, genomeNames: { "bad": [null] } }));
});

test("challenge progress is idempotent and invalid progress is preserved", () => {
  completeChallenge("1"); completeChallenge("1");
  assert.deepEqual(listCompleted(), ["1"]);
  assert.throws(() => completeChallenge(""));
  data.set("cellhub.progress.v1", "broken");
  assert.throws(() => completeChallenge("2"));
  assert.equal(data.get("cellhub.progress.v1"), "broken");
});

function makeFrame(rows, links = []) {
  const cells = new Float32Array(rows.length * STRIDE);
  rows.forEach((row, i) => cells.set(row, i * STRIDE));
  return { type: "frame", cells, links: new Uint32Array(links), terrain: new Float32Array(), stats: new Float32Array([0, rows.length, 0, 0, 0]) };
}

test("tutorial requirements use live mechanics and progress does not overwrite challenges", () => {
  const state = { genome: [newMode(0)], initialMode: 0, inserted: 0, running: false, frame: null, selected: false, savedGenome: false };
  const editor = TUTORIALS.find(t => t.id === "genome");
  assert(!evaluateTutorialStep(editor.steps[0], state));
  state.genome.push(newMode(1));
  assert(evaluateTutorialStep(editor.steps[0], state));
  assert(!evaluateTutorialStep(editor.steps[1], state));
  state.initialMode = 1;
  assert(evaluateTutorialStep(editor.steps[1], state));
  state.genome[1].childA = 0;
  assert(evaluateTutorialStep(editor.steps[2], state));
  const organism = TUTORIALS.find(t => t.id === "organism");
  state.frame = makeFrame([[1,500,500,10,1,1,0,0,0,0],[2,518,500,10,2,1,0,0,0,0]], [1,2]);
  assert(evaluateTutorialStep(organism.steps.find(s=>s.requirement === "livingBond"), state));
  state.frame.links = new Uint32Array();
  assert(!evaluateTutorialStep(organism.steps.find(s=>s.requirement === "livingBond"), state));
  completeChallenge("1"); completeTutorial("genome"); completeTutorial("genome");
  assert.deepEqual(listCompleted(), ["1"]);
  assert.deepEqual(listCompletedTutorials(), ["genome"]);
});

test("organism lesson waits for the requested movement and linked daughter roles", () => {
  const organism = TUTORIALS.find(t => t.id === "organism");
  const roles = organism.steps.find(s => s.requirement === "roles");
  const adhesion = organism.steps.find(s => s.requirement === "adhesion");
  const state = { genome: [newMode(0), {...newMode(1),kind:1}, {...newMode(2),kind:2}],
    initialMode:0,inserted:0,running:false,frame:null,selected:false,savedGenome:false };
  assert(!evaluateTutorialStep(roles,state), "choosing the three types alone is not enough");
  state.genome[2].canDivide = false;
  assert(!evaluateTutorialStep(roles,state));
  state.genome[2].speed = 15;
  assert(evaluateTutorialStep(roles,state));
  state.genome[0].bindChildren = true;
  state.genome[0].childB = 1;
  assert(!evaluateTutorialStep(adhesion,state), "different children must have the requested roles");
  state.genome[0].childA = 2;
  assert(evaluateTutorialStep(adhesion,state));
  state.initialMode = 1;
  assert(!evaluateTutorialStep(adhesion,state), "the linked initial cell must be the photo role");
});

test("the first campaign manifest loads and its demo wins in the real WASM simulation", async () => {
  const { default: init, Simulation } = await import("../frontend/wasm/simulation.js");
  await init({ module_or_path: await readFile(new URL("../frontend/wasm/simulation_bg.wasm", import.meta.url)) });
  const text = await readFile(new URL("../levels/1.json", import.meta.url), "utf8");
  const level = JSON.parse(text);
  const sim = new Simulation(level.test.seed);
  try {
    const status = () => JSON.parse(sim.level_status());
    sim.load_level_json(text);
    const initial = status();
    assert.equal(initial.id, level.id);
    assert.equal(initial.state, "running");
    assert.equal(initial.demo, false);
    assert.equal(initial.counts.user, 0);
    assert.equal(initial.demoActions, level.demo.length);

    sim.start_level_demo();
    assert.equal(status().demo, true);
    sim.advance(1);
    let current = status();
    const opening = level.demo.filter(action => action.at === 0);
    assert(opening.length > 0);
    assert.equal(current.demoCursor, opening.length);
    assert.equal(current.counts.user, opening.filter(action => action.op === "spawn").length);

    for (let second = 0; second < 120 && current.state === "running"; second++) {
      sim.advance(30);
      current = status();
    }
    assert.equal(current.state, "won", JSON.stringify(current));
    const objective = current.conditions.find(condition => condition.key === "user-cells");
    assert.equal(objective?.target, level.conditions["user-cells"]);
    assert.equal(objective?.met, true);
    assert(current.counts.user >= objective.target);
  } finally { sim.free(); }
});


test("every offered campaign solution wins in real WASM without exceeding player limits", async () => {
  const { default: init, Simulation } = await import("../frontend/wasm/simulation.js");
  await init({ module_or_path: await readFile(new URL("../frontend/wasm/simulation_bg.wasm", import.meta.url)) });
  let checked = 0;
  for (const name of await readdir(new URL("../levels/", import.meta.url))) {
    if (!name.endsWith(".json") || name === "index.json") continue;
    const text = await readFile(new URL(`../levels/${name}`, import.meta.url), "utf8");
    const level = JSON.parse(text);
    if (level.tutorial || level.solutionStatus !== "verified") continue;
    const sim = new Simulation(level.test.seed);
    try {
      sim.load_level_json(text); sim.start_level_demo();
      let current = JSON.parse(sim.level_status());
      for (let second = 0; second < (level.test.maxSeconds ?? 600) && current.state === "running"; second++) {
        sim.advance(30); current = JSON.parse(sim.level_status());
        assert(!current.demoError, `${level.id}: ${current.demoError}`);
        assert(current.inserted <= level.budget, `${level.id}: placement budget`);
        if (level.removeMax !== null) assert(current.removed <= level.removeMax, `${level.id}: removal budget`);
      }
      assert.equal(current.state, "won", `${level.id}: ${JSON.stringify(current)}`);
      checked++;
    } finally { sim.free(); }
  }
  assert(checked >= 31, `Only ${checked} verified solutions were tested`);
});

test("WASM manual commands and demo playback share the campaign budget and tools", async () => {
  const { default: init, Simulation } = await import("../frontend/wasm/simulation.js");
  await init({ module_or_path: await readFile(new URL("../frontend/wasm/simulation_bg.wasm", import.meta.url)) });
  const level = JSON.parse(await readFile(new URL("../levels/1.json", import.meta.url), "utf8"));
  level.budget = 1;
  const sim = new Simulation(level.test.seed);
  try {
    sim.load_level_json(JSON.stringify(level)); sim.start_level_demo(); sim.advance(1);
    assert.equal(JSON.parse(sim.level_status()).inserted, 1);
    sim.stop_level_demo();
    const genes = flatten([{ ...newMode(0), speed: 0 }]);
    assert.throws(() => sim.spawn(1500, 1500, 0, genes));
    const id = sim.snapshot()[0];
    assert.throws(() => sim.boost(id));
    assert.throws(() => sim.remove(id));
    assert.throws(() => sim.move_cell(id, 1200, 1200));
    assert.equal(JSON.parse(sim.level_status()).inserted, 1);
    sim.load_level_json(JSON.stringify(level));
    const invalid = flatten([{ ...newMode(0), childA: 1 }, { ...newMode(1), kind: 3 }]);
    assert.throws(() => sim.spawn(1500, 1500, 0, invalid));
    assert.equal(JSON.parse(sim.level_status()).inserted, 0);
  } finally { sim.free(); }
});
