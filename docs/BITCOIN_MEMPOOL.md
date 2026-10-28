# Bounded pending-spend and receipt reconciliation

## One fresh, scoped observation

`CoreTestnet4::reconcile_mempool(&account, scope)` combines a **fresh** confirmed
UTXO observation with the local node's pending activity for the same explicit
receive/change prefixes. It never accepts a detached cached balance from the UI.
The existing scope/counter validation, loopback/cookie policy, exact amount parser,
privacy boundaries and opt-in `core-rpc` feature remain in force. The default build
and UI stay offline; no address counters or observation records are persisted.

All work shares one budget: 112 request attempts, a 60-second monotonic deadline,
and at most ten seconds per request. Nested UTXO observation does not reset it.
The existing 4 KiB HTTP-header and 16 KiB response-body limits still apply.

1. Verify testnet4 identity and a recent, non-syncing chain tip.
2. Read [`getmempoolinfo`](https://bitcoincore.org/en/doc/31.0.0/rpc/blockchain/getmempoolinfo/);
   require completed initial loading and at most **128 global mempool transactions**.
3. Capture canonical, unique transaction IDs and the sequence counter using
   [`getrawmempool false true`](https://bitcoincore.org/en/doc/31.0.0/rpc/blockchain/getrawmempool/).
4. Read and validate fresh confirmed UTXOs, requiring the original tip.
5. Query [`getdescriptoractivity`](https://bitcoincore.org/en/doc/31.0.0/rpc/blockchain/getdescriptoractivity/)
   with no blocks, explicit descriptors/ranges and `include_mempool=true`.
6. Validate and reconcile at most **64 wallet-relevant activity events**.
7. Recheck chain identity/tip, then the loaded pool, its complete ID set and sequence.

Changed sets/counters, replacements, evictions, confirmations and observed tip
growth discard the whole overlay as `SnapshotChanged`; detected lost anchors are
reorg errors (including nested confirmed-observation errors). No partial balance
is returned and there is no silent retry. A subsequent explicit call starts fresh.
Unloaded/large pools, excessive activity and work caps are explicit incomplete
outcomes. Malformed/oversized replies and transport/authentication failures remain
opaque backend errors, never a successful zero.

The **global** pool limit is intentional: even a small wallet on a busy node may
receive an incomplete result before account disclosure. This first adapter does
not silently sample the pool or relax response/work caps to show a balance.

## Projection rules

Every event's transaction must belong to the captured pool. Reject confirmed
height/hash values, unknown scripts/kinds, ambiguous receive/spend fields, duplicate
receiving outpoints, duplicate spending inputs and duplicate spent outpoints.
Scripts must match locally derived BIP84 branch/index scripts. Monetary JSON
lexemes use the existing exact satoshi decoder, with no floating-point conversion.
The wire DTO remains flat: Serde's internally-tagged enum buffering is not used
for monetary fields because it discards the original numeric lexeme.

Process receipts before dependencies, independently of event order. A wallet spend
must reference either a fresh confirmed output or a pending wallet receipt, with
the exact same amount, branch and index. Missing dependencies or conflicting
metadata fail closed. Reject cycles in wallet-relevant pending dependencies and
impossible spends of coinbases with fewer than 100 confirmations. The latter is
next-block eligibility, distinct from the conservative <=100 display bucket.

Return the confirmed observation unchanged and three distinct exact-satoshi sums:

- `confirmed_pending_spent_sats`: gross confirmed input value consumed by pending
  transactions, **not** payment value or fee attribution;
- `pending_unspent_receipts_sats`: pending wallet outputs not consumed by another
  pending transaction, including change and self-transfers;
- `projected_total_sats`: confirmed total minus pending-spent confirmed inputs plus
  terminal pending wallet outputs, using checked arithmetic and `MAX_MONEY` bounds.

For example, a pending parent spending 1 BTC into 0.99999 BTC of wallet change,
followed by a child spending that change into a 0.99998 BTC wallet receipt,
projects 0.99998 BTC—not the sum of both receipts. A receipt carries a flag if
another pending transaction consumes it; spends distinguish confirmed versus
pending inputs. Ordering is not a public contract. Results contain wallet-private
metadata and deliberately implement neither Debug nor persistence/serialization.

## Security and scope limits

The projection is **not an available/spendable balance**. It includes immature
confirmed coins; pending transactions may be replaced, evicted, rejected by other
nodes or never confirmed. The node's pool is only its own current view, not a
network-wide acceptance or confirmation guarantee. Zero remains scoped to the
queried prefixes and node, never evidence of complete address history.

The node can consistently omit/fabricate data. These checks do not independently
verify transaction signatures, all external input dependencies or Bitcoin consensus.
Cycle detection covers wallet-relevant edges, not the complete transaction graph.
Sequence bracketing is not an atomic RPC snapshot, a hardware monotonic counter or
process attestation: a restart/restored state can evade it, and state can change
immediately after the final checks. There is no permanent pending-transaction cache,
historical ledger, RBF management, fee bumping, signing or broadcasting API.

## Verification and audit cost

Synthetic tests cover exact one-satoshi receipts, order independence, parent/child
change, empty/refreshed observations, replacement/eviction/sequence changes, tip
changes/reorgs, loaded/global/event/work caps, duplicate/malformed metadata, missing
dependencies, cycles and supply bounds. Existing confirmed/discovery tests run again.

The isolated Core 31.1 harness checks empty testnet4 pool schemas and a nonempty
**peer-disabled regtest** pool. Ignored test code signs only published-vector keys,
submits a parent and child locally, verifies their exact projection, replaces the
parent with a higher-fee transaction (evicting both originals), and mines its
confirmation. Fresh UTXO and empty-pool observations then agree. These are worthless
test coins, with no Core wallet creation or public-network broadcast. Production
still rejects regtest; its RPC allowlist contains no signing/submission method.
Public API snapshot consistency is exercised with synthetic loopback responses;
real regtest checks use private test-only helpers, not a production network bypass.

Local verification on 2026-10-28 passed 76 regular Rust tests, four Python tests,
six live Core checks, the native macOS Keychain smoke test, formatting and strict
Clippy checks. CI repeats the supported macOS/Linux checks on each pushed commit.

The increment adds **418 net production Rust lines**, including comments/formatting:
393 for the mempool module, seven Core wiring lines, and 18 net UTXO refactoring
lines to share one budget. No dependency, version, Cargo feature or lockfile changes;
the resolved graph remains 459 packages. Test/harness additions are separate from
production code. The preceding UTXO module was backed up separately as `7c53985`.

Next: protected atomic application of discovery recommendations, with complete
account matching and monotonic counters. UI integration, seed persistence, signing
and mainnet remain gated; these observation checks do not satisfy release review.
