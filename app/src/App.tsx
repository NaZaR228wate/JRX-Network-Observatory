import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CapabilityMatrix } from "./types";

// M0 scaffold only. This proves the vertical slice — collector declarations
// reach the renderer through the capability model — and nothing more.
// Visual design lands in M2/M6; the topology in M4.
export function App() {
  const [matrix, setMatrix] = useState<CapabilityMatrix | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<CapabilityMatrix>("get_capabilities")
      .then(setMatrix)
      .catch((e: unknown) => setError(String(e)));
  }, []);

  if (error) return <pre>error: {error}</pre>;
  if (!matrix) return <p>Loading…</p>;

  return (
    <main>
      <h1>JRX Network Observatory</h1>
      <p>M0 foundation — capability model reaching the UI.</p>

      <h2>Capabilities</h2>
      <ul>
        {matrix.rows.map((row) => (
          <li key={row.probe}>
            {row.describes} — <strong>{row.state.state}</strong>
            {row.state.state === "observed" && ` via ${row.state.mechanism}`}
            {row.state.state === "available" && ` (needs ${row.state.missing})`}
            {row.state.state === "not_possible" && ` (${row.state.reason})`}
          </li>
        ))}
      </ul>

      <h2>Refused by design</h2>
      <ul>
        {matrix.refused.map((row) => (
          <li key={row.class}>{row.class}</li>
        ))}
      </ul>
    </main>
  );
}
