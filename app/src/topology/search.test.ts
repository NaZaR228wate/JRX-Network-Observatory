import { describe, expect, it } from "vitest";
import { matches, searchDevices } from "./search";
import type { Device } from "../types";

function device(over: Partial<Device["facts"]> & { id?: string; category?: string }): Device {
  return {
    id: over.id ?? "192.168.1.10",
    facts: {
      addresses: over.addresses ?? ["192.168.1.10"],
      mac: over.mac ?? null,
      hostname: over.hostname ?? null,
      vendor: over.vendor ?? null,
      services: over.services ?? [],
      upnp_types: [],
      sources: ["arp_cache"],
      mac_randomised: over.mac_randomised ?? false,
    },
    inference: {
      category: (over.category as Device["inference"]["category"]) ?? "unknown",
      confidence: "none",
      family: null,
      rationale: "Not identified.",
      supporting: [],
      history: [],
    },
    evidence: [],
    is_self: false,
    is_gateway: false,
  } as Device;
}

const NETWORK: Device[] = [
  device({ id: "192.168.1.1", addresses: ["192.168.1.1"], hostname: "gateway", vendor: "MikroTik", category: "infrastructure" }),
  device({ id: "192.168.1.10", addresses: ["192.168.1.10"], hostname: "Nazars-MacBook-Pro", vendor: "Apple", category: "computers" }),
  device({ id: "192.168.1.20", addresses: ["192.168.1.20"], mac: "a4:83:e7:11:22:33", hostname: "HP-LaserJet", services: ["_ipp._tcp"], category: "smart_home" }),
  device({ id: "192.168.1.50", addresses: ["192.168.1.50"], mac_randomised: true }),
];

describe("search", () => {
  it("finds a device by its announced name", () => {
    expect(searchDevices(NETWORK, "macbook").map((d) => d.id)).toEqual(["192.168.1.10"]);
  });

  it("finds a device by address", () => {
    expect(searchDevices(NETWORK, "192.168.1.20").map((d) => d.id)).toEqual(["192.168.1.20"]);
  });

  it("finds devices by manufacturer", () => {
    expect(searchDevices(NETWORK, "apple").map((d) => d.id)).toEqual(["192.168.1.10"]);
  });

  it("finds devices by hardware address", () => {
    expect(searchDevices(NETWORK, "a4:83:e7").map((d) => d.id)).toEqual(["192.168.1.20"]);
  });

  it("finds devices by category", () => {
    expect(searchDevices(NETWORK, "infrastructure").map((d) => d.id)).toEqual(["192.168.1.1"]);
  });

  it("finds devices by advertised service", () => {
    expect(searchDevices(NETWORK, "_ipp").map((d) => d.id)).toEqual(["192.168.1.20"]);
  });

  it("ignores case and surrounding space", () => {
    expect(searchDevices(NETWORK, "  MACBOOK  ")).toHaveLength(1);
  });

  // A search box that blanks the map when cleared is a trap.
  it("returns everything for an empty query", () => {
    expect(searchDevices(NETWORK, "")).toHaveLength(NETWORK.length);
    expect(searchDevices(NETWORK, "   ")).toHaveLength(NETWORK.length);
  });

  it("returns nothing for a query that matches nothing", () => {
    expect(searchDevices(NETWORK, "nonexistent")).toHaveLength(0);
  });

  // JRX has no owner field and will not guess at one, so there is nothing to
  // search. This guards against an inferred-person field being added later.
  it("does not match against anything JRX inferred about a person", () => {
    expect(searchDevices(NETWORK, "nazar").map((d) => d.id)).toEqual(["192.168.1.10"]);
    // ...and that match came from the announced hostname, which is a fact:
    expect(NETWORK[1]!.facts.hostname).toContain("Nazars");
  });

  it("does not match against a rationale or other inferred text", () => {
    // Every device's rationale contains "identified"; none should match.
    expect(searchDevices(NETWORK, "identified")).toHaveLength(0);
  });

  it("matches() agrees with the filter", () => {
    for (const d of NETWORK) {
      expect(matches(d, "apple")).toBe(searchDevices([d], "apple").length === 1);
    }
  });
});
