export const WORLD = 3072;
export const POPULATION_LIMIT = 20_000;
export const STRIDE = 12;
export const MODE_STRIDE = 22;
export const GENOME_ABI_VERSION = -4;
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
    info: "Активно движется с заданной скоростью, расходуя массу. Сигнал S1 усиливает или ослабляет тягу.",
  },
  {
    name: "Вироцит",
    latin: "Virocyte",
    color: 0xc997ed,
    info: "При контакте передаёт свой геном соседней клетке. Перезарядка — 3 секунды.",
  },
  {
    name: "Кератиноцит",
    latin: "Keratinocyte",
    color: 0xb3c3d0,
    info: "Экономный обмен веществ. Защищает себя и прямо связанных соседей от девороцитов, вирусов, глюкоцитов и цианида.",
  },
  {
    name: "Девороцит",
    latin: "Devorocyte",
    color: 0xf08373,
    info: "Забирает массу соседних клеток, кроме защищённых кератиноцитов. «Скорость всасывания» задаёт силу.",
  },
  {
    name: "Липоцит",
    latin: "Lipocyte",
    color: 0xe8d17a,
    info: "Жировой запас: почти не тратит массу, имеет повышенный приоритет питания и блокирует диффузию сигналов.",
  },
  {
    name: "Буйоцит",
    latin: "Buoyocyte",
    color: 0x8fd8c0,
    info: "Меняет плотность и всплывает или тонет. Работает только при гравитации. «Плотность» — от −1 до +1.",
  },
  {
    name: "Глюкоцит",
    latin: "Glueocyte",
    color: 0xd4a6e8,
    info: "Приклеивается к незащищённым клеткам при контакте. «Максимум связей» ограничивает число прикреплений.",
  },
  {
    name: "Нитрацит",
    latin: "Nitrocyte",
    color: 0x7bb98a,
    info: "Фиксирует азот и медленно набирает массу без света и пищи. Делится питанием через связи.",
  },
  {
    name: "Стереоцит",
    latin: "Stereocyte",
    color: 0xe6a26b,
    info: "Сравнивает стимул слева и справа и выдаёт разностный сигнал S1–S4. Используется для наведения.",
  },
  {
    name: "Сенсоцит",
    latin: "Senseocyte",
    color: 0x9cc7e8,
    info: "Измеряет выбранный стимул — клетки, пищу, липоциты, свет, стену или движение рядом — и выдаёт сигнал S1–S4.",
  },
  {
    name: "Миоцит",
    latin: "Myocyte",
    color: 0xd98c9c,
    info: "Сокращает связи с соседями. Сила сокращения задаётся «Силой мышцы», а сигнал S1 включает её.",
  },
  {
    name: "Нейроцит",
    latin: "Neurocyte",
    color: 0xc9b3e8,
    info: "Постоянный источник сигнала S1–S4. Отрицательное значение работает как ингибитор.",
  },
  {
    name: "Секроцит",
    latin: "Secrocyte",
    color: 0xa8c98a,
    info: "Выделяет цианид, липазу, протеазу, сигналы S1–S4 или запахи. Азот и масса расходуются.",
  },
  {
    name: "Стемоцит",
    latin: "Stemocyte",
    color: 0xe3b52e,
    info: "Стволовая клетка: при S1 ≥ 0,5 превращается в режим 1, при S2 ≥ 0,5 — в режим 2. Путь 1 приоритетнее.",
  },
  {
    name: "Гамета",
    latin: "Gamete",
    color: 0xf2a6c4,
    info: "Не делится. Сливается с гаметой той же группы совместимости, образуя клетку режима «После слияния».",
  },
  {
    name: "Цилиоцит",
    latin: "Ciliocyte",
    color: 0x8ed0a1,
    info: "Гребёт ресничками: медленнее жгутикоцита, но управляется сигналами S1/S2 влево и вправо.",
  },
] as const;
export const TYPE_COUNT = TYPES.length;
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
  channel: number;
  subtype: number;
  signal: number;
};
export type NumericModeKey = {
  [K in keyof Mode]: Mode[K] extends number ? K : never;
}[keyof Mode];
export const newMode = (index: number): Mode => ({
  name: `M${index + 1}`, kind: 0, threshold: 2.4, childA: index, childB: index,
  ratio: 0.5, speed: 12, angle: 0.8, lifespan: 240,
  canDivide: true, bindChildren: false, keepAdhesionA: false, keepAdhesionB: false,
  childAngleA: 0, childAngleB: 0, mirrorA: false, mirrorB: false,
  red: -1, green: -1, blue: -1, channel: 0, subtype: 0, signal: 0,
});

