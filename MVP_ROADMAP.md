# JRX Network Observatory — MVP Roadmap

**Status:** Approved · **Date:** 2026-08-24 · **Target:** demo-ready in ~4 weeks, v1 complete in ~6

---

## 1. The product goal, stated as a test

> **A person downloads JRX, opens it, and understands their network within 30 seconds — with no onboarding, no account, and no explanation from us.**

This is the acceptance criterion for the MVP, not a slogan. It is testable, and it
is the tie-breaker for every scope argument below.

**The 30-second test.** Sit someone in front of a cold-launched JRX. Say nothing.
After 30 seconds, they must be able to answer, unprompted:

1. What kind of network am I on, and what is it called?
2. Which device on this screen is mine?
3. What is the router, and where is it?
4. Roughly how many devices are around me, and what kinds?
5. **What can this app see about me — and what can it not?**

Question 5 is the one that separates JRX from a scanner. It is why the Visibility
Panel ships in M2, before the topology.

**The failure mode we are designing against:** a person opens the app on guest WiFi,
sees an empty circle and no explanation, and closes it. On *any* network — isolated,
VPN'd, permission-denied, Ethernet-only — JRX must be able to explain itself. An
observability tool that shows nothing without saying why is broken.

### First-impression priority order

Fixed, and it drives the milestone sequence:

1. **Network identity** — the immediate answer
2. **Visibility explanation** — what we can and cannot see, here, now
3. **Device discovery** — who else is here
4. **Radial topology** — the picture that makes it click

---

## 2. Milestones

Estimates assume one developer at five focused days per week, ramping up on Rust.
Dependencies are strictly sequential unless noted.

---

### M0 · Foundation — 3 days

Scaffolding only. No product surface.

- Cargo workspace: `core`, `collector`, `app/src-tauri`
- Tauri v2 + React + TypeScript + Vite; hot reload working both directions
- Core type definitions: `Signal`, `Device`, `NetworkProfile`, `Capability`,
  `Category`, `Confidence`
- The `Probe` trait, with `posture()`, `requires()`, and `reads()`
- CI: build, clippy, fmt, test
- **The four privacy invariant checks, wired on day one:** no capture library in
  the dependency graph; no undeclared `reads()`; CSP permits no remote origin; app
  runs unprivileged

**Exit:** empty app window launches on macOS; CI green; invariant checks fail
correctly when deliberately violated.

> Wiring the invariants in M0 rather than at the end is the point. They are how the
> privacy principles survive the twentieth commit rather than becoming a comment.

---

### M1 · Network Identity — 3 days · *Priority 1*

The immediate answer.

- `interfaces`, `routes`, `wifi` probes (macOS)
- Connection-type classification: Wi-Fi · Ethernet · Cellular hotspot · Virtual
  (VPN) · Unknown
- Hotspot detection heuristic — **labeled as a heuristic in the UI**
- Network posture inference: Home · Public/Guest · Enterprise · Isolated
- `NetworkProfile` fingerprint (local hash — never leaves the device)
- Network Identity card in the UI

**Exit:** cold launch answers "you are on Wi-Fi *Network*, 5 GHz, via router at
192.168.1.1" in under 400 ms. Correct on Wi-Fi, on Ethernet, on an iPhone hotspot,
and with a VPN active.

---

### M2 · Visibility Panel — 4 days · *Priority 2 · highest-risk milestone*

The differentiator, and deliberately scheduled before the pretty picture.

- Capability matrix generated from probe declarations plus live permission state
- Four states rendered: **Observed** · **Available** · **Not possible** ·
  **Refused by design**
- Live permission detection on macOS: Location Services (required for SSID) and
  Local Network (required for mDNS/SSDP on Sequoia+)
- Inline grant actions for the Available state
- **Every degraded state from ARCHITECTURE §12 implemented with specific copy** —
  permission denied, client isolation, VPN active, no WiFi hardware, genuinely
  empty network
