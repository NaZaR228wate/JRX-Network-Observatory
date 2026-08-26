import { describe, expect, it } from "vitest";
import { describeState } from "./DiscoveryProgress";
import type { DiscoveryQuality, Isolation } from "../types";

function quality(over: Partial<DiscoveryQuality>): DiscoveryQuality {
  return {
    verdict: "healthy",
    explanation: "explanation text",
    sources: [],
    local_network: "working",
    ...over,
  };
}

const say = (q: Partial<DiscoveryQuality>, isolation: Isolation = "normal") =>
  describeState(quality(q), isolation);

describe("degraded network states", () => {
  it("reports a healthy network plainly", () => {
    const state = say({});
    expect(state.tone).toBe("ok");
    expect(state.headline).toMatch(/receiving local discovery/i);
  });

  // The three situations the product must never conflate.
  it("distinguishes isolation, a small network, and blocked announcements", () => {
    const isolated = say({ verdict: "network_appears_empty" }, "likely_isolated");
    const small = say({ verdict: "network_appears_empty" }, "no_peers_observed");
    const blocked = say({ local_network: "likely_blocked" });

    const headlines = [isolated.headline, small.headline, blocked.headline];
    expect(new Set(headlines).size).toBe(3);

    expect(isolated.headline).toMatch(/keep its devices apart/i);
    expect(small.headline).toMatch(/no other devices/i);
    expect(blocked.headline).toMatch(/not reaching JRX/i);
  });

  // An empty map caused by our own failure must never be blamed on the network.
  it("blames a failed probe on JRX, not on the network", () => {
    const state = say({ verdict: "discovery_blocked", explanation: "arp failed" });
    expect(state.headline).toMatch(/not an empty network/i);
    expect(state.detail).toContain("arp failed");
  });

  // A probe failure outranks an isolation reading: we cannot conclude anything
  // about the network from a run that did not complete.
  it("prefers our own failure over an isolation claim", () => {
    const state = say({ verdict: "discovery_blocked" }, "likely_isolated");
    expect(state.headline).toMatch(/not an empty network/i);
  });

  it("never claims to know local network permission", () => {
    const state = say({ local_network: "likely_blocked" });
    expect(state.detail).toMatch(/does not let JRX check|rather than something the system told us/i);
  });

  it("always gives both a headline and a detail", () => {
    const combinations: [Partial<DiscoveryQuality>, Isolation][] = [
      [{}, "normal"],
      [{ verdict: "degraded" }, "normal"],
      [{ verdict: "discovery_blocked" }, "normal"],
      [{ verdict: "network_appears_empty" }, "likely_isolated"],
      [{ verdict: "network_appears_empty" }, "no_peers_observed"],
      [{ local_network: "likely_blocked" }, "normal"],
    ];
    for (const [q, isolation] of combinations) {
      const state = say(q, isolation);
      expect(state.headline.length).toBeGreaterThan(0);
      expect(state.detail.length).toBeGreaterThan(0);
    }
  });

  // JRX must never imply it can read anyone's traffic.
  it("no state wording implies packet or content visibility", () => {
    const forbidden = /packet|payload|browsing|password|decrypt|intercept/i;
    const combinations: [Partial<DiscoveryQuality>, Isolation][] = [
      [{}, "normal"],
      [{ verdict: "discovery_blocked" }, "normal"],
      [{ verdict: "network_appears_empty" }, "likely_isolated"],
      [{ verdict: "network_appears_empty" }, "no_peers_observed"],
      [{ local_network: "likely_blocked" }, "normal"],
    ];
    for (const [q, isolation] of combinations) {
      const state = say(q, isolation);
      expect(state.headline).not.toMatch(forbidden);
      expect(state.detail).not.toMatch(forbidden);
    }
  });
});
