import { useEffect, useRef, useState } from "react";
import { TICK_RATE, flatten, STRIDE, TYPES, type Command, type Frame, type Mode } from "../model";
import { DivisionAnimator, divisionMotionEnabled, playDivisionAnimation } from "../division-animation";

import { drawCell, cellExtents } from "./cell-drawing";
import { Slider } from "./Slider";

type PreviewParameters = { genome: Mode[]; initialMode: number; age: number };
type PreviewRequest = PreviewParameters & { id: number };

function sameGenome(a: PreviewParameters, b: PreviewParameters) {
  return a.genome === b.genome && a.initialMode === b.initialMode;
}

/** Preview owns a separate laboratory: its clock and cells never affect the game. */
export function GenomePreview({ genome, initialMode }: { genome: Mode[]; initialMode: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const worker = useRef<Worker | null>(null);
  const latest = useRef<PreviewParameters>({ genome, initialMode, age: 0 });
  const sequence = useRef(0);
  const pending = useRef<PreviewRequest | null>(null);
  const presented = useRef<PreviewRequest | null>(null);
  const [animator] = useState(() => new DivisionAnimator());
  const [ready, setReady] = useState(false);
  const [age, setAge] = useState(0);
  const [result, setResult] = useState<{ frame: Frame; request: PreviewRequest } | null>(null);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");
  const frame = result?.frame ?? null;
  latest.current = { genome, initialMode, age };

  useEffect(() => {
    const w = new Worker(new URL("../simulation.worker.ts", import.meta.url), { type: "module", name: "cellhub-preview" });
    worker.current = w;
    w.onmessage = ({ data }) => {
      if (data.type === "ready") setReady(true);
      const request = pending.current;
      if (data.type === "frame" && request && data.previewRequestId === request.id
        && sameGenome(request, latest.current) && request.age === latest.current.age) {
        pending.current = null;
        setResult({ frame: data, request });
        setBusy(false);
      }
      if (data.type === "error" && (data.requestId === undefined || data.requestId === request?.id)) {
        setError("Не удалось показать развитие этого генома.");
        setBusy(false);
      }
    };
    w.onerror = () => { setError("Предпросмотр недоступен. Геном можно проверить в чашке."); setBusy(false); };
    return () => { w.terminate(); worker.current = null; };
  }, []);

  useEffect(() => {
    if (!ready) return;
    setBusy(true);
    setError("");
    const request = { genome, initialMode, age, id: ++sequence.current };
    pending.current = request;
    const previous = presented.current;
    if (previous && (!sameGenome(previous, request) || age < previous.age || age > previous.age + 1)) {
      animator.update(frame, performance.now(), false);
    }
    const timer = window.setTimeout(() => {
      const command: Command = {
        type: "preview", genome: flatten(genome), mode: initialMode, steps: age * TICK_RATE, requestId: request.id,
      };
      worker.current?.postMessage(command);
    }, 160);
    return () => window.clearTimeout(timer);
  }, [genome, initialMode, age, ready]);

  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const isOpen = () => element.closest("details")?.open === true;
    const request = result?.request;
    const previous = presented.current;
    const animate = !!request && !!previous && isOpen() && divisionMotionEnabled()
      && sameGenome(previous, request) && request.age === previous.age + 1;
    animator.update(frame, performance.now(), animate);
    function draw(cells = animator.sample(performance.now())) {
      if (!element) return;
      if (request && isOpen()) presented.current = request;
      const width = element.clientWidth || 300, height = 180;
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const pixelsWide = Math.round(width * dpr), pixelsHigh = Math.round(height * dpr);
      if (element.width !== pixelsWide) element.width = pixelsWide;
      if (element.height !== pixelsHigh) element.height = pixelsHigh;
      const ctx = element.getContext("2d");
      if (!ctx) return;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, width, height);
      if (!frame || !cells?.length) return;
      let left = Infinity, right = -Infinity, top = Infinity, bottom = -Infinity;
      const positions = new Map<number, number>();
      for (let i = 0; i < cells.length; i += STRIDE) {
        positions.set(cells[i], i);
        const extent = cells[i + 3] * (cellExtents[cells[i + 4]] ?? 1);
        left = Math.min(left, cells[i + 1] - extent);
        right = Math.max(right, cells[i + 1] + extent);
        top = Math.min(top, cells[i + 2] - extent);
        bottom = Math.max(bottom, cells[i + 2] + extent);
      }
      const scale = Math.min((width - 40) / Math.max(right - left, 80), (height - 40) / Math.max(bottom - top, 80));
      const x = (index: number) => width / 2 + (cells[index + 1] - (left + right) / 2) * scale;
      const y = (index: number) => height / 2 + (cells[index + 2] - (top + bottom) / 2) * scale;
      ctx.strokeStyle = "#adc89d";
      ctx.lineWidth = 2;
      for (let i = 0; i < frame.links.length; i += 2) {
        const a = positions.get(frame.links[i]), b = positions.get(frame.links[i + 1]);
        if (a === undefined || b === undefined) continue;
        ctx.beginPath(); ctx.moveTo(x(a), y(a)); ctx.lineTo(x(b), y(b)); ctx.stroke();
      }
      for (let i = 0; i < cells.length; i += STRIDE) {
        drawCell(ctx, cells, i, x(i), y(i), Math.max(2, cells[i + 3] * scale));
      }
    }
    draw();
    const stop = playDivisionAnimation(animator, draw);
    const observer = new ResizeObserver(() => draw());
    observer.observe(element);
    return () => { stop(); observer.disconnect(); };
  }, [result]);

  return <section className="genome-preview" aria-label="Предпросмотр развития организма" aria-busy={busy}>
    <div className="genome-preview-heading"><h3>Развитие организма</h3><span>{busy ? "Расчёт…" : `${frame?.cells.length ? frame.cells.length / STRIDE : 0} кл.`}</span></div>
    <canvas ref={canvas} aria-label={`Организм в возрасте ${age} секунд`} role="img" />
    {error && <p role="alert">{error}</p>}
    {!busy && !error && frame && frame.cells.length === 0 && <p>К этому возрасту клетки погибли.</p>}
    <label className="preview-age">Возраст: <output>{age} с</output>
      <Slider label="Возраст предпросмотра" min={0} max={60} step={1} value={age} onChange={setAge} />
    </label>
    <details><summary>Условия предпросмотра</summary><p>Начальная клетка помещена в освещённую область с пищей и без радиации. Ползунок меняет возраст только здесь; время и клетки в основной чашке сохраняются.</p></details>
  </section>;
}
