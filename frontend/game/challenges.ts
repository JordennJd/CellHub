import { newMode, STRIDE, type Mode, type Frame } from "../model";
import type { PlateData } from "./storage";
export type ChallengeObjective = {
    id: string;
    label: string;
    metric: "population" | "typePopulation" | "qualifiedPopulation" | "generation" | "bonds" | "typeBonds" | "infections" | "elapsed" | "age" | "mass";
    target: number;
    kind?: number;
    partnerKind?: number;
    minAge?: number;
    minMass?: number;
};
export type ObjectiveProgress = ChallengeObjective & { current: number; complete: boolean };

/** All conditions must hold in the same frame; inserted cells never count as births. */
export function evaluateChallenge(challenge: Challenge, frame: Frame | null, inserted: number) {
    const criteria = challenge.objectives ?? [{ id: "population", label: "Клеток", metric: "population" as const, target: challenge.target }];
    const cells = frame?.cells;
    const values = criteria.map(objective => {
        let current = 0;
        if (frame) {
            if (objective.metric === "population") current = cells!.length / STRIDE;
            else if (objective.metric === "bonds") current = frame.links.length / 2;
            else if (objective.metric === "typeBonds") {
                const kinds = new Map<number, number>();
                for (let i = 0; i < cells!.length; i += STRIDE) kinds.set(cells![i], cells![i + 4]);
                for (let i = 0; i < frame.links.length; i += 2) {
                    const a = kinds.get(frame.links[i]), b = kinds.get(frame.links[i + 1]);
                    if ((a === objective.kind && b === objective.partnerKind) || (b === objective.kind && a === objective.partnerKind)) current++;
                }
            }
            else if (objective.metric === "infections") current = frame.stats[4] ?? 0;
            else if (objective.metric === "elapsed") current = (frame.stats[0] ?? 0) / 30;
            else if (cells) for (let i = 0; i < cells.length; i += STRIDE) {
                if (objective.kind !== undefined && cells[i + 4] !== objective.kind) continue;
                if (objective.metric === "typePopulation") current++;
                else if (objective.metric === "qualifiedPopulation") {
                    if (cells[i + 6] >= (objective.minAge ?? 0) && cells[i + 5] >= (objective.minMass ?? 0)) current++;
                }
                else if (objective.metric === "generation") current = Math.max(current, cells[i + 7]);
                else if (objective.metric === "mass") current = Math.max(current, cells[i + 5]);
                else if (objective.metric === "age") current = Math.max(current, cells[i + 6]);
            }
        }
        return { ...objective, current, complete: !!frame && Number.isFinite(current) && current >= objective.target };
    });
    const eligible = Number.isSafeInteger(inserted) && inserted > 0 && inserted <= challenge.budget;
    return { eligible, complete: eligible && values.every(v => v.complete), objectives: values };
}

