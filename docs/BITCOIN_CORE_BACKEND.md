# Bitcoin Core — read-only testnet4 boundary

## Implemented scope

`core_rpc::CoreTestnet4` is a blocking macOS/Linux client enabled only by the
`core-rpc` Cargo feature. Construction does not make a request. The default build
and desktop demo remain offline; no UI, storage or background polling is connected.

The public API has three operations:

- `status()` reads chain identity and node-reported sync/tip information.
- `verify_account(&BitcoinTestnetAccount)` checks both public descriptors and
  compares indices 0 and 1 on each branch against local derivation. It does not
  issue addresses, change counters, import descriptors or persist anything.
- `discover(&account, limits)` performs bounded confirmed-history address discovery,
  respecting saved counters and explicitly rejecting incomplete history or reorgs.
  See the [discovery policy and limits](BITCOIN_DISCOVERY.md).

The private RPC allowlist is exactly `getblockchaininfo`, `getblockhash`,
`getdescriptorinfo`, `deriveaddresses`, `scanblocks` (start only) and
`getdescriptoractivity`. There is no arbitrary RPC/URL API, wallet
creation, wallet loading, descriptor import, balance claim, signing,
broadcast or seed-entry path. Core's utility descriptor check needs no Core wallet.

## Connection and credential policy

- Only `http://127.0.0.1:48332/`, Core's default testnet4 RPC endpoint. No DNS name,
  configurable port, remote server, proxy, redirect, TLS or compression features.
- The caller supplies an absolute, trusted Core data-directory path. Credentials
  come only from `testnet4/.cookie`; neither `bitcoin.conf` nor environment-provided
  RPC passwords are read. Core itself still requires the user's correct setup.
- Open the cookie with no final-symlink following and nonblocking file access;
  require a regular, single-link file owned by the current user, with no group/other
  permission bits. Trusted parent directories and same-user OS access remain boundaries.
- Bound reads to 257 bytes and accept only Core's standard 75-byte `__cookie__:`
  plus lowercase hex token. Nonstandard cookies, newline suffixes and shared-user
  cookies fail closed. An absent/rejected cookie never triggers a password fallback.
- Read anew for each request so node restarts cannot leave retained credentials.
  Cookie and Base64 buffers use fixed-capacity/zeroizing storage where controlled
  by Monoform. HTTP/header/socket library copies are **not** guaranteed to be wiped.
- Mark the authorization header sensitive. Additionally, the feature selects
  `log/max_level_off`: `ureq-proto` TRACE can dump raw wire bytes despite header
  redaction. This deliberately disables `log`-based logging throughout this build;
  a test checks that the compile-time maximum is Off. No RPC/body logging is added.

HTTP is acceptable only within this fixed loopback boundary. Do not expose Core's
RPC server publicly, assume a cookie grants read-only authority, or treat cookie
authentication as an independent signing protection. Core cookies ordinarily grant
broad RPC access; read-only restrictions belong to this client, not the credential.

## Untrusted responses and observations

Each request has a ten-second end-to-end timeout, a 4 KiB HTTP-header limit and a
16 KiB response-body limit. Redirects and inherited proxy settings are disabled.
An operation makes at most two requests for status or six for account verification;
its total time can therefore approach 20 or 60 seconds. Future UI integration must
use a reviewed background/cancellation path, never block the UI thread. Discovery
also has a capped request count and a 60-second remaining-time deadline.

Require JSON-RPC 2.0 and the exact request ID, typed required result fields, valid
unsigned integer heights/times, consistent heights, and a canonical lowercase
64-character tip hash. Duplicate known fields, trailing JSON, wrong types, missing
results, and mixed success/error envelopes fail. New unrelated Core fields may be
ignored. Descriptor flags/checksums must match, and address arrays must contain
exactly two locally matching addresses. Errors expose only static variants.

