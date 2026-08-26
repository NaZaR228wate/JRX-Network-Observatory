import type { DiscoveryQuality, Isolation, SourceQuality } from "../types";

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

/** How much to trust what is on the map.
 *
 *  Four distinct situations, deliberately not collapsed into one "nothing
 *  found" message. They have different causes and different things the user
 *  can do about them, and only the quality model decides which applies. */
export function QualityBanner({
  quality,
  isolation,
}: {
  quality: DiscoveryQuality;
  isolation: Isolation;
}) {
  const state = describeState(quality, isolation);

  return (
    <div className={`banner ${state.tone}`}>
      <strong>{state.headline}</strong>
      <div className="note">{state.detail}</div>
    </div>
  );
}

export interface BannerState {
  tone: "ok" | "warn" | "off";
  headline: string;
  detail: string;
}

export function describeState(quality: DiscoveryQuality, isolation: Isolation): BannerState {
  // Our own failure comes first: an empty map caused by a broken probe must
  // never be blamed on the network.
  if (quality.verdict === "discovery_blocked") {
    return {
      tone: "warn",
      headline: "JRX could not finish looking, so this is not an empty network.",
      detail: quality.explanation,
    };
  }

  // Devices demonstrably exist, but their announcements are not reaching us.
  if (quality.local_network === "likely_blocked") {
    return {
      tone: "warn",
      headline: "Devices are here, but local announcements are not reaching JRX.",
      detail:
        "Your computer knows about other devices, yet none of them announced " +
        "themselves. Either this network suppresses those announcements, or " +
        "macOS has not granted JRX local network access. macOS does not let " +
        "JRX check which, so this is our reading of the evidence rather than " +
        "something the system told us.",
    };
  }

  if (isolation === "likely_isolated") {
    return {
      tone: "off",
      headline: "This network appears to keep its devices apart.",
      detail:
        "Only your router answered, on a network with room for many more. " +
        "Guest and workplace Wi-Fi commonly do this on purpose. JRX can still " +
        "describe this computer and your connection, but it cannot see what " +
        "the network is hiding — and no software can.",
    };
  }

  if (isolation === "no_peers_observed") {
    return {
      tone: "off",
      headline: "No other devices have answered yet.",
      detail:
        "This is a small network, so there may genuinely be nothing else on " +
        "it. Every discovery source ran without error.",
    };
  }

  if (quality.verdict === "degraded") {
    return {
      tone: "warn",
      headline: "Part of the picture is missing.",
      detail: quality.explanation,
    };
  }

  return {
    tone: "ok",
    headline: "JRX is receiving local discovery information.",
    detail: quality.explanation,
  };
}
