import { lazy, Suspense, useState } from "react";
import { RustWorldView } from "./RustWorldView";
import type { WorldViewProps } from "./world-view-types";
export type { ViewState } from "./world-view-types";

const PixiWorldView = lazy(() => import("./PixiWorldView").then((module) => ({ default: module.PixiWorldView })));

export function WorldView(props: WorldViewProps) {
  const [fallback, setFallback] = useState(() =>
    !("gpu" in navigator) || new URLSearchParams(location.search).get("renderer") === "webgl",
  );
  if (fallback) return <Suspense fallback={<div className="world" role="status">Загрузка WebGL…</div>}>
    <PixiWorldView {...props} />
  </Suspense>;
  return <RustWorldView {...props} onError={(message) => {
    console.warn("CellHub: switching to WebGL", message);
    setFallback(true);
  }} />;
}
