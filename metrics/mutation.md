# Mutation testing — the honesty rules, checked by breaking them

JRX's value rests on its byte accounting and recognition being *true*. A test
that passes is not enough; the test has to **fail when the code is wrong**. This
note records a run of [`cargo-mutants`](https://mutants.rs) against the two
modules where a silent bug would be a lie to the user, and what it found.

> A green test that cannot fail is worse than no test. This file exists so that
> claim is auditable rather than asserted.

**Tool:** `cargo-mutants` 27.1.0 · **Date:** 2026-09-03 · **Scope:**
`core/src/history.rs` (recognition) and `core/src/activity/session.rs` (session
byte accounting). Both are pure `core` logic, so the run is deterministic and
platform-independent.

## Result

| Module | Mutants | Caught | Unviable | **Survived** |
|---|---|---|---|---|
| `core/src/history.rs` | 16 | 12 | 4 | **0** |
| `core/src/activity/session.rs` | 74 | 52 | 5 | **17** |

`history.rs` has no surviving viable mutants: every way the tool could break
recognition is caught by a test.

## What the first run caught that we had missed

The initial run surfaced real gaps in the *byte-integrity* logic — the part
where a bug would invent traffic. Tests were added to close each one, and the
mutants are now caught:

- **A partial counter reset.** When an interface counter falls in one direction
  while climbing in the other, the whole sample must be dropped, not mined for
  the direction that grew. (`a_partial_interface_reset_adds_nothing`)
- **An unchanged socket.** Seeing a socket again with identical counters is zero
  new bytes — a mutation that treated "unchanged" as a reset would have counted
  the entire counter as fresh traffic. (`re_observing_an_unchanged_socket_adds_nothing`)
- **A reused five-tuple that falls in one direction.** Only the new
  connection's own bytes count, never an underflowed subtraction turned into a
  huge number. (`a_socket_counter_that_falls_counts_only_the_new_connection`)
- **The digest itself.** A golden-value test pins the recognition hash and the
  exact string it is built from — which is a stored-data contract, since changing
  it would orphan every record already on disk. (`digests_are_stable_across_versions`)

## The 17 survivors in `session.rs`, accounted for

None of them affect a byte count shown to the user. Left in the open rather than
hidden:

- **Equivalent mutants (cannot be killed, and should not be).** `counter_delta`
  `<`→`<=` produces an identical result when the counters are equal; `per_second`
  `/`→`*` is identical at the 1-second sampling interval the product uses.
- **Display ordering only.** Mutating the `+` in the connection sort comparator
  (`render`) reorders a program's connection list; it changes no total. No test
  pins cosmetic ordering, by choice.
- **A cosmetic flag.** The `name_is_truncated` "(name shortened)" hint.
- **Memory bounding, not accounting.** The idle-sample logic that decides when a
  long-silent program is dropped from memory. It governs footprint, never a
  reported byte.

These are documented, not chased: pinning display order or an equivalent
operator would add brittle tests that assert nothing a user relies on.

## Reproducing

```sh
cargo install cargo-mutants
cargo mutants -f core/src/history.rs -f core/src/activity/session.rs
```

## Not yet swept

This run covered recognition and session accounting — the highest-stakes logic.
The other honesty-bearing modules (`collector/src/activity/macos.rs` parsing,
`core/src/device.rs` merge rules, `core/src/activity/owner.rs` network-owner
lookup, `collector/src/store.rs`) carry their own targeted honesty tests but
have not had a full mutation sweep. That is the honest state, and the next place
to point the tool.
