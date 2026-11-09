import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { WorldView, type ViewState } from "./WorldView";
import {
  defaultGenome,
  multicellularGenome,
  newMode,
  removeMode,
  serializeGenome,
  parseGenome,
  MAX_MODES,
  type NumericModeKey,
  flatten,
  inflate,
  TYPES,
  STRIDE,
  type Mode,
  type Frame,
  type Command,
} from "./model";
import "./style.css";
function App() {
  const worker = useRef<Worker | null>(null);
  const [ready, setReady] = useState(false),
    [frame, setFrame] = useState<Frame | null>(null),
    [genome, setGenome] = useState(defaultGenome),
    [mode, setMode] = useState(0),
    [speed, setSpeed] = useState(1),
    [tool, setTool] = useState("place"),
    [layer, setLayer] = useState("biome"),
    [selected, setSelected] = useState<number | null>(null),
    [renderer, setRenderer] = useState("GPU"),
    [view, setView] = useState<ViewState>({ zoom: 1, x: 0, y: 0 }),
    [recenter, setRecenter] = useState(0),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [tab, setTab] = useState("genome");
  const selection = useRef(selected);
  selection.current = selected;
  function send(c: Command) {
    worker.current?.postMessage(c);
  }
  useEffect(() => {
    const w = new Worker(new URL("./simulation.worker.ts", import.meta.url), {
      type: "module",
    });
    worker.current = w;
    w.onmessage = ({ data }) => {
      if (data.type === "ready") setReady(true);
      if (data.type === "frame") setFrame(data);
      if (data.type === "error") setError(data.message);
      if (data.type === "genome" && data.id === selection.current) {
        if (data.genome.length) {
          setGenome(inflate(data.genome));
          setMode(data.mode);
        } else setNotice("Клетка уже завершила жизненный цикл.");
      }
    };
    w.onerror = (e) => setError(e.message || "Не удалось загрузить Worker");
    return () => w.terminate();
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
    setSpeed(value);
    send({ type: "speed", value });
  }
  function point(x: number, y: number, id: number | null) {
    if (tool === "place") {
      send({ type: "spawn", x, y, mode, genome: flatten(genome) });
    } else {
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
  function addArchetype(copy = false) {
    if (genome.length >= MAX_MODES) return;
    const index = genome.length;
    const archetype = copy ? { ...m,
      childA: m.childA === mode ? index : m.childA,
      childB: m.childB === mode ? index : m.childB,
    } : newMode(index);
    setGenome([...genome, archetype]);
    setMode(index);
  }
  function organismExample() {
    setGenome(multicellularGenome());
    setMode(0);
    setTool("place");
    selection.current = null;
    setSelected(null);
    setNotice("Пример загружен: поместите M1 на светлом участке субстрата.");
  }
  function demo() {
    send({ type: "reset" });
    setSelected(null);
    const g = defaultGenome();
    for (let k = 0; k < 6; k++)
      for (let i = 0; i < 10; i++) {
        send({
          type: "spawn",
          x: 600 + (k % 3) * 700 + Math.cos(i * 2.4) * 80,
          y: 700 + Math.floor(k / 3) * 1300 + Math.sin(i * 2.4) * 80,
          mode: k,
          genome: flatten(g),
        });
      }
    setNotice("60 клеток помещены в разные биомы.");
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
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={m[key]}
        onChange={(e) => update(key, Number(e.target.value))}
      />
    </label>
  );
  return (
    <div className="app">
      <header>
        <div className="brand">
          <span className="brandmark">◉</span> CellHub{" "}
          <span className="version">LAB / 01</span>
        </div>
        <div className="experiment">
          <span className="status-dot" />
          {ready ? "Эксперимент 73421" : "Загрузка WASM…"}
          <span className="separator">/</span>
          <span>Открытый субстрат</span>
        </div>
        <button className="quiet" onClick={save}>
          ↓ Сохранить геном
        </button>
      </header>
      <main>
        <section className="lab">
          <div className="lab-top">
            <div>
              <span className="eyebrow">ЖИВЫЕ СИСТЕМЫ</span>
              <h1>
                Маленькая жизнь.
                <br />
                <span>Большие возможности.</span>
              </h1>
            </div>
            <div className="engine">
              <span className="status-dot" />
              RUST · WASM
              <br />
              <small>
                {renderer.replace("Renderer", "").toUpperCase()} / 30 Hz
              </small>
            </div>
          </div>
          <WorldView
            frame={frame}
            tool={tool}
            layer={layer}
            selected={selected}
            onPoint={point}
            onView={setView}
            onRenderer={setRenderer}
            onError={setError}
            recenter={recenter}
          />
          <div className="map-toolbar">
            <div className="segmented">
              {[
                ["place", "＋", "Поместить"],
                ["select", "◎", "Выбрать"],
                ["pan", "✥", "Переместить"],
              ].map(([key, icon, label]) => (
                <button
                  key={key}
                  title={label}
                  aria-pressed={tool === key}
                  className={tool === key ? "active" : ""}
                  onClick={() => setTool(key)}
                >
                  {icon}
                  <span>{label}</span>
                </button>
              ))}
            </div>
            <button
              className="square"
              title="Показать всю карту"
              onClick={() => setRecenter((v) => v + 1)}
            >
              ⛶
            </button>
          </div>
          <div className="layer-control">
            <span>СЛОЙ СУБСТРАТА</span>
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
          </div>
          {!stats?.[1] && ready && (
            <div className="empty-state">
              <span className="empty-cell">◉</span>
              <h2>Всё начинается с клетки</h2>
              <p>
                Нажмите на субстрат, чтобы посеять жизнь.
                <br />
                Её будущий путь задаётся в редакторе справа.
              </p>
              <button onClick={demo}>
                Начать с примера <span>↗</span>
              </button>
            </div>
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
            <b>└────┘</b>
            <span>3072 × 3072</span>
          </div>
          <div className="transport">
            <button
              className="play"
              disabled={!ready}
              title={speed ? "Пауза" : "Продолжить"}
              onClick={() => changeSpeed(speed ? 0 : 1)}
            >
              {speed ? "Ⅱ" : "▶"}
            </button>
            <button
              className="step"
              disabled={speed !== 0 || !ready}
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
                  onClick={() => changeSpeed(s)}
                >
                  {s}×
                </button>
              ))}
            </div>
            <div className="time">
              <span>ВРЕМЯ СИМУЛЯЦИИ</span>
              <b>
                {((stats?.[0] || 0) / 30).toFixed(1)} <small>с</small>
              </b>
            </div>
            <div className="population">
              <span>ПОПУЛЯЦИЯ</span>
              <b>
                {stats?.[1] || 0}
                <small> / 5 000</small>
              </b>
            </div>
            <button
              className="quiet reset"
              onClick={() => {
                send({ type: "reset" });
                setSelected(null);
              }}
            >
              ↺ Очистить
            </button>
          </div>
        </section>
        <aside>
          <div className="editor-title">
            <div>
              <span className="eyebrow">КОНСТРУКТОР ЖИЗНИ</span>
              <h2>Редактор генома</h2>
            </div>
            <span className="dna">⌘</span>
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
              <div className="section-label">
                АРХЕТИП КЛЕТКИ <span>{mode + 1} / {genome.length}</span>
              </div>
              <div className="modes">
                {genome.map((g, i) => (
                  <button
                    key={i}
                    aria-label={`Архетип M${i + 1}`}
                    aria-pressed={i === mode}
                    className={mode === i ? "active" : ""}
                    onClick={() => setMode(i)}
                  >
                    <i
                      style={{
                        background: "#" + TYPES[g.kind].color.toString(16),
                      }}
                    />
                    M{i + 1}
                  </button>
                ))}
              </div>
              <div className="archetype-actions">
                <button disabled={genome.length >= MAX_MODES} onClick={() => addArchetype()}>＋ Архетип</button>
                <button disabled={genome.length >= MAX_MODES} onClick={() => addArchetype(true)}>Копия</button>
                <button disabled={genome.length === 1} title="Ссылки на удалённый архетип перейдут к следующему, а для последнего — к предыдущему"
                  onClick={() => {
                    setGenome(removeMode(genome, mode));
                    setMode(Math.min(mode, genome.length - 2));
                    setNotice("Архетип удалён; ссылки на него перенаправлены к соседнему.");
                  }}>Удалить</button>
              </div>
              <button className="organism-example" onClick={organismExample}>Пример: многоклеточный организм</button>
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
                <span className="mode-badge">M{mode + 1}</span>
              </div>
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
              <p className="type-info">{type.info}</p>
              <div className="section-label line">
                ЖИЗНЕННЫЙ ЦИКЛ <span>↗</span>
              </div>
              <label className="toggle">
                <input type="checkbox" checked={m.canDivide} onChange={(e) => update("canDivide", e.target.checked)} />
                <span>Разрешить деление</span>
              </label>
              {!m.canDivide && <p className="editor-note">Архетип не размножается. Клетка продолжает питаться, двигаться и стареть.</p>}
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
                          M{i + 1} · {TYPES[g.kind].name}
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
              <p className="editor-note">Каждый потомок наследует все прежние связи родителя, если включён его переключатель. Новая связь A–B задаётся отдельно.</p>
              {range(
                "Распределение массы",
                "ratio",
                0.2,
                0.8,
                0.01,
                "",
                (v) => `${Math.round(v * 100)} : ${Math.round((1 - v) * 100)}`,
              )}
              <div className="split">
                <span style={{ width: `${m.ratio * 100}%` }}>A</span>
                <span>B</span>
              </div>
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
                <span>M{mode + 1}</span>
                <i>→</i>
                {m.canDivide ? <>
                  <span>M{m.childA + 1}</span>
                  <i>{m.bindChildren ? "⟷" : "+"}</i>
                  <span>M{m.childB + 1}</span>
                  <small>{m.bindChildren ? "Связанные потомки" : "Без новой связи A–B"} · наследуют весь геном</small>
                </> : <span>Без деления</span>}
              </div>
              {selected !== null && (
                <button
                  className="apply"
                  disabled={!cell}
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
                Изменения задают геном новых клеток. Выберите клетку на карте,
                чтобы изменить её геном отдельно.
              </p>
            </div>
          ) : (
            <div className="editor-content">
              <div className="section-label">ЭКСПЕРИМЕНТ / 73421</div>
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
                    : "Включите инструмент «Выбрать» и нажмите на клетку в субстрате."}
                </p>
              )}
              <button className="apply" onClick={demo}>
                Новый пример · 60 клеток
              </button>
              <p className="editor-note">
                Пример заменит текущую популяцию. Радиация случайно изменяет
                режимы генома. Свет питает фотоциты; запас пищи истощается и
                восстанавливается.
              </p>
            </div>
          )}
          <footer>
            <span className="status-dot" />
            Локальная симуляция<span>v0.1 / experimental</span>
          </footer>
        </aside>
      </main>
      <div className="bottom-bar">
        <span>
          <i />{" "}
          {tool === "place"
            ? "Клик — поместить клетку"
            : tool === "select"
              ? "Клик — выбрать клетку"
              : "Перетаскивание — переместить карту"}
          <span className="desktop-hint">
            {" "}
            · Перетаскивание — обзор · Колесо — масштаб
          </span>
        </span>
        <span>
          SEED 73421 <span className="separator">/</span> {TYPES.length} ТИПОВ · {genome.length} АРХЕТИПОВ
        </span>
      </div>
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
