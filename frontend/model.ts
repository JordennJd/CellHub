export const WORLD = 3072;
export const POPULATION_LIMIT = 20_000;
export const MAX_MODES = 1000;
export const STRIDE = 12;
export const DIVISION_STRIDE = 8;
export const MODE_STRIDE = 102;
export const CELL_DETAILS_STRIDE = 13;
export const SIGNAL_COUNT = 8;
export const GAMETE_GROUP_COUNT = 16;
export const GENOME_ABI_VERSION = -5;
export const TYPES = [
  {
    name: "Фотоцит",
    latin: "Photocyte",
    color: 0xa4db71,
    info: "Преобразует свет в массу. В слое «Свет» насыщенный жёлтый цвет обозначает больше света.",
  },
  {
    name: "Фагоцит",
    latin: "Phagocyte",
    color: 0xf1bf72,
    info: "Поглощает питательные вещества среды. В слое «Пища» насыщенный зелёный цвет обозначает больше пищи.",
  },
  {
    name: "Жгутикоцит",
    latin: "Flagellocyte",
    color: 0x71cbd4,
    info: "Активно движется с заданной скоростью, расходуя массу.",
  },
  {
    name: "Вироцит",
    latin: "Virocyte",
    color: 0xc997ed,
    info: "Заражает незащищённую клетку своим геномом или выбранным фрагментом. Перезарядка — 3 секунды.",
  },
  {
    name: "Кератиноцит",
    latin: "Keratinocyte",
    color: 0xb3c3d0,
    info: "Защищает себя и непосредственно связанных соседей от поедания, инфекции, приклеивания и внешних токсинов.",
  },
  {
    name: "Девороцит",
    latin: "Devorocyte",
    color: 0xf08373,
    info: "Забирает массу и азот незащищённых соседей; вместе с ними может получить токсин жертвы.",
  },
  {
    name: "Нитроцит",
    latin: "Nitrocyte",
    color: 0x8ed6b3,
    info: "Пополняет запас азота и передаёт избыток по постоянным связям.",
  },

  { name: "Липоцит", latin: "Lipocyte", color: 0xe4b978, info: "Экономно хранит энергию, блокирует передачу сигналов и после гибели оставляет покрытую пищу." },
  { name: "Буйоцит", latin: "Buoyocyte", color: 0x8ebfe8, info: "Меняет плотность и всплывает или тонет при включённой гравитации." },
  { name: "Глюоцит", latin: "Glueocyte", color: 0xccbd78, info: "Приклеивается к незащищённым соседям и стенке чашки." },
  { name: "Стереоцит", latin: "Stereocyte", color: 0xb8a0e5, info: "Преобразует разницу стимула слева и справа в знаковый сигнал." },
  { name: "Сенсоцит", latin: "Senseocyte", color: 0xa7cbe6, info: "Измеряет стимул среды и передаёт его в выбранный сигнальный канал." },
  { name: "Миоцит", latin: "Myocyte", color: 0xe68fa1, info: "Сокращает, изгибает и смещает связанные клетки." },
  { name: "Нейроцит", latin: "Neurocyte", color: 0xb89ce0, info: "Вычисляет до восьми уравнений и управляет сигналами S1–S8." },
  { name: "Секретоцит", latin: "Secrocyte", color: 0xdca18c, info: "Выделяет выбранное вещество или изменяет сигналы соседей." },
  { name: "Стемоцит", latin: "Stemocyte", color: 0x9ed8bd, info: "Меняет активный режим по сигналу или программируемому условию без деления." },
  { name: "Гамета", latin: "Gamete", color: 0xe7c893, info: "Сливается с другой гаметой, смешивая геномы. Обычное деление отключено." },
  { name: "Цилиоцит", latin: "Ciliocyte", color: 0x88c9c3, info: "Раздельно управляет левыми и правыми ресничками и перемещает соседей при контакте." },
];
export const PROGRAM_TARGETS = [
  ...Array.from({ length: 8 }, (_, i) => "S" + (i + 1)),
  "Скорость", "Плотность", "Сокращение", "Изгиб", "Подъём",
  "Левые реснички", "Правые реснички", "Интенсивность секреции", "Переход A", "Переход B",
];
export const PROGRAM_INPUTS = [
  "Константа", ...Array.from({ length: 8 }, (_, i) => "S" + (i + 1)),
  "Масса", "Возраст", "Азот", "Число связей", "Таймер sin(возраст)",
];
export const SENSORS = ["Свет", "Пища", "Покрытая пища", "Клетки", "Стенка", "Скорость", "Нитраты", "Липоциты рядом", "Жгутикоциты и цилиоциты рядом"];
export const SECRETIONS = [
  "HCN", "Липаза", "Протеаза", "Запах пищи", "Запах покрытой пищи", "Стимул света", "Стимул стенки",
  ...Array.from({ length: 8 }, (_, i) => ["+S" + (i + 1), "−S" + (i + 1)]).flat(),
];
export type Program = {
  target: number;
  input: number;
  kind: number;
  scale: number;
  offset: number;
  threshold: number;
  below: number;
};
export const newProgram = (): Program => ({ target: 0, input: 0, kind: 0, scale: 1, offset: 0, threshold: 0.5, below: 0 });
const emptyProgram = (): Program => ({
  target: 0, input: 0, kind: 0, scale: 0, offset: 0, threshold: 0, below: 0,
});
function validProgram(value: unknown): value is Program {
  if (!value || typeof value !== "object") return false;
  const p = value as Program;
  return Number.isInteger(p.target) && p.target >= 0 && p.target <= 17
    && Number.isInteger(p.input) && p.input >= 0 && p.input <= 13
    && [0, 1].includes(p.kind)
    && [p.scale, p.offset, p.threshold, p.below].every(v =>
      typeof v === "number" && Number.isFinite(v) && Math.abs(v) <= 1000);
}
export type Mode = {
  name: string;
  kind: number;
  threshold: number;
  childA: number;
  childB: number;
  ratio: number;
  speed: number;
  angle: number;
  lifespan: number;
  canDivide: boolean;
  bindChildren: boolean;
  keepAdhesionA: boolean;
  keepAdhesionB: boolean;
  childAngleA: number;
  childAngleB: number;
  mirrorA: boolean;
  mirrorB: boolean;
  red: number;
  green: number;
  blue: number;
  maxConnections: number;
  nutrientPriority: number;
  prioritizeWhenStarving: boolean;
  density: number;
  signalChannel: number;
  sensor: number;
  sensorGain: number;
  targetRed: number;
  targetGreen: number;
  targetBlue: number;
  senseTolerance: number;
  secretion: number;
  secretionRate: number;
  transitionA: number;
  transitionB: number;
  transitionThresholdA: number;
  transitionThresholdB: number;
  offspringMode: number;
  compatibleMode: number; // Nonnegative mode index, or -(group + 1).
  telomeres: number;
  ciliaLeft: number;
  ciliaRight: number;
  contraction: number;
  bending: number;
  lift: number;
  virusCopyFrom: number;
  programs: Program[];
};
export type NumericModeKey = {
  [K in keyof Mode]: Mode[K] extends number ? K : never;
}[keyof Mode];
export const newMode = (index: number): Mode => ({
  name: `M${index + 1}`, kind: 0, threshold: 2.4, childA: index, childB: index,
  ratio: 0.5, speed: 12, angle: 0.8, lifespan: 240,
  canDivide: true, bindChildren: false, keepAdhesionA: false, keepAdhesionB: false,
  childAngleA: 0, childAngleB: 0, mirrorA: false, mirrorB: false,
  red: -1, green: -1, blue: -1,
  maxConnections: -1, nutrientPriority: 1, prioritizeWhenStarving: false,
  density: 0, signalChannel: 0, sensor: 0, sensorGain: 1,
  targetRed: -1, targetGreen: -1, targetBlue: -1, senseTolerance: 0.2,
  secretion: 0, secretionRate: 0.2, transitionA: index, transitionB: index,
  transitionThresholdA: 0.5, transitionThresholdB: 1,
  offspringMode: index, compatibleMode: index, telomeres: -1,
  ciliaLeft: 6, ciliaRight: 6, contraction: 0.3, bending: 0, lift: 0,
  virusCopyFrom: -1, programs: [],
});
export const defaultGenome = (): Mode[] =>
  [newMode(0)];

