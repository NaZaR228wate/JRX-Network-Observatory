# JRX Network Observatory — Architecture

**Status:** Approved · **Date:** 2026-08-24 · **Version:** 1.0 (pre-implementation)

---

## 1. What this document is

The engineering contract for JRX Observatory v1. It defines module boundaries, the
collection model, the data flow, and the privacy guarantees that the code must
uphold. Technology choices and their rejected alternatives live in
[TECH_DECISIONS.md](TECH_DECISIONS.md); delivery sequencing lives in
[MVP_ROADMAP.md](MVP_ROADMAP.md).

JRX is the product identity. No other name appears in code, UI, bundle
identifiers, or documentation.

---

## 2. Product principles as architectural constraints

These are not aspirations. Each one binds a concrete part of the design, and each
is testable.

| # | Principle | Architectural consequence | How it is enforced |
|---|---|---|---|
| 1 | Privacy-first network intelligence | Collection surface is an explicit, enumerated allowlist — not "whatever we can get" | Every probe implements a trait that declares what it reads |
| 2 | No root/admin in v1 | No BPF, no ETW, no privileged helper, no launchd daemon, no Windows service | CI asserts the app runs and passes its smoke suite as a standard user |
| 3 | No packet payload collection | No capture library is a dependency at all | `pcap`, `pnet_datalink` capture APIs, and libpcap are denied in `deny.toml` |
| 4 | No cloud backend in v1 | No server, no API client, no sync | Tauri CSP + capability config permit zero remote origins |
| 5 | No accounts | App opens directly to the dashboard | No auth module exists |
| 6 | Local processing first | All classification, correlation, and storage happen on-device | The only egress is LAN multicast, plus one opt-in, off-by-default public-IP lookup |

**Principle 2 is load-bearing.** Everything downstream — the probe set, the
honest capability matrix, the packaging story, the absence of a daemon — follows
from refusing to ask for elevation. It is a product feature, not a limitation.

---

## 3. System overview

```
┌──────────────────────────────────────────────────────────────────────┐
│  app/  —  Tauri v2 shell                                             │
│                                                                      │
│   ┌────────────────────────────────────────────────────────────┐     │
│   │  WebView (React + TypeScript)                              │     │
│   │  Network Identity · Visibility Panel · Radial Topology     │     │
│   │  Device Inspector · Traffic Strip                          │     │
│   │  ── zero network logic, zero OS access ──                  │     │
│   └───────────────▲──────────────────────┬─────────────────────┘     │
│      events       │                      │  commands                 │
│   ┌───────────────┴──────────────────────▼─────────────────────┐     │
│   │  Rust host — command handlers, event bus, scheduler        │     │
│   └───────────────┬────────────────────────────────────────────┘     │
└───────────────────┼──────────────────────────────────────────────────┘
                    │  in-process async API (daemon-ready boundary)
        ┌───────────▼───────────┐        ┌────────────────────────┐
        │  collector/  (Rust)   │───────▶│  core/  (Rust)         │
        │  Platform probes      │signals │  Domain model          │
        │  macos/ · windows/    │        │  Identity · Classify   │
        │  The ONLY OS-facing   │        │  Capability matrix     │
        │  code in the system   │        │  SQLite store          │
        └───────────┬───────────┘        └────────────────────────┘
                    │ reads only
   ┌────────────────┴──────────────────────────────────────────┐
   │ getifaddrs · ARP/NDP table · CoreWLAN/WlanAPI · route     │
   │ mDNS 5353 · SSDP 1900 · ICMP echo · socket table · ifstat │
   └───────────────────────────────────────────────────────────┘
```

---

## 4. Repository layout

