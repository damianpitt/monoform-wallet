# Seed architecture

## Locked recovery format

Monoform uses one English 24-word BIP39 mnemonic as the recovery root for Bitcoin,
Monero, and Ethereum. The initial wallet format fixes the BIP39 passphrase to the empty
string. The phrase is the only secret a user must preserve.

BIP39 converts the 24 words into a 512-bit seed. Monoform then applies fixed,
chain-specific derivation rules:

| Chain | Derivation |
|---|---|
| Bitcoin | BIP84 account `m/84'/0'/0'`, then `/change/address_index` |
| Ethereum | BIP44 address `m/44'/60'/0'/0/0` |
| Monero | Ledger-compatible account `m/44'/128'/0'/0/0` |

Paths are part of the wallet format. They cannot change silently between releases.
Every implementation must pass published, cross-implementation test vectors before it
may accept a real mnemonic.

The current offline seed core validates the English 24-word format and derives the
BIP39 seed without a UI or storage path. It zeroizes owned inputs, the mnemonic's word
indices, and the returned seed on drop. This is best-effort memory hygiene, not a claim
that library internals, compiler-created copies, swap, or crash dumps are fully erased.
Real-phrase import remains disabled pending the vault and independent review.

See `DERIVATION_AND_IMPORT.md` for chain discovery, interoperability, and the import
ceremony.

## Monero conversion

A native Monero mnemonic is not a BIP39 HD-wallet root. Monoform follows the published
Ledger Monero scheme instead of inventing a new conversion:

1. Derive the secp256k1 BIP32 private child at `m/44'/128'/0'/0/0`.
2. Apply Keccak-256 and reduce the result modulo the edwards25519 scalar order to obtain
   the Monero private spend key.
3. Apply Keccak-256 and scalar reduction to the private spend key to obtain the private
   view key.
4. Derive the public spend key, public view key, primary address, and Monero
   subaddresses using Monero's standard rules.

Monoform uses Monero subaddresses within this account. It does not change the BIP44
account component to model Monero accounts.

Reference implementation:
[`LedgerHQ/app-monero/src/monero_init.c`](https://github.com/LedgerHQ/app-monero/blob/develop/src/monero_init.c#L845-L922)

## Independent recovery

The 24 BIP39 words are not accepted directly by the normal mnemonic-restore flow in
official Monero software. Monoform must therefore provide a small offline recovery path
that can reproduce and display:

- The Monero private spend key and private view key.
- The public keys and primary address.
- A standard 25-word legacy Monero mnemonic encoding the derived spend key.
- The fixed derivation path and implementation version.

The recovery implementation and test vectors must be usable without a network
connection. This ensures that loss or abandonment of Monoform cannot trap funds.

BIP39 does not encode a Monero wallet birthday. Monoform records the creation date and
estimated restore height as non-secret metadata and includes them in backup guidance.
If that metadata is lost, recovery must remain possible by scanning from genesis.

## Hardware-wallet boundary

> **Never import the recovery phrase of a hardware wallet that holds or has held real
> funds. Entering hardware-wallet words into Monoform exposes the seed to a
> general-purpose computer and permanently removes hardware-only protection.**

Monoform is intended to receive a fresh phrase generated offline and dedicated to
Monoform. A hardware wallet may be used to generate and display a fresh valid 24-word
BIP39 phrase. In that workflow the hardware device acts as an offline seed generator,
not as a continuing security boundary.

Once the phrase is entered into Monoform:

- Treat every account derived from it as software-wallet funds.
- Do not reuse the phrase for a hardware-only wallet.
- Do not claim that deleting Monoform restores hardware isolation.
- Do not enter the phrase in a browser, website, issue report, test fixture, log, or
  support conversation.

The import ceremony must explain this boundary before showing the secret-entry field
and require explicit acknowledgement. Alpha builds accept marked test mnemonics only;
mainnet import remains gated on independent review.

## Standards and references

- [BIP39: Mnemonic code for generating deterministic keys](https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki)
- [BIP84: Derivation scheme for native SegWit accounts](https://github.com/bitcoin/bips/blob/master/bip-0084.mediawiki)
- [SLIP-0044 registered coin types](https://github.com/satoshilabs/slips/blob/master/slip-0044.md)
- [Ledger Monero application](https://github.com/LedgerHQ/app-monero)
- [Monero mnemonic formats](https://docs.getmonero.org/mnemonics/)
- [Monero CLI recovery options](https://docs.getmonero.org/interacting/monero-wallet-cli-reference/)