export type Challenge = {
    id: string;
    title: string;
    description: string;
    target: number;
    budget: number;
    requires?: string;
    allowedKinds: number[];
    hint: string;
    spoiler: string;
    environment: PlateData["environment"];
    genome: Mode[];
    objectives?: ChallengeObjective[];
};
const environment = { light: 1, food: 1, radiation: 0, viscosity: 1, aging: true, killAtEdge: false, onlyPointMutations: false, gravity: 0, density: 0 };
// These are small web-adapted exercises, not claims of original level parity.
export const CHALLENGES: Challenge[] = [
    {
        id: "algae", title: "Водоросли", target: 16, budget: 3, allowedKinds: [0],
        description: "Вырастите не менее 16 фотоцитов, поместив не больше трёх клеток.",
        hint: "Фотоциты получают энергию от света. Выберите освещённую область и дождитесь нескольких делений.",
        spoiler: "Поместите исходную клетку в верхнюю левую область чашки. Включите ускорение и наблюдайте за популяцией.",
        environment: { ...environment, light: 2 }, genome: [{ ...newMode(0), speed: 0, threshold: 1.8, lifespan: 1000 }],
    },
    {
        id: "feeding", title: "Питательная среда", target: 12, budget: 2, requires: "algae", allowedKinds: [0, 1],
        description: "В среде без света вырастите не менее 12 клеток, используя не больше двух вставок.",
        hint: "При отсутствии света фотоцитам не хватает энергии. Другой тип клеток получает её из пищи.",
        spoiler: "Выберите для M1 фагоцит. Поместите его в питательный пояс и запустите время.",
        environment: { ...environment, light: 0, food: 3 }, genome: [{ ...newMode(0), speed: 0, threshold: 1.8, lifespan: 1000 }],
    },
    {
        id: "colony", title: "Колония", target: 40, budget: 1, requires: "feeding", allowedKinds: [0, 1, 2],
        description: "Создайте популяцию не менее 40 клеток из одной исходной клетки.",
        hint: "Одна клетка должна дать несколько поколений потомков. Проверьте режимы обеих дочерних клеток и источник питания.",
        spoiler: "Начните с фагоцита, задайте обеим дочерям M1 и массу деления около 1,8. Поместите клетку в питательный пояс.",
        environment: { ...environment, light: 2, food: 4 }, genome: [{ ...newMode(0), threshold: 1.8, speed: 8, lifespan: 1000 }],
    },
    {
        id: "adhesion", title: "Связанные потомки", target: 4, budget: 1, requires: "colony", allowedKinds: [0, 1, 2],
        description: "Из одной клетки получите не менее четырёх живых клеток, три связи и второе поколение потомков.",
        hint: "Новая связь между дочерями и наследование прежних связей — разные настройки. Для растущей колонии нужны обе.",
        spoiler: "Выберите фагоцит, массу деления 1,8 и скорость 0. Свяжите дочерей и включите наследование связей у обеих. Поместите клетку в питательный пояс справа от центра.",
        environment: { ...environment, food: 4 }, genome: [{ ...newMode(0), threshold: 1.8, speed: 0, lifespan: 1000 }],
        objectives: [
            { id: "population", label: "Клеток", metric: "population", target: 4 },
            { id: "bonds", label: "Связей", metric: "bonds", target: 3 },
            { id: "generation", label: "Поколение", metric: "generation", target: 2 },
        ],
    },
    {
        id: "swimmer", title: "Разделение ролей", target: 2, budget: 1, requires: "adhesion", allowedKinds: [0, 1, 2],
        description: "Из одной клетки получите связанных фотоцита и жгутикоцита. Дождитесь возраста жгутикоцита 10 секунд, сохранив обе роли.",
        hint: "Один режим делится на две разные дочерние роли. Жгутик двигает организм, а фотоцит восполняет энергию на свету.",
        spoiler: "Создайте M1-фотоцит с массой деления 1,2: дочери M2 и M3, связать. M2 — фотоцит, M3 — жгутикоцит со скоростью 15; отключите их деление. Начальный режим M1. Поместите его в верхнюю левую светлую область.",
        environment: { ...environment, light: 2, food: 3 }, genome: [{ ...newMode(0), threshold: 1.8, speed: 0, lifespan: 1000 }],
        objectives: [
            { id: "photo", label: "Фотоцитов", metric: "typePopulation", kind: 0, target: 1 },
            { id: "flag", label: "Жгутикоцитов", metric: "typePopulation", kind: 2, target: 1 },
            { id: "bonds", label: "Связей фотоцит — жгутикоцит", metric: "typeBonds", kind: 0, partnerKind: 2, target: 1 },
            { id: "time", label: "Возраст жгутикоцита, с", metric: "age", kind: 2, target: 10 },
        ],
    },
    {
        id: "survival", title: "Экономный обмен", target: 1, budget: 1, requires: "swimmer", allowedKinds: [0, 1, 2, 4],
        description: "Без света и пищи сохраните кератиноцит до возраста 40 секунд с массой не менее 0,35 нг.",
        hint: "Кератиноцит медленнее расходует энергию. Движение тоже требует массу, даже когда питания нет.",
        spoiler: "Выберите кератиноцит, подвижность 0, запретите деление и задайте срок жизни больше 40 секунд. Поместите одну клетку в чашку и наблюдайте.",
        environment: { ...environment, light: 0, food: 0 }, genome: [{ ...newMode(0), speed: 0, canDivide: false, lifespan: 1000 }],
        objectives: [
            { id: "keratin", label: "Кератиноцитов", metric: "typePopulation", kind: 4, target: 1 },
            { id: "mass", label: "Масса кератиноцита, нг", metric: "mass", kind: 4, target: 0.35 },
            { id: "time", label: "Возраст кератиноцита, с", metric: "age", kind: 4, target: 40 },
            { id: "survivors", label: "Клеток с возрастом ≥ 40 с и массой ≥ 0,35 нг", metric: "qualifiedPopulation", kind: 4, minAge: 40, minMass: 0.35, target: 1 },
        ],
    },
    {
        id: "predation", title: "Контактное питание", target: 1, budget: 3, requires: "survival", allowedKinds: [0, 1, 2, 4, 5],
        description: "В среде без ресурсов вырастите девороцит до массы 1,4 нг. Его пищей могут стать другие клетки.",
        hint: "Девороцит забирает массу у близких соседей. Кератиноцит защищён от такого поглощения.",
        spoiler: "Отключите деление и движение у фагоцита и девороцита. На паузе поместите фагоцит, рядом девороцит, с другой стороны ещё фагоцит. Между центрами соседей оставьте примерно один диаметр клетки. Увеличьте масштаб для точной расстановки и запустите время.",
        environment: { ...environment, light: 0, food: 0 }, genome: [{ ...newMode(0), kind: 1, speed: 0, canDivide: false, lifespan: 1000 }],
        objectives: [
            { id: "predator", label: "Девороцитов", metric: "typePopulation", kind: 5, target: 1 },
            { id: "mass", label: "Масса девороцита, нг", metric: "mass", kind: 5, target: 1.4 },
        ],
    },
    {
        id: "viral-transfer", title: "Передача генома", target: 2, budget: 2, requires: "predation", allowedKinds: [0, 1, 2, 3, 4, 5],
        description: "Добейтесь одной передачи генома и получите двух живых вироцитов.",
        hint: "Вироцит копирует свой геном в соседнюю клетку другого типа. Два исходных вироцита не считаются передачей.",
        spoiler: "На паузе поместите неподвижный фагоцит без деления. Поменяйте тип в редакторе на вироцит и поместите его рядом, на расстоянии около диаметра клетки. Запустите время. Первоначальный фагоцит должен стать вироцитом.",
        environment: { ...environment, food: 3 }, genome: [{ ...newMode(0), kind: 1, speed: 0, canDivide: false, lifespan: 1000 }],
        objectives: [
            { id: "infections", label: "Передач генома", metric: "infections", target: 1 },
            { id: "viros", label: "Живых вироцитов", metric: "typePopulation", kind: 3, target: 2 },
        ],
    },
];
export type TutorialRequirement = "place" | "run" | "divide" | "inspect" | "save" | "modes" | "initial" | "branches" | "roles" | "adhesion" | "livingBond";
export type TutorialStep = { id: string; title: string; description: string; requirement: TutorialRequirement };
export type TutorialContext = { genome: Mode[]; initialMode: number; inserted: number; running: boolean; frame: Frame | null; selected: boolean; savedGenome: boolean };
export type Tutorial = { id: string; title: string; description: string; steps: TutorialStep[]; genome: Mode[]; environment: PlateData["environment"] };
export const TUTORIAL_STEPS: TutorialStep[] = [
    { id: "place", requirement: "place", title: "Поместите клетку", description: "Выберите инструмент «Поместить» и нажмите на освещённую область чашки." },
    { id: "run", requirement: "run", title: "Запустите время", description: "Снимите паузу, чтобы клетка начала расти." },
    { id: "divide", requirement: "divide", title: "Дождитесь деления", description: "Наблюдайте, как из одной клетки получаются две. При необходимости ускорьте время." },
    { id: "inspect", requirement: "inspect", title: "Изучите клетку", description: "Выберите диагностику и нажмите на клетку. Сравните её массу и возраст." },
    { id: "save", requirement: "save", title: "Сохраните геном", description: "Откройте редактор генома и сохраните конструкцию под собственным именем." },
];
// Three practical web lessons; original tutorial scripts are not claimed verbatim.
export const TUTORIALS: Tutorial[] = [
    { id: "microscope", title: "Микроскоп", description: "Помещение клетки, течение времени, деление, диагностика и сохранение.",
        steps: TUTORIAL_STEPS, genome: [{ ...newMode(0), speed: 0, threshold: 1.8, lifespan: 1000 }], environment: { ...environment, light: 2 } },
    { id: "genome", title: "Редактор генома", description: "Несколько режимов, начальная клетка и разные дочерние роли.",
        genome: [{ ...newMode(0), speed: 0, threshold: 1.8, lifespan: 1000 }], environment: { ...environment, light: 2, food: 3 },
        steps: [
            { id: "modes", requirement: "modes", title: "Добавьте режим", description: "В редакторе нажмите «Добавить режим». Каждый режим задаёт поведение клетки, а весь геном сохраняется у потомков." },
            { id: "initial", requirement: "initial", title: "Выберите начальный режим", description: "В поле «Начальный режим» выберите M2. Это отдельный выбор: просмотр настроек M1 не должен менять режим новой клетки." },
            { id: "branches", requirement: "branches", title: "Назначьте разные режимы дочерям", description: "Откройте настройки M2. Разрешите деление и назначьте дочери A режим M1, дочери B — M2." },
            { id: "place", requirement: "place", title: "Поместите конструкцию", description: "Поместите клетку на свету. Она начинает жизнь в режиме, который указан в поле «Начальный режим»." },
            { id: "divide", requirement: "divide", title: "Проверьте деление", description: "Запустите время и дождитесь потомков. Их поведение определяется назначенными режимами." },
            { id: "save", requirement: "save", title: "Сохраните конструкцию", description: "Сохраните геном под отдельным именем, чтобы вернуться к нему позже." },
        ] },
    { id: "organism", title: "Многоклеточный организм", description: "Разделение ролей и связи между потомками.",
        genome: [{ ...newMode(0), speed: 0, threshold: 1.2, lifespan: 1000 }], environment: { ...environment, light: 2, food: 3 },
        steps: [
            { id: "roles", requirement: "roles", title: "Задайте три клеточные роли", description: "Создайте три режима: M1-фотоцит, M2-фагоцит и M3-жгутикоцит. Для M3 отключите деление и установите скорость 15." },
            { id: "adhesion", requirement: "adhesion", title: "Свяжите дочерние клетки", description: "В M1 назначьте дочерям M2 и M3, включите «Связать дочерние клетки». Начальным режимом оставьте M1." },
            { id: "place", requirement: "place", title: "Поместите организм", description: "Поместите M1 в верхнюю левую освещённую часть чашки и запустите время." },
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
    if (!CHALLENGES.some(c => c.id === id))
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
