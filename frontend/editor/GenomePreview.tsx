import { useEffect, useRef, useState } from "react";
import { flatten, STRIDE, TYPES, type Frame, type Mode } from "../model";

/** Preview owns a separate laboratory: its clock and cells never affect the game. */
export function GenomePreview({ genome, initialMode }: { genome: Mode[]; initialMode: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const worker = useRef<Worker | null>(null);
  const latest = useRef({ genome, initialMode, age: 0 });
  const [ready, setReady] = useState(false);
  const [age, setAge] = useState(0);
  const [frame, setFrame] = useState<Frame | null>(null);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState("");
  latest.current = { genome, initialMode, age };

  useEffect(() => {
    const w = new Worker(new URL("../simulation.worker.ts", import.meta.url), { type: "module" });
    worker.current = w;
    w.onmessage = ({ data }) => {
      if (data.type === "ready") setReady(true);
      if (data.type === "frame") { setFrame(data); setBusy(false); }
      if (data.type === "error") { setError("Не удалось показать развитие этого генома."); setBusy(false); }
    };
    w.onerror = () => { setError("Предпросмотр недоступен. Геном можно проверить в чашке."); setBusy(false); };
    return () => { w.terminate(); worker.current = null; };
  }, []);

  useEffect(() => {
    if (!ready) return;
    setBusy(true);
    setError("");
    const timer = window.setTimeout(() => {
      const current = latest.current;
      worker.current?.postMessage({ type: "preview", genome: flatten(current.genome), mode: current.initialMode, steps: current.age * 30 });
    }, 160);
    return () => window.clearTimeout(timer);
  }, [genome, initialMode, age, ready]);

  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    function draw() {
      if (!element) return;
      const width = element.clientWidth || 300, height = 180;
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      element.width = Math.round(width * dpr);
      element.height = height * dpr;
      const ctx = element.getContext("2d");
      if (!ctx) return;
      ctx.scale(dpr, dpr);
      ctx.clearRect(0, 0, width, height);
      if (!frame?.cells.length) return;
      const cells = frame.cells;
      let left = Infinity, right = -Infinity, top = Infinity, bottom = -Infinity;
      const positions = new Map<number, number>();
      for (let i = 0; i < cells.length; i += STRIDE) {
        positions.set(cells[i], i);
        left = Math.min(left, cells[i + 1] - cells[i + 3]);
        right = Math.max(right, cells[i + 1] + cells[i + 3]);
        top = Math.min(top, cells[i + 2] - cells[i + 3]);
        bottom = Math.max(bottom, cells[i + 2] + cells[i + 3]);
      }
      const scale = Math.min((width - 40) / Math.max(right - left, 80), (height - 40) / Math.max(bottom - top, 80));
      const x = (index: number) => width / 2 + (cells[index + 1] - (left + right) / 2) * scale;
      const y = (index: number) => height / 2 + (cells[index + 2] - (top + bottom) / 2) * scale;
      ctx.strokeStyle = "#769f7d";
      ctx.lineWidth = 2;
      for (let i = 0; i < frame.links.length; i += 2) {
        const a = positions.get(frame.links[i]), b = positions.get(frame.links[i + 1]);
        if (a === undefined || b === undefined) continue;
        ctx.beginPath(); ctx.moveTo(x(a), y(a)); ctx.lineTo(x(b), y(b)); ctx.stroke();
      }
      for (let i = 0; i < cells.length; i += STRIDE) {
        const radius = Math.max(2, cells[i + 3] * scale);
        const color = cells[i + 10] >= 0 ? cells[i + 10] : TYPES[cells[i + 4]]?.color ?? 0xa4db71;
        ctx.fillStyle = `#${color.toString(16).padStart(6, "0")}`;
        ctx.strokeStyle = "#315d43";
        ctx.lineWidth = 1.5;
        ctx.beginPath(); ctx.arc(x(i), y(i), radius, 0, Math.PI * 2); ctx.fill(); ctx.stroke();
        ctx.fillStyle = "#315d43";
        ctx.beginPath(); ctx.arc(x(i), y(i), Math.max(1, radius * 0.23), 0, Math.PI * 2); ctx.fill();
        const angle = cells[i + 9], handedness = cells[i + 11] ? -1 : 1;
        ctx.beginPath();
        ctx.moveTo(x(i), y(i));
        ctx.lineTo(x(i) + Math.cos(angle) * radius * .8, y(i) + Math.sin(angle) * radius * .8);
        ctx.lineTo(x(i) + Math.cos(angle + handedness * .5) * radius * .5, y(i) + Math.sin(angle + handedness * .5) * radius * .5);
        ctx.stroke();
      }
    }
    draw();
    const observer = new ResizeObserver(draw);
    observer.observe(element);
    return () => observer.disconnect();
  }, [frame]);

  return <section className="genome-preview" aria-label="Предпросмотр развития организма" aria-busy={busy}>
    <div className="genome-preview-heading"><h3>Развитие организма</h3><span>{busy ? "Расчёт…" : `${frame?.cells.length ? frame.cells.length / STRIDE : 0} кл.`}</span></div>
    <canvas ref={canvas} aria-label={`Организм в возрасте ${age} секунд`} role="img" />
    {error && <p role="alert">{error}</p>}
    {!busy && !error && frame && frame.cells.length === 0 && <p>К этому возрасту клетки погибли.</p>}
    <label className="preview-age">Возраст: <output>{age} с</output>
      <input aria-label="Возраст предпросмотра" type="range" min={0} max={60} step={1} value={age} onChange={e => setAge(Number(e.target.value))} />
    </label>
    <details><summary>Условия предпросмотра</summary><p>Начальная клетка помещена в освещённую область с пищей и без радиации. Ползунок меняет возраст только здесь; время и клетки в основной чашке сохраняются.</p></details>
  </section>;
}
