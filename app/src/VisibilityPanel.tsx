import { invoke } from "@tauri-apps/api/core";
import type {
  CapabilityMatrix,
  CapabilityRow,
  Permission,
  PermissionInfo,
} from "./types";

// The Visibility Panel (ARCHITECTURE.md §9). A core feature, not a settings
// screen: it is what separates JRX from a scanner.
export function VisibilityPanel({ matrix }: { matrix: CapabilityMatrix }) {
  const observed = matrix.rows.filter((r) => r.state.state === "observed");
  const available = matrix.rows.filter((r) => r.state.state === "available");
  const blocked = matrix.rows.filter((r) => r.state.state === "not_possible");
  const permission = (id: Permission): PermissionInfo | undefined =>
    matrix.permissions.find((p) => p.permission === id);

  return (
    <section className="panel">
      <div className="panel-head">
        <h3>What JRX can see</h3>
        <p className="note">
          Generated from what each probe declares it reads, checked against the
          permissions this Mac reports right now.
        </p>
      </div>

      <Group tone="ok" title="Observed" count={observed.length}
        blurb="Working now, and how.">
        {observed.map((row) => (
          <Item key={row.probe} label={row.describes}
            detail={row.state.state === "observed" ? row.state.mechanism : ""} />
        ))}
      </Group>

      <Group tone="warn" title="Available" count={available.length}
        blurb="One permission away.">
        {available.map((row) => (
          <PermissionItem key={row.probe} row={row}
            info={row.state.state === "available" ? permission(row.state.missing) : undefined} />
        ))}
      </Group>

      <Group tone="off" title="Not possible" count={blocked.length + matrix.limitations.length}
        blurb="Blocked by the platform at this privilege level. JRX does not ask for administrator access.">
        {blocked.map((row) => (
          <Item key={row.probe} label={row.describes}
            detail={row.state.state === "not_possible" ? row.state.reason : ""} />
        ))}
        {matrix.limitations.map((l) => (
          <Item key={l.describes} label={l.describes} detail={l.reason} />
        ))}
      </Group>

      <Group tone="refused" title="Refused by design" count={matrix.refused.length}
        blurb="Technically possible. Deliberately not built.">
        {matrix.refused.map((r) => (
          <Item key={r.class} label={humanise(r.class)} detail={r.rationale} />
        ))}
      </Group>
    </section>
  );
}

function Group({
  tone, title, count, blurb, children,
}: {
  tone: "ok" | "warn" | "off" | "refused";
  title: string;
  count: number;
  blurb: string;
  children: React.ReactNode;
}) {
  return (
    <div className={`group ${tone}`}>
      <div className="group-head">
        <span className="group-title">{title}</span>
        <span className={`pill ${tone === "refused" ? "" : tone}`}>{count}</span>
      </div>
      <p className="group-blurb">{blurb}</p>
      <ul className="items">{children}</ul>
    </div>
  );
}

function Item({ label, detail }: { label: string; detail: string }) {
  return (
    <li className="item">
      <div className="item-label">{label}</div>
      {detail && <div className="note">{detail}</div>}
    </li>
  );
}

function PermissionItem({
  row, info,
}: {
  row: CapabilityRow;
  info: PermissionInfo | undefined;
}) {
  if (row.state.state !== "available") return null;
  const unverifiable = row.state.certainty === "unverifiable";

  return (
    <li className="item">
      <div className="item-label">{row.describes}</div>
      <div className="note">
        {unverifiable ? (
          <>
            Needs <strong>{info?.label ?? row.state.missing}</strong> access.
            macOS provides no way for JRX to check this in advance, so this is
            our best statement — not something the system told us.
          </>
        ) : info?.state === "not_requested" ? (
          <>
            JRX has not asked for <strong>{info.label}</strong> yet. macOS will
            prompt you the first time it is needed — nothing has been refused.
          </>
        ) : (
          <>
            <strong>{info?.label ?? row.state.missing}</strong> is turned off
            for JRX. macOS told us this directly.
          </>
        )}
        {info && info.state !== "not_requested" && <> {info.grant_hint}.</>}
      </div>
      {info && info.state !== "not_requested" && (
        <button className="grant"
          onClick={() => void invoke("open_privacy_settings", { permission: info.permission })}>
          Open settings
        </button>
      )}
    </li>
  );
}

/** "packet_payload" -> "Packet payload" */
function humanise(raw: string): string {
  const s = raw.replace(/_/g, " ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}
