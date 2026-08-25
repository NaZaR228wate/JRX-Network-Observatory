import { describe, expect, it } from "vitest";
import {
  CATEGORY_ORDER,
  angleFor,
  nodeRadius,
  placeGroups,
  placeMembers,
  placeSelf,
} from "./layout";

const CENTER = { x: 500, y: 400 };

describe("level 1 placement", () => {
  it("gives every category a fixed, distinct sector", () => {
    const angles = CATEGORY_ORDER.map(angleFor);
    expect(new Set(angles).size).toBe(CATEGORY_ORDER.length);
  });

  it("is deterministic: the same input places nodes identically", () => {
    expect(placeGroups(CENTER, 240)).toEqual(placeGroups(CENTER, 240));
  });

  // A category emptying must not move the others around the ring.
  it("keeps a category's angle regardless of what else exists", () => {
    const before = angleFor("unknown");
    const after = angleFor("unknown");
    expect(after).toBe(before);
    expect(angleFor("computers")).not.toBe(angleFor("unknown"));
  });

  it("places every group on the ring radius", () => {
    for (const group of placeGroups(CENTER, 240)) {
      const distance = Math.hypot(group.x - CENTER.x, group.y - CENTER.y);
      expect(distance).toBeCloseTo(240, 6);
    }
  });

  // This machine must always be findable in the same place.
  it("puts this machine inside the ring, on its own spoke", () => {
    const self = placeSelf(CENTER, 240);
    const distance = Math.hypot(self.x - CENTER.x, self.y - CENTER.y);
    expect(distance).toBeLessThan(240);
    expect(self.x).toBeCloseTo(CENTER.x, 6);
    expect(self.y).toBeGreaterThan(CENTER.y);
  });

  it("makes the router the largest node so it anchors the picture", () => {
    expect(nodeRadius("router")).toBeGreaterThan(nodeRadius("group"));
    expect(nodeRadius("router")).toBeGreaterThan(nodeRadius("self"));
    expect(nodeRadius("device")).toBeLessThan(nodeRadius("self"));
  });
});

describe("level 2 placement", () => {
  it("places exactly as many points as members", () => {
    for (const count of [1, 7, 40]) {
      expect(placeMembers(CENTER, count, 120, 80)).toHaveLength(count);
    }
  });

  it("is deterministic", () => {
    expect(placeMembers(CENTER, 40, 120, 80)).toEqual(
      placeMembers(CENTER, 40, 120, 80),
    );
  });

  // Adding a member must not relocate the ones already placed, or opening a
  // group during live discovery would make everything jump.
  it("keeps earlier members in place when the group grows", () => {
    const before = placeMembers(CENTER, 6, 120, 80);
    const after = placeMembers(CENTER, 7, 120, 80);
    // The first ring holds at least six, so those six keep their ring.
    for (let i = 0; i < 6; i += 1) {
      const b = before[i]!;
      const a = after[i]!;
      const rBefore = Math.hypot(b.x - CENTER.x, b.y - CENTER.y);
      const rAfter = Math.hypot(a.x - CENTER.x, a.y - CENTER.y);
      expect(rAfter).toBeCloseTo(rBefore, 6);
    }
  });

  it("spreads a large group over several rings instead of one crowded circle", () => {
    const radii = placeMembers(CENTER, 40, 120, 80).map((p) =>
      Math.round(Math.hypot(p.x - CENTER.x, p.y - CENTER.y)),
    );
    expect(new Set(radii).size).toBeGreaterThan(1);
  });

  it("never places two members on the same point", () => {
    const points = placeMembers(CENTER, 40, 120, 80);
    const keys = points.map((p) => `${p.x.toFixed(3)},${p.y.toFixed(3)}`);
    expect(new Set(keys).size).toBe(points.length);
  });

  it("handles an empty group without producing a point", () => {
    expect(placeMembers(CENTER, 0, 120, 80)).toEqual([]);
  });
});
