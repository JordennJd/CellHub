import { useEffect, useRef } from "react";
import init, { WorldRenderer } from "./rust-wasm/cellhub_frontend.js";
import { WORLD, STRIDE, type Frame } from "./model";
import type { WorldViewProps } from "./world-view-types";
let moduleReady: Promise<unknown> | undefined;
const layerIndex = (layer: string) =>
  ({ biome: 0, light: 1, radiation: 2, food: 3 } as Record<string, number>)[
    layer
  ] ?? 0;

export type BackendPreference = "auto" | "webgpu" | "webgl";
type RustWorldViewProps = WorldViewProps & { backend: BackendPreference };

export function RustWorldView(props: RustWorldViewProps) {
  const host = useRef<HTMLDivElement>(null),
    live = useRef(props);
  live.current = props;
  const renderer = useRef<WorldRenderer | null>(null);
  const camera = useRef({ x: WORLD / 2, y: WORLD / 2, zoom: 0.55 });
  const frame = useRef<Frame | null>(null);
  const appliedFrame = useRef<Frame | null>(null);
  const appliedView = useRef("");
  const resetView = useRef<(() => void) | null>(null);
  const lastReport = useRef(0);

  const viewKey = () => {
    const c = camera.current;
    return `${c.x}/${c.y}/${c.zoom}/${layerIndex(live.current.layer)}/${live.current.selected}`;
  };
  const applyView = (force = false) => {
    const instance = renderer.current;
    if (!instance) return;
    const key = viewKey();
    if (!force && key === appliedView.current) return;
    appliedView.current = key;
    const c = camera.current;
    instance.set_view(
      c.x,
      c.y,
      c.zoom,
      layerIndex(live.current.layer),
      live.current.selected ?? -1,
    );
  };
  const applyFrame = (force = false) => {
    const instance = renderer.current,
      next = frame.current;
    if (!instance || !next) return;
    if (!force && next === appliedFrame.current) return;
    try {
      instance.set_frame(next.cells, next.terrain, next.links);
      appliedFrame.current = next;
    } catch (e) {
      live.current.onError(`Renderer: ${String(e)}`);
    }
  };
  const reportView = (force = false) => {
    const now = performance.now();
    if (!force && now - lastReport.current < 100) return;
    lastReport.current = now;
    live.current.onView({ ...camera.current });
  };

  // JS only forwards new snapshots and view changes. Rust schedules and paints
  // its own animation frames; there is no JS render loop anymore.
  useEffect(() => {
    frame.current = props.frame;
    applyFrame();
  }, [props.frame]);
  useEffect(() => {
    applyView();
  }, [props.layer, props.selected]);
  useEffect(() => {
    resetView.current?.();
  }, [props.recenter]);

  useEffect(() => {
    const el = host.current!;
    let disposed = false;
    const mountCanvas = () => {
      const value = document.createElement("canvas");
      value.style.width = "100%";
      value.style.height = "100%";
      value.setAttribute("aria-label", "Субстрат CellHub: клетки и биомы");
      el.appendChild(value);
      return value;
    };
    let canvas = mountCanvas();

    const reset = () => {
      const c = camera.current;
      c.x = WORLD / 2;
      c.y = WORLD / 2;
      c.zoom = Math.max(
        0.001,
        (Math.min(el.clientWidth, el.clientHeight) / WORLD) * 0.93,
      );
      applyView(true);
      reportView(true);
    };
    resetView.current = reset;
    reset();

    let down: { x: number; y: number; cx: number; cy: number } | null = null,
      moved = false;
    const point = (e: PointerEvent | WheelEvent) => {
      const r = canvas.getBoundingClientRect();
      return { x: e.clientX - r.left, y: e.clientY - r.top };
    };
    const world = (p: { x: number; y: number }) => ({
      x: (p.x - el.clientWidth / 2) / camera.current.zoom + camera.current.x,
      y: (p.y - el.clientHeight / 2) / camera.current.zoom + camera.current.y,
    });
    const pd = (e: PointerEvent) => {
      const p = point(e);
      down = { ...p, cx: camera.current.x, cy: camera.current.y };
      moved = false;
      canvas.setPointerCapture(e.pointerId);
    };
    const pm = (e: PointerEvent) => {
      if (!down) return;
      const p = point(e);
      if (Math.hypot(p.x - down.x, p.y - down.y) > 5) moved = true;
      if (moved || live.current.tool === "pan") {
        camera.current.x = down.cx - (p.x - down.x) / camera.current.zoom;
        camera.current.y = down.cy - (p.y - down.y) / camera.current.zoom;
        applyView();
        reportView();
      }
    };
    const pu = (e: PointerEvent) => {
      if (down && !moved && live.current.tool !== "pan") {
        const p = world(point(e));
        if (p.x >= 0 && p.y >= 0 && p.x <= WORLD && p.y <= WORLD) {
          let id: number | null = null,
            best = Infinity;
          const data = live.current.frame?.cells;
          if (data)
            for (let i = 0; i < data.length; i += STRIDE) {
              const d = Math.hypot(data[i + 1] - p.x, data[i + 2] - p.y);
              if (d < Math.max(data[i + 3], 12 / camera.current.zoom) && d < best) {
                id = data[i];
                best = d;
              }
            }
          live.current.onPoint(p.x, p.y, id);
        }
      }
      down = null;
      if (canvas.hasPointerCapture(e.pointerId))
        canvas.releasePointerCapture(e.pointerId);
      reportView(true);
    };
    const cancel = () => {
      down = null;
    };
    const wheel = (e: WheelEvent) => {
      e.preventDefault();
      const p = point(e),
        before = world(p);
      camera.current.zoom = Math.max(
        0.12,
        Math.min(4, camera.current.zoom * Math.exp(-e.deltaY * 0.001)),
      );
      const after = world(p);
      camera.current.x += before.x - after.x;
      camera.current.y += before.y - after.y;
      applyView();
      reportView(true);
    };
    const addListeners = (target: HTMLCanvasElement) => {
      target.addEventListener("pointerdown", pd);
      target.addEventListener("pointermove", pm);
      target.addEventListener("pointerup", pu);
      target.addEventListener("pointercancel", cancel);
      target.addEventListener("lostpointercapture", cancel);
      target.addEventListener("wheel", wheel, { passive: false });
    };
    const removeListeners = (target: HTMLCanvasElement) => {
      target.removeEventListener("pointerdown", pd);
      target.removeEventListener("pointermove", pm);
      target.removeEventListener("pointerup", pu);
      target.removeEventListener("pointercancel", cancel);
      target.removeEventListener("lostpointercapture", cancel);
      target.removeEventListener("wheel", wheel);
    };

    (async () => {
      try {
        await (moduleReady ??= init().catch((e) => {
          moduleReady = undefined;
          throw e;
        }));
        if (disposed) return;
        let instance: WorldRenderer;
        try {
          instance = await WorldRenderer.create(canvas, props.backend);
        } catch (e) {
          if (disposed) return;
          if (props.backend !== "auto") throw e;
          // A canvas keeps the first context type forever. If WebGPU got far
          // enough to claim it, start over on a fresh canvas with WebGL2.
          console.warn("CellHub: WebGPU failed, retrying with WebGL", e);
          canvas.remove();
          canvas = mountCanvas();
          instance = await WorldRenderer.create(canvas, "webgl");
        }
        if (disposed) {
          instance.destroy();
          instance.free();
          return;
        }
        renderer.current = instance;
        instance.set_error_handler((message: string) => {
          if (!disposed) live.current.onError(message);
        });
        const backend = instance.backend();
        live.current.onRenderer(backend === "webgl" ? "Rust WebGL" : "Rust WebGPU");
        if (backend === "webgpu") {
          // Keep this Promise handler in JS: wgpu 24's WebGPU lost callback
          // drops its wasm-bindgen Closure before device.lost settles.
          const gpuContext = canvas.getContext("webgpu") as unknown as {
            getConfiguration?: () => {
              device: { lost: Promise<{ reason: string; message: string }> };
            } | null;
          };
          gpuContext
            ?.getConfiguration?.()
            ?.device.lost.then((info) => {
              if (!disposed && info.reason !== "destroyed")
                live.current.onError(`WebGPU device lost: ${info.message}`);
            });
        }
        addListeners(canvas);
        applyFrame(true);
        applyView(true);
        reportView(true);
      } catch (e) {
        if (!disposed) live.current.onError(`Renderer: ${String(e)}`);
      }
    })();

    return () => {
      disposed = true;
      resetView.current = null;
      removeListeners(canvas);
      canvas.remove();
      const instance = renderer.current;
      renderer.current = null;
      if (instance) {
        instance.destroy();
        instance.free();
      }
    };
  }, []);
  return <div className="world" ref={host} />;
}
