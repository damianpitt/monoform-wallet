# Locked product decisions

These decisions define Monoform Wallet. Changing one requires a public design proposal and
the governance process described in `GOVERNANCE.md`.

## Identity

- Product: **Monoform Wallet**; short form: **Monoform**
- Tagline: **Sovereign Money, One form, Three Chains**
- Signature line: **Three chains. One form.**
- Promise: **A wallet you can read.**
- Philosophy: **The Monochrome Principle** — remove everything that does not help a
  person receive, send, or verify
- License: GPL-3.0-or-later

## Implementation and platforms

- Rust is the primary application language.
- egui/eframe provides the interface.
- Linux and macOS are the first supported platforms.
- Android follows only after the desktop core is proven and independently reviewed.
- Browser extensions and web wallets are excluded.

## Networks and assets

- Permanent base-chain scope: Bitcoin, Monero, and Ethereum mainnet.
- Ethereum supports ETH and a small source-controlled ERC-20 allowlist.
- Initial token candidates are DAI, USDC, and USDT; inclusion requires contract and
  issuer-risk review at implementation time.
- Arbitrary token import, remote token lists, NFTs, dapps, approvals, permits, swaps,
  staking, bridging, and blind contract signing are excluded.
- One ZK rollup may later serve as a stablecoin payment rail. It must publish data to
  Ethereum, have a practical censorship or exit path, and use issuer-native allowlisted
  assets. Monoform does not provide the bridge.
- Lightning is deferred. A connector to a user-operated node is evaluated before an
  embedded node; either path requires an independent design and security gate.

## Recovery root and derivation

- Monoform wallets use one English 24-word BIP39 mnemonic with an empty BIP39
  passphrase as their sole recovery secret.
- The mnemonic must be generated offline, outside Monoform, and dedicated exclusively
  to Monoform.
- Bitcoin uses the BIP84 account `m/84'/0'/0'`; receive and change addresses append
  `/change/address_index`.
- Ethereum uses `m/44'/60'/0'/0/0`.
- Monero follows Ledger's published scheme: derive the secp256k1 child at
  `m/44'/128'/0'/0/0`, then apply the documented Monero spend-key and view-key
  conversion.
- Monero subaddresses are derived within the resulting Monero account. Monoform does
  not create additional Monero wallets by changing the BIP44 account component.
- Monoform must ship an offline, independently testable recovery path that can export
  the derived Monero private keys and a standard 25-word legacy Monero mnemonic.
- The Monero creation date or restore height is non-secret recovery metadata. Losing
  it may require scanning from genesis but must not make funds unrecoverable.

### Hardware-wallet boundary

Never import the recovery phrase of a hardware wallet that holds or has held real
funds. Entering hardware-wallet words into Monoform exposes the root secret to a
general-purpose computer and permanently removes hardware-only protection for every
account derived from it.

A hardware wallet may be used as an offline generator for a fresh 24-word BIP39 phrase
only when that phrase is created specifically for Monoform. After the phrase is entered
into Monoform, the resulting wallet is a software wallet and must never be described as
hardware-protected, even if the generating device still retains the same seed.

## Auditability

- Every feature reports its production-line and dependency delta.
- Dependency code counts toward the conceptual audit surface.
- Network and asset modules are selected at compile time; no executable logic or
  configuration is downloaded at runtime.
- Cryptographic primitives come from mature reviewed libraries and are not reimplemented.
- Tests, verification code, documentation, and explicit safety checks are never removed
  merely to reduce a line-count metric.
