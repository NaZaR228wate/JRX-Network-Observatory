// Ordering and filtering for the program list.
//
// Rows that reorder on every refresh are unreadable, so the default ranking is
// by session total — a number that only grows, meaning a quiet second cannot
// demote a program and swap two rows back and forth.

import type { ProcessActivity } from "../types";

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
