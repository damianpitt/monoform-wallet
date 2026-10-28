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
[Storage protocol and release gates](PUBLIC_STATE_STORAGE.md). Seed storage and UI
integration remain disabled.

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
reorgs between calls remain trust limits. Privacy-sensitive observations have no Debug
representation or persistence path. Bounded mempool reconciliation brackets fresh
confirmed/pending observations with pool IDs/sequence and chain checks, rejects
duplicates/cycles/inconsistent inputs, and never equates a projection with spendability.
Local-pool omissions and restart/restore races remain trust limits. UI integration,
scan-result persistence and signing remain disabled. See
[Core backend limits and tests](BITCOIN_CORE_BACKEND.md).

## Review cadence

Update this model with every signing feature, platform, network, update mechanism,
and new class of remote input. Security review is a release deliverable.
