# Phase 3 preparation — Monero

Phase 3 adds Monero without weakening Monoform's small, auditable core. Planning and
public test-vector work may happen earlier, but wallet integration does not bypass the
Bitcoin release gates in Phases 1 and 2.

The only production target is a standard Monero account derived from Monoform's
dedicated 24-word BIP39 root. Stagenet comes first. Mainnet remains absent until a
Monero-specific external review approves the complete derivation, scanning, signing,
recovery, and release path.

## Fixed scope

Phase 3 supports:

- Ledger-compatible derivation at `m/44'/128'/0'/0/0`;
- one primary account and standard Monero subaddresses;
- local balance and history discovery;
- receive and single-recipient transfer;
- deterministic transaction review before signing;
- offline export of the account keys and equivalent 25-word legacy mnemonic; and
- an explicitly configured user-operated daemon.

Phase 3 does not support multisig, mining, merchant RPC, integrated addresses, payment
IDs, multiple recipient transfers, sweeping, remote scanning services, automatic
public-node discovery, or hardware-wallet control. Those features require separate
designs and security reviews.

## Non-negotiable boundaries

1. The mnemonic, BIP39 seed, private spend key, private view key, and transaction secret
   material never enter UI state, logs, crash reports, network requests, or error text.
2. The view key remains local. Monoform scans data obtained from a daemon; it does not
   outsource wallet scanning to a service.
3. The daemon is untrusted. It may observe requests, omit or reorder data, lie about
   fees or chain state, or attempt denial of service. It never receives signing
   authority.
4. All amounts use integer atomic units. Floating-point values are presentation only.
5. Monoform does not implement Keccak, scalar arithmetic, edwards25519 operations,
   RingCT, or transaction proofs from scratch.
6. Mainnet activation and endpoints are excluded from the stagenet implementation
   build until its release gate is complete. Network constants may exist for offline
   address-vector tests.

## Proposed module boundary

The Monero adapter will expose narrow typed operations to the application layer:

```text
MoneroAdapter
    derive_public_account(vault_handle)
    receive_address(subaddress_index)
    sync(sync_cursor, daemon)
    prepare_transfer(intent, daemon) -> ReviewModel
    sign(approved_review, vault_handle) -> SignedTransaction
    broadcast(signed_transaction, daemon) -> TransactionId
    export_recovery(vault_handle, offline_destination)
```

Internally, derivation and signing may use secret-bearing types. These types must not
implement debug formatting, serialization, cloning, equality, or conversion to ordinary
strings unless a reviewed protocol requires it. Network responses and UI models remain
separate from secret-bearing types.

No empty source-module hierarchy will be added in advance. Each module enters the tree
with tests and a concrete responsibility so that structural code does not disguise the
real audit surface.

## Work packages

### M0 — Dependency and vector decision

- Record the chosen Monero and cryptographic libraries in an architecture decision.
- Reject unpinned Git dependencies and libraries without an acceptable license.
- Report direct and resolved dependency growth before approval.
- Document any transitive `unsafe` code and native build requirements.
- Freeze public Ledger-compatible derivation vectors for mainnet and stagenet addresses.
- Validate every vector with two independent implementations, including Ledger's
  published implementation as one reference.

Exit gate: two maintainers approve the dependency report and public vectors. No secret
input or network code exists.

### M1 — Offline derivation and recovery

- Accept marked test mnemonics through a test-only interface.
- Derive the BIP32 child and Monero spend/view keys behind the vault boundary.
- Encode primary and subaddresses for the selected network.
- Export the private keys and equivalent standard 25-word legacy mnemonic through a
  separate offline command.
- Prove that exported recovery material recreates the same address in independent
  Monero tooling.
- Add zeroization and secret-redaction tests.

Exit gate: all golden vectors and recovery round trips pass offline on Linux and macOS.
The graphical application still has no seed-entry field.

### M2 — View-only stagenet synchronization

- Connect only to a node explicitly configured by the user.
- Verify network identity and reject mainnet/testnet mismatches.
- Scan locally for owned outputs, subaddresses, spends, unlock state, and reorgs.
- Persist only encrypted wallet state and non-secret restore metadata.
- Support interruption, resume, full rescan, corrupted-cache recovery, and scan from
  genesis when restore height is unavailable.
