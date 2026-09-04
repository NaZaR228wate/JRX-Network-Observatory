import type { Device, DiscoverySource, Evidence } from "../types";
import { categoryLabel, categoryTone, confidenceLabel } from "./visual";
import { CategoryIcon } from "./icons";

const SOURCE_LABEL: Record<DiscoverySource, string> = {
  arp_cache: "Already known to this Mac — nothing was sent",
  mdns: "Announced itself over mDNS",
  ssdp: "Answered a UPnP search",
  self_interface: "This device",
  default_route: "Holds the default route",
};

const EVIDENCE_LABEL: Record<Evidence["kind"], string> = {
  mac_address: "Hardware address",
  hostname: "Announced name",
  service_type: "Advertised service",
  upnp_device_type: "UPnP device type",
  vendor: "Manufacturer",
  gateway_role: "Verified role",
  self_role: "Verified role",
};

/** Level 3. Reads entirely from data already in hand — opening a device
 *  performs no network work. */
export function DeviceDetail({
  device,
  isNew = false,
  onClose,
}: {
  device: Device;
  isNew?: boolean;
  onClose: () => void;
}) {
  const { facts, inference } = device;
  const unidentified = inference.category === "unknown";

  return (
    <aside className="detail" aria-label="Device details">
      <div className="detail-head">
        <div className="detail-head-main">
          <span className={`detail-icon ${categoryTone(inference.category)} conf-${inference.confidence}`}>
            <CategoryIcon category={inference.category} size={24} />
          </span>
          <div>
            <h3>{displayName(device)}</h3>
            {unidentified ? (
              // Not "Unknown / 0%": choosing not to guess is a result, not a
              // failure, and the wording must not read like one.
              <p className="note">Unidentified device</p>
            ) : (
              <p className="note">
                {categoryLabel(inference.category)} · {confidenceLabel(inference.confidence)}
                {inference.family && ` · ${inference.family}`}
              </p>
            )}
          </div>
        </div>
        <button className="back" onClick={onClose} aria-label="Close details">
          ✕
        </button>
      </div>

      {isNew && (
        <p className="detail-new">
          New on this network — JRX has not seen this device here before.
        </p>
      )}

      {unidentified && (
        <p className="unidentified-note">
          JRX has observed this device on the network but does not have enough
          evidence to say what kind of device it is. That is a result, not an
          error — guessing would be worse.
        </p>
      )}

      <Section title="What JRX knows" hint="Observed directly. No interpretation.">
        <Row label="Address">
          <span className="mono">{facts.addresses.join(", ")}</span>
        </Row>
        <Row label="Hardware address">
          {facts.mac ? (
            <>
              <span className="mono">{facts.mac}</span>
              {facts.mac_randomised && (
                <div className="note">
                  This address is randomised. The device is deliberately
                  rotating its identity, so it cannot be traced between
                  networks and its manufacturer is not knowable.
                </div>
              )}
            </>
          ) : (
            <span className="state">Not visible from here</span>
          )}
        </Row>
        <Row label="Announced name">
          {facts.hostname ?? <span className="state">Announced none</span>}
        </Row>
        <Row label="Manufacturer">
          {facts.vendor ?? (
            <span className="state">
              {facts.mac_randomised ? "Not knowable — randomised address" : "Not registered"}
            </span>
          )}
        </Row>
        <Row label="Advertised services">
          {facts.services.length > 0 ? (
            <ul className="chips">
              {facts.services.map((s) => (
                <li key={s} className="mono">
                  {s}
                </li>
              ))}
            </ul>
          ) : (
            <span className="state">Advertised none</span>
          )}
        </Row>
        <Row label="Found by">
          <ul className="plain">
            {facts.sources.map((s) => (
              <li key={s}>{SOURCE_LABEL[s]}</li>
            ))}
          </ul>
        </Row>
      </Section>

      {!unidentified && (
        <Section title="What JRX concludes" hint="Derived from the evidence below.">
          <Row label="Category">{categoryLabel(inference.category)}</Row>
          <Row label="Kind">
            {inference.family ?? (
              <span className="state">Not determined — the evidence does not say</span>
            )}
          </Row>
          <Row label="Confidence">{confidenceLabel(inference.confidence)}</Row>
        </Section>
      )}

      <Section title="Why" hint="The exact observations behind the conclusion.">
        {inference.supporting.length > 0 ? (
          <ul className="evidence">
            {inference.supporting.map((e, i) => (
              <li key={`${e.kind}-${e.value}-${i}`}>
                <span className="ev-kind">{EVIDENCE_LABEL[e.kind]}</span>
                <span className="mono">{e.value}</span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="state">
            Nothing this device revealed says what kind of device it is.
          </p>
        )}
        <p className="note">{inference.rationale}</p>

        {inference.history.length > 0 && (
          <ol className="trail">
            {inference.history.map((change, i) => (
              <li key={i}>
                <span className="mono">{change.triggered_by.value}</span> →{" "}
                {categoryLabel(change.to)} ({confidenceLabel(change.confidence)})
              </li>
            ))}
          </ol>
        )}
      </Section>

      <Section title="What JRX does not know" hint="Stated plainly rather than guessed.">
        <ul className="plain">
          <li>The exact model. Nothing on the network reveals it.</li>
          <li>Who owns or uses it. JRX has no such information and will not infer it.</li>
          {!facts.hostname && <li>What it is called. It announced no name.</li>}
          {facts.mac_randomised && (
            <li>Its manufacturer, because its hardware address is randomised.</li>
          )}
          <li>What it is sending. JRX never reads packet contents.</li>
        </ul>
      </Section>
    </aside>
  );
}

function displayName(device: Device): string {
  if (device.facts.hostname) return device.facts.hostname;
  if (device.facts.vendor) return `${device.facts.vendor} device`;
  return device.facts.addresses[0] ?? "Unidentified device";
}

function Section({
  title,
  hint,
  children,
}: {
  title: string;
  hint: string;
  children: React.ReactNode;
}) {
  return (
    <section className="detail-section">
      <h4>{title}</h4>
      <p className="note section-hint">{hint}</p>
      {children}
    </section>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="detail-row">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}
