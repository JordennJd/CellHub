import { useEffect, useRef, useState } from 'react';
import { Modal } from './Modal';
import { TUTORIALS, listCompleted, listCompletedTutorials } from './challenges';
import { loadChallengeLevels, type LevelManifest } from './levels';
import { Library } from './Library';
import { Settings } from './Settings';
import type { LibraryKind, PlateData } from './storage';
import type { Session } from './Laboratory';
import './home.css';

type ModeIconKind = 'challenge' | 'sandbox' | 'chaos' | 'arena';

function ModeIcon({ kind }: { kind: ModeIconKind }) {
    return <svg viewBox="0 0 32 32" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
        {kind === 'challenge' && <><circle cx="16" cy="16" r="11"/><circle cx="16" cy="16" r="5"/><path d="M16 2v5M16 25v5M2 16h5M25 16h5"/></>}
        {kind === 'sandbox' && <><circle cx="16" cy="16" r="12"/><circle cx="12" cy="13" r="3"/><circle cx="20" cy="19" r="4"/><circle cx="11" cy="22" r="1" fill="currentColor" stroke="none"/></>}
        {kind === 'chaos' && <><path d="m11 10 11 4M11 10l4 14M22 14l-7 10"/><circle cx="10" cy="9" r="4"/><circle cx="24" cy="14" r="4"/><circle cx="15" cy="25" r="4"/></>}
        {kind === 'arena' && <><path d="M16 3v26"/><circle cx="10" cy="16" r="7"/><circle cx="22" cy="16" r="7"/></>}
    </svg>;
}

function ModeCard({ kind, title, description, onClick }: {
    kind: ModeIconKind;
    title: string;
    description: string;
    onClick?: () => void;
}) {
    return <button className="home-mode-card" disabled={!onClick} onClick={onClick}
        aria-labelledby={`mode-${kind}-title`} aria-describedby={`mode-${kind}-description mode-${kind}-status`}>
        <span className="home-mode-icon"><ModeIcon kind={kind}/></span>
        <span className="home-mode-title" id={`mode-${kind}-title`}>{title}</span>
        <span className="home-mode-description" id={`mode-${kind}-description`}>{description}</span>
        <span className="home-mode-status" id={`mode-${kind}-status`}>
            {onClick ? <><span>{kind === 'sandbox' ? 'Начать' : kind === 'arena' ? 'Войти' : 'Выбрать'}</span><span aria-hidden="true">→</span></> : 'Пока недоступно'}
        </span>
    </button>;
}

