# Protected testnet receive issuance — 2026-11-23

`CoreTestnet4::receive_address(&mut store, limits)` returns one saved Bitcoin
testnet4 receive address after fresh confirmed-history discovery and a recovery-gap
check. It requires `core-rpc`, an existing protected account and explicit descriptor
disclosure consent. No Core wallet, signing, broadcast or seed entry is added.

## One fresh operation and one protected save

1. Load authenticated public state, retaining the existing exclusive store lock.
2. Run complete [confirmed discovery](BITCOIN_DISCOVERY.md) on both branches,
   respecting saved counters and rejecting missing history, reorgs and bounded-work
   failures. Cached discovery and balance observations are never accepted as input.
3. Recheck node identity/readiness and require exactly the same height **and hash**
   as discovery. Even ordinary tip growth requires an explicit retry. A shared
   budget covers loading, discovery and this final check: 60 seconds and 256 RPCs
   by default, at most 512 when explicitly configured. No budget is reset per stage.
4. The crate-private store step reloads/recover-verifies state, compares the full
   extended account key and origin, and merges both branch counters upward.
5. Let `used_next` be the last confirmed-used receive index plus one, or zero if
   none. Let `index = max(saved_next_receive, used_next)`. Require
   `index - used_next < 20` before issuing: index 19 is the twentieth unused address
   on an empty account; index 20 is blocked until confirmed usage advances the gap.
6. Derive that address locally. Save **both discovered counters plus the advanced
   receive counter in one atomic commit**. Only acknowledged saving within the
   deadline returns `ReceiveAddress { address, index, anchor_height, observed_at }`.

Pending transactions cannot widen the gap. Confirmed spent history counts as use;
a zero balance cannot establish non-use. Imported/saved counters are never lowered
to satisfy the policy. Exhausted branches and address/height/request limits fail
closed. A gap refusal saves neither the new reservation nor discovery recommendations;
initial loading can still recover an earlier interrupted commit.

The protected store's lower-level offline reservation APIs remain available, with
their original persistence guarantees. They do not enforce history/gap policy and
are not used by the desktop receive action.

## Explicit failures and remaining limits

| `ReceiveError` | Result |
|---|---|
| `Load(StorageError)` | Account verification/recovery failed before any node query. |
| `Discovery(DiscoveryError)` | Incomplete/unready/wrong-network history, a reorg or a work limit; no new reservation. |
| `SnapshotChanged` | Final tip differs, including ordinary growth; no new reservation. |
| `GapLimit` | At least 20 unused slots after the last confirmed use; no new save. |
| `Deadline` | Time expired before entering the reservation step. |
| `Commit(StorageError)` | Save failed; counters may have advanced. Reload/recover before inferring surviving state. |
| `CommittedDeadline` | Reservation acknowledged after the deadline; counters saved, but no address returned. |

OS/credential calls cannot be forcibly interrupted and may finish saving after the
network deadline. A safe retry reloads and discovers again, possibly skipping an
unreturned index. Never reset counters or remove state/credentials to retry. File
sync and OS read-back retain the [existing durability limits](PUBLIC_STATE_STORAGE.md).
The chain may change during the final OS save; the observation time precedes it.

Confirmed history is still node-reported. Gap checks cannot reveal unused issued
addresses absent from an old export, detect unconfirmed usage missing from saved
counters, prevent a lying node, or coordinate separate stores for the same account.
Use one active issuing copy. Physical power-loss,
packaged-app lifecycle and independent security review remain release gates.

## Desktop flow

Run `cargo run --locked --features desktop,core-rpc`. Enter the account and local
Core folders, acknowledge query/counter-saving consent, then choose **New receive
address**. The existing worker owns the store and node; inputs stay locked until
completion. Every request performs fresh discovery and consumes a new index only
after the guarded save. The screen shows the complete selectable address, index,
testnet4 label, history anchor and observation age. No QR, automatic clipboard write,
automatic retry, background query or mainnet activation is introduced.

Starting another action, editing a folder, changing consent/mode or clearing the
screen discards its previous result. Saving is not undone by clearing/closing the
screen. This increment does not yet reopen and display previous reservations; keep
the displayed test address privately if needed. At the gap limit, use an already
reserved address and wait for confirmed activity rather than creating more.

## Verification and audit cost

Eight new storage/coordinator tests cover one atomic commit at the exact RPC cap,
the twentieth/twenty-first boundary, confirmed gap advancement, final tip changes
and readiness, incomplete/wrong-chain/reorg history, shared-cap exhaustion, full-key
binding and stale counters, pre-query credential/limit failures, and before/after-
rename interruption with safe retry. Three desktop tests cover complete address
rendering, consent/single-worker gating, stale-address removal and distinct errors;
the existing field-edit regression now checks saved addresses too.

The native Keychain/Secret Service smoke test now issues and reopens a guarded
reservation using synthetic node responses. An additional ignored 61-second test
was run locally: a late final credential write saves both counters but returns only
`CommittedDeadline`. It adds no production clock hooks and is excluded from ordinary CI.
Local verification passed **115 regular all-feature Rust tests**, 26 default Rust
tests, four Python checks, native Keychain receive, formatting, strict Clippy and
current advisory/license/source policy. CI retains isolated Core schema checks;
a live-node successful GUI receive is not claimed. Native previews use only the
published public fixture.

This increment adds **182 net production Rust physical lines**: 81 for the receive
coordinator, 32 net for shared full-account merging and atomic reservation, three
for Core wiring and 66 for consent/action/display/error handling. Total: **2,877**,
including comments/spacing and excluding tests/test wiring. No dependency, version,
feature or lockfile changes: **16 production direct names, 473 locked entries**.
The accessibility maintenance exceptions still expire on 2026-12-01 UTC.

Next: reopen and display previously reserved receive addresses without advancing
counters, so reopening the app does not require another reservation.
