# Mutation testing — the honesty rules, checked by breaking them

JRX's value rests on its byte accounting, recognition, and network-owner claims
being *true*. A test that passes is not enough; the test has to **fail when the
code is wrong**. This note records runs of [`cargo-mutants`](https://mutants.rs)
against the modules where a silent bug would be a lie to the user, and what they
found.

> A green test that cannot fail is worse than no test. This file exists so that
> claim is auditable rather than asserted.

**Tool:** `cargo-mutants` 27.1.0 · **Date:** 2026-09-03 · **Scope:** the pure
honesty-bearing logic — recognition, session byte accounting, network-owner
lookup, and the recognition store. Pure `core`/`collector` logic, so the runs are
deterministic and platform-independent.

## Result

| Module | What it protects | Mutants | Caught | Unviable | **Survived** |
|---|---|---|---|---|---|
| `core/src/history.rs` | recognition keys & verdicts | 16 | 12 | 4 | **0** |
| `core/src/activity/owner.rs` | "network owner ≠ website", None when unknown | 41 | 38 | 1 | **2** |
| `collector/src/store.rs` | the recognition database | 97 | 2 | 95 | **0** |
| `core/src/activity/session.rs` | session byte accounting | 74 | 52 | 5 | **17** |

`history.rs` and `store.rs` have no surviving viable mutants: every way the tool
could break them is caught by a test.

## Gaps the tool found, and closed

The runs surfaced real holes where a bug would have gone unnoticed. Tests were
added for each and confirmed to now catch the mutant:

- **A fabricated network owner.** `network_owner` could be made to return
  `Some("xyzzy")` for any address and no test complained — there was no direct
  test that a known range names its owner, or that an unmatched address returns
  `None`. That second one is the cardinal rule ("an unidentified address stays an
  address"). Both are now pinned, along with range-boundary membership and the
  local-vs-internet split. (22 mutants in `owner.rs` closed at once.)
- **A partial counter reset.** When an interface counter falls in one direction
  while climbing in the other, the whole sample must be dropped, not mined for
  the direction that grew. (`a_partial_interface_reset_adds_nothing`)
- **An unchanged socket.** Seeing a socket again with identical counters is zero
  new bytes — a mutation that treated "unchanged" as a reset would have counted
  the entire counter as fresh traffic. (`re_observing_an_unchanged_socket_adds_nothing`)
- **A reused five-tuple that falls in one direction.** Only the new connection's
  own bytes count, never an underflowed subtraction turned huge.
  (`a_socket_counter_that_falls_counts_only_the_new_connection`)
- **The digest itself.** A golden-value test pins the recognition hash and the
  exact string a key is built from — a stored-data contract, since changing it
  would orphan every record already on disk. (`digests_are_stable_across_versions`)

## The survivors, accounted for

Left in the open rather than hidden. **None of them affects a byte, an owner, or
a verdict shown to the user.**

- `owner.rs` (2) — both in `application_bundle`'s path-boundary guard, where the
  mutated condition is unreachable (the start index can never equal or exceed the
  end for a real path). Equivalent mutants.
- `session.rs` (17) — equivalent operators (`counter_delta` `<`→`<=` is identical
  when counters are equal; `per_second` `/`→`*` is identical at the 1-second
  sampling interval used); connection **display ordering**; the cosmetic
  `name_is_truncated` hint; and the idle-sample logic that bounds **memory**, not
  any reported total. Pinning these would add brittle tests that assert nothing a
  user relies on.

## Reproducing

```sh
cargo install cargo-mutants
cargo mutants -f core/src/history.rs -f core/src/activity/owner.rs \
              -f core/src/activity/session.rs -f collector/src/store.rs
```

## Not yet swept

The device classifier (`core/src/device.rs` — the "don't over-classify, Unknown
by default" rules) and the `nettop` parser (`collector/src/activity/nettop.rs`)
carry their own targeted tests but have not had a full mutation sweep. The
classifier is the highest-value place to point the tool next; this is the honest
state.
