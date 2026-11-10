import { useState } from "react";
export type LabSettings = {
    confirmExit: boolean;
    confirmReset: boolean;
    pauseWhenHidden: boolean;
};
export const DEFAULT_SETTINGS: LabSettings = { confirmExit: true, confirmReset: true, pauseWhenHidden: true };
const KEY = "cellhub.settings.v1";
export function readSettings(): LabSettings {
    try {
        const raw = JSON.parse(localStorage.getItem(KEY) ?? "null");
        if (!raw || ["confirmExit", "confirmReset", "pauseWhenHidden"].some(k => typeof raw[k] !== "boolean"))
            return { ...DEFAULT_SETTINGS };
        return { confirmExit: raw.confirmExit, confirmReset: raw.confirmReset, pauseWhenHidden: raw.pauseWhenHidden };
    }
    catch {
        return { ...DEFAULT_SETTINGS };
    }
}
export function saveSettings(settings: LabSettings) {
    try {
        localStorage.setItem(KEY, JSON.stringify(settings));
    }
    catch {
        throw new Error("Браузер не разрешил сохранить настройки.");
    }
}
export function Settings({ onClose }: {
    onClose: () => void;
}) {
    const [settings, setSettings] = useState(readSettings);
    const [error, setError] = useState("");
    function update(key: keyof LabSettings, value: boolean) {
        try {
            const next = { ...settings, [key]: value };
            saveSettings(next);
            setSettings(next);
            setError("");
        }
        catch (e) {
            setError(e instanceof Error ? e.message : "Не удалось сохранить настройки.");
        }
    }
    return <section className="settings-panel" aria-labelledby="settings-title">
    <div className="section-heading"><h2 id="settings-title">Настройки</h2><button onClick={onClose}>Назад</button></div>
    <label className="setting-row"><input type="checkbox" checked={settings.confirmExit} onChange={e => update("confirmExit", e.target.checked)}/>Подтверждать выход из лаборатории</label>
    <p className="muted">Сохранённые записи останутся в библиотеке. Текущий несохранённый эксперимент будет закрыт.</p>
    <label className="setting-row"><input type="checkbox" checked={settings.confirmReset} onChange={e => update("confirmReset", e.target.checked)}/>Подтверждать очистку и перезапуск</label>
    <label className="setting-row"><input type="checkbox" checked={settings.pauseWhenHidden} onChange={e => update("pauseWhenHidden", e.target.checked)}/>Ставить на паузу при переключении вкладки</label>
    {error && <p role="alert" className="error-message">{error}</p>}
  </section>;
}
