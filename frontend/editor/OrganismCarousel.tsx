import { useEffect, useMemo, useRef, useState } from 'react';
import { modeColor, TYPES, type Mode } from '../model';
import { demoOrganisms } from '../organisms';
import { drawCell, cellExtents } from './cell-drawing';

/** Static type previews share the game's drawing code; no simulation workers. */
export function CellAppearance({ mode }: { mode: Mode }) {
    const canvas = useRef<HTMLCanvasElement>(null);
    useEffect(() => {
        const ctx = canvas.current?.getContext('2d');
        if (!ctx) return;
        ctx.clearRect(0, 0, 112, 80);
        const row = new Float32Array(12);
        row[4] = mode.kind; row[10] = modeColor(mode);
        const extent = cellExtents[mode.kind] ?? 1;
        drawCell(ctx, row, 0, 56 + (mode.kind === 2 ? 13 : 0), 40, 29 / Math.max(1, extent * .72));
    }, [mode]);
    return <figure className="organism-cell"><canvas ref={canvas} width={112} height={80} role="img" aria-label={TYPES[mode.kind].name}/><figcaption>{mode.name} · {TYPES[mode.kind].name}</figcaption></figure>;
}

export function OrganismCarousel({ allowedKinds, onChoose }: { allowedKinds?: number[]; onChoose: (genome: Mode[], initialMode: number, name: string) => void }) {
    const catalog = useMemo(() => demoOrganisms(allowedKinds).map(item => ({ ...item, modes: item.genome() })), [allowedKinds]);
    const [index, setIndex] = useState(0);
    const current = Math.min(index, catalog.length - 1);
    const item = catalog[current];
    if (!item) return null;
    return <section className="organism-carousel" aria-label="Готовые организмы">
        <div className="organism-carousel-heading"><h3>Готовые организмы</h3><span>{current + 1} / {catalog.length}</span></div>
        <div className="organism-carousel-navigation"><button type="button" aria-label="Предыдущий организм" disabled={catalog.length < 2} onClick={() => setIndex((current - 1 + catalog.length) % catalog.length)}>←</button>
            <strong aria-live="polite">{item.name}</strong>
            <button type="button" aria-label="Следующий организм" disabled={catalog.length < 2} onClick={() => setIndex((current + 1) % catalog.length)}>→</button></div>
        <div className="organism-cell-previews">{item.modes.map((mode, i) => <CellAppearance key={`${item.id}-${i}`} mode={mode}/>)}</div>
        <p>{item.description}</p>
        <button type="button" onClick={() => onChoose(structuredClone(item.modes), item.initialMode, item.name)}>Выбрать организм</button>
    </section>;
}
