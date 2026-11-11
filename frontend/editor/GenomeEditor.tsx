import { useState } from "react";
import { defaultGenome, modeColor, multicellularGenome, newMode, removeMode, MAX_MODES, POPULATION_LIMIT, TYPES, type Mode, type NumericModeKey } from "../model";
import { GenomePreview } from "./GenomePreview";
import { CellSettings, Setting } from "./CellSettings";
import "./editor.css";

export type GenomeEditorProps = {
  genome: Mode[];
  onChange: (genome: Mode[]) => void;
  initialMode: number;
  onInitialModeChange: (mode: number) => void;
  onSave: () => void;
  onLoad: () => void;
  allowedKinds?: number[];
};

function ModeName({ value, onChange }: { value: string; onChange: (name: string) => void }) {
  const [draft, setDraft] = useState(value);
  function commit() { const name = draft.trim().slice(0, 40) || value; setDraft(name); onChange(name); }
  return <label className="genome-name">Название режима<input value={draft} maxLength={40} onChange={e => setDraft(e.target.value)} onBlur={commit}
    onKeyDown={e => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") setDraft(value); }} /></label>;
}

export function GenomeEditor({ genome, onChange, initialMode, onInitialModeChange, onSave, onLoad, allowedKinds }: GenomeEditorProps) {
  const [editing, setEditing] = useState(0);
  const [confirm, setConfirm] = useState<"reset" | "example" | "remove" | null>(null);
  const [message, setMessage] = useState("");
  const mode = Math.min(editing, genome.length - 1);
  const m = genome[mode];
  if (!m) return null;
  const allowed = (kind: number) => !allowedKinds || allowedKinds.includes(kind);
  function update<K extends keyof Mode>(key: K, value: Mode[K]) {
    onChange(genome.map((item, i) => i === mode ? { ...item, [key]: value } : item));
  }
  function add(copy: boolean) {
    if (genome.length >= MAX_MODES) return;
    const index = genome.length;
    const copiedReference = (reference: number) => reference === mode ? index : reference;
    const next = copy ? {
      ...m, name: `${m.name} — копия`,
      childA: copiedReference(m.childA), childB: copiedReference(m.childB),
      transitionA: copiedReference(m.transitionA), transitionB: copiedReference(m.transitionB),
      offspringMode: copiedReference(m.offspringMode), compatibleMode: copiedReference(m.compatibleMode),
      virusCopyFrom: copiedReference(m.virusCopyFrom), programs: m.programs.map(program => ({ ...program })),
    }
      : { ...newMode(index), kind: allowedKinds?.[0] ?? 0 };
    onChange([...genome, next]); setEditing(index); setMessage("");
  }
  function perform() {
    if (confirm === "remove") {
      const next = removeMode(genome, mode);
      onChange(next);
      onInitialModeChange(initialMode > mode ? initialMode - 1 : initialMode === mode ? Math.min(mode, next.length - 1) : initialMode);
      setEditing(Math.min(mode, next.length - 1));
      setMessage("Ссылки на удалённый режим перенаправлены к соседнему.");
    } else {
      onChange(confirm === "example" ? multicellularGenome() : defaultGenome().map(g => ({ ...g, kind: allowedKinds?.[0] ?? 0 })));
      onInitialModeChange(0); setEditing(0); setMessage("");
    }
    setConfirm(null);
  }
  const setting = (label: string, key: NumericModeKey, min: number, max: number, step: number, help: string, unit = "") =>
    <Setting label={label} value={m[key]} min={min} max={max} step={step} help={help} unit={unit} onChange={v => update(key, v)} />;
  const orientation = (label: string, key: "angle" | "childAngleA" | "childAngleB", help: string) =>
    <Setting label={label} value={Math.round(m[key] * 180 / Math.PI)} min={-180} max={180} step={1} unit="°" help={help}
      onChange={v => update(key, Math.max(-3.14, Math.min(3.14, v * Math.PI / 180)))} />;
  function setColor(color: number | null) {
    const channels = color === null ? [-1, -1, -1] : [(color >> 16) & 255, (color >> 8) & 255, color & 255].map(v => Math.round(v / 255 * 1e6) / 1e6);
    onChange(genome.map((item, i) => i === mode ? { ...item, red: channels[0], green: channels[1], blue: channels[2] } : item));
  }
  const connectionLimit = <>
    <label className="genome-check"><input type="checkbox" checked={m.maxConnections >= 0}
      onChange={e => update("maxConnections", e.target.checked ? 4 : -1)} />
      {m.kind === 9 ? "Задать порог для приклеивания и деления" : "Ограничивать деление по числу связей"}</label>
    {m.maxConnections >= 0 && <label className="genome-cell-type">Порог постоянных связей
      <input type="number" min={0} max={POPULATION_LIMIT} step={1} value={m.maxConnections}
        onChange={e => {
          const value = e.currentTarget.valueAsNumber;
          if (Number.isInteger(value)) update("maxConnections", Math.min(POPULATION_LIMIT, Math.max(0, value)));
        }} />
    </label>}
    {m.kind === 9 && <p>Глюоцит приклеивает соседей, пока у него меньше пяти связей. Заданный порог может уменьшить этот предел.</p>}
    {m.maxConnections >= 0 && <p>При достижении порога клетка перестаёт делиться. Существующие связи сохраняются.</p>}
  </>;

  return <div className="genome-editor">
    <div className="genome-file-actions"><button onClick={onSave}>Сохранить геном</button><button onClick={onLoad}>Загрузить геном</button></div>
    <label className="genome-initial">Начальный режим
      <select value={initialMode} onChange={e => onInitialModeChange(Number(e.target.value))}>
        {genome.map((g, i) => <option key={i} value={i}>{g.name} · {TYPES[g.kind].name}</option>)}
      </select>
    </label>
    <details className="genome-preview-disclosure"><summary>Предпросмотр развития</summary><GenomePreview genome={genome} initialMode={initialMode} /></details>
    <section className="genome-modes-section" aria-label="Режимы генома">
      <h3>Режимы</h3>
      <div className="genome-modes">{genome.map((g, i) => <button key={i} onClick={() => setEditing(i)} aria-pressed={mode === i} className={mode === i ? "active" : ""} title={`${g.name}: ${TYPES[g.kind].name}${i === initialMode ? ", начальный режим" : ""}`}><span className="genome-color-dot" aria-hidden="true" style={{backgroundColor:`#${modeColor(g).toString(16).padStart(6,"0")}`}} />{g.name}{i === initialMode ? " · начальный" : ""}</button>)}</div>
      <div className="genome-mode-actions"><button disabled={genome.length >= MAX_MODES} onClick={() => add(false)}>Добавить режим</button><button disabled={genome.length >= MAX_MODES} onClick={() => add(true)}>Копировать</button><button disabled={genome.length === 1} onClick={() => setConfirm("remove")}>Удалить</button></div>
      {genome.length >= MAX_MODES && <p>Достигнут предел: {MAX_MODES} режимов.</p>}
    </section>
    <ModeName key={`${mode}:${m.name}`} value={m.name} onChange={name => update("name", name)} />
    <label className="genome-cell-type">Тип клетки<select value={m.kind} onChange={e => update("kind", Number(e.target.value))}>
      {TYPES.map((t, i) => <option value={i} key={i} disabled={!allowed(i)}>{t.name}{allowed(i) ? "" : " — недоступен в задании"}</option>)}
    </select></label>
    <p className="genome-type-help">{TYPES[m.kind].info}</p>
    <section className="genome-color-section"><h3>Цвет клетки</h3>
      <label className="genome-check"><input type="checkbox" checked={m.red !== -1} onChange={e => setColor(e.target.checked ? TYPES[m.kind].color : null)} />Свой цвет</label>
      {m.red !== -1 ? <><label className="genome-color-picker">Цвет<input aria-label="Цвет клетки" type="color" value={`#${modeColor(m).toString(16).padStart(6,"0")}`} onChange={e => setColor(Number.parseInt(e.target.value.slice(1),16))} /></label>
        {([['red','Красный'],['green','Зелёный'],['blue','Синий']] as const).map(([key,label]) => <div className="genome-color-channel" key={key}><label>{label}<input aria-label={label} type="number" value={Number(m[key].toFixed(3))} min={0} max={1} step={0.01} onChange={e => {const value=e.currentTarget.valueAsNumber;if(Number.isFinite(value))update(key,Math.min(1,Math.max(0,value)));}} /></label><input aria-label={`${label}: ползунок`} type="range" value={m[key]} min={0} max={1} step={0.01} onChange={e => update(key,Number(e.target.value))} /></div>)}</>
        : <p>Используется цвет типа клетки.</p>}
    </section>
    <section className="genome-division">
      <h3>Деление</h3>
      <label className="genome-check"><input type="checkbox" checked={m.canDivide && m.kind !== 16} disabled={m.kind === 16} onChange={e => update("canDivide", e.target.checked)} />Разрешить деление</label>
      {m.kind === 16 ? <p>Гамета создаёт потомка при слиянии с другой гаметой. Настройки слияния находятся ниже.</p>
        : !m.canDivide && <p>Клетка продолжает питаться, двигаться и стареть, но не создаёт потомков.</p>}
      {m.kind === 9 && connectionLimit}
      <fieldset disabled={!m.canDivide || m.kind === 16}>
        <legend className="sr-only">Параметры деления</legend>
        {setting("Масса для деления", "threshold", 1, 8, 0.1, "Деление возможно при достижении этой массы и возраста 0,5 с. Рост зависит от типа клетки, доступной пищи и света.", "нг")}
        {m.kind !== 9 && connectionLimit}
        <div className="genome-children">{(["childA", "childB"] as const).map((key, index) => <section key={key}>
          <label>Дочерняя клетка {index === 0 ? "A" : "B"}<select value={m[key]} onChange={e => update(key, Number(e.target.value))}>{genome.map((g, i) => <option key={i} value={i}>{g.name} · {TYPES[g.kind].name}</option>)}</select></label>
          <label className="genome-check"><input type="checkbox" checked={index === 0 ? m.keepAdhesionA : m.keepAdhesionB} onChange={e => update(index === 0 ? "keepAdhesionA" : "keepAdhesionB", e.target.checked)} />Может унаследовать связи родителя</label>
          <label className="genome-check"><input type="checkbox" checked={index === 0 ? m.mirrorA : m.mirrorB} onChange={e => update(index === 0 ? "mirrorA" : "mirrorB", e.target.checked)} />Зеркально отразить</label>
          {orientation(`Направление дочери ${index === 0 ? 'A' : 'B'}`,index === 0 ? 'childAngleA' : 'childAngleB','Поворачивает дочь относительно направления от родителя вдоль оси деления. Зеркальное отражение меняет знак поворотов этой клетки и её дальнейшего развития.')}
        </section>)}</div>
        <label className="genome-check"><input type="checkbox" checked={m.bindChildren} onChange={e => update("bindChildren", e.target.checked)} />Связать дочерние клетки</label>
        <details><summary>Как работают связи</summary><p>Обычно прежняя связь переходит к ближайшим клеткам, которым разрешено её унаследовать. Если обе дочери связаны и могут наследовать, у родителя с двумя соседями каждая дочь получает по одной прежней связи. При поперечном делении связанной пары без других соседей сохраняются две боковые связи. Вместе со связями между дочерьми они замыкают контур. Если наследование выключено у обеих дочерей, прежние связи исчезают.</p></details>
        {setting("Доля массы дочери A", "ratio", 0.2, 0.8, 0.01, "Дочь A получает указанную долю массы, дочь B — оставшуюся. Например, 0,5 означает равное распределение.")}
        {orientation("Угол деления","angle","Поворачивает ось, вдоль которой располагаются дочерние клетки, относительно направления родителя. Зеркально отражённая клетка поворачивает её в обратную сторону.")}
      </fieldset>
      <p className="genome-lineage">{m.name} → {m.kind === 16 ? `слияние → ${genome[m.offspringMode].name}` : m.canDivide ? `${genome[m.childA].name} ${m.bindChildren ? "↔" : "+"} ${genome[m.childB].name}` : "без деления"}</p>
    </section>
    <section><h3>Питание</h3>
      {setting("Приоритет питания", "nutrientPriority", 0.25, 4, 0.05, "Повышает долю питания, которую получает клетка при обмене по постоянным связям.", "×")}
      <label className="genome-check"><input type="checkbox" checked={m.prioritizeWhenStarving}
        onChange={e => update("prioritizeWhenStarving", e.target.checked)} />Повышать приоритет при голодании</label>
    </section>
    <section><h3>Жизненный цикл</h3>
      {setting(m.kind === 2 ? "Скорость жгутика" : "Подвижность", "speed", 0, 60, 1, m.kind === 2 ? "Жгутик двигает клетку и связанные с ней клетки. Более высокая скорость увеличивает расход массы." : "Задаёт подвижность свободной клетки. Связанная клетка без жгутика сама не перемещает организм.")}
      {setting("Продолжительность жизни", "lifespan", 20, 1000, 10, "После этого возраста клетка погибает, если старение включено в настройках среды.", "с")}
    </section>
    <CellSettings mode={m} genome={genome} onChange={changes => onChange(genome.map((item, index) => index === mode ? { ...item, ...changes } : item))} />
    <details className="genome-examples"><summary>Начать с готового генома</summary>
      <button onClick={() => setConfirm("reset")}>Одна клетка</button>
      {[0, 1, 2].every(allowed) && <button onClick={() => setConfirm("example")}>Многоклеточный организм</button>}
    </details>
    {confirm && <div role="alert" className="genome-confirm"><p>{confirm === "remove" ? "Удалить режим? Ссылки потомков будут перенаправлены к соседнему." : "Заменить геном редактора? Несохранённые изменения будут потеряны. Клетки в чашке останутся прежними."}</p><div><button onClick={perform}>{confirm === "remove" ? "Удалить режим" : "Заменить"}</button><button onClick={() => setConfirm(null)}>Отмена</button></div></div>}
    {message && <p role="status">{message}</p>}
  </div>;
}
