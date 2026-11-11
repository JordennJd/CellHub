import { useEffect, useRef, useState } from 'react';
import { DEFAULT_ENVIRONMENT, type Command, type Environment, type Frame, type Mode, flatten, inflate } from '../model';
import type { LevelStatus } from './levels';
type InspectResult = {
    genome: Mode[];
    mode: number;
};
type PendingRequest = {
    resolve: (value: any) => void;
    reject: (error: Error) => void;
    timer: ReturnType<typeof setTimeout>;
};
type InspectRequest = PendingRequest & {
    id: number;
};
type Transition = {
    id: string;
    kind: 'reset' | 'restore';
    level: boolean;
    inserted: number;
    previousFrame: Frame | null;
    previousInserted: number;
    previousPending: number;
};
const REQUEST_TIMEOUT = 15000;
function parseLevelStatus(raw: unknown): LevelStatus | null {
    if (typeof raw !== 'string' || !raw)
        return null;
    try {
        const value = JSON.parse(raw) as LevelStatus;
        return value && typeof value.id === 'string' && value.id ? value : null;
    }
    catch {
        return null;
    }
}
export function useSimulation() {
    const worker = useRef<Worker | null>(null), available = useRef(false);
    const [ready, setReady] = useState(false), [frame, setFrame] = useState<Frame | null>(null);
    const [levelStatus, setLevelStatus] = useState<LevelStatus | null>(null);
    const [resetting, setResetting] = useState(false);
    const [error, setError] = useState(''), [speed, setSpeed] = useState(0);
    const [environment, setEnvironment] = useState<Environment>({ ...DEFAULT_ENVIRONMENT });
    const [inserted, setInserted] = useState(0);
    const names = useRef(new Map<string, string[]>());
    const requests = useRef(new Map<string, PendingRequest>());
    const inspectRequest = useRef<InspectRequest | null>(null);
    const transition = useRef<Transition | null>(null);
    const pendingSpawns = useRef(0), successfulSpawns = useRef(0);
    function send(command: Command) { worker.current?.postMessage(command); }
    function changeSpeed(value: number) {
        if (transition.current && value !== 0)
            return;
        setSpeed(value);
        send({ type: 'speed', value });
    }
    function cancelInspect(reason: Error) {
        const pending = inspectRequest.current;
        inspectRequest.current = null;
        if (pending) {
            clearTimeout(pending.timer);
            pending.reject(reason);
        }
    }
    function rejectRequest(id: string, reason: Error) {
        const pending = requests.current.get(id);
        if (pending) {
            clearTimeout(pending.timer);
            requests.current.delete(id);
            pending.reject(reason);
        }
    }
    function rejectAll(reason: Error) {
        cancelInspect(reason);
        for (const id of [...requests.current.keys()])
            rejectRequest(id, reason);
    }
    useEffect(() => {
        const w = new Worker(new URL('../simulation.worker.ts', import.meta.url), { type: 'module' });
        worker.current = w;
        w.onmessage = ({ data }) => {
            if (data.type === 'ready') {
                available.current = true;
                w.postMessage({ type: 'speed', value: 0 });
                setReady(true);
            }
            // Worker messages are ordered. Ignore old frames and spawn acknowledgments
            // until the matching transition acknowledgment establishes the new world.
            if (data.type === 'frame') {
                if (transition.current)
                    transition.current.previousFrame = data;
                else {
                    setFrame(data);
                    setLevelStatus(parseLevelStatus(data.level));
                }
            }
            if (data.type === 'spawned') {
                if (transition.current) {
                    transition.current.previousInserted++;
                    transition.current.previousPending = Math.max(0, transition.current.previousPending - 1);
                }
                else {
                    pendingSpawns.current = Math.max(0, pendingSpawns.current - 1);
                    successfulSpawns.current++;
                    setInserted(successfulSpawns.current);
                }
            }
            if (data.type === 'reset' || data.type === 'restored') {
                const current = transition.current;
                if (!current || current.id !== data.requestId)
                    return;
                transition.current = null;
                successfulSpawns.current = current.inserted;
                pendingSpawns.current = 0;
                setInserted(current.inserted);
                setResetting(false);
                if (data.settings && (data.type === 'restored' || current.level))
                    setEnvironment(data.settings);
                const pending = requests.current.get(data.requestId);
                if (pending) {
                    clearTimeout(pending.timer);
                    requests.current.delete(data.requestId);
                    pending.resolve(true);
                }
            }
            if (data.type === 'error') {
                console.warn('CellHub command failed', data.command, data.message);
                const messages: Record<string, string> = { restore: 'Не удалось загрузить эксперимент: файл повреждён или несовместим.', spawn: 'Не удалось поместить клетку. Проверьте геном и доступное место в чашке.', inspect: 'Клетка уже исчезла. Выберите другую.', move: 'Не удалось перенести клетку. Выберите её заново.', edit: 'Не удалось изменить клетку. Выберите её заново.', boost: 'Не удалось подпитать клетку. Выберите её заново.', export: 'Не удалось сохранить эксперимент. Попробуйте ещё раз.' };
                const reason = new Error(messages[data.command] ?? 'Действие недоступно. Попробуйте ещё раз.');
                setError(reason.message);
                if (data.command === 'spawn') {
                    if (transition.current)
                        transition.current.previousPending = Math.max(0, transition.current.previousPending - 1);
                    else
                        pendingSpawns.current = Math.max(0, pendingSpawns.current - 1);
                }
                if (data.command === 'inspect')
                    cancelInspect(reason);
                if (typeof data.requestId === 'string')
                    rejectRequest(data.requestId, reason);
                // Initialization and uncategorized worker failures affect every request.
                if (!data.command) {
                    rejectAll(reason);
                    available.current = false;
                    setReady(false);
                }
            }
            if (data.type === 'genome' && inspectRequest.current?.id === data.id && !transition.current) {
                const pending = inspectRequest.current;
                if (!pending)
                    return;
                inspectRequest.current = null;
                clearTimeout(pending.timer);
                try {
                    if (!data.genome.length)
                        throw new Error('Клетка уже разделилась или погибла.');
                    const labels = names.current.get(Array.from(data.genome as Float32Array).join(','));
                    pending.resolve({ genome: inflate(data.genome).map((m, i) => ({ ...m, name: labels?.[i] ?? m.name })), mode: data.mode });
                }
                catch (reason) {
                    pending.reject(reason instanceof Error ? reason : new Error(String(reason)));
                }
            }
            if (data.type === 'export') {
                const pending = requests.current.get(data.requestId);
                if (pending) {
                    clearTimeout(pending.timer);
                    requests.current.delete(data.requestId);
                    pending.resolve(data.state);
                }
            }
        };
        w.onerror = () => {
            const reason = new Error('Не удалось запустить лабораторию. Перезагрузите страницу.');
            available.current = false;
            setReady(false);
            setError(reason.message);
            rejectAll(reason);
            transition.current = null;
            setResetting(false);
        };
        return () => {
            available.current = false;
            w.terminate();
            worker.current = null;
            rejectAll(new Error('Лаборатория закрыта.'));
            transition.current = null;
        };
    }, []);
    function configure(settings: Environment) { setEnvironment(settings); send({ type: 'configure', settings }); }
    function spawn(x: number, y: number, mode: number, genome: Mode[], budget = Infinity) {
        if (!available.current || transition.current)
            return;
        if (successfulSpawns.current + pendingSpawns.current >= budget) {
            setError('Лимит вставок исчерпан. Перезапустите попытку.');
            return;
        }
        const data = flatten(genome);
        names.current.set(Array.from(data).join(','), genome.map(m => m.name));
        pendingSpawns.current++;
        send({ type: 'spawn', x, y, mode, genome: data });
    }
    function request<T>(key: string, command: Command): Promise<T> {
        if (!available.current)
            return Promise.reject(new Error('Лаборатория ещё не готова.'));
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                requests.current.delete(key);
                reject(new Error('Действие не завершилось. Попробуйте ещё раз.'));
            }, REQUEST_TIMEOUT);
            requests.current.set(key, { resolve, reject, timer });
            send(command);
        });
    }
    function beginTransition(kind: Transition['kind'], count: number, level = false): string {
        const id = crypto.randomUUID();
        cancelInspect(new Error('Среда изменена. Выберите клетку заново.'));
        transition.current = { id, kind, level, inserted: count, previousFrame: frame,
            previousInserted: successfulSpawns.current, previousPending: pendingSpawns.current };
        setResetting(true);
        setFrame(null);
        setLevelStatus(null);
        setSpeed(0);
        setInserted(0);
        pendingSpawns.current = 0;
        successfulSpawns.current = 0;
        return id;
    }
    function failedTransition(id: string) {
        const current = transition.current;
        if (current?.id !== id)
            return;
        transition.current = null;
        setResetting(false);
        setFrame(current.previousFrame);
        successfulSpawns.current = current.previousInserted;
        pendingSpawns.current = current.previousPending;
        setInserted(current.previousInserted);
    }
    function reset(settings = environment, chaos?: { seed: number; genome: Float32Array }, bounds?: { width: number; height: number }) {
        if (transition.current)
            return;
        const requestId = beginTransition('reset', 0);
        setEnvironment(settings);
        void request<boolean>(requestId, { type: 'reset', requestId, chaos, bounds })
            .catch(reason => { failedTransition(requestId); setError(String(reason)); });
        configure(settings);
    }
    function resetLevel(levelText: string) {
        if (transition.current)
            return;
        const requestId = beginTransition('reset', 0, true);
        void request<boolean>(requestId, { type: 'reset', requestId, level: levelText })
            .catch(reason => { failedTransition(requestId); setError(String(reason)); });
    }
    function playDemo(restart = false) {
        if (!available.current || transition.current)
            return;
        if (restart) {
            setInserted(0);
            pendingSpawns.current = 0;
            successfulSpawns.current = 0;
        }
        setSpeed(4);
        send({ type: 'demo', restart });
    }
    function stopDemo() {
        if (!available.current || transition.current)
            return;
        setSpeed(0);
        send({ type: 'stopDemo' });
    }
    async function exportWorld(): Promise<string> {
        if (transition.current)
            throw new Error('Дождитесь загрузки среды.');
        const requestId = crypto.randomUUID();
        return request<string>(requestId, { type: 'export', requestId });
    }
    async function restore(state: string, count: number) {
        if (transition.current)
            throw new Error('Дождитесь загрузки среды.');
        const requestId = beginTransition('restore', count);
        try {
            await request<boolean>(requestId, { type: 'restore', state, requestId });
        }
        catch (reason) {
            failedTransition(requestId);
            throw reason;
        }
    }
    function inspect(id: number): Promise<InspectResult> {
        if (!available.current || transition.current)
            return Promise.reject(new Error('Дождитесь загрузки среды.'));
        cancelInspect(new Error('Выбрана другая клетка.'));
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                if (inspectRequest.current?.timer === timer)
                    inspectRequest.current = null;
                reject(new Error('Не удалось получить геном клетки. Попробуйте ещё раз.'));
            }, REQUEST_TIMEOUT);
            inspectRequest.current = { id, resolve, reject, timer };
            send({ type: 'inspect', id });
        });
    }
    return { ready, resetting, frame, levelStatus, error, setError, speed, changeSpeed, environment, configure, inserted, send, spawn, reset, resetLevel, playDemo, stopDemo, exportWorld, restore, inspect, names };
}
