# Monoform Wallet

> Sovereign Money, One form, Three Chains

Monoform Wallet is an early, security-first open-source project. It rejects swaps,
cards, yield, ads, accounts, telemetry, KYC, and financial clutter. The product has
three verbs: **receive, send, verify**.

This repository currently contains the product foundation, a runnable non-signing Rust
interface, and the original web interface study. **It is not a wallet yet. Do not enter
a seed phrase or use it with real funds.**

## Product principles

1. Self-custody is the product, not a feature.
2. Nothing secret leaves the device.
3. No seed phrase is ever accepted by a web page.
4. Every outgoing transaction is understandable before it is signed.
5. No trackers, analytics, remote configuration, or sponsored assets.
6. A small audited surface beats a broad unaudited one.
7. Mainnet remains disabled until the relevant core has independent review.
8. Application code and dependency growth are reported and justified.

## One seed, three chains

Monoform is designed around one dedicated English 24-word BIP39 recovery phrase for
Bitcoin, Monero, and Ethereum. Bitcoin follows BIP84, Ethereum follows BIP44, and
Monero follows Ledger's published derivation at `m/44'/128'/0'/0/0`. The exact
derivation and independent recovery requirements are specified in
[Seed architecture](docs/SEED_ARCHITECTURE.md) and
[Chain derivation and import strategy](docs/DERIVATION_AND_IMPORT.md).

> **Hardware-wallet boundary:** Never import the recovery phrase of a hardware wallet
> that holds or has held real funds. Entering its words into Monoform exposes the seed
> to a general-purpose computer and permanently removes hardware-only protection. A
> hardware device may be used offline to generate a fresh 24-word BIP39 phrase only if
> that phrase is dedicated to Monoform and is never represented as hardware-protected
> after import.

## Scope

| Phase | Capability | Network |
|---|---|---|
| 0 | Runnable Rust UI shell; no keys, signing, storage, or network calls | Demo data |
| 1 | Receive, send, balance, history; software signer | Bitcoin testnet |
| 2 | Reproducible desktop releases and external security review | Bitcoin mainnet |
| 3 | Monero adapter and independent review | Stagenet, then mainnet |
| 4 | ETH and a source-controlled ERC-20 allowlist | Ethereum testnet, then mainnet |
| 5 | One verified ZK stablecoin rail; no bridge | Testnet, then mainnet |
| 6 | Optional Lightning integration | Test networks, then mainnet |

The implementation uses Rust with egui/eframe. Linux and macOS are the first supported
platforms; Android follows only after the desktop core is proven. Chain logic is isolated
behind narrow compile-time adapters. See [Architecture](docs/ARCHITECTURE.md),
[Threat model](docs/THREAT_MODEL.md), [Locked decisions](docs/LOCKED_DECISIONS.md),
and [Roadmap](ROADMAP.md).

Monoform supports Bitcoin, Monero, and Ethereum mainnet. Ethereum assets are limited to
reviewed ERC-20 contracts committed in source. It does not import arbitrary tokens,
load remote token lists, bridge assets, or sign unknown contract calls.

## Run the Rust interface

Install Rust 1.98.1 with `rustup`, then run:

```sh
cargo run --locked
```

The only direct application dependency is eframe 0.36.1, configured with its smaller
Glow renderer and native accessibility support. `Cargo.lock` fixes the complete
dependency graph.

## View the original interface study

Open `prototype/index.html` locally. It is dependency-free and uses fixed demo data.
The prototype deliberately contains no seed field and makes no network requests.

## Project status

**Pre-alpha / design validation.** We welcome critique of the scope, threat model,
accessibility, and transaction-review flow. We are not yet accepting production
cryptography contributions.

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a change. Please report
security concerns privately as described in [SECURITY.md](SECURITY.md); do not open
a public issue for a suspected vulnerability.

## License

Copyright (C) 2026 Monoform Wallet contributors. Licensed under the GNU General
Public License v3.0 or later. See [LICENSE](LICENSE).