- Define bounded responses, timeouts, retry behavior, and maximum resource use.

Exit gate: deterministic stagenet fixtures cover normal sync, malicious responses,
reorganizations, interruption, and cache corruption. Balance and history are verified
against independent Monero tooling.

### M3 — Stagenet transfer

- Accept one standard primary address or subaddress and one amount.
- Reject integrated addresses, payment IDs, unknown networks, unsupported URI fields,
  multiple recipients, and noncanonical encodings.
- Build transactions using reviewed Monero primitives and current consensus rules.
- Present recipient, full address, amount, fee, change behavior, unlock constraints,
  node identity, and final total before signing.
- Bind approval to the exact transaction digest so any post-review mutation fails.
- Broadcast only after local verification of the signed transaction.

Exit gate: positive and adversarial stagenet tests pass, including mutated review data,
malicious daemon responses, fee manipulation, dust-like edge cases, and reorg recovery.

### M4 — Review and mainnet gate

- Complete independent cryptography, privacy, application-security, and supply-chain
  reviews.
- Reproduce release artifacts and publish a signed software bill of materials.
- Run recovery drills using only the 24 words and separately using the exported
  25-word Monero mnemonic.
- Resolve every high or critical finding and retest the fixes.
- Require two maintainers to approve the exact release commit and artifacts.

Only M4 may introduce the mainnet build configuration. It is a release decision, not a
runtime toggle.

## Node and privacy policy

Monoform defaults to a user-operated Monero daemon. It contains no sponsored nodes,
hard-coded public-node list, automatic node discovery, or silent fallback. A remote
daemon operated by someone else can correlate the user's IP address, timing, requested
block ranges, broadcasts, and chain tip; transport privacy does not make that daemon
trusted.

Support for an explicitly entered third-party remote daemon is deferred. If proposed,
it requires a privacy design covering TLS or onion authentication, proxy behavior,
request fingerprinting, failover, node rotation, and clear user disclosure.

`monero-wallet-cli`, `monero-wallet-rpc`, and Ledger are compatibility oracles in the
test environment. Monoform does not ship them as hidden sidecars or cross its secret
boundary through their RPC interfaces.

## Required test corpus

- BIP39 seed and BIP32 child vectors for the fixed path.
- Ledger-compatible private spend key, private view key, and public address vectors.
- Mainnet, stagenet, primary-address, and subaddress encoding vectors.
- Standard 25-word legacy mnemonic checksum and round-trip vectors.
- Valid, malformed, wrong-network, integrated, and noncanonical address cases.
- Owned and unrelated outputs, spent outputs, key images, locked funds, and change.
- Reorganizations, duplicate data, missing data, stale tips, oversized responses, and
  invalid proofs from a hostile daemon fixture.
- Transaction construction, review binding, signing, serialization, and independent
  decode checks.
- Secret-redaction, memory-zeroization, crash-path, and corrupted-state tests.

All fixtures use published vectors or marked test-only material. Real wallet data is
never accepted into the repository.

## Decisions still required

Phase 3 implementation cannot begin until maintainers approve:

1. The Rust wallet-engine and cryptographic dependency set.
2. The daemon protocol subset and version-compatibility policy.
3. The encrypted scan-cache format and migration rules.
4. Subaddress look-ahead and discovery limits.
5. Fee, transaction-weight, and unlock-state review semantics.
6. Release-time separation between stagenet and mainnet builds.
7. The independent reviewers and acceptance criteria.

## Primary references

- [Ledger Monero key initialization](https://github.com/LedgerHQ/app-monero/blob/develop/src/monero_init.c#L845-L922)
- [Monero core wallet implementation](https://github.com/monero-project/monero/tree/master/src/wallet)
- [Monero core tests](https://github.com/monero-project/monero/tree/master/tests)
- [Monero developer guides](https://www.getmonero.org/resources/developer-guides/)
- [Monero wallet CLI reference](https://docs.getmonero.org/interacting/monero-wallet-cli-reference/)
