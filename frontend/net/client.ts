//! Thin browser WebSocket transport around `PredictionWorld`.

import type { Command, Frame } from "../model";
import { PredictionWorld, type SyncState } from "./prediction";
import {
  PROTOCOL_VERSION,
  decodeChunk,
  type RoomInfo,
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
  onDisconnected: () => void;
};

export class MultiplayerClient {
  private socket: WebSocket | null = null;
  private world: PredictionWorld | null = null;
  private sequence = 0;
  private loop: number | null = null;
  private lastFrame = 0;
  private lastPing = 0;
  private intentionalClose = false;
  private phase: NetPhase = "disconnected";
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
      this.stopLoop();
      this.setPhase("disconnected");
      if (!this.intentionalClose) this.callbacks.onDisconnected();
    };
  }

  createRoom(name: string, player: string) {
    this.send({ type: "create", name, player });
  }

  joinRoom(room: string, player: string) {
    this.send({ type: "join", room, player });
  }

  leaveRoom() {
    this.send({ type: "leave" });
    this.world = null;
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
        this.world?.predictSpawn(seq, command.x, command.y, command.mode, command.genome);
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
        this.world = new PredictionWorld();
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
      case "world_reset":
        this.world?.applyReset();
        this.callbacks.onWorldReset();
        break;
      case "pong":
        break;
      case "room_closed":
        this.world = null;
        this.callbacks.onNotice(message.reason);
        this.setPhase("lobby");
        this.send({ type: "list" });
        break;
    }
  }

  private chunk(buffer: ArrayBuffer) {
    const chunk = decodeChunk(buffer);
    if (!chunk) {
      this.callbacks.onError("Повреждённый чанк сцены.");
      return;
    }
    this.world?.applyChunk(chunk);
  }

  private startLoop() {
    if (this.loop !== null) return;
    this.lastFrame = performance.now();
    this.lastPing = this.lastFrame;
    this.loop = window.setInterval(() => {
      const now = performance.now();
      const world = this.world;
      if (world) {
        world.tick((now - this.lastFrame) / 1000, now);
        this.callbacks.onFrame(world.frame(), world.syncState());
      }
      this.lastFrame = now;
      if (now - this.lastPing > 20_000) {
        this.lastPing = now;
        this.send({ type: "ping", t: Date.now() });
      }
    }, 1000 / 30);
  }

  private stopLoop() {
    if (this.loop !== null) {
      window.clearInterval(this.loop);
      this.loop = null;
    }
  }

  private setPhase(phase: NetPhase) {
    if (this.phase === phase) return;
    this.phase = phase;
    this.callbacks.onPhase(phase);
  }
}