export const multicellularGenome = (): Mode[] => [
  { ...newMode(0), kind: 0, threshold: 2, childA: 1, childB: 2, bindChildren: true, angle: 0, lifespan: 1000 },
  { ...newMode(1), kind: 2, canDivide: false, speed: 40, lifespan: 1000 },
  { ...newMode(2), kind: 1, threshold: 2.4, childA: 0, childB: 2, keepAdhesionB: true, angle: 0, lifespan: 1000 },
];

// Header avoids ambiguity between old 12-number rows and extended rows.
export const flatten = (g: Mode[]) => new Float32Array([GENOME_ABI_VERSION, g.length, ...g.flatMap((m) => [
  m.kind, m.threshold, m.childA, m.childB, m.ratio, m.speed, m.angle, m.lifespan,
  Number(m.canDivide), Number(m.bindChildren), Number(m.keepAdhesionA), Number(m.keepAdhesionB),
  m.childAngleA, m.childAngleB, Number(m.mirrorA), Number(m.mirrorB), m.red, m.green, m.blue,
  m.maxConnections, m.nutrientPriority, Number(m.prioritizeWhenStarving),
  m.density, m.signalChannel, m.sensor, m.sensorGain, m.targetRed, m.targetGreen, m.targetBlue, m.senseTolerance,
  m.secretion, m.secretionRate, m.transitionA, m.transitionB, m.transitionThresholdA, m.transitionThresholdB,
  m.offspringMode, m.compatibleMode, m.telomeres, m.ciliaLeft, m.ciliaRight, m.contraction, m.bending, m.lift,
  m.virusCopyFrom, m.programs.length,
  ...Array.from({ length: 8 }, (_, i) => m.programs[i] ?? emptyProgram()).flatMap(p =>
    [p.target, p.input, p.kind, p.scale, p.offset, p.threshold, p.below]),
])]);

