# Bitcoin public state — version 1

## Implemented boundary

`BitcoinTestnetAccount::encode_public_state()` returns a canonical text record.
`BitcoinTestnetAccount::from_public_state()` restores a watch-only account and resumes
both address counters without a recovery phrase. Neither method writes files, encrypts
data, contacts a node, or enables signing. The desktop UI still uses demonstration data.

Version 1 selects **Bitcoin testnet4**, BIP84 account `m/84'/1'/0'`, receiving branch
`0`, and change branch `1`. The old offline address fixtures remain unchanged. A `tb1`
address or `tpub` alone cannot identify the testnet variant; the future backend must
verify its actual chain against this record before using any response.

## Record format

One ASCII line, no newline or whitespace, at most 256 bytes:

```text
monoform:1:bitcoin-testnet4:FINGERPRINT:TPUB:NEXT_RECEIVE:NEXT_CHANGE:CHECKSUM
```

- `FINGERPRINT`: eight lowercase hexadecimal characters, root fingerprint.
- `TPUB`: canonical Base58Check compressed public account key, depth 3, child `0'`.
- Counters: canonical unsigned decimal, each from `0` through `2147483648` inclusive.
  Each is the next index to issue, not the last address used on-chain. `2147483648`
  means exhausted; no address may be generated there. Failure leaves counters unchanged.
- `CHECKSUM`: lowercase SHA256 hexadecimal of all preceding ASCII bytes, excluding
  the colon before the checksum. Uses the existing hash library, not new crypto code.

The parser rejects unknown versions/networks, extra/missing fields, private extended
keys, mainnet keys, malformed curve points, wrong account depth/child, excessive
counters, noncanonical spellings, corruption, and truncated or oversized records.
Descriptors are regenerated, not stored redundantly. Errors do not echo input.
The checked-in fixture is public test data and must never receive real funds.

## Security and privacy limits

Public does **not** mean safe to publish. The account key and descriptors reveal all
derived addresses and enable address-history correlation. Never include real records
in logs, crash reports, telemetry, Git, or support tickets.

The checksum is neither authentication nor encryption. An attacker can replace a key,
alter counters, or replay an older record and recompute the checksum. This codec
validates structure, not ownership. The root fingerprint, depth, and child number
cannot establish the full ancestry of an imported public key. Before any future
signing, compare the complete account key against the locally derived dedicated seed;
a four-byte fingerprint is not enough. Do not silently accept a different account.

Restoring old counters can reuse addresses. Recovery from a seed must scan both
branches, while recovering from a backup must also respect its known issued counters.
An unrecorded, unused issued address is not discoverable from the blockchain alone.
The codec alone provides no rollback protection or safe persistence. The separate
OS-backed storage layer below anchors its encrypted file to an OS credential.

## Storage decision — implemented as a separate offline module

`storage::PublicStateStore` uses macOS's local login Keychain or Linux's persistent
Secret Service for a random encryption key and commit markers. It uses authenticated
XChaCha20-Poly1305 encryption, private file permissions, an exclusive OS file lock,
same-directory staging, atomic replacement, and file/directory synchronization.
Address reservation returns only after file synchronization and OS-marker acknowledgement.

The OS marker rejects a stale encrypted file while the current marker remains intact;
it is not a hardware monotonic counter and cannot detect a rollback of both the file
and OS credential. Recovery handles interruptions on either side of replacement.
Missing or unavailable credentials fail closed, never triggering automatic key reset.

This text record remains the inner codec, not an encrypted seed vault. The UI is not
connected to storage; the in-memory address API is still not a durable reservation API.
See [OS-backed public-state storage](PUBLIC_STATE_STORAGE.md) for the complete format,
protocol, tests, threat limits, and platform/release gates.

## Backend and discovery decision — implementation deferred

Start with a user-operated Bitcoin Core node on testnet4, loopback-only RPC and local
cookie authentication; no hosted default endpoint and no seed/private-key RPC payloads.
Reject another chain before accepting wallet data. Production requests need bounded
schemas, timeouts, response limits, and explicit unavailable/stale/incomplete states.
Adding a dedicated watch-only Core wallet or importing descriptors is a separate,
user-approved operation, never a side effect of a compatibility check.

Discovery starts at index zero for both account-0 branches. Require 20 consecutive
addresses with no transaction history beyond both the last used address and the
known issued counter before stopping. Using 20 for the external branch follows
[BIP44's discovery convention](https://github.com/bitcoin/bips/blob/master/bip-0044.mediawiki);
applying it to the change branch is Monoform's explicit conservative policy. A zero
balance does not establish that an address was never used. Keep counters monotonic,
never wrap past `2147483647`, and report incomplete discovery if a scan limit, node
failure, or missing/pruned history prevents completion. Seed-only recovery can miss
funds beyond the gap; the eventual UI must explain this and offer a reviewed extended
scan. Address issuance must warn before exceeding recoverable unused-address gaps.

Before backend integration, verify the public descriptors directly with Core's
[`getdescriptorinfo`](https://bitcoincore.org/en/doc/30.0.0/rpc/util/getdescriptorinfo/)
and compare both branches' addresses with
[`deriveaddresses`](https://bitcoincore.org/en/doc/30.0.0/rpc/util/deriveaddresses/).
The optional checker below does not import a descriptor, create a wallet, or send funds:

```sh
cargo run --locked --example bitcoin_core_fixture | python3 scripts/check_bitcoin_core.py /absolute/path/to/bitcoin-data
```

Requires a running local testnet4 Core node and `bitcoin-cli` on PATH. Only the
published fixture is sent; never modify this tool to accept real wallet material.
The checker ignores `bitcoin.conf`, uses the default testnet4 RPC port and local
cookie, and fails if the node is unavailable or reports a different chain. This is
an opt-in developer check, not production networking or a default CI dependency.

## Next gates

The roadmap separates the codec and protected public storage from pending seed-vault,
backend, and UI work. Physical power-loss and packaged-app credential lifecycle tests
remain release gates; current storage is a pre-alpha checkpoint, not audited software.
Dependency vulnerability/license CI checks, maintainer approvals, and signed-release
policy are still required; a passing codec test suite does not satisfy those gates.
