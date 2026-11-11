import { SIGNAL_COUNT } from "../model";

export function CellDetails({ values }: { values?: Float32Array }) {
  if (!values?.length) return null;
  const rows: [string, string][] = Array.from({ length: SIGNAL_COUNT }, (_, i) => [
    `S${i + 1}`, values[i].toFixed(3),
  ]);
  rows.push(
    ["Токсин", values[8].toFixed(3)],
    ["Повреждение", values[9].toFixed(3)],
    ["Теломеры", values[10] < 0 ? "Без ограничения" : String(values[10])],
    ["Связь со стенкой", values[12] ? "Да" : "Нет"],
  );
  return <dl className="cell-details">{rows.map(([label, value]) =>
    <div key={label}><dt>{label}</dt><dd>{value}</dd></div>
  )}</dl>;
}
