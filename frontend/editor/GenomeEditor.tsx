import { Slider } from "./Slider";
import { SlotName } from "./SlotName";
import { CellTypePicker } from "./CellTypePicker";
import { useEffect, useId, useState } from "react";
import { defaultGenome, modeColor, newMode, removeMode, MAX_MODES, POPULATION_LIMIT, TYPES, type Mode, type NumericModeKey } from "../model";
import { demoOrganisms } from "../organisms";
import { listEntries } from "../game/storage";
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

export function GenomeEditor({ genome, onChange, initialMode, onInitialModeChange, onSave, onLoad, allowedKinds }: GenomeEditorProps) {
  const demoId = useId();
  const [demoMode, setDemoMode] = useState(false);
  const [editing, setEditing] = useState(0);
  const [confirm, setConfirm] = useState<"reset" | "types" | "remove" | "choice" | null>(null);
  const [choice, setChoice] = useState<{ name: string; genome: Mode[]; initialMode: number } | null>(null);
  const [savedGenomes, setSavedGenomes] = useState<{ name: string; genome: Mode[]; initialMode: number }[]>([]);
  const [savedError, setSavedError] = useState("");
  const [message, setMessage] = useState("");
  useEffect(() => {
    if (!demoMode) return;
    try {
      setSavedGenomes(listEntries("genome").map(entry => {
        const data = entry.data as { genome: Mode[]; initialMode: number };
        return { name: entry.name, genome: data.genome, initialMode: data.initialMode };
      }));
      setSavedError("");
    } catch (e) {
      setSavedGenomes([]);
      setSavedError(e instanceof Error ? e.message : "Не удалось прочитать библиотеку геномов.");
    }
  }, [demoMode]);
  const mode = Math.min(editing, genome.length - 1);
  const m = genome[mode];
  if (!m) return null;
  const allowed = (kind: number) => !allowedKinds || allowedKinds.includes(kind);
  const catalog = demoOrganisms(allowedKinds);
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
      setMessage("Ссылки на удалённый слот перенаправлены к соседнему.");
    } else if (confirm === "choice" && choice) {
      onChange(structuredClone(choice.genome));
      onInitialModeChange(choice.initialMode);
      setEditing(0);
      setMessage(`Загружен организм «${choice.name}».`);
      setChoice(null);
    } else {
      onChange(confirm === "types" ? TYPES.flatMap((_, kind) => allowed(kind) ? [kind] : []).map((kind, index) => ({ ...newMode(index), kind })) : defaultGenome().map(g => ({ ...g, kind: allowedKinds?.[0] ?? 0 })));
      onInitialModeChange(0); setEditing(0); setMessage("");
    }
    setConfirm(null);
  }
  function choose(name: string, next: Mode[], nextInitial: number) {
    setChoice({ name, genome: next, initialMode: nextInitial });
    setConfirm("choice");
  }
  const setting = (label: string, key: NumericModeKey, min: number, max: number, step: number, help: string, unit = "") =>
    <Setting label={label} value={m[key]} min={min} max={max} step={step} help={help} unit={unit} onChange={v => update(key, v)} />;
  const orientation = (label: string, key: "angle" | "childAngleA" | "childAngleB", help: string) => {
    const limit = key === "angle" ? 3.14 : Math.PI;
    return <Setting label={label} value={Math.round(m[key] * 180 / Math.PI)} min={-180} max={180} step={1} unit="°" help={help}
      onChange={v => update(key, Math.max(-limit, Math.min(limit, v * Math.PI / 180)))} />;
  };
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
    <section className="genome-demo"><label className="genome-check"><input type="checkbox" role="switch" checked={demoMode} aria-controls={demoId} onChange={e => setDemoMode(e.target.checked)}/>Режим демонстрации</label>
      {demoMode && <div id={demoId} className="genome-demo-examples"><p>Пример заменит слоты редактора. Поле и живые клетки сохранятся.</p>
        <button onClick={() => setConfirm("reset")}>Одна клетка</button>
        <button onClick={() => setConfirm("types")}>Все доступные типы</button>
        {catalog.length > 0 ? <>
          <h4>Готовые организмы</h4>
          <div className="genome-demo-list">
            {catalog.map(organism => <button key={organism.id} className="genome-demo-organism" onClick={() => choose(organism.name, organism.genome(), organism.initialMode)}>
              <strong>{organism.name}</strong>
              <small>{organism.description}</small>
            </button>)}
          </div>
        </> : <p>Среди доступных типов клеток нет готовых многоклеточных организмов.</p>}
        {savedGenomes.length > 0 && <>
          <h4>Мои сохранённые геномы</h4>
          <div className="genome-demo-list">
            {savedGenomes.map((entry, index) => {
              const compatible = entry.genome.every(item => allowed(item.kind));
              return <button key={`${entry.name}-${index}`} className="genome-demo-organism" disabled={!compatible}
                title={compatible ? undefined : "Содержит типы клеток, недоступные в этом испытании."}
                onClick={() => choose(entry.name, entry.genome, entry.initialMode)}>
                <strong>{entry.name}</strong>
                <span>{entry.genome.length} слотов{compatible ? "" : " · недоступен"}</span>
              </button>;
            })}
          </div>
        </>}
        {savedError && <p role="alert">{savedError}</p>}
      </div>}
    </section>
    <div className="genome-file-actions"><button onClick={onSave}>Сохранить геном</button><button onClick={onLoad}>Загрузить геном</button></div>
    <label className="genome-initial">Начальный слот
      <select value={initialMode} onChange={e => onInitialModeChange(Number(e.target.value))}>
        {genome.map((g, i) => <option key={i} value={i}>{g.name} · {TYPES[g.kind].name}</option>)}
      </select>
    </label>
    <details className="genome-preview-disclosure"><summary>Предпросмотр развития</summary><GenomePreview genome={genome} initialMode={initialMode} /></details>
    <section className="genome-modes-section" aria-label="Слоты генома">
      <h3>Слоты</h3>
      <div className="genome-modes">{genome.map((g, i) => <button key={i} onClick={() => setEditing(i)} aria-pressed={mode === i} className={mode === i ? "active" : ""} title={`${g.name}: ${TYPES[g.kind].name}${i === initialMode ? ", начальный слот" : ""}`}><span className="genome-color-dot" aria-hidden="true" style={{backgroundColor:`#${modeColor(g).toString(16).padStart(6,"0")}`}} />{g.name}{i === initialMode ? " · начальный" : ""}</button>)}</div>
      <div className="genome-mode-actions"><button disabled={genome.length >= MAX_MODES} onClick={() => add(false)}>Добавить слот</button><button disabled={genome.length >= MAX_MODES} onClick={() => add(true)}>Копировать</button><button disabled={genome.length === 1} onClick={() => setConfirm("remove")}>Удалить</button></div>
      {genome.length >= MAX_MODES && <p>Достигнут предел: {MAX_MODES} слотов.</p>}
    </section>
    <SlotName key={`${mode}:${m.name}`} name={m.name} onRename={name => update("name", name)} />
    <CellTypePicker mode={m} allowedKinds={allowedKinds} onChange={kind => update("kind", kind)}/>
    <section className="genome-color-section"><h3>Цвет клетки</h3>
      <label className="genome-check"><input type="checkbox" checked={m.red !== -1} onChange={e => setColor(e.target.checked ? TYPES[m.kind].color : null)} />Свой цвет</label>
      {m.red !== -1 ? <><label className="genome-color-picker">Цвет<input aria-label="Цвет клетки" type="color" value={`#${modeColor(m).toString(16).padStart(6,"0")}`} onChange={e => setColor(Number.parseInt(e.target.value.slice(1),16))} /></label>
        {([['red','Красный'],['green','Зелёный'],['blue','Синий']] as const).map(([key,label]) => <div className="genome-color-channel" key={key}><label>{label}<input aria-label={label} type="number" value={Number(m[key].toFixed(3))} min={0} max={1} step={0.01} onChange={e => {const value=e.currentTarget.valueAsNumber;if(Number.isFinite(value))update(key,Math.min(1,Math.max(0,value)));}} /></label><Slider label={`${label}: ползунок`} value={m[key]} min={0} max={1} step={0.01} onChange={value => update(key,value)}/></div>)}</>
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
          {orientation(`Направление дочери ${index === 0 ? 'A' : 'B'}`,index === 0 ? 'childAngleA' : 'childAngleB','Поворачивает дочь относительно оси деления в системе отсчёта родителя. Отражение дочери меняет сторону поворотов при её собственном делении.')}
        </section>)}</div>
        <label className="genome-check"><input type="checkbox" checked={m.bindChildren} onChange={e => update("bindChildren", e.target.checked)} />Связать дочерние клетки</label>
        <details><summary>Как работают связи</summary><p>Прежнюю связь может унаследовать дочь, которая остаётся в зоне контакта с соседом и у которой включено наследование. Сестра может перекрыть этот контакт. Если обе связанные дочери могут сохранить по одному контакту с прежними соседями, связи распределяются между ними.</p><p>Если связь между дочерьми выключена, они получают противоположные импульсы вдоль оси деления. Дочь, сохранившая связь с организмом, передаёт ему отдачу.</p></details>
        <div className="mass-distribution"><label><span>Распределение массы<output>{Math.round(m.ratio * 100)} : {Math.round((1 - m.ratio) * 100)}</output></span><Slider label="Распределение массы" value={m.ratio} min={0.2} max={0.8} step={0.01} fraction={m.ratio} ends={["A", "B"]} onChange={value => update("ratio", value)}/></label></div>
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
    {confirm && <div role="alert" className="genome-confirm"><p>{confirm === "remove" ? "Удалить слот? Ссылки потомков будут перенаправлены к соседнему." : confirm === "choice" && choice ? `Загрузить организм «${choice.name}»? Несохранённые изменения будут потеряны. Клетки в чашке останутся прежними.` : "Заменить геном редактора? Несохранённые изменения будут потеряны. Клетки в чашке останутся прежними."}</p><div><button onClick={perform}>{confirm === "remove" ? "Удалить слот" : "Заменить"}</button><button onClick={() => { setConfirm(null); setChoice(null); }}>Отмена</button></div></div>}
    {message && <p role="status">{message}</p>}
  </div>;
}
