// Category icons.
//
// Shapes, not brands (visual.ts): a laptop, a phone, a house, a router, a
// question — the *kind* of thing, never a product or a logo. Drawn once on a
// 24×24 grid with stroke = currentColor, so the same paths serve the SVG map
// (scaled into a node) and the HTML detail header (inside an <svg>). Colour is
// set by the category tone on an ancestor; stroke width stays crisp at any
// scale via non-scaling-stroke in CSS.

import type { Category } from "../types";

/** The vector body of a category icon, on a 0–24 grid. */
export function iconPaths(category: Category) {
  switch (category) {
    case "computers":
      return (
        <>
          <rect x="4" y="6" width="16" height="10" rx="1.4" />
          <path d="M2.4 19h19.2l-1.7-2.4H4.1z" />
        </>
      );
    case "phones":
      return (
        <>
          <rect x="7.5" y="2.8" width="9" height="18.4" rx="2.3" />
          <path d="M10.8 18.4h2.4" />
        </>
      );
    case "smart_home":
      return (
        <>
          <path d="M4 11.4 12 5l8 6.4" />
          <path d="M6 10.3V19h12v-8.7" />
          <path d="M10.4 19v-4.2h3.2V19" />
        </>
      );
    case "infrastructure":
      return (
        <>
          <rect x="4" y="13" width="16" height="6.6" rx="1.7" />
          <circle cx="7.4" cy="16.3" r="0.95" />
          <path d="M12 13V9.4" />
          <path d="M9.1 8.3a4.1 4.1 0 0 1 5.8 0" />
          <path d="M6.8 6.1a7.3 7.3 0 0 1 10.4 0" />
        </>
      );
    case "unknown":
      return (
        <>
          <circle cx="12" cy="12" r="8.4" />
          <path d="M9.5 9.6a2.6 2.6 0 0 1 5 .7c0 1.7-2.5 2-2.5 3.7" />
          <path d="M12 17.5h.01" />
        </>
      );
  }
}

/** HTML-context icon (detail header, chips). */
export function CategoryIcon({
  category,
  size = 22,
}: {
  category: Category;
  size?: number;
}) {
  return (
    <svg
      className="cat-icon"
      viewBox="0 0 24 24"
      width={size}
      height={size}
      aria-hidden="true"
    >
      {iconPaths(category)}
    </svg>
  );
}

/** SVG-map glyph: the icon centred at the node origin, scaled to `size` px. */
export function NodeGlyph({ category, size }: { category: Category; size: number }) {
  const s = size / 24;
  return (
    <g className="node-glyph" transform={`translate(${-12 * s} ${-12 * s}) scale(${s})`}>
      {iconPaths(category)}
    </g>
  );
}
