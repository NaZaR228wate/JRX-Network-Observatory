// Mirrors the serialised shapes from jrx-core. Host -> UI only; the renderer
// never sends these back (ARCHITECTURE.md §10).

export type CapabilityState =
  | { state: "observed"; mechanism: string }
  | { state: "available"; missing: string }
  | { state: "not_possible"; reason: string };

export interface CapabilityRow {
  probe: string;
  describes: string;
  state: CapabilityState;
}

export interface RefusedRow {
  class: string;
}

export interface CapabilityMatrix {
  rows: CapabilityRow[];
  refused: RefusedRow[];
}
