import { multicellularGenome, newMode, type Mode } from "./model";

/**
 * Curated multicellular organisms for the genome editor's demonstration mode.
 * `universality` is a manual ranking: high values survive in more environments
 * and need fewer specialised neighbours, so they are shown first.
 */
export type OrganismOrigin = "substrate" | "player";

export type DemoOrganism = {
  id: string;
  name: string;
  description: string;
  origin: OrganismOrigin;
  universality: number;
  initialMode: number;
  genome: () => Mode[];
};

const mode = (index: number, values: Partial<Mode>): Mode => ({ ...newMode(index), ...values });

const swimmer = (): Mode[] => [
  mode(0, { name: "Фотоцит", kind: 0, threshold: 1.8, childA: 0, childB: 1, angle: 0, lifespan: 1000, bindChildren: true, maxConnections: 2 }),
  mode(1, { name: "Жгутикоцит", kind: 2, threshold: 2, speed: 42, angle: 0, lifespan: 1000, canDivide: false }),
];

const colony = (): Mode[] => [
  mode(0, { name: "Фотоцит", kind: 0, threshold: 1.6, childA: 0, childB: 1, angle: 0, lifespan: 1000, bindChildren: true, maxConnections: 2 }),
  mode(1, { name: "Глюоцит", kind: 9, threshold: 2, childA: 0, childB: 1, angle: 0, lifespan: 1000, bindChildren: true, maxConnections: 4 }),
];

const symbiont = (): Mode[] => [
  mode(0, { name: "Фотоцит", kind: 0, threshold: 1.9, childA: 0, childB: 1, angle: 0, lifespan: 1000, bindChildren: true, maxConnections: 2 }),
  mode(1, { name: "Фагоцит", kind: 1, threshold: 2.4, childA: 0, childB: 2, angle: 0, lifespan: 1000, bindChildren: true, keepAdhesionB: true, maxConnections: 2 }),
  mode(2, { name: "Нитроцит", kind: 6, threshold: 2.5, angle: 0, lifespan: 1000, canDivide: false }),
];

const predator = (): Mode[] => [
  mode(0, { name: "Фагоцит", kind: 1, threshold: 2, childA: 0, childB: 1, angle: 0, lifespan: 1000, bindChildren: true, maxConnections: 2 }),
  mode(1, { name: "Жгутикоцит", kind: 2, threshold: 2, speed: 38, angle: 0, lifespan: 1000, canDivide: false }),
];

const nanobot = (): Mode[] => [
  mode(0, { name: "Фотоцит", kind: 0, threshold: 1.7, childA: 0, childB: 1, angle: 0, lifespan: 1000, bindChildren: true, maxConnections: 2 }),
  mode(1, { name: "Глюоцит", kind: 9, threshold: 2.2, childA: 0, childB: 2, angle: 0, lifespan: 1000, bindChildren: true, keepAdhesionB: true, maxConnections: 3 }),
  mode(2, { name: "Кератиноцит", kind: 4, threshold: 2.4, angle: 0, lifespan: 1000, canDivide: false }),
];

const buoy = (): Mode[] => [
  mode(0, { name: "Фотоцит", kind: 0, threshold: 1.7, childA: 0, childB: 1, angle: 0, lifespan: 1000, bindChildren: true, maxConnections: 2 }),
  mode(1, { name: "Буйоцит", kind: 8, threshold: 2.2, density: -2.2, angle: 0, lifespan: 1000, canDivide: false }),
];

const parasite = (): Mode[] => [
  mode(0, { name: "Фагоцит", kind: 1, threshold: 2, childA: 0, childB: 1, angle: 0, lifespan: 1000, bindChildren: true, maxConnections: 2 }),
  mode(1, { name: "Вироцит", kind: 3, threshold: 2.5, angle: 0, lifespan: 1000, canDivide: false }),
];

/** Built-ins, deliberately sorted from the most universal to the most niche. */
export const DEMO_ORGANISMS: DemoOrganism[] = [
  {
    id: "swimmer",
    name: "Пловец",
    description: "Фотоцит делится на такой же фотоцит и жгутикоцит. Простая подвижная цепочка, которой хватает света.",
    origin: "player",
    universality: 100,
    initialMode: 0,
    genome: swimmer,
  },
  {
    id: "tutorial",
    name: "Учебная тройка",
    description: "Классическая связка из обучения: фотоцит, жгутикоцит и фагоцит с наследованием связи.",
    origin: "player",
    universality: 95,
    initialMode: 0,
    genome: multicellularGenome,
  },
  {
    id: "symbiont",
    name: "Симбионт",
    description: "Фотоцит кормит фагоцита, а тот добавляет нитроцита для обмена азотом. Вдохновлён уровнем «Симбиоз».",
    origin: "substrate",
    universality: 90,
    initialMode: 0,
    genome: symbiont,
  },
  {
    id: "colony",
    name: "Колония мха",
    description: "Фотоциты и глюоциты образуют связанную колонию с несколькими точками крепления. Вдохновлена «Оплодотворением мха».",
    origin: "substrate",
    universality: 80,
    initialMode: 0,
    genome: colony,
  },
  {
    id: "predator",
    name: "Хищник",
    description: "Фагоцит с жгутиком ищет готовую пищу. Нуждается в пищевых пятнах, зато быстро занимает новые участки.",
    origin: "substrate",
    universality: 70,
    initialMode: 0,
    genome: predator,
  },
  {
    id: "nanobot",
    name: "Нанобот",
    description: "Фотоцит, глюоцит и кератиноцит создают защищённую колонию. Вдохновлён уровнем «Нанобот».",
    origin: "substrate",
    universality: 60,
    initialMode: 0,
    genome: nanobot,
  },
  {
    id: "buoy",
    name: "Поплавок",
    description: "Фотоцит удерживает буйоцит, меняющий плотность. Раскрывается при включённой гравитации.",
    origin: "substrate",
    universality: 50,
    initialMode: 0,
    genome: buoy,
  },
  {
    id: "parasite",
    name: "Паразит",
    description: "Фагоцит прикрепляет вироцита, который заражает незащищённых соседей. Ниша для богатых пищей миров.",
    origin: "substrate",
    universality: 40,
    initialMode: 0,
    genome: parasite,
  },
];

export function demoOrganisms(allowedKinds?: number[]): DemoOrganism[] {
  const allowed = (kind: number) => !allowedKinds || allowedKinds.includes(kind);
  return DEMO_ORGANISMS
    .filter(organism => organism.genome().every(mode => allowed(mode.kind)))
    .sort((a, b) => b.universality - a.universality || a.name.localeCompare(b.name, "ru"));
}
