# OS-backed public-state storage — version 1

## Scope and platform choices

This is an offline, **public Bitcoin testnet4 state** store. It persists an account
key, origin fingerprint, and address counters, never a mnemonic, BIP39 seed, private
key, or signing object. The demonstration UI does not call it. No network is added.

| Platform | Credential provider | Policy |
|---|---|---|
| macOS | Local user/login Keychain, native generic credential | No iCloud synchronization requested; no Secure Enclave or biometric guarantee |
| Linux | Persistent Secret Service via the session D-Bus | Encrypted DH transport; requires a configured persistent provider such as GNOME Keyring |
| Other targets | No storage module | No mock, plaintext, transient kernel-keyring, or password fallback |

The pinned [`keyring` native providers](https://docs.rs/keyring/3.6.3/keyring/) supply
credential storage. We instantiate the compiled native builder explicitly rather than
using the globally replaceable default. Only tests may inject an in-memory credential.
Keychain/Secret Service can request OS authorization or unlock; denied requests and
unavailable services return an opaque error. We do not create a passwordless collection,
disable OS protection, or interpret service failure as an absent key. The provider's
collection/password/access-control configuration remains part of the OS trust boundary.
Linux does not promise per-application isolation from other applications in the same
unlocked user session. Neither platform defeats malware running as the user.

The caller chooses an absolute path in trusted **local**, per-user application storage.
The future application locations are `~/Library/Application Support/Monoform/bitcoin-testnet4`
on macOS and `$XDG_DATA_HOME/monoform/bitcoin-testnet4` on Linux, with the normal
`~/.local/share` default. Path resolution is not yet wired into the UI; no automatic
cloud, shared-drive, or network-filesystem storage is supported. Parent directories
must already exist and be trusted. Creation makes only the final directory.

Require directory mode `0700`, file mode `0600`, current-user ownership, regular
single-link files, and no final-component symlinks. Reject insecure existing paths
rather than silently changing their permissions. Files are opened relative to the held
directory descriptor; later path substitutions cannot redirect its I/O. The caller
must still trust parent components during initial opening. Keep the named lock file:
deleting/replacing it undermines cooperative locking.

## Minimal public API

- `PublicStateStore::create(path, account)`: explicit initialization; refuses existing
  encrypted state or an existing OS credential. It never silently resets either.
- `PublicStateStore::open(path)`: verifies and, when necessary, finalizes an interrupted
  commit. Missing/unavailable keys or mismatched files fail closed.
- `descriptors()`: returns the persisted account's public watch-only descriptors.
- `reserve_receive_address()` / `reserve_change_address()`: derive one address,
  persist its advanced counter, and return the address **only after commit succeeds**.

The exclusive nonblocking OS file lock is held for the store's entire lifetime.
Another instance/process gets `Busy`; dropping the store releases the lock. There is
no general save/reset method capable of silently replacing the account or lowering
counters. An application must use reservation, not the standalone in-memory address
methods, when issuing persisted-wallet addresses. Discovery/recovery reconciliation
and account verification against a future signing vault are still separate work.

## Encrypted file and OS credential

`public-state.mfs` is at most 300 bytes:

```text
MFS1 (4 bytes) | random nonce (24 bytes) | encrypted v1 text record | tag (16 bytes)
```

[`XChaCha20Poly1305`](https://docs.rs/chacha20poly1305/0.10.1/chacha20poly1305/)
comes from RustCrypto, not a Monoform primitive. A fresh OS-random 32-byte key is made
only on explicit creation; every write gets a fresh random 24-byte nonce. Associated
data is `MFS1` followed by SHA256 of the canonical directory-path bytes. This binds
the file to its version and location. A folder move is not a supported migration.
Keys and transient plaintext are guarded with `Zeroizing`; this does not certify that
OS providers, allocator internals, swap, or all upstream temporary copies are wiped.

One 96-byte binary credential stores:

```text
key (32 bytes) | committed ciphertext SHA256 (32 bytes) | pending ciphertext SHA256 (32 bytes)
```

The service name is `org.monoform.wallet.public-state.v1`; the credential's account
name is the hexadecimal directory-path hash, not an xpub or address. An all-zero
digest denotes no committed/pending file. No key is placed in a file or derived from
the recovery phrase. OS write acknowledgement plus an exact read-back is required.

## Commit and interruption recovery

1. Load and authenticate the current file against its OS marker while holding the lock.
2. Derive an address and advance its counter locally; do not expose the address yet.
3. Write encrypted bytes to `public-state.pending` with exclusive creation. Flush
   the file and directory. Only a prior bounded, private staging artifact may be removed.
4. Record the pending file's hash in the OS credential, retaining the prior committed hash.
5. Atomically rename the staging file over the final file and flush the directory.
6. Finalize the OS committed hash and clear the pending hash; verify the OS read-back.
7. Only now return the address. Any failure returns an opaque error, not the address.

| State on reopening | Recovery |
|---|---|
| No pending marker; final file matches committed hash | Authenticate and load normally |
| Pending marker; final file still matches prior committed hash | Keep old state, flush it, clear pending marker; no new address had been returned |
| Pending marker; final file matches pending hash | Keep new state, flush it, finalize marker; an unreturned address may be conservatively skipped |
| Interrupted initial creation, no final file, authenticated staged file matches pending marker | Install the staged file, flush, and finalize |
| Missing credential or file, wrong hash/key/version, malformed state | Fail closed; never guess, regenerate keys, or reset counters |

If initial creation fails before any OS marker is installed, explicit creation may
be retried only while no final state or OS credential exists. Opening never creates
a new key. A failed finalization may already have installed the new file; do not
assume that every error left the old filename unchanged. The marker protocol preserves
a recoverable valid state and prevents returning an uncertain reservation.

## Limits and recovery obligations

This detects **file-only** rollback relative to the surviving OS credential. It does
not detect a coordinated restore of both file and credential, OS-store tampering,
malicious same-user writes, or a compromised OS. Keychain/Secret Service acknowledge
updates but expose no hardware monotonic counter or cross-filesystem transaction.
Read-back and filesystem flush requests cannot prove physical power-loss durability
of the credential daemon, drive cache, or filesystem. No stronger guarantee is claimed.

A seed backup can recover funds through future chain discovery, but cannot recover
unused issued-address counters from the blockchain. Arbitrary stale-file restoration,
OS-key deletion, cross-machine migration, and export are deliberately unsupported.
Never delete/reset an existing store to bypass a mismatch. Stop issuance and use a
future reviewed recovery flow that preserves known high-water marks and checks both
branches. No user-facing recovery flow is implemented in this checkpoint.

## Verification and remaining release gates

Deterministic tests cover encrypted reopen/resume, both branches, credential failures
before/after replacement, staged interruption recovery, partial staging, stale files,
corruption/truncation/size bounds, wrong key/version/location, unsafe permissions,
symlinks/hardlinks, exhaustion, malformed credentials, and actual process lock contention.
They use only public fixtures and isolated temporary directories.

An opt-in native smoke test creates and deletes exactly one temporary OS credential:

```sh
cargo test --locked --lib storage::tests::native_store_round_trip -- --ignored --exact
```

CI runs the suite on macOS and Linux, including native Keychain and an isolated Linux
Secret Service session. This is not a real-wallet recovery drill. Packaged-app signing
identity/Keychain authorization across upgrades, Linux collection lock/reboot behavior,
disk-full/flush/physical-power-loss testing, dependency security/license checks, two
maintainer approvals, and independent storage review remain release gates. The seed
vault is still unimplemented; no real funds or secret-entry UI are enabled.

## Audit-surface cost of this checkpoint

The storage module is 371 maintained production lines including comments and formatting;
its fault/recovery tests are kept separately. Four runtime dependencies are newly direct:
`keyring`, `chacha20poly1305`, `getrandom`, and `rustix`; `tempfile` is test-only.
`getrandom`, `rustix`, and `tempfile` reuse versions already in the dependency graph.
The all-target lockfile grows from 408 to 450 packages (+42), without upgrading existing
locked package versions. Native credential and Secret Service transport dependencies
are a real additional audit surface, not hidden behind a small wrapper. Platform-native
bindings and established-library encryption are preferable to duplicating OS/crypto code;
independent dependency and application review is still required.

## Other wallets: inspiration, not inherited security

[Cake's generated secure-storage layer](https://github.com/cake-tech/cake_wallet/blob/dev/tool/configure.dart)
wraps FlutterSecureStorage, and its
[key service](https://github.com/cake-tech/cake_wallet/blob/dev/lib/core/key_service.dart)
stores wallet passwords separately from wallet content. Its current
[shared file helper](https://github.com/cake-tech/cake_wallet/blob/dev/cw_core/lib/utils/file.dart)
uses XChaCha20 encryption through `cake_backup`, a flushed temporary file, and rename.
These are observations of the current development source, not a claim that every
released chain engine uses identical storage. Platform-specific and legacy migration
branches make direct copying inappropriate for Monoform's fresh, single-format core.

[Exodus's desktop documentation](https://www.exodus.com/support/en/articles/8598609-getting-started-with-exodus)
describes local wallet files protected by the user's password. Its
[source policy](https://www.exodus.com/support/en/articles/8598678-is-exodus-open-source)
says parts remain closed-source, despite published open-source components. We cannot
infer its complete desktop cipher, KDF, OS-store, or crash-consistency implementation
from those pages. Monoform adopts the separation and atomic-write pattern, not another
wallet's security claims, codebase, backup policy, or UI framework.