export const SENSE_STIMULI = ["Клетки", "Пища", "Липоциты", "Свет", "Стена", "Движение рядом"] as const;
export const SECRETIONS = [
  "Цианид (HCN)", "Липаза", "Протеаза",
  "+S1", "+S2", "+S3", "+S4",
  "-S1", "-S2", "-S3", "-S4",
  "Запах пищи", "Запах покрытой пищи", "Запах света", "Запах стены",
] as const;
export const GAMETE_GROUPS = Array.from({ length: 8 }, (_, i) => `Группа ${String.fromCharCode(65 + i)}`);
export const CHANNELS = ["S1", "S2", "S3", "S4"] as const;

/** Sensible starting values when the user switches a mode to another type. */
export const TYPE_DEFAULTS: Record<number, Partial<Mode>> = {
  0: { speed: 0 },
  1: { speed: 0 },
  2: { speed: 12 },
  3: { speed: 0 },
  4: { speed: 0 },
  5: { speed: 20 },
  6: { speed: 0, signal: 6 },
  7: { speed: 0, signal: 0 },
  8: { speed: 0, signal: 2 },
  9: { speed: 0, signal: 30 },
  10: { speed: 30, signal: 30, subtype: 0, channel: 0 },
  11: { speed: 30, signal: 30, subtype: 0, channel: 0 },
  12: { speed: 30 },
  13: { speed: 0, signal: 30, channel: 0 },
  14: { speed: 0, signal: 30, subtype: 0 },
  15: { speed: 0 },
  16: { speed: 0, canDivide: false, subtype: 0 },
  17: { speed: 10 },
};
export const typeDefaults = (kind: number): Partial<Mode> => ({ ...TYPE_DEFAULTS[kind] });

/** Type-specific editor metadata; the simulation interprets these fields per type. */
export const TYPE_SIGNAL: Record<number, { label: string; help: string; min: number; max: number; step: number }> = {
  6: { label: "Запас жира", help: "Сколько жира клетка получает при рождении. Жир почти не расходуется и повышает приоритет питания.", min: 0, max: 18, step: 0.5 },
  7: { label: "Плотность", help: "Положительное значение поднимает клетку, отрицательное — топит. Работает только при гравитации.", min: -3.3, max: 3.3, step: 0.1 },
  8: { label: "Максимум связей", help: "Сколько клеток глюкоцит может приклеить к себе.", min: 1, max: 5, step: 1 },
  9: { label: "Фиксация азота", help: "Скорость набора массы без света и пищи. Азот передаётся по связям.", min: 0, max: 60, step: 1 },
  10: { label: "Сила выхода", help: "Максимальная величина сигнала при полной разнице стимулов.", min: 0, max: 60, step: 1 },
  11: { label: "Сила выхода", help: "Максимальная величина сигнала при полном стимуле.", min: 0, max: 60, step: 1 },
  13: { label: "Сигнал", help: "Постоянный выход в выбранный канал. Отрицательное значение работает как ингибитор.", min: -60, max: 60, step: 1 },
  14: { label: "Сила секреции", help: "Интенсивность выделения выбранного вещества.", min: 0, max: 60, step: 1 },
};
export const typeSignal = (kind: number) => TYPE_SIGNAL[kind];
export const TYPE_SPEED: Record<number, { label: string; help: string }> = {
  2: { label: "Скорость жгутика", help: "Жгутик двигает клетку и связанный организм. Более высокая скорость увеличивает расход массы." },
  5: { label: "Скорость всасывания", help: "Сколько массы девороцит забирает у соседей за секунду." },
  10: { label: "Дальность чувства", help: "Радиус, в котором стереоцит сравнивает стимулы слева и справа." },
  11: { label: "Дальность чувства", help: "Радиус, в котором сенсоцит ищет клетки или пищу." },
  12: { label: "Сила мышцы", help: "Насколько сильно миоцит сжимает связи. Сигнал S1 от 0 до 60 задаёт степень сокращения; значение 0 отключает мышцу." },
  17: { label: "Сила ресничек", help: "Скорость движения цилиоцита. Сигналы S1 и S2 подруливают влево и вправо." },
};
export const typeSpeed = (kind: number) => TYPE_SPEED[kind];
export const usesSpeed = (kind: number) => [2, 5, 10, 11, 12, 17].includes(kind);
export const usesChannel = (kind: number) => [10, 11, 13].includes(kind);
export const usesSubtype = (kind: number) => [10, 11, 14, 16].includes(kind);
export const subtypeOptions = (kind: number): readonly string[] =>
  kind === 14 ? SECRETIONS : kind === 16 ? GAMETE_GROUPS : SENSE_STIMULI;