export function Home({ onStart }: { onStart: (s: Omit<Session, 'key'>) => void }) {
    const [page, setPage] = useState<'home' | 'challenges'>('home');
    const [library, setLibrary] = useState<LibraryKind | null>(null);
    const [settings, setSettings] = useState(false);
    const [levels, setLevels] = useState<LevelManifest[] | null>(null), [levelsError, setLevelsError] = useState('');
    const heading = useRef<HTMLHeadingElement>(null);
    const previousPage = useRef(page);
    const [progress] = useState(() => {
        try {
            return { completed: listCompleted(), tutorials: listCompletedTutorials(), error: '' };
        } catch (e) {
            return { completed: [] as string[], tutorials: [] as string[], error: String(e) };
        }
    });

    useEffect(() => {
        let active = true;
        loadChallengeLevels().then(value => { if (active) setLevels(value); })
            .catch(error => { if (active) setLevelsError(String(error)); });
        return () => { active = false; };
    }, []);

    useEffect(() => {
        if (previousPage.current === page) return;
        previousPage.current = page;
        heading.current?.focus({ preventScroll: true });
        window.scrollTo(0, 0);
    }, [page]);

    return <div className="home-shell">
        <header className="app-header">
            <a className="brand" href="#" onClick={e => { e.preventDefault(); setPage('home'); }}><span aria-hidden="true">◉</span><span>CellHub</span></a>
            <button onClick={() => setSettings(true)}>Настройки</button>
        </header>
        <main className="home-content">
            {page === 'home' ? <>
                <div className="home-heading"><h1 ref={heading} tabIndex={-1}>Маленькая клетка. Большая жизнь.</h1><p>Создайте организм и посмотрите, как он растёт.</p></div>
                <section className="home-start" aria-labelledby="start-title">
                    <div><span className="home-start-label">Начните здесь</span><h2 id="start-title">Первый эксперимент — с подсказками</h2>
                    <p>Поместите клетку, запустите время и увидите первое деление. Обучение подскажет каждое действие.</p>
                    </div>
                    <button className="primary" onClick={() => onStart({ kind: 'tutorial', tutorialId: (TUTORIALS.find(lesson => !progress.tutorials.includes(lesson.id)) ?? TUTORIALS[0]).id })}>{progress.tutorials.length ? 'Продолжить обучение' : 'Начать обучение'} →</button>
                </section>
                <h2 className="home-modes-heading">Или выберите свой путь</h2>
                <div className="home-mode-grid">
                    <ModeCard kind="challenge" title="Испытания" description="Пошаговое обучение и задачи с конкретной целью." onClick={() => setPage('challenges')}/>
                    <ModeCard kind="sandbox" title="Песочница" description="Создавайте клетки и меняйте среду без заданий и ограничений." onClick={() => onStart({ kind: 'sandbox' })}/>
                    <ModeCard kind="chaos" title="Хаос" description="Наблюдайте за готовым живым миром с мутациями и сменой дня и ночи." onClick={() => onStart({ kind: 'chaos' })}/>
                    <ModeCard kind="arena" title="Сетевая игра" description="Создайте комнату или присоединитесь к друзьям по коду." onClick={() => onStart({ kind: 'arena' })}/>
                </div>
                <div className="home-secondary-grid">
                    <section className="home-panel" aria-labelledby="home-open">
                        <div><h2 id="home-open">Открыть эксперимент</h2><p>Сохранённые чашки: продолжите или переиграйте опыт.</p></div>
                        <div className="home-panel-actions"><button onClick={() => setLibrary('plate')}>Открыть эксперимент</button></div>
                    </section>
                    <section className="home-panel" aria-labelledby="home-saved">
                        <div><h2 id="home-saved">Ваши работы</h2><p>Сохранённые геномы и организмы для новых опытов.</p></div>
                        <div className="home-panel-actions"><button onClick={() => setLibrary('genome')}>Банк геномов</button></div>
                    </section>
                </div>
            </> : <>
                <div className="page-heading"><button onClick={() => setPage('home')}>← Режимы</button>
                    <h1 ref={heading} tabIndex={-1}>Испытания</h1>
                    <p>Начните с уроков обучения, затем выберите любую задачу.</p>
                </div>
                {progress.error && <p role="alert">{progress.error}</p>}
                {levelsError && <p role="alert">Не удалось загрузить испытания: {levelsError}</p>}
                <div className="challenge-list">
                    {TUTORIALS.map((lesson, i) => {
                        const done = progress.tutorials.includes(lesson.id);
                        return <article className="challenge-card" key={`lesson-${lesson.id}`}>
                            <span className="challenge-number">{String(i + 1).padStart(2, '0')}</span>
                            <div><h2>{lesson.title}</h2><p>{lesson.description}</p>
                                {done && <p className="success">Пройдено</p>}
                                <button className="primary" onClick={() => onStart({ kind: 'tutorial', tutorialId: lesson.id })}>{done ? 'Повторить урок' : 'Начать урок'}</button>
                            </div>
                        </article>;
                    })}
                    {levels === null ? (!levelsError && <p role="status">Загружаем испытания…</p>) : levels.map((level, i) => {
                        const done = progress.completed.includes(level.id);
                        return <article className="challenge-card" key={level.id}>
                            <span className="challenge-number">{String(TUTORIALS.length + i + 1).padStart(2, '0')}</span>
                            <div><h2>{level.title}</h2><p>{level.description}</p>
                                {done && <p className="success">Пройдено</p>}
                                <button className="primary" onClick={() => onStart({ kind: 'challenge', level })}>{done ? 'Повторить' : 'Начать'}</button>
                            </div>
                        </article>;
                    })}
                </div>
            </>}
        </main>
        {library && <Modal label="Библиотека" onClose={() => setLibrary(null)}>
            <Library kind={library} onClose={() => setLibrary(null)} onLoad={entry => {
                if (entry.kind !== 'plate') {
                    onStart({ kind: 'sandbox', entry });
                    return;
                }
                const data = entry.data as PlateData;
                const level = data.challengeId ? levels?.find(value => value.id === data.challengeId) : undefined;
                if (level) {
                    onStart({ kind: 'challenge', level, entry });
                    return;
                }
                onStart({ kind: data.mode === 'chaos' ? 'chaos' : 'sandbox', challengeId: data.challengeId, entry });
            }}/>
        </Modal>}
        {settings && <Modal label="Настройки" onClose={() => setSettings(false)}><Settings onClose={() => setSettings(false)}/></Modal>}
    </div>;
}