function float32Decimal(value: number): number {
  if (value === 0) return value;
  // Display a short decimal while preserving the exact value sent back to Rust.
  for (let digits = 1; digits <= 9; digits++) {
    const decimal = Number(value.toPrecision(digits));
    if (Math.fround(decimal) === value) return decimal;
  }
  return value;
}

function validMainV4(kind: number, channel: number, subtype: number, signal: number): boolean {
  return Number.isInteger(kind) && kind >= 0 && kind < TYPES.length
    && Number.isInteger(channel) && channel >= 0 && channel <= 3
    && Number.isInteger(subtype) && subtype >= 0 && subtype <= 15
    && Number.isFinite(signal) && signal >= -60 && signal <= 60;
}

type MainV4Mode = Pick<Mode, "kind" | "speed" | "childA" | "childB"> & {
  channel: number;
  subtype: number;
  signal: number;
};

// Preserve the four-channel controls of the old main schema in v5 fields
// and equations. Round arithmetic as Rust does so inspection and restored
// genome names use the same bytes as a world migrated by the engine.
function migrateMainV4(m: MainV4Mode): Partial<Mode> {
  const divide = (a: number, b: number) => float32Decimal(Math.fround(Math.fround(a) / Math.fround(b)));
  const rule = (target: number, input: number, scale: number, offset: number, threshold: number, below: number): Program =>
    ({ target, input, kind: 1, scale, offset, threshold, below });
  switch (m.kind) {
    case 2: {
      const scale = float32Decimal(Math.fround(Math.fround(m.speed) / 4.8));
      return { programs: [
        rule(8, 1, scale, m.speed, -3.84, divide(m.speed, 5)),
        rule(8, 1, -scale, m.speed, 4.8, 0),
      ] };
    }
    case 6: return { kind: 7, nutrientPriority: 4 };
    case 7: return { kind: 8, density: Math.max(-3.3, Math.min(3.3, -m.signal)) };
    case 8: return { kind: 9, maxConnections: Math.max(1, Math.min(5, Math.round(m.signal))) };
    case 9: return { kind: 6 };
    case 10:
    case 11:
      return { signalChannel: m.channel, sensor: [3, 1, 7, 0, 4, 8][m.subtype] ?? 3,
        sensorGain: m.subtype < 6 ? divide(Math.max(m.signal, 0), 12.5) : 0 };
    case 12: {
      const scale = divide(m.speed, 288);
      return { programs: [
        rule(10, 1, scale, 0, 0, 0),
        rule(10, 1, -scale, divide(m.speed, 60), 4.8, 0),
      ] };
    }
    case 13: return { signalChannel: m.channel, sensorGain: divide(m.signal, 12.5) };
    case 14:
      return {
        secretion: [0, 1, 2, 7, 9, 11, 13, 8, 10, 12, 14, 3, 4, 5, 6][m.subtype] ?? 0,
        secretionRate: m.subtype === 15 ? 0 : divide(Math.max(m.subtype <= 1 ? 1 : 0, m.signal), 15),
      };
    case 15:
      return { transitionA: m.childA, transitionB: m.childB,
        programs: [rule(16, 1, 0, 1, 0.04, -1), rule(17, 2, 0, 1, 0.04, -1)] };
    case 16: return { offspringMode: m.childA, compatibleMode: -(m.subtype + 1) };
    case 17: return { ciliaLeft: divide(m.speed, 2), ciliaRight: divide(m.speed, 2) };
    default: return {};
  }
}

