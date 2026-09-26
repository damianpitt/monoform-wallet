# Chain derivation and import strategy

This document explains how Monoform turns one dedicated 24-word BIP39 phrase into the
three supported chain accounts and how a user can recover each account outside
Monoform. The cryptographic rules in `SEED_ARCHITECTURE.md` are normative; this page
defines import behavior, discovery, and interoperability.

## Input contract

The initial wallet format accepts exactly one English 24-word BIP39 mnemonic. Monoform
validates the word list and checksum, applies BIP39 NFKD normalization, and derives the
512-bit BIP39 seed with an empty passphrase. Custom word counts, custom derivation
paths, native Monero mnemonics, raw private keys, and BIP39 passphrases are outside the
initial import flow.

The phrase must be fresh, generated offline, and dedicated to Monoform. A hardware
wallet may generate it, but entering the words into Monoform makes every derived
account software-accessible. Never import a hardware-wallet phrase that holds or has
held real funds.

## Fixed derivations

| Chain | Root or address path | Initial discovery rule |
|---|---|---|
| Bitcoin | `m/84'/0'/0'` | Scan BIP84 external and change branches with a documented gap limit |
| Ethereum | `m/44'/60'/0'/0/0` | One externally owned account; allowlisted ERC-20 assets reuse this address |
| Monero | `m/44'/128'/0'/0/0` | One Ledger-compatible primary account; discover standard Monero subaddresses |

Monoform does not expose editable paths. A future path change requires a versioned
wallet format, migration design, public test vectors, and governance approval.

## Bitcoin import

Monoform derives the native SegWit account at `m/84'/0'/0'`. Receiving addresses use
`m/84'/0'/0'/0/index`; change addresses use `m/84'/0'/0'/1/index`.

The offline pre-alpha testnet account uses coin type `1`. It removes private derivation
material before retaining public receive and change branches, then advances separate
in-memory cursors for `m/84'/1'/0'/0/index` and `m/84'/1'/0'/1/index`. Its addresses
begin with `tb1`. This does not change the locked mainnet recovery path above. There
is no descriptor persistence, network connection, signing, storage, or user-facing
phrase import yet.

Import performs deterministic address discovery from index zero. The gap limit and
backend strategy must be fixed before testnet import is enabled. Recovery in another
BIP39/BIP84 wallet requires the same phrase, account path, and discovery of both
branches. Monoform will document the final gap limit beside its test vectors.

The initial format does not scan legacy BIP44, nested SegWit BIP49, or Taproot BIP86
accounts. Funds held on those paths are not Monoform funds and will not appear.

## Ethereum import

Monoform derives one Ethereum externally owned account at `m/44'/60'/0'/0/0`. ETH and
every supported ERC-20 token share this address. Tokens are identified by exact
chain ID and contract address from the source-controlled allowlist; seed import never
discovers or enables arbitrary tokens.

Recovery in another compatible Ethereum wallet requires selecting the same BIP44
address path. Monoform does not scan alternate Ledger Live, legacy, or user-defined
paths in the initial format.

## Monero import

Monoform follows Ledger's published conversion rather than treating BIP39 words as a
native Monero mnemonic:

1. Derive the secp256k1 BIP32 private child at `m/44'/128'/0'/0/0`.
2. Apply Keccak-256 and scalar reduction to obtain the Monero private spend key.
3. Apply Keccak-256 and scalar reduction again to obtain the private view key.
4. Derive the public keys, primary address, and standard Monero subaddresses.

The wallet creation date and estimated restore height are stored as non-secret
metadata. If both are lost, a scan from genesis remains the recovery fallback.

Official Monero software does not accept the 24 BIP39 words in its normal mnemonic
restore field. Monoform must therefore ship an offline recovery command that exports
the private spend key, private view key, primary address, and an equivalent standard
25-word legacy Monero mnemonic. Those outputs allow recovery without Monoform.

## Recovery matrix

| Asset | Recover using only the 24 words | Portable recovery route |
|---|---|---|
| BTC | Yes | Any compatible BIP39/BIP84 wallet using account `m/84'/0'/0'` |
| ETH and allowed ERC-20s | Yes | Any compatible Ethereum wallet using `m/44'/60'/0'/0/0` |
| XMR | Yes | Monoform-compatible derivation, a Ledger using the same root, or offline export to standard Monero keys/25 words |

Restore date or height improves Monero scanning speed but is not required to regain
control. Token allowlist data is public metadata and can be reconstructed from the
corresponding Monoform release.

## Import implementation gates

Secret entry remains disabled until all of the following are complete:

- Official BIP39 vectors validate parsing, normalization, checksum, and seed output.
- Bitcoin and Ethereum addresses match independent wallet implementations.
- Monero keys and addresses match Ledger-derived public vectors.
- The offline Monero recovery command reproduces the same account from its exported
  legacy mnemonic and keys.
- Temporary mnemonic and seed buffers are zeroized and excluded from logs and errors.
- The hardware-wallet boundary is acknowledged before the secret-entry field appears.
- Testnet recovery drills pass on Linux and macOS.
- Independent review approves the relevant chain before mainnet is enabled.

## References

- [BIP39](https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki)
- [BIP84](https://github.com/bitcoin/bips/blob/master/bip-0084.mediawiki)
- [BIP44](https://github.com/bitcoin/bips/blob/master/bip-0044.mediawiki)
- [SLIP-0044 coin types](https://github.com/satoshilabs/slips/blob/master/slip-0044.md)
- [Ledger Monero key initialization](https://github.com/LedgerHQ/app-monero/blob/develop/src/monero_init.c#L845-L922)
- [Monero mnemonic formats](https://docs.getmonero.org/mnemonics/)
- [Monero wallet CLI recovery](https://docs.getmonero.org/interacting/monero-wallet-cli-reference/)