export const subtypeLabel = (kind: number) =>
  kind === 14 ? "Секретируемое вещество" : kind === 16 ? "Группа совместимости" : "Что измерять";
export const defaultGenome = (): Mode[] =>
  [newMode(0)];

export const multicellularGenome = (): Mode[] => [
  { ...newMode(0), kind: 0, threshold: 2, childA: 1, childB: 2, bindChildren: true, angle: 0, lifespan: 1000 },
  { ...newMode(1), kind: 2, canDivide: false, speed: 40, lifespan: 1000 },
  { ...newMode(2), kind: 1, threshold: 2.4, childA: 2, childB: 0, keepAdhesionA: true, angle: 0, lifespan: 1000 },
];

// Header avoids ambiguity between old 12-number rows and extended rows.
export const flatten = (g: Mode[]) => new Float32Array([GENOME_ABI_VERSION, g.length, ...g.flatMap((m) => [
  m.kind, m.threshold, m.childA, m.childB, m.ratio, m.speed, m.angle, m.lifespan,
  Number(m.canDivide), Number(m.bindChildren), Number(m.keepAdhesionA), Number(m.keepAdhesionB),
  m.childAngleA, m.childAngleB, Number(m.mirrorA), Number(m.mirrorB), m.red, m.green, m.blue,
  m.channel, m.subtype, m.signal,
])]);

export function inflate(g: Float32Array): Mode[] {
  const version = g[0];
  const versioned = version === GENOME_ABI_VERSION || version === -3;
  const legacy = !versioned && g.length === 64;
  const stride = version === GENOME_ABI_VERSION ? MODE_STRIDE : version === -3 ? 19 : legacy ? 8 : 12;
  const offset = versioned ? 2 : 0;
  const count = versioned ? g[1] : g.length / stride;
  if (!Number.isInteger(count) || count < 1 || g.length !== offset + count * stride || !g.every(Number.isFinite))
    throw new Error("Неверный размер или данные генома");
  const modes = Array.from({ length: count }, (_, i) => {
    const a = Array.from(g.slice(offset + i * stride, offset + (i + 1) * stride),
      value => Math.round(value * 1e6) / 1e6);
    const flags = legacy ? [] : [...a.slice(8, 12), ...(versioned ? a.slice(14, 16) : [])];
    if (flags.some(v => v !== 0 && v !== 1)) throw new Error("Некорректные переключатели генома");
    if (version === GENOME_ABI_VERSION && (!Number.isInteger(a[19]) || a[19] < 0 || a[19] > 3
      || !Number.isInteger(a[20]) || a[20] < 0 || a[20] > 15 || a[21] < -60 || a[21] > 60))
      throw new Error("Некорректные сигнальные параметры генома");
    return {
      ...newMode(i), kind: a[0], threshold: a[1], childA: a[2], childB: a[3],
      ratio: a[4], speed: a[5], angle: a[6], lifespan: a[7],
      canDivide: legacy || a[8] === 1, bindChildren: !legacy && a[9] === 1,
      keepAdhesionA: !legacy && a[10] === 1, keepAdhesionB: !legacy && a[11] === 1,
      ...(versioned ? { childAngleA: a[12], childAngleB: a[13], mirrorA: a[14] === 1, mirrorB: a[15] === 1,
        red: a[16], green: a[17], blue: a[18] } : {}),
      ...(version === GENOME_ABI_VERSION ? { channel: a[19], subtype: a[20], signal: a[21] } : {}),
    };
  });
  return validateModes(modes, version === GENOME_ABI_VERSION ? 4 : version === -3 ? 3 : 2);
}

// Removing an archetype redirects references to it to the following archetype
// (or the preceding one for the last entry), then renumbers all references.
export function removeMode(genome: Mode[], index: number): Mode[] {
  if (genome.length <= 1 || index < 0 || index >= genome.length) return genome;
  const reference = (v: number) => v > index ? v - 1 : v === index ? Math.min(index, genome.length - 2) : v;
  return genome.filter((_, i) => i !== index).map((m) => ({
    ...m, childA: reference(m.childA), childB: reference(m.childB),
  }));
}

export function serializeGenome(genome: Mode[]): string {
  return JSON.stringify({ version: 4, modes: validateModes(genome, 4) });
}

