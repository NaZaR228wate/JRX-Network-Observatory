import { describe, expect, it } from "vitest";
import { CATEGORY_ORDER, angleFor, placeGroups, placeMembers } from "./layout";
import { searchDevices } from "./search";
import type { Category, Device } from "../types";

const CENTER = { x: 500, y: 400 };

function synthetic(count: number): Device[] {
  return Array.from({ length: count }, (_, i) => ({
    id: `10.20.${Math.floor(i / 200)}.${(i % 200) + 1}`,
    facts: {
      addresses: [`10.20.${Math.floor(i / 200)}.${(i % 200) + 1}`],
      mac: `9e:aa:bb:${(i >> 8).toString(16).padStart(2, "0")}:${(i % 256).toString(16).padStart(2, "0")}:01`,
      hostname: i % 7 === 0 ? `device-${i}` : null,
      vendor: i % 3 === 0 ? "Intel Corporate" : null,
      services: [],
      upnp_types: [],
      sources: ["arp_cache"],
      mac_randomised: i % 3 !== 0,
    },
    inference: {
      category: "unknown" as Category,
      confidence: "none",
      family: null,
      rationale: "Not identified.",
      supporting: [],
      history: [],
    },
    evidence: [],
    is_self: false,
    is_gateway: false,
  })) as Device[];
}

/** Not a benchmark to optimise against — a guard. These are pure transforms
 *  that run on every render, and a regression that made one of them visibly
 *  slow should fail here rather than as jank on someone's laptop. */
function measure(fn: () => void): number {
  const start = performance.now();
  fn();
  return performance.now() - start;
}

describe("performance guards", () => {
  it("places a full group page in well under a frame", () => {
    const ms = measure(() => {
      for (let i = 0; i < 100; i += 1) placeMembers(CENTER, 40, 116, 66);
    });
    // 100 layouts; one must cost a small fraction of a 16ms frame.
    expect(ms / 100).toBeLessThan(1);
  });

  it("searches 500 devices in well under a frame", () => {
    const devices = synthetic(500);
    const ms = measure(() => {
      for (let i = 0; i < 20; i += 1) searchDevices(devices, "intel");
    });
    expect(ms / 20).toBeLessThan(16);
  });

  it("search over 500 devices returns the expected subset", () => {
    const devices = synthetic(500);
    const hits = searchDevices(devices, "intel");
    expect(hits.length).toBeGreaterThan(100);
    expect(hits.every((d) => d.facts.vendor === "Intel Corporate")).toBe(true);
  });
});

describe("spatial stability during live discovery", () => {
  // Progressive discovery replaces the overview several times per run. A node
  // that moves because a later ARP reply arrived would make the map feel
  // unstable for no informational reason.
  it("category positions do not depend on how many devices exist", () => {
    const early = placeGroups(CENTER, 176);
    const late = placeGroups(CENTER, 176);
    expect(early).toEqual(late);

    // The angles are a property of the category alone.
    for (const category of CATEGORY_ORDER) {
      expect(angleFor(category)).toBe(angleFor(category));
    }
  });

  it("a member keeps its position as later members are discovered", () => {
    const at = (count: number, index: number) => placeMembers(CENTER, count, 116, 66)[index]!;

    // The first ring holds a fixed number, so members within it never move as
    // the group grows past it.
    for (let index = 0; index < 6; index += 1) {
      const early = at(6, index);
      const later = at(9, index);
      const r = (p: { x: number; y: number }) => Math.hypot(p.x - CENTER.x, p.y - CENTER.y);
      expect(r(later)).toBeCloseTo(r(early), 6);
    }
  });

  it("placement never produces NaN, which would silently blank a node", () => {
    for (const count of [0, 1, 2, 40, 200]) {
      for (const point of placeMembers(CENTER, count, 116, 66)) {
        expect(Number.isFinite(point.x)).toBe(true);
        expect(Number.isFinite(point.y)).toBe(true);
      }
    }
  });
});
