//! Thin browser WebSocket transport around `PredictionWorld`.

import { NetworkProfiler } from "./profiling";
import type { Command, Frame } from "../model";
import { PredictionWorld, type SyncState } from "./prediction";
import {
  PROTOCOL_VERSION,
  CHUNKS_PER_AXIS,
  decodeChunk,
  type ChaosPreset,
  type LeaderboardEntry,
  type RoomInfo,
  type RoomMode,
  type ServerControl,
} from "./protocol";

export type NetPhase = "connecting" | "lobby" | "online" | "disconnected";

export type Welcome = Extract<ServerControl, { type: "welcome" }>;

export type NetCallbacks = {
  onPhase: (phase: NetPhase) => void;
  onRooms: (rooms: RoomInfo[]) => void;
  onWelcome: (welcome: Welcome) => void;
  onFrame: (frame: Frame, sync: SyncState) => void;
  onInspect: (id: number, genome?: number[], mode?: number) => void;
  onError: (message: string) => void;
  onNotice: (message: string) => void;
  onWorldReset: () => void;
  onPlayerCount: (players: number) => void;
  onLeaderboard: (entries: LeaderboardEntry[]) => void;
  onDisconnected: () => void;
};

export class MultiplayerClient {
  private readonly profiler = new NetworkProfiler();
  private socket: WebSocket | null = null;
  private world: PredictionWorld | null = null;
  private sequence = 0;
  // At most one pending snapshot per region. A slow display never replays
  // every intermediate scene packet before it can show the latest authority.
  private pendingChunks = new Map<number, { buffer: ArrayBuffer; tick: number; receivedAt: number }>();
  private loop: number | null = null;
  private lastFrame = 0;
  private lastPing = 0;
  private intentionalClose = false;
  private phase: NetPhase = "disconnected";
  private playerId = 0;
  private readonly callbacks: NetCallbacks;

  constructor(callbacks: NetCallbacks) {
    this.callbacks = callbacks;
  }

  connect(url: string) {
    this.close();
    this.intentionalClose = false;
    this.setPhase("connecting");
    let socket: WebSocket;
    try {
      socket = new WebSocket(url);
    } catch (error) {
      this.callbacks.onError(`Не удалось открыть WebSocket: ${String(error)}`);
      this.setPhase("disconnected");
      return;
    }
    socket.binaryType = "arraybuffer";
    this.socket = socket;
    socket.onopen = () => {
      if (this.socket !== socket) return;
      this.setPhase("lobby");
      this.send({ type: "list" });
      this.startLoop();
    };
    socket.onmessage = ({ data }) => {
      if (this.socket !== socket) return;
      if (typeof data === "string") this.control(data);
      else if (data instanceof ArrayBuffer) this.chunk(data);
    };
    socket.onerror = () => {
      if (this.socket === socket && !this.intentionalClose)
        this.callbacks.onError("Ошибка соединения с сервером.");
    };
    socket.onclose = () => {
      if (this.socket !== socket) return;
      this.socket = null;
      this.world = null;
      this.pendingChunks.clear();
      this.stopLoop();
      this.setPhase("disconnected");
      if (!this.intentionalClose) this.callbacks.onDisconnected();
    };
  }

  createRoom(name: string, player: string, mode: RoomMode, chaos?: ChaosPreset) {
    this.send({ type: "create", name, player, mode, chaos });
  }

  joinRoom(room: string, player: string) {
    this.send({ type: "join", room, player });
  }

  leaveRoom() {
    this.send({ type: "leave" });
    this.world = null;
    this.pendingChunks.clear();
    this.setPhase("lobby");
    this.send({ type: "list" });
  }

  listRooms() {
    this.send({ type: "list" });
  }

  command(command: Command): boolean {
    switch (command.type) {
      case "spawn": {
        const seq = this.nextSequence();
        this.world?.predictSpawn(seq, command.x, command.y, command.mode, command.genome, this.playerId);
        if (
          !this.send({
            type: "spawn",
            seq,
            x: command.x,
            y: command.y,
            mode: command.mode,
            genome: Array.from(command.genome),
          })
        ) {
          this.world?.applyAck(seq, undefined, "Нет соединения");
          return false;
        }
        return true;
      }
      case "edit":
        return this.send({
          type: "edit",
          seq: this.nextSequence(),
          id: command.id,
          mode: command.mode,
          genome: Array.from(command.genome),
        });
      case "inspect":
        return this.send({ type: "inspect", seq: this.nextSequence(), id: command.id });
      case "reset":
        return this.send({ type: "reset", seq: this.nextSequence() });
      default:
        return false;
    }
  }

  close() {
    this.intentionalClose = true;
    this.stopLoop();
    const socket = this.socket;
    this.socket = null;
    this.world = null;
    this.pendingChunks.clear();
    socket?.close();
    if (this.phase !== "disconnected") this.setPhase("disconnected");
  }

