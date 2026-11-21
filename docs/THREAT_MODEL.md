# Threat model

## Assets

- Dedicated English 24-word BIP39 recovery mnemonic
- Derived private keys and signing capability
- Transaction intent before signing
- Address and balance privacy
- Release-signing keys and dependency integrity

## Trust boundaries

The operating system, display, clipboard, accessibility services, update channel,
build pipeline, dependency registry, and network providers are separate boundaries.
None is treated as perfectly trustworthy. A fully compromised operating system is
outside the software wallet's ability to defeat and must be stated plainly.

## Priority threats and controls

| Threat | Initial controls |
|---|---|
| Seed exfiltration | Native secret entry, no clipboard by default, no UI return value, zeroization, no telemetry/logging |
| Hardware seed reuse | Blocking warning before import; require acknowledgement that software entry removes hardware-only protection; documentation requires a fresh Monoform-only phrase |
| Address replacement | Full address review, local address validation, QR payload preview, no silent clipboard polling |
| Malicious transaction | Human-readable review generated from decoded final transaction before signing |
| Backend deception | Multiple-source option, chain checks, explicit stale/error states, never trust remote fee or balance blindly |
| Supply-chain compromise | Pinned dependencies, minimal graph, locked CI permissions, signed commits/tags, SBOM, reproducible builds |
| Malicious update | Opt-in signed updates, threshold release process, rollback protection |
| Local vault theft | OS-backed encryption key, memory-hard user unlock KDF where applicable, rate limiting, no plaintext backups |
| Privacy leakage | No analytics, self-hosted node option, documented query leakage, proxy support evaluated per chain |
| Public-state exposure or replacement | Account keys/descriptors are privacy-sensitive; OS-backed authenticated encryption and private file permissions; full account-key verification against the future signing vault still required |
| Cursor rollback or interrupted writes | Exclusive process lock, staged atomic writes and OS commit markers before returning addresses; file-only rollback detected, coordinated file/OS-marker rollback is not |
| UI spoofing | Platform-native secret prompts, consistent signing ceremony, no arbitrary HTML or remote content |
| Maintainer compromise | Two-person release approval, least-privilege GitHub roles, protected branches, hardware security keys |
| Wrong-network transfer | Network shown throughout review; exact chain identity validated before signing |
| Malicious or counterfeit token | Compile-time contract allowlist; no arbitrary import or remote token metadata |
| Issuer-controlled asset | Explicit control/freeze disclosure; exact contract identity; no claim that tokens inherit ETH neutrality |
| Rollup or sequencer failure | Selection gate, Ethereum data availability, documented forced-exit path, no wallet bridge |
| Lightning state loss | Feature deferred; persistence and recovery reviewed independently before any embedded node |

## Seed compatibility risk

A BIP39 mnemonic produces seed material, not one universal account layout. Monoform
therefore fixes and versions every path and conversion in `SEED_ARCHITECTURE.md`.
Bitcoin and Ethereum can be discovered by compatible BIP39 wallets using the documented
paths. Monero requires Monoform's Ledger-compatible conversion or export into standard
Monero recovery material. Test vectors must prove all three results before secret entry
is enabled.

## Hardware-wallet seed import risk

Typing a hardware-wallet recovery phrase into Monoform crosses the hardware security
boundary. Malware, the operating system, input services, memory inspection, or a flaw
in Monoform may then obtain the root secret. Hardware isolation cannot be restored for
that phrase by deleting Monoform later.

Monoform is intended to import a fresh phrase generated offline and dedicated to this
software wallet. A hardware device may generate those words, but users must understand
that it is acting as an offline seed generator. The resulting Monoform wallet is not a
hardware wallet and is not hardware-protected.

## Explicit exclusions for early releases

- Mainnet signing before external review
- Arbitrary contract calldata or blind signing
- Browser extensions and injected web contexts
- Cloud synchronization or remote seed backup
- Unreviewed tokens, plugins, or runtime-loaded chain adapters
- Token approvals, permits, arbitrary contract calls, and bridge transactions

## Public storage boundary

