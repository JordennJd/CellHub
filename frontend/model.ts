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
    info: "Активно движется с заданной скоростью, расходуя массу.",
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
    info: "Экономный обмен веществ и защита от поглощения хищником.",
  },
  {
    name: "Девороцит",
    latin: "Devorocyte",
    color: 0xf08373,
    info: "Забирает массу соседних клеток, кроме защищённых кератиноцитов.",
  },
  {
    name: "Нитроцит",
    latin: "Nitrocyte",
    color: 0x8ed6b3,
    info: "Пополняет запас азота и передаёт избыток по постоянным связям.",
  },
];
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
});
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
  m.maxConnections, m.nutrientPriority, Number(m.prioritizeWhenStarving),
])]);

export function inflate(g: Float32Array): Mode[] {
  const current = g[0] === GENOME_ABI_VERSION;
  const extended = current || g[0] === -3;
  const legacy = !extended && g.length === 64;
  const stride = current ? MODE_STRIDE : extended ? 19 : legacy ? 8 : 12;
  const offset = extended ? 2 : 0;
  const count = extended ? g[1] : g.length / stride;
  if (!Number.isInteger(count) || count < 1 || g.length !== offset + count * stride || !g.every(Number.isFinite))
    throw new Error("Неверный размер или данные генома");
  const modes = Array.from({ length: count }, (_, i) => {
    const a = Array.from(g.slice(offset + i * stride, offset + (i + 1) * stride),
      value => Math.round(value * 1e6) / 1e6);
    const flags = legacy ? [] : a.slice(8, 12);
    if (extended) flags.push(...a.slice(14, 16));
    if (current) flags.push(a[21]);
    if (flags.some(v => v !== 0 && v !== 1)) throw new Error("Некорректные переключатели генома");
    return {
      ...newMode(i), kind: a[0], threshold: a[1], childA: a[2], childB: a[3],
      ratio: a[4], speed: a[5], angle: a[6], lifespan: a[7],
      canDivide: legacy || a[8] === 1, bindChildren: !legacy && a[9] === 1,
      keepAdhesionA: !legacy && a[10] === 1, keepAdhesionB: !legacy && a[11] === 1,
      ...(extended ? { childAngleA: a[12], childAngleB: a[13], mirrorA: a[14] === 1, mirrorB: a[15] === 1,
        red: a[16], green: a[17], blue: a[18] } : {}),
      ...(current ? { maxConnections: a[19], nutrientPriority: a[20], prioritizeWhenStarving: a[21] === 1 } : {}),
    };
  });
  return validateModes(modes, 4);
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
  const numeric: NumericModeKey[] = ["kind", "threshold", "childA", "childB", "ratio", "speed", "angle", "lifespan"];
  const extension = ["childAngleA", "childAngleB", "mirrorA", "mirrorB", "red", "green", "blue"] as const;
  const resourceFields = ["maxConnections", "nutrientPriority", "prioritizeWhenStarving"] as const;
  return modes.map((source, i) => {
    if (!source || typeof source !== "object" || Array.isArray(source)) throw new Error("Некорректные настройки режима");
    const m = { ...source };
    if (version < 3) for (const key of extension) if (m[key] === undefined) m[key] = newMode(i)[key];
    if (version < 4) for (const key of resourceFields) if (m[key] === undefined) m[key] = newMode(i)[key];
    if (version === 1) Object.assign(m, { canDivide: true, bindChildren: false, keepAdhesionA: false, keepAdhesionB: false });
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
      || !([m.red, m.green, m.blue].every(v => v === -1) || [m.red, m.green, m.blue].every(v => v >= 0 && v <= 1)))
      throw new Error("Некорректные настройки режима");
    // Copy only supported fields; imported objects do not extend the model implicitly.
    return { name: m.name?.trim() ?? `M${i + 1}`, kind: m.kind, threshold: m.threshold,
      childA: m.childA, childB: m.childB, ratio: m.ratio, speed: m.speed, angle: m.angle,
      lifespan: m.lifespan, canDivide: m.canDivide, bindChildren: m.bindChildren,
      keepAdhesionA: m.keepAdhesionA, keepAdhesionB: m.keepAdhesionB,
      childAngleA: m.childAngleA, childAngleB: m.childAngleB, mirrorA: m.mirrorA, mirrorB: m.mirrorB,
      red: m.red, green: m.green, blue: m.blue, maxConnections: m.maxConnections,
      nutrientPriority: m.nutrientPriority, prioritizeWhenStarving: m.prioritizeWhenStarving };
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
};
