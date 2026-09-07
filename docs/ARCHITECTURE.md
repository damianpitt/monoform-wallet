# Architecture

## Shape

```text
Monoform UI (Rust + egui; secret-free persistent state)
        |
        | narrow typed messages
        v
Application service (orchestration and policy)
        |
        +-- Vault (seed lifecycle, OS-backed encryption, zeroization)
        +-- Bitcoin adapter
        |     +-- On-chain
        |     +-- Lightning connector                         [later]
        +-- Monero adapter                                    [later]
        +-- EVM adapter
        |     +-- Ethereum mainnet + curated ERC-20 registry  [later]
        |     +-- One verified ZK payment network             [later]
        +-- Network clients (untrusted responses)
```

The UI may request an address, construct a transaction, display a review model, and
request user-approved signing. Secret input is moved immediately into a zeroizing vault
type and is never retained in ordinary application state. The UI never receives a
private key, raw vault record, or signing-capable object.

## Core boundaries

### Vault

The vault owns the entire secret lifecycle. It accepts one English 24-word BIP39
mnemonic only through a native, non-webview input surface; validates and normalizes
it; derives keys inside the core; encrypts persistence using an OS-backed key; and
zeroizes temporary secret memory. The BIP39 passphrase is fixed to the empty string in
the initial format. Logs and errors use opaque identifiers.

Before secret entry, the UI must state that importing words from a hardware wallet
removes hardware-only protection. The intended input is a fresh phrase generated
offline and dedicated to Monoform. See `SEED_ARCHITECTURE.md` and
`DERIVATION_AND_IMPORT.md`.

### Chain adapter

An adapter converts public wallet state into a common view model and converts an
explicit send intent into a deterministic review model. Each adapter owns its address
validation, derivation paths, fee semantics, transaction encoding, signing, broadcast,
and test vectors. Cross-chain abstractions stop at the application boundary; crypto
primitives are not forced into a misleading common type.

### Network client

All remote responses are hostile input. Network clients use fixed schemas, response
limits, timeouts, TLS, and chain-specific consistency checks. A backend can observe
requests, lie about balances or fees, omit transactions, or attempt denial of service;
it can never receive a private key or seed.

### EVM networks and assets

Ethereum and a future ZK rollup reuse address, signing, transaction-envelope, and
ERC-20 decoding logic. Network identity, finality, fees, RPC policy, native gas asset,
and contract allowlists remain explicit compile-time configuration. The transaction
review always displays the network and complete token contract identity.

Monoform never loads a token list or network module remotely. Adding a token or network is
a reviewed source change and a new signed application release. Bridge transactions,
arbitrary calldata, approvals, permits, and unknown contracts are rejected.

## Dependency policy

- Prefer mature, narrowly scoped libraries with active security maintenance.
- Do not implement cryptographic primitives.
- Pin the dependency graph and review lockfile changes.
- Deny known vulnerabilities and unexpected licenses in CI.
- No runtime code downloads, remote configuration, analytics, or third-party scripts.
- Every release includes a software bill of materials.

## Why desktop first

Rust with egui/eframe keeps the maintained application in one primary language and
avoids a webview and JavaScript-to-native command bridge. Linux and macOS are the first
release platforms. Android is deferred until desktop signing, storage, lifecycle, and
release processes are independently reviewed. Windows and iOS are not initial targets.

## Decision records required before implementation

1. Bitcoin library and backend strategy
2. OS key-store behavior and fallback policy
3. Process isolation for signing
4. Update mechanism and release-key custody
5. Reproducible build targets
6. Validation test vectors for the locked seed architecture
7. ERC-20 allowlist governance and issuer-control disclosures
8. Future ZK-rollup selection and removal criteria
9. Lightning external-node versus embedded-node boundary