```
jrx-network-observatory/
├── core/         Rust lib crate — jrx-core
│   ├── model/          Device, NetworkProfile, Signal, Capability, Category
│   ├── identity/       Stable device identity resolution
│   ├── classify/       Evidence-based categorization
│   ├── capability/     The Visibility Panel's source of truth
│   └── store/          SQLite schema, migrations, retention
│
├── collector/    Rust lib crate — jrx-collector
│   ├── probe.rs        The Probe trait — every OS read goes through it
│   ├── passive/        interfaces, routes, arp, wifi, ifcounters, sockets
│   ├── discovery/      mdns, ssdp
│   ├── active/         icmp_sweep  (opt-in, never automatic)
│   ├── macos/          CoreWLAN, sysctl, proc_pidinfo bindings
│   └── windows/        IP Helper, WlanAPI, GetExtendedTcpTable bindings
│
├── app/          Tauri v2 application — jrx-app
│   ├── src-tauri/      Rust host: commands, events, scheduler, CSP
│   └── src/            React + TypeScript UI
│
├── metrics/      Local development & performance measurements only  (see §16)
├── docs/         Design notes, capability research, platform matrices
├── ARCHITECTURE.md
├── TECH_DECISIONS.md
└── MVP_ROADMAP.md
```

### Dependency rule

```
app  ──▶  collector  ──▶  core
app  ──▶  core
```

Strictly one-directional, enforced in CI.

- `core` has **no** OS-facing dependencies and **no** Tauri dependency. It is pure
  logic over inputs, so it is fully unit-testable with fixture data and runs
  identically on a CI box with no network.
- `collector` never imports Tauri. It exposes an async, transport-agnostic API.
  This is what makes the future daemon a wrapper rather than a rewrite (§15).
- The WebView never touches the OS. Its only inputs are Tauri commands and events.

---

## 5. Process and trust model

**v1 is a single process.** One unprivileged user-space process containing the
Tauri host and, in-process, the collector.

There is no agent, no daemon, no service, no helper tool, and no elevation prompt.
Because collection needs no privilege (§6), a separate privileged process would
add attack surface, installer complexity, and platform-specific lifecycle code in
exchange for nothing.

The process boundary that *does* matter is the **WebView boundary**. The renderer
is treated as untrusted:

- Tauri capability config exposes only the explicit command allowlist in §10.
- CSP denies all remote origins. No CDN, no font host, no analytics. Every asset
  is bundled.
- No command accepts a raw path, shell string, or arbitrary host. Commands take
  typed enums and validated device IDs only.
- The renderer cannot initiate a scan mode it was not offered, and cannot widen
  the collection surface.

---

## 6. The collection subsystem

### 6.1 The Probe contract

Every read of operating-system or network state implements one trait:

```
trait Probe {
    fn id(&self) -> ProbeId;
    fn posture(&self) -> Posture;          // Passive | Active
    fn requires(&self) -> &[Permission];   // LocalNetwork, Location, …
    fn reads(&self) -> &[DataClass];       // the declared collection surface
    async fn run(&self, ctx: &ProbeCtx) -> Result<Vec<Signal>, ProbeError>;
}
```

`reads()` is not documentation. It is the machine-readable declaration that the
Visibility Panel renders and that CI audits. Adding a probe that reads something
undeclared fails the build. This is how principle 1 stays true over time.

### 6.2 Passive probes — always on, zero network emission

| Probe | macOS | Windows | Yields |
|---|---|---|---|
| `interfaces` | `getifaddrs` | `GetAdaptersAddresses` | Link type, MAC, addresses, MTU, up/down |
| `routes` | `sysctl` route table | `GetIpForwardTable2` | Default gateway, per-interface metric |
| `wifi` | CoreWLAN (`CWInterface`) | `WlanQueryInterface` | SSID, BSSID, band, channel, RSSI, security |
| `arp` | `sysctl NET_RT_FLAGS` | `GetIpNetTable2` | Neighbors the OS already knows — pure cache read |
| `ifcounters` | `getifaddrs` AF_LINK | `GetIfEntry2` | Cumulative rx/tx bytes, sampled for throughput |
| `sockets` | `proc_pidinfo` | `GetExtendedTcpTable` / `…UdpTable` | Local↔remote endpoints, owning process |

The `arp` probe is the reason meaningful results appear in under a second: it
reads a table the OS has already populated. Nothing is sent.

### 6.3 Discovery probes — standard multicast, on by default

| Probe | Mechanism | Yields |
|---|---|---|
| `mdns` | DNS-SD over UDP/5353 multicast | **Hostnames and advertised service types** |
| `ssdp` | UPnP `M-SEARCH` over UDP/1900 | Routers, media renderers, printers, IoT |

