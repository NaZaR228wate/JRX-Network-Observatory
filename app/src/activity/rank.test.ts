import { describe, expect, it } from "vitest";
import {
  bytes,
  displayName,
  groupPrograms,
  rate,
  searchGroups,
  searchPrograms,
  sortGroups,
  sortPrograms,
} from "./rank";
import type { ProcessActivity } from "../types";

function program(over: Partial<ProcessActivity>): ProcessActivity {
  return {
    pid: 1,
    process_name: "proc",
    executable_path: null,
    application: null,
    name_is_truncated: false,
    session_bytes_in: 0,
    session_bytes_out: 0,
    rate_in: 0,
    rate_out: 0,
    active_connections: 0,
    idle_samples: 0,
    connections: [],
    ...over,
  };
}

const PROGRAMS: ProcessActivity[] = [
  program({ pid: 10, process_name: "Telegram", session_bytes_in: 1_000, session_bytes_out: 500, active_connections: 2 }),
  program({ pid: 20, process_name: "claude", application: "Claude", session_bytes_in: 100, session_bytes_out: 9_000, active_connections: 8 }),
  program({ pid: 30, process_name: "Spotify", session_bytes_in: 50_000, session_bytes_out: 100, active_connections: 1 }),
];

describe("ordering", () => {
  it("ranks by total activity by default", () => {
    expect(sortPrograms(PROGRAMS, "activity").map((p) => p.pid)).toEqual([30, 20, 10]);
  });

  it("sorts by each requested measure", () => {
    expect(sortPrograms(PROGRAMS, "download")[0]!.pid).toBe(30);
    expect(sortPrograms(PROGRAMS, "upload")[0]!.pid).toBe(20);
    expect(sortPrograms(PROGRAMS, "connections")[0]!.pid).toBe(20);
  });

  // Rows that swap places every second are unreadable.
  it("is fully determined, so equal programs never swap places", () => {
    const tied = [
      program({ pid: 2, session_bytes_in: 100 }),
      program({ pid: 1, session_bytes_in: 100 }),
    ];
    expect(sortPrograms(tied, "activity").map((p) => p.pid)).toEqual([1, 2]);
    expect(sortPrograms([...tied].reverse(), "activity").map((p) => p.pid)).toEqual([1, 2]);
  });

  // Session totals only grow, so a quiet second cannot demote a program.
  it("a quiet interval does not reorder the list", () => {
    const before = sortPrograms(PROGRAMS, "activity").map((p) => p.pid);
    const quiet = PROGRAMS.map((p) => ({ ...p, rate_in: 0, rate_out: 0 }));
    expect(sortPrograms(quiet, "activity").map((p) => p.pid)).toEqual(before);
  });

  it("does not mutate the input", () => {
    const original = PROGRAMS.map((p) => p.pid);
    sortPrograms(PROGRAMS, "upload");
    expect(PROGRAMS.map((p) => p.pid)).toEqual(original);
  });
});

describe("naming", () => {
  // Only the executable's path proves which application it belongs to.
  it("prefers a proven application name over the executable name", () => {
    expect(displayName(PROGRAMS[1]!)).toBe("Claude");
    expect(displayName(PROGRAMS[0]!)).toBe("Telegram");
  });

  it("falls back to the process name when no application was proven", () => {
    expect(displayName(program({ process_name: "rapportd" }))).toBe("rapportd");
  });
});

describe("search", () => {
  it("finds a program by its own name", () => {
    expect(searchPrograms(PROGRAMS, "spotify").map((p) => p.pid)).toEqual([30]);
  });

  it("finds a program by its application name", () => {
    expect(searchPrograms(PROGRAMS, "claude").map((p) => p.pid)).toEqual([20]);
  });

  it("finds programs by the network owner they are talking to", () => {
    const withOwner = [
      program({
        pid: 40,
        process_name: "curl",
        connections: [
          {
            protocol: "tcp", remote_address: "104.18.32.1", remote_port: 443,
            state: "Established", network_owner: "Cloudflare",
            session_bytes_in: 1, session_bytes_out: 1, is_open: true,
          },
        ],
      }),
    ];
    expect(searchPrograms(withOwner, "cloudflare")).toHaveLength(1);
  });

  it("returns everything for an empty query", () => {
    expect(searchPrograms(PROGRAMS, "")).toHaveLength(3);
    expect(searchPrograms(PROGRAMS, "   ")).toHaveLength(3);
  });

  // JRX has no website data, so there is nothing of the sort to search.
  it("matches nothing that looks like a website", () => {
    expect(searchPrograms(PROGRAMS, "cloudflare.com")).toHaveLength(0);
    expect(searchPrograms(PROGRAMS, ".com")).toHaveLength(0);
  });
});

