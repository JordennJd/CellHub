import { RustWorldView } from "./RustWorldView";
import type { BackendPreference } from "./backend";
import type { WorldViewProps } from "./world-view-types";
export type { ViewState } from "./world-view-types";

type Props = WorldViewProps & {
  backend: BackendPreference;
  onBackendFailure: (backend: BackendPreference, message: string) => void;
};

export function WorldView({
  backend,
  onBackendFailure,
  ...props
}: Props) {
  return (
    <RustWorldView
      key={backend}
      {...props}
      backend={backend}
      onBackendFailure={(message) => onBackendFailure(backend, message)}
    />
  );
}
