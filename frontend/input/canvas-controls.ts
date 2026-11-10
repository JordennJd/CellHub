export type Point = { x: number; y: number };
export type Camera = Point & { zoom: number };
export type CanvasAction = "place" | "select";

type Options = {
  camera: Camera;
  worldSize: number;
  tool: () => string;
  onPoint: (point: Point, action: CanvasAction) => void;
  onView: (final: boolean) => void;
};

// Pointer capture keeps a gesture intact outside the canvas. A multi-touch
// sequence stays a gesture until every finger is lifted, so it cannot plant cells.
export function bindCanvasControls(canvas: HTMLCanvasElement, options: Options) {
  const { camera, worldSize, onView } = options;
  const pointers = new Map<number, Point>();
  let start: (Point & { camera: Camera; action: string; id: number; pointerType: string }) | null = null;
  let holdTimer: ReturnType<typeof setTimeout> | undefined;
  let repeatTimer: ReturnType<typeof setInterval> | undefined;
  let repeated = false;
  let mousePosition: Point | null = null;
  const stopRepeating = () => {
    clearTimeout(holdTimer);
    clearInterval(repeatTimer);
    holdTimer = undefined;
    repeatTimer = undefined;
  };
  let moved = false;
  let multiTouch = false;
  let pinch: { distance: number; zoom: number; anchor: Point } | null = null;
  const local = (event: PointerEvent | WheelEvent): Point => {
    const rect = canvas.getBoundingClientRect();
    return { x: event.clientX - rect.left, y: event.clientY - rect.top };
  };
  const world = (point: Point): Point => ({
    x: (point.x - canvas.clientWidth / 2) / camera.zoom + camera.x,
    y: (point.y - canvas.clientHeight / 2) / camera.zoom + camera.y,
  });
  const placeAtCursor = () => {
    if (!mousePosition || !start || start.action !== "place") return;
    const p = mousePosition;
    if (p.x < 0 || p.y < 0 || p.x > canvas.clientWidth || p.y > canvas.clientHeight) return;
    const point = world(p);
    if (point.x >= 0 && point.y >= 0 && point.x <= worldSize && point.y <= worldSize)
      options.onPoint(point, "place");
  };
  const clampZoom = (zoom: number) => Math.max(
    Math.min(0.12, Math.min(canvas.clientWidth, canvas.clientHeight) / worldSize * 0.93),
    Math.min(4, zoom),
  );
  const pair = () => {
    const [a, b] = [...pointers.values()];
    return { center: { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }, distance: Math.hypot(a.x - b.x, a.y - b.y) };
  };
  const down = (event: PointerEvent) => {
    event.preventDefault();
    const touch = event.pointerType === "touch";
    if (!touch && (event.button < 0 || event.button > 2 || start)) return;
    if (touch && start && !pointers.size) return;
    const point = local(event);
    if (touch) pointers.set(event.pointerId, point);
    canvas.setPointerCapture(event.pointerId);
    if (pointers.size >= 2) {
      multiTouch = true;
      const { center, distance } = pair();
      pinch = { distance: Math.max(distance, 1), zoom: camera.zoom, anchor: world(center) };
      return;
    }
    start = { ...point, camera: { ...camera }, id: event.pointerId, pointerType: event.pointerType,
      action: touch ? options.tool() : event.button === 1 ? "pan" : event.button === 2 ? "select" : "place" };
    moved = false;
    repeated = false;
    if (event.pointerType === "mouse" && event.button === 0) {
      mousePosition = point;
      holdTimer = setTimeout(() => {
        repeated = true;
        placeAtCursor();
        repeatTimer = setInterval(placeAtCursor, 200);
      }, 1000);
    }
  };
  const move = (event: PointerEvent) => {
    if (pointers.has(event.pointerId)) pointers.set(event.pointerId, local(event));
    if (pinch && pointers.size >= 2) {
      event.preventDefault();
      const { center, distance } = pair();
      camera.zoom = clampZoom(pinch.zoom * distance / pinch.distance);
      camera.x = pinch.anchor.x - (center.x - canvas.clientWidth / 2) / camera.zoom;
      camera.y = pinch.anchor.y - (center.y - canvas.clientHeight / 2) / camera.zoom;
      onView(false);
      return;
    }
    if (!start || start.id !== event.pointerId) return;
    event.preventDefault();
    const point = local(event);
    if (start.pointerType === "mouse" && start.action === "place") {
      if (!(event.buttons & 1)) { finish(event, true); return; }
      mousePosition = point;
    }
    if (Math.hypot(point.x - start.x, point.y - start.y) > 5) moved = true;
    if (start.action === "pan" || (event.pointerType === "touch" && moved)) {
      camera.x = start.camera.x - (point.x - start.x) / camera.zoom;
      camera.y = start.camera.y - (point.y - start.y) / camera.zoom;
      onView(false);
    }
  };
  const finish = (event: PointerEvent, cancelled = false) => {
    if (start?.id !== event.pointerId && !pointers.has(event.pointerId)) return;
    event.preventDefault();
    stopRepeating();
    const movingMouseClick = start?.pointerType === "mouse" && start.action === "place";
    if (!cancelled && start?.id === event.pointerId && (!moved || movingMouseClick) && !repeated && !multiTouch && start.action !== "pan") {
      const point = world(local(event));
      if (point.x >= 0 && point.y >= 0 && point.x <= worldSize && point.y <= worldSize)
        options.onPoint(point, start.action === "select" ? "select" : "place");
    }
    pointers.delete(event.pointerId);
    pinch = null;
    if (pointers.size >= 2) {
      const { center, distance } = pair();
      pinch = { distance: Math.max(distance, 1), zoom: camera.zoom, anchor: world(center) };
    } else if (pointers.size === 1) {
      const [id, point] = [...pointers][0];
      start = { ...point, id, camera: { ...camera }, action: "pan", pointerType: "touch" };
      moved = true;
    } else {
      start = null;
      multiTouch = false;
    }
    if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
    onView(true);
  };
  const up = (event: PointerEvent) => finish(event);
  const cancel = (event: PointerEvent) => finish(event, true);
  const wheel = (event: WheelEvent) => {
    event.preventDefault();
    const point = local(event), before = world(point);
    const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? canvas.clientHeight : 1);
    camera.zoom = clampZoom(camera.zoom * Math.exp(-delta * 0.001));
    const after = world(point);
    camera.x += before.x - after.x;
    camera.y += before.y - after.y;
    onView(true);
  };
  const abandon = () => {
    stopRepeating();
    const ids = new Set([...pointers.keys(), ...(start ? [start.id] : [])]);
    start = null;
    pointers.clear();
    pinch = null;
    multiTouch = false;
    for (const id of ids) if (canvas.hasPointerCapture(id)) canvas.releasePointerCapture(id);
  };
  const visibility = () => { if (document.hidden) abandon(); };
  window.addEventListener("blur", abandon);
  document.addEventListener("visibilitychange", visibility);
  const prevent = (event: Event) => event.preventDefault();
  const listeners = { pointerdown: down, pointermove: move, pointerup: up, pointercancel: cancel, lostpointercapture: cancel, wheel };
  for (const [name, handler] of Object.entries(listeners))
    canvas.addEventListener(name, handler as EventListener, { passive: false });
  const defaults = ["contextmenu", "auxclick", "dblclick", "dragstart", "selectstart"];
  defaults.forEach((name) => canvas.addEventListener(name, prevent));
  return () => {
    abandon();
    window.removeEventListener("blur", abandon);
    document.removeEventListener("visibilitychange", visibility);
    for (const [name, handler] of Object.entries(listeners)) canvas.removeEventListener(name, handler as EventListener);
    defaults.forEach((name) => canvas.removeEventListener(name, prevent));
    pointers.clear();
  };
}