- Snapshot tests per permission-state combination, including all-denied

**Exit:** with Location and Local Network both denied, the app is still fully
comprehensible and tells the user exactly what is missing, why, and how to fix it.

> **This milestone retires the top-ranked project risk.** macOS Sequoia's Local
> Network prompt makes mDNS return silently empty when denied, and Sonoma requires
> Location Services merely to read an SSID. A demo that shows zero devices with no
> explanation is the single worst outcome available to this product. Solving that
> before building the map is the whole reason for this ordering — and it means every
> later milestone inherits a working permission story instead of discovering one.

---

### M3 · Passive Discovery — 5 days · *Priority 3*

Who else is here.

- `arp` probe — immediate device seeds from the OS neighbor cache
- `mdns` probe — hostnames and service types (**the highest-value signal**)
- `ssdp` probe — UPnP devices
- Bundled IEEE OUI database and build-time refresh script
- Device identity resolution, ordered: MAC → UPnP UUID → mDNS instance → ephemeral
- **Randomized-MAC detection and labeling**
- Evidence-based classification into the five categories, enforcing the
  over-classification rule
- Device list view (the accessible parallel to the topology, built first)

**Exit:** on a real home network, devices appear within 1 s and enrich over ~8 s.
Router, printer, and TV classify correctly. Phones with randomized MACs are labeled
as such rather than guessed. Unknown count is displayed as informative, not as an
error.

---

### M4 · Radial Topology — 5 days · *Priority 4 · the demo centerpiece*

- Radial SVG layout: **router at center**, **this device highlighted**
- Fixed angular arcs per category: Computers · Phones · Smart home ·
  Infrastructure · Unknown
- Stable, deterministic node positions across renders
- **Staged reveal driven by real discovery events** — center → this device → ring
  skeleton → nodes fading in as found. The animation *is* the data arriving, not a
  loading simulation.
- Neutral, non-speculative treatment for Unknown nodes
- Device Inspector on selection — **showing the evidence behind every conclusion**
- `prefers-reduced-motion` renders the settled state directly
- Keyboard navigation; category never conveyed by color alone
- Visual review at 0, 1, 12, and 60 devices

**Exit: DEMO-READY.** The 30-second test passes with a person who has never seen
the app.

**≈ Week 4.** This is the milestone worth showing people.

---

### M4.5 · Product validation — complete

Not in the original plan. Added because M0–M4 had never been checked as a
single product, and several connection modes had gone four milestones without
validation.

- Physical link separated from tunnel: a VPN no longer replaces the connection
- Seven deterministic scenario fixtures, development-only, running through the
  real pipeline
- Home screen reordered to network → this device → devices → visibility
- Four distinct degraded states, chosen by the quality model
- Group filtering on observed facts
- Bounded rendering proved at 506 devices: 7 nodes at level 1, 40 at level 2
- Two bugs found only by running against a real network (see the M4.5 report)

**Exit:** the 30-second test passes on a real network and on every fixture.

---

### M5 · My Device + Network Activity — complete

The scope changed once phase 0 measured what is actually available. The plan
above assumed per-application bandwidth was impossible unprivileged and that
endpoints would be named by reverse DNS and ASN. Both assumptions were wrong,
in opposite directions, and the milestone was rebuilt around the measurements
(see docs/M5_PHASE0_FEASIBILITY.md and docs/M5_REPORT.md).

- Interface throughput from `netstat -ib` counter deltas — available on the
  first frame, never blocked by the slower provider
- Per-program activity from `/usr/bin/nettop`, unprivileged: which programs are
  talking, session bytes each, active connection count, per-connection remote
  address / port / protocol / state / owner
- A session accounting model that survives a socket closing, a counter reset,
  an interface switch and a reused PID — totals are *observed since JRX opened*,
  never since boot
- Offline network-owner attribution from bundled published ranges; **no**
  reverse DNS, **no** ASN, **no** IP-to-domain (ADR-019)
