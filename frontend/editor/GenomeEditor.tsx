import { useState } from "react";
import { defaultGenome, newMode, removeMode, MAX_MODES, TYPES, type Mode } from "../model";
import { demoOrganisms } from "../organisms";
import { listEntries, type GenomeData, type LibraryEntry } from "../game/storage";
import { CellSettings, DivisionSettings, Setting } from "./CellSettings";
import { CellTypePicker } from "./CellTypePicker";
import { GenomePreview } from "./GenomePreview";
import { GenomeSlots } from "./GenomeSlots";
import { Slider } from "./Slider";
import { SlotName } from "./SlotName";
import { cellColorHex, colorChannels, nextSlotColor } from "./slot-colors";
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

function CellColor({ mode, onChange }: { mode: Mode; onChange: (changes: Partial<Mode>) => void }) {
  const custom = mode.red >= 0;
  return <section className="genome-color-section">
    <h3>Цвет клетки</h3>
    <div className="genome-color-controls">
      <label className="genome-check"><input type="checkbox" checked={custom}
        onChange={event => onChange(colorChannels(event.currentTarget.checked ? TYPES[mode.kind].color : null))} />Свой цвет</label>
      {custom && <input aria-label="Цвет клетки" type="color" value={cellColorHex(mode)}
        onChange={event => onChange(colorChannels(Number.parseInt(event.currentTarget.value.slice(1), 16)))} />}
    </div>
    {custom && <details><summary>Каналы RGB</summary>
      {([['red', 'Красный'], ['green', 'Зелёный'], ['blue', 'Синий']] as const).map(([key, label]) =>
        <div className="genome-color-channel" key={key}>
          <label>{label}<input type="number" value={Number(mode[key].toFixed(3))} min={0} max={1} step={0.01}
            onChange={event => {
              const value = event.currentTarget.valueAsNumber;
              if (Number.isFinite(value)) onChange({ [key]: Math.min(1, Math.max(0, value)) });
            }} /></label>
          <Slider label={`${label}: ползунок`} value={mode[key]} min={0} max={1} step={0.01}
            onChange={value => onChange({ [key]: value })} />
        </div>)}
    </details>}
  </section>;
}

