import { Modal } from './Modal';
import { useEffect, useRef, useState } from 'react';
import { WorldView, type ViewState } from '../WorldView';
import { initialBackend, type BackendPreference } from '../backend';
import { defaultGenome, flatten, inflate, TYPES, STRIDE, DEFAULT_ENVIRONMENT, type Mode } from '../model';
import { GenomeEditor } from '../editor/GenomeEditor';
import { useSimulation } from './useSimulation';
import { EnvironmentPanel } from './EnvironmentPanel';
import { CHALLENGES, completeChallenge, TUTORIALS, completeTutorial, evaluateChallenge, evaluateTutorialStep } from './challenges';
import { ObjectivePanel } from './ObjectivePanel';
import { Library } from './Library';
import { saveEntry, type LibraryEntry, type GenomeData, type PlateData } from './storage';
import { readSettings } from './Settings';
export type Session = {
    key: string;
    kind: 'sandbox' | 'challenge' | 'tutorial';
    challengeId?: string;
    tutorialId?: string;
    entry?: LibraryEntry;
};
export function Laboratory({ session, onExit }: {
    session: Session;
    onExit: () => void;
}) {
    const sim = useSimulation(), challenge = CHALLENGES.find(c => c.id === session.challengeId);
    const lesson = TUTORIALS.find(t => t.id === session.tutorialId) ?? TUTORIALS[0];
    const [genome, setGenome] = useState<Mode[]>(() => session.entry ? (session.entry.data as GenomeData).genome : structuredClone(challenge?.genome ?? (session.kind === 'tutorial' ? lesson.genome : defaultGenome())));
    const [initialMode, setInitialMode] = useState(session.entry ? (session.entry.data as GenomeData).initialMode : 0);
    const [tab, setTab] = useState(challenge || session.kind === 'tutorial' ? 'objective' : 'genome');
    const [tool, setTool] = useState('place'), [selected, setSelected] = useState<number | null>(null), [layer, setLayer] = useState('biome');
    const [backend, setBackend] = useState<BackendPreference>(initialBackend), [recenter, setRecenter] = useState(0), [zoomStep, setZoomStep] = useState(0), [view, setView] = useState<ViewState>({ x: 0, y: 0, zoom: 1 });
    const [library, setLibrary] = useState(false), [saveKind, setSaveKind] = useState<'genome' | 'plate' | null>(null), [name, setName] = useState(''), [busy, setBusy] = useState(false), [notice, setNotice] = useState('');
    const [completed, setCompleted] = useState(false), [help, setHelp] = useState<'hint' | 'spoiler' | null>(null), [tutorial, setTutorial] = useState(0), [tutorialDone, setTutorialDone] = useState(false);
    const [savedGenome, setSavedGenome] = useState(false);
    const [progressSaved, setProgressSaved] = useState(false);
    const [diagnostic, setDiagnostic] = useState<{ genome: Mode[]; mode: number } | null>(null);
    const [diagnosticRevision, setDiagnosticRevision] = useState(0);
    const [title, setTitle] = useState(session.entry?.kind === 'plate' ? session.entry.name : challenge?.title ?? (session.kind === 'tutorial' ? lesson.title : 'Новый эксперимент'));
    const [confirm, setConfirm] = useState<{
        title: string;
        message: string;
        action: () => void;
    } | null>(null);
    const [plateId, setPlateId] = useState(session.entry?.kind === 'plate' ? session.entry.id : undefined);
    const initialized = useRef(false), victory = useRef(false), copyPending = useRef(false);
    const stats = sim.frame?.stats, count = stats?.[1] ?? 0;
    const objective = challenge ? evaluateChallenge(challenge, sim.frame, sim.inserted) : null;
    let cell: Float32Array | undefined;
    if (sim.frame && selected !== null)
        for (let i = 0; i < sim.frame.cells.length; i += STRIDE)
            if (sim.frame.cells[i] === selected) {
                cell = sim.frame.cells.subarray(i, i + STRIDE);
                break;
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
        else
            sim.reset({ ...DEFAULT_ENVIRONMENT, ...(challenge?.environment ?? (session.kind === 'tutorial' ? lesson.environment : {})) });
    }, [sim.ready]);
    useEffect(() => { if (sim.resetting || !challenge || victory.current || !objective?.complete)
        return; victory.current = true; sim.changeSpeed(0); setCompleted(true); saveProgress(); }, [objective?.complete, sim.resetting]);
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
    }, [selected, cell?.[4], cell?.[8], sim.resetting, diagnosticRevision]);
    useEffect(() => { const hidden = () => { if (document.hidden && readSettings().pauseWhenHidden)
        sim.changeSpeed(0); }; const before = (e: BeforeUnloadEvent) => { if (readSettings().confirmExit) {
        e.preventDefault();
        e.returnValue = '';
    } }; document.addEventListener('visibilitychange', hidden); window.addEventListener('beforeunload', before); return () => { document.removeEventListener('visibilitychange', hidden); window.removeEventListener('beforeunload', before); }; }, []);
    function failure(message: unknown) { sim.setError(message instanceof Error ? message.message : String(message)); }
    function saveProgress() {
        try {
            if (challenge) completeChallenge(challenge.id);
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
    function reset() { ask('Начать заново?', 'Текущие клетки и время будут сброшены. Геном редактора останется.', () => { sim.reset(challenge?.environment ?? sim.environment); setSelected(null); victory.current = false; setCompleted(false); setTutorial(0); setTutorialDone(false); setSavedGenome(false); setProgressSaved(false); }, readSettings().confirmReset); }
    function point(x: number, y: number, id: number | null, action: 'place' | 'select') {
        if (!sim.ready || sim.resetting || busy)
            return;
        if (tool === 'move' && selected !== null) {
            sim.send({ type: 'move', id: selected, x, y });
            setTool('select');
            return;
        }
        if (action === 'place') {
            if (challenge && genome.some(m => !challenge.allowedKinds.includes(m.kind))) {
                failure('Этот тип клетки недоступен в испытании.');
                return;
            }
            sim.spawn(x, y, initialMode, genome, challenge?.budget);
            return;
        }
        if (id !== null && tool === 'remove') {
            sim.send({ type: 'remove', id });
            if (selected === id)
                setSelected(null);
            return;
        }
        if (id !== null && tool === 'boost') {
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
            const data = saveKind === 'genome' ? { genome, initialMode } : { state: await sim.exportWorld(), genome, initialMode, environment: sim.environment, inserted: sim.inserted, genomeNames: Object.fromEntries(sim.names.current), ...(challenge ? { challengeId: challenge.id } : {}) };
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
    function load(entry: LibraryEntry) { const data = entry.data as GenomeData; if (challenge && data.genome.some(m => !challenge.allowedKinds.includes(m.kind))) {
        failure('В этом геноме есть типы клеток, недоступные в испытании.');
        return;
    } ask('Загрузить геном?', 'Он заменит текущий черновик редактора.', () => { setGenome(data.genome); setInitialMode(data.initialMode); setLibrary(false); setNotice('Геном загружен.'); }); }
    const toolLabels: [
        string,
        string
    ][] = [['place', 'Поместить'], ['select', 'Диагностика'], ['pan', 'Обзор'], ...(!challenge ? [['move', 'Перенести'], ['boost', 'Подпитать'], ['remove', 'Удалить']] as [
            string,
            string
        ][] : [])];
    return <div className="laboratory"><header className="app-header"><button onClick={leave}>← Меню</button><h1>{title}</h1><button disabled={!sim.ready || sim.resetting} onClick={() => { sim.changeSpeed(0); setSaveKind('plate'); setName(title === 'Новый эксперимент' ? '' : title); }}>Сохранить эксперимент</button></header>
 <main className="lab-layout"><section className="microscope" aria-label="Микроскоп"><div className="canvas-actions"><div className="canvas-view-controls" role="group" aria-label="Масштаб чашки"><button aria-label="Уменьшить масштаб" onClick={() => setZoomStep(v => v - 1)}>−</button><button aria-label="Увеличить масштаб" onClick={() => setZoomStep(v => v + 1)}>+</button><button onClick={() => setRecenter(v => v + 1)}>Вся чашка</button></div><label><span className="sr-only">Показать слой</span><select value={layer} onChange={e => setLayer(e.target.value)}><option value="biome">Среда</option><option value="light">Свет</option><option value="food">Пища</option><option value="radiation">Радиация</option></select></label></div><div className="canvas-wrap"><WorldView frame={sim.frame} tool={tool} layer={layer} selected={selected} backend={backend} onPoint={point} onView={setView} onError={() => failure('Не удалось показать чашку. Попробуйте перезагрузить страницу.')} recenter={recenter} zoomStep={zoomStep} onBackendFailure={(failed) => { if (failed === 'webgpu')
        setBackend('webgl');
    else
        failure('В этом браузере не удалось открыть изображение чашки.'); }}/>
 {!sim.ready && <div className="canvas-note" role="status">Открываем лабораторию…</div>}{sim.ready && !count && <div className="canvas-note">{sim.inserted ? 'Клеток не осталось. Измените геном или начните заново.' : 'Выберите «Поместить» и нажмите на чашку.'}</div>}
 </div>
 <div className="tool-bar" role="toolbar" aria-label="Инструменты">{toolLabels.map(([value, label]) => <button key={value} aria-pressed={tool === value} onClick={() => setTool(value)}>{label}</button>)}</div>
 <p className="control-hint">{tool === 'move' ? (selected === null ? 'Сначала выберите клетку диагностикой.' : 'Нажмите на новое место для выбранной клетки.') : tool === 'pan' ? 'Перетаскивайте чашку для обзора.' : tool === 'place' ? 'Нажатие — поместить клетку.' : tool === 'boost' ? 'Нажмите на клетку, чтобы увеличить её запас энергии.' : tool === 'remove' ? 'Нажмите на клетку, которую нужно удалить.' : 'Нажмите на клетку для диагностики.'} Колесо или щипок — масштаб.</p>
 <div className="transport"><button className="primary" disabled={!sim.ready || sim.resetting} onClick={() => sim.changeSpeed(sim.speed ? 0 : 1)}>{sim.speed ? 'Пауза' : 'Продолжить'}</button><button disabled={!sim.ready || sim.resetting || sim.speed !== 0} onClick={() => sim.send({ type: 'step' })}>Один шаг</button><label>Скорость<select aria-label="Скорость" value={sim.speed || 1} onChange={e => sim.changeSpeed(Number(e.target.value))}><option value="1">1×</option><option value="2">2×</option><option value="4">4×</option></select></label><div className="world-stats"><span>Время <strong>{((stats?.[0] ?? 0) / 30).toFixed(1)} с</strong></span><span>Клеток <strong>{count}</strong></span></div><button disabled={!sim.ready || sim.resetting} onClick={reset}>{challenge ? 'Повторить' : 'Начать заново'}</button></div>
 </section><aside className="lab-sidebar"><nav className="tabs" aria-label="Панели лаборатории">{[...(challenge || session.kind === 'tutorial' ? [['objective', 'Задача']] : []), ['genome', 'Геном'], ['observe', 'Наблюдение'], ['environment', 'Среда']].map(([value, label]) => <button key={value} aria-current={tab === value ? 'page' : undefined} onClick={() => setTab(value)}>{label}</button>)}</nav><div className="sidebar-content">{session.kind === 'tutorial' && tab !== 'objective' && !tutorialDone && <p className="tutorial-context" role="status">{lesson.steps[tutorial].title}: {lesson.steps[tutorial].description}</p>}
 {tab === 'genome' && <GenomeEditor genome={genome} onChange={setGenome} initialMode={initialMode} onInitialModeChange={setInitialMode} allowedKinds={challenge?.allowedKinds} onSave={() => { setName(''); setSaveKind('genome'); }} onLoad={() => setLibrary(true)}/>}
 {tab === 'environment' && <><EnvironmentPanel value={sim.environment} onChange={sim.configure} locked={!!challenge || sim.resetting || !sim.ready} lockedReason={challenge ? 'Условия заданы испытанием.' : undefined}/>{!challenge && <button className="danger" onClick={() => ask('Стерилизовать чашку?', 'Все клетки и их связи будут удалены. Среда и её возраст останутся.', () => { sim.send({ type: 'sterilize' }); setSelected(null); }, readSettings().confirmReset)}>Стерилизовать чашку</button>}</>}
 {tab === 'observe' && <section className="panel-section"><h2>Наблюдение</h2><dl className="metrics"><div><dt>Рождений</dt><dd>{stats?.[2] ?? 0}</dd></div><div><dt>Погибло</dt><dd>{stats?.[3] ?? 0}</dd></div><div><dt>Связей</dt><dd>{(sim.frame?.links.length ?? 0) / 2}</dd></div></dl>{cell ? <><h3>Выбранная клетка</h3><dl className="cell-details">{[['Тип', TYPES[cell[4]]?.name], ['Режим', diagnostic?.genome[diagnostic.mode]?.name ?? '…'], ['Зеркальность', cell[11] ? 'Да' : 'Нет'], ['Масса', `${cell[5].toFixed(2)} нг`], ['Возраст', `${cell[6].toFixed(1)} с`], ['Поколение', cell[7]], ['Изменений генома', cell[8]]].map(([k, v]) => <div key={k}><dt>{k}</dt><dd>{v}</dd></div>)}</dl><button onClick={copyGenome}>Скопировать геном в редактор</button>{!challenge && <button onClick={() => ask('Изменить выбранную клетку?', 'Геном редактора будет применён только к этой клетке.', () => { const values = flatten(genome); sim.names.current.set(Array.from(values).join(','), genome.map(mode => mode.name)); sim.send({ type: 'edit', id: selected!, mode: initialMode, genome: values }); setDiagnosticRevision(value => value + 1); setNotice('Геном применён к клетке.'); })}>Применить геном редактора</button>}</> : <p>{selected === null ? 'Выберите клетку инструментом «Диагностика».' : 'Клетка разделилась или погибла. Выберите потомка.'}</p>}</section>}
 {tab === 'objective' && <ObjectivePanel challenge={challenge} objectives={objective?.objectives ?? []} inserted={sim.inserted} completed={completed} help={help} onHelp={setHelp} lesson={lesson} step={tutorial} tutorialDone={tutorialDone} progressSaved={progressSaved} onRetryProgress={saveProgress}/>}
 </div></aside></main>
 {(sim.error || notice) && <div className={'toast ' + (sim.error ? 'error' : '')} role={sim.error ? 'alert' : 'status'}><span>{sim.error || notice}</span><button aria-label="Закрыть сообщение" onClick={() => { sim.setError(''); setNotice(''); }}>×</button></div>}
 {library && <Modal label="Банк геномов" onClose={() => setLibrary(false)}><Library kind="genome" onLoad={load} onClose={() => setLibrary(false)}/></Modal>}
 {saveKind && <Modal label={saveKind === 'genome' ? 'Сохранить геном' : 'Сохранить эксперимент'} onClose={() => { if (!busy)
        setSaveKind(null); }}><h2 id="save-title">{saveKind === 'genome' ? 'Сохранить геном' : 'Сохранить эксперимент'}</h2><form onSubmit={e => { e.preventDefault(); void save(); }}><label>Название<input autoFocus required maxLength={80} value={name} onChange={e => setName(e.target.value)}/></label><div className="button-row"><button type="button" disabled={busy} onClick={() => setSaveKind(null)}>Отмена</button><button className="primary" disabled={busy || !name.trim()}>{busy ? 'Сохраняем…' : 'Сохранить'}</button></div></form></Modal>}
 {confirm && <Modal label={confirm.title} onClose={() => setConfirm(null)}><h2 id="confirm-title">{confirm.title}</h2><p>{confirm.message}</p><div className="button-row"><button autoFocus onClick={() => setConfirm(null)}>Отмена</button><button className="primary" onClick={() => { const action = confirm.action; setConfirm(null); action(); }}>Подтвердить</button></div></Modal>}
 </div>;
}
