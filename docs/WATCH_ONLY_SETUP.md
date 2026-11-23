# Reviewed watch-only account setup — 2026-11-21

The macOS/Linux desktop can explicitly create protected Bitcoin testnet4 state
from the existing [Monoform version-1 public export](BITCOIN_PUBLIC_STATE.md).
It imports public account material and saved counters, never a recovery phrase,
private key or signing object. No new export dialect or descriptor importer is added.

```sh
cargo run --locked --features desktop,core-rpc
```

Both features are needed for this edition; setup itself makes no network requests
and needs no running Core node. Default and `desktop`-only builds remain offline.

## Review, then create

1. Select **Set up a watch-only account**. Enter an absolute destination account
   folder and the absolute path to your public export file. Parent directories must
   already exist in trusted, private local storage. Creation makes only the final
   destination folder, or uses an existing private empty folder.
2. Choose **Review public export**. Only the source is read; this step does not
   create a destination, request an OS credential or contact a node.
3. Compare the complete displayed record, including the full account key and next
   receive/change counters, with your trusted source. The screen identifies testnet4
   and BIP84 account 0 at `m/84'/1'/0'`. A checksum detects errors; it cannot prove
   ownership, ancestry or freshness. Old counters may reuse previously issued addresses.
4. Explicitly acknowledge verification, then choose **Create protected account**.
   The worker reparses and saves that exact reviewed snapshot. Later edits to the
   source file cannot substitute another key or reset the approved counters.
5. Success returns to Open account without connection consent or a balance. Choose
   the local node folder and explicitly allow Refresh to discover history. Import
   alone does not authorize address issuance or establish completeness/spendability.

Editing either setup path, switching modes, reviewing again or clearing the screen
discards prior review/approval. Inputs are non-interactive while the single worker
runs; no overlapping actions or automatic retries are added. Paths, reviews, approval
and balances are not remembered across launches. Reviewed public text is selectable
and privacy-sensitive on screen and through clipboard/accessibility services.

## File and storage boundary

The source must be a current-user-owned, regular, single-link file with mode `0600`.
Final symlinks, hard links, directories and FIFOs are rejected. A nonblocking,
close-on-exec file descriptor is opened before its metadata is checked. Trusted
parent components and same-user OS access remain boundaries.

At most 257 bytes are read to distinguish records larger than the codec's 256-byte
limit. Input must be the exact canonical ASCII record, without a newline, BOM,
whitespace repair, unknown version/network, private/mainnet key or invalid counters.
The existing codec performs all key/checksum/format validation. Only its regenerated
public record reaches the UI. The bounded raw buffer is preallocated and zeroized
on rejection or success, including accidental selection of a secret file; this
does not promise erasure of OS/library buffers. Never intentionally select secrets.
The source export remains plaintext; encrypting the imported copy does not protect
that original file. Keep it private. The app never deletes or rewrites it.

Only confirmed creation invokes `PublicStateStore::create` inside the worker, using
the existing native Keychain/Secret Service policy and atomic protocol. Existing
final **or staged** state and OS credentials cause refusal; no key reset or overwrite
is offered. This increment closes a creation edge case where a staged-only artifact
could otherwise be discarded by initialization. Regular updates retain their existing
recovery/cleanup behavior.

Creation failure may leave a partial account after an OS or filesystem error. Open
the same account to recover and verify before retrying; a staged file without its
credential can fail closed and is preserved for recovery analysis. A worker thread
keeps the UI responsive, but OS authorization cannot be forcibly cancelled. No
seed-vault, mainnet, packaged-app durability or independent-review gate is waived.

## Verification and audit cost

Eight new regular desktop tests cover strict bounded files, permissions/links/FIFO,
no destination access on invalid input, separate approval, successful real review,
path-edit invalidation, narrow/full review rendering and creation-state transitions.
The existing busy-input regression now checks both paths in both modes. A storage
test verifies staged-only state survives refused creation without credential writes.
An additional native-provider test creates through the real UI worker, changes the
source after review, verifies saved counters and rejects replacement of existing
state. It deletes only its isolated temporary credential. CI runs it on both OSes.

Local checks passed **104 regular all-feature Rust tests**, 26 offline-desktop Rust
tests, the native Keychain setup test, four Python checks, formatting and strict
Clippy. Dependency advisory/license/source policy is also checked before publication.
Native framebuffer previews checked full and minimum-size review layouts using
only the published public fixture, without file, credential or node access.

This increment adds **166 net production Rust physical lines**: 112 for desktop
dispatch/review/approval, 49 for file validation/creation and five for preserving
staged state. Total is **2,695 lines**, including comments/spacing and excluding tests
and test wiring. No dependency, version, Cargo feature or lockfile changes:
**16 production direct dependency names, 473 locked entries**. Existing accessibility
maintenance exceptions retain their 2026-12-01 UTC deadline.

The subsequent [protected receive flow](BITCOIN_RECEIVE.md) requires fresh discovery
and gap-limit checks before acknowledged reservation. Reopening and redisplaying
prior reservations without consuming another index is next.
