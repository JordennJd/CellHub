import type { Frame } from "./model";
export type ViewState = { zoom: number; x: number; y: number };
export type WorldViewProps = {
  frame: Frame | null;
  tool: string;
  layer: string;
  selected: number | null;
  onPoint: (x: number, y: number, id: number | null) => void;
  onView: (v: ViewState) => void;
  onRenderer: (name: string) => void;
  onError: (message: string) => void;
  recenter: number;
};
