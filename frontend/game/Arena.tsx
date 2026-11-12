import { useTouchControls } from "../input/useTouchControls";
import { CellDetails } from "./CellDetails";
import { useEffect, useRef, useState } from 'react';
import { CELL_DETAILS_STRIDE, STRIDE, TYPES, defaultGenome, type Mode } from '../model';
import { OnlinePanel } from '../OnlinePanel';
import { WorldView, type ViewState } from '../WorldView';
import { initialBackend, type BackendPreference } from '../backend';
import { GenomeEditor } from '../editor/GenomeEditor';
import { Library } from './Library';
import { Modal } from './Modal';
import { SimulationHeader } from './SimulationHeader';
import { saveEntry, type GenomeData, type LibraryEntry } from './storage';
import { useArena, type ArenaInspectResult } from './useArena';
import './arena.css';

const PLAYER_KEY = 'cellhub.player';

function defaultServerUrl() {
    const requested = new URLSearchParams(location.search).get('server');
    if (requested)
        return requested;
    return `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`;
}

function storedPlayer() {
    try {
        return localStorage.getItem(PLAYER_KEY) || 'Игрок';
    }
    catch {
        return 'Игрок';
    }
}

function rememberPlayer(name: string) {
    try {
        localStorage.setItem(PLAYER_KEY, name);
    }
    catch {
        // A blocked storage is not a reason to refuse the network game.
    }
}

function failure(error: unknown) {
    return error instanceof Error ? error.message : String(error);
}

