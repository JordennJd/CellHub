import { Slider } from "./Slider";
import {
  GAMETE_GROUP_COUNT, POPULATION_LIMIT, PROGRAM_INPUTS, PROGRAM_TARGETS, SECRETIONS, SENSORS, TYPES, newProgram,
  type Mode, type NumericModeKey, type Program,
} from "../model";

export function Setting({ label, help, value, min, max, step, unit = "", onChange }: {
  label: string;
  help: string;
  value: number;
  min: number;
  max: number;
  step: number;
  unit?: string;
  onChange: (value: number) => void;
}) {
  return <div className="genome-setting">
    <label><span>{label}{unit && `, ${unit}`}</span>
      <input type="number" aria-label={label} value={Number(value.toPrecision(9))} min={min} max={max} step={step}
        onChange={e => {
          const value = e.currentTarget.valueAsNumber;
          if (Number.isFinite(value)) onChange(Math.min(max, Math.max(min, value)));
        }} />
    </label>
    <Slider label={`${label}: ползунок`} value={value} min={min} max={max} step={step} onChange={onChange}/>
    <details><summary aria-label={`Справка: ${label}`} title={`Справка: ${label}`}>?</summary><p>{help}</p></details>
  </div>;
}

type CellSettingsProps = {
  mode: Mode;
  genome: Mode[];
  onChange: (changes: Partial<Mode>) => void;
};

type ModeSettingsProps = Pick<CellSettingsProps, "mode" | "onChange">;

function ConnectionLimit({ mode, onChange }: ModeSettingsProps) {
  const glue = mode.kind === 9;
  const enabled = mode.maxConnections >= 0;
  const label = glue
    ? (mode.canDivide ? "Задать порог для приклеивания и деления" : "Ограничить приклеивание по числу связей")
    : "Ограничивать деление по числу связей";
  return <>
    <label className="genome-check"><input type="checkbox" checked={enabled}
      onChange={e => onChange({ maxConnections: e.target.checked ? 4 : -1 })} />{label}</label>
    {enabled && <label className="genome-cell-type">Порог постоянных связей
      <input type="number" min={0} max={POPULATION_LIMIT} step={1} value={mode.maxConnections}
        onChange={e => {
          const value = e.currentTarget.valueAsNumber;
          if (Number.isInteger(value)) onChange({ maxConnections: Math.min(POPULATION_LIMIT, Math.max(0, value)) });
        }} />
    </label>}
    {glue && <p>Глюоцит приклеивает соседей, пока у него меньше пяти связей. Заданный порог может уменьшить этот предел.</p>}
    {enabled && mode.canDivide && <p>При достижении порога клетка перестаёт делиться. Существующие связи сохраняются.</p>}
  </>;
}

export function DivisionSettings({ mode, onChange }: ModeSettingsProps) {
  if (mode.kind === 16) return null;

  const orientation = (label: string, key: "angle" | "childAngleA" | "childAngleB", help: string) =>
    <Setting label={label} value={Math.round(mode[key] * 180 / Math.PI)} min={-180} max={180} step={1} unit="°" help={help}
      onChange={value => onChange({ [key]: Math.max(-3.14, Math.min(3.14, value * Math.PI / 180)) })} />;

  return <section className="genome-division">
    <h3>Деление</h3>
    <label className="genome-check"><input type="checkbox" checked={mode.canDivide}
      onChange={e => onChange({ canDivide: e.target.checked })} />Разрешить деление</label>
    {mode.canDivide && <fieldset>
      <legend className="sr-only">Параметры деления</legend>
      <Setting label="Масса для деления" value={mode.threshold} min={1} max={8} step={0.1} unit="нг"
        onChange={threshold => onChange({ threshold })}
        help="Деление возможно при достижении этой массы и возраста 0,5 с. Рост зависит от типа клетки, доступной пищи и света." />
      <div className="mass-distribution"><label>
        <span>Распределение массы<output>{Math.round(mode.ratio * 100)} : {Math.round((1 - mode.ratio) * 100)}</output></span>
        <Slider label="Распределение массы" value={mode.ratio} min={0.2} max={0.8} step={0.01} fraction={mode.ratio}
          ends={["A", "B"]} onChange={ratio => onChange({ ratio })} />
      </label></div>
      {orientation("Угол деления", "angle", "Поворачивает ось деления относительно направления родителя. У зеркально отражённой клетки угол отсчитывается в обратную сторону.")}
      {mode.kind !== 9 && <ConnectionLimit mode={mode} onChange={onChange} />}
      <label className="genome-check"><input type="checkbox" checked={mode.bindChildren}
        onChange={e => onChange({ bindChildren: e.target.checked })} />Связать дочерние клетки</label>
      <div className="genome-children">
        {([
          { name: "A", keep: "keepAdhesionA", mirror: "mirrorA", angle: "childAngleA" },
          { name: "B", keep: "keepAdhesionB", mirror: "mirrorB", angle: "childAngleB" },
        ] as const).map(({ name, keep, mirror, angle }) => <section key={name} aria-label={`Дочерняя клетка ${name}`}>
          <strong>Дочерняя клетка {name}</strong>
          <label className="genome-check"><input type="checkbox" checked={mode[keep]}
            onChange={e => onChange({ [keep]: e.target.checked })} />Может унаследовать связи родителя</label>
          <label className="genome-check"><input type="checkbox" checked={mode[mirror]}
            onChange={e => onChange({ [mirror]: e.target.checked })} />Зеркально отразить</label>
          {orientation(`Направление дочери ${name}`, angle, "Поворачивает дочь относительно оси деления в системе отсчёта родителя. Отражение дочери меняет сторону поворотов при её собственном делении.")}
        </section>)}
      </div>
      <details><summary>Как работают связи</summary>
        <p>Прежнюю связь может унаследовать дочь, которая остаётся в зоне контакта с соседом и у которой включено наследование. Сестра может перекрыть этот контакт. Если обе связанные дочери могут сохранить по одному контакту с прежними соседями, связи распределяются между ними.</p>
        <p>Если связь между дочерьми выключена, они получают противоположные импульсы вдоль оси деления. Дочь, сохранившая связь с организмом, передаёт ему отдачу.</p>
      </details>
    </fieldset>}
  </section>;
}

