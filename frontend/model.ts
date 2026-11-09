export const WORLD = 3072;
export const POPULATION_LIMIT = 20_000;
export const STRIDE = 10;
export const MAX_MODES = 40;
export const MODE_STRIDE = 12;
export const TYPES = [
  {
    name: "Фотоцит",
    latin: "Photocyte",
    color: 0xa4db71,
    info: "Преобразует свет в массу. Лучше растёт на солнечном мелководье.",
  },
  {
    name: "Фагоцит",
    latin: "Phagocyte",
    color: 0xf1bf72,
    info: "Эффективно поглощает питательные вещества субстрата.",
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
];
export type Mode = {
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
};
export type NumericModeKey = {
  [K in keyof Mode]: Mode[K] extends number ? K : never;
}[keyof Mode];
export const newMode = (index: number): Mode => ({
  kind: 0, threshold: 2.4, childA: index, childB: index,
  ratio: 0.5, speed: 12, angle: 0.8, lifespan: 240,
  canDivide: true, bindChildren: false, keepAdhesionA: false, keepAdhesionB: false,
});
export const defaultGenome = (): Mode[] =>
  Array.from({ length: 8 }, (_, i) => ({ ...newMode(i), kind: i % TYPES.length }));

export const multicellularGenome = (): Mode[] => [
  { ...newMode(0), kind: 0, threshold: 2, childA: 1, childB: 2, bindChildren: true, angle: 0, lifespan: 1000 },
  { ...newMode(1), kind: 2, canDivide: false, speed: 40, lifespan: 1000 },
  { ...newMode(2), kind: 1, threshold: 2.4, childA: 2, childB: 0, keepAdhesionA: true, angle: 0, lifespan: 1000 },
];

export const flatten = (g: Mode[]) => new Float32Array(g.flatMap((m) => [
  m.kind, m.threshold, m.childA, m.childB, m.ratio, m.speed, m.angle, m.lifespan,
  Number(m.canDivide), Number(m.bindChildren), Number(m.keepAdhesionA), Number(m.keepAdhesionB),
]));

export function inflate(g: Float32Array): Mode[] {
  const legacy = g.length === 64;
  const stride = legacy ? 8 : MODE_STRIDE;
  if (!g.length || g.length % stride || g.length / stride > MAX_MODES)
    throw new Error("Неверный размер генома");
  return Array.from({ length: g.length / stride }, (_, i) => {
    const a = Array.from(g.slice(i * stride, (i + 1) * stride),
      (value) => Math.round(value * 1e6) / 1e6);
    return {
      kind: a[0], threshold: a[1], childA: a[2], childB: a[3],
      ratio: a[4], speed: a[5], angle: a[6], lifespan: a[7],
      canDivide: legacy || a[8] === 1, bindChildren: !legacy && a[9] === 1,
      keepAdhesionA: !legacy && a[10] === 1, keepAdhesionB: !legacy && a[11] === 1,
    };
  });
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
  return JSON.stringify({ version: 2, modes: genome });
}

export function parseGenome(text: string): Mode[] {
  const raw = JSON.parse(text);
  const legacy = Array.isArray(raw);
  const modes = legacy ? raw : raw?.version === 2 ? raw.modes : null;
  if (!Array.isArray(modes) || modes.length < 1 || modes.length > MAX_MODES || (legacy && modes.length !== 8))
    throw new Error("Неверный формат генома");
  const numeric: NumericModeKey[] = ["kind", "threshold", "childA", "childB", "ratio", "speed", "angle", "lifespan"];
  return modes.map((m) => {
    if (!m || numeric.some((k) => typeof m[k] !== "number" || !Number.isFinite(m[k]))
      || !Number.isInteger(m.kind) || m.kind < 0 || m.kind >= TYPES.length
      || [m.childA, m.childB].some((v) => !Number.isInteger(v) || v < 0 || v >= modes.length)
      || m.threshold < 1 || m.threshold > 8 || m.ratio < 0.2 || m.ratio > 0.8
      || m.speed < 0 || m.speed > 60 || Math.abs(m.angle) > 3.14 || m.lifespan < 20 || m.lifespan > 1000
      || (!legacy && ["canDivide", "bindChildren", "keepAdhesionA", "keepAdhesionB"].some((k) => typeof m[k] !== "boolean")))
      throw new Error("Некорректные настройки архетипа");
    return { ...m, canDivide: legacy ? true : m.canDivide, bindChildren: legacy ? false : m.bindChildren,
      keepAdhesionA: legacy ? false : m.keepAdhesionA, keepAdhesionB: legacy ? false : m.keepAdhesionB };
  });
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
    mode: i % TYPES.length,
  }));
}

export type Command =
  | { type: "spawn"; x: number; y: number; mode: number; genome: Float32Array }
  | { type: "edit"; id: number; mode: number; genome: Float32Array }
  | { type: "inspect"; id: number }
  | { type: "speed"; value: number }
  | { type: "reset" }
  | { type: "step" };
export type Frame = {
  type: "frame";
  cells: Float32Array;
  links: Uint32Array;
  terrain: Float32Array;
  stats: Float32Array;
};
