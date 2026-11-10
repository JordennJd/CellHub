import { useEffect, useRef, useState } from "react";
import { downloadEntry, importEntry, listEntries, parseEntry, removeEntry, renameEntry, type LibraryEntry, type LibraryKind } from "./storage";
export function Library({ kind, onLoad, onClose }: {
    kind: LibraryKind;
    onLoad: (entry: LibraryEntry) => void;
    onClose: () => void;
}) {
    const [error, setError] = useState("");
    const [entries, setEntries] = useState<LibraryEntry[]>([]);
    const [renaming, setRenaming] = useState<string | null>(null);
    const [name, setName] = useState("");
    const input = useRef<HTMLInputElement>(null);
    const refresh = () => setEntries(listEntries(kind));
    function action(fn: () => void) {
        setError("");
        try {
            fn();
        }
        catch (e) {
            setError(e instanceof Error ? e.message : "Не удалось выполнить действие.");
        }
    }
    // Initial errors must be visible, without allowing a corrupt library to be replaced.
    useEffect(() => {
        try {
            setEntries(listEntries(kind));
            setError("");
        }
        catch (e) {
            setError(e instanceof Error ? e.message : "Не удалось открыть библиотеку.");
        }
    }, [kind]);
    return <section className="library-panel" aria-labelledby="library-title">
    <div className="section-heading">
      <h2 id="library-title">{kind === "genome" ? "Библиотека геномов" : "Сохранённые эксперименты"}</h2>
      <button onClick={onClose}>Назад</button>
    </div>
    <p>{kind === "genome" ? "Загрузите конструкцию в редактор или перенесите её в файл." : "Откройте сохранённую чашку, чтобы продолжить эксперимент."}</p>
    <button onClick={() => input.current?.click()}>Импортировать из файла</button>
    <input ref={input} hidden type="file" accept=".json,application/json" aria-label="Импортировать сохранение" onChange={async (e) => {
            const file = e.currentTarget.files?.[0];
            e.currentTarget.value = "";
            if (!file)
                return;
            setError("");
            try {
                if (file.size > 20 * 1024 * 1024)
                    throw new Error("Файл слишком большой. Максимальный размер — 20 МБ.");
                const text = await file.text();
                if (parseEntry(text).kind !== kind)
                    throw new Error(kind === "genome" ? "Это эксперимент. Откройте его в разделе сохранённых экспериментов." : "Это геном. Откройте его в библиотеке геномов.");
                importEntry(text);
                refresh();
            }
            catch (e) {
                setError(e instanceof Error ? e.message : "Не удалось импортировать файл.");
            }
        }}/>
    {error && <p role="alert" className="error-message">{error}</p>}
    {!error && !entries.length && <p className="empty-message">{kind === "genome" ? "Сохранённых геномов пока нет. Создайте конструкцию в лаборатории и сохраните её." : "Сохранённых экспериментов пока нет. Сохраните чашку в лаборатории, чтобы продолжить позже."}</p>}
    <ul className="library-list">
      {entries.map(entry => <li key={entry.id}>
        {renaming === entry.id ? <form onSubmit={e => { e.preventDefault(); action(() => { renameEntry(entry.id, name); setRenaming(null); refresh(); }); }}>
          <label>Новое название<input autoFocus value={name} maxLength={80} onChange={e => setName(e.target.value)}/></label>
          <div className="button-row"><button type="submit">Сохранить название</button><button type="button" onClick={() => setRenaming(null)}>Отмена</button></div>
        </form> : <>
          <h3>{entry.name}</h3>
          <p className="muted">{new Date(entry.updatedAt).toLocaleString("ru-RU", { dateStyle: "medium", timeStyle: "short" })}</p>
          <div className="button-row">
            <button className="primary" onClick={() => action(() => onLoad(entry))}>Открыть</button>
            <button onClick={() => { setRenaming(entry.id); setName(entry.name); }}>Переименовать</button>
            <button onClick={() => action(() => downloadEntry(entry))}>Экспортировать</button>
            <button className="danger" onClick={() => {
                    if (window.confirm(`Удалить «${entry.name}»? Это действие нельзя отменить.`))
                        action(() => { removeEntry(entry.id); refresh(); });
                }}>Удалить</button>
          </div>
        </>}
      </li>)}
    </ul>
  </section>;
}
