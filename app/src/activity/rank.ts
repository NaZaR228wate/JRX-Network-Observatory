// Ordering and filtering for the program list.
//
// Rows that reorder on every refresh are unreadable, so the default ranking is
// by session total — a number that only grows, meaning a quiet second cannot
// demote a program and swap two rows back and forth.

import type { ConnectionActivity, ProcessActivity } from "../types";

export type SortKey = "activity" | "download" | "upload" | "connections";

export const SORT_OPTIONS: { key: SortKey; label: string }[] = [
  { key: "activity", label: "Total activity" },
  { key: "download", label: "Download" },
  { key: "upload", label: "Upload" },
  { key: "connections", label: "Connections" },
];

export function sessionTotal(program: ProcessActivity): number {
  return program.session_bytes_in + program.session_bytes_out;
}

export function displayName(program: ProcessActivity): string {
  return program.application ?? program.process_name;
}

/** Sort programs. Ties break on PID so the order is fully determined and never
 *  depends on the order the host happened to send them. */
export function sortPrograms(
  programs: ProcessActivity[],
  key: SortKey,
): ProcessActivity[] {
  const value = (p: ProcessActivity) => {
    switch (key) {
      case "activity":
        return sessionTotal(p);
      case "download":
        return p.session_bytes_in;
      case "upload":
        return p.session_bytes_out;
      case "connections":
        return p.active_connections;
    }
  };
  return [...programs].sort((a, b) => value(b) - value(a) || a.pid - b.pid);
}

/** Filter on observed facts only: the program's own name, the application the
 *  path proves it belongs to, and network owners it is actually talking to.
 *  There is nothing here about websites, because JRX does not know any. */
export function searchPrograms(
  programs: ProcessActivity[],
  query: string,
): ProcessActivity[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return programs;

  return programs.filter((program) => {
    const haystack = [
      program.process_name,
      program.application ?? "",
      program.executable_path ?? "",
      ...program.connections.map((c) => c.network_owner ?? ""),
      ...program.connections.map((c) => c.remote_address ?? ""),
    ];
    return haystack.some((field) => field.toLowerCase().includes(needle));
  });
}

/** Bytes, in words a person reads rather than a raw count. */
export function bytes(value: number): string {
  if (value >= 1_073_741_824) return `${(value / 1_073_741_824).toFixed(2)} GB`;
  if (value >= 1_048_576) return `${(value / 1_048_576).toFixed(1)} MB`;
  if (value >= 1024) return `${(value / 1024).toFixed(0)} KB`;
  return `${value} B`;
}

export function rate(value: number): string {
  if (value >= 1_048_576) return `${(value / 1_048_576).toFixed(1)} MB/s`;
  if (value >= 1024) return `${(value / 1024).toFixed(0)} KB/s`;
  return `${value} B/s`;
}

// ---- grouping one application's processes into one row ----
//
// A single application often runs as several processes — a browser's helpers,
// a menu-bar app and its agent. The OS attributes traffic per process, so
// without grouping the same app appears as several rows, which reads as odd.
//
// Processes are merged ONLY when they share an `application` the executable's
// path proved (the same rule `displayName` already trusts). Two processes that
// merely share a name, or have no proven application, are never merged —
// guessing that they are "the same app" would be exactly the kind of false
// precision JRX refuses. A one-process app is just a group of one.

export interface ProgramGroup {
  /** Stable id: the application when proven, else the single process's pid. */
  key: string;
  label: string;
  application: string | null;
  /** How many processes were merged (1 when nothing was). */
  process_count: number;
  pids: number[];
  process_names: string[];
  /** Only meaningful for a single un-merged process. */
  executable_path: string | null;
  name_is_truncated: boolean;
  session_bytes_in: number;
  session_bytes_out: number;
  rate_in: number;
  rate_out: number;
  active_connections: number;
  connections: ConnectionActivity[];
}

function toGroup(application: string | null, procs: ProcessActivity[]): ProgramGroup {
  const first = procs[0]!;
  const sum = (f: (p: ProcessActivity) => number) => procs.reduce((s, p) => s + f(p), 0);
  return {
    key: application ? `app:${application}` : `pid:${first.pid}`,
    label: application ?? first.process_name,
    application,
    process_count: procs.length,
    pids: procs.map((p) => p.pid).sort((a, b) => a - b),
    process_names: [...new Set(procs.map((p) => p.process_name))],
    executable_path: application ? null : first.executable_path,
    name_is_truncated: procs.some((p) => p.name_is_truncated),
    session_bytes_in: sum((p) => p.session_bytes_in),
    session_bytes_out: sum((p) => p.session_bytes_out),
    rate_in: sum((p) => p.rate_in),
    rate_out: sum((p) => p.rate_out),
    active_connections: sum((p) => p.active_connections),
    connections: procs.flatMap((p) => p.connections),
  };
}

/** Collapse processes of one proven application into a single group. Input
 *  order of the first appearance of each application is preserved; a later
 *  sort makes the final order deterministic. */
export function groupPrograms(programs: ProcessActivity[]): ProgramGroup[] {
  const byApp = new Map<string, ProcessActivity[]>();
  const order: string[] = [];
  const singles: ProcessActivity[] = [];
  for (const p of programs) {
    if (p.application) {
      if (!byApp.has(p.application)) {
        byApp.set(p.application, []);
        order.push(p.application);
      }
      byApp.get(p.application)!.push(p);
    } else {
      singles.push(p);
    }
  }
  const groups = order.map((app) => toGroup(app, byApp.get(app)!));
  for (const p of singles) groups.push(toGroup(null, [p]));
  return groups;
}

function groupValue(group: ProgramGroup, key: SortKey): number {
  switch (key) {
    case "activity":
      return group.session_bytes_in + group.session_bytes_out;
    case "download":
      return group.session_bytes_in;
    case "upload":
      return group.session_bytes_out;
    case "connections":
      return group.active_connections;
  }
}

/** Same ranking as {@link sortPrograms}, ties broken on the stable key. */
export function sortGroups(groups: ProgramGroup[], key: SortKey): ProgramGroup[] {
  return [...groups].sort(
    (a, b) => groupValue(b, key) - groupValue(a, key) || a.key.localeCompare(b.key),
  );
}

/** Filter groups on observed facts only — the same fields as
 *  {@link searchPrograms}, across every process merged into the group. */
export function searchGroups(groups: ProgramGroup[], query: string): ProgramGroup[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return groups;
  return groups.filter((group) => {
    const haystack = [
      group.label,
      group.application ?? "",
      ...group.process_names,
      group.executable_path ?? "",
      ...group.connections.map((c) => c.network_owner ?? ""),
      ...group.connections.map((c) => c.remote_address ?? ""),
    ];
    return haystack.some((field) => field.toLowerCase().includes(needle));
  });
}
