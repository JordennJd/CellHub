import { useRef, useState } from "react";
import { MAX_MODES, TYPES, type Mode } from "../model";
import { cellColorHex } from "./slot-colors";

type SlotSelection = {
  genome: Mode[];
  selected: number;
  initial: number;
  onSelect: (index: number) => void;
};

type Daughter = "childA" | "childB";

function SlotLabel({ mode, start, branch }: { mode: Mode; start?: boolean; branch?: string }) {
  return <>
    {branch && <span className="slot-branch">{branch}</span>}
    <span className="genome-color-dot" aria-hidden="true" style={{ backgroundColor: cellColorHex(mode) }} />
    <span className="slot-label">{mode.name}</span>
    {start && <span className="slot-start">Старт</span>}
  </>;
}

function SlotPicker({ genome, selected, initial, onSelect }: SlotSelection) {
  const picker = useRef<HTMLDetailsElement>(null);
  const trigger = useRef<HTMLElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const search = query.trim().toLocaleLowerCase();
  const slots = open ? genome.map((mode, index) => ({ mode, index })).filter(({ mode, index }) =>
    !search || `${mode.name} ${TYPES[mode.kind].name} ${TYPES[mode.kind].latin} m${index + 1}`
      .toLocaleLowerCase().includes(search)) : [];

  function close() {
    if (picker.current) picker.current.open = false;
    setQuery("");
    trigger.current?.focus();
  }

  return <details className="slot-picker" ref={picker} onToggle={event => setOpen(event.currentTarget.open)} onKeyDown={event => {
    if (event.key === "Escape" && picker.current?.open) {
      event.stopPropagation();
      close();
    }
  }}>
    <summary ref={trigger} aria-label={`Редактируемый слот: ${genome[selected].name}`}>
      <SlotLabel mode={genome[selected]} start={selected === initial} />
    </summary>
    {open && <div className="slot-picker-content">
      {genome.length > 8 && <input type="search" aria-label="Найти слот" placeholder="Найти слот" value={query}
        onChange={event => setQuery(event.currentTarget.value)} />}
      {slots.length ? <ul className="slot-options" aria-label="Список слотов">
        {slots.map(({ mode, index }) => <li key={index}>
          <button type="button" aria-pressed={selected === index} title={`${mode.name} · ${TYPES[mode.kind].name}`}
            onClick={() => { onSelect(index); close(); }}>
            <SlotLabel mode={mode} start={index === initial} />
          </button>
        </li>)}
      </ul> : <p role="status">Слоты не найдены.</p>}
    </div>}
  </details>;
}

const daughters = [["childA", "A"], ["childB", "B"]] as const;
const divides = (mode: Mode) => mode.canDivide && mode.kind !== 16;

function divisionLayers(genome: Mode[], initial: number) {
  const layers = [[initial]];
  const seen = new Set([initial]);
  for (let depth = 0; depth < layers.length; depth++) {
    const next: number[] = [];
    for (const index of layers[depth]) {
      const parent = genome[index];
      if (!divides(parent)) continue;
      for (const [key] of daughters) {
        const child = parent[key];
        if (!seen.has(child)) {
          seen.add(child);
          next.push(child);
        }
      }
    }
    if (next.length) layers.push(next);
  }
  return { layers, other: genome.map((_, index) => index).filter(index => !seen.has(index)) };
}

function DivisionGraph({ genome, selected, initial, onSelect, onDaughterChange }: SlotSelection & {
  onDaughterChange: (daughter: Daughter, index: number) => void;
}) {
  const { layers, other } = divisionLayers(genome, initial);
  const parent = genome[selected];

  function node(index: number, branch?: string) {
    const mode = genome[index];
    return <button key={branch} type="button" aria-pressed={selected === index}
      title={`${mode.name} · ${TYPES[mode.kind].name}`} onClick={() => onSelect(index)}>
      <SlotLabel mode={mode} branch={branch} />
    </button>;
  }

  function rows(indices: number[]) {
    return <ul className="division-graph-rows">{indices.map(index => <li key={index}>
      {node(index)}
      {divides(genome[index]) && <>
        <span className="division-graph-arrow" aria-hidden="true">→</span>
        {daughters.map(([key, branch]) => node(genome[index][key], branch))}
      </>}
    </li>)}</ul>;
  }

  return <>
    {divides(parent) && <fieldset className="division-graph-edit">
      <legend>Потомки {parent.name}</legend>
      {daughters.map(([key, branch]) => <label key={key}>Дочерняя клетка {branch}
        <select value={parent[key]} onChange={event => onDaughterChange(key, Number(event.currentTarget.value))}>
          {genome.map((mode, index) => <option key={index} value={index}>{mode.name} · {TYPES[mode.kind].name}</option>)}
        </select>
      </label>)}
    </fieldset>}
    <div className="division-graph" role="region" aria-label="Поколения генома" tabIndex={0}>
      {layers.map((layer, depth) => <section key={depth}>
        <h4>{depth === 0 ? "Начальный слот" : `Поколение ${depth}`}</h4>
        {rows(layer)}
      </section>)}
      {!!other.length && <section><h4>Остальные слоты</h4>{rows(other)}</section>}
    </div>
  </>;
}

export function GenomeSlots({ genome, selected, initial, onSelect, onInitialChange, onDaughterChange, onAdd, onRemove }:
  SlotSelection & {
    onInitialChange: (index: number) => void;
    onDaughterChange: (daughter: Daughter, index: number) => void;
    onAdd: (copy: boolean) => void;
    onRemove: () => void;
  }) {
  const [graphOpen, setGraphOpen] = useState(false);
  return <section className="genome-modes-section" aria-label="Слоты генома">
    <h3>Слоты</h3>
    <div className="genome-slot-controls">
      <div className="genome-slot-field">
        <span>Редактируемый слот</span>
        <SlotPicker genome={genome} selected={selected} initial={initial} onSelect={onSelect} />
      </div>
      <label className="genome-initial">Начальный слот
        <select aria-label="Начальный слот" value={initial} onChange={event => onInitialChange(Number(event.currentTarget.value))}>
          {genome.map((mode, index) => <option key={index} value={index}>{mode.name} · {TYPES[mode.kind].name}</option>)}
        </select>
      </label>
    </div>
    <div className="genome-mode-actions">
      <button type="button" disabled={genome.length >= MAX_MODES} onClick={() => onAdd(false)}>Добавить слот</button>
      <button type="button" disabled={genome.length >= MAX_MODES} onClick={() => onAdd(true)}>Копировать</button>
      <button type="button" disabled={genome.length === 1} onClick={onRemove}>Удалить</button>
    </div>
    {genome.length >= MAX_MODES && <p>Достигнут предел в {MAX_MODES} слотов.</p>}
    <details className="division-graph-disclosure" onToggle={event => setGraphOpen(event.currentTarget.open)}>
      <summary>Граф делений</summary>
      {graphOpen && <DivisionGraph genome={genome} selected={selected} initial={initial}
        onSelect={onSelect} onDaughterChange={onDaughterChange} />}
    </details>
  </section>;
}
