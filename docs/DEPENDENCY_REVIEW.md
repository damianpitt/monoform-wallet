# Dependency and feature review — 2026-10-29

Scope: the `71133f1` baseline plus this desktop-feature separation. This is a
first-party inventory and public-advisory triage, not an independent security audit
or release approval. No cryptographic primitive, OS-storage protection, parsing
limit or consistency check is removed to lower a count.

## Build only the required components

`eframe` is now optional behind `desktop`, and the application binary requires
that feature. The default build includes the offline seed/account core and native
protected public-state storage. `core-rpc` separately adds the bounded read-only
testnet4 client. These features do not enable each other.

| Normal/build dependency packages | Before: default | Now: default | Now: `core-rpc` | Now: `desktop` | Now: both |
|---|---:|---:|---:|---:|---:|
| macOS arm64 | 172 | 60 | 76 | 172 | 187 |
| Linux x86-64 | 263 | 74 | 90 | 263 | 273 |

Counts include build dependencies and exclude Monoform itself and dev-only edges.
They identify unique package/version entries for each target, not source lines or
the exact machine code retained by the linker. Test builds add development
dependencies and may unify more library features. The lockfile still has **459
entries: Monoform plus 458 registry packages**, across platforms/configurations.
There are no new packages, upgrades or lockfile changes in this increment.
The manifest still has 16 distinct production dependency names; eframe moves from
mandatory to optional. Package counts do not measure the size of those libraries.

The reduction removes 112 macOS / 189 Linux dependency packages from core-only
builds, without adding production Rust lines. Full desktop graphs are unchanged;
this does not shrink a future GUI release or isolate a signer process.

The baseline has 2,287 production Rust physical lines including comments/spacing,
with test code/wiring separated. That total is unchanged. It describes the current
Bitcoin testnet foundation, not a complete three-chain wallet.

```sh
cargo check --locked
cargo test --locked --features core-rpc
cargo run --locked --features desktop
cargo clippy --locked --all-targets --all-features -- -D warnings
```

CI checks the default and desktop builds separately and tests the backend both
without the GUI and with all features. The GUI remains demonstration-only.
To reproduce graph counts, run the following with each of the two target triples
and no feature flag, `--features core-rpc`, `--features desktop` or `--all-features`:

```sh
cargo tree --locked --target aarch64-apple-darwin --edges normal,build --prefix none --format '{p}'
```

Remove trailing ` (*)` duplicate markers, exclude the Monoform root and count
unique lines. Linux uses `x86_64-unknown-linux-gnu`; resolving its graph on macOS
does not constitute a Linux compilation. Feature semantics follow the
[Cargo reference](https://doc.rust-lang.org/cargo/reference/features.html).

## Feature choices retained

- BIP39 English only, explicit Unicode normalization and secret zeroization.
- BIP32's existing secp256k1 backend; no handwritten replacement cryptography.
- Native Keychain / synchronous Secret Service with no plaintext/mock fallback.
- HTTP client defaults disabled: no remote TLS stack, compression, cookie jar or
  SOCKS feature for this fixed loopback adapter. Proxy discovery and redirects are
  disabled in its configuration; compile-time logging suppression is retained.
- Native accessibility, bundled fonts, Glow, Wayland and X11 remain in desktop
  builds. Persistence, link launching and WGPU are not enabled. Eframe itself
  requires clipboard support; there is no claim that the desktop is clipboard-free.

Making storage optional or replacing crypto/GUI libraries solely to improve counts
is deferred. Those changes need a separate behavioral and security justification.
The existing `bip32` dependency enables upstream ECDSA/SHA256 features in `k256`,
which also brings signing-related support such as RFC6979 despite Monoform having
no transaction-signing API. Dependency selection, not application line count,
must address that extra surface during the cryptography migration review.

## Current public-advisory findings

An [OSV batch query](https://google.github.io/osv.dev/api/) of all **458 locked
registry name/version pairs** returned eight package-version matches covering five
advisory IDs. The response was complete, with no pagination. These findings are
not suppressed and are not evidence of a new Monoform-specific exploit.

| Package(s) | Public finding | Current disposition |
|---|---|---|
| `crypto-bigint 0.5.5` | [RUSTSEC-2026-0387](https://rustsec.org/advisories/RUSTSEC-2026-0387.html): timing variability in operations intended to be constant-time; fixed upstream in 0.7.6 | Present in both platforms' core graphs via `bip32 → k256 → elliptic-curve`. Secret derivation makes this a priority release blocker. Practical reachability and remediation remain unresolved. |
| `block-buffer 0.10.4` | [GHSA-qwgh-2vcv-g2f7](https://github.com/advisories/GHSA-qwgh-2vcv-g2f7): buffer invariant corruption after a caught callback panic; advertised fixed version 0.12.1 | Present through `digest` and the existing cryptography stack. The advisory's package is spelled `block_buffer`; OSV matched the locked hyphenated name. Monoform contains no `catch_unwind` recovery; the pinned implementation differs from the advisory's inline-buffer example. Those facts do not clear the broad affected-version match. Obtain upstream applicability confirmation or migrate before release. |
| `zvariant 4.2.0 / 5.15.0` | [RUSTSEC-2026-0370](https://rustsec.org/advisories/RUSTSEC-2026-0370.html) | Maintenance notice: functionality moved into zbus 6, not a reported exploit. Version 5 is in the Linux desktop accessibility graph; version 4 is locked but inactive in the supported target graphs checked here. |
| `zbus_names 3.0.0 / 4.3.4` | [RUSTSEC-2026-0371](https://rustsec.org/advisories/RUSTSEC-2026-0371.html) | Same upstream migration requirement and desktop/locked distinction. |
| `zvariant_derive 4.2.0 / 5.15.0` | [RUSTSEC-2026-0372](https://rustsec.org/advisories/RUSTSEC-2026-0372.html) | Same upstream migration requirement and desktop/locked distinction. |

The patched cryptography versions cross the existing transitive dependency
constraints. Forcing an incompatible version or copying patches into Monoform is
not a safe lockfile-only fix. The next security increment must evaluate compatible
upstream migrations/replacements and rerun derivation vectors, public-state
compatibility and storage/backend checks. Accessibility is not removed to conceal
maintenance notices.

The OSV result is a dated database observation. It does not establish exhaustive
coverage, applicability, exploitability or absence of vulnerabilities. Automated
vulnerability enforcement is not yet installed in CI, and these findings remain
open before any real-funds or signing release.

## License inventory and remaining gates

Cargo metadata supplies license declarations for all 458 registry packages, with
no Git/path dependencies outside Monoform. Many declarations offer MIT/Apache
choices; bundled fonts, Unicode data and other alternatives require preservation
of their own applicable notices. Metadata alone is not a license-compliance audit.

Before release: resolve the cryptography findings, plan the Linux accessibility
upstream migration, enforce reviewed vulnerability/license policy in CI, produce
notices/SBOM and obtain independent review. Continue reporting production-line,
direct-dependency and resolved-graph changes in every meaningful increment.

Local verification passed default (25) and backend (76) regular Rust tests, the
all-feature test configuration, four Python verification tests, formatting and
strict all-target/all-feature Clippy. No new tests mirror this manifest-only
change; CI now exercises the additional independent feature configurations.
