import { useEffect, useRef } from 'react';
import { modeColor, TYPES, type Mode } from '../model';
import { drawCell } from './cell-drawing';

export function CellTypePicker({ mode, allowedKinds, onChange }: {
  mode: Mode; allowedKinds?: number[]; onChange: (kind: number) => void;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const type = TYPES[mode.kind];
  useEffect(() => {
    const ctx = canvas.current?.getContext('2d');
    if (!ctx) return;
    const cell = new Float32Array(12);
    cell[4] = mode.kind; cell[10] = modeColor(mode);
    ctx.clearRect(0, 0, 160, 120);
    drawCell(ctx, cell, 0, 80, 60, 25);
  }, [mode.kind, mode.red, mode.green, mode.blue]);
  return <section className="cell-type-picker">
    <label>Тип клетки<select value={mode.kind} onChange={event => onChange(Number(event.target.value))}>
      {TYPES.map((item, kind) => <option key={kind} value={kind} disabled={!!allowedKinds && !allowedKinds.includes(kind)}>{item.name} / {item.latin}</option>)}
    </select></label>
    <div className="cell-type-preview"><canvas ref={canvas} width="160" height="120" role="img" aria-label={type.name}/><div><strong>{type.name}</strong><span>{type.latin}</span></div></div>
    <details><summary>О типе клетки</summary><p>{type.info}</p></details>
  </section>;
}
