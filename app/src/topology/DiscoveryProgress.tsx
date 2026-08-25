import type { DiscoveryQuality, SourceQuality } from "../types";

/** Honest progress.
 *
 *  The total amount of work is unknown — we cannot know how many devices will
 *  answer — so there is no percentage to show. Staged states instead: each
 *  source reports what it found the moment it finishes. */
export function DiscoveryProgress({
  sources,
  done,
}: {
  sources: SourceQuality[];
  done: boolean;
}) {
  const pending = ["arp_cache", "mdns", "ssdp"].filter(
    (m) => !sources.some((s) => s.method === m),
  );

  return (
    <div className="progress">
      <div className="progress-head">
        <span className="pulse" aria-hidden={done} />
        {done ? "Discovery complete" : "Discovering your network…"}
      </div>
      <ul className="progress-list">
        {sources.map((source) => (
          <li key={source.method}>
            <span className="tick">{source.status.status === "ok" ? "✓" : "!"}</span>
            <span className="src">{sourceName(source.method)}</span>
            <span className="mono result">{describe(source)}</span>
          </li>
        ))}
        {pending.map((method) => (
          <li key={method} className="waiting">
            <span className="tick">·</span>
            <span className="src">{sourceName(method)}</span>
            <span className="result state">listening…</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function sourceName(method: string): string {
  switch (method) {
    case "arp_cache":
      return "Neighbour cache";
    case "mdns":
      return "mDNS";
    case "ssdp":
      return "SSDP";
    default:
      return method;
  }
}

function describe(source: SourceQuality): string {
  if (source.status.status === "failed") return source.status.reason;
  if (source.method === "mdns") {
    return `${source.names_resolved} names · ${source.services_seen} service types`;
  }
  return `${source.observations} ${source.observations === 1 ? "entry" : "entries"}`;
}

/** How much to trust what is on the map. Never lets the map simply look
 *  broken. */
export function QualityBanner({ quality }: { quality: DiscoveryQuality }) {
  const tone =
    quality.verdict === "healthy"
      ? "ok"
      : quality.verdict === "network_appears_empty"
        ? "off"
        : "warn";

  const headline = () => {
    if (quality.local_network === "likely_blocked") {
      return "Devices are visible, but this network appears to block local discovery.";
    }
    switch (quality.verdict) {
      case "healthy":
        return "JRX is receiving local discovery information.";
      case "degraded":
        return "Some of the picture is missing.";
      case "discovery_blocked":
        return "Discovery could not run properly, so this is not an empty network.";
      case "network_appears_empty":
        return "Other devices cannot be observed from this connection.";
    }
  };

  return (
    <div className={`banner ${tone}`}>
      <strong>{headline()}</strong>
      <div className="note">{quality.explanation}</div>
    </div>
  );
}
