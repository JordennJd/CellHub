import { useTouchControls } from "../input/useTouchControls";
import { ChaosPanel, ChaosStatus, ChaosObservation, ChaosOverlay } from './ChaosPanel';
import { CellDetails } from "./CellDetails";
import { Modal } from './Modal';
import { useEffect, useRef, useState } from 'react';
import { WorldView, type ViewState } from '../WorldView';
import { initialBackend, type BackendPreference } from '../backend';
import { defaultGenome, chaosGenome, flatten, inflate, TYPES, STRIDE, CELL_DETAILS_STRIDE, DEFAULT_ENVIRONMENT, type Mode } from '../model';
import { GenomeEditor } from '../editor/GenomeEditor';
import { useSimulation } from './useSimulation';
import { EnvironmentPanel } from './EnvironmentPanel';
import { completeChallenge, TUTORIALS, completeTutorial, evaluateTutorialStep } from './challenges';
import { challengeBudget, starterGenome, type LevelManifest } from './levels';
import { ObjectivePanel } from './ObjectivePanel';
import { Library } from './Library';
import { saveEntry, type LibraryEntry, type GenomeData, type PlateData } from './storage';
import { readSettings } from './Settings';
export type Session = {
    key: string;
    kind: 'sandbox' | 'challenge' | 'tutorial' | 'arena' | 'chaos';
    level?: LevelManifest;
    challengeId?: string;
    tutorialId?: string;
    entry?: LibraryEntry;
};
const TOOL_BY_NAME: Record<string, string> = { place: 'place', select: 'select', move: 'move', add: 'boost', remove: 'remove' };
function levelTools(level?: LevelManifest): [string, string][] {
    const labels: [string, string][] = [['place', 'Поместить'], ['select', 'Диагностика'], ['pan', 'Обзор'],
        ['move', 'Перенести'], ['boost', 'Подпитать'], ['remove', 'Удалить']];
    if (!level) return labels;
    const allowed = new Set(level.tools.filter(name => TOOL_BY_NAME[name]).map(name => TOOL_BY_NAME[name]));
    return labels.filter(([value]) => value === 'pan' || allowed.has(value));
}
export function Laboratory({ session, onExit }: {
    session: Session;
    onExit: () => void;
}) {
    const touchControls = useTouchControls();
    const chaosMode = session.kind === 'chaos';
    const canvasHost = useRef<HTMLDivElement>(null);
    const chaosSeed = useRef(crypto.getRandomValues(new Uint32Array(1))[0]);
    const sim = useSimulation(), level = session.level;
    const lesson = TUTORIALS.find(t => t.id === session.tutorialId) ?? TUTORIALS[0];
    const starter = level ? starterGenome(level) : null;
    const [genome, setGenome] = useState<Mode[]>(() => session.entry ? (session.entry.data as GenomeData).genome : structuredClone(starter?.genome ?? (session.kind === 'tutorial' ? lesson.genome : chaosMode ? chaosGenome() : defaultGenome())));
    const [initialMode, setInitialMode] = useState(session.entry ? (session.entry.data as GenomeData).initialMode : starter?.initialMode ?? 0);
    const [tab, setTab] = useState(level || session.kind === 'tutorial' ? 'objective' : 'genome');
    const tools = levelTools(level);
    const [tool, setTool] = useState(tools[0]?.[0] ?? 'place'), [selected, setSelected] = useState<number | null>(null), [layer, setLayer] = useState('biome');
    const [backend, setBackend] = useState<BackendPreference>(initialBackend), [recenter, setRecenter] = useState(0), [zoomStep, setZoomStep] = useState(0), [view, setView] = useState<ViewState>({ x: 0, y: 0, zoom: 1 });
    const [library, setLibrary] = useState(false), [saveKind, setSaveKind] = useState<'genome' | 'plate' | null>(null), [name, setName] = useState(''), [busy, setBusy] = useState(false), [notice, setNotice] = useState('');
    const [completed, setCompleted] = useState(false), [help, setHelp] = useState<'hint' | 'spoiler' | null>(null), [tutorial, setTutorial] = useState(0), [tutorialDone, setTutorialDone] = useState(false);
    const [savedGenome, setSavedGenome] = useState(false);
    const [progressSaved, setProgressSaved] = useState(false);
    const [diagnostic, setDiagnostic] = useState<{ genome: Mode[]; mode: number } | null>(null);
    const [diagnosticRevision, setDiagnosticRevision] = useState(0);
    const [title, setTitle] = useState(session.entry?.kind === 'plate' ? session.entry.name : level?.title ?? (session.kind === 'tutorial' ? lesson.title : chaosMode ? 'Хаос' : 'Новый эксперимент'));
    const [confirm, setConfirm] = useState<{
        title: string;
        message: string;
        action: () => void;
    } | null>(null);
    const [plateId, setPlateId] = useState(session.entry?.kind === 'plate' ? session.entry.id : undefined);
    const initialized = useRef(false), victory = useRef(false), copyPending = useRef(false), demoPlayed = useRef(false);
    const stats = sim.frame?.stats, count = stats?.[1] ?? 0;
    let cell: Float32Array | undefined, details: Float32Array | undefined, nitrogen: number | undefined;
    let connections = 0;
    if (sim.frame && selected !== null)
        for (let i = 0; i < sim.frame.cells.length; i += STRIDE)
            if (sim.frame.cells[i] === selected) {
                cell = sim.frame.cells.subarray(i, i + STRIDE);
                nitrogen = sim.frame.nitrogen?.[i / STRIDE];
                details = sim.frame.details?.subarray(i / STRIDE * CELL_DETAILS_STRIDE, (i / STRIDE + 1) * CELL_DETAILS_STRIDE);
                break;
            }
    if (cell && sim.frame) {
        for (const endpoint of sim.frame.links) if (endpoint === selected) connections++;
    }
    function newBounds() {
        const box = canvasHost.current?.getBoundingClientRect();
        const height = chaosMode ? 6144 : 3072;
        const ratio = box && box.height > 0 ? box.width / box.height : 1.6;
        return { width: Math.round(3072 * Math.max(0.5, Math.min(4, ratio)) / 64) * 64 * (chaosMode ? 2 : 1), height };
    }
    useEffect(() => {
        if (!sim.ready || initialized.current)
            return;
        initialized.current = true;
        if (session.entry?.kind === 'plate') {
            const data = session.entry.data as PlateData;
            if (data.genomeNames)
                sim.names.current = new Map(Object.entries(data.genomeNames).flatMap(([key, names]) => {
                    try { return [[Array.from(flatten(inflate(new Float32Array(key.split(',').map(Number))))).join(','), names] as [string, string[]]]; }
                    catch { return []; }
                }));
            sim.restore(data.state, data.inserted).catch(e => sim.setError(String(e)));
        }
        else if (level)
            sim.resetLevel(JSON.stringify(level));
        else
            sim.reset({ ...DEFAULT_ENVIRONMENT, ...(session.kind === 'tutorial' ? lesson.environment : {}) }, chaosMode ? { seed: chaosSeed.current, genome: flatten(genome) } : undefined, newBounds());
    }, [sim.ready]);
    useEffect(() => {
        if (!level || sim.resetting || victory.current || sim.levelStatus?.state !== 'won')
            return;
        victory.current = true;
        sim.changeSpeed(0);
        if (!demoPlayed.current) {
            setCompleted(true);
            saveProgress();
        }
    }, [sim.levelStatus?.state, sim.resetting]);
    useEffect(() => {
        if (chaosMode && sim.ready && !sim.resetting) {
            const values = flatten(genome);
            sim.names.current.set(Array.from(values).join(','), genome.map(mode => mode.name));
            sim.send({ type: 'chaos-pool', genome: values });
        }
    }, [genome, chaosMode, sim.ready, sim.resetting]);
    useEffect(() => {
        if (session.kind !== 'tutorial' || tutorialDone || sim.resetting)
            return;
        const step = lesson.steps[tutorial];
        if (!evaluateTutorialStep(step, { genome, initialMode, inserted: sim.inserted, running: sim.speed > 0, frame: sim.frame, selected: !!cell, savedGenome }))
            return;
        if (step.requirement === 'divide' || step.requirement === 'livingBond') {
            sim.changeSpeed(0);
            if (lesson.steps[tutorial + 1]?.requirement === 'inspect') setTool('select');
        }
        if (tutorial < lesson.steps.length - 1) setTutorial(tutorial + 1);
        else {
            setTutorialDone(true);
            saveProgress();
        }
    }, [genome, initialMode, sim.inserted, sim.speed, sim.frame, selected, tutorial, savedGenome, tutorialDone, sim.resetting]);
    useEffect(() => {
        setDiagnostic(null);
        if (selected === null || sim.resetting || !cell) return;
        let active = true;
        sim.inspect(selected).then(result => { if (active) setDiagnostic(result); }).catch(() => {});
        return () => { active = false; };
    }, [selected, cell?.[4], cell?.[8], details?.[11], sim.resetting, diagnosticRevision]);
    useEffect(() => { const hidden = () => { if (document.hidden && readSettings().pauseWhenHidden)
        sim.changeSpeed(0); }; const before = (e: BeforeUnloadEvent) => { if (readSettings().confirmExit) {
        e.preventDefault();
        e.returnValue = '';
    } }; document.addEventListener('visibilitychange', hidden); window.addEventListener('beforeunload', before); return () => { document.removeEventListener('visibilitychange', hidden); window.removeEventListener('beforeunload', before); }; }, []);
    function failure(message: unknown) { sim.setError(message instanceof Error ? message.message : String(message)); }
    function saveProgress() {
        try {
            if (level) completeChallenge(level.id);
            else completeTutorial(lesson.id);
            setProgressSaved(true);
            sim.setError('');
        } catch (e) {
            setProgressSaved(false);
            failure(e);
        }
    }
    function ask(title: string, message: string, action: () => void, enabled = true) { if (enabled) {
        sim.changeSpeed(0);
        setConfirm({ title, message, action });
    }
    else
        action(); }
    function leave() { ask('Выйти из лаборатории?', 'Несохранённые изменения эксперимента и генома будут потеряны.', onExit, readSettings().confirmExit); }
    function reset() { ask('Начать заново?', 'Текущие клетки и время будут сброшены. Геном редактора останется.', () => { if (level)
        sim.resetLevel(JSON.stringify(level));
    else
        sim.reset(session.kind === 'tutorial' ? lesson.environment : sim.environment); setSelected(null); victory.current = false; demoPlayed.current = false; setCompleted(false); setTutorial(0); setTutorialDone(false); setSavedGenome(false); setProgressSaved(false); }, readSettings().confirmReset); }
    function demo() {
        if (!level || !sim.ready || sim.resetting)
            return;
        if (sim.levelStatus?.demo) {
            sim.stopDemo();
            return;
        }
        const restart = !sim.levelStatus || sim.levelStatus.state !== 'running' || sim.levelStatus.elapsed > 0 || sim.inserted > 0;
        demoPlayed.current = true;
        victory.current = false;
        setCompleted(false);
        setProgressSaved(false);
        setSelected(null);
        sim.playDemo(restart);
    }
    function point(x: number, y: number, id: number | null, action: 'place' | 'select') {
        if (!sim.ready || sim.resetting || busy)
            return;
        if (tool === 'move' && selected !== null) {
            if (sim.levelStatus?.demo)
                sim.stopDemo();
            sim.send({ type: 'move', id: selected, x, y });
            setTool(touchControls ? 'select' : 'place');
            return;
        }
        if (action === 'place') {
            if (level && !level.tools.includes('place')) {
                failure('В этом испытании нельзя помещать новые клетки.');
                return;
            }
            if (level && genome.some(m => !level.allowedKinds.includes(m.kind))) {
                failure('Этот тип клетки недоступен в испытании.');
                return;
            }
            if (sim.levelStatus?.demo)
                sim.stopDemo();
            sim.spawn(x, y, initialMode, genome, level ? challengeBudget(level) : undefined);
            return;
        }
        if (id !== null && tool === 'remove') {
            if (sim.levelStatus?.demo)
                sim.stopDemo();
            sim.send({ type: 'remove', id });
            if (selected === id)
                setSelected(null);
            return;
        }
        if (id !== null && tool === 'boost') {
            if (sim.levelStatus?.demo)
                sim.stopDemo();
            sim.send({ type: 'boost', id });
            return;
        }
        setSelected(id);
        setTab('observe');
    }
    async function copyGenome() { if (selected === null || copyPending.current)
        return; const id = selected; ask('Заменить геном редактора?', 'Текущий черновик будет заменён геномом выбранной клетки.', () => { copyPending.current = true; sim.inspect(id).then(({ genome, mode }) => { setGenome(genome); setInitialMode(mode); setTab('genome'); setNotice('Геном клетки скопирован в редактор.'); }).catch(failure).finally(() => copyPending.current = false); }); }
    async function save() {
        if (!saveKind || busy)
            return;
        setBusy(true);
        sim.setError('');
        try {
            const data = saveKind === 'genome' ? { genome, initialMode } : { state: await sim.exportWorld(), genome, initialMode, environment: sim.environment, inserted: sim.inserted, genomeNames: Object.fromEntries(sim.names.current), ...(chaosMode ? { mode: 'chaos' } : {}), ...(level ? { challengeId: level.id } : {}) };
            const entry = saveEntry(saveKind, name, data, saveKind === 'plate' ? plateId : undefined);
            if (saveKind === 'plate') {
                setTitle(entry.name);
                setPlateId(entry.id);
            }
            setNotice(saveKind === 'genome' ? 'Геном сохранён.' : 'Эксперимент сохранён.');
            if (saveKind === 'genome' && session.kind === 'tutorial' && lesson.steps[tutorial].requirement === 'save')
                setSavedGenome(true);
            setSaveKind(null);
        }
        catch (e) {
            failure(e);
        }
        finally {
            setBusy(false);
        }
    }
    function load(entry: LibraryEntry) { const data = entry.data as GenomeData; if (level && data.genome.some(m => !level.allowedKinds.includes(m.kind))) {
        failure('В этом геноме есть типы клеток, недоступные в испытании.');
        return;
    } ask('Загрузить геном?', 'Он заменит текущий черновик редактора.', () => { setGenome(data.genome); setInitialMode(data.initialMode); setLibrary(false); setNotice('Геном загружен.'); }); }
    const toolLabels = tools;
    return <div className="laboratory" data-controls={touchControls ? "touch" : "mouse"}><header className="app-header"><button onClick={leave}>← Меню</button><h1>{title}</h1><button disabled={!sim.ready || sim.resetting} onClick={() => { sim.changeSpeed(0); setSaveKind('plate'); setName(title === 'Новый эксперимент' ? '' : title); }}>Сохранить эксперимент</button></header>
 <main className="lab-layout"><section className="microscope" aria-label="Микроскоп"><div className="canvas-actions"><div className="canvas-view-controls" role="group" aria-label="Масштаб поля"><button aria-label="Уменьшить масштаб" onClick={() => setZoomStep(v => v - 1)}>−</button><button aria-label="Увеличить масштаб" onClick={() => setZoomStep(v => v + 1)}>+</button><button onClick={() => setRecenter(v => v + 1)}>Всё поле</button></div><label><span className="sr-only">Показать слой</span><select value={layer} onChange={e => setLayer(e.target.value)}><option value="biome">Среда</option><option value="light">Свет</option><option value="food">Пища</option><option value="covered">Покрытая пища</option><option value="nitrates">Нитраты</option><option value="radiation">Радиация</option></select></label></div><div className="canvas-wrap" ref={canvasHost}><WorldView frame={sim.frame} tool={tool} layer={layer} selected={selected} backend={backend} onPoint={point} onView={setView} onError={() => failure('Не удалось показать чашку. Попробуйте перезагрузить страницу.')} recenter={recenter} zoomStep={zoomStep} onBackendFailure={(failed) => { if (failed === 'webgpu')
        setBackend('webgl');
    else
        failure('В этом браузере не удалось открыть изображение чашки.'); }}/>
 {chaosMode && <ChaosStatus frame={sim.frame}/>}
 {sim.frame?.chaos && layer === 'radiation' && <ChaosOverlay data={sim.frame.chaos} view={view} width={sim.frame.worldWidth ?? 6144} height={sim.frame.worldSize ?? 6144}/>}
 {!sim.ready && <div className="canvas-note" role="status">Открываем лабораторию…</div>}{sim.ready && !count && <div className="canvas-note">{chaosMode ? 'Клеток не осталось. Можно заселить чашку вручную или дождаться самозарождения, если оно включено.' : sim.inserted ? 'Клеток не осталось. Измените геном или начните заново.' : touchControls ? 'Коснитесь поля, чтобы поместить клетку.' : 'ЛКМ по полю — поместить клетку.'}</div>}
 </div>
 {touchControls && <div className="tool-bar" role="toolbar" aria-label="Инструменты">{toolLabels.map(([value, label]) => <button key={value} aria-pressed={tool === value} onClick={() => setTool(value)}>{label}</button>)}</div>}
 <p className="control-hint">{!touchControls && tool !== 'move' ? 'ЛКМ — поместить · ПКМ — выбрать · Зажатое колёсико — обзор · Колесо — масштаб' : tool === 'move' ? 'Нажмите на новое место для выбранной клетки.' : 'Касание — действие · Перетаскивание — обзор · Щипок — масштаб'}</p>
 <div className="transport"><button className="primary" disabled={!sim.ready || sim.resetting} onClick={() => sim.changeSpeed(sim.speed ? 0 : 1)}>{sim.speed ? 'Пауза' : 'Продолжить'}</button><button disabled={!sim.ready || sim.resetting || sim.speed !== 0} onClick={() => sim.send({ type: 'step' })}>Один шаг</button><label>Скорость<select aria-label="Скорость" value={sim.speed || 1} onChange={e => sim.changeSpeed(Number(e.target.value))}><option value="1">1×</option><option value="2">2×</option><option value="4">4×</option></select></label><div className="world-stats"><span>Время <strong>{((stats?.[0] ?? 0) / 30).toFixed(1)} с</strong></span><span>Клеток <strong>{count}</strong></span></div><button disabled={!sim.ready || sim.resetting} onClick={reset}>{level ? 'Повторить' : chaosMode ? 'Повторить мир' : 'Начать заново'}</button></div>
 </section><aside className="lab-sidebar"><nav className="tabs" aria-label="Панели лаборатории">{[...(level || session.kind === 'tutorial' ? [['objective', 'Задача']] : []), ['genome', 'Геном'], ['observe', 'Наблюдение'], ['environment', 'Среда']].map(([value, label]) => <button key={value} aria-current={tab === value ? 'page' : undefined} onClick={() => setTab(value)}>{label}</button>)}</nav><div className="sidebar-content">{session.kind === 'tutorial' && tab !== 'objective' && !tutorialDone && <p className="tutorial-context" role="status">{lesson.steps[tutorial].title}: {lesson.steps[tutorial].description}</p>}
 {tab === 'genome' && <GenomeEditor genome={genome} onChange={setGenome} initialMode={initialMode} onInitialModeChange={setInitialMode} allowedKinds={level?.allowedKinds} onSave={() => { setName(''); setSaveKind('genome'); }} onLoad={() => setLibrary(true)}/>}
 {tab === 'environment' && <>{sim.frame?.chaos && <ChaosPanel data={sim.frame.chaos} disabled={sim.resetting || !sim.ready} onConfigure={(enabled, rate, viable) => sim.send({ type: 'chaos-configure', enabled, rate, viable })} onNewWorld={seed => ask('Создать новый мир?', 'Текущие клетки и время будут сброшены. Геном редактора останется.', () => { sim.reset(sim.environment, { seed, genome: flatten(genome) }, newBounds()); setSelected(null); setRecenter(v => v + 1); }, readSettings().confirmReset)}/>}<EnvironmentPanel value={sim.environment} onChange={sim.configure} locked={!!level || sim.resetting || !sim.ready} lockedReason={level ? 'Условия заданы испытанием.' : undefined}/>{!level && <button className="danger" onClick={() => ask('Стерилизовать чашку?', 'Все клетки и их связи будут удалены. Среда и её возраст останутся.', () => { sim.send({ type: 'sterilize' }); setSelected(null); }, readSettings().confirmReset)}>Стерилизовать чашку</button>}</>}
 {tab === 'observe' && <>{sim.frame?.chaos && <ChaosObservation data={sim.frame.chaos}/>}<section className="panel-section"><h2>Наблюдение</h2><dl className="metrics"><div><dt>Рождений</dt><dd>{stats?.[2] ?? 0}</dd></div><div><dt>Погибло</dt><dd>{stats?.[3] ?? 0}</dd></div><div><dt>Связей</dt><dd>{(sim.frame?.links.length ?? 0) / 2}</dd></div></dl>{cell ? <><h3>Выбранная клетка</h3><dl className="cell-details">{[['Тип', TYPES[cell[4]]?.name], ['Слот', diagnostic?.genome[diagnostic.mode]?.name ?? '…'], ['Зеркальность', cell[11] ? 'Да' : 'Нет'], ['Масса', `${cell[5].toFixed(2)} нг`], ['Азот', nitrogen === undefined ? '…' : `${(nitrogen * 100).toFixed(1)}%`], ['Постоянных связей', connections], ['Возраст', `${cell[6].toFixed(1)} с`], ['Поколение', cell[7]], ['Изменений генома', cell[8]]].map(([k, v]) => <div key={k}><dt>{k}</dt><dd>{v}</dd></div>)}</dl><CellDetails values={details}/>{!level && !touchControls && <div className="button-row"><button onClick={() => setTool('move')}>Переместить</button><button onClick={() => sim.send({ type: 'boost', id: selected! })}>Подпитать</button><button className="danger" onClick={() => { sim.send({ type: 'remove', id: selected! }); setSelected(null); }}>Удалить</button></div>}<button onClick={copyGenome}>Скопировать геном в редактор</button>{!level && <button onClick={() => ask('Изменить выбранную клетку?', 'Геном редактора будет применён только к этой клетке.', () => { const values = flatten(genome); sim.names.current.set(Array.from(values).join(','), genome.map(mode => mode.name)); sim.send({ type: 'edit', id: selected!, mode: initialMode, genome: values }); setDiagnosticRevision(value => value + 1); setNotice('Геном применён к клетке.'); })}>Применить геном редактора</button>}</> : <p>{selected === null ? touchControls ? 'Выберите клетку инструментом «Диагностика».' : 'Нажмите ПКМ на клетку для диагностики.' : 'Клетка разделилась или погибла. Выберите потомка.'}</p>}</section></>}
 {tab === 'objective' && <ObjectivePanel level={level} status={sim.levelStatus} inserted={sim.inserted} completed={completed} help={help} onHelp={setHelp} onDemo={demo} lesson={lesson} step={tutorial} tutorialDone={tutorialDone} progressSaved={progressSaved} onRetryProgress={saveProgress}/>}
 </div></aside></main>
 {(sim.error || notice) && <div className={'toast ' + (sim.error ? 'error' : '')} role={sim.error ? 'alert' : 'status'}><span>{sim.error || notice}</span><button aria-label="Закрыть сообщение" onClick={() => { sim.setError(''); setNotice(''); }}>×</button></div>}
 {library && <Modal label="Банк геномов" onClose={() => setLibrary(false)}><Library kind="genome" onLoad={load} onClose={() => setLibrary(false)}/></Modal>}
 {saveKind && <Modal label={saveKind === 'genome' ? 'Сохранить геном' : 'Сохранить эксперимент'} onClose={() => { if (!busy)
        setSaveKind(null); }}><h2 id="save-title">{saveKind === 'genome' ? 'Сохранить геном' : 'Сохранить эксперимент'}</h2><form onSubmit={e => { e.preventDefault(); void save(); }}><label>Название<input autoFocus required maxLength={80} value={name} onChange={e => setName(e.target.value)}/></label><div className="button-row"><button type="button" disabled={busy} onClick={() => setSaveKind(null)}>Отмена</button><button className="primary" disabled={busy || !name.trim()}>{busy ? 'Сохраняем…' : 'Сохранить'}</button></div></form></Modal>}
 {confirm && <Modal label={confirm.title} onClose={() => setConfirm(null)}><h2 id="confirm-title">{confirm.title}</h2><p>{confirm.message}</p><div className="button-row"><button autoFocus onClick={() => setConfirm(null)}>Отмена</button><button className="primary" onClick={() => { const action = confirm.action; setConfirm(null); action(); }}>Подтвердить</button></div></Modal>}
 </div>;
}
