import { useState } from "react";
import { RustWorldView, type BackendPreference } from "./RustWorldView";
import type { WorldViewProps } from "./world-view-types";
export type { ViewState } from "./world-view-types";

const initialBackend = (): BackendPreference => {
  const requested = new URLSearchParams(location.search).get("renderer");
  return requested === "webgl" || requested === "webgpu" ? requested : "auto";
};

export function WorldView(props: WorldViewProps) {
  const [backend, setBackend] = useState(initialBackend);
  return (
    <RustWorldView
      key={backend}
      {...props}
      backend={backend}
      onError={(message) => {
        // A lost WebGPU device cannot be recovered in place. Remount with a
        // fresh canvas; a canvas that once held WebGPU can never become WebGL2.
        if (backend === "auto" && message.startsWith("WebGPU device lost")) {
          console.warn("CellHub: switching to WebGL", message);
          setBackend("webgl");
          return;
        }
        props.onError(message);
      }}
    />
  );
}
