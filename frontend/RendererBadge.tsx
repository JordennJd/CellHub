import type { BackendPreference } from "./backend";

const LABELS: Record<BackendPreference, string> = {
  webgpu: "WebGPU",
  webgl: "WebGL2",
};

export function RendererBadge({
  backend,
}: {
  backend: BackendPreference | null;
}) {
  if (!backend) return null;
  const label = LABELS[backend];
  return (
    <span
      className="renderer-badge"
      data-backend={backend}
      title={`Активный графический backend: ${label}`}
      aria-label={`Активный графический backend: ${label}`}
    >
      <span className="renderer-badge-dot" aria-hidden="true" />
      Графика: {label}
    </span>
  );
}
