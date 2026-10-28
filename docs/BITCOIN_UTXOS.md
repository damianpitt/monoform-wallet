# Anchored UTXO observations and exact satoshi balances

## Scope — not discovery or spendability

`CoreTestnet4::observe_utxos(&account, UtxoScope)` is an explicit, read-only testnet4
operation. Both scope fields are counts, not final indices: query indices
`0..receive_count` and `0..change_count`. Each count must be 1–10,000 and cover
the supplied account's corresponding saved issued-address prefix. Invalid or
undersized scopes fail before contacting Core. There is no implicit default range.

The caller can use a successful discovery result's examined counts, but must still
keep the two observations separate. A UTXO snapshot can never establish that an
address was unused or that history discovery completed. Zero means no confirmed
UTXOs were reported **within this scope**, not that the entire wallet is empty.
No counters, discovery recommendations or protected state are written.

The opt-in build feature, fixed loopback endpoint, local-cookie authentication,
disabled proxies/redirects/logging, and strict schemas remain unchanged. The UI
and default build stay offline. Descriptors reveal account linkage: user consent
is required before future UI integration discloses them to the local node.

## One checked confirmed-chain snapshot

Require testnet4 name/genesis and a recent, non-syncing node. Pin the initial
reported tip before sending account descriptors. Query both branches together in
one [`scantxoutset start`](https://bitcoincore.org/en/doc/31.0.0/rpc/blockchain/scantxoutset/)
request with explicit ranges; never combine independently timed branch scans.
Core 31+ supplies the scan height/hash, outputs and total.

Locally derive BIP84 P2WPKH scripts using the existing key/hash libraries. Require:

- successful, complete scan and a canonical snapshot hash;
- at most 64 returned outputs and a scanned-output count no smaller than matches;
- exact script membership in the requested local branch/index map;
- canonical transaction/creation-block hashes and unique `(txid, vout)` outpoints;
- typed confirmed creation heights, consistent confirmation counts and tip metadata;
- one height per creation-block hash and active-chain membership at each output height;
- checked per-output sums agreeing exactly with the independently decoded Core total.

An older or conflicting scan anchor is rejected. Normal chain growth can yield a
newer scan snapshot: verify both the initial tip and the actual scan tip are still
active before returning. Changed/lost anchors or creation blocks discard the entire
observation as `UtxoError::Reorg`. Return the scan's actual anchor, not the later tip.

The result includes scope, anchor, observation time, output metadata, confirmed
total satoshis and an immature-coinbase bucket. Output ordering is not a contract.
Privacy-sensitive result types do not implement `Debug`. There is no cache, automatic
polling, signing capability or production persistence path.

## Exact amounts — no floating-point conversion

The existing `serde_json` dependency's `raw_value` feature preserves each amount's
original JSON numeric text. A small bounded parser converts whole/fractional parts
directly to `u64` satoshis using integer arithmetic: one BTC is 100,000,000 satoshis.
Accept nonnegative ordinary decimals with at most eight fractional places, from
zero through 21,000,000 BTC. Reject signs (including negative zero), exponents,
quoted amounts, extra precision, noncanonical leading zeros and excessive values.
This deliberately accepts Core's fixed-decimal format, not arbitrary JSON number
notation. Never round or truncate a fractional satoshi.

Every amount, the aggregate and Core's declared total must be within `MAX_MONEY`.
Use checked addition; a mismatching total or malformed amount produces no balance.
Tests compare parsing with the independent `bitcoin::Amount` test dependency.

`total_sats` includes all confirmed outputs returned within scope, including coinbases
and outputs already spent by an unconfirmed transaction. Conservatively classify
coinbase outputs with at most 100 confirmations as `immature_coinbase_sats`; 101
confirmations exits that bucket. This is a wallet display policy, not an assertion
about the earliest next-block consensus spend. Neither the total nor the difference
is a spendable/available balance: mempool spends, pending receipts, ownership by a
signing vault, fees and transaction policy remain outside this module.

## Limits, pruning and failure

Discovery and UTXO observation share one bounded RPC helper. Observation allows
80 request attempts, a 60-second monotonic work deadline, and at most ten seconds
per network call (or the remaining deadline, if smaller). The existing 4 KiB HTTP
header and 16 KiB body caps apply. The body cap can reject a busy account before
the semantic 64-output cap is reached; do not truncate or return partial balances.
Large wallets require a separately reviewed larger-snapshot/response strategy.

Busy/aborted scans, stale/syncing nodes and caps/deadlines remain explicit incomplete
errors. Authentication, transport and malformed data remain opaque backend errors;
none is interpreted as a successful zero. No automatic retry or `abort` call is
exposed. Core's shared UTXO scan may continue after a client timeout; the client
deadline cannot promise to cancel a full-node scan or bound filesystem stalls.

Pruned nodes are allowed here: pruning removes historical block/undo data, not the
current validated UTXO set or active-chain height/hash metadata. This does not relax
the separate discovery requirement for complete, unpruned history.

## Trust and integration gates

The node can omit or fabricate consistent outputs/amounts. These checks establish
local script association and response consistency, not independent transaction,
unspentness or consensus proofs. Separate calls are not an atomic service snapshot
and cannot promise detection of a transient reorg that reverses between checks.
An anchor can become stale immediately after return. Keep the result attached to
the caller's trusted account context and explicit scope when later adding a cache/UI.
Never present it as a permanent balance or authorization to sign.

## Verification and audit cost

On 2026-10-12, local macOS checks passed 65 default/opt-in Rust tests, four Python
checker tests, strict linting and formatting. The isolated Core 31.1 harness also
passed descriptor compatibility, history-schema and empty/nonempty UTXO checks.

The nonempty case mines **local regtest-only** coinbases with wallets and peers
disabled: a mature receiving output, an immature change output, and 100 outputs
outside the requested prefixes. Exact validation returns only the two in-scope
outputs, with a 100-BTC confirmed total and a 50-BTC immature bucket. No signatures,
private keys, real funds or public-network broadcasts are involved. Only ignored
test code changes the cookie path for this fixture; the public client still rejects
regtest and syncing testnet4. Synthetic tests exercise the public API's anchor-growth,
reorg, limits, script/metadata, duplicate, sub-satoshi, total and maturity boundaries.
These checks do not replace external review or the outstanding release gates.

The change adds **330 net production Rust lines**, including comments/formatting:
309 for the UTXO module, 40 for the shared budget, 10 net Core wiring lines, and
29 removed through discovery-helper reuse. No dependency package or version changes;
`Cargo.lock` remains unchanged at 459 packages. Enabling `serde_json/raw_value`
adds a library feature to the existing opt-in dependency, not a new decimal/runtime
library. Test/harness growth is reported separately from production code.

Separate [bounded mempool reconciliation](BITCOIN_MEMPOOL.md) now uses this UTXO
operation within the same deadline; standalone confirmed observations retain their
original semantics. Saved-state writes, coin selection, signing, mainnet and UI
integration remain gated. The expanded live harness uses published-vector signing
only in ignored regtest fixtures; production still has no signing/submission API.
