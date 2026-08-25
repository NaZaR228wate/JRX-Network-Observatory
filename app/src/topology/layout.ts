// Deterministic placement for the topology.
//
// Pure geometry: the same input always produces the same coordinates, so a
// node never moves between renders and never jitters. This is the whole reason
// a radial layout was chosen over a force simulation (TECH_DECISIONS.md
// ADR-007) — a home network is a star, and a simulation would only add motion
// that means nothing.

import type { Category } from "../types";

export interface Point {
  x: number;
  y: number;
}

export interface PlacedGroup extends Point {
  category: Category;
  /** Angle in degrees, for drawing the connector to the centre. */
  angle: number;
}

/** Fixed sector per category, in the order they are always listed.
 *  Owning a permanent angle means an emptying category does not shuffle the
 *  others around the ring. */
export const CATEGORY_ORDER: Category[] = [
  "computers",
  "phones",
  "smart_home",
  "infrastructure",
  "unknown",
];

/** Straight up, so the first category reads as the top of the dial. */
const FIRST_ANGLE = -90;

export function angleFor(category: Category): number {
  const index = CATEGORY_ORDER.indexOf(category);
  if (index < 0) return FIRST_ANGLE;
  return FIRST_ANGLE + (index * 360) / CATEGORY_ORDER.length;
}

function polar(center: Point, radius: number, degrees: number): Point {
  const radians = (degrees * Math.PI) / 180;
  return {
    x: center.x + radius * Math.cos(radians),
    y: center.y + radius * Math.sin(radians),
  };
}

/** Level 1: the five category sectors around the router. */
export function placeGroups(center: Point, radius: number): PlacedGroup[] {
  return CATEGORY_ORDER.map((category) => {
    const angle = angleFor(category);
    return { category, angle, ...polar(center, radius, angle) };
  });
}

/** This machine sits between the router and the ring, on its own fixed spoke,
 *  so it is always in the same place and always easy to find. */
export function placeSelf(center: Point, radius: number): Point {
  return polar(center, radius * 0.46, 90);
}

/** Level 2: members of one group, on concentric rings around the centre.
 *
 *  Rings rather than one big circle so a group of forty stays legible without
 *  the nodes touching. Index-based, so ordering the members deterministically
 *  is enough to make the whole layout deterministic. */
export function placeMembers(
  center: Point,
  count: number,
  innerRadius: number,
  ringGap: number,
): Point[] {
  const points: Point[] = [];
  let placed = 0;
  let ring = 0;

  while (placed < count) {
    const radius = innerRadius + ring * ringGap;
    // Roughly even spacing: circumference grows with the ring, so outer rings
    // hold more without crowding.
    const capacity = Math.max(6, Math.floor((2 * Math.PI * radius) / 78));
    const inThisRing = Math.min(capacity, count - placed);
    // Alternate the starting angle so rings do not line up into spokes.
    const offset = ring % 2 === 0 ? 0 : 180 / inThisRing;

    for (let i = 0; i < inThisRing; i += 1) {
      points.push(polar(center, radius, FIRST_ANGLE + offset + (i * 360) / inThisRing));
    }

    placed += inThisRing;
    ring += 1;
  }

  return points;
}

/** Node radius by role. The router anchors the picture, so it is the largest;
 *  unknown devices are deliberately quiet. */
export function nodeRadius(role: "router" | "self" | "group" | "device"): number {
  switch (role) {
    case "router":
      return 27;
    case "self":
      return 19;
    case "group":
      return 23;
    case "device":
      return 11;
  }
}