function validateModes(modes: unknown, version: 1 | 2 | 3 | 4): Mode[] {
  if (!Array.isArray(modes) || modes.length < 1 || (version === 1 && modes.length !== 8))
    throw new Error("Неверный формат генома");
  const numeric: NumericModeKey[] = ["kind", "threshold", "childA", "childB", "ratio", "speed", "angle", "lifespan", "channel", "subtype", "signal"];
  const daughter = ["childAngleA", "childAngleB", "mirrorA", "mirrorB", "red", "green", "blue"] as const;
  const signalling = ["channel", "subtype", "signal"] as const;
  return modes.map((source, i) => {
    if (!source || typeof source !== "object" || Array.isArray(source)) throw new Error("Некорректные настройки режима");
    const m = { ...source };
    if (version < 4) for (const key of signalling) if (m[key] === undefined) m[key] = newMode(i)[key];
    if (version < 3) for (const key of daughter) if (m[key] === undefined) m[key] = newMode(i)[key];
    if (version === 1) Object.assign(m, { canDivide: true, bindChildren: false, keepAdhesionA: false, keepAdhesionB: false });
    if ((m.name !== undefined && (typeof m.name !== "string" || !m.name.trim()))
      || numeric.some(key => typeof m[key] !== "number" || !Number.isFinite(m[key]))
      || ["childAngleA", "childAngleB", "red", "green", "blue"].some(key => typeof m[key] !== "number" || !Number.isFinite(m[key]))
      || !Number.isInteger(m.kind) || m.kind < 0 || m.kind >= TYPES.length
      || !Number.isInteger(m.channel) || m.channel < 0 || m.channel > 3
      || !Number.isInteger(m.subtype) || m.subtype < 0 || m.subtype > 15
      || m.signal < -60 || m.signal > 60
      || [m.childA, m.childB].some(v => !Number.isInteger(v) || v < 0 || v >= modes.length)
      || m.threshold < 1 || m.threshold > 8 || m.ratio < 0.2 || m.ratio > 0.8
      || m.speed < 0 || m.speed > 60 || Math.abs(m.angle) > 3.14 || m.lifespan < 20 || m.lifespan > 1000
      || Math.abs(m.childAngleA) > 3.14 || Math.abs(m.childAngleB) > 3.14
      || ["canDivide", "bindChildren", "keepAdhesionA", "keepAdhesionB", "mirrorA", "mirrorB"].some(key => typeof m[key] !== "boolean")
      || !([m.red, m.green, m.blue].every(v => v === -1) || [m.red, m.green, m.blue].every(v => v >= 0 && v <= 1)))
      throw new Error("Некорректные настройки режима");
    // Copy only supported fields; imported objects do not extend the model implicitly.
    return { name: m.name?.trim() ?? `M${i + 1}`, kind: m.kind, threshold: m.threshold,
      childA: m.childA, childB: m.childB, ratio: m.ratio, speed: m.speed, angle: m.angle,
      lifespan: m.lifespan, canDivide: m.canDivide, bindChildren: m.bindChildren,
      keepAdhesionA: m.keepAdhesionA, keepAdhesionB: m.keepAdhesionB,
      childAngleA: m.childAngleA, childAngleB: m.childAngleB, mirrorA: m.mirrorA, mirrorB: m.mirrorB,
      red: m.red, green: m.green, blue: m.blue,
      channel: m.channel, subtype: m.subtype, signal: m.signal };
  });
}

export function parseGenome(text: string): Mode[] {
  const raw = JSON.parse(text);
  if (Array.isArray(raw)) return validateModes(raw, 1);
  if (raw?.version !== 2 && raw?.version !== 3 && raw?.version !== 4) throw new Error("Неподдерживаемый формат генома");
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

export type Command =
  | { type: "spawn"; x: number; y: number; mode: number; genome: Float32Array }
  | { type: "edit"; id: number; mode: number; genome: Float32Array }
  | { type: "inspect"; id: number }
  | { type: "speed"; value: number }
  | { type: "reset"; requestId?: string }
  | { type: "configure"; settings: Environment }
  | { type: "remove" | "boost"; id: number }
  | { type: "move"; id: number; x: number; y: number }
  | { type: "sterilize" }
  | { type: "export"; requestId: string }
  | { type: "restore"; state: string; requestId?: string }
  | { type: "preview"; genome: Float32Array; mode: number; steps: number }
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
};
export const DEFAULT_ENVIRONMENT: Environment = {
  light: 1, food: 1, radiation: 1, viscosity: 1, aging: true, killAtEdge: false,
  onlyPointMutations: false, gravity: 0, density: 0,
};
export type Frame = {
  type: "frame";
  cells: Float32Array;
  links: Uint32Array;
  terrain: Float32Array;
  stats: Float32Array;
};