export function Arena({ onExit }: { onExit: () => void }) {
    const touchControls = useTouchControls();
    const arena = useArena();
    const [serverUrl] = useState(defaultServerUrl);
    const [playerName] = useState(storedPlayer);
    const [lobbyOpen, setLobbyOpen] = useState(true);
    const [genome, setGenome] = useState<Mode[]>(defaultGenome);
    const [initialMode, setInitialMode] = useState(0);
    const [tab, setTab] = useState('genome');
    const [tool, setTool] = useState('place');
    const [layer, setLayer] = useState('biome');
    const [selected, setSelected] = useState<number | null>(null);
    const [diagnostic, setDiagnostic] = useState<ArenaInspectResult | null>(null);
    const [backend, setBackend] = useState<BackendPreference>(initialBackend);
    const [recenter, setRecenter] = useState(0);
    const [zoomStep, setZoomStep] = useState(0);
    const [view, setView] = useState<ViewState>({ x: 0, y: 0, zoom: 1 });
    const [library, setLibrary] = useState(false);
    const [saveKind, setSaveKind] = useState<'genome' | null>(null);
    const [name, setName] = useState('');
    const [busy, setBusy] = useState(false);
    const selectedRef = useRef<number | null>(null);
    selectedRef.current = selected;
    const online = arena.phase === 'online';
    const room = arena.room;
    const stats = arena.frame?.stats;
    const count = stats?.[1] ?? 0;

    let cell: Float32Array | undefined, details: Float32Array | undefined, nitrogen: number | undefined;
    let connections = 0;
    if (arena.frame && selected !== null)
        for (let i = 0; i < arena.frame.cells.length; i += STRIDE)
            if (arena.frame.cells[i] === selected) {
                cell = arena.frame.cells.subarray(i, i + STRIDE);
                nitrogen = arena.frame.nitrogen?.[i / STRIDE];
                details = arena.frame.details?.subarray(i / STRIDE * CELL_DETAILS_STRIDE, (i / STRIDE + 1) * CELL_DETAILS_STRIDE);
                break;
            }

    if (cell && arena.frame) {
        for (const endpoint of arena.frame.links) if (endpoint === selected) connections++;
    }

    useEffect(() => {
        setLobbyOpen(arena.phase !== 'online');
    }, [arena.phase]);

    useEffect(() => {
        setSelected(null);
        setDiagnostic(null);
    }, [arena.resetVersion]);

    useEffect(() => {
        if (online)
            return;
        setSelected(null);
        setDiagnostic(null);
    }, [online]);

    useEffect(() => {
        if (online && selected !== null && selected > 0 && cell) void inspectCell(selected);
    }, [online, selected, cell?.[4], cell?.[8], details?.[11]]);

    function exit() {
        arena.disconnect();
        onExit();
    }

    async function inspectCell(id: number) {
        setDiagnostic(null);
        try {
            const result = await arena.inspect(id);
            if (selectedRef.current === id)
                setDiagnostic(result);
        }
        catch (error) {
            arena.setError(failure(error));
        }
    }

    function point(x: number, y: number, id: number | null, action: 'place' | 'select') {
        if (!online)
            return;
        if (action === 'place') {
            arena.spawn(x, y, initialMode, genome);
            return;
        }
        if (id === null) {
            setSelected(null);
            setDiagnostic(null);
            return;
        }
        if (id < 0) {
            arena.setNotice('Клетка ещё не подтверждена сервером.');
            return;
        }
        setSelected(id);
        setTab('observe');

    }

    async function copyGenome() {
        if (selected === null)
            return;
        try {
            const result = await arena.inspect(selected);
            setGenome(result.genome);
            setInitialMode(result.mode);
            setTab('genome');
            arena.setNotice('Геном клетки скопирован в редактор.');
        }
        catch (error) {
            arena.setError(failure(error));
        }
    }

    function applyGenome() {
        if (selected === null)
            return;
        if (arena.edit(selected, initialMode, genome))
            arena.setNotice('Геном редактора передан выбранной клетке.');
        else
            arena.setError('Нет соединения с сервером.');
    }

    function resetWorld() {
        arena.reset();
        setSelected(null);
        setDiagnostic(null);
    }

    async function saveGenome() {
        if (busy)
            return;
        setBusy(true);
        arena.setError('');
        try {
            saveEntry('genome', name, { genome, initialMode });
            setSaveKind(null);
            arena.setNotice('Геном сохранён в банк геномов.');
        }
        catch (error) {
            arena.setError(failure(error));
        }
        finally {
            setBusy(false);
        }
    }

    function loadGenome(entry: LibraryEntry) {
        const data = entry.data as GenomeData;
        setGenome(data.genome);
        setInitialMode(data.initialMode);
        setLibrary(false);
        arena.setNotice('Геном загружен в редактор арены.');
    }

    return <div className="laboratory arena-shell" data-controls={touchControls ? "touch" : "mouse"}>
        <SimulationHeader title={room ? `Арена · ${room.name}` : 'Арена'} onExit={exit}
            action={{ label: 'Комната', run: () => setLobbyOpen(true) }}/>
        <main className="lab-layout">
            <section className="microscope" aria-label="Общая сцена арены">
                <div className="canvas-actions">
                    <div className="canvas-view-controls" role="group" aria-label="Масштаб поля">
                        <button aria-label="Уменьшить масштаб" onClick={() => setZoomStep(value => value - 1)}>−</button>
                        <button aria-label="Увеличить масштаб" onClick={() => setZoomStep(value => value + 1)}>+</button>
                        <button onClick={() => setRecenter(value => value + 1)}>Всё поле</button>
                    </div>
                    <label><span className="sr-only">Показать слой</span>
                        <select value={layer} onChange={event => setLayer(event.target.value)}>
                            <option value="biome">Среда</option>
                            <option value="light">Свет</option>
                            <option value="food">Пища</option><option value="covered">Покрытая пища</option>
                            <option value="nitrates">Нитраты</option>
                            <option value="radiation">Радиация</option>
                        </select>
                    </label>
                    <span className="arena-zoom" aria-label="Текущий масштаб">{Math.round(view.zoom * 100)}%</span>
                </div>
                <div className="canvas-wrap">
                    <WorldView frame={arena.frame} tool={tool} layer={layer} selected={selected}
                        backend={backend} onPoint={point} onView={setView} recenter={recenter} zoomStep={zoomStep}
                        onError={() => arena.setError('Не удалось показать общую сцену. Попробуйте перезагрузить страницу.')}
                        onBackendFailure={failed => {
                            if (failed === 'webgpu')
                                setBackend('webgl');
                            else
                                arena.setError('В этом браузере не удалось открыть изображение арены.');
                        }}/>
                    {!online && <div className="canvas-note" role="status">Подключение к серверу арены…</div>}
                    {online && !count && <div className="canvas-note">Поле свободно. Поместите первую клетку.</div>}
                </div>
                {touchControls && <div className="tool-bar" role="toolbar" aria-label="Инструменты арены">
                    {[['place', 'Поместить'], ['select', 'Диагностика'], ['pan', 'Обзор']].map(([value, label]) =>
                        <button key={value} aria-pressed={tool === value} onClick={() => setTool(value)}>{label}</button>)}
                </div>}
                <p className="control-hint">{touchControls ? 'Касание — действие · Перетаскивание — обзор · Щипок — масштаб' : 'ЛКМ — поместить · ПКМ — выбрать · Зажатое колёсико — обзор · Колесо — масштаб'}</p>
                <div className="transport">
                    <div className={`arena-status ${online ? 'online' : ''}`} role="status">
                        <span className="arena-status-dot" aria-hidden="true"/>
                        {online ? 'В сети' : 'Нет соединения'}

                    </div>
                    <div className="world-stats">
                        <span>Время <strong>{((arena.sync?.lastTick ?? 0) / 30).toFixed(1)} с</strong></span>
                        <span>Клеток <strong>{count}</strong></span>

                    </div>
                    <button disabled={!online || !arena.host} onClick={resetWorld}
                        title={!arena.host ? 'Очистить мир может только создатель комнаты' : undefined}>
                        Очистить мир
                    </button>
                </div>
            </section>
            <aside className="lab-sidebar">
                <nav className="tabs" aria-label="Панели арены">
                    {[['genome', 'Геном'], ['observe', 'Наблюдение'], ['room', 'Комната']].map(([value, label]) =>
                        <button key={value} aria-current={tab === value ? 'page' : undefined} onClick={() => setTab(value)}>{label}</button>)}
                </nav>
                <div className="sidebar-content">
                    {tab === 'genome' && <GenomeEditor genome={genome} onChange={setGenome} initialMode={initialMode}
                        onInitialModeChange={setInitialMode} onSave={() => { setName(''); setSaveKind('genome'); }}
                        onLoad={() => setLibrary(true)}/>}
                    {tab === 'observe' && <section className="panel-section">
                        <h2>Общая сцена</h2>
                        <dl className="metrics">
                            <div><dt>Игроков</dt><dd>{room?.players ?? 0}</dd></div>
                            <div><dt>Клеток</dt><dd>{count}</dd></div>
                            <div><dt>Связей</dt><dd>{(arena.frame?.links.length ?? 0) / 2}</dd></div>
                        </dl>
                        {cell ? <>
                            <h3>Выбранная клетка</h3>
                            <dl className="cell-details">
                                {[['Тип', TYPES[cell[4]]?.name], ['Слот', diagnostic?.genome[diagnostic.mode]?.name ?? '…'],
                                    ['Постоянных связей', connections], ['Зеркальность', cell[11] ? 'Да' : 'Нет'],
                                    ['Масса', `${cell[5].toFixed(2)} нг`], ['Азот', nitrogen === undefined ? '…' : `${(nitrogen * 100).toFixed(1)}%`],
                                    ['Возраст', `${cell[6].toFixed(1)} с`], ['Поколение', cell[7]],
                                    ['Изменений генома', cell[8]]].map(([key, value]) =>
                                    <div key={key}><dt>{key}</dt><dd>{value}</dd></div>)}
                            </dl>
                            <CellDetails values={details}/>
                            <button onClick={() => void copyGenome()}>Скопировать геном в редактор</button>
                            <button onClick={applyGenome}>Применить геном редактора</button>
                        </> : <p>{selected === null ? 'Выберите клетку инструментом «Диагностика».' : 'Клетка разделилась или погибла. Выберите потомка.'}</p>}
                    </section>}
                    {tab === 'room' && <section className="panel-section">
                        <h2>Комната</h2>
                        {room ? <>
                            <dl className="arena-room-metrics">
                                <div><dt>Название</dt><dd>{room.name}</dd></div>
                                <div><dt>Код</dt><dd><code>{room.id}</code></dd></div>
                                <div><dt>Создатель</dt><dd>{room.host}{arena.host ? ' (вы)' : ''}</dd></div>
                                <div><dt>Игроков</dt><dd>{room.players}</dd></div>
                                <div><dt>Тик сервера</dt><dd>{arena.sync?.lastTick ?? room.tick}</dd></div>
                                <div><dt>Чанков получено</dt><dd>{arena.sync?.seen ?? 0} / {arena.sync?.total ?? 36}</dd></div>
                            </dl>
                            <p className="arena-note">Мир 3072 × 3072 принадлежит серверу. Клиент получает по одному чанку за тик и предсказывает движение между обновлениями.</p>
                            <div className="button-row">
                                <button onClick={() => { arena.leaveRoom(); setLobbyOpen(true); }}>Выйти в список комнат</button>
                                <button className="quiet" onClick={() => { arena.disconnect(); setLobbyOpen(true); }}>Отключиться</button>
                            </div>
                        </> : <p>Подключитесь к серверу и выберите комнату.</p>}
                    </section>}
                </div>
            </aside>
        </main>
        {(arena.error || arena.notice) && <div className={'toast ' + (arena.error ? 'error' : '')} role={arena.error ? 'alert' : 'status'}>
            <span>{arena.error || arena.notice}</span>
            <button aria-label="Закрыть сообщение" onClick={() => { arena.setError(''); arena.setNotice(''); }}>×</button>
        </div>}
        <OnlinePanel open={lobbyOpen} phase={arena.phase} serverUrl={serverUrl} rooms={arena.rooms}
            room={room} host={arena.host} playerName={playerName} sync={arena.sync}
            onClose={() => online ? setLobbyOpen(false) : exit()}
            onConnect={url => arena.connect(url)}
            onCreate={(name, player) => { rememberPlayer(player); arena.createRoom(name, player); }}
            onJoin={(id, player) => { rememberPlayer(player); arena.joinRoom(id, player); }}
            onRefresh={arena.refreshRooms}
            onLeave={() => { arena.leaveRoom(); setLobbyOpen(true); }}
            onDisconnect={() => { arena.disconnect(); setLobbyOpen(true); }}/>
        {library && <Modal label="Банк геномов" onClose={() => setLibrary(false)}>
            <Library kind="genome" onLoad={loadGenome} onClose={() => setLibrary(false)}/>
        </Modal>}
        {saveKind && <Modal label="Сохранить геном" onClose={() => { if (!busy)
            setSaveKind(null); }}>
            <h2 id="save-title">Сохранить геном</h2>
            <form onSubmit={event => { event.preventDefault(); void saveGenome(); }}>
                <label>Название<input autoFocus required maxLength={80} value={name} onChange={event => setName(event.target.value)}/></label>
                <div className="button-row"><button type="button" disabled={busy} onClick={() => setSaveKind(null)}>Отмена</button>
                    <button className="primary" disabled={busy || !name.trim()}>{busy ? 'Сохраняем…' : 'Сохранить'}</button></div>
            </form>
        </Modal>}
    </div>;
}
