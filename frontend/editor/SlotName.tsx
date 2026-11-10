import { useState } from "react";

export function SlotName({ name, onRename }: { name: string; onRename: (name: string) => void }) {
  const [draft, setDraft] = useState(name);
  function commit() {
    const value = draft.trim() || name;
    setDraft(value);
    onRename(value);
  }
  return <label className="select-label slot-name">
    Имя слота
    <input type="text" value={draft} onChange={(e) => setDraft(e.target.value)}
      onBlur={commit} onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") { e.preventDefault(); setDraft(name); }
      }} />
  </label>;
}