export function inflate(g: Float32Array): Mode[] {
  const current = g[0] === GENOME_ABI_VERSION;
  const version4 = g[0] === -4;
  const extended = current || version4 || g[0] === -3;
  const legacy = !extended && g.length === 64;
  const stride = current ? MODE_STRIDE : version4 ? 22 : extended ? 19 : legacy ? 8 : 12;
  const offset = extended ? 2 : 0;
  const count = extended ? g[1] : g.length / stride;
  if (!Number.isInteger(count) || count < 1 || count > MAX_MODES
    || g.length !== offset + count * stride || !g.every(Number.isFinite))
    throw new Error("Неверный размер или данные генома");
  // Both v4 layouts have the same header and row length. Recognize the old
  // main layout for the entire genome before interpreting the resource fields.
  const mainV4 = version4 && Array.from({ length: count }, (_, i) => offset + i * stride)
    .every(at => validMainV4(g[at], g[at + 19], g[at + 20], g[at + 21]));
  const resourceFields = current || (version4 && !mainV4);
  const modes = Array.from({ length: count }, (_, i) => {
    const a = Array.from(g.slice(offset + i * stride, offset + (i + 1) * stride), float32Decimal);
    const maxKind = current || mainV4 ? TYPES.length - 1 : version4 ? 6 : 5;
    if (a[0] > maxKind) throw new Error("Тип клетки недоступен в этой версии генома");
    if (current && (!Number.isInteger(a[45]) || a[45] < 0 || a[45] > 8)) throw new Error("Неверное число уравнений");
    if (current) {
      for (let row = 0; row < 8; row++) {
        const at = 46 + row * 7;
        if (!validProgram({
          target: a[at], input: a[at + 1], kind: a[at + 2], scale: a[at + 3],
          offset: a[at + 4], threshold: a[at + 5], below: a[at + 6],
        })) throw new Error("Некорректное уравнение");
      }
    }
    const flags = legacy ? [] : a.slice(8, 12);
    if (extended) flags.push(...a.slice(14, 16));
    if (resourceFields) flags.push(a[21]);
    if (flags.some(v => v !== 0 && v !== 1)) throw new Error("Некорректные переключатели генома");
    return {
      ...newMode(i), kind: a[0], threshold: a[1], childA: a[2], childB: a[3],
      ratio: a[4], speed: a[5], angle: a[6], lifespan: a[7],
      canDivide: legacy || a[8] === 1, bindChildren: !legacy && a[9] === 1,
      keepAdhesionA: !legacy && a[10] === 1, keepAdhesionB: !legacy && a[11] === 1,
      ...(extended ? { childAngleA: a[12], childAngleB: a[13], mirrorA: a[14] === 1, mirrorB: a[15] === 1,
        red: a[16], green: a[17], blue: a[18] } : {}),
      ...(resourceFields ? { maxConnections: a[19], nutrientPriority: a[20], prioritizeWhenStarving: a[21] === 1 } : {}),
      ...(mainV4 ? { channel: a[19], subtype: a[20], signal: a[21] } : {}),
      ...(current ? {
        density:a[22],signalChannel:a[23],sensor:a[24],sensorGain:a[25],
        targetRed:a[26],targetGreen:a[27],targetBlue:a[28],senseTolerance:a[29],
        secretion:a[30],secretionRate:a[31],transitionA:a[32],transitionB:a[33],
        transitionThresholdA:a[34],transitionThresholdB:a[35],
        offspringMode:a[36],compatibleMode:a[37],telomeres:a[38],ciliaLeft:a[39],ciliaRight:a[40],
        contraction:a[41],bending:a[42],lift:a[43],virusCopyFrom:a[44],
        programs:Array.from({length: Math.max(0, Math.min(8, a[45]))},(_,i) => {
          const row=a.slice(46+i*7,53+i*7);
          return {target:row[0],input:row[1],kind:row[2],scale:row[3],offset:row[4],threshold:row[5],below:row[6]};
        }),
      } : {}),
    };
  });
  return validateModes(modes, mainV4 ? 4 : 5);
}

