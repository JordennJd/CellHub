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

function lighten(color: number, amount = 0.45) {
  const channel = (shift: number) => Math.round(((color >> shift) & 255) * (1 - amount) + 255 * amount);
  return `#${((channel(16) << 16) | (channel(8) << 8) | channel(0)).toString(16).padStart(6, "0")}`;
}

/** Appendages that stick out of the collision circle need extra preview padding. */
function cellExtent(kind: number) {
  if (kind === 2) return 2;
  if (kind === 13 || kind === 16) return 1.45;
  if (kind === 3 || kind === 8 || kind === 17) return 1.25;
  if (kind === 4) return 1.1;
  return 1;
}

/** Draw the same type-specific organelles as the WGSL scene, in local cell space. */
function drawCellDetails(ctx: CanvasRenderingContext2D, kind: number, x: number, y: number, r: number, angle: number, ink: string, accent: string) {
  ctx.save();
  ctx.translate(x, y);
  ctx.rotate(angle);
  ctx.lineCap = "round";
  ctx.strokeStyle = ink;
  ctx.fillStyle = accent;
  const w = Math.max(0.8, r * 0.055);
  const dot = (cx: number, cy: number, size: number, alpha = 1) => {
    ctx.globalAlpha = alpha; ctx.beginPath(); ctx.arc(cx, cy, size, 0, Math.PI * 2); ctx.fill();
  };
  const disc = (cx: number, cy: number, radius: number, width = w, alpha = 1) => {
    ctx.strokeStyle = ink; ctx.globalAlpha = alpha; ctx.lineWidth = width;
    ctx.beginPath(); ctx.arc(cx, cy, radius, 0, Math.PI * 2); ctx.stroke();
  };
  const line = (ax: number, ay: number, bx: number, by: number, width = w, alpha = 1) => {
    ctx.globalAlpha = alpha; ctx.lineWidth = width;
    ctx.beginPath(); ctx.moveTo(ax, ay); ctx.lineTo(bx, by); ctx.stroke();
  };
  // Shared faint nucleus, matching the shader.
  disc(r * 0.18, 0, r * 0.28, w, 0.28);
  if (kind === 0) {
    for (let i = 0; i < 5; i++) {
      const a = i * 1.256637 + 0.35, px = Math.cos(a) * r * 0.5, py = Math.sin(a) * r * 0.5, size = Math.max(1.2, r * 0.11);
      dot(px, py, size, 0.55); disc(px, py, size, w * 0.6, 0.8);
    }
  } else if (kind === 1) {
    for (let i = 0; i < 3; i++) {
      const a = i * 2.094395 + 0.5, px = Math.cos(a) * r * 0.35 + r * 0.32, py = Math.sin(a) * r * 0.35;
      dot(px, py, r * 0.16, 0.32); disc(px, py, r * 0.16, w * 0.8, 0.85);
    }
    line(r * 0.88, r * 0.3, r * 0.88, -r * 0.3, w, 0.9);
  } else if (kind === 2) {
    line(-r, 0, -r * 1.45, r * 0.22, w, 0.95);
    line(-r * 1.45, r * 0.22, -r * 1.9, -r * 0.12, w, 0.95);
    dot(-r, 0, r * 0.11, 0.6);
  } else if (kind === 3) {
    for (let i = 0; i < 8; i++) {
      const a = i * Math.PI / 4;
      line(Math.cos(a) * r * 0.94, Math.sin(a) * r * 0.94, Math.cos(a) * r * 1.22, Math.sin(a) * r * 1.22, w * 0.8, 0.85);
    }
    disc(0, 0, r * 0.56, w * 0.6, 0.5);
  } else if (kind === 4) {
    disc(0, 0, r * 0.72, w, 0.8);
    for (let i = 0; i < 6; i++) {
      const a = i * 1.047198 + 0.52;
      ctx.fillStyle = ink; dot(Math.cos(a) * r * 0.72, Math.sin(a) * r * 0.72, Math.max(0.9, r * 0.06), 0.7);
    }
  } else if (kind === 5) {
    line(r * 0.25, r * 0.42, r * 0.82, 0, w, 0.95);
    line(r * 0.25, -r * 0.42, r * 0.82, 0, w, 0.95);
    disc(0, 0, r * 0.55, w * 0.7, 0.6);
  } else if (kind === 6) {
    const px = -r * 0.08;
    dot(px, 0, r * 0.55, 0.25); disc(px, 0, r * 0.55, w, 0.9); dot(px - r * 0.18, 0, r * 0.12, 0.35);
  } else if (kind === 7) {
    dot(0, 0, r * 0.46, 0.3); disc(0, 0, r * 0.46, w, 0.9); dot(-r * 0.14, r * 0.16, r * 0.1, 0.5);
  } else if (kind === 8) {
    ctx.fillStyle = ink;
    for (let i = 0; i < 6; i++) {
      const a = i * 1.047198;
      dot(Math.cos(a) * r * 1.08, Math.sin(a) * r * 1.08, Math.max(1, r * 0.09), 0.9);
    }
    disc(0, 0, r * 0.5, w * 0.6, 0.45);
  } else if (kind === 9) {
    ctx.fillStyle = ink;
    for (let i = 0; i < 4; i++) {
      const a = i * Math.PI / 2 + Math.PI / 4;
      dot(Math.cos(a) * r * 0.45, Math.sin(a) * r * 0.45, r * 0.1, 0.85);
    }
    disc(0, 0, r * 0.62, w * 0.5, 0.45);
  } else if (kind === 10) {
    for (const sign of [1, -1]) {
      const px = r * 0.4, py = sign * r * 0.42;
      dot(px, py, r * 0.15, 0.4); disc(px, py, r * 0.15, w * 0.8, 0.9);
      line(r * 0.15, 0, px, py, w * 0.5, 0.5);
    }
  } else if (kind === 11) {
    const px = r * 0.42;
    dot(px, 0, r * 0.2, 0.3); disc(px, 0, r * 0.2, w * 0.8, 0.9);
    ctx.fillStyle = ink; dot(px, 0, r * 0.07, 0.9);
    disc(px, 0, r * 0.34, w * 0.5, 0.45);
  } else if (kind === 12) {
    for (let i = -2; i <= 2; i++) {
      const offset = i * r * 0.22;
      line(-r * 0.55, offset, r * 0.55, offset, w * 0.55, 0.8);
    }
  } else if (kind === 13) {
    line(-r * 0.05, 0, -r * 1.22, 0, w, 0.9);
    line(-r * 1.22, 0, -r * 1.4, r * 0.3, w * 0.7, 0.9);
    line(-r * 1.22, 0, -r * 1.4, -r * 0.3, w * 0.7, 0.9);
    dot(r * 0.1, 0, r * 0.18, 0.4);
  } else if (kind === 14) {
    for (let i = 0; i < 3; i++) {
      const a = i * 2.094395 + 0.5, px = Math.cos(a) * r * 0.3 + r * 0.3, py = Math.sin(a) * r * 0.3;
      dot(px, py, r * 0.13, 0.45); disc(px, py, r * 0.13, w * 0.7, 0.85);
    }
    line(r * 0.55, 0, r * 0.9, 0, w * 0.6, 0.7);
  } else if (kind === 15) {
    dot(0, 0, r * 0.38, 0.28); disc(0, 0, r * 0.38, w, 0.9);
    ctx.fillStyle = ink; dot(-r * 0.08, 0, r * 0.16, 0.8);
  } else if (kind === 16) {
    line(-r * 0.7, 0, -r * 1.35, r * 0.42, w, 0.9);
    dot(r * 0.25, 0, r * 0.14, 0.5); disc(0, 0, r * 0.6, w * 0.5, 0.4);
  } else if (kind === 17) {
    for (let i = 0; i < 12; i++) {
      const a = i * Math.PI / 6;
      line(Math.cos(a) * r, Math.sin(a) * r, Math.cos(a) * r * 1.18, Math.sin(a) * r * 1.18, w * 0.7, 0.85);
    }
    disc(0, 0, r * 0.55, w * 0.5, 0.45);
  }
  ctx.globalAlpha = 1;
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
        const extent = cells[i + 3] * cellExtent(cells[i + 4]);
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
        const radius = Math.max(2, cells[i + 3] * scale);
        const kind = cells[i + 4];
        const color = cells[i + 10] >= 0 ? cells[i + 10] : TYPES[kind]?.color ?? 0xa4db71;
        const cellColor = `#${color.toString(16).padStart(6, "0")}`;
        const ink = cells[i + 10] >= 0 && relativeLuminance(color) < minimumInkLuminance
          ? "#dce6dc" : cellColor;
        ctx.fillStyle = cellColor;
        ctx.strokeStyle = ink;
        ctx.lineWidth = 1.5;
        ctx.beginPath(); ctx.arc(x(i), y(i), radius, 0, Math.PI * 2);
        ctx.globalAlpha = 0.22; ctx.fill();
        ctx.globalAlpha = 1; ctx.stroke();
        const accent = lighten(cells[i + 10] >= 0 && relativeLuminance(color) < minimumInkLuminance ? 0xdce6dc : color);
        drawCellDetails(ctx, kind, x(i), y(i), radius, cells[i + 9], ink, accent);
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