function Channel({ value, onChange }: { value: number; onChange: (value: number) => void }) {
  return <label className="genome-cell-type">Сигнальный канал
    <select value={value} onChange={e => onChange(Number(e.target.value))}>
      {Array.from({ length: 8 }, (_, index) => <option key={index} value={index}>S{index + 1}</option>)}
    </select>
  </label>;
}

function ModeReference({ label, value, genome, emptyLabel, onChange }: {
  label: string;
  value: number;
  genome: Mode[];
  emptyLabel?: string;
  onChange: (value: number) => void;
}) {
  return <label className="genome-cell-type">{label}
    <select value={value} onChange={e => onChange(Number(e.target.value))}>
      {emptyLabel && <option value={-1}>{emptyLabel}</option>}
      {genome.map((mode, index) => <option key={index} value={index}>{mode.name} · {TYPES[mode.kind].name}</option>)}
    </select>
  </label>;
}

function SensorSettings({ mode, onChange }: ModeSettingsProps) {
  const targetColor = mode.targetRed < 0 ? 0xffffff
    : (Math.round(mode.targetRed * 255) << 16) | (Math.round(mode.targetGreen * 255) << 8) | Math.round(mode.targetBlue * 255);
  function setTargetColor(color: number) {
    onChange({
      targetRed: ((color >> 16) & 255) / 255,
      targetGreen: ((color >> 8) & 255) / 255,
      targetBlue: (color & 255) / 255,
    });
  }
  return <>
    <label className="genome-cell-type">Ощущаемый стимул
      <select value={mode.sensor} onChange={e => onChange({ sensor: Number(e.target.value) })}>
        {SENSORS.map((name, index) => <option key={index} value={index}>{name}</option>)}
      </select>
    </label>
    <Channel value={mode.signalChannel} onChange={signalChannel => onChange({ signalChannel })} />
    <Setting label="Коэффициент сенсора" value={mode.sensorGain} min={-16} max={16} step={0.1}
      onChange={sensorGain => onChange({ sensorGain })}
      help={"Преобразует ощущение в выходной сигнал. Отрицательное значение меняет знак сигнала."
        + ([7, 8].includes(mode.sensor) ? " Параметр «Подвижность» также задаёт дальность поиска." : "")} />
    {[3, 7, 8].includes(mode.sensor) && <>
      <label className="genome-check"><input type="checkbox" checked={mode.targetRed >= 0}
        onChange={e => e.target.checked ? setTargetColor(targetColor)
          : onChange({ targetRed: -1, targetGreen: -1, targetBlue: -1 })} />Различать клетки по цвету</label>
      {mode.targetRed >= 0 && <>
        <label className="genome-color-picker">Целевой цвет
          <input aria-label="Целевой цвет сенсора" type="color" value={`#${targetColor.toString(16).padStart(6, "0")}`}
            onChange={e => setTargetColor(Number.parseInt(e.target.value.slice(1), 16))} />
        </label>
        <Setting label="Допуск цвета" value={mode.senseTolerance} min={0} max={1} step={0.01}
          onChange={senseTolerance => onChange({ senseTolerance })}
          help="Большее значение позволяет сенсору реагировать на более широкий диапазон цветов." />
      </>}
    </>}
  </>;
}