- Two providers that fail separately, four honest health states, and an
  under-reporting note when program totals fall short of the interface total

**Corrected assumption:** per-application bandwidth is *not* impossible on
macOS (the original §5 out-of-scope entry and ARCHITECTURE §8.4 both said so).
`nettop` provides it unprivileged. What is genuinely unavailable is the far
end's *identity* — domain, service — and that is refused, not merely missing.

**Exit:** on the real Mac, throughput responds to a live download, the program
list is readable by a non-expert, and nothing on screen implies a website,
domain or service. Validated end-to-end in the bundled Tauri app, not only in
preview.

---

### M6 · Persistence and Polish — 3 days

- SQLite schema, migrations, 7-day retention sweep
- **Known vs. new device** — "3 devices you have not seen on this network before"
- Network recognition — "you have been on this network before"
- `clear_all_data()` — real, immediate, verifiable erase
- Empty, loading, and error states finished across every view
- Visual design pass: dark, restrained, monospace for numerics, one accent colour

**Exit:** relaunch recognizes the network and correctly flags new devices. Clearing
data demonstrably empties the store.

---

### M7 · Windows Parity — 5 days

- `GetAdaptersAddresses`, `GetIpForwardTable2`, `GetIpNetTable2`, `GetIfEntry2`
- `WlanAPI` for WiFi details (**no location permission required — simpler than macOS**)
- `GetExtendedTcpTable` / `…UdpTable` for socket-to-process mapping
- `IcmpSendEcho2` for the opt-in sweep
- Windows capability matrix and its own degraded states
- WebView2 rendering verification for the topology
- MSI/NSIS packaging (unsigned — ADR-013)

**Exit:** feature parity on Windows 11; capability matrix correctly reflects the
platform differences.

**≈ Week 6.** v1 complete.

---

## 3. Schedule

| Milestone | Days | Cumulative | Marker |
|---|---|---|---|
| M0 Foundation | 3 | 3 | |
| M1 Network Identity | 3 | 6 | |
| M2 Visibility Panel | 4 | 10 | *risk retired* |
| M3 Passive Discovery | 5 | 15 | |
| M4 Radial Topology | 5 | 20 | **DEMO-READY — week 4** |
| M5 My Device + Network Activity | 3 | 23 | **complete** |
| M6 Persistence & Polish | 3 | 26 | |
| M7 Windows Parity | 5 | 31 | **v1 COMPLETE — week 6.2** |

**Honest reading:** 31 working days is ~6.2 weeks with **zero slack**, and slack is
always consumed. Treat week 4 (M4, macOS demo) as the reliable commitment and week 6
as the optimistic one. If Rust ramp-up runs long, the descope levers in order are:
**M5 first** (traffic is the least essential to the 30-second promise), then **M7**
(ship macOS-only and add Windows after the demo). M0–M4 are not negotiable — they
*are* the product.

---

## 4. Demo script

Roughly two minutes, mapped to the priority order.

1. **Open the app cold.** Say nothing for ten seconds. Let the reveal run. The
   network identity resolves, the ring populates, devices name themselves.
2. **"It already knows where it is."** Wi-Fi, network name, band, router, and that
   it recognizes this network from before.
3. **"Here is the part nobody else ships."** The Visibility Panel. Walk all four
   columns — and land on **Refused by design**. *This is what we could build and
   chose not to.*
4. **"And here is what it will not pretend to know."** Open an Unknown device.
   Randomized MAC, no vendor, no guess. *Most tools would have labeled this an
   iPhone. We don't guess.*
5. **Trigger a download.** The throughput strip responds live; endpoints resolve to
   recognizable names.
6. **Optional, if the audience is technical:** run the active sweep — showing the
   consent disclosure first, and noting it is disabled on guest and corporate
   networks by design.

**Demo hygiene.** Rehearse on the actual network. On guest or conference WiFi,
client isolation may reveal nothing — in which case the isolated-network explanation
*is* the demo, and it is a genuinely good one: the app explains a network property
that most people never knew existed.

