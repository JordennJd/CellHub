import type { CSSProperties } from 'react';

export function Slider({ label, value, min, max, step, onChange, ends, fraction }: {
  label: string; value: number; min: number; max: number; step: number;
  onChange: (value: number) => void; ends?: [string, string]; fraction?: number;
}) {
  const progress = Math.max(0, Math.min(100, (fraction ?? (value - min) / (max - min)) * 100));
  return <span className="slider-control" style={{ '--progress': `${progress}%` } as CSSProperties}>
    <input type="range" aria-label={label} aria-valuetext={ends ? `${Math.round(value * 100)} : ${Math.round((1 - value) * 100)}` : undefined}
      value={value} min={min} max={max} step={step} onChange={event => onChange(Number(event.target.value))}/>
    {ends && <span className="slider-ends" aria-hidden="true"><span>{ends[0]}</span><span>{ends[1]}</span></span>}
  </span>;
}
