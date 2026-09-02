# JRX Network Observatory — Technology Decisions

**Status:** Approved · **Date:** 2026-08-24

Architecture decision records. Each states the context, the decision, what was
rejected and why, and what the decision costs us. Rejected options are recorded
deliberately — a decision without its alternatives is not reviewable.

**Standing constraints for every record below:** privacy-first; no root/admin in
v1; no packet payload collection; no cloud backend in v1; no accounts; local
processing first.

---

## ADR-001 — Tauri v2 with a Rust core, over Electron

**Context.** JRX needs a cross-platform desktop shell for macOS and Windows,
capable of a polished animated UI, that credibly presents as a security product.

**Decision.** Tauri v2. Rust for the host, collector, and domain core; React +
TypeScript in the WebView.

**Rejected — Electron.** Faster to build and the largest ecosystem, but it ships
~150 MB installers with a bundled Chromium and Node runtime. For a network-facing
security tool, a bundled Node runtime with filesystem and child-process access is a
materially worse attack surface, and a 150 MB privacy tool undermines its own
message. Uniform Chromium rendering was the real loss.

**Rejected — Go daemon plus web UI.** Excellent concurrency and cross-compilation
for scanning work, but GUI packaging is the least mature path of the three and it
still requires a second language for the UI.

**Rejected — fully native (SwiftUI + WinUI).** Best platform integration and the
best permission handling, but two complete UI implementations is not compatible
with a 3–6 week first demo.

**Consequences.**
- ~10 MB installers, low idle memory, minimal attack surface.
- Rust's type system is a genuine asset for parsing OS structures correctly.
- Tauri v2 compiles to iOS and Android, so `core` is reusable when mobile arrives
  (ADR-015).
- **Cost:** Rust learning curve against a 3–6 week target. Mitigated by keeping
  `core` pure and platform FFI narrow (ADR-006).
- **Cost:** WKWebView and WebView2 diverge. Mitigated by choosing SVG over
  bleeding-edge CSS (ADR-007) and by testing the topology on both early.

---

## ADR-002 — Unprivileged collection only. No root, no admin, ever in v1

