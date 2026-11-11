import { useEffect, useRef, useState } from "react";
import init, { WorldRenderer } from "./rust-wasm/cellhub_frontend.js";
import { bindCanvasControls, fitDishCamera, resizeDishCamera, zoomDishCamera } from "./input/canvas-controls";
import { WORLD, STRIDE, type Frame } from "./model";
import { DivisionAnimator, divisionMotionEnabled, playDivisionAnimation } from "./division-animation";
import type { BackendPreference } from "./backend";
import type { WorldViewProps } from "./world-view-types";
export type { BackendPreference } from "./backend";
let moduleReady: Promise<unknown> | undefined;
const emptyCells = new Float32Array();
const emptyLinks = new Uint32Array();
const layerIndex = (layer: string) =>
  ({ biome: 0, light: 1, radiation: 2, food: 3, covered: 3, nitrates: 4 } as Record<string, number>)[
    layer
  ] ?? 0;

type RustWorldViewProps = WorldViewProps & {
  backend: BackendPreference;
  onBackendFailure: (message: string) => void;
};

export function RustWorldView(props: RustWorldViewProps) {
  const worldSize = props.frame?.worldSize ?? WORLD;
  const worldWidth = props.frame?.worldWidth ?? worldSize;
  const host = useRef<HTMLDivElement>(null),
    live = useRef(props);
  live.current = props;
  const renderer = useRef<WorldRenderer | null>(null);
  const camera = useRef({ x: WORLD / 2, y: WORLD / 2, zoom: 0.55 });
  const frame = useRef<Frame | null>(null);
  const [animator] = useState(() => new DivisionAnimator());
  const appliedFrame = useRef<Frame | null>(null);
  const appliedProjectionLayer = useRef("");
  const appliedView = useRef("");
  const resetView = useRef<(() => void) | null>(null);
  const zoomView = useRef<((steps: number) => void) | null>(null);
  const previousZoomStep = useRef(props.zoomStep ?? 0);
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
    if (!instance) return;
    const layer = live.current.layer;
    if (!force && next === appliedFrame.current && layer === appliedProjectionLayer.current) return;
    try {
      if (!next) {
        if (appliedFrame.current) instance.set_geometry(emptyCells, emptyLinks);
        appliedFrame.current = null;
        return;
      }
      const field = layer === "nitrates" ? next.nitrates : layer === "covered" ? next.coveredFood : null;
      const terrain = field ? next.terrain.slice() : next.terrain;
      if (field) {
        for (let tile = 0; tile < terrain.length / 4; tile++) {
          terrain[tile * 4 + 2] = field[tile] ?? 0;
        }
      }
      instance.set_frame(animator.sample(performance.now()) ?? next.cells, terrain, next.links, worldWidth, worldSize);
      appliedFrame.current = next;
      appliedProjectionLayer.current = layer;
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

  // Rust owns repaint. Between authoritative snapshots, only active division
  // tweens send draw geometry without reuploading the terrain.
  useEffect(() => {
    const next = props.frame;
    frame.current = next;
    animator.update(next, performance.now(), renderer.current !== null && divisionMotionEnabled());
    applyFrame();
    if (!next) return;
    return playDivisionAnimation(animator, cells => {
      const instance = renderer.current;
      if (!instance || frame.current !== next) return;
      try {
        instance.set_geometry(cells, next.links);
      } catch (e) {
        animator.update(next, performance.now(), false);
        live.current.onError(`Renderer: ${String(e)}`);
      }
    });
  }, [props.frame]);
  useEffect(() => {
    applyView();
    applyFrame();
  }, [props.layer, props.selected]);
  useEffect(() => {
    resetView.current?.();
  }, [props.recenter]);
  useEffect(() => {
    const next = props.zoomStep ?? 0;
    const delta = next - previousZoomStep.current;
    previousZoomStep.current = next;
    if (delta) zoomView.current?.(delta);
  }, [props.zoomStep]);

  useEffect(() => {
    const el = host.current!;
    let disposed = false;
    const mountCanvas = () => {
      const value = document.createElement("canvas");
      value.style.width = "100%";
      value.style.height = "100%";
      value.setAttribute("aria-label", "Поле с клетками");
      el.appendChild(value);
      return value;
    };
    const canvas = mountCanvas();

    const measure = () => ({ width: el.clientWidth, height: el.clientHeight });
    let viewport = measure();
    const reset = () => {
      viewport = measure();
      Object.assign(camera.current, fitDishCamera(viewport, worldSize, worldWidth));
      applyView(true);
      reportView(true);
    };
    resetView.current = reset;
    zoomView.current = (steps) => {
      let anchor = { x: camera.current.x, y: camera.current.y };
      const data = live.current.frame?.cells;
      if (live.current.selected !== null && data) {
        for (let i = 0; i < data.length; i += STRIDE) {
          if (data[i] === live.current.selected) {
            anchor = { x: data[i + 1], y: data[i + 2] };
            break;
          }
        }
      }
      Object.assign(camera.current, zoomDishCamera(camera.current, 1.5 ** steps, anchor, measure(), worldSize, worldWidth));
      applyView(true);
      reportView(true);
    };
    reset();
    const resizeObserver = new ResizeObserver(() => {
      const next = measure();
      if (!next.width || !next.height || (next.width === viewport.width && next.height === viewport.height)) return;
      Object.assign(camera.current, resizeDishCamera(camera.current, viewport, next, worldSize, worldWidth));
      viewport = next;
      applyView(true);
      reportView(true);
    });
    resizeObserver.observe(el);

    let removeControls = () => {};
    const addControls = () => {
      removeControls = bindCanvasControls(canvas, {
        camera: camera.current,
        worldSize, worldWidth,
        tool: () => live.current.tool,
        mouseTool: () => live.current.tool === "move" ? "move" : "place",
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
      zoomView.current = null;
      resizeObserver.disconnect();
      removeControls();
      canvas.remove();
      const instance = renderer.current;
      renderer.current = null;
      if (instance) {
        instance.destroy();
        instance.free();
      }
    };
  }, [worldSize, worldWidth]);
  return <div className="world" ref={host} />;
}
