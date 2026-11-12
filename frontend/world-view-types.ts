import type { BackendPreference } from "./backend";
import type { Frame } from "./model";
export type ViewState = { zoom: number; x: number; y: number };
export type WorldViewProps = {
  frame: Frame | null;
  tool: string;
  layer: string;
  selected: number | null;
  /** Arena owner id -> packed RGB. Cells with an owner get a coloured halo. */
  playerColors?: Record<number, number>;
  onPoint: (x: number, y: number, id: number | null, action: "place" | "select") => void;
  onView: (v: ViewState) => void;
  onError: (message: string) => void;
  onBackend?: (backend: BackendPreference) => void;
  recenter: number;
  zoomStep?: number;
};
