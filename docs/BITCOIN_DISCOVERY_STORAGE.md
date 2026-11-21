# Protected application of discovery counters — 2026-11-02

`PublicStateStore::apply_discovery(&result)` explicitly applies a complete
`CoreTestnet4::discover` result to protected public testnet4 state. The operation
requires `core-rpc`; it makes no network request. The default build and desktop
demo remain offline, with no seed storage, signing or automatic refresh.

## Snapshot, discover, apply

1. Open the protected store, retaining its existing exclusive process lock.
2. Call `store.public_account()` for an owned watch-only account/counter snapshot.
3. Pass that snapshot to `core.discover(&snapshot, limits)` with explicit disclosure
   consent and work limits. Incomplete/error results contain no recommendation.
4. Explicitly pass the successful result to `store.apply_discovery(&result)`.
5. Use the store's reservation methods to issue addresses after saving succeeds.

Changing a detached account snapshot in memory does not save it. Account keys and
counters reveal address linkage; neither snapshots nor observations belong in logs.
There is still no generic public save/reset/account-replacement method.

`Discovery` now has private fields and read-only `anchor_height()`, `anchor_hash()`,
`receive()` and `change()` accessors. This changes the earlier pre-alpha Rust field
access syntax, not the disk format. Successful discovery binds an internal proposed
account to the exact key/origin used for the complete two-branch scan. Safe external
callers cannot construct the result or replace its recommendations. No Debug,
serialization, mutable accessor or persisted discovery-token format is added.
This is an API boundary, not cryptographic attestation of the node or process.

## Full identity, upward-only merging

Application reloads and authenticates the current file/OS anchor while holding the
store lock. It compares the origin fingerprint **and the complete extended public
key**, including curve key, chain code, parent fingerprint, depth and child number.
A matching 32-bit fingerprint alone is insufficient. The typed account/codec fixes
the network and account format to testnet4 BIP84 account 0.

Mismatches return opaque `StorageError::AccountMismatch` without applying the
recommendation. Loading can first finish recovery of an earlier interrupted commit;
that is distinct from accepting a mismatched result.

For a matching account, independently merge each branch:

```text
new_receive = max(current_receive, recommended_receive)
new_change  = max(current_change,  recommended_change)
```

For example, discovery recommends `(4, 6)`, then later receive reservations advance
the store to `(47, 0)`. Application saves `(47, 6)`. It does not replace 47 with 4.
Exhausted counters stay exhausted, and no addition/wrapping is used during merging.

Both branches are encrypted and committed together through the existing
[storage protocol](PUBLIC_STATE_STORAGE.md). On a healthy store, unchanged/older
results return success without another ciphertext, nonce, rename or credential
write. Loading may still finalize a previously interrupted commit. Reapplication
is safe and does not imply another discovery pass happened.

## Errors, interrupted writes and scope

OS-service failure, unsafe paths and file-only rollback retain the existing errors;
they are never treated as empty state or permission to reset counters. The existing
pending-file/OS-marker protocol, locking and file checks are reused unchanged.

An error can occur before or after the atomic rename. Reopening/reloading recovers
either both prior counters or both new counters, never one from each. Callers must
not assume an error means the file stayed unchanged. Retry after recovery uses the
same upward merge. Flush/read-back and injected failures do not prove physical
power-loss durability or packaged-app credential lifecycle behavior.

Application persists only the existing account/origin and next-address counters.
It does not cache scan anchors, examined ranges, last-used indices, balances,
transactions, mempool activity or a claim of current complete history. No schema
migration is needed. Store creation, path identity, credentials and seed boundaries
are unchanged.

An older successful result may be applied after later reservations, tip growth or
a subsequent reorg; there is no fresh network/anchor check here. Upward-only indices
remain conservative even when old usage disappears. This may skip addresses, and
the old scan may not cover the newly issued prefix/gap. Scan again to assess that
scope; applying counters is not a freshness/completeness assertion. A consistently
lying local node, coordinated file/OS-anchor rollback and OS compromise remain
documented trust limits. UI disclosure consent still requires a later integration.

## Verification and audit cost

Six new tests exercise successful save/reopen/address resumption, stale-result
merging beyond the old examined prefix, no-write reapplication, full identity checks
(including simulated fingerprint collision and altered chain/parent metadata),
both sides of rename/finalization failure, unavailable credentials, replayed files
and exhausted counters. They obtain complete results through the public discovery
API against bounded synthetic loopback history, not forged production constructors.

The native Keychain/Secret Service smoke test now also applies and reopens those
recommendations with `core-rpc` enabled. This combines actual OS credentials with
synthetic discovery, not mined end-to-end recovery history. Existing isolated Core
checks remain separate, and physical power-loss/release gates remain open.

The increment adds **54 net production Rust physical lines**, including comments
and spacing: 24 in discovery for the private binding/read-only accessors, and 30 in
storage for snapshot export and atomic application. Total production Rust is
**2,341 lines** under the prior counting method. Test-only helpers/wiring are counted
separately. No dependency, feature, version or lockfile changes are introduced;
the graph remains 473 lock entries, with 16 production direct dependency names.

Local verification passed 82 regular backend Rust tests (six new), 25 default Rust
tests, four Python checks, the extended native Keychain smoke test, six isolated
Core checks, formatting, strict Clippy and the existing dependency policy. CI runs
the native discovery-application smoke test on both macOS and Linux.

The first macOS CI attempt was cancelled before checkout by GitHub's scheduled
[macOS 14 retirement brownout](https://github.com/actions/runner-images/issues/13518).
The follow-up CI checkpoint selects the supported `macos-15` ARM runner. This
changes the verification host, not the wallet format or a declared minimum app OS.

The following module implements [one bounded watch-only refresh](BITCOIN_REFRESH.md)
joining protected state, discovery and balance observations before desktop integration.
Typed failures, explicit query consent and signing/seed-entry gates remain in force.