// Removing an archetype redirects references to it to the following archetype
// (or the preceding one for the last entry), then renumbers all references.
export function removeMode(genome: Mode[], index: number): Mode[] {
  if (genome.length <= 1 || index < 0 || index >= genome.length) return genome;
  const reference = (v: number) => v > index ? v - 1 : v === index ? Math.min(index, genome.length - 2) : v;
  return genome.filter((_, i) => i !== index).map((m) => ({
    ...m, childA: reference(m.childA), childB: reference(m.childB),
    transitionA: reference(m.transitionA), transitionB: reference(m.transitionB),
    offspringMode: reference(m.offspringMode), compatibleMode: reference(m.compatibleMode),
    virusCopyFrom: m.virusCopyFrom < 0 ? -1 : reference(m.virusCopyFrom),
  }));
}

export function serializeGenome(genome: Mode[]): string {
  return JSON.stringify({ version: 5, modes: validateModes(genome, 5) });
}

function hasMainV4Fields(modes: unknown): boolean {
  return Array.isArray(modes) && modes.some(mode =>
    mode && typeof mode === "object" && ["channel", "subtype", "signal"].some(key => Object.hasOwn(mode, key)));
}

/** Library entries predate an explicit genome version. Named v4 fields identify the old main schema. */
export function parseStoredGenome(modes: unknown): Mode[] {
  return validateModes(modes, hasMainV4Fields(modes) ? 4 : 2);
}

