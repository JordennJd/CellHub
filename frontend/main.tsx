import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { WorldView, type ViewState } from "./WorldView";
import { RendererToggle } from "./RendererToggle";
import { initialBackend, type BackendPreference } from "./backend";
import { OnlinePanel } from "./OnlinePanel";
import { MultiplayerClient, type NetPhase } from "./net/client";
import type { RoomInfo } from "./net/protocol";
import type { SyncState } from "./net/prediction";
import {
  defaultGenome,
  multicellularGenome,
  newMode,
  removeMode,
  serializeGenome,
  parseGenome,
  type NumericModeKey,
  flatten,
  inflate,
  TYPES,
  STRIDE,
  POPULATION_LIMIT,
  initialPopulation,
  type Mode,
  type Frame,
  type Command,
} from "./model";
import { SlotName } from "./editor/SlotName";
import { useTouchControls } from "./input/useTouchControls";
import "./style.css";
function defaultServerUrl() {
  const requested = new URLSearchParams(location.search).get("server");
  if (requested) return requested;
  const protocol = location.protocol === "https:" ? "wss" : "ws";
  return `${protocol}://${location.host}/ws`;
}
function App() {
  const touchControls = useTouchControls();
  const worker = useRef<Worker | null>(null);
  const network = useRef<MultiplayerClient | null>(null);
  const onlineRef = useRef(false);
  const resumeSpeed = useRef(1);
  const [ready, setReady] = useState(false),
    [frame, setFrame] = useState<Frame | null>(null),
    [onlineOpen, setOnlineOpen] = useState(false),
    [netPhase, setNetPhase] = useState<NetPhase>("disconnected"),
    [rooms, setRooms] = useState<RoomInfo[]>([]),
    [netRoom, setNetRoom] = useState<RoomInfo | null>(null),
    [isHost, setIsHost] = useState(false),
    [sync, setSync] = useState<SyncState | null>(null),
    [serverUrl] = useState(defaultServerUrl),
    [genome, setGenome] = useState(defaultGenome),
    [mode, setMode] = useState(0),
    [speed, setSpeed] = useState(1),
    [tool, setTool] = useState("place"),
    [layer, setLayer] = useState("biome"),
    [selected, setSelected] = useState<number | null>(null),
    [backend, setBackend] = useState<BackendPreference>(initialBackend),
    [webgpuBlocked, setWebgpuBlocked] = useState(false),
    [view, setView] = useState<ViewState>({ zoom: 1, x: 0, y: 0 }),
    [recenter, setRecenter] = useState(0),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [demoMode, setDemoMode] = useState(false),
    [tab, setTab] = useState("genome");
  const slotNames = useRef(new Map<string, string[]>());
  const selection = useRef(selected);
  selection.current = selected;
  const speedRef = useRef(speed);
  speedRef.current = speed;
  const online = netPhase === "online";
  function changeBackend(next: BackendPreference) {
    setBackend(next);
    setError("");
  }
  function backendFailed(failed: BackendPreference, message: string) {
    console.warn(`CellHub: ${failed} renderer failed`, message);
    if (failed === "webgpu") {
      setWebgpuBlocked(true);
      setBackend("webgl");
      setError("");
      setNotice(
        "WebGPU недоступен — включён WebGL. Кнопка WebGPU заблокирована до перезагрузки.",
      );
      return;
    }
    setError(message);
  }
  function send(c: Command) {
    if (c.type === "spawn" || c.type === "edit")
      slotNames.current.set(Array.from(c.genome).join(","), genome.map((slot) => slot.name));
    if (onlineRef.current) {
      if (c.type === "speed" || c.type === "step") {
        setNotice("В онлайн-комнате время и шаг задаёт сервер.");
        return;
      }
      if (!network.current?.command(c))
        setError("Нет соединения с сервером.");
      return;
    }
    worker.current?.postMessage(c);
  }
  useEffect(() => {
    const w = new Worker(new URL("./simulation.worker.ts", import.meta.url), {
      type: "module",
    });
    worker.current = w;
    w.onmessage = ({ data }) => {
      if (data.type === "ready") {
        setReady(true);
        const q = new URLSearchParams(location.search);
        const population = initialPopulation(Number(q.get("count")));
        if(q.has("paused")) { w.postMessage({type:"speed",value:0}); setSpeed(0); }
        const g=flatten(defaultGenome());
        for (const cell of population) w.postMessage({ type: "spawn", ...cell, genome: g });
      }
      if (data.type === "frame" && !onlineRef.current) setFrame(data);
      if (data.type === "error") setError(data.message);
      if (data.type === "genome" && data.id === selection.current) {
        if (data.genome.length) {
          const names = slotNames.current.get(Array.from(data.genome as Float32Array).join(","));
          setGenome(inflate(data.genome).map((slot, i) => ({ ...slot, name: names?.[i] ?? slot.name })));
          setMode(data.mode);
        } else setNotice("Клетка уже завершила жизненный цикл.");
      }
    };
    w.onerror = (e) => setError(e.message || "Не удалось загрузить Worker");
    return () => w.terminate();
  }, []);
  useEffect(() => {
    const client = new MultiplayerClient({
      onPhase: (phase) => {
        onlineRef.current = phase === "online";
        setNetPhase(phase);
      },
      onRooms: setRooms,
      onWelcome: (welcome) => {
        onlineRef.current = true;
        setNetRoom(welcome.room);
        setIsHost(welcome.host);
        setOnlineOpen(false);
        setSync(null);
        setSelected(null);
        selection.current = null;
        setFrame(null);
        setTab("genome");
        resumeSpeed.current = speedRef.current;
        worker.current?.postMessage({ type: "speed", value: 0 });
        setSpeed(0);
        setNotice(`Подключено: ${welcome.room.name} (${welcome.room.id}).`);
      },
      onFrame: (next, nextSync) => {
        if (!onlineRef.current) return;
        setFrame(next);
        setSync(nextSync);
      },
      onInspect: (id, genome, mode) => {
        if (id !== selection.current) return;
        if (genome?.length) {
          setGenome(inflate(Float32Array.from(genome)));
          setMode(mode ?? 0);
        } else {
          setNotice("Клетка уже завершила жизненный цикл.");
        }
      },
      onError: setError,
      onNotice: setNotice,
      onWorldReset: () => {
        setSelected(null);
        selection.current = null;
      },
      onPlayerCount: (players) =>
        setNetRoom((room) => (room ? { ...room, players } : room)),
      onDisconnected: () => {
        onlineRef.current = false;
        setNetRoom(null);
        setSync(null);
        const previous = resumeSpeed.current;
        worker.current?.postMessage({ type: "speed", value: previous });
        setSpeed(previous);
        setNotice("Соединение с сервером закрыто.");
      },
    });
    network.current = client;
    return () => client.close();
  }, []);
  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => setNotice(""), 4000);
    return () => clearTimeout(t);
  }, [notice]);
  const m = genome[mode],
    type = TYPES[m.kind],
    stats = frame?.stats;
  let cell: Float32Array | undefined;
  if (frame && selected !== null) {
    for (let i = 0; i < frame.cells.length; i += STRIDE)
      if (frame.cells[i] === selected) {
        cell = frame.cells.subarray(i, i + STRIDE);
        break;
      }
  }
  function update<K extends keyof Mode>(key: K, value: Mode[K]) {
    setGenome((g) =>
      g.map((m, i) => (i === mode ? { ...m, [key]: value } : m)),
    );
  }
  function changeSpeed(value: number) {
    if (onlineRef.current) {
      setNotice("В онлайн-комнате скорость задаёт сервер.");
      return;
    }
    setSpeed(value);
    send({ type: "speed", value });
  }
  function point(x: number, y: number, id: number | null, action: "place" | "select") {
    if (action === "place") {
      send({ type: "spawn", x, y, mode, genome: flatten(genome) });
    } else {
      if (id !== null && id < 0) {
        selection.current = null;
        setSelected(null);
        setNotice("Клетка ещё не подтверждена сервером.");
        return;
      }
      selection.current = id;
      setSelected(id);
      if (id !== null) {
        send({ type: "inspect", id });
        setTab("genome");
      }
    }
  }
  function save() {
    try {
      localStorage.setItem("cellhub.genome.v2", serializeGenome(genome));
      setNotice("Геном сохранён в этом браузере.");
    } catch {
      setError("Браузер не разрешил сохранить геном в локальном хранилище.");
    }
  }
  function load() {
    try {
      const saved = localStorage.getItem("cellhub.genome.v2") ?? localStorage.getItem("cellhub.genome.v1");
      if (!saved) throw new Error();
      setGenome(parseGenome(saved));
      setMode(0);
      setNotice("Сохранённый геном загружен.");
    } catch {
      setError("Сохранённого совместимого генома нет.");
    }
  }
  function addSlot(copy = false) {
    const index = genome.length;
    const slot = copy ? { ...m, name: `M${index + 1}`,
      childA: m.childA === mode ? index : m.childA,
      childB: m.childB === mode ? index : m.childB,
    } : newMode(index);
    setGenome([...genome, slot]);
    setMode(index);
  }
  function loadExample(slots: Mode[]) {
    setGenome(slots);
    setMode(0);
    setNotice("Пример загружен в редактор. Состояние канваса сохранено.");
  }
  const range = (
    label: string,
    key: NumericModeKey,
    min: number,
    max: number,
    step: number,
    unit = "",
    format?: (v: number) => string,
  ) => (
    <label className="range">
      <span>
        {label}
        <b>
          {format ? format(m[key]) : m[key].toFixed(step < 1 ? 2 : 0)}
          {unit}
        </b>
      </span>
      <span className={`slider-control ${key === "ratio" ? "mass-slider" : ""}`} style={{ "--progress": `${key === "ratio" ? m[key] * 100 : (m[key] - min) / (max - min) * 100}%` } as React.CSSProperties}>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={m[key]}
        onChange={(e) => update(key, Number(e.target.value))}
      />
      {key === "ratio" && <span className="slider-ends" aria-hidden="true"><span>A</span><span>B</span></span>}
      </span>
    </label>
  );
  return (
    <div className="app" data-controls={touchControls ? "touch" : "mouse"}>
      <header>
        <div className="brand">
          <span className="brandmark" aria-hidden="true">◉</span> CellHub
        </div>
        {!ready && <span role="status">Загрузка…</span>}
        <div className="header-actions">
          <RendererToggle
            value={backend}
            webgpuBlocked={webgpuBlocked}
            onChange={changeBackend}
          />
          <button
            className={`quiet network-toggle ${online ? "online" : ""}`}
            onClick={() => setOnlineOpen(true)}
          >
            <i className="network-dot" />
            <span className="network-label">
              {online ? `СЕТЬ · ${netRoom?.id ?? ""}` : "Сеть"}
            </span>
          </button>
          <button className="quiet" onClick={save}>
            ↓ Сохранить геном
          </button>
        </div>
      </header>
      <main>
        <section className="lab">
          <WorldView
            frame={frame}
            tool={tool}
            layer={layer}
            selected={selected}
            onPoint={point}
            onView={setView}
            onError={setError}
            onBackendFailure={backendFailed}
            recenter={recenter}
            backend={backend}
          />
          {touchControls && <div className="map-toolbar" aria-label="Инструменты карты">
            <div className="segmented">
              {[
                ["place", "＋", "Поместить"],
                ["select", "◎", "Выбрать"],
                ["pan", "✥", "Переместить"],
              ].map(([key, icon, label]) => (
                <button
                  key={key}
                  title={label}
                  aria-label={label}
                  aria-pressed={tool === key}
                  className={tool === key ? "active" : ""}
                  onClick={() => setTool(key)}
                >
                  {icon}
                  <span>{label}</span>
                </button>
              ))}
            </div>
          </div>}
          <div className="layer-control">
            <select
              aria-label="Слой субстрата"
              value={layer}
              onChange={(e) => setLayer(e.target.value)}
            >
              <option value="biome">Биомы</option>
              <option value="light">Освещённость</option>
              <option value="food">Питательные вещества</option>
              <option value="radiation">Радиация</option>
            </select>
            <button className="square" title="Показать всю карту" aria-label="Показать всю карту"
              onClick={() => setRecenter((value) => value + 1)}>⛶</button>
          </div>
          {!stats?.[1] && ready && (
            <div className="empty-state">{touchControls ? "Коснитесь карты, чтобы поставить клетку" : "ЛКМ по карте — поставить клетку"}</div>
          )}
          <div className="map-legend">
            {["Мелководье", "Питательный пояс", "Глубины", "Радиация"].map(
              (name, i) => (
                <span key={name}>
                  <i
                    style={{
                      background: ["#8cad6e", "#6cae9f", "#6b94b6", "#b07bab"][
                        i
                      ],
                    }}
                  />
                  {name}
                </span>
              ),
            )}
          </div>
          <div className="map-scale">
            <span>{Math.round(view.zoom * 100)}%</span>
          </div>
          <div className="transport">
            <button
              className="play"
              title={online ? "Время задаёт сервер" : speed ? "Пауза" : "Продолжить"}
              disabled={!ready || online}
              onClick={() => changeSpeed(speed ? 0 : 1)}
            >
              {speed ? "Ⅱ" : "▶"}
            </button>
            <button
              className="step"
              disabled={speed !== 0 || !ready || online}
              title="Один шаг симуляции"
              onClick={() => send({ type: "step" })}
            >
              ▸│
            </button>
            <div className="speed">
              {[1, 2, 4].map((s) => (
                <button
                  className={speed === s ? "active" : ""}
                  key={s}
                  disabled={online}
                  onClick={() => changeSpeed(s)}
                >
                  {s}×
                </button>
              ))}
            </div>
            <div className="time">
              <span>ВРЕМЯ</span>
              <b>
                {((stats?.[0] || 0) / 30).toFixed(1)} <small>с</small>
              </b>
            </div>
            <div className="population">
              <span>ПОПУЛЯЦИЯ</span>
              <b>
                {stats?.[1] || 0}
                <small> / {POPULATION_LIMIT.toLocaleString("ru-RU")}</small>
              </b>
            </div>
            <button
              className="quiet reset"
              disabled={online && !isHost}
              title={online && !isHost ? "Очистить мир может только создатель комнаты" : undefined}
              onClick={() => {
                send({ type: "reset" });
                setSelected(null);
              }}
            >
              ↺ Очистить
            </button>
            {online && (
              <div className="server-pill" title="Сервер присылает чанки последовательно">
                <span className="status-dot" /> SERVER
                <small>{sync?.seen ?? 0}/{sync?.total ?? 36}</small>
              </div>
            )}
          </div>
        </section>
        <aside>
          <div className="editor-title">
            <div>
              <h2>Редактор генома</h2>
            </div>
          </div>
          <div className="tabs">
            <button
              className={tab === "genome" ? "active" : ""}
              onClick={() => setTab("genome")}
            >
              Геном
            </button>
            <button
              className={tab === "observe" ? "active" : ""}
              onClick={() => setTab("observe")}
            >
              Наблюдение
            </button>
          </div>
          {tab === "genome" ? (
            <div className="editor-content">
              <section className="demo-panel">
                <label className="demo-toggle">
                  <span>Режим демонстрации</span>
                  <input type="checkbox" role="switch" checked={demoMode}
                    aria-controls="genome-examples" onChange={(e) => setDemoMode(e.target.checked)} />
                </label>
                {demoMode && <div id="genome-examples" className="demo-examples">
                  <p className="editor-note">Пример заменит слоты редактора. Клетки и канвас сохранят текущее состояние.</p>
                  <button onClick={() => loadExample(multicellularGenome())}>Пример: многоклеточный организм</button>
                  <button onClick={() => loadExample(TYPES.map((_, i) => ({ ...newMode(i), kind: i })))}>Пример: все типы клеток</button>
                </div>}
              </section>
              <div className="section-label">
                СЛОТ ГЕНОМА <span>{mode + 1} / {genome.length}</span>
              </div>
              <div className="modes">
                {genome.map((g, i) => (
                  <button
                    key={i}
                    aria-label={`Слот ${g.name}`}
                    aria-pressed={i === mode}
                    className={mode === i ? "active" : ""}
                    onClick={() => setMode(i)}
                  >
                    <i
                      style={{
                        background: "#" + TYPES[g.kind].color.toString(16),
                      }}
                    />
                    {g.name}
                  </button>
                ))}
              </div>
              <div className="slot-actions">
                <button onClick={() => addSlot()}>＋ Слот</button>
                <button onClick={() => addSlot(true)}>Копия</button>
                <button disabled={genome.length === 1} title="Ссылки на удалённый слот перейдут к следующему, а для последнего — к предыдущему"
                  onClick={() => {
                    setGenome(removeMode(genome, mode));
                    setMode(Math.min(mode, genome.length - 2));
                    setNotice("Слот удалён; ссылки на него перенаправлены к соседнему.");
                  }}>Удалить</button>
              </div>
              <SlotName key={`${mode}:${m.name}`} name={m.name} onRename={(name) => update("name", name)} />
              <div className="cell-type-control">
              <label className="select-label">
                Тип клетки
                <select
                  value={m.kind}
                  onChange={(e) => update("kind", Number(e.target.value))}
                >
                  {TYPES.map((t, i) => (
                    <option value={i} key={i}>
                      {t.name} / {t.latin}
                    </option>
                  ))}
                </select>
              </label>
              <div className="cell-card">
                <div
                  className="cell-preview"
                  style={
                    {
                      "--cell-color": "#" + type.color.toString(16),
                    } as React.CSSProperties
                  }
                >
                  <span />
                  <i />
                </div>
                <div>
                  <h3>{type.name}</h3>
                  <span>{type.latin}</span>
                </div>
                <span className="mode-badge">{m.name}</span>
              </div>
              <p className="type-info">{type.info}</p>
              </div>
              <div className="section-label line">
                ЖИЗНЕННЫЙ ЦИКЛ
              </div>
              <label className="toggle">
                <input type="checkbox" checked={m.canDivide} onChange={(e) => update("canDivide", e.target.checked)} />
                <span>Разрешить деление</span>
              </label>
              {!m.canDivide && <p className="editor-note">Деление клетки выключено. Клетка продолжает питаться, двигаться и стареть.</p>}
              <fieldset className="division-settings" disabled={!m.canDivide}>
              {range("Масса для деления", "threshold", 1, 8, 0.1, " нг")}
              <div className="children">
                {(["childA", "childB"] as const).map((key, i) => (
                  <div className="child-settings" key={key}>
                  <label>
                    Дочерняя клетка {i ? "B" : "A"}
                    <select
                      value={m[key]}
                      onChange={(e) => update(key, Number(e.target.value))}
                    >
                      {genome.map((g, i) => (
                        <option key={i} value={i}>
                          {g.name} · {TYPES[g.kind].name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="toggle">
                    <input type="checkbox" checked={i ? m.keepAdhesionB : m.keepAdhesionA}
                      onChange={(e) => update(i ? "keepAdhesionB" : "keepAdhesionA", e.target.checked)} />
                    <span>{i ? "B" : "A"} наследует связи родителя</span>
                  </label>
                  </div>
                ))}
              </div>
              <label className="toggle">
                <input type="checkbox" checked={m.bindChildren} onChange={(e) => update("bindChildren", e.target.checked)} />
                <span>Связать A и B после деления</span>
              </label>
              {range(
                "Распределение массы",
                "ratio",
                0.2,
                0.8,
                0.01,
                "",
                (v) => `${Math.round(v * 100)} : ${Math.round((1 - v) * 100)}`,
              )}
              {range(
                "Угол деления",
                "angle",
                -3.14,
                3.14,
                0.01,
                "°",
                (v) => `${Math.round((v * 180) / Math.PI)}`,
              )}
              </fieldset>
              {range(m.kind === 2 ? "Скорость жгутика" : "Скорость свободной клетки", "speed", 0, 60, 1, " ед/с")}
              {m.kind === 2 && <p className="editor-note">Жгутик тянет за собой связанные клетки организма.</p>}
              {range("Продолжительность жизни", "lifespan", 20, 1000, 10, " с")}
              <div className="lineage">
                <span>{m.name}</span>
                <i>→</i>
                {m.canDivide ? <>
                  <span>{genome[m.childA].name}</span>
                  <i>{m.bindChildren ? "⟷" : "+"}</i>
                  <span>{genome[m.childB].name}</span>
                </> : <span>Без деления</span>}
              </div>
              {selected !== null && (
                <button
                  className="apply"
                  disabled={!cell || selected < 0}
                  onClick={() => {
                    send({
                      type: "edit",
                      id: selected,
                      mode,
                      genome: flatten(genome),
                    });
                    setNotice("Геном передан выбранной клетке.");
                  }}
                >
                  Применить к клетке #{selected}
                </button>
              )}
              <div className="editor-actions">
                <button onClick={save}>↓ Сохранить</button>
                <button onClick={load}>↑ Загрузить</button>
                <button
                  title="Вернуть стандартный геном редактора"
                  onClick={() => { setGenome(defaultGenome()); setMode(0); }}
                >
                  ↺
                </button>
              </div>
              <p className="editor-note">
                Геном используется для новых клеток.
              </p>
            </div>
          ) : (
            <div className="editor-content">
              <div className="metric-grid">
                {[
                  ["Рождений", stats?.[2] || 0],
                  ["Погибло", stats?.[3] || 0],
                  ["Заражений", stats?.[4] || 0],
                  ["Связей", (frame?.links.length || 0) / 2],
                ].map(([k, v]) => (
                  <div key={k}>
                    <span>{k}</span>
                    <b>{v}</b>
                  </div>
                ))}
              </div>
              <h3>
                {selected !== null ? `Клетка #${selected}` : "Выберите клетку"}
              </h3>
              {cell ? (
                <dl>
                  {[
                    ["Тип", TYPES[cell[4]].name],
                    ["Масса", `${cell[5].toFixed(3)} нг`],
                    ["Возраст", `${cell[6].toFixed(1)} с`],
                    ["Поколение", cell[7]],
                    ["Изменения генома", cell[8]],
                    ["Связей", frame?.links.filter((id) => id === selected).length || 0],
                  ].map(([k, v]) => (
                    <div key={k}>
                      <dt>{k}</dt>
                      <dd>{v}</dd>
                    </div>
                  ))}
                </dl>
              ) : (
                <p className="type-info">
                  {selected
                    ? "Клетка разделилась или погибла. Выберите её потомка."
                    : touchControls ? "Выберите инструмент «Выбрать» и коснитесь клетки." : "Нажмите ПКМ на клетку."}
                </p>
              )}
            </div>
          )}
        </aside>
      </main>
      <div className="bottom-bar">
        {touchControls ? "Два пальца — масштаб и перемещение" : "ЛКМ — поставить / удерживать — серия · ПКМ — выбрать · Зажать колёсико — перемещение · Прокрутка — масштаб"}
      </div>
      <OnlinePanel
        open={onlineOpen}
        phase={netPhase}
        serverUrl={serverUrl}
        rooms={rooms}
        room={netRoom}
        host={isHost}
        playerName="Наблюдатель"
        sync={sync}
        onClose={() => setOnlineOpen(false)}
        onConnect={(url) => network.current?.connect(url)}
        onCreate={(name, player) => network.current?.createRoom(name, player)}
        onJoin={(room, player) => network.current?.joinRoom(room, player)}
        onRefresh={() => network.current?.listRooms()}
        onLeave={() => {
          network.current?.leaveRoom();
          setNetRoom(null);
          setSync(null);
          setSelected(null);
          const previous = resumeSpeed.current;
          worker.current?.postMessage({ type: "speed", value: previous });
          setSpeed(previous);
        }}
        onDisconnect={() => {
          network.current?.close();
          setNetRoom(null);
          setSync(null);
          setSelected(null);
          const previous = resumeSpeed.current;
          worker.current?.postMessage({ type: "speed", value: previous });
          setSpeed(previous);
          setNotice("Сетевая сессия завершена.");
        }}
      />
      {(error || notice) && (
        <div
          className={`toast ${error ? "error" : ""}`}
          role={error ? "alert" : "status"}
        >
          {error || notice}
          <button
            aria-label="Закрыть"
            onClick={() => {
              setError("");
              setNotice("");
            }}
          >
            ×
          </button>
        </div>
      )}
    </div>
  );
}
createRoot(document.getElementById("root")!).render(<App />);