describe("grouping one application's processes", () => {
  // A browser and its helpers, all proven to belong to the same bundle.
  const chromeMain = program({
    pid: 100, process_name: "Google Chrome", application: "Google Chrome",
    session_bytes_in: 1_000, session_bytes_out: 200, rate_in: 10, rate_out: 2, active_connections: 3,
  });
  const chromeHelper = program({
    pid: 101, process_name: "Google Chrome Helper", application: "Google Chrome",
    session_bytes_in: 4_000, session_bytes_out: 800, rate_in: 40, rate_out: 8, active_connections: 5,
  });
  const rapportd = program({ pid: 200, process_name: "rapportd", session_bytes_in: 10 });
  const otherd = program({ pid: 201, process_name: "rapportd", session_bytes_in: 20 });

  it("merges processes that share a proven application into one row", () => {
    const groups = groupPrograms([chromeMain, chromeHelper]);
    expect(groups).toHaveLength(1);
    const g = groups[0]!;
    expect(g.label).toBe("Google Chrome");
    expect(g.process_count).toBe(2);
    expect(g.pids).toEqual([100, 101]);
    // metrics are summed
    expect(g.session_bytes_in).toBe(5_000);
    expect(g.session_bytes_out).toBe(1_000);
    expect(g.rate_in).toBe(50);
    expect(g.active_connections).toBe(8);
  });

  it("never merges processes without a proven application, even with the same name", () => {
    const groups = groupPrograms([rapportd, otherd]);
    expect(groups).toHaveLength(2);
    expect(groups.every((g) => g.process_count === 1)).toBe(true);
  });

  it("leaves a single-process application as a group of one", () => {
    const groups = groupPrograms([program({ pid: 5, process_name: "claude", application: "Claude" })]);
    expect(groups).toHaveLength(1);
    expect(groups[0]!.process_count).toBe(1);
    expect(groups[0]!.label).toBe("Claude");
  });

  it("ranks groups by total activity, deterministically", () => {
    const groups = groupPrograms([rapportd, chromeMain, chromeHelper]);
    const ranked = sortGroups(groups, "activity").map((g) => g.label);
    expect(ranked[0]).toBe("Google Chrome"); // 6000 B total beats rapportd's 10 B
    // stable: same input in any order gives the same result
    const reranked = sortGroups(groupPrograms([chromeHelper, chromeMain, rapportd]), "activity").map((g) => g.label);
    expect(reranked).toEqual(ranked);
  });

  it("searches every process merged into a group, and the application name", () => {
    const groups = groupPrograms([chromeMain, chromeHelper]);
    expect(searchGroups(groups, "helper")).toHaveLength(1);   // a member's process name
    expect(searchGroups(groups, "chrome")).toHaveLength(1);   // the application
    expect(searchGroups(groups, "firefox")).toHaveLength(0);
  });
});

describe("formatting", () => {
  it("scales byte counts to something readable", () => {
    expect(bytes(512)).toBe("512 B");
    expect(bytes(2048)).toBe("2 KB");
    expect(bytes(5 * 1_048_576)).toBe("5.0 MB");
    expect(bytes(3 * 1_073_741_824)).toBe("3.00 GB");
  });

  it("scales rates the same way", () => {
    expect(rate(0)).toBe("0 B/s");
    expect(rate(2048)).toBe("2 KB/s");
    expect(rate(2 * 1_048_576)).toBe("2.0 MB/s");
  });

  // A zero rate is a real measurement and must be shown as one.
  it("shows a zero rate rather than hiding it", () => {
    expect(rate(0)).toContain("0");
  });
});
