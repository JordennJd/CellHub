import assert from "node:assert/strict";
import { test, beforeEach, after } from "node:test";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
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
const { newMode, flatten, STRIDE } = await import(pathToFileURL(join(directory, "model.mjs")));
const { CHALLENGES, listCompleted, completeChallenge, evaluateChallenge, TUTORIALS, evaluateTutorialStep, completeTutorial, listCompletedTutorials } = await import(pathToFileURL(join(directory, "challenges.mjs")));
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
  completeChallenge(CHALLENGES[0].id); completeChallenge(CHALLENGES[0].id);
  assert.deepEqual(listCompleted(), [CHALLENGES[0].id]);
  assert.throws(() => completeChallenge("missing"));
  data.set("cellhub.progress.v1", "broken");
  assert.throws(() => completeChallenge(CHALLENGES[1].id));
  assert.equal(data.get("cellhub.progress.v1"), "broken");
});

function makeFrame(rows, links = [], tick = 0, infections = 0, births = 0) {
  const cells = new Float32Array(rows.length * STRIDE);
  rows.forEach((row, i) => cells.set(row, i * STRIDE));
  return { type: "frame", cells, links: new Uint32Array(links), terrain: new Float32Array(), stats: new Float32Array([tick, rows.length, births, 0, infections]) };
}
test("compound objectives require simultaneous criteria and legal insertion budget", () => {
  const adhesion = CHALLENGES.find(c => c.id === "adhesion");
  const rows = Array.from({length: 4}, (_, i) => [i + 1, 500, 500, 10, 1, 1, 1, 2, 0, 0]);
  assert(!evaluateChallenge(adhesion, makeFrame(rows), 1).complete);
  const connected = makeFrame(rows, [1, 2, 2, 3, 3, 4]);
  assert(evaluateChallenge(adhesion, connected, 1).complete);
  assert(!evaluateChallenge(adhesion, connected, 2).complete);
  assert(!evaluateChallenge(adhesion, connected, 0).complete);
  assert(!evaluateChallenge(adhesion, null, 1).complete);
  const survival = CHALLENGES.find(c => c.id === "survival");
  assert(!evaluateChallenge(survival, makeFrame([[1, 500, 500, 10, 4, 1, 0, 0, 0, 0]], [], 3000), 1).complete,
    "waiting before placing a fresh cell cannot satisfy survival");
  const virus = CHALLENGES.find(c => c.id === "viral-transfer");
  const viros = makeFrame([[1,500,500,10,3,1,0,0,0,0],[2,518,500,10,3,1,0,0,0,0]]);
  assert(!evaluateChallenge(virus, viros, 2).complete, "two directly inserted virocytes are not an infection");
  viros.stats[4] = 1;
  assert(evaluateChallenge(virus, viros, 2).complete);
});

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
  completeChallenge("algae"); completeTutorial("genome"); completeTutorial("genome");
  assert.deepEqual(listCompleted(), ["algae"]);
  assert.deepEqual(listCompletedTutorials(), ["genome"]);
});

test("role and survival objectives require the intended relationship on living cells", () => {
  const swimmer = CHALLENGES.find(c => c.id === "swimmer");
  const roles = [
    [1,500,500,10,0,1,12,1,0,0],
    [2,518,500,10,2,1,12,1,0,0],
    [3,536,500,10,1,1,12,1,0,0],
  ];
  assert(!evaluateChallenge(swimmer, makeFrame(roles, [2,3]), 1).complete,
    "a bond to a third role cannot replace the requested photo/flagellocyte bond");
  assert(evaluateChallenge(swimmer, makeFrame(roles, [2,1]), 1).complete,
    "the requested bond works regardless of endpoint order");
  const survival = CHALLENGES.find(c => c.id === "survival");
  const splitRequirements = [
    [1,500,500,10,4,0.2,45,1,0,0],
    [2,518,500,10,4,0.8,12,1,0,0],
  ];
  assert(!evaluateChallenge(survival, makeFrame(splitRequirements), 1).complete,
    "age and mass must belong to the same keratinocyte");
  splitRequirements[0][5] = 0.4;
  assert(evaluateChallenge(survival, makeFrame(splitRequirements), 1).complete);
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

test("every mechanic challenge has a measured legal solution in the real WASM simulation", async () => {
  const { default: init, Simulation } = await import("../frontend/wasm/simulation.js");
  await init({ module_or_path: await readFile(new URL("../frontend/wasm/simulation_bg.wasm", import.meta.url)) });
  const staticCell = kind => [{ ...newMode(0), kind, canDivide: false, speed: 0, lifespan: 1000 }];
  const frame = sim => ({type: "frame", cells: sim.snapshot(), links: sim.links(), terrain: sim.terrain(), stats: sim.stats()});
  for (const challenge of CHALLENGES) {
    const sim = new Simulation(73421);
    sim.set_circular_dish(true);
    let inserted = 0;
    const spawn = (x,y,genome,mode=0) => {
      assert(genome.every(m=>challenge.allowedKinds.includes(m.kind)), `${challenge.id}: solution uses allowed types`);
      sim.spawn(x,y,mode,flatten(genome)); inserted++;
    };
    try {
      const e = challenge.environment;
      sim.configure(e.light,e.food,e.radiation,e.viscosity,e.aging,e.killAtEdge,e.onlyPointMutations,e.gravity,e.density,e.nitrogen);
      if (challenge.id === "algae") spawn(600,600,challenge.genome);
      if (challenge.id === "feeding") spawn(1792,2112,challenge.genome.map(m=>({...m,kind:1})));
      if (challenge.id === "colony") spawn(600,600,challenge.genome);
      if (challenge.id === "adhesion") spawn(2000,1300,challenge.genome.map(m=>({...m,kind:1,bindChildren:true,keepAdhesionA:true,keepAdhesionB:true})));
      if (challenge.id === "swimmer") spawn(600,600,[
        {...newMode(0),kind:0,threshold:1.2,speed:0,childA:1,childB:2,bindChildren:true,lifespan:1000},
        {...newMode(1),kind:0,canDivide:false,speed:0,lifespan:1000},
        {...newMode(2),kind:2,canDivide:false,speed:15,lifespan:1000},
      ]);
      if (challenge.id === "survival") spawn(600,600,staticCell(4));
      if (challenge.id === "predation") {spawn(600,600,staticCell(5));spawn(618,600,staticCell(1));spawn(582,600,staticCell(1));}
      if (challenge.id === "viral-transfer") {spawn(600,600,staticCell(1));spawn(618,600,staticCell(3));}
      assert(inserted <= challenge.budget);
      let result = evaluateChallenge(challenge,frame(sim),inserted);
      for (let second=0;second<300 && !result.complete;second++) {
        sim.advance(30); result=evaluateChallenge(challenge,frame(sim),inserted);
      }
      assert(result.complete, `${challenge.id}: ${JSON.stringify(result.objectives)}`);
      console.log(`${challenge.id}: achieved at ${(sim.stats()[0]/30).toFixed(1)}s with ${inserted} insertions`);
    } finally { sim.free(); }
  }
});
