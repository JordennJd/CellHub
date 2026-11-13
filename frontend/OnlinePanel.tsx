import { useState } from "react";
import type { NetPhase } from "./net/client";
import type { RoomInfo, RoomMode } from "./net/protocol";
import type { SyncState } from "./net/prediction";

type Props = {
  open: boolean;
  phase: NetPhase;
  serverUrl: string;
  rooms: RoomInfo[];
  room: RoomInfo | null;
  host: boolean;
  playerName: string;
  sync: SyncState | null;
  onClose: () => void;
  onConnect: (url: string) => void;
  onCreate: (name: string, player: string, mode: RoomMode) => void;
  onJoin: (room: string, player: string) => void;
  onRefresh: () => void;
  onLeave: () => void;
  onDisconnect: () => void;
};

const MODE_LABELS: Record<RoomMode, string> = {
  sandbox: "Песочница",
  arena: "Арена",
};

export function OnlinePanel(props: Props) {
  const [url, setUrl] = useState(props.serverUrl);
  const [name, setName] = useState("Моя колония");
  const [mode, setMode] = useState<RoomMode>("arena");
  const [player, setPlayer] = useState(props.playerName);
  if (!props.open) return null;
  const online = props.phase === "online";
  const connecting = props.phase === "connecting";
  const disconnected = props.phase === "disconnected";
  return (
    <div className="online-backdrop" role="presentation" onMouseDown={props.onClose}>
      <section
        className="online-panel"
        role="dialog"
        aria-modal="true"
        aria-label="Сетевая лаборатория"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header>
          <div>
            <span className="eyebrow">СОВМЕСТНАЯ СЦЕНА</span>
            <h2>Сетевая лаборатория</h2>
          </div>
          <button className="online-close" aria-label="Закрыть" onClick={props.onClose}>
            ×
          </button>
        </header>
        <p className="online-note">
          Редактор генома остаётся вашим. Сервер разделяет только общую сцену,
          обновляет её 10 раз в секунду. Полная карта приходит каждые 0,6 секунды.
        </p>
        <label className="online-field">
          Ваше имя
          <input
            value={player}
            maxLength={32}
            disabled={online}
            onChange={(event) => setPlayer(event.target.value)}
          />
        </label>
        {connecting && (
          <p className="online-status" role="status">
            Подключаемся к серверу…
          </p>
        )}
        {disconnected && (
          <>
            <p className="online-empty">
              Соединение закрыто. Проверьте адрес сервера и попробуйте снова.
            </p>
            <button
              className="online-primary"
              disabled={!url.trim()}
              onClick={() => props.onConnect(url.trim())}
            >
              Подключиться снова
            </button>
          </>
        )}
        {props.phase === "lobby" && (
          <>
            <div className="online-section-head">
              <span className="eyebrow">ПУБЛИЧНЫЕ КОМНАТЫ</span>
              <button className="quiet" onClick={props.onRefresh}>
                Обновить
              </button>
            </div>
            <div className="online-rooms">
              {props.rooms.length === 0 && (
                <p className="online-empty">
                  Пока ни одной комнаты. Создайте первую — сервер сразу запустит
                  в ней симуляцию.
                </p>
              )}
              {props.rooms.map((room) => {
                const max = room.max_players ?? 0;
                const full = max > 0 && room.players >= max;
                return (
                  <button
                    className={full ? "online-room-row full" : "online-room-row"}
                    key={room.id}
                    disabled={full}
                    title={full ? `В комнате нет мест: ${room.players} из ${max}.` : undefined}
                    onClick={() => props.onJoin(room.id, player)}
                  >
                    <span>
                      <b>{room.name}</b>
                      <small>
                        {MODE_LABELS[room.mode]} · {room.id} · создатель {room.host}
                      </small>
                    </span>
                    <span>
                      <b>{room.population.toLocaleString("ru-RU")}</b>
                      <small className="online-capacity">
                        {room.players} / {max > 0 ? max : "—"} игроков
                      </small>
                    </span>
                    <i>{full ? "Заполнена" : "Войти ↗"}</i>
                  </button>
                );
              })}
            </div>
            <div className="online-create">
              <div className="online-create-head">
                <span className="eyebrow">СВОЯ КОМНАТА</span>
              </div>
              <label className="online-field">
                Название
                <input
                  value={name}
                  maxLength={48}
                  onChange={(event) => setName(event.target.value)}
                />
              </label>
              <div className="online-create-row">
                <div className="online-mode" role="radiogroup" aria-label="Режим комнаты">
                  {(["sandbox", "arena"] as const).map((value) => (
                    <button
                      key={value}
                      type="button"
                      role="radio"
                      aria-checked={mode === value}
                      className={mode === value ? "active" : undefined}
                      onClick={() => setMode(value)}
                    >
                      <b>{MODE_LABELS[value]}</b>
                      <small>
                        {value === "sandbox"
                          ? "Без очков и рейтинга."
                          : "Мир «Хаоса», очки и лидеры."}
                      </small>
                    </button>
                  ))}
                </div>
                <button
                  className="online-create-submit"
                  disabled={!name.trim()}
                  onClick={() => props.onCreate(name.trim(), player, mode)}
                >
                  Создать
                </button>
              </div>
              <p className="online-note">
                {mode === "arena"
                  ? "Арена наследует мир «Хаоса»: биомы, смену суток, радиацию и самозарождение, а также начисляет очки за биомассу и разрешает ставить новую клетку раз в 2 секунды."
                  : "Песочница начинает общий мир без очков и ограничений на постановку клеток."}
              </p>
            </div>
          </>
        )}
        {!online && (
          <details className="online-server">
            <summary>
              Сервер: <code>{url}</code>
            </summary>
            <label className="online-field">
              Адрес WebSocket
              <input
                value={url}
                disabled={connecting}
                onChange={(event) => setUrl(event.target.value)}
                spellCheck={false}
              />
            </label>
          </details>
        )}
        {online && props.room && (
          <div className="online-room">
            <div className="online-room-head">
              <div>
                <span className="eyebrow">{props.room.mode === "arena" ? "АРЕНА" : "ПЕСОЧНИЦА"}</span>
                <h3>{props.room.name}</h3>
              </div>
              <span className="online-code">{props.room.id}</span>
            </div>
            <div className="online-metrics">
              <div>
                <span>Игроков</span>
                <b>
                  {props.room.players} / {props.room.max_players ?? "—"}
                </b>
              </div>
              <div>
                <span>Клеток</span>
                <b>
                  {(props.sync?.population ?? props.room.population).toLocaleString("ru-RU")}
                </b>
              </div>
              <div>
                <span>Чанки</span>
                <b>
                  {props.sync?.seen ?? 0} / {props.sync?.total ?? 36}
                </b>
              </div>
              <div>
                <span>Тик сервера</span>
                <b>{props.sync?.lastTick ?? props.room.tick}</b>
              </div>
            </div>
            <p className="online-note">
              {props.host
                ? "Вы создатель комнаты: можете очистить общий мир."
                : `Создатель комнаты: ${props.room.host}. Время и биология принадлежат серверу.`}
            </p>
            <div className="online-actions">
              <button onClick={props.onLeave}>Выйти в список комнат</button>
              <button className="quiet" onClick={props.onDisconnect}>
                Отключиться
              </button>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
