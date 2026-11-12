import { useEffect, useId, useLayoutEffect, useRef, useState, type PointerEvent } from 'react';
import { modeColor, TYPES, type Mode } from '../model';
import { drawCell } from './cell-drawing';
import './CellTypePicker.css';

type HelpState = 'hover' | 'focus' | 'pinned' | null;

export function CellTypePicker({ mode, allowedKinds, onChange }: {
  mode: Mode; allowedKinds?: number[]; onChange: (kind: number) => void;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const popover = useRef<HTMLDivElement>(null);
  const leaveTimer = useRef<number | undefined>(undefined);
  const [help, setHelp] = useState<HelpState>(null);
  const selectId = useId();
  const helpId = useId();
  const open = help !== null;
  const type = TYPES[mode.kind];
  const color = modeColor(mode);
  useEffect(() => {
    const ctx = canvas.current?.getContext('2d');
    if (!ctx) return;
    const cell = new Float32Array(12);
    cell[4] = mode.kind;
    cell[10] = color;
    ctx.clearRect(0, 0, 96, 80);
    drawCell(ctx, cell, 0, 50, 40, 21);
  }, [mode.kind, color]);

  useLayoutEffect(() => {
    const button = trigger.current;
    const panel = popover.current;
    if (!open || !button || !panel) return;

    const position = () => {
      const anchor = button.getBoundingClientRect();
      const gap = 6;
      const edge = 8;
      const width = document.documentElement.clientWidth;
      const height = document.documentElement.clientHeight;
      const below = anchor.bottom + gap;
      const above = anchor.top - gap - panel.offsetHeight;
      const top = below + panel.offsetHeight <= height - edge ? below : above;
      panel.style.top = `${Math.max(edge, top)}px`;
      panel.style.left = `${Math.max(edge, Math.min(anchor.right - panel.offsetWidth, width - panel.offsetWidth - edge))}px`;
    };
    const dismissOutside = ({ target }: Event) => {
      if (target instanceof Node && !button.contains(target) && !panel.contains(target)) setHelp(null);
    };
    const dismissOnEscape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      setHelp(null);
    };
    const visibility = new IntersectionObserver(([entry]) => {
      if (!entry.isIntersecting) setHelp(null);
    });

    panel.showPopover?.();
    position();
    visibility.observe(button);
    window.addEventListener('resize', position);
    window.addEventListener('keydown', dismissOnEscape, true);
    document.addEventListener('scroll', position, true);
    document.addEventListener('pointerdown', dismissOutside, true);
    document.addEventListener('focusin', dismissOutside);
    return () => {
      visibility.disconnect();
      window.clearTimeout(leaveTimer.current);
      window.removeEventListener('resize', position);
      window.removeEventListener('keydown', dismissOnEscape, true);
      document.removeEventListener('scroll', position, true);
      document.removeEventListener('pointerdown', dismissOutside, true);
      document.removeEventListener('focusin', dismissOutside);
      if (panel.isConnected) panel.hidePopover?.();
    };
  }, [open, type]);

  const keepHover = () => window.clearTimeout(leaveTimer.current);
  const leaveHover = (event: PointerEvent) => {
    if (event.pointerType === 'touch' || help !== 'hover') return;
    keepHover();
    // Give the pointer time to cross the gap between the button and the card.
    leaveTimer.current = window.setTimeout(() => setHelp(state => state === 'hover' ? null : state), 130);
  };

  return <section className="cell-type-picker">
    <div className="genome-cell-type">
      <label htmlFor={selectId}>Тип клетки</label>
      <select id={selectId} value={mode.kind} onChange={event => onChange(Number(event.target.value))}>
        {TYPES.map((item, kind) => (kind === mode.kind || !allowedKinds || allowedKinds.includes(kind)) &&
          <option key={kind} value={kind}>{item.name}</option>)}
      </select>
    </div>
    <button type="button" className="cell-type-help" ref={trigger}
      aria-label={`О типе клетки «${type.name}»`} aria-describedby={helpId} aria-controls={helpId} aria-expanded={open}
      onPointerEnter={event => {
        if (event.pointerType === 'touch') return;
        keepHover();
        setHelp(state => state ?? 'hover');
      }}
      onPointerLeave={leaveHover}
      onFocus={event => {
        if (event.currentTarget.matches(':focus-visible')) setHelp('focus');
      }}
      onBlur={() => setHelp(state => state === 'focus' ? null : state)}
      onClick={() => setHelp(state => state === 'pinned' ? null : 'pinned')}>
      <canvas ref={canvas} width="96" height="80" aria-hidden="true"/>
      <span aria-hidden="true">?</span>
    </button>
    <div className="cell-type-help-card" ref={popover} id={helpId} popover="manual" role="tooltip" hidden={!open}
      onPointerEnter={keepHover} onPointerLeave={leaveHover}>
      <strong>{type.latin}</strong>
      <p>{type.info}</p>
    </div>
  </section>;
}
