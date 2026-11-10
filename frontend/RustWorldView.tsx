import { useEffect, useRef } from "react";
import init, { WorldRenderer } from "./rust-wasm/cellhub_frontend.js";
import { bindCanvasControls } from "./input/canvas-controls";
import { WORLD, STRIDE, type Frame } from "./model";
import type { BackendPreference } from "./backend";
import type { WorldViewProps } from "./world-view-types";
export type { BackendPreference } from "./backend";
let moduleReady: Promise<unknown> | undefined;
const layerIndex = (layer: string) =>
  ({ biome: 0, light: 1, radiation: 2, food: 3 } as Record<string, number>)[
    layer
  ] ?? 0;

type RustWorldViewProps = WorldViewProps & {
  backend: BackendPreference;
  onBackendFailure: (message: string) => void;
};

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
    const canvas = mountCanvas();

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

    let removeControls = () => {};
    const addControls = () => {
      removeControls = bindCanvasControls(canvas, {
        camera: camera.current,
        worldSize: WORLD,
        tool: () => live.current.tool,
        onView: (final) => { applyView(); reportView(final); },
        onPoint: (p, action) => {
          let id: number | null = null, best = Infinity;
          const data = live.current.frame?.cells;
          if (action === "select" && data)
            for (let i = 0; i < data.length; i += STRIDE) {
              const distance = Math.hypot(data[i + 1] - p.x, data[i + 2] - p.y);
              if (distance < Math.max(data[i + 3], 12 / camera.current.zoom) && distance < best) {
                id = data[i]; best = distance;
              }
            }
          live.current.onPoint(p.x, p.y, id, action);
        },
      });
    };

    (async () => {
      try {
        await (moduleReady ??= init().catch((e) => {
          moduleReady = undefined;
          throw e;
        }));
        if (disposed) return;
        const instance = await WorldRenderer.create(canvas, props.backend);
        if (disposed) {
          instance.destroy();
          instance.free();
          return;
        }
        renderer.current = instance;
        instance.set_error_handler((message: string) => {
          if (!disposed) live.current.onBackendFailure(message);
        });
        const backend = instance.backend();
        canvas.dataset.renderer = backend;
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
                live.current.onBackendFailure(
                  `WebGPU device lost: ${info.message}`,
                );
            });
        }
        addControls();
        applyFrame(true);
        applyView(true);
        reportView(true);
      } catch (e) {
        if (!disposed) live.current.onBackendFailure(`Renderer: ${String(e)}`);
      }
    })();

    return () => {
      disposed = true;
      resetView.current = null;
      removeControls();
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
