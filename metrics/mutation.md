# Mutation testing — the honesty rules, checked by breaking them

JRX's value rests on its byte accounting, recognition, network-owner claims, and
classification being *true*. A test that passes is not enough; the test has to
**fail when the code is wrong**. This note records runs of
[`cargo-mutants`](https://mutants.rs) against the modules where a silent bug would
be a lie to the user, and what they found.

> A green test that cannot fail is worse than no test. This file exists so that
> claim is auditable rather than asserted.

**Tool:** `cargo-mutants` 27.1.0 · **Date:** 2026-09-03 · **Scope:** the pure
honesty-bearing logic. Deterministic, platform-independent `core`/`collector`.

## Result

| Module | What it protects | Mutants | Caught | Unviable | **Survived** |
|---|---|---|---|---|---|
| `core/src/history.rs` | recognition keys & verdicts | 16 | 12 | 4 | **0** |
| `core/src/activity/owner.rs` | "network owner ≠ website", None when unknown | 41 | 38 | 1 | **2** |
| `collector/src/store.rs` | the recognition database | 97 | 2 | 95 | **0** |
| `core/src/activity/session.rs` | session byte accounting | 74 | 52 | 5 | **17** |
| `core/src/device.rs` | classification — Unknown by default | 154 | 92 | 19 | **43** |

## Gaps the tool found, and closed

The runs surfaced real holes where a bug would have gone unnoticed. Tests were
added for each and confirmed to now catch the mutant:

- **A fabricated network owner.** `network_owner` could be made to return
  `Some("xyzzy")` for any address, or `None` for a known one, and no test
  complained — the range matching had no direct tests. That left the cardinal
  rule ("an unmatched address stays unidentified, never a guess") unverified. Now
  pinned, with range-boundary membership and the local-vs-internet split. (22
  mutants in `owner.rs` closed.)
- **A partial counter reset**, **an unchanged socket**, and **a reused five-tuple
  that falls in one direction** — three ways the byte accounting could have
  invented traffic, now each caught. (`session.rs`)
- **The digest itself** — a golden-value test pins the recognition hash and the
  exact string a key is built from, a stored-data contract. (`history.rs`)

## The survivors, accounted for

Left in the open rather than hidden. **None of them lets JRX claim more than it
knows** — no fabricated byte, owner, verdict, or over-confident classification.

- `history.rs` (0), `store.rs` (0) — no surviving viable mutants.
- `owner.rs` (2) — both an unreachable path-boundary guard in `application_bundle`.
  Equivalent mutants.
- `session.rs` (17) — equivalent operators, connection **display ordering**, the
  cosmetic `name_is_truncated` hint, and the idle logic that bounds **memory**,
  never a reported total.
- `device.rs` (43) — the important part: **every classification survivor pushes
  toward *less* classification, not more.** The `definitive_service_category` /
  `_upnp_category` / `_family` mutants turn an `||` chain of "this signal means
  this category" into an always-false `&&`, so the code would return *Unknown*
  more often — the safe direction, and the exact opposite of the over-classify
  bug the project defines itself against. The rest are `label()` display strings,
  which evidence is *shown* as supporting a conclusion (not the conclusion), and
  `DeviceTable` merge/dedup bookkeeping. The cardinal guard — that JRX does **not**
  invent a device type — holds: those mutations are caught. What is uncovered is
  *recall* (which specific service string maps to which category) and display,
  not honesty. Pinning every service-string mapping would be a large, brittle
  suite asserting behaviour no user relies on, so it is documented rather than
  chased.

## Reproducing

```sh
cargo install cargo-mutants
cargo mutants -f core/src/history.rs -f core/src/activity/owner.rs \
              -f core/src/activity/session.rs -f collector/src/store.rs \
              -f core/src/device.rs
```

## Still to sweep

The `nettop` parser (`collector/src/activity/nettop.rs`) and the macOS OS
adapters carry targeted tests but have not had a full mutation sweep. The
honesty-defining logic — recognition, accounting, owner attribution, and the
"don't over-classify" guard — has, and holds.
