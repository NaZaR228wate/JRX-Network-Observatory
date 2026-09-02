# M5 — My Device + Network Activity — close-out report

**Branch:** `claude/m5-my-device-activity` · **HEAD:** `6f3e9ff` · **Date:** 2026-09-03
**Platform validated on:** macOS 26.5.2 (build 25F84), Apple Silicon (arm64),
Rust 1.98.0, Node 24.18.0.

This is the §49 report the milestone requires. It was written *after* validating
the existing implementation end-to-end on the real Mac, not from the code alone.
The M5 code was already present on this branch (commits M5(1)–M5(4), all
2026-08-26); this pass validated it, corrected documentation that no longer
matched it, and fixed two real defects. **M5 was not rewritten.**

Every claim below is tagged:

- **[PHYSICAL]** — observed on this Mac, in the bundled app or via the real tools.
- **[FIXTURE]** — exercised through deterministic fixtures / unit tests only.
- **[UNVERIFIED]** — not checked this session; stated as a gap, not a pass.

---

## A. Branch and HEAD

- Current branch `claude/m5-my-device-activity`, HEAD `6f3e9ff`
  ("M5(4): reverse the streaming decision — it cost a whole core").
- Working tree was clean on arrival. It now carries the six edits from section G
  plus this report, all uncommitted, for review.
- `main` is still at `e84addf` (M4). Neither M4.5 nor M5 has been merged.
- Commits on the branch, oldest first: `3e03339` M5(1) provider + session
  accounting + sampling loop · `33ab4b4` M5(2) the Activity screen · `808aba8`
  M5(3) scale, mutation coverage, the attribution gap · `6f3e9ff` M5(4) revert
  the streaming `nettop` decision.

**Discrepancy vs. the handoff:** the handoff document described M5 as *not yet
implemented* ("M5 Phase 0 only proved feasibility"). The repository contradicts
that: full M5 is implemented on this branch. Trusting the repository, the real
next step was validation and close-out, not implementation.

---

## B. Architecture implemented

The dependency direction from ARCHITECTURE §4 holds: `app → collector → core`,
with no Tauri or OS calls in `core` and no Tauri in `collector`.

- **`core/src/activity/`** — pure session model (`session.rs`, 703 lines),
  network-owner lookup over bundled ranges (`owner.rs`), typed models (`mod.rs`).
  No OS, no Tauri. All accounting rules live here and are unit-tested offline.
- **`collector/src/activity/`** — the OS adapter: `provider.rs` defines two
  traits, `macos.rs` implements them, `nettop.rs` parses `nettop` output,
  `monitor.rs` owns one sampling loop. `nettop` is reached only here.
- **`app/src-tauri/src/lib.rs`** — `start_activity` / `stop_activity` commands,
  one background sampling thread, snapshots emitted on `activity://snapshot`.
- **`app/src/activity/`** — `Activity.tsx` renders typed snapshots; `rank.ts`
  sorts/searches. The renderer starts no samples and computes no deltas.

**[PHYSICAL]** The two-provider split is real and matters: `InterfaceActivity`
(from `netstat -ib`) and `ProcessConnection` (from `nettop`) are separate boxed
trait objects, so the richer one can fail without taking throughput down.

---

## C. Provider design

- **Interface throughput:** `NetstatInterfaceProvider` reads `netstat -ib`
  counters; deltas become rates. Available on the first frame. **[PHYSICAL]**
- **Per-program connections:** `NettopConnectionProvider` runs
  `nettop -x -L 1 -J state,bytes_in,bytes_out` once per sample and parses the
  flat process/socket listing; ownership comes from row position, never a guess.
  **[PHYSICAL]**
- **Process identity:** PID paired with the executable path resolved via
  `proc_pidpath` (recovers names `nettop` truncates at 15 chars). A reused PID is
  treated as a new program. **[PHYSICAL]** — confirmed live: `Claude Helper`
  inside `/Applications/Claude.app/…` displayed as **Claude**; the WebKit XPC
  service `com.apple.WebKit.Networking` displayed as *itself*, attributed to
  nothing, because it is shared by every WebKit client.
- **Network owner:** offline lookup against bundled published ranges. **[PHYSICAL]**
  — live, `34.149.66.163` resolved to **Google Cloud** with the on-screen caveat
  "this address belongs to their published range. It is not the site you
  visited"; most endpoints correctly showed "Network owner unavailable".
- **Deliberately absent:** reverse DNS, ASN, IP-to-domain, service names
  (ADR-019). The `rtt_ms` field exists in the model but is currently always
  `None` — see section O.

`nettop` is treated as a tool, not an API: JRX shells out to the shipped binary
and never links `NetworkStatistics.framework` (ADR-018). Parse failure degrades
to "unavailable", never to a fabricated zero.

---

