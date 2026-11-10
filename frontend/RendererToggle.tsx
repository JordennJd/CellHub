import type { BackendPreference } from "./backend";

export function RendererToggle({
  value,
  webgpuBlocked,
  onChange,
}: {
  value: BackendPreference;
  webgpuBlocked: boolean;
  onChange: (value: BackendPreference) => void;
}) {
  return (
    <div className="renderer-toggle" role="group" aria-label="Графический backend">
      <button
        type="button"
        data-backend="webgpu"
        disabled={webgpuBlocked}
        aria-pressed={value === "webgpu"}
        aria-label={webgpuBlocked ? "WebGPU недоступен" : "WebGPU"}
        title={
          webgpuBlocked
            ? "WebGPU недоступен. Перезагрузите страницу, чтобы повторить попытку."
            : "Использовать WebGPU"
        }
        className={value === "webgpu" ? "active" : ""}
        onClick={() => onChange("webgpu")}
      >
        WebGPU
        {webgpuBlocked && (
          <span className="renderer-blocked" aria-hidden="true">
            ✕
          </span>
        )}
      </button>
      <button
        type="button"
        data-backend="webgl"
        aria-pressed={value === "webgl"}
        title="Использовать WebGL2"
        className={value === "webgl" ? "active" : ""}
        onClick={() => onChange("webgl")}
      >
        WebGL
      </button>
    </div>
  );
}
