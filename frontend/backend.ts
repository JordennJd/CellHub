export type BackendPreference = "webgpu" | "webgl";

/** WebGPU is attempted on every load unless the URL explicitly asks for WebGL. */
export function initialBackend(): BackendPreference {
  const requested = new URLSearchParams(location.search).get("renderer");
  return requested === "webgl" ? "webgl" : "webgpu";
}
