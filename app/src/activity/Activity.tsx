import { useMemo, useState } from "react";
import type {
  ActivityHealth,
  ActivitySnapshot,
  ConnectionActivity,
  ProcessActivity,
} from "../types";
import {
  SORT_OPTIONS,
  type SortKey,
  bytes,
  displayName,
  rate,
  searchPrograms,
  sortPrograms,
} from "./rank";

/** This Mac's own live network activity.
 *
 *  Shows which programs are talking, how much, and to whose network. It does
 *  not show websites, and says so where that would otherwise be assumed —
 *  see PRODUCT_BOUNDARIES.md. */
export function Activity({ snapshot }: { snapshot: ActivitySnapshot | null }) {
  const [sort, setSort] = useState<SortKey>("activity");
  const [query, setQuery] = useState("");
  const [openPid, setOpenPid] = useState<number | null>(null);

  const programs = useMemo(() => {
    if (!snapshot) return [];
    return sortPrograms(searchPrograms(snapshot.programs, query), sort);
  }, [snapshot, query, sort]);

  const open = programs.find((p) => p.pid === openPid) ?? null;

  return (
    <section className="activity">
      <h3>Activity</h3>

      <LiveTotals snapshot={snapshot} />

      {snapshot && <HealthLine health={snapshot.health} />}

      {snapshot?.health.state !== "no_network" && (
        <>
          <div className="programs-head">
            <h4>Programs active now</h4>
            <div className="programs-controls">
              <input
                className="search"
                type="search"
                placeholder="Search program or network owner…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                aria-label="Search programs by observed facts"
              />
              <select
                className="sort"
                value={sort}
                onChange={(e) => setSort(e.target.value as SortKey)}
                aria-label="Sort programs"
              >
                {SORT_OPTIONS.map((o) => (
                  <option key={o.key} value={o.key}>
                    {o.label}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {snapshot?.health.state === "initializing" && programs.length === 0 ? (
            <p className="note preparing">Preparing program activity…</p>
          ) : programs.length === 0 ? (
            <p className="note">
              {query
                ? "No program matches that."
                : "No program has moved data since JRX started watching."}
            </p>
          ) : (
            <ul className="programs">
              {programs.map((program) => (
                <ProgramRow
                  key={`${program.pid}-${program.process_name}`}
                  program={program}
                  open={program.pid === openPid}
                  onToggle={() =>
                    setOpenPid(program.pid === openPid ? null : program.pid)
                  }
                />
              ))}
            </ul>
          )}
        </>
      )}

      {open && <ProgramDetail program={open} />}
    </section>
  );
}

function LiveTotals({ snapshot }: { snapshot: ActivitySnapshot | null }) {
  const live = snapshot !== null;
  return (
    <div className="live">
      <div className="live-head">
        <span className={`dot ${live ? "on" : ""}`} aria-hidden="true" />
        <span>This Mac — live</span>
        {snapshot && <span className="mono live-iface">{snapshot.interface}</span>}
      </div>

      <div className="live-rates">
        <Metric arrow="↓" value={snapshot ? rate(snapshot.rate_in) : "—"} label="download" />
        <Metric arrow="↑" value={snapshot ? rate(snapshot.rate_out) : "—"} label="upload" />
      </div>

      <dl className="live-totals">
        <div>
          <dt>Observed this session</dt>
          <dd>
            {snapshot ? (
              <>
                ↓ <span className="mono">{bytes(snapshot.session_bytes_in)}</span>
                {"  "}↑ <span className="mono">{bytes(snapshot.session_bytes_out)}</span>
              </>
            ) : (
              "—"
            )}
          </dd>
          {/* Two different questions, kept apart on purpose. */}
          <div className="note">
            Only what JRX has watched move since you opened it — not what this
            Mac has sent all along.
          </div>
        </div>
        <div>
          <dt>Active connections</dt>
          <dd className="mono">{snapshot ? snapshot.active_connections : "—"}</dd>
        </div>
      </dl>
    </div>
  );
}

function Metric({ arrow, value, label }: { arrow: string; value: string; label: string }) {
  return (
    <div className="metric">
      <span className="metric-arrow">{arrow}</span>
      <span className="metric-value mono">{value}</span>
      <span className="metric-label">{label}</span>
    </div>
  );
}

/** Honest states. A parser error is never the headline. */
function HealthLine({ health }: { health: ActivityHealth }) {
  switch (health.state) {
    case "full":
      return null;
    case "initializing":
      return (
        <div className="banner off">
          <strong>Network activity is available. Preparing program details…</strong>
          <div className="note">
            macOS takes a few seconds to make per-program figures available the
            first time after starting up.
          </div>
        </div>
      );
    case "limited":
      return (
        <div className="banner warn">
          <strong>
            JRX can measure this Mac's total network activity, but program-level
            details are currently unavailable.
          </strong>
          <details className="diagnostic">
            <summary>Technical detail</summary>
            <span className="mono">{health.reason}</span>
          </details>
        </div>
      );
    case "no_network":
      return (
        <div className="banner off">
          <strong>No active network connection.</strong>
          <div className="note">There is nothing for JRX to measure right now.</div>
        </div>
      );
  }
}

function ProgramRow({
  program,
  open,
  onToggle,
}: {
  program: ProcessActivity;
  open: boolean;
  onToggle: () => void;
}) {
  const quiet = program.rate_in === 0 && program.rate_out === 0;
  return (
    <li className={`program ${open ? "open" : ""} ${quiet ? "quiet" : ""}`}>
      <button onClick={onToggle} aria-expanded={open}>
        <span className="program-name">
          {displayName(program)}
          {program.name_is_truncated && (
            <span className="note" title="macOS reported a shortened name and the process had already exited">
              {" "}(name shortened)
            </span>
          )}
        </span>
        <span className="program-bytes mono">
          ↓ {bytes(program.session_bytes_in)}
        </span>
        <span className="program-bytes mono">
          ↑ {bytes(program.session_bytes_out)}
        </span>
        <span className="program-conns">
          {program.active_connections}{" "}
          {program.active_connections === 1 ? "connection" : "connections"}
        </span>
      </button>
    </li>
  );
}

function ProgramDetail({ program }: { program: ProcessActivity }) {
  return (
    <aside className="program-detail" aria-label={`${displayName(program)} detail`}>
      <div className="detail-head">
        <div>
          <h4>{displayName(program)}</h4>
          <p className="note mono">
            {program.executable_path ?? program.process_name} · PID {program.pid}
          </p>
        </div>
      </div>

      <dl className="detail-metrics">
        <div>
          <dt>Observed this session</dt>
          <dd className="mono">
            ↓ {bytes(program.session_bytes_in)} ↑ {bytes(program.session_bytes_out)}
          </dd>
        </div>
        <div>
          <dt>Current rate</dt>
          <dd className="mono">
            ↓ {rate(program.rate_in)} ↑ {rate(program.rate_out)}
          </dd>
        </div>
        <div>
          <dt>Connections</dt>
          <dd className="mono">{program.active_connections}</dd>
        </div>
      </dl>

      <h5>Destinations</h5>
      <p className="note">
        JRX knows which network a connection goes to when that network publishes
        its address ranges. It does not know which website — those are different
        things, and JRX will not guess.
      </p>

      {program.connections.length === 0 ? (
        <p className="state">No connections observed.</p>
      ) : (
        <ul className="destinations">
          {program.connections.slice(0, 20).map((c, i) => (
            <Destination key={`${c.remote_address}-${c.remote_port}-${i}`} connection={c} />
          ))}
        </ul>
      )}
    </aside>
  );
}

function Destination({ connection }: { connection: ConnectionActivity }) {
  return (
    <li className={connection.is_open ? "" : "closed"}>
      <div className="dest-line">
        <span className="mono dest-addr">
          {connection.remote_address ?? "no peer"}
          {connection.remote_port !== null && `:${connection.remote_port}`}
        </span>
        <span className="dest-proto">{connection.protocol.toUpperCase()}</span>
        {connection.state && <span className="dest-state">{connection.state}</span>}
        {connection.rtt_ms !== null && (
          <span className="mono dest-rtt">{connection.rtt_ms.toFixed(0)} ms</span>
        )}
        {!connection.is_open && <span className="dest-state">closed</span>}
      </div>
      <div className="dest-line">
        <span className="note">
          {connection.network_owner ? (
            <>
              Network owner: <strong>{connection.network_owner}</strong>
              {" — this address belongs to their published range. It is not the site you visited."}
            </>
          ) : (
            "Network owner unavailable"
          )}
        </span>
        <span className="mono dest-bytes">
          ↓ {bytes(connection.session_bytes_in)} ↑ {bytes(connection.session_bytes_out)}
        </span>
      </div>
    </li>
  );
}
