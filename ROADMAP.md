# Roadmap

Dates are intentionally absent. Security gates, not launch pressure, advance a phase.

## Phase 0 — Foundation

- [x] Product charter and explicit non-goals
- [x] Threat model and security reporting policy
- [x] Chain-adapter architecture
- [x] Non-signing monochrome interface prototype
- [x] Product name and philosophy: Monoform Wallet / The Monochrome Principle
- [x] Platform decision: Rust with egui/eframe; Linux and macOS first
- [x] Network policy: BTC, XMR, Ethereum mainnet, curated ERC-20 assets
- [x] One-seed policy: dedicated 24-word BIP39 root with fixed chain derivations
- [ ] Formal namespace and trademark clearance
- [ ] Initial maintainers and governance sign-off
- [x] CI for Rust formatting, compilation, linting, and tests
- [x] Dependency policy and pinned Cargo resolution
- [ ] Signed commits and tags policy

## Phase 1 — Bitcoin testnet alpha

- [x] Rust workspace and demonstration-only egui/eframe desktop shell
- [x] Offline BIP39 English 24-word validation with public vectors and empty passphrase
- [x] Test-only BIP39 compatibility checks using public vectors (no import path)
- [x] Offline BIP84 testnet receive-address derivation with published and independent vectors
- [ ] Complete BIP84 branches, account descriptors, and wallet state
- [ ] Bitcoin Core-compatible testnet backend
- [ ] Receive, coin-aware send, fee selection, and transaction review
- [ ] Encrypted local keystore using operating-system facilities
- [ ] Memory-zeroization tests and secret-redaction tests
- [ ] Linux and macOS reproducible build path

Exit gate: test vectors pass, no mainnet endpoints exist, threat-model review is
complete, and two maintainers approve the signing path.

## Phase 2 — Bitcoin mainnet candidate

- [ ] Independent cryptography and application-security review
- [ ] Reproducible release artifacts and signed SBOM
- [ ] Hardware-wallet and watch-only evaluation
- [ ] Recovery drill and corrupted-storage tests
- [ ] Public beta with conservative value limits communicated clearly

Exit gate: all high/critical findings fixed and re-tested; release signed by two
maintainers.

## Phase 3 — Monero

Preparation and release gates are defined in
[`docs/PHASE_3_MONERO.md`](docs/PHASE_3_MONERO.md). Planning may proceed early; Monero
wallet integration does not bypass the Phase 1 and Phase 2 exit gates.

- [ ] Implement Ledger-compatible BIP39 derivation at `m/44'/128'/0'/0/0`
- [ ] Export private keys and a standard 25-word legacy mnemonic for offline recovery
- [ ] Test recovery against Ledger-derived vectors and independent Monero tooling
- [ ] Implement stagenet balance, history, receive, transfer, and scanning
- [ ] Complete privacy, restore, and independent security reviews
- [ ] Enable mainnet only after its dedicated release gate

## Phase 4 — Ethereum and selected assets

- [ ] Implement ETH balance, history, receive, EIP-1559 transfer, and review
- [ ] Add a source-controlled ERC-20 registry with exact mainnet contracts
- [ ] Initially evaluate DAI, USDC, and USDT
- [ ] Support only balance, Transfer history, and ordinary token transfers
- [ ] Display issuer control and contract risk without euphemism
- [ ] Exclude approvals, permits, arbitrary tokens, NFTs, dapps, and blind signing

## Phase 5 — One verified ZK payment rail

- [ ] Evaluate current rollups only after Ethereum support is stable
- [ ] Require Ethereum data availability and a practical censorship/exit path
- [ ] Require issuer-native stablecoin contracts and transparent upgrade control
- [ ] Reuse the EVM adapter through a compile-time network module
- [ ] Exclude all bridge operations from Monoform

## Phase 6 — Lightning evaluation

- [ ] Prototype connection to a user-operated Lightning node
- [ ] Measure code, dependency, persistence, backup, liquidity, and privacy costs
- [ ] Consider embedded LDK only through a separate design and security review

Every adapter receives its own design proposal, test-vector suite, privacy review, and
independent release gate. No adapter or network inherits approval from another.
