# Bounded Bitcoin history discovery

## Scope and consent

`CoreTestnet4::discover(&account, limits)` scans confirmed testnet4 history for
receiving branch 0 and change branch 1. It requires an explicitly selected local
Core 31+ node with a synchronized `blockfilterindex=1`, unpruned block/undo data,
and a recent, non-syncing tip. Monoform does not configure, restart or synchronize
the user's node automatically. The existing loopback/cookie/response boundaries
remain in force, and default builds and the demo UI remain offline.

Discovery sends public descriptors to the local node. They reveal the account's
address linkage, so future UI integration must obtain consent. No seed/private
key is sent, no Core wallet is created/imported, no balance is computed, and
neither the account nor the protected store is modified.

## History, not unspent balance

Scan from genesis to a pinned, node-reported tip, in address batches of at most
20 and block-height chunks of at most 4,096. Core's
[`scanblocks`](https://bitcoincore.org/en/doc/31.0.0/rpc/blockchain/scanblocks/)
finds candidate blocks using its basic filters. Exact block/undo activity from
[`getdescriptoractivity`](https://bitcoincore.org/en/doc/31.0.0/rpc/blockchain/getdescriptoractivity/)
confirms usage: a matching receive **or spend** counts, even if every received
output was subsequently spent. Filter false positives do not count as usage.
There is no UTXO-only fallback.

Locally derive each BIP84 P2WPKH script with existing key/hash libraries. Match
Core's exact script bytes against those scripts, not an inferred descriptor or
an address label. Require typed event kinds, confirmed heights/block hashes,
the requested chunk/hash and the node's active-chain hash at every event height.
Reject mismatched scripts, unconfirmed/malformed events, duplicate candidate
hashes, wrong returned ranges and unfinished scans.

Mempool activity is explicitly excluded. This is confirmed-history discovery,
not pending-transaction reconciliation, transaction inclusion verification or
a complete transaction history view. Amounts, txids and monetary totals are not
interpreted by this module.

## Saved counters and gap policy

Each branch starts at address index zero, even if a saved cursor is nonzero.
After covering the entire anchored history for an address batch:

```text
recommended_next = max(saved_next, last_confirmed_used + 1)
complete_branch  = examined_count >= recommended_next + 20
```

With no usage, `last_confirmed_used + 1` is zero. Thus completion requires at
least 20 consecutively unused indices beyond both the last used address and the
saved issued-address prefix. Batches can conservatively examine extra addresses.
Any later usage resets the stopping target. A counter is never reduced or wrapped.

Success returns the pinned height/hash and both branches' examined counts, highest
used index and recommended next index. It is a transient recommendation only;
applying it needs a separately reviewed atomic storage/orchestration operation.
In particular, discovery must not overwrite a higher counter issued concurrently.
Failure returns **no partial recommendation**. Counters stay untouched on every path.

The gap is a discovery policy, not proof there are no later funds. Seed-only
recovery can miss activity beyond an unused gap. Unused addresses issued without
a surviving saved counter cannot be recovered from blockchain history alone.
Future recovery UI must explain these limits and offer a reviewed extended scan.

## Work limits and incomplete results

Defaults: 200 addresses per branch, maximum chain height 1,000,000 and 256 RPC
requests. Caller-selected limits may not exceed 10,000 addresses per branch,
height 1,000,000 or 512 requests. Address limits must allow at least the gap;
request limits must be nonzero. Invalid settings fail before contacting Core.

An operation has a 60-second monotonic network-work deadline; each request uses
the smaller of its remaining time and the existing ten-second RPC timeout.
The request budget includes chain/status and anchor checks, not just scan calls.
The 4 KiB HTTP-header and 16 KiB body limits also apply to scans/activity. A busy
address/block may exceed the body limit: reject it, never truncate its activity.
This is not a real-time guarantee for filesystem stalls or a suspended process.

`DiscoveryError::Incomplete(reason)` distinguishes syncing, stale tip, pruning,
unavailable history/filter index/Core support, address cap, exhausted derivation
space, height cap, request cap and deadline. Other authentication/transport/schema
failures remain explicit opaque `Backend(CoreError)` errors. None represents an
empty wallet or a complete pass.

`scanblocks start` uses Core's shared transient scan slot. Discovery does not
invoke `abort`, cancel someone else's scan or automatically retry a busy scan.
After a client timeout, Core may continue its bounded chunk; the client deadline
does not promise to cancel server work. Use the user's own node, off the UI thread.
No RPC that mutates wallets, transactions or the active chain is exposed.

## Chain changes and trust

Check testnet4 name/genesis and pin the tip hash at its reported height before
descriptor disclosure. Check event membership and the original anchor again
before returning success. A changed anchor, lost height, inactive block or shorter
chain returns `DiscoveryError::Reorg`, discarding both branch recommendations.
Normal tip growth is allowed if the original anchor remains active; new blocks
are outside this pass and require a later observation.

This is not an atomic node snapshot, independent consensus validation, or proof
against a malicious node. Separate calls cannot guarantee detection of a transient
reorg that reverses itself between checks, or a change immediately after return.
A failed/aborted scan may instead report unavailable history when Core cannot
establish why it failed. Consumers must treat results as anchored observations,
not a permanent guarantee or authorization to sign.

## Tests and audit cost

Synthetic loopback tests cover both branches, fully spent history, false positives,
gap resets, saved counters and shortened final batches, caps/deadlines, pruning,
staleness/sync, unavailable history, malformed results, reorgs and harmless tip growth.
Scripts are compared with the independent `bitcoin` test dependency.

The isolated Core harness now enables a temporary filter index. It verifies actual
filter-scan/activity response schemas at testnet4 genesis and verifies that the
public discovery API correctly refuses that syncing node. The readiness bypass
used for the schema check is private test code only. Used/spent and reorg cases
are synthetic, not a mined end-to-end Core history test; this limitation remains
explicit and does not satisfy the external review or release gates.

On 2026-10-11, local macOS verification passed 53 default/opt-in Rust checks, both
live Core compatibility/schema checks, the isolated native Keychain smoke check,
four Python checker tests, formatting and strict linting. Linux/macOS CI runs the
regression suite; the isolated live Core harness is additionally enabled on Linux.

This incremental module adds 318 production lines plus 45 net lines in the Core
transport/wiring, including comments and formatting. No dependency is added or
upgraded; the lockfile remains 459 packages. Including the previously uncommitted
Core backend, this checkpoint adds 664 production Rust lines relative to `a1f1905`
and retains its nine-package optional dependency growth (450 to 459).

Separate [anchored UTXO observations and exact satoshi balances](BITCOIN_UTXOS.md)
are now implemented without treating those balances as evidence of complete history.
Discovery and observation reuse one bounded RPC helper; discovery's error mapping,
pruned-history policy and stopping rules remain unchanged. Persistence of discovery
recommendations, mempool reconciliation and UI integration remain gated.
