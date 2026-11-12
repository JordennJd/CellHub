import { flatten, newMode, multicellularGenome, HALF_TURN, type Mode, type Program } from "./model";

/**
 * Curated multicellular organisms shared by the genome editor's demonstration
 * mode and the chaos world's spontaneous immigration. `span` is the number of
 * cells seeded in a chain when the organism appears in chaos.
 */
export type DemoOrganism = {
  id: string;
  name: string;
  description: string;
  initialMode: number;
  span: number;
  genome: () => Mode[];
};

const mode = (index: number, values: Partial<Mode>): Mode => ({
  ...newMode(index), angle: 0, lifespan: 1000, ...values,
});

const rule = (target: number, input: number, scale: number, offset = 0): Program => ({
  target, input, kind: 0, scale, offset, threshold: 0, below: 0,
});

/**
 * A quarter turn between generations makes compact, self-avoiding colonies
 * instead of a straight line that can leave a light or food patch.
 */
const QUARTER_TURN = HALF_TURN / 2;

/** A photocyte that keeps both daughters attached, growing a feeding filament. */
const filament = (): Mode[] => [
  mode(0, {
    name: "Фотоцит", kind: 0, threshold: 1.8, childA: 0, childB: 0,
    bindChildren: true, keepAdhesionA: true, keepAdhesionB: true, maxConnections: 2,
  }),
];

/**
 * A mobile phagocyte assembled as a branching tree. M1 is the seed: it becomes
 * a phagocyte head (M2) and a flagellated body (M3). M2 keeps itself and buds a
 * fresh M1, so the colony grows module by module. Each module adds one muscle
 * pair, a food sensor and a pulling flagellum.
 */
const smartPhagocyte = (): Mode[] => [
  mode(0, {
    name: "Фагоцит-семя", kind: 1, threshold: 1.8, childA: 1, childB: 2,
    bindChildren: true, keepAdhesionA: true, angle: QUARTER_TURN, maxConnections: 4,
  }),
  mode(1, {
    name: "Фагоцит-голова", kind: 1, threshold: 1.8, childA: 0, childB: 1,
    bindChildren: true, keepAdhesionB: true, childAngleB: HALF_TURN,
    angle: 0, maxConnections: 3,
  }),
  mode(2, {
    name: "Жгутикоцит-тело", kind: 2, threshold: 1.6, childA: 3, childB: 6,
    bindChildren: true, keepAdhesionA: true, prioritizeWhenStarving: true,
    speed: 12, angle: QUARTER_TURN, maxConnections: 3,
  }),
  mode(3, {
    name: "Миоцит", kind: 12, threshold: 1.4, childA: 4, childB: 5,
    bindChildren: true, keepAdhesionA: true, prioritizeWhenStarving: true,
    angle: QUARTER_TURN, maxConnections: 3,
    programs: [rule(10, 1, 0.3, 0.25), rule(11, 13, 0.3)],
  }),
  mode(4, {
    name: "Миоцит-2", kind: 12, canDivide: false, prioritizeWhenStarving: true,
    contraction: 0.3, bending: 0.2,
  }),
  mode(5, {
    name: "Сенсоцит", kind: 11, canDivide: false, prioritizeWhenStarving: true,
    sensor: 1, signalChannel: 0, sensorGain: 1,
  }),
  mode(6, {
    name: "Жгутикоцит", kind: 2, canDivide: false, speed: 25, prioritizeWhenStarving: true,
    programs: [rule(8, 1, 28, 6)],
  }),
];

/**
 * The same corrected branching tree as the smart phagocyte. The head (M2) stays
 * a devorocyte so it can feed and reproduce seeds; the body (M3) is a
 * keratinocyte that shields the head, the myocytes and the flagellum from the
 * predator's own digestive range.
 */
const smartPredator = (): Mode[] => [
  mode(0, {
    name: "Девороцит-семя", kind: 5, threshold: 1.8, childA: 1, childB: 2,
    bindChildren: true, keepAdhesionA: true, angle: QUARTER_TURN,
    nutrientPriority: 1, maxConnections: 4,
  }),
  mode(1, {
    name: "Девороцит-голова", kind: 5, threshold: 1.8, childA: 0, childB: 1,
    bindChildren: true, keepAdhesionB: true, childAngleB: HALF_TURN,
    angle: 0, nutrientPriority: 1, maxConnections: 3,
  }),
  mode(2, {
    name: "Кератиноцит-тело", kind: 4, threshold: 1.2, childA: 3, childB: 6,
    bindChildren: true, keepAdhesionA: true, prioritizeWhenStarving: true,
    angle: QUARTER_TURN, nutrientPriority: 1, maxConnections: 3,
  }),
  mode(3, {
    name: "Миоцит", kind: 12, threshold: 1.1, childA: 4, childB: 5,
    bindChildren: true, keepAdhesionA: true, prioritizeWhenStarving: true,
    angle: QUARTER_TURN, nutrientPriority: 1.2, maxConnections: 3,
    programs: [rule(10, 1, 0.3, 0.25), rule(11, 13, 0.3)],
  }),
  mode(4, {
    name: "Миоцит-2", kind: 12, canDivide: false, prioritizeWhenStarving: true,
    contraction: 0.3, bending: 0.2,
  }),
  mode(5, {
    name: "Сенсоцит", kind: 11, canDivide: false, prioritizeWhenStarving: true,
    sensor: 3, signalChannel: 0, sensorGain: 1,
  }),
  mode(6, {
    name: "Жгутикоцит", kind: 2, canDivide: false, speed: 25, prioritizeWhenStarving: true,
    programs: [rule(8, 1, 28, 6)],
  }),
];