function availableTargets(kind: number): number[] {
  switch (kind) {
    case 8: return [8, 9];
    case 12: return [8, 10, 11, 12];
    case 13: return [0, 1, 2, 3, 4, 5, 6, 7, 8];
    case 14: return [8, 15];
    case 15: return [8, 16, 17];
    case 17: return [8, 13, 14];
    default: return [8];
  }
}

function ProgramEditor({ mode, onChange }: ModeSettingsProps) {
  const targets = availableTargets(mode.kind);
  function update(index: number, changes: Partial<Program>) {
    onChange({ programs: mode.programs.map((program, at) => at === index ? { ...program, ...changes } : program) });
  }
  return <section className="genome-programs">
    <h3>Программируемые параметры</h3>
    <p>Уравнения читают состояние клетки и задают выбранный параметр. Для одного выхода их результаты складываются.</p>
    {mode.programs.map((program, index) => {
      const supported = targets.includes(program.target);
      return <fieldset className="genome-program" key={index}>
        <legend>Уравнение {index + 1}</legend>
        <label className="genome-cell-type">Изменяемый параметр
          <select aria-label={`Уравнение ${index + 1}. Параметр`} value={supported ? program.target : ""}
            onChange={e => update(index, { target: Number(e.target.value) })}>
            {!supported && <option value="" disabled>Выберите параметр</option>}
            {targets.map(target => <option key={target} value={target}>{PROGRAM_TARGETS[target]}</option>)}
          </select>
        </label>
        {supported ? <>
          <label className="genome-cell-type">Источник значения
            <select aria-label={`Уравнение ${index + 1}. Источник`} value={program.input}
              onChange={e => update(index, { input: Number(e.target.value) })}>
              {PROGRAM_INPUTS.map((name, input) => <option key={input} value={input}>{name}</option>)}
            </select>
          </label>
          <label className="genome-cell-type">Зависимость
            <select aria-label={`Уравнение ${index + 1}. Зависимость`} value={program.kind}
              onChange={e => update(index, { kind: Number(e.target.value) })}>
              <option value={0}>Линейная</option><option value={1}>Пороговая</option>
            </select>
          </label>
          <Setting label={`Уравнение ${index + 1}. Коэффициент`} value={program.scale} min={-1000} max={1000} step={0.1}
            onChange={scale => update(index, { scale })} help="Умножается на входное значение." />
          <Setting label={`Уравнение ${index + 1}. Смещение`} value={program.offset} min={-1000} max={1000} step={0.1}
            onChange={offset => update(index, { offset })} help="Добавляется после умножения входа на коэффициент." />
          {program.kind === 1 && <>
            <Setting label={`Уравнение ${index + 1}. Порог`} value={program.threshold} min={-1000} max={1000} step={0.1}
              onChange={threshold => update(index, { threshold })}
              help="При входе не ниже порога применяется линейное выражение. При меньшем входе используется отдельное значение." />
            <Setting label={`Уравнение ${index + 1}. Ниже порога`} value={program.below} min={-1000} max={1000} step={0.1}
              onChange={below => update(index, { below })} help="Выходное значение при входе ниже порога." />
          </>}
        </> : <p>Уравнение для параметра «{PROGRAM_TARGETS[program.target]}» приостановлено. Выберите доступный параметр, чтобы применить его к этому типу клетки.</p>}
        <button type="button" onClick={() => onChange({ programs: mode.programs.filter((_, at) => at !== index) })}>
          Удалить уравнение {index + 1}
        </button>
      </fieldset>;
    })}
    <button type="button" disabled={mode.programs.length >= 8}
      onClick={() => onChange({ programs: [...mode.programs, { ...newProgram(), target: targets[0] }] })}>
      Добавить уравнение
    </button>
    <p>{mode.programs.length} из 8 уравнений</p>
  </section>;
}

