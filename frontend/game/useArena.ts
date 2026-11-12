import { useEffect, useRef, useState } from 'react';
import { chaosGenome, flatten, inflate, type Frame, type Mode } from '../model';
import { chaosOrganismsBuffer } from '../organisms';
import { MultiplayerClient, type NetPhase, type Welcome } from '../net/client';
import type { LeaderboardEntry, PlayerInfo, RoomInfo, RoomMode } from '../net/protocol';
import type { SyncState } from '../net/prediction';

export type ArenaInspectResult = { genome: Mode[]; mode: number };

type PendingInspect = {
    resolve: (value: ArenaInspectResult) => void;
    reject: (error: Error) => void;
    timer: ReturnType<typeof setTimeout>;
};

const REQUEST_TIMEOUT = 15000;

/**
 * Network-backed scene adapter used by the Arena.
 *
 * It mirrors the subset of `useSimulation` that the shared views need, but the
 * authoritative world lives on `cellhub-server`. Biology, tick rate and world
 * resets are never applied locally; the client only predicts cell positions
 * between server chunks.
 */
export function useArena() {
    const client = useRef<MultiplayerClient | null>(null);
    const phaseRef = useRef<NetPhase>('disconnected');
    const pendingInspects = useRef(new Map<number, PendingInspect>());
    const [phase, setPhase] = useState<NetPhase>('disconnected');
    const [rooms, setRooms] = useState<RoomInfo[]>([]);
    const [room, setRoom] = useState<RoomInfo | null>(null);
    const [player, setPlayer] = useState<PlayerInfo | null>(null);
    const [leaderboard, setLeaderboard] = useState<LeaderboardEntry[]>([]);
    const [host, setHost] = useState(false);
    const [frame, setFrame] = useState<Frame | null>(null);
    const [sync, setSync] = useState<SyncState | null>(null);
    const [error, setError] = useState('');
    const [notice, setNotice] = useState('');
    const [resetVersion, setResetVersion] = useState(0);
    const names = useRef(new Map<string, string[]>());

    function rejectInspect(id: number, reason: Error) {
        const pending = pendingInspects.current.get(id);
        if (!pending)
            return;
        clearTimeout(pending.timer);
        pendingInspects.current.delete(id);
        pending.reject(reason);
    }

    function rejectInspects(reason: Error) {
        for (const id of [...pendingInspects.current.keys()])
            rejectInspect(id, reason);
    }

    function clearRoom() {
        setRoom(null);
        setPlayer(null);
        setLeaderboard([]);
        setHost(false);
        setFrame(null);
        setSync(null);
    }

    useEffect(() => {
        const connection = new MultiplayerClient({
            onPhase: next => {
                phaseRef.current = next;
                setPhase(next);
                if (next !== 'online')
                    clearRoom();
            },
            onRooms: next => setRooms(next),
            onWelcome: (welcome: Welcome) => {
                setRoom(welcome.room);
                setPlayer(welcome.player);
                setLeaderboard([]);
                setHost(welcome.host);
                setFrame(null);
                setSync(null);
                setNotice(`Подключено: ${welcome.room.name} (${welcome.room.id}).`);
            },
            onFrame: (next, nextSync) => {
                setFrame(next);
                setSync(nextSync);
            },
            onInspect: (id, genome, mode) => {
                const pending = pendingInspects.current.get(id);
                if (!pending)
                    return;
                clearTimeout(pending.timer);
                pendingInspects.current.delete(id);
                if (!genome?.length) {
                    pending.reject(new Error('Клетка уже разделилась или погибла.'));
                    return;
                }
                try {
                    const values = Float32Array.from(genome);
                    const labels = names.current.get(Array.from(values).join(','));
                    pending.resolve({
                        genome: inflate(values).map((entry, index) => ({ ...entry, name: labels?.[index] ?? entry.name })),
                        mode: mode ?? 0,
                    });
                }
                catch (reason) {
                    pending.reject(reason instanceof Error ? reason : new Error(String(reason)));
                }
            },
            onError: message => setError(message),
            onNotice: message => setNotice(message),
            onWorldReset: () => {
                rejectInspects(new Error('Мир очищен создателем комнаты.'));
                setResetVersion(value => value + 1);
            },
            onPlayerCount: players => setRoom(current => current ? { ...current, players } : current),
            onLeaderboard: entries => setLeaderboard(entries),
            onDisconnected: () => {
                rejectInspects(new Error('Соединение с сервером закрыто.'));
                clearRoom();
            },
        });
        client.current = connection;
        return () => {
            rejectInspects(new Error('Арена закрыта.'));
            connection.close();
            client.current = null;
        };
    }, []);

    function rememberGenome(genome: Mode[]) {
        const values = flatten(genome);
        names.current.set(Array.from(values).join(','), genome.map(entry => entry.name));
        return values;
    }

    function connect(url: string) {
        setError('');
        setNotice('');
        client.current?.connect(url);
    }

    function createRoom(name: string, playerName: string, mode: RoomMode) {
        setError('');
        const chaos = mode === 'arena'
            ? {
                genome: Array.from(flatten(chaosGenome())),
                organisms: Array.from(chaosOrganismsBuffer()),
            }
            : undefined;
        client.current?.createRoom(name, playerName, mode, chaos);
    }

    function joinRoom(id: string, player: string) {
        setError('');
        client.current?.joinRoom(id, player);
    }

    function refreshRooms() {
        client.current?.listRooms();
    }

    function leaveRoom() {
        rejectInspects(new Error('Вы покинули комнату.'));
        client.current?.leaveRoom();
        clearRoom();
    }

    function disconnect() {
        rejectInspects(new Error('Соединение с сервером закрыто.'));
        client.current?.close();
        clearRoom();
    }

    function spawn(x: number, y: number, mode: number, genome: Mode[]) {
        if (phaseRef.current !== 'online') {
            setError('Нет соединения с сервером.');
            return false;
        }
        return client.current?.command({
            type: 'spawn', x, y, mode, genome: rememberGenome(genome),
        }) ?? false;
    }

    function edit(id: number, mode: number, genome: Mode[]) {
        if (phaseRef.current !== 'online') {
            setError('Нет соединения с сервером.');
            return false;
        }
        return client.current?.command({
            type: 'edit', id, mode, genome: rememberGenome(genome),
        }) ?? false;
    }

    function inspect(id: number): Promise<ArenaInspectResult> {
        if (phaseRef.current !== 'online')
            return Promise.reject(new Error('Нет соединения с сервером.'));
        const previous = pendingInspects.current.get(id);
        if (previous) {
            clearTimeout(previous.timer);
            pendingInspects.current.delete(id);
            previous.reject(new Error('Запрос отменён.'));
        }
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                pendingInspects.current.delete(id);
                reject(new Error('Сервер не ответил. Попробуйте ещё раз.'));
            }, REQUEST_TIMEOUT);
            pendingInspects.current.set(id, { resolve, reject, timer });
            if (!client.current?.command({ type: 'inspect', id })) {
                clearTimeout(timer);
                pendingInspects.current.delete(id);
                reject(new Error('Нет соединения с сервером.'));
            }
        });
    }

    function reset() {
        if (phaseRef.current !== 'online')
            return;
        if (!host) {
            setNotice('Очистить общий мир может только создатель комнаты.');
            return;
        }
        client.current?.command({ type: 'reset' });
    }

    return {
        phase, rooms, room, player, leaderboard, host, frame, sync, error, setError, notice, setNotice,
        resetVersion, names,
        connect, createRoom, joinRoom, refreshRooms, leaveRoom, disconnect,
        spawn, edit, inspect, reset,
    };
}

export type Arena = ReturnType<typeof useArena>;
