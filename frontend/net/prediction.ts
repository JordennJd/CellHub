//! Client-side predictive scene.
//!
//! The server owns biology, genomes and the tick counter. This layer only
//! predicts the shared scene between authoritative chunks: cells coast along
//! their last known heading, adhesion constraints are solved locally, and every
//! chunk update overwrites the affected cells unconditionally.
//!
//! Local spawns are shown immediately as temporary negative-ID ghosts. When
//! the server accepts a spawn, the ghost disappears as soon as the real cell
//! arrives in a chunk (or as soon as the acknowledgement arrives, if the cell
//! is already known).

import { MODE_STRIDE, type Frame } from "../model";
import {
  CHUNK_COUNT,
  CHUNK_SIZE,
  CHUNKS_PER_AXIS,
  CELL_STRIDE,
  RENDER_STRIDE,
  TERRAIN_SIZE,
  TERRAIN_TILES_PER_CHUNK,
  chunkKey,
  type ChunkFrame,
} from "./protocol";

const WORLD = 3072;
const SIZE_MARGIN = 12;
const PREDICTION_HORIZON_MS = 1600;
const SOLVER_PASSES = 12;

type SceneCell = {
  id: number;
  x: number;
  y: number;
  radius: number;
  kind: number;
  mass: number;
  age: number;
  generation: number;
  mutations: number;
  angle: number;
  speed: number;
  linked: boolean;
  syncedAt: number;
};

type GhostCell = SceneCell & { seq: number };

export type SyncState = {
  seen: number;
  total: number;
  lastTick: number;
  population: number;
};

export class PredictionWorld {
  private cells = new Map<number, SceneCell>();
  private ghosts = new Map<number, GhostCell>();
  private links = new Map<string, [number, number]>();
  private linksByChunk = new Map<number, Set<string>>();
  private lastSeen = new Map<number, number>();
  private seenChunks = new Set<number>();
  private pendingBySeq = new Map<number, number>();
  private realToGhost = new Map<number, number>();
  private nextGhostId = -1;
  private lastTick = 0;
  private population = 0;
  private births = 0;
  private deaths = 0;
  private infections = 0;
  private terrain = new Float32Array(TERRAIN_SIZE * TERRAIN_SIZE * 4);
  private stats = new Float32Array(5);

  applyChunk(chunk: ChunkFrame, now = performance.now()) {
    const cells = chunk.cells;
    if (cells.length % CELL_STRIDE !== 0) return;
    const key = chunkKey(chunk.chunkX, chunk.chunkY);
    this.seenChunks.add(key);
    this.lastTick = Math.max(this.lastTick, chunk.tick);
    this.population = chunk.population;
    this.births = chunk.births;
    this.deaths = chunk.deaths;
    this.infections = chunk.infections;
    this.stats[0] = chunk.tick;
    this.stats[1] = chunk.population;
    this.stats[2] = chunk.births;
    this.stats[3] = chunk.deaths;
    this.stats[4] = chunk.infections;
    this.copyTerrain(chunk);
    for (let i = 0; i < cells.length; i += CELL_STRIDE) {
      const id = Math.trunc(cells[i]);
      if (!(id > 0)) continue;
      let cell = this.cells.get(id);
      if (!cell) {
        cell = {
          id,
          x: cells[i + 1],
          y: cells[i + 2],
          radius: cells[i + 3],
          kind: cells[i + 4],
          mass: cells[i + 5],
          age: cells[i + 6],
          generation: cells[i + 7],
          mutations: cells[i + 8],
          angle: cells[i + 9],
          speed: cells[i + 10] || 0,
          linked: cells[i + 11] !== 0,
          syncedAt: now,
        };
        this.cells.set(id, cell);
      } else {
        cell.x = cells[i + 1];
        cell.y = cells[i + 2];
        cell.radius = cells[i + 3];
        cell.kind = cells[i + 4];
        cell.mass = cells[i + 5];
        cell.age = cells[i + 6];
        cell.generation = cells[i + 7];
        cell.mutations = cells[i + 8];
        cell.angle = cells[i + 9];
        cell.speed = cells[i + 10] || 0;
        cell.linked = cells[i + 11] !== 0;
        cell.syncedAt = now;
      }
      this.lastSeen.set(id, chunk.tick);
      const ghostId = this.realToGhost.get(id);
      if (ghostId !== undefined) this.removeGhost(ghostId);
    }
    this.replaceChunkLinks(key, chunk.links);
    if (chunk.chunkX === 0 && chunk.chunkY === 0)
      this.prune(chunk.tick, now);
  }

