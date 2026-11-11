import { Slider } from "../editor/Slider";
import { useEffect, useRef, useState } from 'react';
import { TYPES, type Frame } from '../model';
import type { ViewState } from '../world-view-types';
import './chaos.css';

export const CHAOS_BIOMES = [
    ['Солнечное мелководье', '#425b31', 1.20, 0.005, 0.35, 0.70, 1.0],
    ['Питательная заводь', '#294c3f', 0.45, 0.015, 1.15, 0.90, 1.4],
    ['Азотные источники', '#365344', 0.75, 0.010, 0.65, 1.50, 1.1],
    ['Бедное плато', '#4d4933', 0.85, 0.005, 0.20, 0.35, 0.6],
    ['Мутационное болото', '#4b3851', 0.65, 0.280, 0.70, 0.80, 1.0],
    ['Органическая дельта', '#41543a', 0.55, 0.020, 1.30, 0.55, 1.7],
    ['Сумеречное убежище', '#293d48', 0.35, 0.005, 0.45, 1.20, 0.8],
    ['Водорослевые сады', '#3d603a', 1.10, 0.035, 0.75, 1.00, 1.1],
    ['Минеральный хребет', '#4b5052', 0.70, 0.090, 0.90, 1.35, 1.3],
    ['Тихий оазис', '#355648', 0.90, 0.010, 0.55, 0.95, 0.9],
] as const;

type ChaosData = NonNullable<Frame['chaos']>;

export function ChaosStatus({ frame }: { frame: Frame | null }) {
    const info = frame?.chaos?.info;
    if (!info) return null;
    const phase = ((frame?.stats[0] ?? 0) / 30) % 60;
    const label = phase < 15 ? 'День → закат' : phase < 30 ? 'Закат → ночь' : phase < 45 ? 'Ночь → рассвет' : 'Рассвет → день';
    return <div className="chaos-status" role="status" aria-live="off">
        <span>{label} · свет {Math.round(info[1] * 100)}%</span>
        <span>{info[2] ? `Самозарождение через ${Math.min(info[5], info[6]).toFixed(1)} с` : 'Самозарождение выключено'}</span>

    </div>;
}

export function ChaosPanel({ data, disabled, onConfigure, onNewWorld }: {
    data: ChaosData;
    disabled: boolean;
    onConfigure: (enabled: boolean, rate: number, viable: boolean) => void;
    onNewWorld: (seed: number) => void;
}) {
    const info = data.info;
    const [seed, setSeed] = useState(String(info[0]));
    const [seedError, setSeedError] = useState('');
    useEffect(() => { setSeed(String(info[0])); setSeedError(''); }, [info[0]]);
    return <section className="panel-section chaos-settings"><h2>Мир хаоса</h2>
        <p>Плавный цикл суток — 60 секунд. Ночной свет — 40% дневного. Три радиационных облака движутся по чашке; их курс виден на слое «Радиация».</p>
        <fieldset disabled={disabled}>
            <label className="toggle"><input type="checkbox" checked={!!info[2]} onChange={e => onConfigure(e.target.checked, info[3], !!info[4])}/>Самозарождение</label>
            <label className="range"><span>Появлений в минуту<output>{info[3]}</output></span><Slider label="Появлений в минуту" min={1} max={60} step={1} value={info[3]} onChange={rate => onConfigure(!!info[2], rate, !!info[4])}/></label>
            <label className="toggle"><input type="checkbox" checked={!!info[4]} onChange={e => onConfigure(!!info[2], info[3], e.target.checked)}/>Предпочитать жизнеспособные слоты</label>
            <p>Новые клетки выбирают из {info[9]} слотов текущего редактора. В жизнеспособном варианте первая клетка выбирает фотоцит или фагоцит, если такой режим есть. Остальные клетки организма выбирают любой слот.</p>
            <p>По умолчанию первое появление через 6 секунд, затем каждые 6 секунд. Раз в 30 секунд появляется связанный организм из 3–5 клеток вместо одиночной клетки. Отключение останавливает оба вида появления.</p>
            <label>Seed мира<input value={seed} inputMode="numeric" maxLength={10} onChange={e => { setSeed(e.target.value); setSeedError(''); }}/></label>
            {seedError && <p role="alert">{seedError}</p>}
            <div className="button-row"><button onClick={() => {
                const value = Number(seed);
                if (!/^\d+$/.test(seed) || !Number.isInteger(value) || value < 0 || value > 0xffffffff) { setSeedError('Введите целое число от 0 до 4294967295.'); return; }
                onNewWorld(value);
            }}>Создать по seed</button><button onClick={() => onNewWorld(crypto.getRandomValues(new Uint32Array(1))[0])}>Новый мир</button></div>
        </fieldset>
        <details><summary>10 биомов и их свойства</summary><p>Базовые значения до настроек среды и суточного цикла. На границах свойства смешиваются; ресурсные пятна дополнительно меняют ёмкость.</p>
            <div className="chaos-table-wrap"><table className="chaos-biomes"><thead><tr><th>Биом</th><th>Свет</th><th>Фон</th><th>Пища</th><th>Азот</th><th>Рост пищи</th></tr></thead><tbody>{CHAOS_BIOMES.map(([name, color, light, radiation, food, nitrogen, regeneration]) => <tr key={name}><th><i style={{ background: color }}/>{name}</th><td>{light}</td><td>{radiation}</td><td>{food}</td><td>{nitrogen}</td><td>{regeneration}×</td></tr>)}</tbody></table></div>
        </details>
    </section>;
}