## D. Real-Mac validation (the full suite)

Run natively on the Mac through the Tauri CLI (not bare `cargo` — README).

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | **[PHYSICAL]** clean (exit 0) |
| `cargo clippy --workspace --all-targets -- -D warnings` | **[PHYSICAL]** clean |
| `cargo clippy … --features fixtures -- -D warnings` | **[PHYSICAL]** clean |
| `cargo test --workspace --all-targets` | **[PHYSICAL]** 323 passed, 0 failed |
| `cargo test … --features fixtures` | **[PHYSICAL]** 337 passed, 0 failed |
| Frontend `tsc --noEmit` + `vite build` | **[PHYSICAL]** clean |
| Frontend `vitest run` | **[PHYSICAL]** 54 passed |
| `tauri build` (dmg) | **[PHYSICAL]** exit 0 → `JRX Observatory_0.1.0_aarch64.dmg` (3.7 MB) |
| `tauri build --bundles app` | **[PHYSICAL]** exit 0 → `JRX Observatory.app`, id `app.jrx.observatory`, adhoc-signed |
| Release-with-fixtures compile guard (ADR-017) | **[PHYSICAL]** fails to compile as designed (`compile_error!`) |

The 337-vs-323 difference is the 14 `fixture_scenarios` tests, which only compile
under the `fixtures` feature.

> Note on tooling: the plain `npm run build:app` inside CI-style `CI=1` fails with
> `invalid value '1' for '--ci'` — the Tauri CLI parses `CI` as a boolean. This is
> a script-environment issue, not a build failure; every build above succeeded
> with `CI` unset. Worth pinning `CI=true` in `.github/workflows/ci.yml` before CI
> ever runs (it never has — there is no git remote).

---

## E. Activity overview result (the real screen)

**[PHYSICAL]** Launched the bundled `JRX Observatory.app` and drove the real
Activity screen (not a browser preview). Observed:

- **This Mac — live**, interface `en0`, download/upload rates updating each second
  (e.g. ↓ 10 KB/s ↑ 25 KB/s, later ↓ 5 KB/s ↑ 671 B/s).
- **Observed this session** shown *distinctly* from the interface's lifetime
  totals — e.g. session ↓ 2.4 MB ↑ 655 KB, with the wording "Only what JRX has
  watched move since you opened it — not what this Mac has sent all along."
- **Active connections:** ~53, updating.
- **Programs active now**, ranked by session total: Claude (↓ 2.2 MB ↑ 416 KB,
  20 connections), ChatGPT, Discord, Telegram, Steam Helper, Spotify, steam_osx,
  launchd, apsd, webprivacyd, com.apple.WebKit.Networking — each with its own
  session bytes and connection count.
- **What JRX can see / cannot see** panel and the four-state capability summary
  (6 working now · 4 need permission · 4 not possible here · 5 refused by design).

This confirms the **event/data path end-to-end**: the frontend `invoke`
registered listeners, `start_activity` ran, and typed `activity://snapshot`
events reached the renderer and updated live. (The M4.5 lesson — a browser
preview does not prove the Tauri ACL/event path — was respected; this was the
bundled app.)

---

## F. Redacted real process example

**[PHYSICAL]** From the live **Claude** detail panel:

```
Claude
/Applications/Claude.app/Contents/Frameworks/Claude Helper.app/…/Claude Helper · PID 1037
Observed this session   ↓ 4.6 MB ↑ 823 KB      Current rate  ↓ 441 B/s ↑ 143 B/s
Connections 20

Destinations
  160.79.104.10:443   UDP                     ↓ 4.6 MB ↑ 332 KB
    Network owner unavailable
  160.79.104.10:443   TCP  ESTABLISHED        ↓ 2 KB ↑ 414 KB
    Network owner unavailable
  34.149.66.163:443   TCP  ESTABLISHED        ↓ 1 KB ↑ 39 KB
    Network owner: Google Cloud — this address belongs to their published
    range. It is not the site you visited.
```

Note what is present (address, port, protocol, TCP state, session bytes, owner
where defensible) and what is absent (no hostname, no domain, no service name).

---

## G. Documentation and defects corrected

Only what validation proved wrong. No M5 logic was changed.

**Documentation drift (docs contradicted the shipped code):**

1. **TECH_DECISIONS.md** — the ADR summary table still read "020 · One
   long-lived `nettop`, not one per sample", the exact opposite of the rewritten
   ADR-020 body. Corrected to "one `nettop` per sample, warmed once".
2. **ARCHITECTURE.md §8.4** — the "Available" bullet still listed "reverse-DNS
   and bundled-ASN attribution", both of which ADR-019 refuses. Rewritten to
   "offline network-owner attribution … Not reverse DNS and not ASN (ADR-019)".
