import type { NetworkIdentity, NetworkIdentityReport } from "./types";
import { bandLabel, connectionLabel, networkLine } from "./labels";

/** The first thing on screen: what kind of connection, and which network.
 *
 *  Deliberately not a technical readout. A person should be able to read the
 *  first two lines and stop there. */
export function NetworkCard({
  report,
  selfAddress,
}: {
  report: NetworkIdentityReport;
  selfAddress: string | null;
}) {
  const id = report.identity;
  const net = networkLine(id);

  return (
    <section className="netcard">
      <div className="netcard-main">
        <h2 className="connection">{connectionLabel(id.connection)}</h2>
        <p className={`network-name ${net.tone === "off" ? "state" : ""}`}>
          {net.value}
          {signalSummary(id) && <span className="net-extra"> · {signalSummary(id)}</span>}
        </p>
        {net.note && <p className="note netcard-note">{net.note}</p>}
      </div>

      <dl className="netcard-facts">
        <Fact label="Router">
          {id.gateway ? <span className="mono">{id.gateway}</span> : <span className="state">not found</span>}
        </Fact>
        <Fact label="This computer">
          {selfAddress ? <span className="mono">{selfAddress}</span> : <span className="state">unknown</span>}
          <div className="note">highlighted on the map below</div>
        </Fact>
        {id.tunnel && (
          <Fact label="Route">
            <span className="pill warn">Traffic leaves through a tunnel</span>
            <div className="note">
              A VPN is carrying your traffic. The connection and network above
              are still the physical ones you are attached to.
            </div>
          </Fact>
        )}
        {id.other_active.length > 0 && (
          <Fact label="Also connected">
            {id.other_active.map((other) => (
              <div key={other.interface} className="note">
                {connectionLabel(other.connection)}
                {other.local_ip && <span className="mono"> · {other.local_ip}</span>}
              </div>
            ))}
          </Fact>
        )}
      </dl>
    </section>
  );
}

function Fact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="netcard-fact">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

/** Band and signal, in words rather than numbers where possible. */
function signalSummary(id: NetworkIdentity): string | null {
  if (id.wifi.status !== "associated") return null;
  const parts: string[] = [];
  const band = bandLabel(id.wifi.band);
  if (band) parts.push(band);
  if (id.wifi.signal_dbm != null) parts.push(signalWords(id.wifi.signal_dbm));
  return parts.length > 0 ? parts.join(" · ") : null;
}

/** A number in dBm means nothing to most people; the word does. */
function signalWords(dbm: number): string {
  if (dbm >= -55) return "strong signal";
  if (dbm >= -67) return "good signal";
  if (dbm >= -75) return "weak signal";
  return "very weak signal";
}
