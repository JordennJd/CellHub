import assert from "node:assert/strict";
import { test } from "node:test";
import { defaultGenome, newMode, multicellularGenome, flatten, inflate, parseGenome, serializeGenome, removeMode, modeColor, TYPES, MODE_STRIDE } from "../frontend/model.ts";

test("archetype settings and all adhesion flags survive encoding and saving", () => {
  const modes = multicellularGenome();
  modes.push({ ...newMode(3), bindChildren: true, keepAdhesionA: true, keepAdhesionB: true });
  assert.deepEqual(inflate(flatten(modes)), modes.map((m, i) => ({ ...m, name: `M${i + 1}` })));
  assert.deepEqual(parseGenome(serializeGenome(modes)), modes);
  assert.equal(modes[1].canDivide, false);
  assert.equal(modes[2].keepAdhesionA, true);
  assert.equal(modes[2].keepAdhesionB, false);
  assert.equal(modes[2].bindChildren, false);
});

test("legacy eight-mode saves migrate without adding bonds", () => {
  const legacy = Array.from({ length: 8 }, (_, i) => newMode(i)).map(({kind,threshold,childA,childB,ratio,speed,angle,lifespan}) => ({kind,threshold,childA,childB,ratio,speed,angle,lifespan}));
  const loaded = parseGenome(JSON.stringify(legacy));
  assert.equal(loaded.length, 8);
  assert.equal(loaded[0].name, "M1");
  assert.deepEqual(inflate(new Float32Array(legacy.flatMap(Object.values))), loaded);
});

test("genomes accept unbounded slots and reject broken references and flags", () => {
  for (const n of [1, 3, 12, 19, 41, 100]) {
    const g = Array.from({ length: n }, (_, i) => newMode(i));
    assert.equal(parseGenome(serializeGenome(g)).length, n);
    assert.equal(inflate(flatten(g)).length, n);
  }
  for (const invalid of [[],
    [{ ...newMode(0), childA: 1 }], [{ ...newMode(0), childB: -1 }],
    [{ ...newMode(0), kind: 0.5 }], [{ ...newMode(0), bindChildren: 1 }],
    [{ ...newMode(0), canDivide: undefined }], [{ ...newMode(0), speed: NaN }]]) {
    assert.throws(() => parseGenome(JSON.stringify({ version: 2, modes: invalid })));
  }
  assert.throws(() => parseGenome('{"version":3,"modes":[]}'));
});

test("deleting an archetype remaps incoming references and permits cycles", () => {
  const g = multicellularGenome();
  const removed = removeMode(g, 1);
  assert.equal(removed.length, 2);
  assert.deepEqual(removed.map((m) => [m.childA, m.childB]), [[1, 1], [1, 0]]);
  const last = removeMode(g, 2);
  assert.deepEqual(last.map((m) => [m.childA, m.childB]), [[1, 1], [1, 1]]);
  const only = [newMode(0)];
  assert.equal(removeMode(only, 0), only);
  assert.equal(g.length, 3);
  assert.deepEqual(parseGenome(serializeGenome(removed)), removed);
});

test("stress populations stay within the world without clamping or duplicate positions", async () => {
  const { initialPopulation, POPULATION_LIMIT, WORLD } = await import("../frontend/model.ts");
  for (const count of [1, 60, 5000, POPULATION_LIMIT]) {
    const points = initialPopulation(count);
    assert.equal(points.length, count);
    assert.equal(new Set(points.map(({x,y}) => `${x}/${y}`)).size, count);
    assert(points.every(({x,y,mode}) => x > 12 && x < WORLD - 12 && y > 12 && y < WORLD - 12 && mode >= 0 && mode < 6));
  }
  assert.equal(initialPopulation(POPULATION_LIMIT + 1).length, POPULATION_LIMIT);
  assert.equal(initialPopulation(2.9).length, 2);
  for (const count of [0, -1, NaN, Infinity]) assert.deepEqual(initialPopulation(count), []);
});