The new storage module persists public testnet4 state only. Native credentials and
same-user OS access remain trusted; it provides neither Secure Enclave isolation nor
a hardware monotonic counter. Missing/locked credentials fail closed. Flush requests
and OS read-back do not certify power-loss durability. Migration and restoring both
an old state file and its OS marker need a separate recovery design. See
[Storage protocol and release gates](PUBLIC_STATE_STORAGE.md). Seed storage remains
disabled. The opt-in desktop opens protected watch-only accounts or explicitly
creates one from a reviewed public export.

Explicit [discovery application](BITCOIN_DISCOVERY_STORAGE.md) binds complete results
to the full extended account key and origin, then reloads and merges counters upward
under the store lock. Full-key checks reject mismatches even when origin fingerprints
collide; upward merging preserves later reservations. Existing recovery handles
interrupted commits with the same durability limits. Full scan/anchor records are
not persisted or claimed fresh by saving counters.

## Local read-only Core boundary

The opt-in backend accepts only fixed loopback testnet4 RPC with local cookie
authentication. It bounds headers/bodies/time, disables proxies/redirects and library
logging, checks chain name/genesis, and exposes no wallet/chain mutation RPC method. Cookie
authentication does not constrain Core's own broad RPC authority; same-user/OS
compromise remains outside this boundary. Node status and descriptor checks are
not independent consensus verification or complete history/balance observations.
Bounded confirmed-history discovery checks scripts, honors saved cursors and
rejects incomplete scans or detected reorgs without returning partial recommendations.
Core filter scans use its shared transient scan slot and can outlive a client timeout;
the client neither aborts another scan nor claims atomic snapshot/consensus protection.
Scoped UTXO observation matches exact local scripts, rejects duplicate/inconsistent
outpoints/anchors and independently sums decimal JSON amounts as integer satoshis.
Confirmed totals include immature coinbases and unconfirmed spends: they are not
spendable balances or complete-history proofs. Node omission/fabrication and transient
reorgs between calls remain trust limits. Full privacy-sensitive observations have
no Debug representation or persistence path; only discovered account counters can
be explicitly merged into protected state. Bounded mempool reconciliation brackets fresh
confirmed/pending observations with pool IDs/sequence and chain checks, rejects
duplicates/cycles/inconsistent inputs, and never equates a projection with spendability.
Local-pool omissions and restart/restore races remain trust limits. Full scan-record
caching and signing remain disabled. See
[Core backend limits and tests](BITCOIN_CORE_BACKEND.md).

An explicit [watch-only refresh](BITCOIN_REFRESH.md) shares one work budget across
discovery and observation, requires matching anchors, and defers counter application
until those checks pass. Commit errors remain uncertain; acknowledged but late
commits have a separate error. OS calls cannot be forcibly cancelled, and the
chain/pool can change during a final store write. Saving counters and refresh success
do not establish current spendability or complete history.

The [watch-only desktop](WATCH_ONLY_DESKTOP.md) requires unchecked disclosure consent
for explicit account/node folders. Folder edits revoke consent and discard old
figures; inputs are locked while a worker runs, and failed refreshes replace prior
balances. A worker opens and drops the store and detailed observations; the UI
receives aggregate amounts, height, observation time or static errors for refresh. These public
values and paths remain privacy-sensitive in process memory, on screen and through
accessibility/clipboard services; no process isolation is claimed. OS authorization
may outlast the network deadline. Closing the app is not a cancellation guarantee.

[Watch-only setup](WATCH_ONLY_SETUP.md) accepts only a bounded, private, user-owned
regular file and validates the canonical testnet4 record before any destination or
credential access. Only regenerated public text reaches review; the bounded raw
buffer is zeroized even on rejection. Checksum and key-shape checks do not authenticate
ownership, ancestry or counter freshness. Separate approval follows full-record review;
path edits discard it, and creation uses that exact snapshot even if the source changes.
No automatic network query or address issuance follows import. The original export
remains plaintext and privacy-sensitive. Existing final/staged state and credentials
are preserved; uncertain creation requires recovery, never automatic overwrite/reset.

## Review cadence

Update this model with every signing feature, platform, network, update mechanism,
and new class of remote input. Security review is a release deliverable.