3. **MVP_ROADMAP.md** — the M5 section was still the pre-Phase-0 plan (reverse
   DNS/ASN, and per-application bandwidth marked "**impossible** under ADR-002").
   Phase 0 disproved that. Rewrote the M5 section to what shipped, marked it
   complete in the schedule, and struck the out-of-scope "per-application
   bandwidth" row.
4. **PRODUCT_BOUNDARIES.md** — the "Shown" table listed "…state, RTT", but RTT is
   never on screen (see O). Removed RTT from the shown list.

**Real defects found by validation:**

5. **collector/src/identity.rs** — `WifiStatus` was imported unconditionally but
   used only on macOS, so `cargo clippy -D warnings` **fails on any non-macOS
   target**, which would break CI's `windows-latest` leg the first time CI runs.
   Gated the import to `cfg(target_os = "macos")`. Verified: the non-macOS clippy
   leg (Linux) went from error → clean after the fix.
6. **collector/Cargo.toml** — removed the dead `activity-spike` feature, a
   left-over from the Phase-0 spike that no code references.

All six changes re-validated on the Mac: fmt clean, clippy clean (both feature
sets, real recompile), 323 tests passing.

---

## H. Socket disappearance / session accounting

This is the substance of M5, and the honesty rules it protects.

- Session totals are accumulated from **positive deltas while a socket exists**
  and survive that socket closing — they are not a sum of currently-visible
  sockets. **[FIXTURE]** `traffic_survives_the_socket_disappearing`.
- The first sample only sets where counting begins; it claims no prior traffic.
  **[FIXTURE]** `the_first_sample_establishes_a_baseline_and_claims_no_traffic`.
- A counter that goes backwards (five-tuple reuse / reset) restarts counting
  rather than producing negative or inflated traffic. **[FIXTURE]**
  `a_reused_five_tuple_restarts_counting_instead_of_going_negative`,
  `a_counter_reset_is_not_treated_as_traffic`.
- A reused PID becomes a separate program; the executable path distinguishes
  programs before the PID does. **[FIXTURE]** two dedicated tests.
- Interface switch keeps the session but restarts counters. **[FIXTURE]**
- **[PHYSICAL]** On the live screen, "Observed this session" was consistently
  **lower** than the interface lifetime totals and moved independently of them —
  exactly the shape the model predicts. The under-reporting side was chosen on
  purpose (section O).

---

## I. Cold-start behaviour

**[PHYSICAL]** Interface throughput appeared on the first frame; `nettop`'s slow
first call was paid by `warm()` off the critical path (a separate thread), so the
screen was never blank waiting for it. Program rows filled in a couple of seconds
after launch. The `initializing` health state ("Preparing program details…")
exists for the window before per-program data is ready. No six-second blank
spinner — the failure mode Phase 0 warned about did not occur.

---

## J. Limited-provider fallback

- **[FIXTURE]** The four health states (`full`, `initializing`, `limited`,
  `no_network`) are unit-tested, and `limited` renders "JRX can measure this
  Mac's total network activity, but program-level details are currently
  unavailable" with the technical reason behind a disclosure, never as the
  headline. `repeated_failures_eventually_report_limited` passes.
- **[UNVERIFIED — PHYSICAL]** I did not force `nettop` to fail on the real Mac
  this session (it worked throughout), so the `limited` banner was not seen live.
  The fallback is proven in logic, not physically triggered.

---

## K. CPU / memory / refresh performance

**[PHYSICAL]** Measured on the running bundled app over a 60-second window at
1 Hz:

- **CPU:** 0.0–0.6 % of one core (avg ≈ 0.3 %). Matches M5(4)'s "0.2 % CPU" claim.
- **Resident memory:** 96–101 MB, stable (no growth over the window).
- **nettop cadence:** ≈ 30 distinct short-lived `nettop` PIDs across 60 s, **none
  persisting** — confirming one spawn per sample, not a long-lived streaming
  child. This is the decision M5(4) reverted to, and it holds in the shipped app.
- Children reaped: 7 transient `<defunct>` seen mid-window, **0 at the end**.

This physically confirms the ADR-020 reversal: the 128%-CPU streaming child is
gone; per-sample spawn costs ~0.3 % CPU.

---

## L. Rust / frontend test counts

- **[PHYSICAL]** Rust: **323** tests (default features) / **337** (with
  `fixtures`), 0 failures, on the Mac. Includes 8 privacy-invariant tests, 13
  activity-honesty tests, 6 activity-scale tests.
- **[PHYSICAL]** Frontend: **54** tests (vitest), 0 failures; `tsc` clean.

---

## M. Mutation testing

- **[FIXTURE]** The commits record that the four accounting rules and the four
  Phase-0 honesty rules were mutation-tested and each mutation was caught. Those
  tests are present and pass.
