import { parseGenome, parseStoredGenome, type Mode, type Environment } from "../model";
export type LibraryKind = "genome" | "plate";
export type GenomeData = {
    genome: Mode[];
    initialMode: number;
};
export type PlateData = GenomeData & {
    state: string;
    environment: Environment;
    inserted: number;
    challengeId?: string;
    mode?: "sandbox" | "chaos";
    slotNames?: string[];
    genomeNames?: Record<string, string[]>;
};
export type LibraryEntry = {
    id: string;
    name: string;
    kind: LibraryKind;
    updatedAt: string;
    data: unknown;
};
const KEY = "cellhub.library.v1";
const MAX_FILE_SIZE = 20 * 1024 * 1024;
const LEGACY_ID = "legacy-genome-v2";
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
export function validName(name: string): string {
    const value = name.trim();
    if (!value || value.length > 80)
        throw new Error("Введите название длиной от 1 до 80 символов.");
    return value;
}
export function validateData(kind: LibraryKind, data: unknown): GenomeData | PlateData {
    if (!record(data) || !Array.isArray(data.genome) || data.genome.length > 1000)
        throw new Error("В сохранении нет корректного генома.");
    const genome = parseStoredGenome(data.genome);
    if (typeof data.initialMode !== "number" || !Number.isInteger(data.initialMode) || data.initialMode < 0 || data.initialMode >= genome.length)
        throw new Error("Начальный режим сохранения недоступен.");
    const base = { genome, initialMode: data.initialMode };
    if (kind === "genome")
        return base;
    if (typeof data.state !== "string" || !data.state || data.state.length > MAX_FILE_SIZE || !record(data.environment))
        throw new Error("В сохранении нет данных эксперимента.");
    let state: unknown;
    try {
        state = JSON.parse(data.state);
    }
    catch {
        throw new Error("Данные эксперимента повреждены.");
    }
    if (!record(state) || state.version !== 1 || !Array.isArray(state.values)
        || state.values.length > 10000000
        || state.values.some(v => typeof v !== "number" || !Number.isFinite(v)))
        throw new Error("Данные эксперимента повреждены или их версия не поддерживается.");
    const e: Record<string, unknown> = { onlyPointMutations: false, gravity: 0, density: 0, nitrogen: 1, ...data.environment };
    if ([e.light, e.food, e.radiation, e.nitrogen].some(v => typeof v !== "number" || !Number.isFinite(v) || v < 0 || v > 5)
        || typeof e.viscosity !== "number" || !Number.isFinite(e.viscosity) || e.viscosity < 0.1 || e.viscosity > 5
        || typeof e.aging !== "boolean" || typeof e.killAtEdge !== "boolean"
        || typeof e.onlyPointMutations !== "boolean"
        || typeof e.gravity !== "number" || !Number.isFinite(e.gravity) || e.gravity < 0 || e.gravity > 1
        || typeof e.density !== "number" || !Number.isFinite(e.density) || e.density < -3.3 || e.density > 3.3)
        throw new Error("Параметры среды сохранения некорректны.");
    if (typeof data.inserted !== "number" || !Number.isSafeInteger(data.inserted) || data.inserted < 0)
        throw new Error("Счётчик помещённых клеток некорректен.");
    if (data.mode !== undefined && data.mode !== "sandbox" && data.mode !== "chaos")
        throw new Error("Неизвестный режим эксперимента.");
    if (data.challengeId !== undefined && (typeof data.challengeId !== "string" || data.challengeId.length > 80))
        throw new Error("Не удалось прочитать испытание сохранения.");
    if (data.slotNames !== undefined && (!Array.isArray(data.slotNames) || data.slotNames.some(n => typeof n !== "string" || n.length > 80)))
        throw new Error("Имена режимов сохранения некорректны.");
    if (data.genomeNames !== undefined && (!record(data.genomeNames) || Object.keys(data.genomeNames).length > 20000
        || Object.entries(data.genomeNames).some(([key, names]) => !key || key.length > 2400000
            || !Array.isArray(names) || names.length > 1000 || names.some(n => typeof n !== "string" || !n.trim() || n.length > 80))))
        throw new Error("Имена режимов живых клеток некорректны.");
    return { ...base, ...(data.mode === undefined ? {} : { mode: data.mode as "sandbox" | "chaos" }), state: data.state, environment: e as PlateData["environment"], inserted: data.inserted,
        ...(data.challengeId === undefined ? {} : { challengeId: data.challengeId as string }),
        ...(data.slotNames === undefined ? {} : { slotNames: data.slotNames as string[] }),
        ...(data.genomeNames === undefined ? {} : { genomeNames: data.genomeNames as Record<string, string[]> }) };
}
function validateEntry(value: unknown): LibraryEntry {
    if (!record(value) || typeof value.id !== "string" || !value.id || value.id.length > 100
        || typeof value.name !== "string" || (value.kind !== "genome" && value.kind !== "plate")
        || typeof value.updatedAt !== "string" || !Number.isFinite(Date.parse(value.updatedAt)))
        throw new Error("Не удалось прочитать запись библиотеки.");
    return { id: value.id, name: validName(value.name), kind: value.kind, updatedAt: value.updatedAt, data: validateData(value.kind, value.data) };
}
function readLibrary(): LibraryEntry[] {
    let raw: string | null;
    try {
        raw = localStorage.getItem(KEY);
    }
    catch {
        throw new Error("Браузер не разрешает открыть сохранения.");
    }
    if (raw === null)
        return [];
    try {
        const parsed: unknown = JSON.parse(raw);
        if (!record(parsed) || parsed.version !== 1 || !Array.isArray(parsed.entries))
            throw new Error();
        const entries = parsed.entries.map(validateEntry);
        if (new Set(entries.map(e => e.id)).size !== entries.length)
            throw new Error();
        return entries;
    }
    catch {
        throw new Error("Библиотека повреждена. Исходные данные сохранены в браузере и не перезаписаны.");
    }
}
function writeLibrary(entries: LibraryEntry[]) {
    try {
        localStorage.setItem(KEY, JSON.stringify({ version: 1, entries }));
    }
    catch {
        throw new Error("Не удалось сохранить: в браузере недостаточно места или сохранение запрещено. Экспортируйте запись в файл.");
    }
}
export function listEntries(kind?: LibraryKind): LibraryEntry[] {
    let entries = readLibrary();
    // Keep old storage untouched; only migrate once into a separately named entry.
    const migrated = localStorage.getItem("cellhub.library.legacy-migrated") === "1";
    const legacy = localStorage.getItem("cellhub.genome.v2") ?? localStorage.getItem("cellhub.genome.v1");
    if (!migrated && legacy !== null && !entries.some(e => e.id === LEGACY_ID)) {
        let genome: Mode[];
        try {
            genome = parseGenome(legacy);
        }
        catch {
            throw new Error("Прежний геном повреждён. Его исходная запись оставлена без изменений.");
        }
        entries = [...entries, { id: LEGACY_ID, name: "Прежний геном", kind: "genome", updatedAt: new Date().toISOString(), data: { genome, initialMode: 0 } }];
        writeLibrary(entries);
        localStorage.setItem("cellhub.library.legacy-migrated", "1");
    }
    return entries.filter(e => !kind || e.kind === kind).sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
}
export function saveEntry(kind: LibraryKind, name: string, data: unknown, id?: string): LibraryEntry {
    const entries = listEntries();
    const normalized = validName(name);
    if (entries.some(e => e.kind === kind && e.name.toLocaleLowerCase() === normalized.toLocaleLowerCase() && e.id !== id))
        throw new Error("Запись с таким названием уже существует. Выберите другое название.");
    if (id && !entries.some(e => e.id === id && e.kind === kind))
        throw new Error("Сохранение для замены не найдено.");
    const entry: LibraryEntry = { id: id ?? crypto.randomUUID(), kind, name: normalized, updatedAt: new Date().toISOString(), data: validateData(kind, data) };
    writeLibrary([...entries.filter(e => e.id !== entry.id), entry]);
    return entry;
}
export function removeEntry(id: string) { writeLibrary(listEntries().filter(e => e.id !== id)); }
export function renameEntry(id: string, name: string): LibraryEntry {
    const entry = listEntries().find(e => e.id === id);
    if (!entry)
        throw new Error("Запись уже удалена.");
    return saveEntry(entry.kind, name, entry.data, id);
}
export function parseEntry(text: string): LibraryEntry {
    if (text.length > MAX_FILE_SIZE)
        throw new Error("Файл слишком большой. Максимальный размер — 20 МБ.");
    let raw: unknown;
    try {
        raw = JSON.parse(text);
    }
    catch {
        throw new Error("Файл не удалось прочитать.");
    }
    if (record(raw) && raw.format === "cellhub-library" && raw.version === 1)
        return validateEntry(raw.entry);
    // Accept the previous standalone genome format as an import, without changing it.
    try {
        const genome = parseGenome(text);
        return { id: crypto.randomUUID(), kind: "genome", name: "Импортированный геном", updatedAt: new Date().toISOString(), data: { genome, initialMode: 0 } };
    }
    catch {
        throw new Error("Формат сохранения не поддерживается или файл повреждён.");
    }
}
export function importEntry(text: string): LibraryEntry {
    const entry = parseEntry(text);
    const names = new Set(listEntries(entry.kind).map(e => e.name.toLocaleLowerCase()));
    let name = entry.name;
    for (let n = 2; names.has(name.toLocaleLowerCase()); n++)
        name = `${entry.name.slice(0, 70)} (${n})`;
    return saveEntry(entry.kind, name, entry.data);
}
export function exportEntry(entry: LibraryEntry): string {
    return JSON.stringify({ format: "cellhub-library", version: 1, entry: validateEntry(entry) }, null, 2);
}
export function downloadEntry(entry: LibraryEntry) {
    const url = URL.createObjectURL(new Blob([exportEntry(entry)], { type: "application/json" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = `${entry.name.replace(/[<>:"/\\|?*\x00-\x1f]/g, "_")}.${entry.kind}.json`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
}