These emit multicast queries indistinguishable from what macOS, Windows, and every
phone on the network already send continuously. They are ordinary participation in
LAN service discovery, not scanning.

**mDNS is the single highest-value signal in the product.** It is what converts a
list of IP addresses into "Living Room Apple TV" and "HP LaserJet". Its service
types are also the primary classification evidence (§8.3).

### 6.4 The active probe — opt-in, never automatic

`icmp_sweep` pings the local /24 to establish liveness for hosts absent from ARP
and silent on mDNS. It uses unprivileged datagram ICMP on macOS and
`IcmpSendEcho2` on Windows — no raw sockets, no elevation.

It is governed by three hard rules:

1. **Never runs automatically.** It requires a deliberate user action, every time.
2. **Discloses before running** what it will emit, to which subnet, and that
   corporate network monitoring may flag it.
3. **Is disabled entirely** when the network is classified as Public/Guest or
   Enterprise (§7.2).

Rationale: an unannounced subnet sweep looks like reconnaissance to any EDR or IDS
product. A privacy-first tool must not surprise a security team.

### 6.5 What is never implemented

Packet capture. Payload inspection. DNS query logging. Browsing history. Port
scanning beyond liveness. Credential access. Deauthentication or any active WiFi
technique. Silent background collection while the app is closed.

These are absent from the dependency graph, not merely disabled by a flag.

### 6.6 Scheduling

| Cadence | Probes |
|---|---|
| Once at cold start | `interfaces`, `routes`, `wifi`, `arp` |
| 1 s | `ifcounters` (throughput sampling) |
| 5 s | `sockets` |
| 15 s | `arp` re-read |
| Continuous listen + query burst at 0 s, 2 s, 5 s, 15 s | `mdns`, `ssdp` |
| On network change | Full re-run, new `NetworkProfile` resolution |
| User action only | `icmp_sweep` |

Probes run concurrently on a Tokio runtime with per-probe timeouts. **A failing or
permission-denied probe degrades one row of the Visibility Panel — it never blocks
the UI, and it never produces a silent empty result.** (§12)

---

## 7. Data flow

### 7.1 Cold start — the 30-second promise, budgeted

The product goal is that a person understands their network within 30 seconds of
opening the app. That is a latency budget, and it is an acceptance criterion:

| Time | Event | What the user sees |
|---|---|---|
| 0 ms | Window paints | JRX shell, skeleton cards — never a blank window |
| ~150 ms | `interfaces` + `routes` return | **"You are on Wi-Fi"** — connection type resolved |
| ~400 ms | `wifi` returns, `NetworkProfile` resolved | Network name, band, signal, router address |
| ~600 ms | Capability matrix computed | **Visibility Panel populated** — what we can and cannot see, here, now |
| ~800 ms | `arp` returns | First device nodes appear in the topology |
| 1–8 s | mDNS/SSDP responses stream in | Nodes gain names, vendors, categories; ring populates |
| ~8 s | Discovery quiesces | Topology settled; counts stable |
| 8–30 s | User reads | Understanding, unaided |

Everything after the 400 ms mark is **progressive enrichment**. Nodes appear
immediately as anonymous entries and gain identity as evidence arrives. The UI
never waits for a complete picture before showing a partial one — but it always
labels a partial picture as partial.

### 7.2 Network identity resolution

On every network change the collector resolves a `NetworkProfile`:

- **Connection type** — the *physical* link, from interface link type and the
  routing table: `Wifi` · `Ethernet` · `UsbTether` · `Unknown`.
  A tunnel is **not** a connection type. When one carries the default route it
  is reported alongside the physical link, which is resolved from the routing
  table: the physical interface keeps routes of its own while the tunnel holds
  only the default (M4.5).
- **Hotspot detection** — heuristic, and labeled as a heuristic in the UI:
  the OS metered-connection flag, carrier OUI on the gateway MAC, characteristic
  hotspot subnets, and cellular-class interface naming.
- **Network posture** — `Home` · `Public/Guest` · `Enterprise` · `Isolated`,
  inferred from client isolation, captive-portal presence, domain membership, and
  subnet size. **Posture gates the active probe (§6.4).**
