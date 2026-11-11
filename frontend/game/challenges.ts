import { DEFAULT_ENVIRONMENT, newMode, STRIDE, type Mode, type Frame } from "../model";
import type { PlateData } from "./storage";
export type TutorialRequirement = "place" | "run" | "divide" | "inspect" | "save" | "modes" | "initial" | "branches" | "roles" | "adhesion" | "livingBond";
export type TutorialStep = { id: string; title: string; description: string; requirement: TutorialRequirement };
export type TutorialContext = { genome: Mode[]; initialMode: number; inserted: number; running: boolean; frame: Frame | null; selected: boolean; savedGenome: boolean };
export type Tutorial = { id: string; title: string; description: string; steps: TutorialStep[]; genome: Mode[]; environment: PlateData["environment"] };
export const TUTORIAL_STEPS: TutorialStep[] = [
    { id: "place", requirement: "place", title: "Поместите клетку", description: "Выберите слой «Свет» и инструмент «Поместить». Нажмите на жёлтую область в верхней левой части чашки." },
    { id: "run", requirement: "run", title: "Запустите время", description: "Снимите паузу, чтобы клетка начала расти." },
    { id: "divide", requirement: "divide", title: "Дождитесь деления", description: "Наблюдайте, как из одной клетки получаются две. При необходимости ускорьте время." },
    { id: "inspect", requirement: "inspect", title: "Изучите клетку", description: "Выберите диагностику и нажмите на клетку. Сравните её массу и возраст." },
    { id: "save", requirement: "save", title: "Сохраните геном", description: "Откройте редактор генома и сохраните конструкцию под собственным именем." },
];
// Three practical web lessons; original tutorial scripts are not claimed verbatim.
export const TUTORIALS: Tutorial[] = [
    { id: "microscope", title: "Микроскоп", description: "Помещение клетки, течение времени, деление, диагностика и сохранение.",
        steps: TUTORIAL_STEPS, genome: [{ ...newMode(0), speed: 0, threshold: 1.8, lifespan: 1000 }], environment: { ...DEFAULT_ENVIRONMENT, radiation: 0, light: 2 } },
    { id: "genome", title: "Редактор генома", description: "Несколько режимов, начальная клетка и разные дочерние роли.",
        genome: [{ ...newMode(0), speed: 0, threshold: 1.8, lifespan: 1000 }], environment: { ...DEFAULT_ENVIRONMENT, radiation: 0, light: 2, food: 3 },
        steps: [
            { id: "modes", requirement: "modes", title: "Добавьте режим", description: "В редакторе нажмите «Добавить режим». Каждый режим задаёт поведение клетки, а весь геном сохраняется у потомков." },
            { id: "initial", requirement: "initial", title: "Выберите начальный режим", description: "В поле «Начальный режим» выберите M2. Это отдельный выбор: просмотр настроек M1 не должен менять режим новой клетки." },
            { id: "branches", requirement: "branches", title: "Назначьте разные режимы дочерям", description: "Откройте настройки M2. Разрешите деление и назначьте дочери A режим M1, дочери B — M2." },
            { id: "place", requirement: "place", title: "Поместите конструкцию", description: "Поместите клетку на свету. Она начинает жизнь в режиме, который указан в поле «Начальный режим»." },
            { id: "divide", requirement: "divide", title: "Проверьте деление", description: "Запустите время и дождитесь потомков. Их поведение определяется назначенными режимами." },
            { id: "save", requirement: "save", title: "Сохраните конструкцию", description: "Сохраните геном под отдельным именем, чтобы вернуться к нему позже." },
        ] },
    { id: "organism", title: "Многоклеточный организм", description: "Разделение ролей и связи между потомками.",
        genome: [{ ...newMode(0), speed: 0, threshold: 1.2, lifespan: 1000 }], environment: { ...DEFAULT_ENVIRONMENT, radiation: 0, light: 2, food: 3 },
        steps: [
            { id: "roles", requirement: "roles", title: "Задайте три клеточные роли", description: "Создайте три режима: M1-фотоцит, M2-фагоцит и M3-жгутикоцит. Для M3 отключите деление и установите скорость 15." },
            { id: "adhesion", requirement: "adhesion", title: "Свяжите дочерние клетки", description: "В M1 назначьте дочерям M2 и M3, включите «Связать дочерние клетки». Начальным режимом оставьте M1." },
            { id: "place", requirement: "place", title: "Поместите организм", description: "Выберите слой «Свет», поместите M1 в жёлтую область в верхней левой части чашки и запустите время." },
            { id: "livingBond", requirement: "livingBond", title: "Наблюдайте связанные роли", description: "Дождитесь связанной пары: фагоцит питается, жгутикоцит двигает организм. При необходимости исправьте режимы и начните заново." },
            { id: "save", requirement: "save", title: "Сохраните организм", description: "Сохраните геном, чтобы использовать его в другом эксперименте." },
        ] },
];