/**
 * Photocyte + glueocyte: a stationary mat that spreads over a rich patch.
 * The photocyte keeps itself at the old joint and lays glue towards the light.
 */
const moss = (): Mode[] => [
  mode(0, {
    name: "Фотоцит", kind: 0, threshold: 1.8, childA: 1, childB: 0,
    bindChildren: true, keepAdhesionB: true, childAngleB: HALF_TURN, angle: QUARTER_TURN,
    maxConnections: 4,
  }),
  mode(1, { name: "Глюоцит", kind: 9, canDivide: false, maxConnections: 4, prioritizeWhenStarving: true }),
];

/** Photocyte + glueocyte + keratinocyte: a small armoured nursery. */
const nanobot = (): Mode[] => [
  mode(0, { name: "Фотоцит", kind: 0, threshold: 2, childA: 0, childB: 1, bindChildren: true, maxConnections: 4 }),
  mode(1, {
    name: "Глюоцит", kind: 9, threshold: 1, childA: 1, childB: 2,
    bindChildren: true, keepAdhesionA: true, keepAdhesionB: true, prioritizeWhenStarving: true,
  }),
  mode(2, { name: "Кератиноцит", kind: 4, canDivide: false, prioritizeWhenStarving: true }),
];

/** Built-ins in curated order. */
export const DEMO_ORGANISMS: DemoOrganism[] = [
  {
    id: "filament",
    name: "Фотоцитовая нить",
    description: "Один фотоцит с удержанием обеих дочерей вырастает в связанную нить. Быстро занимает освещённые участки.",
    initialMode: 0,
    span: 2,
    genome: filament,
  },
  {
    id: "tutorial",
    name: "Двигающийся фагоцит",
    description: "Классическая связка из обучения: фотоцит, жгутикоцит и фагоцит с наследованием связи.",
    initialMode: 0,
    span: 3,
    genome: multicellularGenome,
  },
  {
    id: "smart-phagocyte",
    name: "Умный фагоцит",
    description: "Фагоцит-семя выращивает голову, жгутиковое тело, пару миоцитов и сенсоцит. Голова размножается новыми семенами, сенсоцит чувствует пищу, миоциты направляют тело, жгутики тянут его к еде.",
    initialMode: 0,
    span: 1,
    genome: smartPhagocyte,
  },
  {
    id: "moss",
    name: "Мох",
    description: "Фотоцит и глюоцит образуют стелющуюся колонию.",
    initialMode: 0,
    span: 2,
    genome: moss,
  },
  {
    id: "nanobot",
    name: "Нанобот",
    description: "Фотоцит, глюоцит и кератиноцит создают защищённую колонию.",
    initialMode: 0,
    span: 3,
    genome: nanobot,
  },
  {
    id: "smart-predator",
    name: "Умный хищник",
    description: "Девороцит-семя выращивает девороцитовую голову, кератиновое тело, пару миоцитов и сенсоцит. Голова питается добычей и размножается новыми семенами, кератиновое тело прикрывает своих от пищеварения, сенсоцит ищет клетки, миоциты направляют тело, жгутики тянут его к добыче.",
    initialMode: 0,
    span: 1,
    genome: smartPredator,
  },
];

export function demoOrganisms(allowedKinds?: number[]): DemoOrganism[] {
  const allowed = (kind: number) => !allowedKinds || allowedKinds.includes(kind);
  return DEMO_ORGANISMS.filter(organism => organism.genome().every(mode => allowed(mode.kind)));
}

/**
 * Catalog buffer for chaos: `[version, count, (span, weight, genome...)*]`.
 * Every curated organism has the same appearance weight.
 */
export function chaosOrganismsBuffer(): Float32Array {
  const entries = DEMO_ORGANISMS.flatMap(organism => {
    const genome = flatten(organism.genome());
    return [organism.span, 1, ...genome];
  });
  return new Float32Array([1, DEMO_ORGANISMS.length, ...entries]);
}