function validateModes(modes: unknown, version: 1 | 2 | 3 | 4 | 5): Mode[] {
  if (!Array.isArray(modes) || modes.length < 1 || modes.length > MAX_MODES || (version === 1 && modes.length !== 8))
    throw new Error("Неверный формат генома");
  const numeric: NumericModeKey[] = ["kind", "threshold", "childA", "childB", "ratio", "speed", "angle", "lifespan"];
  const extension = ["childAngleA", "childAngleB", "mirrorA", "mirrorB", "red", "green", "blue"] as const;
  const advancedKeys = ["density", "signalChannel", "sensor", "sensorGain", "targetRed", "targetGreen", "targetBlue", "senseTolerance", "secretion", "secretionRate", "transitionA", "transitionB", "transitionThresholdA", "transitionThresholdB", "offspringMode", "compatibleMode", "telomeres", "ciliaLeft", "ciliaRight", "contraction", "bending", "lift", "virusCopyFrom", "programs"] as const;
  const resourceFields = ["maxConnections", "nutrientPriority", "prioritizeWhenStarving"] as const;
  const mainV4 = version === 4 && hasMainV4Fields(modes);
  return modes.map((source, i) => {
    if (!source || typeof source !== "object" || Array.isArray(source)) throw new Error("Некорректные настройки режима");
    const m = { ...source };
    if (version < 3) for (const key of extension) if (m[key] === undefined) m[key] = newMode(i)[key];
    if (version < 5) {
      const defaults = newMode(i);
      for (const key of advancedKeys) if (m[key] === undefined) m[key] = defaults[key];
    }
    if (version < 4 || mainV4) for (const key of resourceFields) if (m[key] === undefined) m[key] = newMode(i)[key];
    if (version === 1) Object.assign(m, { canDivide: true, bindChildren: false, keepAdhesionA: false, keepAdhesionB: false });
    if (mainV4) {
      if (!validMainV4(m.kind, m.channel, m.subtype, m.signal)) throw new Error("Некорректные сигнальные параметры генома");
      Object.assign(m, migrateMainV4(m));
    }
    if ((m.name !== undefined && (typeof m.name !== "string" || !m.name.trim()))
      || numeric.some(key => typeof m[key] !== "number" || !Number.isFinite(m[key]))
      || ["childAngleA", "childAngleB", "red", "green", "blue"].some(key => typeof m[key] !== "number" || !Number.isFinite(m[key]))
      || !Number.isInteger(m.kind) || m.kind < 0 || m.kind >= TYPES.length
      || [m.childA, m.childB].some(v => !Number.isInteger(v) || v < 0 || v >= modes.length)
      || m.threshold < 1 || m.threshold > 8 || m.ratio < 0.2 || m.ratio > 0.8
      || m.speed < 0 || m.speed > 60 || Math.abs(m.angle) > 3.14 || m.lifespan < 20 || m.lifespan > 1000
      || Math.abs(m.childAngleA) > 3.14 || Math.abs(m.childAngleB) > 3.14
      || !Number.isInteger(m.maxConnections) || m.maxConnections < -1 || m.maxConnections > POPULATION_LIMIT
      || !Number.isFinite(m.nutrientPriority) || m.nutrientPriority < 0.25 || m.nutrientPriority > 4
      || typeof m.prioritizeWhenStarving !== "boolean"
      || ["canDivide", "bindChildren", "keepAdhesionA", "keepAdhesionB", "mirrorA", "mirrorB"].some(key => typeof m[key] !== "boolean")
      || advancedKeys.filter(key => key !== "programs").some(key => typeof m[key] !== "number" || !Number.isFinite(m[key]))
      || !Number.isInteger(m.signalChannel) || m.signalChannel < 0 || m.signalChannel > 7
      || !Number.isInteger(m.sensor) || m.sensor < 0 || m.sensor >= SENSORS.length || Math.abs(m.sensorGain) > 16
      || Math.abs(m.density) > 3.3 || m.senseTolerance < 0 || m.senseTolerance > 1
      || !([m.targetRed,m.targetGreen,m.targetBlue].every(v => v === -1) || [m.targetRed,m.targetGreen,m.targetBlue].every(v => v >= 0 && v <= 1))
      || !Number.isInteger(m.secretion) || m.secretion < 0 || m.secretion > 22 || m.secretionRate < 0 || m.secretionRate > 4
      || [m.transitionA,m.transitionB,m.offspringMode].some(v => !Number.isInteger(v) || v < 0 || v >= modes.length)
      || !Number.isInteger(m.compatibleMode) || m.compatibleMode < -GAMETE_GROUP_COUNT || m.compatibleMode >= modes.length
      || !Number.isInteger(m.virusCopyFrom) || m.virusCopyFrom < -1 || m.virusCopyFrom >= modes.length
      || [m.transitionThresholdA,m.transitionThresholdB].some(v => Math.abs(v)>1000)
      || !Number.isInteger(m.telomeres) || m.telomeres < -1 || m.telomeres > 1000
      || [m.ciliaLeft,m.ciliaRight].some(v => Math.abs(v)>60)
      || [m.contraction,m.bending,m.lift].some(v => Math.abs(v)>1)
      || !Array.isArray(m.programs) || m.programs.length>8
      || m.programs.some((p: unknown) => !validProgram(p))
      || !([m.red, m.green, m.blue].every(v => v === -1) || [m.red, m.green, m.blue].every(v => v >= 0 && v <= 1)))
      throw new Error("Некорректные настройки режима");
    // Copy only supported fields; imported objects do not extend the model implicitly.
    return { name: m.name?.trim() ?? `M${i + 1}`, kind: m.kind, threshold: m.threshold,
      childA: m.childA, childB: m.childB, ratio: m.ratio, speed: m.speed, angle: m.angle,
      lifespan: m.lifespan, canDivide: m.canDivide, bindChildren: m.bindChildren,
      keepAdhesionA: m.keepAdhesionA, keepAdhesionB: m.keepAdhesionB,
      childAngleA: m.childAngleA, childAngleB: m.childAngleB, mirrorA: m.mirrorA, mirrorB: m.mirrorB,
      red: m.red, green: m.green, blue: m.blue, maxConnections: m.maxConnections,
      nutrientPriority: m.nutrientPriority, prioritizeWhenStarving: m.prioritizeWhenStarving,
      density: m.density, signalChannel: m.signalChannel, sensor: m.sensor, sensorGain: m.sensorGain,
      targetRed: m.targetRed, targetGreen: m.targetGreen, targetBlue: m.targetBlue,
      senseTolerance: m.senseTolerance, secretion: m.secretion, secretionRate: m.secretionRate,
      transitionA: m.transitionA, transitionB: m.transitionB,
      transitionThresholdA: m.transitionThresholdA, transitionThresholdB: m.transitionThresholdB,
      offspringMode: m.offspringMode, compatibleMode: m.compatibleMode, telomeres: m.telomeres,
      ciliaLeft: m.ciliaLeft, ciliaRight: m.ciliaRight,
      contraction: m.contraction, bending: m.bending, lift: m.lift, virusCopyFrom: m.virusCopyFrom,
      programs: m.programs.map((p: Program) => ({
        target: p.target, input: p.input, kind: p.kind, scale: p.scale,
        offset: p.offset, threshold: p.threshold, below: p.below,
      })) };
  });
}

