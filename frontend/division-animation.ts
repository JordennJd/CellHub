import { DIVISION_STRIDE, STRIDE, type Frame } from "./model";

const DURATION = 180;
const TAU = 2 * Math.PI;
type Pose = { x: number; y: number; radius: number; angle: number };
type Tween = { from: Pose; started: number; offset: number };

function cellOffsets(cells: Float32Array) {
  const offsets = new Map<number, number>();
  for (let i = 0; i < cells.length; i += STRIDE) offsets.set(cells[i], i);
  return offsets;
}

function interpolate(tween: Tween, cells: Float32Array, now: number): Pose {
  const { from, offset, started } = tween;
  const fraction = Math.max(0, Math.min(1, (now - started) / DURATION));
  const t = fraction * fraction * (3 - 2 * fraction);
  const delta = cells[offset + 9] - from.angle;
  return {
    x: from.x + (cells[offset + 1] - from.x) * t,
    y: from.y + (cells[offset + 2] - from.y) * t,
    radius: from.radius + (cells[offset + 3] - from.radius) * t,
    angle: from.angle + (delta - TAU * Math.round(delta / TAU)) * t,
  };
}

/** Derive draw-only geometry from observed divisions and authoritative frames. */
export class DivisionAnimator {
  private frame: Frame | null = null;
  private tweens = new Map<number, Tween>();
  private scratch?: Float32Array;

  get active() { return this.tweens.size !== 0; }

  update(frame: Frame | null, now: number, animate = true) {
    const previous = this.frame;
    const ongoing = this.tweens;
    this.frame = frame;
    this.tweens = new Map();
    if (!animate || !previous || !frame || frame.stats[0] < previous.stats[0]) return;
    const events = frame.stats[0] > previous.stats[0] ? frame.divisions : undefined;
    if (!events?.length && !ongoing.size) return;
    const targets = cellOffsets(frame.cells);
    for (const [id, tween] of ongoing) {
      const offset = targets.get(id);
      if (offset !== undefined && now - tween.started < DURATION) {
        this.tweens.set(id, { ...tween, offset });
      }
    }
    if (!events?.length) return;
    const observed = cellOffsets(previous.cells);
    for (let i = 0; i + DIVISION_STRIDE <= events.length; i += DIVISION_STRIDE) {
      const tick = events[i], parent = events[i + 1];
      if (tick <= previous.stats[0] || tick > frame.stats[0] || !observed.has(parent) || targets.has(parent)) continue;
      const priorTween = ongoing.get(parent);
      const from = priorTween && now - priorTween.started < DURATION
        ? interpolate(priorTween, previous.cells, now)
        : { x: events[i + 4], y: events[i + 5], radius: events[i + 6], angle: events[i + 7] };
      for (const id of [events[i + 2], events[i + 3]]) {
        const offset = targets.get(id);
        if (offset !== undefined) this.tweens.set(id, { from, started: now, offset });
      }
    }
  }

  sample(now: number): Float32Array | undefined {
    const cells = this.frame?.cells;
    if (!cells) return;
    for (const [id, tween] of this.tweens) {
      if (now - tween.started >= DURATION) this.tweens.delete(id);
    }
    if (!this.active) return cells;
    if (this.scratch?.length !== cells.length) this.scratch = new Float32Array(cells.length);
    this.scratch.set(cells);
    for (const tween of this.tweens.values()) {
      const { x, y, radius, angle } = interpolate(tween, cells, now);
      this.scratch.set([x, y, radius], tween.offset + 1);
      this.scratch[tween.offset + 9] = angle;
    }
    return this.scratch;
  }
}

export function divisionMotionEnabled() {
  return !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Sample only while geometry is changing. Rendering remains the caller's job. */
export function playDivisionAnimation(animator: DivisionAnimator, draw: (cells: Float32Array) => void) {
  let request: number | undefined;
  const step = (now: number) => {
    const cells = animator.sample(now);
    if (cells) draw(cells);
    request = animator.active ? requestAnimationFrame(step) : undefined;
  };
  if (animator.active) request = requestAnimationFrame(step);
  return () => {
    if (request !== undefined) cancelAnimationFrame(request);
  };
}