export function ChaosObservation({ data }: { data: ChaosData }) {
    const samples = Array.from({ length: data.history.length / 20 }, (_, i) => data.history.subarray(i * 20, i * 20 + 20));
    const start = samples[0]?.[0] ?? 0, end = samples.at(-1)?.[0] ?? 1;
    const max = Math.max(1, ...samples.map(s => s[1]));
    const points = (column: number) => samples.map(s => `${8 + (s[0] - start) / Math.max(1, end - start) * 304},${112 - s[column] / max * 100}`).join(' ');
    const last = samples.at(-1);
    const events = Array.from({ length: data.events.length / 3 }, (_, i) => data.events.subarray(i * 3, i * 3 + 3)).reverse();
    const describe = (kind: number, count: number) => [
        `Мир заселён: ${count} клеток`, 'Рассвет', 'Закат', `Появилась клетка`,
        `Появился организм: ${count} клеток`, `Стерилизация: удалено ${count} клеток`,
        count ? 'Самозарождение включено / настроено' : 'Самозарождение выключено',
    ][kind] ?? 'Событие';
    return <section className="panel-section chaos-observation"><h2>Экосистема</h2>
        <p>Самозарождение: {data.info[7]} клеток, в том числе {data.info[8]} организмов. Стартовая популяция учитывается отдельно.</p>
        <figure><figcaption>Популяция · последние 5 минут</figcaption>
            <svg viewBox="0 0 320 136" role="img" aria-label={`Динамика популяции, сейчас ${last?.[1] ?? 0} клеток, максимум на графике ${max}`}>
                <path d="M8 12V112H312" fill="none" stroke="currentColor" opacity=".3"/>
                {TYPES.map((type, i) => <polyline key={type.latin} points={points(i + 2)} fill="none" stroke={`#${type.color.toString(16).padStart(6, '0')}`} strokeWidth="1.1" opacity=".7"/>)}
                <polyline points={points(1)} fill="none" stroke="var(--accent)" strokeWidth="2"/>
                <text x="8" y="132">{start.toFixed(0)} с</text><text x="275" y="132">{end.toFixed(0)} с</text><text x="12" y="18">{max}</text>
            </svg>
        </figure>
        <details><summary>Состав по типам</summary><ul className="chaos-type-list">{TYPES.map((type, i) => <li key={type.latin}><i style={{ background: `#${type.color.toString(16).padStart(6, '0')}` }}/>{type.name}<strong>{last?.[i + 2] ?? 0}</strong></li>)}</ul></details>
        <h3>Журнал мира</h3><ol className="chaos-journal">{events.map((event, i) => <li key={`${event[0]}-${i}`}><time>{(event[0] / 30).toFixed(1)} с</time><span>{describe(event[1], event[2])}</span></li>)}</ol>
    </section>;
}

export function ChaosOverlay({ data, view, width, height }: { data: ChaosData; view: ViewState; width: number; height: number }) {
    const host = useRef<SVGSVGElement>(null);
    const [size, setSize] = useState({ width: 1, height: 1 });
    useEffect(() => {
        const observer = new ResizeObserver(entries => {
            const box = entries[0]?.contentRect;
            if (box) setSize({ width: box.width, height: box.height });
        });
        observer.observe(host.current!);
        return () => observer.disconnect();
    }, []);
    const x = (v: number) => (v - view.x) * view.zoom + size.width / 2;
    const y = (v: number) => (v - view.y) * view.zoom + size.height / 2;
    return <svg className="chaos-overlay" ref={host} aria-label="Направления движения радиационных облаков" role="img">
        <defs><clipPath id="chaos-dish-clip"><rect x={x(0)} y={y(0)} width={width * view.zoom} height={height * view.zoom}/></clipPath></defs>
        <g clipPath="url(#chaos-dish-clip)">{[0, 1, 2].map(i => {
            const cloud = data.info.subarray(10 + i * 6, 16 + i * 6);
            const cx = x(cloud[0]), cy = y(cloud[1]);
            const angle = Math.atan2(cloud[5], cloud[4]) * 180 / Math.PI;
            return <g key={i}><circle cx={cx} cy={cy} r={cloud[2] * view.zoom} fill="none" strokeDasharray="5 6"/>
                <g transform={`translate(${cx} ${cy}) rotate(${angle})`}><path d="M-14 0H20M12-7L20 0L12 7"/></g><text x={cx + 10} y={cy - 12}>Облако {i + 1}</text></g>;
        })}</g>
    </svg>;
}