- **Fingerprint** — a local hash of gateway MAC + subnet + BSSID. This is what
  lets the app say "you have been here before" without any account or server.
  It never leaves the device.

**A tunnel holding the default route is reported as a route, not as the
connection.** Replacing "Wi-Fi / Home / 192.168.1.14" with "VPN / utun6" loses
the thing the user asked about. The physical link, its address, its subnet and
its gateway all continue to be reported; the tunnel is an additional fact.

---

## 8. Domain model

### 8.1 Device identity

Identity must survive DHCP lease changes without merging two distinct devices.
Resolution is strictly ordered, first match wins:

1. **Globally-unique MAC** — universally administered, from the ARP/NDP table
2. **UPnP UUID** — stable across reboots where advertised
3. **mDNS instance name** — stable in practice for named devices
4. **Ephemeral** — IP + first-seen, **explicitly flagged unstable in the UI**

**Randomized MAC detection is a first-class feature.** When the locally-administered
bit is set in the first octet, the device is labeled *"Randomized MAC — this device
is protecting its identity."* No vendor is inferred, and the device is not
correlated across networks.

This is deliberate. Modern phones randomize their MAC per SSID, so the devices
users most want to identify are the hardest to identify. Explaining that honestly
is more valuable than a confident wrong guess — and it demonstrates that JRX
understands privacy from both sides of the lens.

### 8.2 Signals

Every observation is a `Signal { device_ref, kind, value, source_probe, observed_at,
weight }`. Signals are append-only evidence. `Device` records are *derived* from
signals, so classification is always re-derivable and always explainable — the
Device Inspector shows the exact evidence behind every conclusion.

### 8.3 Classification — evidence-based, Unknown by default

Five categories, exactly as approved:

**Computers** · **Phones** · **Smart home** · **Infrastructure** · **Unknown**

Evidence, strongest first:

| Category | High confidence | Medium confidence |
|---|---|---|
| Infrastructure | Holds the default route; BSSID matches the associated AP | DHCP/DNS server role; mesh or NAS service advertisement |
| Computers | `_smb._tcp`, `_ssh._tcp`, `_rfb._tcp`, `_workstation._tcp` | Desktop-class OUI **plus** a corroborating service |
| Phones | `_apple-mobdev2._tcp`; iOS lockdown port 62078 | Mobile-class OUI **plus** a corroborating signal |
| Smart home | `_hap._tcp`, `_matterc._udp`, `_googlecast._tcp`, `_airplay._tcp`, `_ipp._tcp`, `_printer._tcp` | UPnP device type of renderer/printer/camera class |
| Unknown | — | *the default* |

**The over-classification rule, non-negotiable:**

> A device is assigned a category only on **High** confidence, or on **Medium**
> confidence supported by **at least one non-OUI signal**. Otherwise it stays
> **Unknown**.

Corollary: **an OUI vendor match alone never classifies a device.** "Apple, Inc."
does not distinguish a MacBook from an iPhone from an Apple TV. The vendor is
displayed as an observed fact; the category remains Unknown.

Unknown is presented as a legitimate, informative outcome — *"12 devices seen ·
3 not identifiable"* — never as an error or a failure state. Guessing is worse
than admitting.

### 8.4 Traffic model

What is available without elevation, and what is not, stated precisely:

- **Available:** per-interface throughput from counter deltas (accurate); the set
  of active remote endpoints with owning process; and offline network-owner
  attribution of those endpoints from bundled published ranges. Not reverse
  DNS and not ASN — both are refused (ADR-019).
- **Not available:** any historical per-destination volume beyond what the OS
  keeps, and any view of another device's traffic.
- **Corrected in M5:** this section previously claimed per-application
  bandwidth requires administrator access. On macOS that is false.
  `/usr/bin/nettop` reports bytes per socket with the owning process, system
  wide, unprivileged. Measured, not assumed — see
  [docs/M5_PHASE0_FEASIBILITY.md](docs/M5_PHASE0_FEASIBILITY.md). The Windows
  equivalent remains unproven.

### `nettop` is a tool, not an API

