import { Slider } from "../editor/Slider";
import type { Environment } from '../model';
export function EnvironmentPanel({ value, onChange, locked = false, lockedReason }: {
    value: Environment;
    onChange: (v: Environment) => void;
    locked?: boolean;
    lockedReason?: string;
}) {
    return <section className="panel-section"><h2>Среда</h2>{locked && lockedReason && <p>{lockedReason}</p>}<fieldset disabled={locked}>
 {([['light', 'Свет', 0, 5], ['food', 'Питание', 0, 5], ['nitrogen', 'Нитраты', 0, 5], ['radiation', 'Радиация', 0, 5], ['viscosity', 'Вязкость', 0.1, 5]] as const).map(([key, label, min, max]) => <label className="range" key={key}><span>{label}<output>{value[key].toFixed(1)}×</output></span><Slider label={label} min={min} max={max} step={0.1} value={value[key]} onChange={next => onChange({ ...value, [key]: next })}/></label>)}
 <label className="toggle"><input type="checkbox" checked={value.aging} onChange={e => onChange({ ...value, aging: e.target.checked })}/>Старение клеток</label>
 <label className="toggle"><input type="checkbox" checked={value.killAtEdge} onChange={e => onChange({ ...value, killAtEdge: e.target.checked })}/>Гибель у края</label></fieldset>
 <fieldset disabled={locked}>
 <label className="range"><span>Гравитация<output>{value.gravity.toFixed(1)}</output></span><Slider label="Гравитация" min={0} max={1} step={0.1} value={value.gravity} onChange={gravity => onChange({ ...value, gravity })}/></label>
 <label className="range"><span>Плотность среды<output>{value.density.toFixed(1)}</output></span><Slider label="Плотность среды" min={-3.3} max={3.3} step={0.1} value={value.density} onChange={density => onChange({ ...value, density })}/></label>
 <p>При включённой гравитации: плотность 0 — равновесие; ниже — клетки оседают, выше — всплывают.</p>
 <label className="toggle"><input type="checkbox" checked={value.onlyPointMutations} onChange={e => onChange({ ...value, onlyPointMutations: e.target.checked })}/>Только точечные мутации</label>
 <p>Точечные мутации меняют параметры генома, сохраняя типы клеток и переходы между режимами.</p>
 </fieldset>
 <details><summary>Как среда влияет на клетки</summary><p>Фотоциты получают энергию от света, фагоциты — из пищи. Радиация меняет геном. Вязкость замедляет движение. Свет, питание, радиация и вязкость показаны относительно исходной среды.</p></details></section>;
}
