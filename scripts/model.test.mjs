import assert from "node:assert/strict";
import { test } from "node:test";
import { defaultGenome, newMode, newProgram, multicellularGenome, flatten, inflate, parseGenome, parseStoredGenome, serializeGenome, removeMode, modeColor, TYPES, MODE_STRIDE, MAX_MODES } from "../frontend/model.ts";

test("archetype settings and all adhesion flags survive encoding and saving", () => {
  const modes = multicellularGenome();
  modes.push({ ...newMode(3), bindChildren: true, keepAdhesionA: true, keepAdhesionB: true });
  assert.deepEqual(inflate(flatten(modes)), modes.map((m, i) => ({ ...m, name: `M${i + 1}` })));
  assert.deepEqual(parseGenome(serializeGenome(modes)), modes);
  assert.equal(modes[1].canDivide, false);
  assert.equal(modes[2].keepAdhesionA, false);
  assert.equal(modes[2].keepAdhesionB, true);
  assert.equal(modes[2].bindChildren, false);
});

test("the tutorial phagocyte keeps its heading when it retains itself", () => {
  const genome = multicellularGenome();
  const phagocyte = genome[2];
  assert.equal(phagocyte.childB, 2);
  assert.equal(phagocyte.keepAdhesionB, true);
  assert(Math.abs(phagocyte.childAngleB - Math.PI) < 1e-6, "the retained daughter turns away from the colony");
});

test("legacy eight-mode saves migrate without adding bonds", () => {
  const legacy = Array.from({ length: 8 }, (_, i) => newMode(i)).map(({kind,threshold,childA,childB,ratio,speed,angle,lifespan}) => ({kind,threshold,childA,childB,ratio,speed,angle,lifespan}));
  const loaded = parseGenome(JSON.stringify(legacy));
  assert.equal(loaded.length, 8);
  assert.equal(loaded[0].name, "M1");
  assert.deepEqual(inflate(new Float32Array(legacy.flatMap(Object.values))), loaded);
});

test("inspection preserves small coefficients and all float32 genome bits", () => {
  const coefficients = [1e-7, -3.7e-8, 0.123456789, 1.015e-12, 0.5, -999.997];
  const genome = [{ ...newMode(0), kind: 13, speed: 1e-7, sensorGain: -3.7e-8,
    programs: coefficients.map((value, channel) => ({
      ...newProgram(), target: channel, input: 8, scale: value,
      offset: value / 3, threshold: value / 7, below: -value,
    })),
  }];
  const encoded = flatten(genome);
  const decoded = inflate(encoded);
  assert.equal(decoded[0].programs[0].scale, 1e-7);
  assert.deepEqual(flatten(decoded), encoded);
  assert.deepEqual(flatten(parseGenome(serializeGenome(decoded))), encoded);
});

test("genomes accept up to a thousand modes and reject broken references and flags", () => {
  for (const n of [1, 3, 12, 19, 41, 100, MAX_MODES]) {
    const g = Array.from({ length: n }, (_, i) => newMode(i));
    assert.equal(parseGenome(serializeGenome(g)).length, n);
    assert.equal(inflate(flatten(g)).length, n);
  }
  const oversized = Array.from({ length: MAX_MODES + 1 }, (_, i) => newMode(i));
  assert.throws(() => parseGenome(serializeGenome(oversized)));
  assert.throws(() => inflate(flatten(oversized)));
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
  assert.deepEqual(removed.map((m) => [m.childA, m.childB]), [[1, 1], [0, 1]]);
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
  assert.equal(encoded[0], -5);
  assert.equal(encoded[1], genome.length);
  assert.deepEqual(inflate(encoded), genome);
  assert.deepEqual(parseGenome(serializeGenome(genome)), genome);
  // Library v1 wrappers used version2 before the extension; extra valid fields
  // must survive until the outer library format is changed independently.
  assert.deepEqual(parseGenome(JSON.stringify({version:2,modes:genome})), genome);
  assert.equal(modeColor(genome[0]), 0x3399e6);
  assert.equal(modeColor(genome[1]), 0xff0000);
});

