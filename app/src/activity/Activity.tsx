import { useMemo, useState } from "react";
import type { ActivityHealth, ActivitySnapshot, ConnectionActivity } from "../types";
import {
  SORT_OPTIONS,
  type SortKey,
  type ProgramGroup,
  bytes,
  groupPrograms,
  rate,
  searchGroups,
  sortGroups,
} from "./rank";

/** This Mac's own live network activity.
 *
 *  Shows which programs are talking, how much, and to whose network. It does
 *  not show websites, and says so where that would otherwise be assumed —
 *  see PRODUCT_BOUNDARIES.md. */
export function Activity({ snapshot }: { snapshot: ActivitySnapshot | null }) {
  const [sort, setSort] = useState<SortKey>("activity");
  const [query, setQuery] = useState("");
  const [openKey, setOpenKey] = useState<string | null>(null);

  const groups = useMemo(() => {
    if (!snapshot) return [];
    return sortGroups(searchGroups(groupPrograms(snapshot.programs), query), sort);
  }, [snapshot, query, sort]);

  const open = groups.find((g) => g.key === openKey) ?? null;

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

          <UnattributedNote snapshot={snapshot} groups={groups} />

          {snapshot?.health.state === "initializing" && groups.length === 0 ? (
            <p className="note preparing">Preparing program activity…</p>
          ) : groups.length === 0 ? (
            <p className="note">
              {query
                ? "No program matches that."
                : "No program has moved data since JRX started watching."}
            </p>
          ) : (
            <ul className="programs">
              {groups.map((group) => (
                <ProgramRow
                  key={group.key}
                  group={group}
                  open={group.key === openKey}
                  onToggle={() => setOpenKey(group.key === openKey ? null : group.key)}
                />
              ))}
            </ul>
          )}
        </>
      )}

      {open && <ProgramDetail group={open} />}
    </section>
  );
}

/** Program totals can fall short of the interface total, and the reason is
 *  worth saying rather than leaving the reader to notice a discrepancy.
 *
 *  A connection that opens and closes between two samples is counted by the
 *  interface but cannot be attributed: JRX never watched its counter move, and
 *  claiming the whole figure would also claim traffic from before it was
 *  watching. Under-reporting is the honest side to err on. */
function UnattributedNote({
  snapshot,
  groups,
}: {
  snapshot: ActivitySnapshot | null;
  groups: ProgramGroup[];
}) {
  if (!snapshot) return null;

  const attributed = groups.reduce(
    (sum, g) => sum + g.session_bytes_in + g.session_bytes_out,
    0,
  );
  const total = snapshot.session_bytes_in + snapshot.session_bytes_out;
  // Only worth mentioning once the gap is both real and large.
  if (total < 65_536 || attributed >= total * 0.75) return null;

  return (
    <p className="note unattributed">
      Programs below account for {bytes(attributed)} of the{" "}
      {bytes(total)} observed. Connections that open and close between two
      samples are counted for this Mac but cannot be attributed to a program —
      JRX never saw their totals change, and guessing would mean claiming
      traffic it did not watch.
    </p>
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
  group,
  open,
  onToggle,
}: {
  group: ProgramGroup;
  open: boolean;
  onToggle: () => void;
}) {
  const quiet = group.rate_in === 0 && group.rate_out === 0;
  return (
    <li className={`program ${open ? "open" : ""} ${quiet ? "quiet" : ""}`}>
      <button onClick={onToggle} aria-expanded={open}>
        <span className="program-name">
          {group.label}
          {group.process_count > 1 && (
            <span className="note" title="Several processes of the same application, merged into one row">
              {" "}· {group.process_count} processes
            </span>
          )}
          {group.name_is_truncated && (
            <span className="note" title="macOS reported a shortened name and the process had already exited">
              {" "}(name shortened)
            </span>
          )}
        </span>
        <span className="program-bytes mono">
          ↓ {bytes(group.session_bytes_in)}
        </span>
        <span className="program-bytes mono">
          ↑ {bytes(group.session_bytes_out)}
        </span>
        <span className="program-conns">
          {group.active_connections}{" "}
          {group.active_connections === 1 ? "connection" : "connections"}
        </span>
      </button>
    </li>
  );
}

function ProgramDetail({ group }: { group: ProgramGroup }) {
  return (
    <aside className="program-detail" aria-label={`${group.label} detail`}>
      <div className="detail-head">
        <div>
          <h4>{group.label}</h4>
          <p className="note mono">
            {group.process_count > 1
              ? `${group.process_count} processes · PIDs ${group.pids.join(", ")}`
              : `${group.executable_path ?? group.process_names[0]} · PID ${group.pids[0]}`}
          </p>
        </div>
      </div>

      <dl className="detail-metrics">
        <div>
          <dt>Observed this session</dt>
          <dd className="mono">
            ↓ {bytes(group.session_bytes_in)} ↑ {bytes(group.session_bytes_out)}
          </dd>
        </div>
        <div>
          <dt>Current rate</dt>
          <dd className="mono">
            ↓ {rate(group.rate_in)} ↑ {rate(group.rate_out)}
          </dd>
        </div>
        <div>
          <dt>Connections</dt>
          <dd className="mono">{group.active_connections}</dd>
        </div>
      </dl>

      <h5>Destinations</h5>
      <p className="note">
        JRX knows which network a connection goes to when that network publishes
        its address ranges. It does not know which website — those are different
        things, and JRX will not guess.
      </p>

      {group.connections.length === 0 ? (
        <p className="state">No connections observed.</p>
      ) : (
        <ul className="destinations">
          {group.connections.slice(0, 20).map((c, i) => (
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
