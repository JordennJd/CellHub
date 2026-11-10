import { useEffect, useRef, useState } from 'react';
import { Modal } from './Modal';
import { CHALLENGES, TUTORIALS, listCompleted, listCompletedTutorials } from './challenges';
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
    const [page, setPage] = useState<'home' | 'challenges' | 'tutorials'>('home');
    const [library, setLibrary] = useState<LibraryKind | null>(null);
    const [settings, setSettings] = useState(false);
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
                <div className="home-heading"><h1 ref={heading} tabIndex={-1}>Выберите режим</h1></div>
                <div className="home-mode-grid">
                    <ModeCard kind="challenge" title="Испытания" description="Задачи с заданными условиями и целями." onClick={() => setPage('challenges')}/>
                    <ModeCard kind="sandbox" title="Песочница" description="Своя среда и свободные эксперименты." onClick={() => onStart({ kind: 'sandbox' })}/>
                    <ModeCard kind="chaos" title="Хаос" description="Биомы, паразиты и сложные экосистемы."/>
                    <ModeCard kind="arena" title="Арена" description="Соревнования организмов с другими игроками." onClick={() => onStart({ kind: 'arena' })}/>
                </div>
                <div className="home-secondary-grid">
                    <section className="home-panel" aria-labelledby="home-tutorials">
                        <div><h2 id="home-tutorials">Обучение</h2><p>Микроскоп, геном и первые организмы — пошагово.</p></div>
                        <div className="home-panel-actions"><button onClick={() => setPage('tutorials')}>Выбрать урок</button></div>
                    </section>
                    <section className="home-panel" aria-labelledby="home-saved">
                        <div><h2 id="home-saved">Ваши работы</h2><p>Сохранённые эксперименты и геномы.</p></div>
                        <div className="home-panel-actions">
                            <button onClick={() => setLibrary('plate')}>Открыть эксперимент</button>
                            <button onClick={() => setLibrary('genome')}>Банк геномов</button>
                        </div>
                    </section>
                </div>
            </> : <>
                <div className="page-heading"><button onClick={() => setPage('home')}>← Режимы</button>
                    <h1 ref={heading} tabIndex={-1}>{page === 'tutorials' ? 'Обучение' : 'Испытания'}</h1>
                    <p>{page === 'tutorials' ? 'Уроки можно проходить в любом порядке.' : 'Каждая задача открывает следующую.'}</p>
                </div>
                {progress.error && <p role="alert">{progress.error}</p>}
                <div className="challenge-list">
                    {page === 'tutorials' ? TUTORIALS.map((lesson, i) => <article className="challenge-card" key={lesson.id}>
                        <span className="challenge-number">{String(i + 1).padStart(2, '0')}</span>
                        <div><h2>{lesson.title}</h2><p>{lesson.description}</p>
                            {progress.tutorials.includes(lesson.id) && <p className="success">Пройдено</p>}
                            <button className="primary" onClick={() => onStart({ kind: 'tutorial', tutorialId: lesson.id })}>Начать урок</button>
                        </div>
                    </article>) : CHALLENGES.map((challenge, i) => {
                        const locked = challenge.requires && !progress.completed.includes(challenge.requires);
                        const done = progress.completed.includes(challenge.id);
                        return <article className="challenge-card" key={challenge.id}>
                            <span className="challenge-number">{String(i + 1).padStart(2, '0')}</span>
                            <div><h2>{challenge.title}</h2><p>{challenge.description}</p>
                                {done && <p className="success">Пройдено</p>}
                                {locked ? <p>Сначала пройдите «{CHALLENGES.find(v => v.id === challenge.requires)?.title}».</p>
                                    : <button className="primary" onClick={() => onStart({ kind: 'challenge', challengeId: challenge.id })}>{done ? 'Повторить' : 'Начать'}</button>}
                            </div>
                        </article>;
                    })}
                </div>
            </>}
        </main>
        {library && <Modal label="Библиотека" onClose={() => setLibrary(null)}>
            <Library kind={library} onClose={() => setLibrary(null)} onLoad={entry => onStart(entry.kind === 'plate'
                ? { kind: (entry.data as PlateData).challengeId ? 'challenge' : 'sandbox', challengeId: (entry.data as PlateData).challengeId, entry }
                : { kind: 'sandbox', entry })}/>
        </Modal>}
        {settings && <Modal label="Настройки" onClose={() => setSettings(false)}><Settings onClose={() => setSettings(false)}/></Modal>}
    </div>;
}