test("resource settings survive ABI and library round trips and legacy modes get neutral defaults", () => {
  const mode = { ...newMode(0), kind: 6, maxConnections: 3, nutrientPriority: 2.5, prioritizeWhenStarving: true };
  assert.deepEqual(inflate(flatten([mode])), [mode]);
  assert.deepEqual(parseGenome(serializeGenome([mode])), [mode]);
  const old = flatten([newMode(0)]).slice(0, 21);
  old[0] = -3;
  assert.deepEqual(inflate(old), [newMode(0)]);
  const version4 = flatten([mode]).slice(0, 24);
  version4[0] = -4;
  assert.deepEqual(inflate(version4), [mode]);
  for (const [index, value] of [[19, 2.5], [19, -2], [20, 0], [21, 0.5]]) {
    const broken = flatten([mode]);
    broken[2 + index] = value;
    assert.throws(() => inflate(broken));
  }
});

test("main v4 genome files, library modes and raw rows migrate to the same v5 genome", () => {
  const modes = TYPES.map((_, kind) => ({
    name: `M${kind + 1}`, kind, threshold: 2.4, childA: 0, childB: 1, ratio: 0.5,
    speed: 21.37, angle: 0.8, lifespan: 240, canDivide: true, bindChildren: false,
    keepAdhesionA: false, keepAdhesionB: false, childAngleA: 0, childAngleB: 0,
    mirrorA: false, mirrorB: false, red: -1, green: -1, blue: -1,
    channel: 3, subtype: 0, signal: 0,
  }));
  Object.assign(modes[7], { signal: 1.25 });
  Object.assign(modes[8], { signal: 3 });
  Object.assign(modes[10], { subtype: 2, signal: 30 });
  Object.assign(modes[11], { subtype: 5, signal: 10 });
  Object.assign(modes[13], { signal: -25 });
  Object.assign(modes[14], { subtype: 10, signal: 45 });
  Object.assign(modes[16], { subtype: 15 });
  const raw = new Float32Array([-4, modes.length, ...modes.flatMap((m, index) => [
    ...flatten([{ ...newMode(index), ...m }]).slice(2, 21), m.channel, m.subtype, m.signal,
  ])]);
  const migrated = parseGenome(JSON.stringify({ version: 4, modes }));
  assert.deepEqual(parseStoredGenome(modes), migrated);
  assert.deepEqual(flatten(inflate(raw)), flatten(migrated));
  assert.deepEqual(parseGenome(serializeGenome(migrated)), migrated);
  assert.deepEqual(migrated.map(m => m.kind), [0, 1, 2, 3, 4, 5, 7, 8, 9, 6, 10, 11, 12, 13, 14, 15, 16, 17]);
  assert.equal(migrated[6].nutrientPriority, 4);
  assert.equal(migrated[7].density, -1.25);
  assert.deepEqual([migrated[10].sensor, migrated[11].sensor], [7, 8]);
  assert.deepEqual([migrated[13].signalChannel, migrated[13].sensorGain], [3, -2]);
  assert.deepEqual([migrated[14].secretion, migrated[14].secretionRate], [14, 3]);
  assert.deepEqual([migrated[15].transitionA, migrated[15].transitionB], [0, 1]);
  assert.deepEqual(migrated[15].programs.map(p => [p.target, p.input, p.threshold]), [[16, 1, 0.04], [17, 2, 0.04]]);
  assert.deepEqual([migrated[16].offspringMode, migrated[16].compatibleMode], [0, -16]);
  assert.equal(migrated[17].ciliaLeft, migrated[17].ciliaRight);
});

test("ambiguous raw v4 rows prefer main and malformed mixed layouts are rejected", () => {
  const baseRow = [...flatten([newMode(0)]).slice(2, 21)];
  const ambiguous = [6, ...baseRow.slice(1), 0, 1, 0];
  assert.equal(inflate(new Float32Array([-4, 1, ...ambiguous]))[0].kind, 7);
  const main = [10, ...baseRow.slice(1), 0, 0, 30];
  const branch = [...baseRow, -1, 1, 0];
  assert.throws(() => inflate(new Float32Array([-4, 2, ...main, ...branch])));
  assert.throws(() => inflate(new Float32Array([-4, 1, ...main.slice(0, 19), 4, 0, 30])));
});

