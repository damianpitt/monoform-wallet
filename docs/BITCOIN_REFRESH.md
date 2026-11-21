# Bounded watch-only refresh — 2026-11-21

`CoreTestnet4::refresh_watch_only(&mut store, limits)` joins protected public state,
confirmed-history discovery, fresh UTXOs and pending reconciliation. It requires
`core-rpc`, an existing testnet4 store and the user's configured local Core. It is
an explicit developer API; desktop connection and disclosure consent remain gated.

## One budget, one account and matching anchors

A single monotonic budget starts before account loading. All nested RPCs share
**60 seconds** and `DiscoveryLimits.max_requests`: **256 by default**, configurable
from 1 to **512 total requests**. This is the whole refresh cap, not a fresh
allocation for each stage. Per-request timeouts remain at most ten seconds and
HTTP header/body limits remain 4/16 KiB. Existing discovery address/height limits,
64-output/event limits and the 128-transaction global mempool cap remain in force.

1. Authenticate and load a public account/counter snapshot from the protected store.
2. Complete bounded discovery on both branches, retaining the private full-account
   binding and recommended counters. Errors contain no partial recommendation.
3. Observe fresh confirmed/pending state for **both examined prefixes**, including
   the stopping gap, using the account bound to discovery. No detached cached
   balance or UI-supplied script/key/range is accepted.
4. Require discovery and balances to have the same anchor height **and** hash.
   Ordinary tip growth across stages also returns `SnapshotChanged`, requiring an
   explicit retry. Existing reorg/incomplete errors retain their typed context.
5. Check remaining time and apply counters through the existing full-account match,
   upward merge, no-op detection and atomic protected commit.
6. Check time again; return `WatchOnlyRefresh { discovery, observation }` only after
   acknowledged persistence within the deadline.

Standalone discovery/UTXO/mempool APIs keep their original budgets and behavior.
Private shared-budget entry points reuse their checks without resetting time or
request counts. No RPC method, endpoint, fallback or automatic retry is added.
The store's existing process lock and exclusive mutable borrow span the call.
Initial loading can recover earlier interrupted work; failed new network work
does not apply a new recommendation.

## Failure states

| `RefreshError` | Meaning |
|---|---|
| `Load(StorageError)` | Account authentication/recovery failed before any node request. |
| `Discovery(DiscoveryError)` | Discovery failed, was incomplete or exhausted the shared budget; no new application. |
| `Observation(MempoolError)` | Confirmed/pending work failed or exhausted the remaining budget; no new application. |
| `SnapshotChanged` | Discovery and balance anchors differ; no new application. |
| `Deadline` | Work expired after observation but before application. |
| `Commit(StorageError)` | Application failed, possibly before or after rename; reload before inferring surviving counters. |
| `CommittedDeadline` | Persistence was acknowledged, then the time check expired; counters are saved, but no successful observation is returned. |

The operation returns no successful partial balance or confirmed-only fallback.
Errors remain opaque and contain no cookie, wallet data or underlying service text.
A failed initial recovery refresh must not authorize issuing addresses from an
unverified imported account. Separately applying successful discovery remains an
explicit lower-level operation.

OS/credential/filesystem calls cannot be forcibly interrupted here. They may return
much later; time checks report lateness when control returns. This is not a hard
real-time or cancellation guarantee. A commit error is not a promise that nothing
changed. Safe retries reload/recover state and rerun observation.

## Privacy, balances and persistence

Descriptors/ranges disclose address linkage to the selected local Core. Future UI
must obtain disclosure consent before invoking the operation off the UI thread.
`WatchOnlyRefresh` has no Debug, serialization or persistence implementation.
Confirmed totals and pending projections stay distinct exact-satoshi values,
retaining immature-coinbase/change/self-transfer semantics. They are not spendable
balances or proof of global history/confirmation. Gap policy, node fabrication or
omissions and transient chain/pool changes remain trust limits.

Only existing public account/origin/counters are saved. Anchors, amounts, outpoints
and pending activity remain transient. Observation timestamps precede the final
OS commit and must retain their age; success does not promise the chain/pool stayed
unchanged during saving or afterward. Formats, credentials and seed/signing gates
are unchanged. No background polling or node startup is introduced.

## Verification and audit cost

Six new regular tests cover exact sums/ranges, success at the exact request cap,
shared-cap exhaustion across stages, pre-network storage/limit failures, failed
discovery/transport and changed anchors without commits, before/after-rename
errors with recovery, and dropped pending transactions on repeated refresh without
rewriting unchanged counters. Fixtures use public keys and synthetic Core replies.
The native Keychain/Secret Service smoke test now refreshes and reopens the advanced
state using real OS credentials with synthetic observation.

A separate ignored test waits 61 seconds during a final simulated credential write
and verifies `CommittedDeadline` with both counters saved. It was run locally and
is excluded from ordinary CI; it adds no production clock hooks or mock clock.
Local checks passed **88 regular backend/all-feature Rust tests**, 25 default Rust
tests, four Python checks, native Keychain refresh, six isolated Core 31.1 checks,
the slow-commit test, formatting, strict Clippy and dependency policy. Physical
power-loss, packaged-app lifecycle and independent review remain release gates.

This increment adds **103 net production Rust physical lines**, including comments
and spacing: 79 in refresh, 20 net for shared discovery/mempool entry points, two
in deadline accounting and two in Core wiring. Total production Rust is **2,444
lines**. Test code is separate. The functional increment adds no package/version/
feature changes: 473 locked entries and 16 production direct dependency names.
The preceding security checkpoint `482b4a2` separately patches rustix and replaces
the yanked executor. Accessibility exceptions retain their 2026-12-01 UTC deadline.

Next: an explicit watch-only desktop connection and refresh/status view, with query
consent, clear incomplete/error states and the existing seed/signing/mainnet gates.