---

## 5. Explicitly out of scope for v1

Postponing these is a decision, not an omission.

| Deferred | Reason |
|---|---|
| Accounts, backend, licensing | ADR-004 — friction in the exact 30 seconds we are optimizing |
| Download website | Not needed to demo; separate project |
| Code signing, notarization, auto-update | ADR-013 — ~$500/yr before there is anything to distribute |
| Mobile (Android, iOS) | ADR-015 — a companion with a smaller promise, not a port |
| Privileged daemon / background collection | ADR-002, ADR-003 — no probe needs it |
| GeoIP maps | MaxMind licensing; offline network-owner attribution covers the need (not ASN — ADR-019) |
| ~~Per-application bandwidth~~ | ~~Requires admin~~ — **shipped in M5**; `nettop` provides it unprivileged (phase 0 overturned this) |
| Alerting and notifications | Needs a trustworthy baseline first; premature |
| Reporting and export | No demonstrated need yet |
| Linux support | Third platform; no current requirement |

**Never, at any version:** packet capture · payload inspection · DNS query logging ·
browsing history · credential access · silent background monitoring · cloud upload
of device inventories. These are absent from the dependency graph, not disabled by
a flag.

---

## 6. Risk register

| # | Risk | Impact | Mitigation | Retired by |
|---|---|---|---|---|
| 1 | macOS permission cliffs — Sequoia Local Network, Sonoma Location-for-SSID. Denial makes mDNS return **silently empty** | **Critical** — kills a demo with no visible cause | Detect and explain every state; inline grant actions; snapshot tests for all-denied | **M2** |
| 2 | Client isolation on guest/corporate WiFi returns zero devices | High — demo on wrong network shows nothing | Designed "isolated network" state; explaining it *is* a good demo | **M2** |
| 3 | MAC randomization defeats OUI on exactly the devices users care about | Medium | mDNS names carry identification; randomization labeled honestly (ADR-008) | **M3** |
| 4 | Rust ramp-up exceeds estimate | Medium — schedule slip | Pure `core` keeps most logic in safe, ordinary Rust; FFI confined to two directories; descope M5 then M7 | Ongoing |
| 5 | WKWebView / WebView2 divergence on the topology | Medium | SVG over bleeding-edge CSS (ADR-007); test both by end of M4 | **M7** |
| 6 | SmartScreen/Gatekeeper flag an unsigned network-discovery binary | Medium — distribution friction | Accepted for demo phase; packaging boundary kept clean so signing adds later without code change | Product phase |
| 7 | Active sweep trips corporate EDR | Medium — reputational | Opt-in per invocation, disclosed, posture-gated off (ADR-009) | **M3** |
| 8 | Sparse home network makes a thin demo | Low | Device list plus Visibility Panel carry the story; empty states are designed, not accidental | **M4** |

Risks 1 and 2 are the ones that matter, they share a mitigation, and both are
retired by M2. That is the entire justification for building the Visibility Panel
before the topology.

---

## 7. After v1

Sequenced by dependency, not committed to dates.

**Product phase (unlocks distribution).** Apple Developer enrollment and
notarization; Windows EV certificate; auto-update channel; the download website;
optional offline licensing (ADR-004). All at the packaging layer — no application
code changes.

**Depth (needs a trustworthy baseline first).** New-device alerting; longer
retention with real historical analytics; richer service fingerprinting; network
comparison across locations.

**Platform.** Android companion — meaningfully more capable than iOS, including
per-app data usage via `NetworkStatsManager`, which desktop cannot offer without
admin. Then iOS, framed accurately as own-device plus Bonjour-discoverable services.
Linux, if demand appears.

**Only if a probe genuinely requires it.** The daemon path from ARCHITECTURE §15 —
wrap the existing `collector` crate, swap the call site for local IPC. The boundary
exists so the option stays open. Exercising it needs a justification that does not
exist today.
