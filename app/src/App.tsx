import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { connectionLabel, networkLine } from "./labels";
import { Topology } from "./topology/Topology";
import { VisibilityPanel } from "./VisibilityPanel";
import type { CapabilityMatrix, NetworkIdentityReport } from "./types";
import "./styles.css";

// M1: Network Identity only. Device discovery is M3, the topology is M4.
export function App() {
  const [report, setReport] = useState<NetworkIdentityReport | null>(null);
  const [caps, setCaps] = useState<CapabilityMatrix | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<NetworkIdentityReport>("get_network_identity")
      .then(setReport)
      .catch((e: unknown) => setError(String(e)));
    invoke<CapabilityMatrix>("get_capabilities")
      .then(setCaps)
      .catch(() => undefined);
  }, []);

  if (error) {
    return (
      <div className="shell">
        <Brand />
        <p className="err">Could not read this network: {error}</p>
      </div>
    );
  }

  if (!report) {
    return (
      <div className="shell">
        <Brand />
        <p className="state">Observing…</p>
      </div>
    );
  }

  const id = report.identity;
  const net = networkLine(id);
  const observed = caps?.rows.filter((r) => r.state.state === "observed").length ?? 0;
  const available = caps?.rows.filter((r) => r.state.state === "available").length ?? 0;
  const refused = caps?.refused.length ?? 0;
  const blocked =
    (caps?.rows.filter((r) => r.state.state === "not_possible").length ?? 0) +
    (caps?.limitations.length ?? 0);

  return (
    <div className="shell">
      <Brand />

      <h2 className="headline">{connectionLabel(id.connection)}</h2>
      <p className="sub">
        {id.interface_label ? `${id.interface_label} · ` : ""}
        <span className="mono">{id.interface || "no active interface"}</span>
        {" · observed in "}
        {report.observed_in_ms} ms
      </p>

      <dl className="card">
        <div className="row">
          <dt>You are connected via</dt>
          <dd>
            {connectionLabel(id.connection)}
            {id.vpn_active && <> <span className="pill warn">VPN active</span></>}
            {id.connection === "unknown" && (
              <div className="note">
                No default route was found, so JRX will not guess how you are
                connected.
              </div>
            )}
          </dd>
        </div>

        <div className="row">
          <dt>Network</dt>
          <dd>
            <span className={net.tone === "off" ? "state" : undefined}>{net.value}</span>
            {net.tone && net.tone !== "off" && (
              <> <span className={`pill ${net.tone}`}>{net.tone === "ok" ? "live" : "limited"}</span></>
            )}
            {net.note && <div className="note">{net.note}</div>}
          </dd>
        </div>

        <div className="row">
          <dt>Router</dt>
          <dd>
            <span className="mono">{id.gateway ?? "unknown"}</span>
            {id.subnet && (
              <div className="note">
                Your subnet is{" "}
                <span className="mono">
                  {id.subnet.network}/{id.subnet.prefix_len}
                </span>
              </div>
            )}
          </dd>
        </div>

        <div className="row">
          <dt>Internet path</dt>
          <dd>
            <div className="path">
              <span className="hop self mono">{id.local_ip ?? "this device"}</span>
              <span className="arrow">→</span>
              <span className="hop mono">{id.gateway ?? "?"}</span>
              <span className="arrow">→</span>
              <span className="hop">internet</span>
            </div>
            <div className="note">
              {id.dns_servers.length > 0 ? (
                <>Names are resolved by <span className="mono">{id.dns_servers.join(", ")}</span></>
              ) : (
                "No DNS resolvers were reported."
              )}
            </div>
          </dd>
        </div>

        <div className="row">
          <dt>Visibility status</dt>
          <dd>
            <div className="vis">
              <span className="pill ok">{observed} observed</span>
              <span className="pill warn">{available} need permission</span>
              <span className="pill off">{blocked} not possible</span>
              <span className="pill">{refused} refused by design</span>
            </div>
            <div className="note">
              JRX runs without administrator access and collects no packet
              contents.
            </div>
          </dd>
        </div>
      </dl>

      <Topology />

      {caps && <VisibilityPanel matrix={caps} />}
    </div>
  );
}

function Brand() {
  return (
    <div className="brand">
      <h1>JRX</h1>
      <span className="tag">Network Observatory</span>
    </div>
  );
}
