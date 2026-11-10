import { useEffect, useRef, useState } from "react";
import { flatten, STRIDE, TYPES, type Frame, type Mode } from "../model";

function relativeLuminance(color: number) {
  const linear = (channel: number) => {
    const value = channel / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear((color >> 16) & 255)
    + 0.7152 * linear((color >> 8) & 255) + 0.0722 * linear(color & 255);
}
// Same 3:1 dark-dish detail threshold as scene.wgsl; body RGB stays user-defined.
const minimumInkLuminance = 3 * (relativeLuminance(0x253e2b) + 0.05) - 0.05;
const tau = Math.PI * 2;
const cellExtents = [1, 1, 2.05, 1.25, 1, 1.27, 1.45];

function polar(radius: number, angle: number): [number, number] {
  return [radius * Math.cos(angle), radius * Math.sin(angle)];
}

function buildCellMarks() {
  const membrane = new Path2D();
  membrane.arc(0, 0, 1, 0, tau);
  const innerRing = new Path2D();
  innerRing.arc(0, 0, 0.72, 0, tau);
  const flagellum = new Path2D(`
    M -0.91 -0.08
    C -1.11 -0.04 -1.25 -0.25 -1.45 -0.21
    C -1.67 -0.17 -1.75 0.12 -2.02 0.06
    C -1.76 0.20 -1.61 -0.09 -1.44 -0.12
    C -1.24 -0.15 -1.10 0.09 -0.91 0.08 Z
  `);
  const chloroplasts = new Path2D(`
    M -0.67 -0.29 C -0.71 -0.52 -0.46 -0.65 -0.33 -0.48
    C -0.25 -0.35 -0.44 -0.24 -0.58 -0.22 Q -0.65 -0.21 -0.67 -0.29 Z
    M 0.04 -0.68 C 0.12 -0.78 0.36 -0.71 0.35 -0.58
    C 0.34 -0.45 0.20 -0.39 0.08 -0.45 Q -0.05 -0.50 0.04 -0.68 Z
    M 0.48 -0.18 C 0.52 -0.33 0.73 -0.28 0.77 -0.11
    C 0.80 0.00 0.67 0.13 0.55 0.09 Q 0.43 0.04 0.48 -0.18 Z
    M 0.28 0.40 C 0.40 0.24 0.62 0.32 0.61 0.45
    C 0.59 0.64 0.35 0.72 0.27 0.58 Q 0.21 0.51 0.28 0.40 Z
    M -0.42 0.38 C -0.29 0.31 -0.12 0.43 -0.14 0.57
    C -0.18 0.73 -0.39 0.72 -0.50 0.60 Q -0.57 0.48 -0.42 0.38 Z
  `);
  const rods = new Path2D();
  for (let i = 0; i < 17; i++) {
    const angle = i * tau / 17;
    rods.moveTo(...polar(0.96, angle));
    rods.lineTo(...polar(1.20, angle));
  }
  const teeth = new Path2D();
  for (let i = 0; i < 19; i++) {
    const angle = i * tau / 19;
    teeth.moveTo(...polar(0.97, angle - 0.078));
    teeth.lineTo(...polar(1.25, angle));
    teeth.lineTo(...polar(0.97, angle + 0.078));
    teeth.closePath();
  }
  const tufts = new Path2D();
  for (let i = 0; i < 5; i++) {
    const angle = i * tau / 5;
    for (const side of [-1, 1]) {
      const joint = polar(1.17, angle + side * 0.14);
      tufts.moveTo(...polar(0.97, angle + side * 0.10));
      tufts.lineTo(...joint);
      tufts.lineTo(...polar(1.39, angle + side * 0.12));
      tufts.moveTo(...joint);
      tufts.lineTo(...polar(1.34, angle + side * 0.28));
    }
  }
  return { membrane, innerRing, flagellum, chloroplasts, rods, teeth, tufts };
}

let cellMarks: ReturnType<typeof buildCellMarks> | undefined;

function drawCell(ctx: CanvasRenderingContext2D, cells: Float32Array, i: number, x: number, y: number, radius: number) {
  const marks = cellMarks ??= buildCellMarks();
  const kind = cells[i + 4];
  const color = cells[i + 10] >= 0 ? cells[i + 10] : TYPES[kind]?.color ?? 0xa4db71;
  const bodyColor = `#${color.toString(16).padStart(6, "0")}`;
  const ink = cells[i + 10] >= 0 && relativeLuminance(color) < minimumInkLuminance ? "#dce6dc" : bodyColor;
  ctx.save();
  ctx.translate(x, y);
  ctx.rotate(cells[i + 9]);
  ctx.scale(radius, cells[i + 11] ? -radius : radius);
  ctx.fillStyle = bodyColor;
  ctx.globalAlpha = 0.22;
  ctx.fill(marks.membrane);
  ctx.strokeStyle = ink;
  ctx.lineWidth = 1.5 / radius;
  ctx.globalAlpha = 1;
  ctx.stroke(marks.membrane);

  ctx.fillStyle = ink;
  ctx.lineWidth = 0.085;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  switch (kind) {
    case 0:
      ctx.fillStyle = "#57a73f";
      ctx.fill(marks.chloroplasts);
      break;
    case 2: ctx.fill(marks.flagellum); break;
    case 3: ctx.stroke(marks.rods); break;
    case 4: ctx.stroke(marks.innerRing); break;
    case 5: ctx.fill(marks.teeth); break;
    case 6: ctx.stroke(marks.tufts); break;
  }
  ctx.fillStyle = ink;
  ctx.globalAlpha = 0.9;
  ctx.beginPath();
  ctx.arc(0, 0, Math.max(0.21, 1 / radius), 0, tau);
  ctx.fill();
  ctx.restore();
}

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
    const w = new Worker(new URL("../simulation.worker.ts", import.meta.url), { type: "module", name: "cellhub-preview" });
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
