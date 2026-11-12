/** Bounded swept-circle contacts for presentation prediction, never biology.
 * Broad phase uses swept bounds, so even a slow display frame cannot tunnel.
 * On a pathological pile we keep the last poses rather than predict through it.
 */
export type ContactBody = {
  id: number; x: number; y: number; radius: number; mass: number;
  details: Float32Array;
};
export type Pose = { x: number; y: number };
const BUCKET = 64;
const MAX_ENTRIES = 100_000;
const MAX_CHECKS = 100_000;
const WORLD = 3072;

export function clampBody(body: ContactBody) {
  const r = Math.min(WORLD / 2, Math.max(0, body.radius));
  body.x = Math.max(r, Math.min(WORLD - r, body.x));
  body.y = Math.max(r, Math.min(WORLD - r, body.y));
}

export function solveContacts(bodies: ContactBody[], starts: Pose[]): boolean {
  let checks = 0, entries = 0;
  const grid = new Map<number, number[]>();
  const bounds = bodies.map((body, i) => {
    const start = starts[i], r = body.radius;
    return [Math.max(0, Math.floor((Math.min(start.x, body.x) - r) / BUCKET)),
      Math.min(47, Math.floor((Math.max(start.x, body.x) + r) / BUCKET)),
      Math.max(0, Math.floor((Math.min(start.y, body.y) - r) / BUCKET)),
      Math.min(47, Math.floor((Math.max(start.y, body.y) + r) / BUCKET))];
  });
  const rollback = () => { bodies.forEach((b, i) => { b.x = starts[i].x; b.y = starts[i].y; }); return false; };
  for (let i = 0; i < bodies.length; i++) {
    const [x0, x1, y0, y1] = bounds[i];
    entries += (x1 - x0 + 1) * (y1 - y0 + 1);
    if (entries > MAX_ENTRIES) return rollback();
    for (let y = y0; y <= y1; y++) for (let x = x0; x <= x1; x++) {
      const key = y * 48 + x;
      const bucket = grid.get(key);
      // A piled spawn is not a useful prediction island. Leave it to the
      // authority instead of spending a frame on quadratically many pairs.
      if (bucket && bucket.length >= 96) return rollback();
      if (bucket) bucket.push(i); else grid.set(key, [i]);
    }
  }
  // Revisit contacts after neighbours and bonds displace each other.
  for (let pass = 0; pass < 3; pass++) {
    let changed = false;
    for (const [bucketKey, bucket] of grid) for (let u = 0; u < bucket.length; u++) for (let v = u + 1; v < bucket.length; v++) {
      const i = bucket[u], j = bucket[v];
      // Visit a pair only in the first shared bucket, without a per-pair Set.
      if (bucketKey !== Math.max(bounds[i][0], bounds[j][0]) + Math.max(bounds[i][2], bounds[j][2]) * 48) continue;
      if (++checks > MAX_CHECKS) return rollback();
      const a = bodies[i], b = bodies[j], sa = starts[i], sb = starts[j];
      const radius = a.radius + b.radius;
      if (Math.min(sa.x, a.x) - Math.max(sb.x, b.x) > radius ||
          Math.min(sb.x, b.x) - Math.max(sa.x, a.x) > radius ||
          Math.min(sa.y, a.y) - Math.max(sb.y, b.y) > radius ||
          Math.min(sb.y, b.y) - Math.max(sa.y, a.y) > radius) continue;
      const invA = a.details[12] ? 0 : 1 / Math.max(.001, a.mass);
      const invB = b.details[12] ? 0 : 1 / Math.max(.001, b.mass);
      const sum = invA + invB;
      if (!sum) continue;
      let dx = b.x - a.x, dy = b.y - a.y;
      let distance = Math.hypot(dx, dy);
      let nx = distance > 1e-6 ? dx / distance : 1;
      let ny = distance > 1e-6 ? dy / distance : 0;
      let error = radius - distance;
      if (pass === 0) {
        // Earliest contact along the relative trajectory; keep the endpoints
        // on the original side of the impact plane even if they crossed fully.
        const px = sb.x - sa.x, py = sb.y - sa.y;
        const vx = dx - px, vy = dy - py;
        const aa = vx * vx + vy * vy;
        const bb = 2 * (px * vx + py * vy);
        const cc = px * px + py * py - radius * radius;
        const discriminant = bb * bb - 4 * aa * cc;
        if (cc < 0 && px * px + py * py > 1e-12) {
          // Independently refreshed regions may already overlap. Preserve
          // their original side instead of letting a drive escape through.
          const initialDistance = Math.hypot(px, py);
          nx = px / initialDistance; ny = py / initialDistance;
          error = radius - dx * nx - dy * ny;
        }
        if (cc >= -1e-6 && aa > 1e-12 && bb < 0 && discriminant >= 0) {
          const time = (-bb - Math.sqrt(discriminant)) / (2 * aa);
          if (time >= -1e-6 && time <= 1) {
            const hx = px + vx * Math.max(0, time), hy = py + vy * Math.max(0, time);
            const length = Math.hypot(hx, hy);
            if (length > 1e-6) { nx = hx / length; ny = hy / length; error = radius - dx * nx - dy * ny; }
          }
        }
      }
      if (error <= 0) continue;
      changed = true;
      a.x -= nx * error * invA / sum; a.y -= ny * error * invA / sum;
      b.x += nx * error * invB / sum; b.y += ny * error * invB / sum;
      clampBody(a); clampBody(b);
    }
    if (!changed) break;
  }
  return true;
}