export function parseGenome(text: string): Mode[] {
  const raw = JSON.parse(text);
  if (Array.isArray(raw)) return validateModes(raw, 1);
  if (raw?.version !== 2 && raw?.version !== 3 && raw?.version !== 4 && raw?.version !== 5) throw new Error("Неподдерживаемый формат генома");
  return validateModes(raw.modes, raw.version);
}

export function modeColor(mode: Mode): number {
  if (mode.red === -1) return TYPES[mode.kind].color;
  return (Math.round(mode.red * 255) << 16) | (Math.round(mode.green * 255) << 8) | Math.round(mode.blue * 255);
}
// Distribute stress-test cells over the entire world, including at the maximum count.
export function initialPopulation(count: number): { x: number; y: number; mode: number }[] {
  const size = Number.isFinite(count) ? Math.min(POPULATION_LIMIT, Math.max(0, Math.floor(count))) : 0;
  if (size === 0) return [];
  const columns = Math.ceil(Math.sqrt(size));
  const rows = Math.ceil(size / columns);
  return Array.from({ length: size }, (_, i) => ({
    x: 20 + (i % columns + 0.5) * (WORLD - 40) / columns,
    y: 20 + (Math.floor(i / columns) + 0.5) * (WORLD - 40) / rows,
    mode: 0,
  }));
}

export type WorldCommand = (
  | { type: "spawn"; x: number; y: number; mode: number; genome: Float32Array }
  | { type: "edit"; id: number; mode: number; genome: Float32Array }
  | { type: "configure"; settings: Environment }
  | { type: "remove" | "boost"; id: number }
  | { type: "move"; id: number; x: number; y: number }
  | { type: "sterilize" }
) & { atTick?: number };
export type Command = WorldCommand
  | { type: "inspect"; id: number }
  | { type: "speed"; value: number }
  | { type: "reset"; requestId?: string }
  | { type: "export"; requestId: string }
  | { type: "restore"; state: string; requestId?: string }
  | { type: "preview"; genome: Float32Array; mode: number; steps: number; requestId?: number }
  | { type: "step" };
export type Environment = {
  light: number;
  food: number;
  radiation: number;
  viscosity: number;
  aging: boolean;
  killAtEdge: boolean;
  onlyPointMutations: boolean;
  gravity: number;
  density: number;
  nitrogen: number;
};
export const DEFAULT_ENVIRONMENT: Environment = {
  light: 1, food: 1, radiation: 1, viscosity: 1, aging: true, killAtEdge: false,
  onlyPointMutations: false, gravity: 0, density: 0, nitrogen: 1,
};
export type Frame = {
  type: "frame";
  cells: Float32Array;
  links: Uint32Array;
  terrain: Float32Array;
  stats: Float32Array;
  nitrogen?: Float32Array;
  nitrates?: Float32Array;
  details?: Float32Array;
  coveredFood?: Float32Array;
  /** Rows: tick, parent ID, daughter A/B IDs, parent x/y/radius/angle. */
  divisions?: Float32Array;
  previewRequestId?: number;
};