  private copyTerrain(chunk: ChunkFrame) {
    const terrain = chunk.terrain;
    if (terrain.length !== TERRAIN_TILES_PER_CHUNK * TERRAIN_TILES_PER_CHUNK * 4)
      return;
    const startX = chunk.chunkX * TERRAIN_TILES_PER_CHUNK;
    const startY = chunk.chunkY * TERRAIN_TILES_PER_CHUNK;
    let source = 0;
    for (let y = 0; y < TERRAIN_TILES_PER_CHUNK; y++) {
      for (let x = 0; x < TERRAIN_TILES_PER_CHUNK; x++) {
        const target = ((startY + y) * TERRAIN_SIZE + startX + x) * 4;
        this.terrain[target] = terrain[source++];
        this.terrain[target + 1] = terrain[source++];
        this.terrain[target + 2] = terrain[source++];
        this.terrain[target + 3] = terrain[source++];
      }
    }
  }

  private replaceChunkLinks(key: number, endpoints: Uint32Array) {
    const previous = this.linksByChunk.get(key);
    if (previous) {
      for (const link of previous) this.links.delete(link);
    }
    const next = new Set<string>();
    for (let i = 0; i + 1 < endpoints.length; i += 2) {
      const a = endpoints[i];
      const b = endpoints[i + 1];
      if (a === b) continue;
      const keyText = a < b ? `${a}:${b}` : `${b}:${a}`;
      next.add(keyText);
      this.links.set(keyText, a < b ? [a, b] : [b, a]);
    }
    this.linksByChunk.set(key, next);
  }

  /** Remove cells that have not appeared in any authoritative chunk for a
   * full cycle. This tolerates entities crossing chunk borders and keeps dead
   * cells visible for at most one extra cycle. */
  private prune(tick: number, now: number) {
    const threshold = tick - CHUNK_COUNT;
    for (const [id, seen] of this.lastSeen) {
      if (seen < threshold) {
        this.cells.delete(id);
        this.lastSeen.delete(id);
      }
    }
    for (const [link, [a, b]] of this.links) {
      if (!this.cells.has(a) || !this.cells.has(b)) this.links.delete(link);
    }
    // Ghosts whose command was never acknowledged must not live forever.
    for (const ghost of this.ghosts.values()) {
      if (now - ghost.syncedAt > 15_000) this.removeGhost(ghost.id);
    }
  }

  predictSpawn(
    seq: number,
    x: number,
    y: number,
    mode: number,
    genome: Float32Array,
    now = performance.now(),
  ) {
    // `flatten` prepends a two-value ABI header before the 19-float rows.
    const index = 2 + mode * MODE_STRIDE;
    if (index < 0 || index + MODE_STRIDE > genome.length) return;
    const kind = Math.round(genome[index]);
    const geneSpeed = genome[index + 5];
    const speed = kind === 2 ? geneSpeed : geneSpeed * 0.15;
    const id = this.nextGhostId--;
    this.pendingBySeq.set(seq, id);
    this.ghosts.set(id, {
      id,
      seq,
      x,
      y,
      radius: 10,
      kind,
      mass: 1,
      age: 0,
      generation: 0,
      mutations: 0,
      angle: 0,
      speed,
      linked: false,
      syncedAt: now,
    });
  }

  applyAck(seq: number, cellId?: number, error?: string) {
    const ghostId = this.pendingBySeq.get(seq);
    if (ghostId === undefined) return;
    if (error || cellId === undefined) {
      this.removeGhost(ghostId);
      return;
    }
    this.pendingBySeq.delete(seq);
    this.realToGhost.set(cellId, ghostId);
    if (this.cells.has(cellId)) this.removeGhost(ghostId);
  }

  private removeGhost(id: number) {
    const ghost = this.ghosts.get(id);
    if (!ghost) return;
    this.ghosts.delete(id);
    this.pendingBySeq.delete(ghost.seq);
    for (const [real, temporary] of this.realToGhost) {
      if (temporary === id) this.realToGhost.delete(real);
    }
  }

