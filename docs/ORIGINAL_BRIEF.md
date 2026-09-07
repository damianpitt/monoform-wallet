# Original brief, normalized

Build a complete open-source wallet with as little code and complexity as practical.

Candidate networks and assets:

- Bitcoin and possibly Litecoin
- Monero
- Ethereum mainnet and a deliberately small set of L2 networks
- Solana and selected SPL tokens

Choose only a few assets and genuinely decentralized base layers. Do not pursue
corporate growth mechanics. Never add fiat services, cards, KYC, swaps, or unrelated
financial features.

The interface should be beautiful, monochrome, and clean, with at most one accent
color. Its essential capabilities are importing an existing 24-word BIP39 recovery
phrase, sending, receiving, and viewing assets. Seed generation may be considered
later. Initial platforms were left open between Android, Linux, and macOS.

## Safety amendment

The project will never ask users to import a hardware-wallet phrase that holds or has
held real funds. Monoform is designed for a fresh 24-word BIP39 phrase generated
offline and dedicated to Monoform. A hardware device may generate it, but entering the
phrase into software permanently removes hardware-only protection.

The interface prototype has no secret entry. Testnet restore comes only after the
native vault boundary is implemented; mainnet import comes only after external review.
Monero follows the locked Ledger-compatible derivation and must provide an independent
offline export into standard Monero recovery material.