export function GenomeEditor({ genome, onChange, initialMode, onInitialModeChange, onSave, onLoad, allowedKinds }: GenomeEditorProps) {
  const [editing, setEditing] = useState(0);
  const [confirm, setConfirm] = useState<"reset" | "types" | "remove" | (GenomeData & { name: string }) | null>(null);
  const [examples, setExamples] = useState<{ saved: LibraryEntry[]; error?: string } | null>(null);
  const [message, setMessage] = useState("");
  const mode = Math.min(editing, genome.length - 1);
  const initial = Math.min(initialMode, genome.length - 1);
  const m = genome[mode];
  if (!m) return null;
  const allowed = (kind: number) => !allowedKinds || allowedKinds.includes(kind);
  const catalog = examples ? demoOrganisms(allowedKinds) : [];
  const choice = typeof confirm === "object" ? confirm : null;

  function changeMode(changes: Partial<Mode>) {
    onChange(genome.map((item, index) => index === mode ? { ...item, ...changes } : item));
  }

  function add(copy: boolean) {
    if (genome.length >= MAX_MODES) return;
    const index = genome.length;
    const names = new Set(genome.map(slot => slot.name));
    let number = index + 1;
    while (names.has(`M${number}`)) number++;
    const copySuffix = " — копия";
    const copiedReference = (reference: number) => reference === mode ? index : reference;
    const next = copy ? {
      ...m, name: `${m.name.slice(0, 40 - copySuffix.length)}${copySuffix}`,
      childA: copiedReference(m.childA), childB: copiedReference(m.childB),
      transitionA: copiedReference(m.transitionA), transitionB: copiedReference(m.transitionB),
      offspringMode: copiedReference(m.offspringMode), compatibleMode: copiedReference(m.compatibleMode),
      virusCopyFrom: copiedReference(m.virusCopyFrom), programs: m.programs.map(program => ({ ...program })),
    } : { ...newMode(index), name: `M${number}`, kind: allowedKinds?.[0] ?? 0, ...colorChannels(nextSlotColor(genome)) };
    onChange([...genome, next]);
    setEditing(mode);
    setConfirm(null);
    setMessage(`Добавлен слот ${next.name}.`);
  }

  function perform() {
    if (confirm === "remove") {
      const next = removeMode(genome, mode);
      onChange(next);
      onInitialModeChange(initial > mode ? initial - 1 : Math.min(initial, next.length - 1));
      setEditing(Math.min(mode, next.length - 1));
      setMessage(`Слот ${m.name} удалён. Ссылки перенаправлены к соседнему слоту.`);
    } else if (choice) {
      onChange(structuredClone(choice.genome));
      onInitialModeChange(choice.initialMode);
      setEditing(0);
      setMessage(`Загружен организм «${choice.name}».`);
    } else {
      const next = confirm === "types"
        ? TYPES.map((_, kind) => kind).filter(allowed).map((kind, index) => ({ ...newMode(index), kind }))
        : defaultGenome().map(slot => ({ ...slot, kind: allowedKinds?.[0] ?? 0 }));
      onChange(next);
      onInitialModeChange(0);
      setEditing(0);
      setMessage("Геном редактора заменён.");
    }
    setConfirm(null);
  }

  function toggleExamples(open: boolean) {
    if (!open) {
      setExamples(null);
      return;
    }
    try {
      setExamples({ saved: listEntries("genome") });
    } catch (error) {
      setExamples({ saved: [], error: error instanceof Error ? error.message : "Не удалось прочитать библиотеку геномов." });
    }
  }

  const confirmation = confirm && <div role="alert" className="genome-confirm">
    <p>{confirm === "remove" ? `Удалить слот ${m.name}? Ссылки на него будут перенаправлены к соседнему слоту.`
      : `${choice ? `Загрузить организм «${choice.name}»?` : "Заменить геном редактора?"} Несохранённые изменения будут потеряны.`}</p>
    <div><button onClick={perform}>{confirm === "remove" ? "Удалить слот" : "Заменить"}</button>
      <button onClick={() => setConfirm(null)}>Отмена</button></div>
  </div>;

  return <div className="genome-editor">
    <p className="editor-intro">Геном — инструкция для новых клеток и их потомков. Настройте его здесь, затем выберите «Добавить клетку» и нажмите на поле. Клетки, уже находящиеся в чашке, сохранят прежний геном.</p>
    <GenomeSlots genome={genome} selected={mode} initial={initial} onInitialChange={onInitialModeChange}
      onSelect={index => { setEditing(index); setConfirm(null); }}
      onDaughterChange={(key, index) => changeMode({ [key]: index })}
      onAdd={add} onRemove={() => setConfirm("remove")} />
    {confirm === "remove" && confirmation}
    <div className="genome-mode-fields">
      <SlotName key={`${mode}:${m.name}`} name={m.name} onRename={name => changeMode({ name })} />
      <CellTypePicker mode={m} allowedKinds={allowedKinds} onChange={kind => changeMode({ kind })} />
    </div>
    <div className="genome-tools">
      <div className="genome-file-actions"><button onClick={onSave}>Сохранить геном</button><button onClick={onLoad}>Загрузить геном</button></div>
      <details className="genome-preview-disclosure"><summary>Предпросмотр развития</summary>
        <GenomePreview genome={genome} initialMode={initial} />
      </details>
      <details className="genome-demo" onToggle={event => toggleExamples(event.currentTarget.open)}>
        <summary>Примеры геномов</summary>
        {examples && <div className="genome-demo-examples">
          <button onClick={() => setConfirm("reset")}>Одна клетка</button>
          <button onClick={() => setConfirm("types")}>Все доступные типы</button>
          {catalog.length > 0 ? <>
            <h4>Организмы по универсальности</h4>
            <div className="genome-demo-list">
              {catalog.map(organism => <button key={organism.id} className="genome-demo-organism"
                onClick={() => setConfirm({ name: organism.name, genome: organism.genome(), initialMode: organism.initialMode })}>
                <strong>{organism.name}</strong>
                <span>{organism.origin === "substrate" ? "из субстрата" : "ручная сборка"} · универсальность {organism.universality}</span>
                <small>{organism.description}</small>
              </button>)}
            </div>
          </> : <p>Среди доступных типов клеток нет готовых многоклеточных организмов.</p>}
          {examples.saved.length > 0 && <>
            <h4>Мои сохранённые геномы</h4>
            <div className="genome-demo-list">
              {examples.saved.map(entry => {
                const data = entry.data as GenomeData;
                const compatible = data.genome.every(item => allowed(item.kind));
                return <button key={entry.id} className="genome-demo-organism" disabled={!compatible}
                  title={compatible ? undefined : "Содержит типы клеток, недоступные в этом испытании."}
                  onClick={() => setConfirm({ ...data, name: entry.name })}>
                  <strong>{entry.name}</strong>
                  <span>{data.genome.length} слотов{compatible ? "" : " · недоступен"}</span>
                </button>;
              })}
            </div>
          </>}
          {examples.error && <p role="alert">{examples.error}</p>}
        </div>}
      </details>
      {confirm !== "remove" && confirmation}
    </div>
    <DivisionSettings mode={m} onChange={changeMode} />
    <section><h3>Питание</h3>
      <Setting label="Приоритет питания" value={m.nutrientPriority} min={0.25} max={4} step={0.05} unit="×"
        help="Повышает долю питания, которую получает клетка при обмене по постоянным связям."
        onChange={nutrientPriority => changeMode({ nutrientPriority })} />
      <label className="genome-check"><input type="checkbox" checked={m.prioritizeWhenStarving}
        onChange={event => changeMode({ prioritizeWhenStarving: event.currentTarget.checked })} />Повышать приоритет при голодании</label>
    </section>
    <section><h3>Жизненный цикл</h3>
      <Setting label={m.kind === 2 ? "Скорость жгутика" : "Подвижность"} value={m.speed} min={0} max={60} step={1}
        help={m.kind === 2 ? "Жгутик двигает клетку и связанные с ней клетки. Более высокая скорость увеличивает расход массы."
          : "Задаёт подвижность свободной клетки."} onChange={speed => changeMode({ speed })} />
      <Setting label="Продолжительность жизни" value={m.lifespan} min={20} max={1000} step={10} unit="с"
        help="После этого возраста клетка погибает, если старение включено в настройках среды."
        onChange={lifespan => changeMode({ lifespan })} />
    </section>
    <CellSettings mode={m} genome={genome} onChange={changeMode} />
    <CellColor key={mode} mode={m} onChange={changeMode} />
    <p role="status" className="sr-only">{message}</p>
  </div>;
}