- **[UNVERIFIED]** I did **not** re-run a mutation tool (`cargo-mutants`) this
  session, so I have not independently confirmed that the intended mutations
  still fail. Per the project's own testing philosophy (§38: "a green test that
  cannot fail is worse than no test"), this is the one testing claim I am
  relaying rather than re-verifying. Recommend a `cargo-mutants` pass on
  `core/src/activity/session.rs` before M5 is considered closed with confidence.

---

## N. Privacy invariant result

**[PHYSICAL]** On the Mac:

- The 8 `privacy_invariants` tests pass: no capture library in the dependency
  graph, app runs unprivileged, CSP permits no remote origin, no undeclared reads.
- The 13 `activity_honesty` tests pass: an address never becomes a hostname,
  ownership is observed not guessed, byte counts are measured not modelled, an
  unmatched address stays unidentified, and the model carries no payload/cookie/
  url/body field.
- **[PHYSICAL]** On the live screen: no domain, hostname, website, or service
  name appeared anywhere; network owner was always framed as "not the site you
  visited". The boundary held in the running product.

---

## O. Bugs and rough edges found in the real bundled app

1. **RTT is claimed but never shown.** `rtt_ms` is hardcoded `None` and `nettop`
   is not asked for the RTT column, yet PRODUCT_BOUNDARIES (now fixed) and the
   Phase-0 doc list RTT as available/shown. **Decision needed:** either request
   the column and show it, or drop the field. Not a correctness bug — nothing
   fabricates RTT — but a promise the UI does not keep.
2. **Windows CI would have failed on first run** — the `WifiStatus` unused-import
   clippy error (fixed, section G). Found only because validation ran clippy on a
   non-macOS target; the Mac never sees it. CI has never actually run (no remote),
   so it was latent.
3. **One app appears as several rows.** "ChatGPT", "Spotify", and "Steam Helper"
   each showed up as two rows (distinct PIDs / helper processes). This is
   *correct and honest* — JRX refuses to merge distinct processes — but it reads
   oddly to a lay user. A UX decision (group by application, or annotate helpers),
   not a defect.
4. **No throughput sparkline.** The roadmap's M5 wording mentioned a "throughput
   strip with sparkline"; the shipped screen shows live rate numbers, no
   sparkline. Minor scope note, not a regression.

No new correctness or event-path bugs were found in the bundled app this session.

---

## P. Remaining weaknesses (honest list)

- **The attribution gap is real and can be large.** A connection that opens and
  closes between two 1 Hz samples is counted by the interface but attributable to
  no program (Phase 0: only ~9 KB of 177 KB attributed over 12 s of short
  fetches). The on-screen note is honest about it, but the gap is inherent to
  1 Hz sampling with no socket age from `nettop`. **[PHYSICAL]** confirmed
  live only as "session < interface"; the large-gap banner itself was not
  triggered in my session (steady traffic).
- **`limited` state and the large-gap banner were not physically triggered.**
  Both are logic/fixture-proven; neither was seen on a real degraded network.
- **Mutation testing not re-run** (section M).
- **RTT decision outstanding** (section O.1).
- **CI has never executed** — no git remote exists; the `CI=1`/`--ci` gotcha and
  the Windows clippy leg would both have bitten on first run.
- **Windows provider is unimplemented** — expected (M7), but it means the whole
  activity path is macOS-only today.

---

## The M5 question

> **Would a normal Mac user find this Activity screen useful even though JRX
> deliberately does not show websites/domains?**

## YES.

Watching the real screen, a non-expert can answer, without jargon: how much this
Mac is moving right now, how much it has moved since JRX opened, which programs
are responsible and how much each, how many connections each holds, and which
*network* (not site) the traffic goes to when that is defensibly known. That is a
genuinely useful and genuinely honest picture, and the "network owner ≠ website"
framing is a feature, not an apology.

The caveats are real and I am not softening them: the attribution gap can make
program totals visibly fall short of the interface total, and one app showing as
several rows looks odd. Both are honesty costs, not dishonesty — and neither
stops the screen from answering the questions a user actually has. **YES, with
those rough edges worth polishing.**

---

## Recommendation for what comes after M5 (not started)

A short **M5.1 polish** before M6, because M5 shipped with three things
physically unexercised: resolve the RTT decision (show it or remove the field);
decide how to present the duplicate-application rows; and physically trigger and
verify the `limited` and large-gap states on a real degraded/isolated network.
Add a `cargo-mutants` run on the session model, and fix the CI environment
(`CI=true`, and confirm the Windows clippy leg is now green) so CI is meaningful
the moment a remote exists.

Then **M6 — Persistence and Polish** as the roadmap has it: SQLite with short
retention, known-vs-new device recognition, network recognition, and a real
`clear_all_data()`.

**M6 was not started, per instruction.**
