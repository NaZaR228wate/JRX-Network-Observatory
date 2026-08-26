# M5 phase 0 — what JRX can truthfully show about this Mac's activity

**Date:** 2026-08-26 · **Branch:** `claude/m5-phase0-activity-feasibility`
**Constraints:** unprivileged, no packet capture, no payload inspection, no TLS
interception, local-only.

Everything below was measured on this Mac. Nothing was designed around an
assumption.

---

## The headline finding

**ARCHITECTURE.md §8.4 was wrong.** It stated that per-application bandwidth
requires administrator access. On macOS it does not: `/usr/bin/nettop` reports
bytes **per socket**, grouped by owning process, for the whole system including
root-owned processes, with no elevation.

That single fact changes what M5 can honestly be.

---

## Sources tested

| Source | Result |
|---|---|
| `netstat -ib` | ✅ Interface byte counters, cumulative, ~4 ms |
| `nettop -x -L 1` | ✅ Per-socket bytes + endpoints + state + RTT + owning process, ~9 ms |
| `proc_pidpath` (libproc) | ✅ Native, no spawn, full executable name from a PID |
| Reverse DNS (`dig -x`) | ❌ **0 of 12** live endpoints had a PTR record |
| WHOIS / online ASN | ❌ Rejected: would disclose every endpoint to a third party |
| Published provider ranges | ✅ Offline network owner for 12 of 28 live connections |
| `lsof -i` | Not adopted — `nettop` supersedes it and also gives bytes |
| Packet capture | Not tested. Out of scope permanently. |

### One important caveat about `nettop`

It links `/System/Library/PrivateFrameworks/NetworkStatistics.framework`.
Running Apple's own shipped binary is fine; **linking that framework ourselves
would be a private API** and JRX must not. This is a decision, not an oversight.

---

## Capability matrix

| Capability | Status | Source | Refresh cost | Accuracy limits |
|---|---|---|---|---|
| Interface throughput | **VERIFIED** | `netstat -ib` deltas | ~4 ms | Includes traffic never attributed to a process, so it exceeds the sum of per-process counts |
| Bytes since observation began | **VERIFIED** | same counters | free | Counters reset when an interface reinitialises |
| Active TCP connections | **VERIFIED** | `nettop` | ~9 ms | Snapshot only; connections shorter than the interval are missed |
| Active UDP connections | **VERIFIED** | `nettop` | same call | No state for UDP; there is none to report |
| Connection state | **VERIFIED** | `nettop` | same call | TCP only |
| Round-trip time | **VERIFIED** | `nettop` | same call | Not yet used; available |
| Owning PID | **VERIFIED** | `nettop` | same call | PIDs are reused; identity across samples is not guaranteed |
| Process name | **VERIFIED** | `nettop` + `proc_pidpath` | ~78 µs | `nettop` truncates at **15 characters**; the PID lookup recovers the rest, and fails for exited processes |
| Application (bundle) name | **NOT ATTEMPTED** | — | — | Executable name is available; mapping helpers to their parent app was not proven |
| **Per-process byte counts** | **VERIFIED** | `nettop` | same call | Cumulative per socket; a closed socket disappears, so totals must be accumulated by us |
| Remote IP | **VERIFIED** | `nettop` | same call | — |
| Remote port | **VERIFIED** | `nettop` | same call | A port is not a protocol: 443 is not proof of HTTPS |
| Hostname / domain | **NOT AVAILABLE** | — | — | Reverse DNS resolved **nothing** for 12 of 12 live endpoints, and would not be proof of a visited site even when it does. The only reliable source is DNS query observation, which JRX refuses by design |
| ASN | **NOT AVAILABLE** | — | — | Needs either an online lookup (leaks the user's endpoints) or a licensed database. Neither is acceptable as-is |
| Organisation / network owner | **VERIFIED, PARTIAL** | Bundled published ranges | free | **12 of 28** live connections. Owner ≠ site: one Cloudflare address fronts millions |
| Service classification | **NOT AVAILABLE** | — | — | Would require inferring a service from an owner or a port. Both are guesses |

---

## Performance

Measured over 20 refreshes, release build, warm:

```
2 process spawns per refresh
nettop   min 8.9   med 9.2   max 23.0  ms
netstat  min 3.4   med 3.7   max  8.3  ms
parse + PID resolution        med 78 µs
                       total  12.9 ms per refresh

  1 Hz -> 1.3% of one core
  2 Hz -> 2.6% of one core
  4 Hz -> 5.2% of one core
```

Peak RSS of the `nettop` child: ~5 MB. Parsing is negligible.

**1 Hz is comfortable; 2 Hz is defensible. Anything faster buys nothing** —
these are counters, not events.

The first `nettop` call after boot cost **6.3 seconds** while it initialised.
A real implementation must warm it at startup, or the first frame stalls.

---

## What the screen can truthfully contain

Every field below was proven on real data:

```
THIS MAC                    en7

  ↓ 2 KB/s      ↑ 209 B/s
  6.00 GB in / 1.03 GB out since the OS started counting

  29 connections to the internet, 3 local

  claude                    10.5 MB out · 17 KB in
    160.x.x.x  TCP/443      network owner unavailable

  Codex (Service)            4.2 MB in · 1.2 MB out
    104.18.x.x TCP/443      Cloudflare — network owner only

  Claude Helper              2.8 MB out · 646 KB in
    160.x.x.x  UDP/443      network owner unavailable
```

**What it must never contain:** a domain, a website name, a service name, or
any suggestion that JRX knows where the user has been.

---

## Honesty rules, enforced by tests

All four mutation-verified:

| Rule | Mutation tried | Caught |
|---|---|---|
| An address never becomes a hostname | Populate `hostname` from the owner lookup | ✅ |
| Ownership is observed, never guessed | Attribute an orphan socket to a placeholder | ✅ |
| Byte counts are measured, never modelled | Default an unreadable count to 1500 | ✅ |
| An unmatched address stays unidentified | Fall back to "Unknown provider" | ✅ |

Plus: the model is scanned for any field that could hold content
(`payload`, `cookie`, `url`, `body`…), and the exec layer is scanned for any
reach toward `tcpdump`, `pcap`, `bpf` or `sudo`.

---

## Recommendation for M5

**Build it, scoped to this Mac, and be specific about the four fields that are
not available.**

The compelling part is not the domain that JRX cannot know — it is the part
nobody else shows honestly: *which of your own programs are talking, how much,
and to whose network*. That is fully available, cheap, and true.

Concretely:

1. **Live throughput** for the active interface.
2. **Traffic by program**, accumulated across samples so a closed socket does
   not erase its history within the session.
3. **Endpoints per program**, with the network owner where a published range
   says so and an honest blank where it does not.
4. **A boundary statement on the same screen**: JRX shows which programs are
   talking and to whose network. It does not know which websites, and it will
   not guess.

Deliberately out of M5: ASN (no acceptable source), service classification
(inference), domains (refused), other devices' traffic (impossible and already
a documented product boundary).