test("single photocyte default and slot names survive saving and deletion", () => {
  assert.deepEqual(defaultGenome(), [newMode(0)]);
  const g = multicellularGenome();
  g[2].name = "Двигатель";
  assert.equal(parseGenome(serializeGenome(g))[2].name, "Двигатель");
  assert.equal(removeMode(g, 1)[1].name, "Двигатель");
  assert.throws(() => parseGenome(serializeGenome([{ ...newMode(0), name: " " }])));
});

test("independent daughter directions, handedness and colors survive all current formats", () => {
  const genome = multicellularGenome();
  Object.assign(genome[0], { childAngleA: 1.2, childAngleB: -.9, mirrorA: true, mirrorB: false, red: .2, green: .6, blue: .9 });
  Object.assign(genome[1], { mirrorB: true, red: 1, green: 0, blue: 0 });
  const encoded = flatten(genome);
  assert.equal(encoded.length, 2 + genome.length * MODE_STRIDE);
  assert.equal(encoded[0], -3);
  assert.equal(encoded[1], genome.length);
  assert.deepEqual(inflate(encoded), genome);
  assert.deepEqual(parseGenome(serializeGenome(genome)), genome);
  // Library v1 wrappers used version2 before the extension; extra valid fields
  // must survive until the outer library format is changed independently.
  assert.deepEqual(parseGenome(JSON.stringify({version:2,modes:genome})), genome);
  assert.equal(modeColor(genome[0]), 0x3399e6);
  assert.equal(modeColor(genome[1]), 0xff0000);
});

test("old twelve-field genomes preserve type hues and daughter directions", () => {
  const old = Array.from({length:19}, (_,i) => {
    const {childAngleA,childAngleB,mirrorA,mirrorB,red,green,blue,...mode} = newMode(i);
    return {...mode,kind:i%TYPES.length};
  });
  const migrated = parseGenome(JSON.stringify({version:2,modes:old}));
  const raw = new Float32Array(old.flatMap(m => [m.kind,m.threshold,m.childA,m.childB,m.ratio,m.speed,m.angle,m.lifespan,
    Number(m.canDivide),Number(m.bindChildren),Number(m.keepAdhesionA),Number(m.keepAdhesionB)]));
  // 228 values are divisible by both12 and19; header disambiguates new values.
  assert.equal(raw.length,228);
  assert.deepEqual(inflate(raw), migrated);
  for(const m of migrated) {
    assert.deepEqual([m.childAngleA,m.childAngleB,m.mirrorA,m.mirrorB],[0,0,false,false]);
    assert.deepEqual([m.red,m.green,m.blue],[-1,-1,-1]);
    assert.equal(modeColor(m),TYPES[m.kind].color);
  }
});

test("extended genomes reject mixed colors, nonfinite angles, missing fields and malformed headers", () => {
  for(const change of [
    {childAngleA:4}, {childAngleB:-4}, {childAngleB:null}, {childAngleA:undefined},
    {mirrorA:1}, {mirrorB:null}, {red:-1,green:0,blue:1}, {red:1.01,green:0,blue:0},
    {red:0,green:NaN,blue:0}, {green:undefined},
  ]) assert.throws(()=>parseGenome(JSON.stringify({version:3,modes:[{...newMode(0),...change}]})));
  const valid=flatten([newMode(0)]);
  for(const [index,value] of [[1,0],[1,2],[1,1.5],[2+14,.5],[2+12,4],[2+16,NaN]]) {
    const broken=valid.slice();broken[index]=value;assert.throws(()=>inflate(broken));
  }
  assert.throws(()=>inflate(valid.slice(0,-1)));
  const selected=removeMode([{...newMode(0),red:1,green:0,blue:0},{...newMode(1),mirrorA:true}],0);
  assert.equal(selected[0].mirrorA,true);
  assert.equal(selected[0].childA,0);
});