  private nextSequence() {
    this.sequence += 1;
    return this.sequence;
  }

  private send(message: object) {
    const socket = this.socket;
    if (!socket || socket.readyState !== WebSocket.OPEN) return false;
    socket.send(JSON.stringify(message));
    return true;
  }

  private control(text: string) {
    let message: ServerControl;
    try {
      message = JSON.parse(text) as ServerControl;
    } catch {
      this.callbacks.onError("Сервер прислал некорректный ответ.");
      return;
    }
    switch (message.type) {
      case "rooms":
        this.callbacks.onRooms(message.rooms);
        break;
      case "welcome":
        if (message.protocol !== PROTOCOL_VERSION) {
          this.callbacks.onError(
            `Версия протокола сервера ${message.protocol} не совпадает с клиентской ${PROTOCOL_VERSION}.`,
          );
          this.close();
          return;
        }
        this.pendingChunks.clear();
        this.world = new PredictionWorld();
        this.playerId = message.player?.id ?? 0;
        this.lastFrame = performance.now();
        this.callbacks.onWelcome(message);
        this.setPhase("online");
        break;
      case "ack":
        this.world?.applyAck(message.seq, message.cell_id, message.error);
        if (message.error) this.callbacks.onError(message.error);
        break;
      case "inspect":
        this.callbacks.onInspect(message.id, message.genome, message.mode);
        break;
      case "error":
        this.callbacks.onError(message.message);
        break;
      case "notice":
        this.callbacks.onNotice(message.message);
        break;
      case "player_count":
        this.callbacks.onPlayerCount(message.players);
        break;
      case "leaderboard":
        this.callbacks.onLeaderboard(message.entries);
        break;
      case "world_reset":
        this.pendingChunks.clear();
        this.world?.applyReset();
        this.callbacks.onWorldReset();
        break;
      case "pong":
        break;
      case "room_closed":
        this.world = null;
        this.pendingChunks.clear();
        this.callbacks.onNotice(message.reason);
        this.setPhase("lobby");
        this.send({ type: "list" });
        break;
    }
  }

  private chunk(buffer: ArrayBuffer) {
    if (!this.world) return;
    if (buffer.byteLength < 31) { this.callbacks.onError("Повреждённый чанк сцены."); return; }
    const view = new DataView(buffer);
    const x = view.getUint8(21), y = view.getUint8(22);
    if (view.getUint8(0) !== 2 || x >= CHUNKS_PER_AXIS || y >= CHUNKS_PER_AXIS) {
      this.callbacks.onError("Повреждённый чанк сцены."); return;
    }
    const key = y * CHUNKS_PER_AXIS + x, tick = view.getUint32(1, true);
    this.profiler.chunk(buffer.byteLength, 0);
    if (tick >= (this.pendingChunks.get(key)?.tick ?? -1))
      this.pendingChunks.set(key, { buffer, tick, receivedAt: performance.now() });
  }

  private applyPendingChunks(world: PredictionWorld) {
    const pending = [...this.pendingChunks.values()].sort((a, b) => a.tick - b.tick);
    this.pendingChunks.clear();
    const started = performance.now();
    for (const entry of pending) {
      const chunk = decodeChunk(entry.buffer);
      if (chunk) world.applyChunk(chunk, entry.receivedAt);
      else this.callbacks.onError("Повреждённый чанк сцены.");
    }
    this.profiler.apply(performance.now() - started);
  }

  private startLoop() {
    if (this.loop !== null) return;
    this.lastFrame = performance.now();
    this.lastPing = this.lastFrame;
    const runFrame = () => {
      const now = performance.now();
      const world = this.world;
      if (world) {
        const started = performance.now();
        this.applyPendingChunks(world);
        const predictionNow = performance.now();
        world.tick((predictionNow - this.lastFrame) / 1000, predictionNow);
        this.callbacks.onFrame(world.frame(), world.syncState());
        this.profiler.frame(performance.now() - started, world.cellCount);
      }
      this.lastFrame = now;
      if (now - this.lastPing > 20_000) {
        this.lastPing = now;
        this.send({ type: "ping", t: Date.now() });
      }
      const workMs = performance.now() - now;
      // Keep time for input/rendering on weak devices. At most 30 scene frames
      // per second, with at least as much idle time as prediction/frame work.
      if (this.loop !== null) {
        this.loop = window.setTimeout(runFrame, Math.max(1000 / 30 - workMs, workMs));
      }
    };
    this.loop = window.setTimeout(runFrame, 1000 / 30);
  }

  private stopLoop() {
    if (this.loop !== null) {
      window.clearTimeout(this.loop);
      this.loop = null;
    }
  }

  private setPhase(phase: NetPhase) {
    if (this.phase === phase) return;
    this.phase = phase;
    this.callbacks.onPhase(phase);
  }
}
