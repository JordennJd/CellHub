import type { TutorialStep } from './challenges';

type Props = {
    lessonStep?: TutorialStep;
    step: number;
    total: number;
    done: boolean;
    count: number;
    inserted: number;
    running: boolean;
    ready: boolean;
    onPlace: () => void;
    onRun: () => void;
    onInspect: () => void;
    onEdit: () => void;
    onSave: () => void;
    onObjective: () => void;
    onExit: () => void;
    challenge: boolean;
    chaos: boolean;
};

export function NextAction(p: Props) {
    let title: string, description: string, label: string, action: () => void;
    if (p.done) {
        title = 'Урок пройден';
        description = 'Вернитесь в меню, чтобы выбрать следующий урок или начать собственный эксперимент.';
        label = 'Выбрать следующий урок'; action = p.onExit;
    } else if (p.lessonStep) {
        title = p.lessonStep.title; description = p.lessonStep.description;
        const requirement = p.lessonStep.requirement;
        if (requirement === 'place') { label = 'Выбрать размещение'; action = p.onPlace; }
        else if (requirement === 'run' || requirement === 'divide' || requirement === 'livingBond') {
            label = p.running ? 'Открыть задачу' : 'Запустить время'; action = p.running ? p.onObjective : p.onRun;
        } else if (requirement === 'inspect') { label = 'Выбрать клетку'; action = p.onInspect; }
        else if (requirement === 'save') { label = 'Сохранить геном'; action = p.onSave; }
        else { label = 'Открыть редактор'; action = p.onEdit; }
    } else if (p.challenge) {
        title = 'Выполните условия испытания'; description = 'Откройте задачу: там указаны цель, ограничения и подсказки.';
        label = 'Показать задачу'; action = p.onObjective;
    } else if (!p.count && !p.chaos) {
        title = p.inserted ? 'Попробуйте другую конструкцию' : 'Поместите первую клетку';
        description = p.inserted ? 'Клетки погибли. Измените геном или условия среды и поместите новую клетку.' : 'Готовый геном уже выбран. Нажмите на поле, чтобы добавить клетку. Затем запустите время.';
        label = 'Выбрать размещение'; action = p.onPlace;
    } else if (!p.running) {
        title = 'Время на паузе'; description = 'Нажмите «Продолжить», чтобы клетки росли и делились. «Один шаг» продвигает опыт на один такт.';
        label = 'Продолжить'; action = p.onRun;
    } else {
        title = 'Наблюдайте за жизнью клеток'; description = 'Выберите клетку, чтобы узнать её тип, массу и возраст. Для изменения конструкции откройте «Геном».';
        label = 'Выбрать клетку'; action = p.onInspect;
    }
    return <section className="next-action" aria-label="Следующее действие">
        <div><span className="next-action-label">{p.lessonStep && !p.done ? `Обучение · шаг ${p.step + 1} из ${p.total}` : 'Что делать дальше'}</span>
            <h2>{title}</h2><p>{description}</p></div>
        <button disabled={!p.ready} onClick={action}>{label} →</button>
    </section>;
}
