import { newMode, type Environment, type Mode } from "../model";

/** Shape of a `levels/<id>.json` manifest from `scripts/build_levels.py`. */
export type LevelGenomeMode = Partial<Mode> & { name?: string };
export type LevelGenome = { version: number; modes: LevelGenomeMode[] };
export type LevelCell = {
    x: number;
    y: number;
    angle: number;
    mass: number;
    mode: number;
    genome: number;
    generation: number;
    age: number;
    class: number;
};
export type LevelManifest = {
    schema: number;
    id: string;
    order: number;
    title: string;
    description: string;
    difficulty: number;
    hints: string[];
    solution: string;
    tools: string[];
    budget: number;
    removeMax: number | null;
    maxCellsOfType: Record<string, number>;
    unlocks: string[];
    allowedKinds: number[];
    tutorial: boolean;
    environment: Environment;
    terrain: Record<string, number>;
    conditions: Record<string, number>;
    genomes: LevelGenome[];
    initial: { cells: LevelCell[]; links: [number, number][]; food: number[][] };
    demo: { at: number; op: string; x: number; y: number; genome?: number; mode?: number; id?: number; mass?: number }[];
    test: { maxSeconds?: number | null; seed?: number };
};
export type LevelCondition = {
    key: string;
    target: number;
    current: number;
    met: boolean;
    required: boolean;
};
export type LevelStatus = {
    id: string;
    state: "running" | "won" | "lost";
    elapsed: number;
    winTime: number;
    winHold: number;
    demo: boolean;
    demoActions: number;
    demoCursor: number;
    tutorial: boolean;
    counts: {
        total: number;
        user: number;
        level: number;
        infected: number;
        hybrid: number;
        black: number;
        red: number;
        yellow: number;
    };
    conditions: LevelCondition[];
};

const modules = import.meta.glob("../../levels/*.json", { import: "default" }) as Record<
    string,
    () => Promise<unknown>
>;
let cache: Promise<LevelManifest[]> | null = null;

export function levelModules(): string[] {
    return Object.keys(modules).filter(path => !path.endsWith("/index.json")).sort();
}

export function loadLevels(): Promise<LevelManifest[]> {
    cache ??= Promise.all(
        levelModules().map(async path => {
            const value = await modules[path]();
            return levelDocument(value, path);
        }),
    ).then(levels => levels.sort((a, b) => a.order - b.order || a.id.localeCompare(b.id)));
    return cache;
}

/** Playable challenge levels. Manifest tutorials are replaced by the web lessons in `challenges.ts`. */
export function loadChallengeLevels(): Promise<LevelManifest[]> {
    return loadLevels().then(levels => levels.filter(level => !level.tutorial));
}

function levelDocument(value: unknown, source: string): LevelManifest {
    if (!value || typeof value !== "object" || Array.isArray(value))
        throw new Error(`Файл уровня ${source} повреждён.`);
    const level = value as LevelManifest;
    if (level.schema !== 1 || typeof level.id !== "string" || !level.id
        || !Array.isArray(level.genomes) || !level.conditions || typeof level.conditions !== "object"
        || !level.initial || !Array.isArray(level.initial.cells))
        throw new Error(`Файл уровня ${source} не соответствует схеме.`);
    return level;
}

/** Expand the compact JSON mode into the editor's full `Mode` shape. */
export function levelGenome(level: LevelManifest, index: number): Mode[] {
    const genome = level.genomes[index];
    if (!genome || !Array.isArray(genome.modes) || genome.modes.length === 0)
        throw new Error("В уровне нет выбранного генома.");
    return genome.modes.map((source, mode) => ({
        ...newMode(mode),
        ...source,
        name: source.name?.trim() || `M${mode + 1}`,
        programs: source.programs ?? [],
    }));
}

/** A sensible editable genome for a fresh attempt. */
export function starterGenome(level: LevelManifest): { genome: Mode[]; initialMode: number } {
    if (level.initial.cells.length > 0) {
        const first = level.initial.cells[0];
        return { genome: levelGenome(level, first.genome), initialMode: first.mode };
    }
    const kind = level.allowedKinds[0] ?? 0;
    return {
        genome: [{ ...newMode(0), kind, threshold: 1.8, speed: kind === 2 ? 12 : 0, lifespan: 1000 }],
        initialMode: 0,
    };
}

export function challengeBudget(level: LevelManifest): number {
    if (level.budget > 0)
        return level.budget;
    return level.tools.includes("add") ? Number.POSITIVE_INFINITY : 0;
}

export const CONDITION_LABELS: Record<string, { label: string; unit?: string; max?: boolean }> = {
    "user-cells": { label: "Клеток игрока", unit: "не менее" },
    "user-cells-max": { label: "Клеток игрока", unit: "не более", max: true },
    "level-cells": { label: "Клеток уровня", unit: "не более", max: true },
    "level-cells-min": { label: "Клеток уровня", unit: "не менее" },
    "total-cells-min": { label: "Всего клеток", unit: "не менее" },
    "infected-level-cells": { label: "Заражённых клеток уровня", unit: "не менее" },
    "infected-level-cells-max": { label: "Заражённых клеток уровня", unit: "не более", max: true },
    "hybrid-cells-min": { label: "Гибридных клеток", unit: "не менее" },
    "black-level-cells": { label: "Чёрных клеток уровня", unit: "не более", max: true },
    "red-level-cells": { label: "Красных клеток уровня", unit: "не более", max: true },
    "red-level-cells-min": { label: "Красных клеток уровня", unit: "не менее" },
    "yellow-level-cells": { label: "Жёлтых клеток уровня", unit: "не более", max: true },
    "yellow-level-cells-min": { label: "Жёлтых клеток уровня", unit: "не менее" },
    "win-time": { label: "Удержание условия", unit: "секунд" },
    "max-time": { label: "Время до поражения", unit: "секунд", max: true },
};

export function conditionText(condition: LevelCondition): string {
    const label = CONDITION_LABELS[condition.key]?.label ?? condition.key;
    return label;
}
