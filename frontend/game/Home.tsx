import { useState } from 'react';
import { Modal } from './Modal';
import { CHALLENGES, TUTORIALS, listCompleted, listCompletedTutorials } from './challenges';
import { Library } from './Library';
import { Settings } from './Settings';
import type { LibraryKind, PlateData } from './storage';
import type { Session } from './Laboratory';

export function Home({ onStart }: { onStart: (s: Omit<Session, 'key'>) => void }) {
    const [page, setPage] = useState<'home' | 'challenges' | 'tutorials'>('home');
    const [library, setLibrary] = useState<LibraryKind | null>(null);
    const [settings, setSettings] = useState(false);
    const [progress] = useState(() => {
        try {
            return { completed: listCompleted(), tutorials: listCompletedTutorials(), error: '' };
        } catch (e) {
            return { completed: [] as string[], tutorials: [] as string[], error: String(e) };
        }
    });

    return <div className="home-shell">
        <header className="app-header">
            <a className="brand" href="#" onClick={e => { e.preventDefault(); setPage('home'); }}>◉ <span>CellHub</span></a>
            <button onClick={() => setSettings(true)}>Настройки</button>
        </header>
        <main className="home-content">
            {page === 'home' ? <>
                <div className="page-heading"><p className="eyebrow">Лаборатория</p><h1>Выберите режим</h1></div>
                <div className="mode-grid">
                    <button className="mode-card" onClick={() => setPage('tutorials')}>
                        <span className="mode-icon">◎</span><h2>Обучение</h2>
                        <p>Освойте микроскоп, редактор генома и создание многоклеточного организма.</p>
                        <span className="card-action">Выбрать урок →</span>
                    </button>
                    <button className="mode-card" onClick={() => setPage('challenges')}>
                        <span className="mode-icon">◇</span><h2>Испытания</h2>
                        <p>Решайте задачи в заданной среде и открывайте следующие испытания.</p>
                        <span className="card-action">Выбрать испытание →</span>
                    </button>
                    <button className="mode-card" onClick={() => onStart({ kind: 'sandbox' })}>
                        <span className="mode-icon">◌</span><h2>Песочница</h2>
                        <p>Настраивайте среду, конструируйте организмы и наблюдайте за ними.</p>
                        <span className="card-action">Новый эксперимент →</span>
                    </button>
                </div>
                <section className="saved-section"><h2>Ваши работы</h2><div className="button-row">
                    <button onClick={() => setLibrary('plate')}>Открыть эксперимент</button>
                    <button onClick={() => setLibrary('genome')}>Банк геномов</button>
                </div></section>
                <section className="future-section" aria-label="Будущие режимы"><h2>Планируемые режимы</h2><div>
                    <p><strong>Хаос</strong><span>Биомы, паразиты и экосистемы. Запланировано.</span></p>
                    <p><strong>Мультиплей</strong><span>Совместные эксперименты. Запланировано.</span></p>
                </div></section>
            </> : <>
                <div className="page-heading"><button onClick={() => setPage('home')}>← Режимы</button>
                    <h1>{page === 'tutorials' ? 'Обучение' : 'Испытания'}</h1>
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
