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

## Review cadence

Update this model with every signing feature, platform, network, update mechanism,
and new class of remote input. Security review is a release deliverable.