  /** Advance the local prediction by a short wall-clock delta. */
  tick(dt: number, now = performance.now()) {
    const step = Math.min(0.1, Math.max(0, dt));
    if (step === 0) return;
    const horizon = now - PREDICTION_HORIZON_MS;
    for (const cell of this.cells.values()) {
      if (cell.syncedAt < horizon || cell.speed === 0) continue;
      cell.x = Math.min(WORLD - SIZE_MARGIN, Math.max(SIZE_MARGIN, cell.x + Math.cos(cell.angle) * cell.speed * step));
      cell.y = Math.min(WORLD - SIZE_MARGIN, Math.max(SIZE_MARGIN, cell.y + Math.sin(cell.angle) * cell.speed * step));
    }
    for (const ghost of this.ghosts.values()) {
      if (ghost.speed === 0) continue;
      ghost.x = Math.min(WORLD - SIZE_MARGIN, Math.max(SIZE_MARGIN, ghost.x + Math.cos(ghost.angle) * ghost.speed * step));
      ghost.y = Math.min(WORLD - SIZE_MARGIN, Math.max(SIZE_MARGIN, ghost.y + Math.sin(ghost.angle) * ghost.speed * step));
    }
    if (!this.links.size) return;
    for (let pass = 0; pass < SOLVER_PASSES; pass++) {
      for (const [aId, bId] of this.links.values()) {
        const a = this.cells.get(aId);
        const b = this.cells.get(bId);
        if (!a || !b) continue;
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const distance = Math.hypot(dx, dy);
        const nx = distance > 0.0001 ? dx / distance : 1;
        const ny = distance > 0.0001 ? dy / distance : 0;
        const error = distance - (a.radius + b.radius + 2);
        const shareA = b.mass / (a.mass + b.mass);
        const shareB = 1 - shareA;
        a.x = Math.min(WORLD - SIZE_MARGIN, Math.max(SIZE_MARGIN, a.x + nx * error * shareA));
        a.y = Math.min(WORLD - SIZE_MARGIN, Math.max(SIZE_MARGIN, a.y + ny * error * shareA));
        b.x = Math.min(WORLD - SIZE_MARGIN, Math.max(SIZE_MARGIN, b.x - nx * error * shareB));
        b.y = Math.min(WORLD - SIZE_MARGIN, Math.max(SIZE_MARGIN, b.y - ny * error * shareB));
      }
    }
  }

  frame(): Frame {
    const count = this.cells.size + this.ghosts.size;
    const cells = new Float32Array(count * RENDER_STRIDE);
    let offset = 0;
    for (const cell of this.cells.values()) {
      offset = writeCell(cells, offset, cell);
    }
    for (const ghost of this.ghosts.values()) {
      offset = writeCell(cells, offset, ghost);
    }
    const links: number[] = [];
    for (const [a, b] of this.links.values()) {
      if (this.cells.has(a) && this.cells.has(b)) links.push(a, b);
    }
    return {
      type: "frame",
      cells,
      links: Uint32Array.from(links),
      terrain: this.terrain,
      stats: this.stats,
    };
  }

  syncState(): SyncState {
    return {
      seen: this.seenChunks.size,
      total: CHUNK_COUNT,
      lastTick: this.lastTick,
      population: this.population,
    };
  }

  applyReset() {
    this.cells.clear();
    this.ghosts.clear();
    this.links.clear();
    this.linksByChunk.clear();
    this.lastSeen.clear();
    this.seenChunks.clear();
    this.pendingBySeq.clear();
    this.realToGhost.clear();
    this.lastTick = 0;
    this.population = 0;
    this.births = 0;
    this.deaths = 0;
    this.infections = 0;
    this.stats.fill(0);
  }

  get cellCount() {
    return this.cells.size + this.ghosts.size;
  }

  get chunkSize() {
    return CHUNK_SIZE;
  }

  get chunksPerAxis() {
    return CHUNKS_PER_AXIS;
  }
}

function writeCell(cells: Float32Array, offset: number, cell: SceneCell) {
  cells[offset] = cell.id;
  cells[offset + 1] = cell.x;
  cells[offset + 2] = cell.y;
  cells[offset + 3] = cell.radius;
  cells[offset + 4] = cell.kind;
  cells[offset + 5] = cell.mass;
  cells[offset + 6] = cell.age;
  cells[offset + 7] = cell.generation;
  cells[offset + 8] = cell.mutations;
  cells[offset + 9] = cell.angle;
  cells[offset + 10] = cell.speed;
  cells[offset + 11] = cell.linked ? 1 : 0;
  return offset + RENDER_STRIDE;
}