This matters enough to state plainly.

**JRX does not link Apple's private `NetworkStatistics` framework.** The macOS
provider runs the shipped `/usr/bin/nettop` executable and parses its output.
Linking the framework directly would be a private API: undocumented, free to
change between OS versions, and grounds for App Store rejection.

The consequence is that the output format is not contractual, so:

- All parsing is isolated behind `InterfaceActivityProvider` /
  `ProcessConnectionProvider`. Nothing else in JRX reads `nettop`.
- A format change degrades rather than propagates. Output that parses to
  nothing is treated as unreadable, never as "no connections" — a fabricated
  zero is worse than an admitted gap.
- **Interface-level activity keeps working when the adapter breaks.** The two
  providers are separate precisely because they fail separately.

`nettop` is spawned once per sample and reaped immediately. Its continuous
logging mode would give better latency but sustains 128% CPU, which is not
something to leave running in the background (TECH_DECISIONS.md ADR-020). The
tool's slow first call after boot is paid by a warm-up off the critical path.

What genuinely does not exist at this privilege level is the *identity* of the
far end. Reverse DNS resolved nothing for 12 of 12 live endpoints, and the only
reliable IP-to-domain source is DNS query observation, which JRX refuses by
design. The traffic view therefore shows which programs are talking, how much,
and to whose network — and states plainly that it does not know which sites.

---

## 9. The capability model — the Visibility Panel's engine

The Visibility Panel is a **core feature, not a settings screen.** It is permanently
visible in the primary layout and is populated before the topology renders.

It is generated, not hand-written. Every probe's declared `requires()` and `reads()`
are combined with live permission state to produce a matrix in four states:

| State | Meaning | Example |
|---|---|---|
| **Observed** | Working now, with the mechanism named | "Device names — via mDNS service discovery" |
| **Available** | One permission grant away, with the grant action inline | "Network name — needs Location Services on macOS" |
| **Not possible** | Platform-blocked at this privilege level | "Per-app bandwidth — requires admin; JRX does not ask for it" |
| **Refused by design** | Technically possible, deliberately not built | "Packet contents · DNS queries · credentials" |

The fourth column is the product's thesis. It states what JRX *could* build and
will not — which is the difference between a scanner and a trustworthy instrument.

Because the matrix is generated from the same declarations the collector enforces,
it cannot drift from the truth. It is also how the mobile story stays honest: on
iOS the panel is mostly red, and **showing that clearly is the product working, not
failing** (§17).

---

## 10. Host ↔ WebView interface

**Commands** (renderer → host), the complete allowlist:

```
get_network_identity()  ·  get_capabilities()  ·  get_devices()
get_device_detail(id)   ·  get_throughput()    ·  get_endpoints()
run_active_sweep(consent_token)                ·  cancel_active_sweep()
request_permission(kind)·  get_history_summary()·  clear_all_data()
```

**Events** (host → renderer):

```
network.changed  ·  capability.changed  ·  device.discovered
device.updated   ·  throughput.tick     ·  scan.progress  ·  probe.degraded
```

Discovery is push-based. `device.discovered` and `device.updated` stream as
evidence arrives, which is what produces the progressive reveal in §7.1 without
any polling.

`run_active_sweep` requires a consent token issued by the host only after the
disclosure dialog is acknowledged. The renderer cannot fabricate one.

---

## 11. Storage

SQLite via `rusqlite` (bundled, WAL), in the OS application-data directory. One
file. No cloud, no sync, no export in v1.

```
networks         id, fingerprint, ssid, bssid, gateway_mac, conn_type,
                 posture, first_seen, last_seen
devices          id, identity_kind, identity_value, vendor, display_name,
                 category, confidence, is_randomized_mac, first_seen, last_seen
device_signals   device_id, kind, value, source_probe, observed_at, weight
device_networks  device_id, network_id, first_seen, last_seen
iface_samples    iface, ts, rx_bytes, tx_bytes
```

**Retention:** `iface_samples` and `device_signals` roll off at 7 days by default,
enforced by a startup sweep. `clear_all_data()` deletes the database file and
restarts collection from empty — a real, immediate, verifiable erase.

