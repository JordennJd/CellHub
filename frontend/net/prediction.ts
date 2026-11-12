//! Client-side predictive scene.
//!
//! The server owns biology, genomes and the tick counter. This layer only
//! predicts the shared scene between authoritative chunks: cells coast along
//! their last known heading; contacts and adhesion are solved locally. Newer
//! authority replaces prediction immediately. Biology never runs here.
//!
//! Local spawns are shown immediately as temporary negative-ID ghosts. When
//! the server acknowledges a spawn, the ghost is removed. Only authority may
//! then introduce the real cell or its descendants.

import { clampBody, solveContacts } from "./contacts";
import { CELL_DETAILS_STRIDE, MODE_STRIDE, TICK_RATE, type Frame } from "../model";
import {
  CHUNK_COUNT,
  SYNC_CYCLE_TICKS,
  CHUNK_SIZE,
  CHUNKS_PER_AXIS,
  CELL_STRIDE,
  RENDER_STRIDE,
  TERRAIN_SIZE,
  TERRAIN_STRIDE,
  TERRAIN_TILES_PER_CHUNK,
  chunkKey,
  type ChunkFrame,
} from "./protocol";

const PREDICTION_HORIZON_MS = (SYNC_CYCLE_TICKS + 1) * 1000 / TICK_RATE;
const SOLVER_PASSES = 12;
const CONSTRAINT_BUDGET = 12_000;

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
  color: number;
  mirrored: number;
  nitrogen: number;
  owner: number;
  details: Float32Array;
  syncedAt: number;
  predictedAt: number;
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
  private linkOwners = new Map<string, { chunk: number; tick: number }>();
  private linksByChunk = new Map<number, Set<string>>();
  private lastSeen = new Map<number, number>();
  private seenChunks = new Set<number>();
  private chunkTicks = new Map<number, number>();
  private pendingBySeq = new Map<number, number>();
  private nextGhostId = -1;
  private lastTick = 0;
  private population = 0;
  private births = 0;
  private deaths = 0;
  private infections = 0;
  private terrain = new Float32Array(TERRAIN_SIZE * TERRAIN_SIZE * 4);
  private nitrates = new Float32Array(TERRAIN_SIZE * TERRAIN_SIZE);
  private coveredFood = new Float32Array(TERRAIN_SIZE * TERRAIN_SIZE);
  private terrainRevision = 0;
  private stats = new Float32Array(5);

  applyChunk(chunk: ChunkFrame, now = performance.now()) {
    const cells = chunk.cells;
    if (cells.length % CELL_STRIDE !== 0) return;
    const key = chunkKey(chunk.chunkX, chunk.chunkY);
    if (chunk.tick <= (this.chunkTicks.get(key) ?? -1)) return;
    this.chunkTicks.set(key, chunk.tick);
    this.seenChunks.add(key);
    this.lastTick = Math.max(this.lastTick, chunk.tick);
    if (chunk.tick === this.lastTick) this.population = chunk.population;
    if (chunk.tick === this.lastTick) {
      this.births = chunk.births; this.deaths = chunk.deaths; this.infections = chunk.infections;
      this.stats.set([chunk.tick, chunk.population, chunk.births, chunk.deaths, chunk.infections]);
    }
    this.copyTerrain(chunk);
    for (let i = 0; i < cells.length; i += CELL_STRIDE) {
      const id = Math.trunc(cells[i]);
      if (!(id > 0)) continue;
      if (chunk.tick < (this.lastSeen.get(id) ?? -1)) continue;
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
          color: cells[i + 12],
          mirrored: cells[i + 13],
          nitrogen: cells[i + 14],
          owner: cells[i + 28],
          details: cells.slice(i + 15, i + 28),
          syncedAt: now,
          predictedAt: now,
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
        cell.color = cells[i + 12];
        cell.mirrored = cells[i + 13];
        cell.nitrogen = cells[i + 14];
        cell.owner = cells[i + 28];
        cell.details.set(cells.subarray(i + 15, i + 28));
        cell.syncedAt = now;
        cell.predictedAt = now;
      }
      this.lastSeen.set(id, chunk.tick);
    }
    this.replaceChunkLinks(key, chunk.links, chunk.tick);
    if (chunk.chunkX === 0 && chunk.chunkY === 0)
      this.prune(chunk.tick, now);
  }

  private copyTerrain(chunk: ChunkFrame) {
    const terrain = chunk.terrain;
    if (terrain.length !== TERRAIN_TILES_PER_CHUNK * TERRAIN_TILES_PER_CHUNK * TERRAIN_STRIDE)
      return;
    const startX = chunk.chunkX * TERRAIN_TILES_PER_CHUNK;
    const startY = chunk.chunkY * TERRAIN_TILES_PER_CHUNK;
    let source = 0;
    let changed = false;
    for (let y = 0; y < TERRAIN_TILES_PER_CHUNK; y++) {
      for (let x = 0; x < TERRAIN_TILES_PER_CHUNK; x++) {
        const tile = (startY + y) * TERRAIN_SIZE + startX + x;
        const target = tile * 4;
        for (let field = 0; field < 4; field++) {
          const value = terrain[source++];
          changed ||= this.terrain[target + field] !== value;
          this.terrain[target + field] = value;
        }
        const nitrate = terrain[source++];
        const coveredFood = terrain[source++];
        changed ||= this.nitrates[tile] !== nitrate || this.coveredFood[tile] !== coveredFood;
        this.nitrates[tile] = nitrate;
        this.coveredFood[tile] = coveredFood;
      }
    }
    if (changed) this.terrainRevision++;
  }

  private replaceChunkLinks(key: number, endpoints: Uint32Array, tick: number) {
    const previous = this.linksByChunk.get(key);
    if (previous) {
      for (const link of previous) {
        if (this.linkOwners.get(link)?.chunk === key) {
          this.links.delete(link); this.linkOwners.delete(link);
        }
      }
    }
    const next = new Set<string>();
    for (let i = 0; i + 1 < endpoints.length; i += 2) {
      const a = endpoints[i];
      const b = endpoints[i + 1];
      if (a === b) continue;
      const keyText = a < b ? `${a}:${b}` : `${b}:${a}`;
      if ((this.linkOwners.get(keyText)?.tick ?? -1) > tick) continue;
      next.add(keyText);
      this.linkOwners.set(keyText, { chunk: key, tick });
      this.links.set(keyText, a < b ? [a, b] : [b, a]);
    }
    this.linksByChunk.set(key, next);
  }

  /** Remove cells that have not appeared in any authoritative chunk for a
   * full cycle. This tolerates entities crossing chunk borders and keeps dead
   * cells visible for at most one extra cycle. */
  private prune(tick: number, now: number) {
    const threshold = tick - SYNC_CYCLE_TICKS;
    for (const [id, seen] of this.lastSeen) {
      if (seen < threshold) {
        this.cells.delete(id);
        this.lastSeen.delete(id);
      }
    }
    for (const [link, [a, b]] of this.links) {
      if (!this.cells.has(a) || !this.cells.has(b)) { this.links.delete(link); this.linkOwners.delete(link); }
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
    owner = 0,
    now = performance.now(),
  ) {
    // `flatten` prepends a two-value ABI header before the mode rows.
    const index = 2 + mode * MODE_STRIDE;
    if (index < 0 || index + MODE_STRIDE > genome.length) return;
    const kind = Math.round(genome[index]);
    const geneSpeed = genome[index + 5];
    const speed = kind === 2 ? geneSpeed : geneSpeed * 0.15;
    const [r, g, b] = genome.slice(index + 16, index + 19);
    const color = r < 0 ? -1 : (Math.round(r * 255) << 16) | (Math.round(g * 255) << 8) | Math.round(b * 255);
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
      color,
      mirrored: 0,
      nitrogen: 1,
      owner,
      details: new Float32Array([0,0,0,0,0,0,0,0,0,0,-1,mode,0]),
      syncedAt: now,
      predictedAt: now,
    });
  }

  applyAck(seq: number, cellId?: number, error?: string) {
    const ghostId = this.pendingBySeq.get(seq);
    if (ghostId === undefined) return;
    if (error || cellId === undefined) {
      this.removeGhost(ghostId);
      return;
    }
    // The accepted cell may already have divided before a slow client sees
    // its region. Never keep a second temporary parent waiting for that ID.
    this.removeGhost(ghostId);
  }

  private removeGhost(id: number) {
    const ghost = this.ghosts.get(id);
    if (!ghost) return;
    this.ghosts.delete(id);
    this.pendingBySeq.delete(ghost.seq);
  }

  /** Advance the local prediction by a short wall-clock delta. */
  tick(dt: number, now = performance.now()) {
    if (!(dt > 0)) return;
    for (const ghost of this.ghosts.values()) if (now - ghost.syncedAt > 15_000) this.removeGhost(ghost.id);
    const bodies = [...this.cells.values(), ...this.ghosts.values()];
    const starts = bodies.map(cell => ({ x: cell.x, y: cell.y }));
    // Large colonies stay at authoritative positions when the constraint solver
    // cannot fit even one full pass. Biology always remains on the server.
    const passes = Math.min(SOLVER_PASSES, Math.floor(CONSTRAINT_BUDGET / Math.max(1, this.links.size)));
    for (const cell of bodies) {
      // Use each cell's receipt clock, not render-frame count. No fixed-step
      // catch-up queue: new snapshots replace local poses immediately.
      const end = Math.min(now, cell.syncedAt + PREDICTION_HORIZON_MS);
      const step = Math.max(0, (end - cell.predictedAt) / 1000);
      cell.predictedAt = now;
      if (!step || cell.details[12] || (cell.linked && passes === 0)) continue;
      cell.x += Math.cos(cell.angle) * cell.speed * step;
      cell.y += Math.sin(cell.angle) * cell.speed * step;
      clampBody(cell);
    }
    for (let pass = 0; pass < passes; pass++) {
      for (const [aId, bId] of this.links.values()) {
        const a = this.cells.get(aId);
        const b = this.cells.get(bId);
        if (!a || !b) continue;
        const anchoredA = a.details[12] !== 0;
        const anchoredB = b.details[12] !== 0;
        if (anchoredA && anchoredB) continue;
        const dx = b.x - a.x;
        const dy = b.y - a.y;
        const distance = Math.hypot(dx, dy);
        const nx = distance > 0.0001 ? dx / distance : 1;
        const ny = distance > 0.0001 ? dy / distance : 0;
        const error = distance - (a.radius + b.radius + 2);
        const shareA = anchoredA ? 0 : anchoredB ? 1 : b.mass / (a.mass + b.mass);
        const shareB = 1 - shareA;
        a.x += nx * error * shareA; a.y += ny * error * shareA;
        b.x -= nx * error * shareB; b.y -= ny * error * shareB;
        clampBody(a); clampBody(b);
      }
    }
    solveContacts(bodies, starts);
  }

  frame(): Frame {
    const count = this.cells.size + this.ghosts.size;
    const cells = new Float32Array(count * RENDER_STRIDE);
    const owners = new Float32Array(count);
    const nitrogen = new Float32Array(count);
    const details = new Float32Array(count * CELL_DETAILS_STRIDE);
    let offset = 0;
    for (const cell of this.cells.values()) {
      owners[offset / RENDER_STRIDE] = cell.owner;
      nitrogen[offset / RENDER_STRIDE] = cell.nitrogen;
      details.set(cell.details, offset / RENDER_STRIDE * CELL_DETAILS_STRIDE);
      offset = writeCell(cells, offset, cell);
    }
    for (const ghost of this.ghosts.values()) {
      owners[offset / RENDER_STRIDE] = ghost.owner;
      nitrogen[offset / RENDER_STRIDE] = ghost.nitrogen;
      details.set(ghost.details, offset / RENDER_STRIDE * CELL_DETAILS_STRIDE);
      offset = writeCell(cells, offset, ghost);
    }
    const links: number[] = [];
    for (const [a, b] of this.links.values()) {
      if (this.cells.has(a) && this.cells.has(b)) links.push(a, b);
    }
    return {
      type: "frame",
      cells,
      owners,
      links: Uint32Array.from(links),
      terrain: this.terrain,
      terrainRevision: this.terrainRevision,
      nitrogen,
      nitrates: this.nitrates,
      details,
      coveredFood: this.coveredFood,
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
    this.linkOwners.clear();
    this.linksByChunk.clear();
    this.lastSeen.clear();
    this.seenChunks.clear();
    this.chunkTicks.clear();
    this.pendingBySeq.clear();
    this.lastTick = 0;
    this.population = 0;
    this.births = 0;
    this.deaths = 0;
    this.infections = 0;
    this.stats.fill(0);
    this.nitrates.fill(0);
    this.coveredFood.fill(0);
    this.terrainRevision++;
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
  cells[offset + 10] = cell.color;
  cells[offset + 11] = cell.mirrored;
  return offset + RENDER_STRIDE;
}
