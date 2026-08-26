import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

/** JRX must never imply it can read anyone's traffic.
 *
 *  This scans the user-facing strings in the interface rather than testing one
 *  component, because the risk is a single careless sentence added months from
 *  now — not a bug in code that exists today. */
function userFacingSources(): { file: string; text: string }[] {
  const root = new URL("./", import.meta.url).pathname;
  const out: { file: string; text: string }[] = [];

  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(path);
      } else if (/\.(tsx|ts)$/.test(entry.name) && !entry.name.includes(".test.")) {
        out.push({ file: path, text: readFileSync(path, "utf8") });
      }
    }
  };
  walk(root);
  return out;
}

/** Claims that would be false. Each is paired with the negations that are
 *  legitimate, so the honest sentences JRX *does* make are not flagged. */
const FORBIDDEN: { pattern: RegExp; why: string }[] = [
  { pattern: /\bwe (?:can )?(?:see|read|capture|inspect)\b[^.]*\b(?:packet|payload|password|message|browsing)/i, why: "claims content visibility" },
  { pattern: /\b(?:shows?|reveals?|displays?)\s+(?:the\s+)?(?:contents?|payloads?|passwords?)/i, why: "claims content visibility" },
  { pattern: /\bdecrypt/i, why: "implies decryption" },
  { pattern: /\bintercept/i, why: "implies interception" },
  { pattern: /\bbrowsing history\b(?!.{0,80}\b(?:never|not|no|cannot|refus))/i, why: "mentions browsing history without a negation" },
];

describe("user-facing wording", () => {
  it("never claims JRX can see packet contents, passwords or browsing", () => {
    const offences: string[] = [];

    for (const { file, text } of userFacingSources()) {
      for (const line of text.split("\n")) {
        // Only strings a person could read, not identifiers or imports.
        if (/^\s*(import|export type|\/\/)/.test(line)) continue;
        for (const { pattern, why } of FORBIDDEN) {
          if (pattern.test(line)) {
            offences.push(`${file}: ${why}\n    ${line.trim()}`);
          }
        }
      }
    }

    expect(offences, offences.join("\n")).toHaveLength(0);
  });

  it("the scanner actually catches an offending sentence", () => {
    // A guard that cannot fail is worse than no guard.
    const bad = 'const copy = "JRX shows the contents of every packet";';
    expect(FORBIDDEN.some((f) => f.pattern.test(bad))).toBe(true);
  });
});