Both the `testnet4` chain name and genesis hash must match before returning status
or disclosing descriptors. Genesis identity comes from Core's
[testnet4 parameters](https://github.com/bitcoin/bitcoin/blob/v31.1/src/kernel/chainparams.cpp):

```text
00000000da84f2bafbbc53dee25a72ae507ff4914b867c565be350b0da8bf043
```

`NodeStatus` contains heights, tip hash/time, pruning flag and observation time:

- `Syncing`: Core reports initial download or headers exceed validated blocks.
- `Stale`: otherwise the reported tip is more than two hours old.
- `Recent`: otherwise the reported tip is recent; this is **not** a promise that
  the node has peers, reached the global tip, retained history or supplied honest data.

The two-hour rule is a conservative display policy, not a Bitcoin consensus rule.
Future-dated tips beyond that allowance and inconsistent genesis/height observations
are rejected. The observation is not automatically refreshed or persisted.
Authentication failure, unavailable/warming-up nodes, wrong chain, malformed replies
and incompatible descriptors stay explicit errors, never a successful zero balance.

A malicious local service can lie consistently about chain and tip. Separate RPC
calls are not an atomic snapshot or service attestation. This module does not
independently validate headers, consensus, transaction inclusion or account ownership.
Descriptor checks establish compatibility, not complete address/history discovery.
The separate discovery operation is complete only under its confirmed-history gap
policy and pinned anchor, not a proof of global completeness or a transaction ledger.
Account verification exposes the account's address linkage to the selected local
node; call it only with the user's consent. Status sends no wallet material.

## Verification and reproducibility

On 2026-10-07, both branches' checksums/flags and indices 0 and 1 matched Bitcoin
Core **31.1.0**, first through the existing Python/CLI checker and then the Rust
HTTP client. The node used a fresh temporary testnet4 directory, disabled wallets,
disabled peer networking/listening/DNS discovery, and remained at genesis. This
does not exercise synchronized history, real peers, balances or reorg handling.

The official macOS arm64 archive matched SHA256
`16a097c09fbd7eb78b240ce1dae123663ea2e5e377cfd6a951e71e227e23cf2f`.
The official checksum list had valid signatures from Ava Chow and Michael Ford,
using public keys from the Bitcoin Core `guix.sigs` builder-key repository. This
does not claim personal/web-of-trust certification or verification of every signer.
The Linux CI archive is pinned to SHA256
`b80d9c3e04da78fb6f0569685673418cf686fadba9042d926d13fb87ff503f9e`.
See the [official verification process](https://bitcoincore.org/en/download/).

To repeat against an already verified Core binary directory:

```sh
python3 -B scripts/test_bitcoin_core.py /absolute/path/to/bitcoin-31.1/bin
```

The harness builds the public fixture and integration test, owns its Core child
process, runs both checks, terminates that child and removes only its temporary
node directory. It does not install software or stop an existing user's node.
The default port must be free. CI is configured to run this live check on Linux
and synthetic backend tests on both platforms; it never requires a public-network sync.

To explicitly inspect a user's local testnet4 node without sending wallet material:

```sh
cargo run --locked --features core-rpc --example bitcoin_core_status -- /absolute/bitcoin-data
```

## Audit cost and next gate

The module adds 297 lines of production Rust including comments/formatting, plus
four net library wiring/documentation lines. The status example, isolated-node
harness and tests are separate developer tooling. Five optional direct dependencies
are added: `ureq`, `serde`, `serde_json`, `base64`, and `log`; the existing graph
entries `serde`/`log` are reused, not upgraded. The all-target lockfile grows
from 450 to 459 packages, with no existing package-version changes. The default
dependency graph is unchanged. No HTTP parser or Base64 implementation is hand-rolled.
HTTP TLS/compression defaults and asynchronous runtimes are not enabled.

Bounded [confirmed-history discovery](BITCOIN_DISCOVERY.md) is now implemented
separately. Its audit cost and further gates are recorded there. Balances, monetary
transaction history, persistence of scan recommendations, UI connection and signing
remain unimplemented; compatibility/discovery checks do not satisfy those gates.