export function evaluateTutorialStep(step: TutorialStep, state: TutorialContext): boolean {
    switch (step.requirement) {
        case "place": return state.inserted > 0;
        case "run": return state.running;
        case "divide": return !!state.frame && (state.frame.stats[2] ?? 0) > state.inserted;
        case "inspect": return state.selected;
        case "save": return state.savedGenome;
        case "modes": return state.genome.length >= 2;
        case "initial": return state.initialMode > 0 && state.initialMode < state.genome.length;
        case "branches": { const initial = state.genome[state.initialMode]; return !!initial?.canDivide && initial.childA !== initial.childB; }
        case "roles": return [0, 1].every(kind => state.genome.some(m => m.kind === kind))
            && state.genome.some(m => m.kind === 2 && !m.canDivide && m.speed === 15);
        case "adhesion": {
            const initial = state.genome[state.initialMode];
            if (!initial?.canDivide || !initial.bindChildren || initial.kind !== 0) return false;
            const a = state.genome[initial.childA]?.kind, b = state.genome[initial.childB]?.kind;
            return (a === 1 && b === 2) || (a === 2 && b === 1);
        }
        case "livingBond": {
            if (!state.frame) return false;
            const kinds = new Map<number, number>();
            for (let i = 0; i < state.frame.cells.length; i += STRIDE) kinds.set(state.frame.cells[i], state.frame.cells[i + 4]);
            for (let i = 0; i < state.frame.links.length; i += 2) {
                const a = kinds.get(state.frame.links[i]), b = kinds.get(state.frame.links[i + 1]);
                if ((a === 1 && b === 2) || (a === 2 && b === 1)) return true;
            }
            return false;
        }
    }
}

const TUTORIAL_PROGRESS = "cellhub.tutorials.v1";
export function listCompletedTutorials(): string[] {
    const raw = localStorage.getItem(TUTORIAL_PROGRESS);
    if (raw === null) return [];
    let values: unknown;
    try { values = JSON.parse(raw); } catch { throw new Error("Прогресс обучения повреждён. Исходные данные не изменены."); }
    if (!Array.isArray(values) || values.some(v => typeof v !== "string")) throw new Error("Прогресс обучения повреждён. Исходные данные не изменены.");
    return [...new Set(values as string[])];
}
export function completeTutorial(id: string): void {
    if (!TUTORIALS.some(t => t.id === id)) throw new Error("Обучение не найдено.");
    const completed = listCompletedTutorials();
    if (!completed.includes(id)) {
        try { localStorage.setItem(TUTORIAL_PROGRESS, JSON.stringify([...completed, id])); }
        catch { throw new Error("Обучение завершено, но браузер не разрешил сохранить прогресс."); }
    }
}

const PROGRESS = "cellhub.progress.v1";
export function listCompleted(): string[] {
    const raw = localStorage.getItem(PROGRESS);
    if (!raw)
        return [];
    let data: unknown;
    try {
        data = JSON.parse(raw);
    }
    catch {
        throw new Error("Не удалось прочитать прогресс испытаний. Исходные данные не изменены.");
    }
    if (!Array.isArray(data) || data.some(v => typeof v !== "string"))
        throw new Error("Прогресс испытаний повреждён. Исходные данные не изменены.");
    return [...new Set(data as string[])];
}
export function completeChallenge(id: string): void {
    if (typeof id !== "string" || !id || id.length > 80)
        throw new Error("Испытание не найдено.");
    const completed = listCompleted();
    if (!completed.includes(id)) {
        try {
            localStorage.setItem(PROGRESS, JSON.stringify([...completed, id]));
        }
        catch {
            throw new Error("Испытание выполнено, но браузер не разрешил сохранить прогресс.");
        }
    }
}