**Deliberately absent tables:** connection history, DNS queries, per-destination
volumes, packet data, process command lines. Storage schema is itself a privacy
statement, and the schema is short enough for a skeptical user to read.

Persistence exists to answer exactly two questions: *"is this device new here?"*
and *"what has throughput looked like recently?"* Anything beyond that is scope.

---

## 12. Degraded and empty states

An observability tool that shows nothing without explaining why is broken. Each of
these is a **designed state with specific copy**, not a fallback:

| Condition | Presentation |
|---|---|
| macOS Local Network permission denied | Panel explains mDNS/SSDP are blocked, what is lost, and offers the grant action |
| macOS Location Services denied | Network name unavailable; everything else continues; grant offered inline |
| Client isolation detected | *"This network isolates its clients — you are intentionally prevented from seeing other devices. This is the network working correctly."* |
| VPN holds the default route | Local visibility reduced; explained as a consequence of the VPN, not an error |
| Ethernet with no WiFi hardware | WiFi rows shown as Not applicable, not as failures |
| Zero devices found | Four different messages, chosen by the quality model: our probe was refused; devices exist but are not announcing; the network has room for many hosts and only the router answered; the network is genuinely small. Isolation is assessed against the subnet size, so a hotspot's /28 is never called client isolation (M4.5) |
| A probe refused by the OS | `SourceStatus::Refused` is distinct from `Failed`. macOS reports a denied Local Network permission as `EHOSTUNREACH`, the same errno as a routing fault; JRX names it as a permission problem and points at the setting (M4.5) |

**The failure mode this section exists to prevent:** a demo on guest WiFi that shows
an empty circle with no explanation. On any network, JRX must be able to explain
itself.

---

## 13. UI architecture

**Stack:** React + TypeScript + Vite. Local state store; no data fetching library
(all data arrives via Tauri events). All assets bundled — no remote fonts, no CDN.

**Primary layout, in the approved priority order:**

1. **Network Identity** — the immediate answer: connection type, network name,
   router, posture
2. **Visibility Panel** — what can and cannot be seen, here, now
3. **Device Discovery** — live count with category breakdown, Unknown shown plainly
4. **Radial Topology** — the centerpiece

### 13.1 Radial topology

- **Router at center**, always — it is the one device identified with certainty.
- **This device highlighted** — visually distinct, immediately locatable.
- **Devices arranged in category arcs** around the ring: Computers · Phones ·
  Smart home · Infrastructure · Unknown, each arc a fixed angular sector so
  positions stay stable between renders.
- **Rendered as SVG.** Home networks hold tens of nodes, not thousands. SVG gives
  stable hit-testing, real focus management, and DOM-addressable nodes for
  accessibility — all of which Canvas would cost us for performance we do not need.
- **Staged reveal on first scan:** center → this device → ring skeleton → nodes
  fading in as discovered. The reveal is driven by real discovery events, so the
  animation *is* the data arriving. It is not a loading simulation.
- **Reduced motion:** `prefers-reduced-motion` renders the settled state directly.
- **Accessibility:** a parallel, fully keyboard-navigable device list is a
  first-class view, not a fallback. The topology carries an accessible name and
  summary. Category is never conveyed by color alone — shape and label carry it too.

Unknown devices render with a deliberately neutral treatment: present, counted,
clearly not identified, and never dressed up with a speculative icon.

---

## 14. Security and privacy model

**Egress inventory** — the complete list of what leaves the machine:

1. mDNS queries to `224.0.0.251:5353` / `ff02::fb` — LAN multicast only
2. SSDP `M-SEARCH` to `239.255.255.250:1900` — LAN multicast only
3. ICMP echo to the local subnet — **only** on explicit per-invocation consent
4. Reverse DNS (PTR) for observed endpoints, via the system resolver
5. *Optional, off by default:* public-IP lookup. This contacts a third party, so
   it is a one-tap action with the endpoint named in the UI, never automatic.

There is no sixth item. No analytics, no crash reporting, no update ping, no
license check.

**Enforcement:** Tauri CSP denies remote origins; the dependency audit denies
capture libraries; the probe declaration audit denies undeclared reads. CI runs all
three.

