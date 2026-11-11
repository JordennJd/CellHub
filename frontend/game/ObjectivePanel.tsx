import { CONDITION_LABELS, challengeBudget, type LevelManifest, type LevelStatus } from './levels';
import type { Tutorial } from './challenges';

type Props = {
    level?: LevelManifest;
    status: LevelStatus | null;
    inserted: number;
    completed: boolean;
    help: 'hint' | 'spoiler' | null;
    onHelp: (value: 'hint' | 'spoiler' | null) => void;
    onDemo: () => void;
    lesson: Tutorial;
    step: number;
    tutorialDone: boolean;
    progressSaved: boolean;
    onRetryProgress: () => void;
};

function budgetText(value: number): string {
    return Number.isFinite(value) ? String(value) : '∞';
}

function conditionValue(key: string, current: number, target: number): string {
    if (key === 'win-time')
        return `${current.toFixed(1)} / ${target} с`;
    return `${Number(current.toFixed(2))} / ${target}`;
}

export function ObjectivePanel({ level, status, inserted, completed, help, onHelp, onDemo, lesson, step, tutorialDone, progressSaved, onRetryProgress }: Props) {
    const retryProgress = <><p>Не удалось сохранить прогресс прохождения.</p><button onClick={onRetryProgress}>Повторить сохранение прогресса</button></>;
    if (level) return <section className="panel-section">
        <h2>{level.title}</h2><p>{level.description}</p>
        <dl className="cell-details">
            {(status?.conditions ?? []).map(condition => {
                const info = CONDITION_LABELS[condition.key];
                return <div key={condition.key}>
                    <dt>{info?.label ?? condition.key}{info?.unit ? ` (${info.unit})` : ''}</dt>
                    <dd className={condition.met ? 'success' : undefined}>
                        {conditionValue(condition.key, condition.current, condition.target)}
                    </dd>
                </div>;
            })}
            {status && <div><dt>Время</dt><dd>{status.elapsed.toFixed(1)} с{status.winTime > 0 ? ` из ${status.winTime.toFixed(0)} с` : ''}</dd></div>}
            {level.tools.includes('place') && <div><dt>Вставлено клеток</dt><dd>{inserted} / {budgetText(challengeBudget(level))}</dd></div>}
        </dl>
        {level.demo.length > 0 && <div className="button-row">
            <button onClick={onDemo} disabled={completed} aria-pressed={!!status?.demo}>
                {status?.demo ? 'Остановить демонстрацию' : 'Показать решение'}
            </button>
        </div>}
        {status?.demo && <p className="help-box" role="status">Идёт автоматическая демонстрация решения: {status.demoCursor} из {status.demoActions} действий.</p>}
        {completed && !status?.demo && <div className="success" role="status"><h3>Испытание пройдено</h3>{progressSaved ? <p>Результат сохранён. Можно продолжить наблюдение или вернуться к списку испытаний.</p> : retryProgress}</div>}
        <div className="button-row">
            <button aria-expanded={help === 'hint'} onClick={() => onHelp(help === 'hint' ? null : 'hint')} disabled={level.hints.length === 0}>Подсказка</button>
            <button aria-expanded={help === 'spoiler'} onClick={() => onHelp(help === 'spoiler' ? null : 'spoiler')} disabled={!level.solution && level.demo.length === 0}>Решение</button>
        </div>
        {help === 'hint' && level.hints.length > 0 && <div className="help-box">{level.hints.map((hint, i) => <p key={i}>{hint}</p>)}</div>}
        {help === 'spoiler' && <p className="help-box">{level.solution || 'Включите демонстрацию — игра сама покажет прохождение.'}</p>}
    </section>;

    const current = lesson.steps[step];
    return <section className="panel-section">
        <p className="eyebrow">Шаг {step + 1} из {lesson.steps.length}</p>
        <h2>{tutorialDone ? 'Урок завершён' : current.title}</h2>
        <p>{tutorialDone ? 'Геном сохранён. Можно продолжить эксперимент или выбрать следующий урок в меню.' : current.description}</p>
        {tutorialDone && !progressSaved && <div role="status">{retryProgress}</div>}
        <ol className="tutorial-list">{lesson.steps.map((item, index) => <li key={item.id} className={index < step || tutorialDone ? 'done' : index === step ? 'current' : ''}>{item.title}</li>)}</ol>
    </section>;
}
