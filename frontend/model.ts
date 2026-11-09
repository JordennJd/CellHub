export const WORLD = 3072;
export const POPULATION_LIMIT = 20_000;
export const STRIDE = 10;
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
};
export const defaultGenome = (): Mode[] =>
  Array.from({ length: 8 }, (_, i) => ({
    kind: i % 6,
    threshold: 2.4,
    childA: i,
    childB: i,
    ratio: 0.5,
    speed: 12,
    angle: 0.8,
    lifespan: 240,
  }));
export const flatten = (g: Mode[]) =>
  new Float32Array(
    g.flatMap((m) => [
      m.kind,
      m.threshold,
      m.childA,
      m.childB,
      m.ratio,
      m.speed,
      m.angle,
      m.lifespan,
    ]),
  );
export function inflate(g: Float32Array): Mode[] {
  return Array.from({ length: 8 }, (_, i) => {
    const a = Array.from(
      g.slice(i * 8, i * 8 + 8),
      (value) => Math.round(value * 1e6) / 1e6,
    );
    return {
      kind: a[0],
      threshold: a[1],
      childA: a[2],
      childB: a[3],
      ratio: a[4],
      speed: a[5],
      angle: a[6],
      lifespan: a[7],
    };
  });
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
  terrain: Float32Array;
  stats: Float32Array;
};