test("old twelve-field genomes preserve type hues and daughter directions", () => {
  const old = Array.from({length:19}, (_,i) => {
    const {childAngleA,childAngleB,mirrorA,mirrorB,red,green,blue,...mode} = newMode(i);
    return {...mode,kind:i%6};
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

test("all eighteen types and eight signal equations survive the version-five formats", () => {
  assert.equal(TYPES.length, 18);
  assert.equal(MODE_STRIDE, 102);
  const modes = TYPES.map((_, index) => ({
    ...newMode(index), kind: index, density: -1.1, signalChannel: 7,
    sensor: 3, sensorGain: -2.5, targetRed: 0.2, targetGreen: 0.4, targetBlue: 0.8,
    senseTolerance: 0.3, secretion: 22, secretionRate: 2.5,
    transitionA: (index + 1) % TYPES.length, transitionB: (index + 2) % TYPES.length,
    transitionThresholdA: -0.4, transitionThresholdB: 0.7,
    offspringMode: (index + 3) % TYPES.length, compatibleMode: (index + 4) % TYPES.length,
    telomeres: 23, ciliaLeft: -15, ciliaRight: 21,
    contraction: -0.6, bending: 0.2, lift: -0.3, virusCopyFrom: index,
    programs: Array.from({ length: 8 }, (_, channel) => ({
      ...newProgram(), target: channel, input: channel + 1, kind: channel % 2,
      scale: -2, offset: 3, threshold: 0.5, below: -1,
    })),
  }));
  const encoded = flatten(modes);
  assert.equal(encoded.length, 2 + 18 * 102);
  assert.deepEqual(inflate(encoded), modes);
  assert.deepEqual(parseGenome(serializeGenome(modes)), modes);
  assert.equal(JSON.parse(serializeGenome(modes)).version, 5);
  assert.deepEqual(inflate(encoded)[13].programs[7], {
    target: 7, input: 8, kind: 1, scale: -2, offset: 3, threshold: 0.5, below: -1,
  });
});

test("advanced mode references are validated and remapped on deletion", () => {
  const referenceKeys = ["transitionA", "transitionB", "offspringMode", "virusCopyFrom"];
  for (const key of referenceKeys) {
    assert.throws(() => serializeGenome([{ ...newMode(0), [key]: 1 }]));
    assert.throws(() => serializeGenome([{ ...newMode(0), [key]: -2 }]));
  }
  for (const key of referenceKeys.filter(key => key !== "virusCopyFrom"))
    assert.throws(() => serializeGenome([{ ...newMode(0), [key]: -1 }]));
  for (const compatibleMode of [-17, -1.5, 1])
    assert.throws(() => serializeGenome([{ ...newMode(0), compatibleMode }]));
  const genome = Array.from({ length: 3 }, (_, index) => ({
    ...newMode(index), transitionA: 2, transitionB: 1, offspringMode: 2, compatibleMode: index === 0 ? -16 : 1,
    virusCopyFrom: index === 0 ? -1 : 2, programs: [{ ...newProgram(), target: 7, input: 8 }],
  }));
  const removed = removeMode(genome, 1);
  assert.deepEqual(removed.map(mode => [mode.transitionA, mode.transitionB, mode.offspringMode, mode.compatibleMode, mode.virusCopyFrom]),
    [[1, 1, 1, -16, -1], [1, 1, 1, 1, 1]]);
  assert.deepEqual(parseGenome(serializeGenome(removed)), removed);
  assert.deepEqual(flatten(inflate(flatten(removed))), flatten(removed));
  assert.equal(genome[0].transitionA, 2);
  assert.equal(removed[0].programs[0].target, 7);
});

test("advanced fields and program headers reject unsupported values", () => {
  for (const invalid of [
    { density: 3.31 }, { signalChannel: 8 }, { signalChannel: -1 }, { sensor: 9 },
    { sensorGain: 16.01 }, { senseTolerance: -0.01 }, { targetRed: -1, targetGreen: 0, targetBlue: 0 },
    { secretion: 23 }, { secretion: 1.5 }, { secretionRate: 4.01 },
    { transitionThresholdA: 1001 }, { ciliaLeft: -61 }, { ciliaRight: 61 },
    { contraction: 1.01 }, { bending: -1.01 }, { lift: 1.01 },
    { telomeres: 1.5 }, { telomeres: -2 }, { telomeres: 1001 }, { programs: undefined },
    { programs: Array.from({ length: 9 }, newProgram) },
  ]) assert.throws(() => serializeGenome([{ ...newMode(0), ...invalid }]));
  for (const invalid of [
    { target: 18 }, { input: 14 }, { kind: 2 }, { scale: 1001 },
    { offset: -1001 }, { threshold: NaN }, { below: Infinity },
  ]) assert.throws(() => serializeGenome([{ ...newMode(0), programs: [{ ...newProgram(), ...invalid }] }]));
  const encoded = flatten([newMode(0)]);
  for (const count of [-1, 8.5, 9]) {
    const broken = encoded.slice();
    broken[2 + 45] = count;
    assert.throws(() => inflate(broken));
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
