// Category visual language.
//
// Colour is never the only signal: each category also carries a distinct
// glyph, so the map stays readable without colour vision and in a screenshot.

import type { Category, Confidence } from "../types";

export function categoryTone(category: Category): string {
  return `cat-${category.replace("_", "-")}`;
}

/** A shape, not a brand. No product imagery. */
export function categoryGlyph(category: Category): string {
  switch (category) {
    case "computers":
      return "▭";
    case "phones":
      return "▯";
    case "smart_home":
      return "◇";
    case "infrastructure":
      return "⬡";
    case "unknown":
      return "○";
  }
}

export function categoryLabel(category: Category): string {
  switch (category) {
    case "computers":
      return "Computers";
    case "phones":
      return "Phones";
    case "smart_home":
      return "Smart home";
    case "infrastructure":
      return "Infrastructure";
    case "unknown":
      return "Unidentified";
  }
}

export function confidenceLabel(confidence: Confidence): string {
  switch (confidence) {
    case "high":
      return "Confident";
    case "medium":
      return "Likely";
    case "none":
      return "Not identified";
  }
}
