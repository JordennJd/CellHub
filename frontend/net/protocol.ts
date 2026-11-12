//! Wire constants and binary chunk decoding for the optional server mode.

export const PROTOCOL_VERSION = 5;
export const CHUNK_SIZE = 512;
export const CHUNKS_PER_AXIS = 6;
export const CHUNK_COUNT = CHUNKS_PER_AXIS * CHUNKS_PER_AXIS;
export const TERRAIN_SIZE = 48;
export const TERRAIN_TILES_PER_CHUNK = TERRAIN_SIZE / CHUNKS_PER_AXIS;
export const CELL_STRIDE = 29;
export const TERRAIN_STRIDE = 6;
/** Client frame layout matches the renderer and the local model: 12 floats. */
export const RENDER_STRIDE = 12;
const BIN_CHUNK = 2;

export type RoomMode = "sandbox" | "arena";

/** Chaos scenario sent by the room creator so arena inherits the Chaos world. */
export type ChaosPreset = {
  genome: number[];
  organisms: number[];
};

export type RoomInfo = {
  id: string;
  name: string;
  population: number;
  players: number;
  tick: number;
  host: string;
  mode: RoomMode;
};

export type PlayerInfo = { id: number; name: string; color: number };

export type LeaderboardEntry = {
  id: number;
  name: string;
  color: number;
  score: number;
};

export type ServerControl =
  | { type: "rooms"; rooms: RoomInfo[] }
  | {
      type: "welcome";
      protocol: number;
      player: PlayerInfo;
      room: RoomInfo;
      host: boolean;
    }
  | { type: "ack"; seq: number; cell_id?: number; error?: string }
  | {
      type: "inspect";
      seq: number;
      id: number;
      genome?: number[];
      mode?: number;
    }
  | { type: "error"; message: string; seq?: number }
  | { type: "notice"; message: string }
  | { type: "player_count"; players: number }
  | { type: "leaderboard"; entries: LeaderboardEntry[] }
  | { type: "world_reset"; tick: number }
  | { type: "pong"; t: number }
  | { type: "room_closed"; reason: string };

export type ChunkFrame = {
  tick: number;
  population: number;
  births: number;
  deaths: number;
  infections: number;
  chunkX: number;
  chunkY: number;
  cells: Float32Array;
  links: Uint32Array;
  terrain: Float32Array;
};

/** Decode one little-endian server chunk. Returns null for malformed input. */
export function decodeChunk(buffer: ArrayBuffer): ChunkFrame | null {
  if (buffer.byteLength < 31) return null;
  const view = new DataView(buffer);
  if (view.getUint8(0) !== BIN_CHUNK) return null;
  const tick = view.getUint32(1, true);
  const population = view.getUint32(5, true);
  const births = view.getUint32(9, true);
  const deaths = view.getUint32(13, true);
  const infections = view.getUint32(17, true);
  const chunkX = view.getUint8(21);
  const chunkY = view.getUint8(22);
  const cellCount = view.getUint16(23, true);
  const linkCount = view.getUint16(25, true);
  const tileCount = view.getUint16(27, true);
  const sparse = view.getUint8(29) === 1;
  if (chunkX >= CHUNKS_PER_AXIS || chunkY >= CHUNKS_PER_AXIS ||
      view.getUint8(29) > 1 || view.getUint8(30) !== 0) return null;
  let offset = 31;
  if (buffer.byteLength < offset + cellCount * (sparse ? 4 : CELL_STRIDE * 4)) return null;
  const cells = new Float32Array(cellCount * CELL_STRIDE);
  for (let row = 0; row < cellCount; row++) {
    if (sparse && offset + 4 > buffer.byteLength) return null;
    const mask = sparse ? view.getUint32(offset, true) : (1 << CELL_STRIDE) - 1;
    if (sparse) offset += 4;
    if (mask >>> CELL_STRIDE) return null;
    for (let i = 0; i < CELL_STRIDE; i++) {
      if (mask & (1 << i)) {
        if (offset + 4 > buffer.byteLength) return null;
        cells[row * CELL_STRIDE + i] = view.getFloat32(offset, true);
        offset += 4;
      }
    }
  }
  const linksStart = offset;
  const terrainStart = linksStart + linkCount * 2 * 4;
  if (buffer.byteLength !== terrainStart + tileCount * TERRAIN_STRIDE * 4) return null;
  const readFloats = (offset: number, count: number) => {
    const out = new Float32Array(count);
    for (let i = 0; i < count; i++) out[i] = view.getFloat32(offset + i * 4, true);
    return out;
  };
  const links = new Uint32Array(linkCount * 2);
  for (let i = 0; i < links.length; i++)
    links[i] = view.getUint32(linksStart + i * 4, true);
  return {
    tick,
    population,
    births,
    deaths,
    infections,
    chunkX,
    chunkY,
    cells,
    links,
    terrain: readFloats(terrainStart, tileCount * TERRAIN_STRIDE),
  };
}

export function chunkKey(x: number, y: number) {
  return y * CHUNKS_PER_AXIS + x;
}