**Threat model.** In scope: accidental over-collection by our own code; data
exfiltration through a compromised renderer or a dependency; over-broad privilege.
Out of scope for v1: a fully compromised host; an actively hostile LAN spoofing
mDNS and ARP responses (a spoofed device would appear as a device — this is a known,
documented limitation of unprivileged passive discovery, not a defect).

---

## 15. Evolution path — how the daemon arrives later

`collector` is a standalone crate exposing an async API and importing no Tauri
symbols. Today the app calls it in-process. When background collection or an
elevated probe is genuinely justified:

1. Wrap the **same crate** in a thin daemon binary (launchd agent / Windows service).
2. Replace the in-process call site with a local IPC transport (Unix socket /
   named pipe, JSON-RPC framing).
3. `core` and the entire UI are untouched.

The boundary is designed now so the option stays open. It is deliberately not
exercised in v1 — building the daemon before there is a probe that needs it would
be cost with no return.

Likewise, the packaging boundary is kept clean so code signing, notarization,
auto-update, and licensing can be added at the installer layer without touching
application code (see MVP_ROADMAP.md §Post-MVP).

---

## 16. The `metrics/` directory

The scaffold originally contained a `telemetry/` directory. It has been renamed to
`metrics/`. In a privacy-first security product "telemetry" reads as phone-home, and
it is among the first words a skeptical user or reviewer will search for. A directory
name is part of the trust surface, and this one did not describe what the code does.

**`metrics/` contains only local development and performance measurements** — probe
timings, cold-start latency budgets (§7.1), and rendering benchmarks used during
development.

**No user data, network inventory, device information, or telemetry of any kind
leaves the device.** JRX has no backend, no analytics, no crash reporting, no update
ping, and no license check. The complete set of what the application may send is
enumerated in §14 and contains five items, all of them LAN-local or explicitly
user-initiated.

See [`metrics/README.md`](metrics/README.md) and TECH_DECISIONS.md ADR-014.

---

## 17. Mobile: a companion, not a port

Mobile is explicitly out of scope for v1. When it arrives it will be a **companion
application with a different, smaller promise** — because the platforms allow a
different, smaller promise. `core` is reusable; the collection surface is not.

| Capability | macOS/Windows | Android | iOS |
|---|---|---|---|
| Own interfaces & connection type | Yes | Yes | Yes |
| Network name (SSID) | Yes (macOS needs Location) | Yes (needs Location) | Yes (needs entitlement + Location) |
| Neighbor/ARP table | Yes | **No** — blocked since Android 10 | **No** |
| mDNS / Bonjour discovery | Yes | Yes (NSD) | Yes (needs Local Network permission) |
| Own-device throughput | Yes | Yes | Yes |
| Per-app traffic | No (needs admin) | **Yes** (`NetworkStatsManager`, special permission) | No |
| Active sweep | Yes (opt-in) | Limited | **No** |

The honest framing, which the Visibility Panel produces automatically:

- **Desktop:** *"Can observe local network devices and traffic metadata."*
- **Android:** *"Can observe this device's network activity, per-app data usage, and
  Bonjour-discoverable neighbors."*
- **iOS:** *"Can observe this device's own network activity and Bonjour-discoverable
  services."*

iOS is the weakest and must never be sold as parity. A capability matrix that shows
mostly red on iOS is the product being trustworthy.

---

## 18. Testing strategy

| Layer | Approach |
|---|---|
| `core` | Pure unit tests over recorded signal fixtures. Classification, identity resolution, and the over-classification rule get dedicated adversarial cases. |
| `collector` | Probes tested against captured platform responses. Live-network tests are marked and excluded from CI. |
| Capability matrix | Snapshot tests per platform × permission-state combination, including all-denied. |
| Degraded states | Every row of §12 has a test that forces the condition and asserts the specific explanation renders. |
| Privacy invariants | CI asserts: no capture library in the graph; no undeclared `reads()`; no remote origin permitted by CSP; app runs unprivileged. |
| UI | Component tests plus visual review of the topology at 0, 1, 12, and 60 devices. |

The privacy invariants are the important ones. They are the mechanism by which
these principles survive the twentieth commit.
