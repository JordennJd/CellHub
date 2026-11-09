import { useEffect, useRef } from "react";
import { Application, Container, Graphics, Text } from "pixi.js";
import { TYPES, WORLD, STRIDE, type Frame } from "./model";
export type ViewState = { zoom: number; x: number; y: number };
type Props = {
  frame: Frame | null;
  tool: string;
  layer: string;
  selected: number | null;
  onPoint: (x: number, y: number, id: number | null) => void;
  onView: (v: ViewState) => void;
  onRenderer: (name: string) => void;
  onError: (message: string) => void;
  recenter: number;
};
export function WorldView(props: Props) {
  const host = useRef<HTMLDivElement>(null);
  const live = useRef(props);
  live.current = props;
  const reset = useRef<(() => void) | null>(null);
  useEffect(() => {
    reset.current?.();
  }, [props.recenter]);
  useEffect(() => {
    let disposed = false;
    const app = new Application();
    let cleanup = () => {};
    (async () => {
      const el = host.current!;
      await app.init({
        resizeTo: el,
        background: 0x091411,
        antialias: true,
        resolution: Math.min(devicePixelRatio, 2),
        autoDensity: true,
        preference: new URLSearchParams(location.search).has("webgpu")
          ? "webgpu"
          : "webgl",
      });
      if (disposed) {
        app.destroy(true);
        return;
      }
      el.appendChild(app.canvas);
      live.current.onRenderer(app.renderer.name);
      const scene = new Container(),
        terrain = new Graphics(),
        dots = new Graphics(),
        cells = new Graphics(),
        labels = new Container();
      scene.addChild(terrain, dots, labels, cells);
      app.stage.addChild(scene);
      const camera = { x: WORLD / 2, y: WORLD / 2, zoom: 0.55 };
      reset.current = () => {
        camera.x = WORLD / 2;
        camera.y = WORLD / 2;
        camera.zoom =
          (Math.min(el.clientWidth, el.clientHeight) / WORLD) * 0.93;
      };
      reset.current();
      const names = [
        ["СОЛНЕЧНОЕ МЕЛКОВОДЬЕ", 470, 650],
        ["ПИТАТЕЛЬНЫЙ ПОЯС", 1750, 1550],
        ["ТЁМНЫЕ ГЛУБИНЫ", 1250, 2600],
        ["РАДИАЦИОННЫЙ КАРМАН", 2330, 680],
      ] as const;
      for (const [text, x, y] of names) {
        const label = new Text({
          text,
          style: {
            fontFamily: "monospace",
            fontSize: 20,
            fill: 0xa4bbae,
            letterSpacing: 3,
          },
        });
        label.position.set(x, y);
        label.alpha = 0.5;
        labels.addChild(label);
      }
      let down: { x: number; y: number; cx: number; cy: number } | null = null;
      let moved = false;
      const point = (e: PointerEvent | WheelEvent) => {
        const r = app.canvas.getBoundingClientRect();
        return { x: e.clientX - r.left, y: e.clientY - r.top };
      };
      const world = (p: { x: number; y: number }) => ({
        x: (p.x - el.clientWidth / 2) / camera.zoom + camera.x,
        y: (p.y - el.clientHeight / 2) / camera.zoom + camera.y,
      });
      const pd = (e: PointerEvent) => {
        const p = point(e);
        down = { ...p, cx: camera.x, cy: camera.y };
        moved = false;
        app.canvas.setPointerCapture(e.pointerId);
      };
      const pm = (e: PointerEvent) => {
        if (!down) return;
        const p = point(e);
        if (Math.hypot(p.x - down.x, p.y - down.y) > 5) moved = true;
        if (moved || live.current.tool === "pan") {
          camera.x = down.cx - (p.x - down.x) / camera.zoom;
          camera.y = down.cy - (p.y - down.y) / camera.zoom;
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
                if (d < Math.max(data[i + 3], 12 / camera.zoom) && d < best) {
                  id = data[i];
                  best = d;
                }
              }
            live.current.onPoint(p.x, p.y, id);
          }
        }
        down = null;
      };
      const wheel = (e: WheelEvent) => {
        e.preventDefault();
        const p = point(e),
          before = world(p);
        camera.zoom = Math.max(
          0.12,
          Math.min(4, camera.zoom * Math.exp(-e.deltaY * 0.001)),
        );
        const after = world(p);
        camera.x += before.x - after.x;
        camera.y += before.y - after.y;
      };
      app.canvas.addEventListener("pointerdown", pd);
      app.canvas.addEventListener("pointermove", pm);
      app.canvas.addEventListener("pointerup", pu);
      app.canvas.addEventListener("pointercancel", () => {
        down = null;
      });
      app.canvas.addEventListener("wheel", wheel, { passive: false });
      let oldFrame: Frame | null = null,
        oldLayer = "";
      let counter = 0;
      app.ticker.add(() => {
        const p = live.current;
        scene.position.set(
          el.clientWidth / 2 - camera.x * camera.zoom,
          el.clientHeight / 2 - camera.y * camera.zoom,
        );
        scene.scale.set(camera.zoom);
        if (counter++ % 15 === 0) p.onView({ ...camera });
        const f = p.frame;
        if (!f) return;
        if (f !== oldFrame || p.layer !== oldLayer) {
          terrain.clear();
          dots.clear();
          for (let j = 0; j < 48; j++)
            for (let i = 0; i < 48; i++) {
              const k = (j * 48 + i) * 4;
              const colors = [0x253e2b, 0x233c38, 0x152d39, 0x3c2940];
              const color =
                p.layer === "light"
                  ? 0xe7cf7d
                  : p.layer === "radiation"
                    ? 0xd28ce9
                    : p.layer === "food"
                      ? 0x75d5bd
                      : colors[f.terrain[k + 3]];
              const alpha =
                p.layer === "biome"
                  ? 1
                  : 0.08 +
                    f.terrain[
                      k +
                        (p.layer === "light"
                          ? 0
                          : p.layer === "radiation"
                            ? 1
                            : 2)
                    ] *
                      0.72;
              terrain.rect(i * 64, j * 64, 64, 64).fill({ color, alpha });
              if (f.terrain[k + 2] > 0.15) {
                dots
                  .circle(
                    i * 64 + 17 + (j % 3) * 9,
                    j * 64 + 27 + (i % 3) * 7,
                    1.8,
                  )
                  .fill({ color: 0xa5cf9b, alpha: f.terrain[k + 2] * 0.5 });
              }
            }
          terrain
            .rect(0, 0, WORLD, WORLD)
            .stroke({ color: 0x7b9889, width: 2 });
          oldLayer = p.layer;
          oldFrame = f;
        }
        cells.clear();
        for (let i = 0; i < f.cells.length; i += STRIDE) {
          const [id, x, y, r, kind, , , , mut, angle] = f.cells.subarray(
            i,
            i + STRIDE,
          );
          const color = TYPES[kind]?.color ?? 0xffffff;
          cells.circle(x, y, r + 3).fill({ color, alpha: 0.08 });
          cells
            .circle(x, y, r)
            .fill({ color, alpha: 0.22 })
            .stroke({ color, width: 1.4 });
          cells
            .circle(
              x + Math.cos(angle) * r * 0.2,
              y + Math.sin(angle) * r * 0.2,
              r * 0.29,
            )
            .fill({ color, alpha: 0.9 });
          if (kind === 2)
            cells
              .moveTo(x - Math.cos(angle) * r, y - Math.sin(angle) * r)
              .lineTo(x - Math.cos(angle) * r * 2, y - Math.sin(angle) * r * 2)
              .stroke({ color, width: 1 });
          if (mut > 0)
            cells.circle(x + r * 0.55, y - r * 0.55, 2).fill(0xffffff);
          if (id === p.selected)
            cells.circle(x, y, r + 7).stroke({ color: 0xffffff, width: 1.5 });
        }
      });
      cleanup = () => {
        reset.current = null;
        app.destroy(true, { children: true });
      };
    })().catch((e) => {
      if (!disposed) live.current.onError(`Renderer: ${String(e)}`);
    });
    return () => {
      disposed = true;
      cleanup();
    };
  }, []);
  return <div className="world" ref={host} />;
}