**Context.** Deep visibility — packet capture, per-connection byte counts — requires
`/dev/bpf` on macOS (root, or Wireshark's ChmodBPF group hack) and ETW on Windows
(admin). Both imply a privileged helper: a launchd daemon and a Windows service.

**Decision.** The collection surface is exactly what an unprivileged process can
read. No elevation prompt exists in the product.

**Rejected — privileged helper for packet capture.** It would consume a large share
of the first release, create a permanent privileged attack surface, require
platform-specific installer and lifecycle code, and directly contradict "no packet
payload collection." Users would be asked for their password by a network tool
before it had earned any trust.

**Rejected — optional elevation for "advanced mode."** Splits the product into two
capability tiers, doubles the testing matrix, and makes the Visibility Panel — the
central feature — conditional and therefore much harder to trust.

**Consequences.**
- No installer privilege escalation. No daemon. No service. No elevation dialog.
- The Visibility Panel becomes honest by construction rather than by discipline.
- **Cost:** per-application bandwidth and per-destination byte counts are
  permanently unavailable. This is stated in the UI, not concealed (ARCHITECTURE §8.4).
- **Enforced:** CI asserts the app runs and passes its smoke suite as a standard
  user, and that no capture library appears in the dependency graph.

---

## ADR-003 — Collector in-process, behind a daemon-ready boundary

**Context.** Where should collection execute relative to the UI?

**Decision.** `collector` is a standalone crate with an async, transport-agnostic
API that imports no Tauri symbols. In v1 the Tauri host calls it **in-process**.

**Rejected — separate sidecar process now.** Enables background collection and
future selective elevation, but adds IPC framing, serialization, lifecycle
management, crash supervision, and an IPC security boundary — for capability we do
not need, because ADR-002 removed the reason to elevate anything.

**Rejected — full daemon plus thin UI now.** All of the above, plus installer
privilege work and platform service lifecycle. Strictly premature.

**Consequences.**
- One unprivileged process. Simple to reason about, simple to ship.
- The migration path is real and cheap: wrap the same crate in a daemon binary,
  swap the call site for local IPC. `core` and the UI are untouched (ARCHITECTURE §15).
- **Cost:** no collection while the app is closed. Acceptable — silent background
  monitoring would contradict "no hidden monitoring."
- **Enforced:** CI fails if `collector` gains a Tauri dependency.

---

## ADR-004 — No accounts, no backend, no licensing in v1

**Context.** The original brief described account creation or activation.

**Decision.** The app opens directly into the dashboard. No signup, no login, no
license key, no server.

**Rejected — offline license key.** Modest work (signature verification, a key
issuer script) and it gates distribution, but it adds friction to the exact 30
seconds we are optimizing, for revenue protection on a product with no revenue yet.

**Rejected — full cloud auth.** A server, a database, and user records. It also
makes the privacy claim dramatically harder to defend: the moment accounts exist,
"what do you store about me?" has a much longer answer.

**Consequences.**
- Zero friction between download and understanding — the core product goal.
- "We have no server, so we cannot receive your data" is a claim that survives
  inspection.
- **Cost:** no usage analytics. Accepted; we would not collect them anyway.
- The packaging boundary stays clean so licensing can be added later at the
  installer layer without touching application code (ADR-013).

---

## ADR-005 — SQLite locally, with a short retention window

**Context.** "Is this device new here?" and "what has throughput looked like?"
require history. Nothing else does.

**Decision.** One SQLite file (`rusqlite`, bundled, WAL) in the OS app-data
directory. 7-day default retention, swept at startup. `clear_all_data()` deletes
the file outright.

**Rejected — no persistence.** Simpler, and maximally private, but it removes
new-device detection and throughput history — two of the more compelling demo
moments — for little privacy gain, since the data never leaves the machine.

**Rejected — embedded time-series store.** Better for high-cardinality metrics we
have deliberately chosen not to collect. Over-engineered.

**Consequences.**
- The schema is short enough that a skeptical user can read it and verify the
  claims (ARCHITECTURE §11).
- The absent tables — connections, DNS, per-destination volume — are themselves a
  privacy statement.
- **Cost:** a migration story is needed from the first schema change onward.

---

## ADR-006 — Pure `core`, narrow platform FFI

**Context.** OS network APIs are unsafe, platform-divergent, and awkward to test.
They must not contaminate the domain logic.

**Decision.** `core` contains no OS-facing dependency and no Tauri dependency —
pure logic over typed inputs. All `unsafe` and every platform binding is confined to
`collector/macos/` and `collector/windows/`, behind the `Probe` trait.

**Rejected — platform code alongside domain logic.** Fewer files, but classification
would become untestable without a live network, and CI could not verify the
privacy invariants.

**Consequences.**
- Classification, identity resolution, and the capability matrix are unit-testable
  against fixtures, offline, deterministically.
- `unsafe` review is concentrated in two directories.
- Directly mitigates the Rust learning-curve risk: most logic is safe, ordinary Rust.
- **Cost:** signal types must be defined before probes can be written. This is
  sequenced into M0 (see MVP_ROADMAP).

---

## ADR-007 — Radial SVG topology, not a force-directed graph

**Context.** The topology view is the demo centerpiece.

**Decision.** A radial layout — router at center, this device highlighted, devices
in fixed angular category arcs — rendered as SVG.

**Rejected — D3 force-directed graph.** The reflexive choice, and wrong here. A home
network *is* a star: one router, N leaves. A force simulation on a star produces
jitter, unstable positions between renders, and the generic "network graph" look
seen in every security dashboard. Radial is simultaneously prettier, more stable,
and more truthful about the actual topology.

**Rejected — Canvas or WebGL rendering.** Necessary above roughly a thousand nodes.
Home networks have tens. Canvas would cost stable hit-testing, DOM-addressable
nodes, real focus management, and screen-reader access, in exchange for performance
we do not need.

**Consequences.**
- Node positions are deterministic and stable across renders — devices do not
  wander between frames.
- Category arcs make the composition legible at a glance, which serves the
  30-second goal directly.
- Accessibility comes largely for free; the parallel list view is first-class.
- **Cost:** re-evaluate if a network exceeds ~150 nodes. Enterprise topologies are
  explicitly not a v1 target.

---

## ADR-008 — Evidence-based classification, Unknown by default

**Context.** Guessing device types produces a more impressive-looking screenshot
and a less trustworthy product.

**Decision.** Categories are derived from weighted signals. A category is assigned
only on **High** confidence, or on **Medium** confidence with **at least one
non-OUI signal**. Otherwise: **Unknown**. Corollary: **an OUI vendor match alone
never classifies a device.**

**Rejected — best-guess classification.** A fuller-looking topology, at the cost of
the product's entire premise. "Apple, Inc." cannot distinguish a MacBook from an
iPhone from an Apple TV, and a confidently wrong label is the fastest way to lose a
technical audience.

**Rejected — ML-based fingerprinting.** Needs training data we do not have and
produces conclusions we could not explain in the Device Inspector.

**Consequences.**
- Unknown is presented as informative — *"12 devices seen · 3 not identifiable"* —
  never as an error.
- Every conclusion is explainable, because signals are retained as evidence and
  shown in the Device Inspector.
- Randomized-MAC devices are labeled as protecting their identity rather than
  mis-attributed (ARCHITECTURE §8.1).
- **Cost:** demo networks will contain unidentified devices. This is the intended
  behavior and should be narrated as such.

---

## ADR-009 — Passive by default, active sweep strictly opt-in

**Context.** ICMP sweeps find hosts that are absent from ARP and silent on mDNS.
They also look exactly like reconnaissance to any EDR or IDS product.

**Decision.** ARP, mDNS, and SSDP run by default. `icmp_sweep` never runs
automatically, requires per-invocation consent with disclosure, and is disabled
entirely on Public/Guest and Enterprise network postures.

**Rejected — active sweep on by default.** Faster, fuller device lists, at the cost
of potentially triggering a corporate security alert during a demo. A privacy-first
tool that surprises a security team has failed on its own terms.

**Rejected — TCP connect sweep.** Works unprivileged everywhere and reveals open
services, but it is materially noisier than ICMP and closer still to port scanning.

**Consequences.**
- Default operation is indistinguishable from ordinary LAN participation — mDNS and
  SSDP traffic identical to what every phone on the network already emits.
- Safe to run on a client's network, at a conference, or in an interview.
- **Cost:** the default device list may be shorter. Mitigated by mDNS being the
  richest source anyway.

---

## ADR-010 — Bundled IEEE OUI database

**Context.** MAC prefix to vendor is the cheapest large improvement in perceived
intelligence.

**Decision.** Bundle a trimmed IEEE MA-L registry, compiled into the binary as a
sorted lookup table. Refreshed at build time by a script.

**Rejected — online OUI lookup.** Would introduce network egress for every unknown
device, leaking the user's device inventory to a third party. Categorically
incompatible with the privacy model.

**Consequences.**
- Instant, offline lookup with zero egress.
- **Cost:** ~1–2 MB of binary size, and the data ages between releases. Acceptable.
- **Important limitation:** modern phones randomize MACs per SSID, so OUI lookup
  returns nothing for the devices users most want identified. The product treats
  this as a feature to explain (ADR-008), not a gap to paper over.

---

## ADR-011 — Public IP lookup off by default

**Context.** Showing the public IP and ISP is a satisfying detail, and it requires
contacting an external service.

**Decision.** Off by default. Available as a one-tap action that names the endpoint
being contacted before contacting it.

**Rejected — on by default.** It is a small, defensible request. But an app whose
first claim is "nothing leaves your machine" must not make an outbound call to a
third party during startup. The inconsistency would be noticed, and rightly.

**Rejected — omit entirely.** Loses a genuinely useful data point for no gain, since
explicit consent resolves the concern cleanly.

**Consequences.**
- The egress inventory (ARCHITECTURE §14) stays short, complete, and auditable.
- The interaction itself demonstrates the product's ethic in miniature.

---

## ADR-012 — macOS first, Windows second

**Context.** Both are v1 targets. They cannot be built simultaneously in 3–6 weeks.

**Decision.** macOS to full polish first; Windows parity immediately after, in M7.

**Rejected — Windows first.** Windows has genuinely better unprivileged APIs
(`GetExtendedTcpTable` gives socket-to-process mapping with no admin, `WlanAPI`
needs no location permission) and a larger user base.

**Rejected — parallel development.** Doubles the surface before either platform is
proven and delays the first demonstrable result past the useful point.

**Reasoning.** macOS is the harder permission environment — Location Services for
SSID, the Sequoia Local Network prompt — and those are the highest-ranked demo risk.
Solving the hard platform first means the capability model is validated under
pressure, and Windows becomes a simplification rather than a discovery.

**Consequences.**
- Permission-cliff handling is designed against the strictest case.
- **Cost:** Windows users wait. Acceptable for a first demo.
- Platform-specific code stays behind the `Probe` trait, so M7 adds modules rather
  than restructuring anything.

---

## ADR-013 — Defer code signing, notarization, and auto-update

**Context.** The product is a demo now and a shippable product later.

**Decision.** Ship unsigned local builds for the demo phase. Defer Apple Developer
enrollment, Windows EV certificate, notarization, and an update channel to the
product phase.

**Rejected — sign from day one.** Roughly $500/year and meaningful setup time before
there is anything worth distributing.

**Consequences.**
- **Known cost:** unsigned builds trigger Gatekeeper and SmartScreen, and SmartScreen
  is especially aggressive toward an unsigned binary that performs network
  discovery. Demo distribution must account for this.
- Mitigated architecturally: signing, notarization, updates, and licensing all live
  at the installer/packaging layer. Adding them later touches no application code
  (ADR-004, ARCHITECTURE §15).

---

## ADR-014 — Rename `telemetry/` to `metrics/`  *(applied)*

**Context.** The scaffold contained a `telemetry/` directory.

**Decision.** Renamed to `metrics/`, scoped explicitly to **local development and
performance measurements only** and documented as such in `metrics/README.md`.

**Reasoning.** In a privacy-first security product, "telemetry" reads as phone-home.
It is among the first words a skeptical user, reviewer, or security-conscious buyer
will search for in the repository. Naming is part of the trust surface, and the
directory did not do what its name implied.

**Rejected — delete it and fold the role into `core`.** Defensible, but development
measurement helpers genuinely do not belong in the domain crate, and `core` is
required to stay free of I/O (ADR-006).

**Consequences.**
- The directory now states, in its own README, that no user data, network inventory,
  device information, or telemetry leaves the device — with a pointer to the complete
  five-item egress inventory in ARCHITECTURE §14.
- Trivial cost. Applied 2026-08-24.

---

## ADR-015 — Mobile as a companion application, not a port

**Context.** Android and iOS are stated future targets.

**Decision.** Out of scope for v1. When built, mobile is a companion with a
different, smaller promise. `core` is reused; the collection surface is not.

**Rejected — cross-platform parity as a goal.** Not achievable. iOS provides no ARP
table, no raw sockets, and no neighbor enumeration. Promising parity would require
either misleading marketing or abandoning the honesty that defines the product.

**Consequences.**
- The capability matrix (ARCHITECTURE §9) generates the honest mobile framing
  automatically — no separate messaging work.
- Android is meaningfully more capable than iOS, including per-app data usage via
  `NetworkStatsManager`, which desktop cannot offer without admin. Worth noting: the
  capability ordering is not simply desktop > mobile.
- Choosing Tauri v2 (ADR-001) keeps this path open without a second core.

---

## ADR-016 — A tunnel is a route, not a connection type

**Context.** `ConnectionType::Vpn` was one of the connection kinds, so a VPN
holding the default route made the connection "VPN" and the interface `utun6`.

**Decision.** Removed. `NetworkIdentity` describes the physical link, and a
tunnel is reported alongside it.

**Rejected — keeping Vpn as a connection kind.** Simpler, and it is what most
tools show. But it loses the answer to the question the user actually asked —
which network am I on — along with the real address, subnet and gateway.

**Consequences.**
- The physical link beneath a tunnel is resolved from the routing table, since
  the physical interface keeps routes of its own while the tunnel holds only
  the default. Ties break by name so the answer never depends on enumeration
  order.
- A tunnel with nothing identifiable beneath it reports Unknown and still names
  the tunnel, rather than presenting the tunnel as the physical connection.
- **Cost:** a heuristic. Documented as one, and covered by fixtures for VPN
  over Wi-Fi and over Ethernet.

---

## ADR-017 — Fixtures are development-only, enforced at compile time

**Context.** Validating seven connection modes requires networks that cannot
all exist on one machine at one time.

**Decision.** A `fixtures` cargo feature, off by default, that supplies routes,
interfaces and observations to the *real* pipeline. `compile_error!` in both
`jrx-collector` and `jrx-app` when the feature is enabled without
`debug_assertions`.

**Rejected — a separate demo data model.** Far easier, and worthless: a demo
model that behaved differently from production would validate nothing, and
would drift the moment production changed.

**Rejected — a runtime flag or a hidden UI switch.** A shipped binary that can
be made to fabricate a network on command is a binary that can lie.

**Consequences.**
- `cargo build --release --features fixtures` fails to compile. Verified.
- A test asserts the feature is never in a default set in either manifest.
- **Cost:** fixtures must be kept faithful. The university fixture initially
  used invented OUI prefixes and reported zero manufacturer-known devices —
  nothing like the network it stands in for. Caught during M4.5 and corrected.

---

## ADR-018 — Run Apple's `nettop`; never link the framework behind it

**Context.** Per-process byte counts are the substance of an activity view.
`/usr/bin/nettop` provides them unprivileged, system wide, at ~9 ms per sample.
It links `/System/Library/PrivateFrameworks/NetworkStatistics.framework`.

**Decision.** Shell out to the shipped binary. Never link the framework.

**Rejected — linking NetworkStatistics directly.** Removes a process spawn and
the 15-character name truncation. It is a private API: undocumented, free to
change between OS versions, and grounds for App Store rejection.

**Rejected — deriving per-process bytes from the socket table.** Would mean
inventing numbers from the existence of a connection, which is the specific
dishonesty ADR-008 exists to prevent.

**Consequences.**
- Two process spawns per refresh; 12.9 ms total, 1.3% of one core at 1 Hz.
- Process names arrive truncated at 15 characters and are completed from the
  PID via `proc_pidpath`, which is native and documented. An exited process
  keeps the truncated name, flagged as truncated.
- **Cost:** the first call after boot takes 6.3 seconds while `nettop`
  initialises. It must be warmed at startup.

---

## ADR-019 — No IP-to-domain mapping, at all

**Context.** Users expect a traffic view to name websites.

**Decision.** JRX does not map addresses to domains. The field exists in the
model and is permanently `None`.

**Evidence.** Reverse DNS was measured against 12 live endpoints on this
machine and resolved **zero** of them — Cloudflare, Apple, Google Cloud and
Telegram publish no PTR records. Where it does resolve, it names the host that
answered, not the site requested: one address fronts millions of sites.

**Rejected — observing DNS queries.** It is the only reliable source, and it
is precisely what `DataClass::DnsQueryHistory` refuses. A record of every name
you look up is a record of everywhere you go.

**Rejected — online ASN or WHOIS lookup.** Would disclose every address this
Mac contacts to a third party, the same inventory-leak problem ADR-010 ruled
out for MAC vendors.

**Consequences.**
- Network owner from bundled published ranges instead: 12 of 28 live
  connections identified, labelled "network owner only".
- **Cost:** the product cannot answer "which sites have I visited". That is
  the correct answer to give, and it is now a stated boundary rather than a
  gap.

---

## ADR-020 — One `nettop` per sample, warmed once

**This decision was made twice.** The first version of this record chose a
single long-lived `nettop -L 0` streaming child, on the strength of a
measurement showing spawn-per-sample averaging 77–1259 ms with a p95of 7.8 s.
That measurement was wrong: it averaged in the tool's cold first call, which
costs seconds once and never again.

Measured properly — twelve spawns at one-second intervals — the first costs
4195 ms and every subsequent one costs 20–71 ms, typically 27 ms.

And the streaming alternative has a cost the first measurement never looked
for: **`nettop -L 0` sustains 128% CPU**, at any `-s` interval, with its output
being drained. Confirmed standalone, so it is the tool's own behaviour and not
an artefact of how JRX reads it.

**Decision.** Spawn one `nettop -L 1` per sample. Pay the first call's cost in
`warm()`, off the critical path.

| | latency | CPU |
|---|---|---|
| streaming `-L 0` | ~8 ms, stable | **128% of a core, sustained** |
| per-sample `-L 1` | ~36 ms, p95 39 ms | ~3.6% of a core at 1 Hz |

**Rejected — streaming.** Better latency, and unusable: a background monitor
that permanently occupies a core is not something to ship, whatever it buys.

**Rejected — linking NetworkStatistics directly.** Would avoid the child
process entirely. It is a private API (ADR-018).

**Consequences.**
- Two short-lived children per tick, each reaped by the capture helper. No
  long-lived child means nothing to orphan if the app is killed.
- A first sample that is slow is reported as *starting up*, not as a failure.
- **Cost:** ~36 ms per tick rather than ~8 ms. Worth an order of magnitude less
  CPU.
- **The lesson worth keeping:** the first measurement compared the wrong
  things. Latency was measured for both options; CPU was measured for neither
  until the app was actually running.

---

## ADR-021 — Local persistence for recognition, with a mobile-ready split

**Context.** JRX shows a truthful snapshot but remembers nothing: every launch
is amnesiac. The two things that make it an *observatory* rather than a live
monitor — "you have been on this network before" and "this device is new here" —
require state that survives a restart. It must not compromise the local-first,
no-cloud, honesty principles, and it should not have to be rebuilt for a future
iOS client (ARCHITECTURE.md §17).

**Decision.** A local SQLite store (via `rusqlite`, SQLite bundled), holding
**one-way digests** of network and device keys — never SSIDs, BSSIDs, MACs, or
addresses. The recognition logic that turns a live observation into a key, and a
stored match into an honest verdict, lives in pure `core` (`core::history`); the
SQLite adapter is a thin platform boundary the `core` logic never sees. Records
are kept for a bounded window (recognition: ~90 days since last seen) and
`clear_all_data()` erases the store verifiably.

**Rejected — no persistence.** Keeps the code simpler but leaves JRX unable to
answer its two most valuable questions; the product name would be a promise it
does not keep.

**Rejected — storing readable identities (SSID/BSSID/MAC in plain columns).**
A local, readable log of every network you have joined and every device you have
seen is exactly the dossier JRX exists not to build. Digesting the keys means
the store can *recognise* a network on return without *recording* what it is.

**Rejected — cloud sync / a hosted history.** Violates local-first and the
no-account, no-backend stance (ADR-004). Recognition is only ever needed on the
machine that observed it.

**Consequences.**
- `core::history` is pure and fully unit-tested offline, and is the exact logic
  a future iOS app reuses; only the SQLite adapter is rewritten per platform.
- A randomised MAC is never persisted as a device identity, so JRX never reports
  a rotating phone as a "new device" (ADR-008, and `history::device_standing`).
- A network match carries its evidence strength: a hardware match (BSSID or
  gateway MAC) is confident; an addressing-only match is reported as *likely*,
  because many networks share `192.168.1.0/24`.
- **Honest limitation:** the digest is a stable fingerprint (FNV-1a), not a
  cryptographic commitment. It keeps a casual reader of the local database from
  seeing a plaintext history; it is not proof against a determined attacker who
  already has the file and brute-forces a 48-bit address. The database is
  local-only and never transmitted, which is the boundary that matters.
- The `rusqlite` dependency is added when the storage adapter lands, not by this
  record.

---

## Summary

| ADR | Decision |
|---|---|
| 001 | Tauri v2 + Rust; not Electron, Go, or native |
| 002 | Unprivileged collection only; no root, ever, in v1 |
| 003 | In-process collector behind a daemon-ready boundary |
| 004 | No accounts, no backend, no licensing in v1 |
| 005 | Local SQLite, 7-day retention, real erase |
| 006 | Pure `core`; `unsafe` and FFI confined to `collector` |
| 007 | Radial SVG topology; not force-directed, not Canvas |
| 008 | Evidence-based classification; Unknown by default; OUI alone never classifies |
| 009 | Passive by default; ICMP sweep opt-in and posture-gated |
| 010 | Bundled IEEE OUI database; no online lookup |
| 011 | Public IP lookup off by default, endpoint disclosed |
| 012 | macOS first, Windows second |
| 013 | Signing, notarization, and updates deferred to the product phase |
| 014 | Rename `telemetry/` → `metrics/` |
| 015 | Mobile is a companion, not a port |
| 016 | A tunnel is a route, not a connection type |
| 017 | Fixtures are development-only, enforced at compile time |
| 018 | Run Apple's `nettop`; never link the framework behind it |
| 019 | No IP-to-domain mapping, at all |
| 020 | One `nettop` per sample, warmed once; not a long-lived streaming child |
| 021 | Local SQLite for recognition; digest-only, core/adapter split for mobile |
