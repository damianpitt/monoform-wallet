# Dependency remediation and CI policy — 2026-10-30

This increment follows the [2026-10-29 inventory](DEPENDENCY_REVIEW.md). It changes
one pinned library version and adds development-time policy checks. It adds no
wallet feature, production Rust line, seed persistence, signing or network access.

## Cryptography upgrade and compatibility

Upgrade `bip32` **0.5.3 → 0.6.0**, retaining disabled defaults and the existing
`secp256k1` feature. The new stable upstream release supports `k256 0.14.0` and
`elliptic-curve 0.14.3`, resolving **crypto-bigint 0.7.10** and **ctutils 0.4.7**.
The old `crypto-bigint 0.5.5` is removed. The resolved bigint version exceeds the
0.7.6 fix required by
[RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html).
No vulnerable-version exception is added for that advisory.

The library's changed HMAC/SEC1 APIs are internal to the dependency; Monoform's
existing borrowed-seed calls compile unchanged. Published seed/address vectors,
independent rust-bitcoin comparisons, fixed descriptor strings, canonical public
state records, rejected malformed/private keys and counter exhaustion checks pass.
The existing state format and protected storage protocol require no migration.
Root/child private keys retain the upstream zeroization boundary; no claim is made
that ordinary tests prove erasure of every compiler/OS copy or side-channel safety.

## Buffer finding: corrected scope, not a security waiver

The prior OSV/GitHub database record used a broad `<0.12.1` range. The
[publisher's advisory](https://github.com/RustCrypto/utils/security/advisories/GHSA-qwgh-2vcv-g2f7)
specifies **>=0.11.0,<0.12.1**. The retained `block-buffer 0.10.4` uses a separate
`pos: u8` field, as verified in its
[published source](https://docs.rs/block-buffer/0.10.4/src/block_buffer/lib.rs.html);
buffer writes do not overwrite that cursor. It is outside the publisher's affected
range and does not implement the reported inline-cursor layout. The new crypto
stack also resolves `block-buffer 0.12.1`, which is the fixed newer implementation.
No 0.11.x/0.12.0 version is locked. This closes the specific applicability finding,
not all possible buffer or panic issues.

A fresh complete OSV batch query of **472 registry package/version pairs** still
matches 0.10.4 to the broader database record, plus six D-Bus maintenance matches.
It no longer reports the old bigint timing finding. The metadata discrepancy is
recorded here; no suppression rule is added to disguise an OSV-clean result.

## Enforced dependency policy

[`deny.toml`](../deny.toml) configures **cargo-deny 0.20.2**, with all application
features enabled and normal/build/development dependencies included for the CI
targets `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu`. It enforces:

- RustSec vulnerability/unsoundness findings and yanked versions fail the check.
- Unmaintained dependencies fail except for the three named notices below.
- Unused advisory exceptions fail, requiring obsolete waivers to be removed.
- License expressions must be satisfied by the explicit allowlist. Exact-package
  exceptions cover Monoform's GPL license, bundled fonts and the `hex_lit` test
  dependency's MITNFA license; they are not blanket license exemptions.
- Unknown registries and Git dependency sources fail.

CI downloads the pinned upstream tool release, verifies the committed SHA256 before
extracting it, then runs `cargo-deny --locked check advisories licenses sources`.
The RustSec database refreshes on each run. Download/database/check failures are
not converted to success. The tool is not a wallet runtime dependency.

To run locally after obtaining and verifying the same tool version:

```sh
cargo deny --locked check advisories licenses sources
```

This checks RustSec's coverage, not every advisory database or actual exploitability.
It does not enforce legal compliance, produce distribution notices/SBOMs, independently
audit crypto, or constitute release approval. CI requires these checks; branch
protection and maintainer approval enforcement are separate repository settings.

## Temporary accessibility maintenance exceptions

Only **RUSTSEC-2026-0370, -0371 and -0372** are excepted. These concern standalone
`zvariant`, `zbus_names` and `zvariant_derive` functionality moving into zbus 6.
They are upstream maintenance notices, not reported vulnerabilities. Current
accessibility dependencies still require zbus 5; even the inspected newer stable
`accesskit_unix 0.24.1` requires it. Updating the GUI alone does not complete that
migration. Native accessibility remains enabled and the GUI version is unchanged.

The CI job has a separate **2026-12-01 UTC** deadline check. On/after that date it
fails until the maintenance exceptions are reviewed/removed with their deadline
gate. Cargo-deny 0.20.2 does not implement the expiry option in newer online docs;
running only the local command does not enforce the CI date gate. The exact IDs,
reasons and deadline remain visible in source. These exceptions are temporary
pre-alpha debt and must be resolved before release, not silently extended.

## Audit cost and verification

Production Rust remains **2,287 physical lines** including comments/spacing,
excluding tests and test wiring. Production direct-dependency names remain **16**.
The lockfile grows **459 → 473 entries** (including Monoform): 14 prior version
entries are replaced and 14 additional entries are introduced. Updated crypto
support and coexistence with older hashing dependencies increase the graph; this
is an explicit security-maintenance cost, not a claim of a smaller runtime.

| Normal/build dependency packages | Prior | Current |
|---|---:|---:|
| macOS core | 60 | 74 |
| macOS `core-rpc` | 76 | 90 |
| macOS `desktop` | 172 | 184 |
| macOS both | 187 | 199 |
| Linux core | 74 | 87 |
| Linux `core-rpc` | 90 | 103 |
| Linux `desktop` | 263 | 276 |
| Linux both | 273 | 286 |

Counts use the prior review's unique normal/build package/version method and exclude
Monoform and dev-only edges. The tool's own release is an additional development
trust boundary, pinned independently of the wallet graph.

Local verification passed 25 default and 76 backend/all-feature regular Rust tests,
four Python tests, formatting, strict Clippy, native Keychain round-trip and all six
live Core checks. Core 31.1 matched descriptors/address ranges, nonempty UTXOs and
the pending parent/child → replacement → confirmation fixture on isolated nodes.
Temporary negative fixtures proved that the policy rejects the old vulnerable
bigint and an unapproved license. No custom cryptography or advisory scanner is added.

Next functional module: protected atomic application of discovery recommendations,
with complete account matching and counters that cannot move backward. Independent
crypto/application review, packaged storage lifecycle validation, reproducible builds,
notices/SBOM and release controls remain gates before real-funds use.
