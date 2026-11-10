import type { Challenge, ObjectiveProgress, Tutorial } from './challenges';

type Props = {
    challenge?: Challenge;
    objectives: ObjectiveProgress[];
    inserted: number;
    completed: boolean;
    help: 'hint' | 'spoiler' | null;
    onHelp: (value: 'hint' | 'spoiler' | null) => void;
    lesson: Tutorial;
    step: number;
    tutorialDone: boolean;
    progressSaved: boolean;
    onRetryProgress: () => void;
};

export function ObjectivePanel({ challenge, objectives, inserted, completed, help, onHelp, lesson, step, tutorialDone, progressSaved, onRetryProgress }: Props) {
    const retryProgress = <><p>Не удалось сохранить прогресс прохождения.</p><button onClick={onRetryProgress}>Повторить сохранение прогресса</button></>;
    if (challenge) return <section className="panel-section">
        <h2>{challenge.title}</h2><p>{challenge.description}</p>
        <dl className="cell-details">
            {objectives.map(objective => <div key={objective.id}>
                <dt>{objective.label}</dt>
                <dd className={objective.complete ? 'success' : undefined}>
                    {Number(objective.current.toFixed(2))} / {objective.target}
                </dd>
            </div>)}
            <div><dt>Вставлено клеток</dt><dd>{inserted} / {challenge.budget}</dd></div>
        </dl>
        {completed && <div className="success" role="status"><h3>Испытание пройдено</h3>{progressSaved ? <p>Результат сохранён. Можно продолжить наблюдение или вернуться к списку испытаний.</p> : retryProgress}</div>}
        <div className="button-row">
            <button aria-expanded={help === 'hint'} onClick={() => onHelp(help === 'hint' ? null : 'hint')}>Подсказка</button>
            <button aria-expanded={help === 'spoiler'} onClick={() => onHelp(help === 'spoiler' ? null : 'spoiler')}>Решение</button>
        </div>
        {help && <p className="help-box">{challenge[help]}</p>}
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