export function CellSettings({ mode, genome, onChange }: CellSettingsProps) {
  const setting = (label: string, key: NumericModeKey, min: number, max: number, step: number, help: string) =>
    <Setting label={label} value={mode[key]} min={min} max={max} step={step} help={help}
      onChange={value => onChange({ [key]: value })} />;
  return <>
    {mode.kind === 3 && <section><h3>Передача генома</h3>
      <ModeReference label="Копировать начиная с слота" value={mode.virusCopyFrom} genome={genome}
        emptyLabel="Весь геном" onChange={virusCopyFrom => onChange({ virusCopyFrom })} />
      <p>При выбранном слоте копируются он и два поколения его потомков.</p>
    </section>}
    {mode.kind === 8 && <section><h3>Плавучесть</h3>
      {setting("Плотность клетки", "density", -3.3, 3.3, 0.1, "Определяет всплытие и оседание при включённой гравитации.")}
    </section>}
    {mode.kind === 9 && <section><h3>Приклеивание</h3>
      <ConnectionLimit mode={mode} onChange={onChange} />
    </section>}
    {[10, 11].includes(mode.kind) && <section><h3>Сенсор</h3><SensorSettings mode={mode} onChange={onChange} /></section>}
    {mode.kind === 12 && <section><h3>Мышца</h3>
      {setting("Сокращение", "contraction", -1, 1, 0.05, "Меняет длину постоянных связей миоцита.")}
      {setting("Изгиб", "bending", -1, 1, 0.05, "Поворачивает связанную структуру в плоскости чашки.")}
      {setting("Подъём", "lift", -1, 1, 0.05, "Создаёт направленное вертикальное воздействие на связанные клетки.")}
    </section>}
    {mode.kind === 13 && <section><h3>Нейросигналы</h3>
      <Channel value={mode.signalChannel} onChange={signalChannel => onChange({ signalChannel })} />
      {setting("Выходной сигнал", "sensorGain", -16, 16, 0.1, "Базовый выход нейроцита. Уравнения позволяют задать зависимость от входных сигналов, возраста и других величин.")}
    </section>}
    {mode.kind === 14 && <section><h3>Секреция</h3>
      <label className="genome-cell-type">Вещество или воздействие
        <select value={mode.secretion} onChange={e => onChange({ secretion: Number(e.target.value) })}>
          {SECRETIONS.map((name, value) => <option key={value} value={value}>{name}</option>)}
        </select>
      </label>
      {setting("Интенсивность секреции", "secretionRate", 0, 4, 0.05, "Секреция расходует азот. Интенсивность также можно задать уравнением.")}
    </section>}
    {mode.kind === 15 && <section><h3>Дифференциация</h3>
      <Channel value={mode.signalChannel} onChange={signalChannel => onChange({ signalChannel })} />
      <ModeReference label="Слот перехода A" value={mode.transitionA} genome={genome}
        onChange={transitionA => onChange({ transitionA })} />
      {setting("Порог перехода A", "transitionThresholdA", -1000, 1000, 0.1, "Переход A готов, когда входной сигнал превышает этот порог. Он имеет приоритет перед переходом B.")}
      <ModeReference label="Слот перехода B" value={mode.transitionB} genome={genome}
        onChange={transitionB => onChange({ transitionB })} />
      {setting("Порог перехода B", "transitionThresholdB", -1000, 1000, 0.1, "Переход B выполняется, если готов его сигнал и не выбран переход A. Уравнения могут задавать условия вместо сигнала.")}
    </section>}
    {mode.kind === 16 && <section><h3>Слияние гамет</h3>
      <label className="genome-cell-type">Совместимость гамет
        <select value={mode.compatibleMode} onChange={e => onChange({ compatibleMode: Number(e.target.value) })}>
          <optgroup label="Слот партнёра">
            {genome.map((partner, index) => <option key={index} value={index}>{partner.name} · {TYPES[partner.kind].name}</option>)}
          </optgroup>
          <optgroup label="Группа совместимости">
            {Array.from({ length: GAMETE_GROUP_COUNT }, (_, group) =>
              <option key={group} value={-(group + 1)}>Группа {String.fromCharCode(65 + group)}</option>)}
          </optgroup>
        </select>
      </label>
      <ModeReference label="Слот потомка" value={mode.offspringMode} genome={genome}
        onChange={offspringMode => onChange({ offspringMode })} />
      <label className="genome-check"><input type="checkbox" checked={mode.telomeres >= 0}
        onChange={e => onChange({ telomeres: e.target.checked ? 16 : -1 })} />Ограничить число поколений потомка</label>
      {mode.telomeres >= 0 && <Setting label="Теломеры потомка" value={mode.telomeres} min={0} max={1000} step={1}
        onChange={telomeres => onChange({ telomeres: Math.round(telomeres) })}
        help="Запас уменьшается при делении. При нулевом запасе клетка погибает." />}
      <p>Гамета размножается слиянием с другой гаметой.</p>
    </section>}
    {mode.kind === 17 && <section><h3>Реснички</h3>
      {setting("Скорость левых ресничек", "ciliaLeft", -60, 60, 1, "Знак задаёт направление переноса соприкасающихся клеток левой стороной.")}
      {setting("Скорость правых ресничек", "ciliaRight", -60, 60, 1, "Независимо управляет переносом клеток правой стороной.")}
    </section>}
    <ProgramEditor mode={mode} onChange={onChange} />
  </>;
}
