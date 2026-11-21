# Explicit watch-only desktop — 2026-11-21

The macOS/Linux desktop can open an **existing protected Bitcoin testnet4 account**
and run one bounded watch-only refresh at a time. The subsequent
[watch-only setup module](WATCH_ONLY_SETUP.md) adds explicit creation from public
exports. Seed entry, signing, mainnet and other chains remain unimplemented.

```sh
cargo run --locked --features desktop,core-rpc
```

`--features desktop` alone opens a small offline welcome screen. The default build
still excludes both GUI and networking. No feature enables the other.

## Explicit connection and transient results

Enter the full absolute paths of the protected account folder and the trusted
local Bitcoin Core data folder. Paths are bounded to 4,096 bytes and reject NUL;
there is no `~` expansion, trimming, folder search or automatic account creation.
Existing storage ownership/permission checks and the backend's fixed testnet4
loopback endpoint/cookie policy still apply. Paths themselves grant no trust.
Core needs the existing [history/index configuration](BITCOIN_DISCOVERY.md).

The unchecked consent explains that queries reveal linked addresses to the local
node and may save discovered address counters. Editing either folder revokes
consent and clears displayed results. Refresh requires both valid paths and consent.
Consent lasts only for that pair of folders in the current process. Nothing runs
at startup, on typing, on clearing the screen or because an age label repaints.

One explicit Refresh starts one worker. Folder fields and consent are disabled
until it completes; the text fields are also explicitly non-interactive because
disabling a surrounding egui panel alone does not block focused keyboard input.
Starting refresh immediately discards prior values. Failure displays a static
message in place of balances; there is no cached success or confirmed-only fallback.
Clear screen resets paths, consent and results when idle; it does not delete the
protected account or OS credentials, and it is not a memory-erasure promise.

Success shows confirmed total, total including pending changes, immature mining
rewards when present, anchor height and observation age. BTC formatting uses only
integer quotient/remainder with eight decimal places. Totals are scoped, include
immature coins and are **not spendable balances or a complete-history proof**.
An advancing age is a local clock display, not an automatic network refresh; a
clock earlier than the observation reports unavailable age. No fiat feed is added.

## Worker and storage boundary

The standard-library worker constructs and owns `PublicStateStore` inside its
thread; no non-`Send` OS credential or store handle crosses threads. It calls the
existing [bounded refresh](BITCOIN_REFRESH.md) with default limits. Only confirmed,
projected and immature integer amounts, height and observation time cross a bounded
one-result channel during refresh. Full account keys, descriptors, histories and OS key material
are dropped with the worker's store/observation. No new logging, serialization,
persistence or general task framework is introduced.

Worker termination, unavailable/locked credentials, authentication, wrong network,
incomplete observations and changed chain anchors produce non-secret static errors.
Failed commits explicitly say counters **may** have advanced; acknowledged but late
commits say counters **were** saved and require a new refresh before showing amounts.
Neither state is presented as a successful balance.

The UI stays responsive during network and OS-store work. OS authorization may
prompt or block beyond the 60-second shared network budget; no hard cancellation
guarantee is added. Closing the app can interrupt a store commit, whose existing
recovery protocol and [durability limits](PUBLIC_STATE_STORAGE.md) still apply.
There are no automatic retries, cancellation control, node startup or background
queries. Screen, clipboard/accessibility services and same-user memory remain
privacy boundaries; a worker thread is not process isolation.

## Verification and audit cost

These counts describe the initial desktop checkpoint `649296f`; the subsequent
[setup module](WATCH_ONLY_SETUP.md) reports its additional code and checks separately.

Seven new tests cover consent and bounded paths, single-worker/result handling,
real worker failure before any credential/node access, exact integer display,
commit uncertainty messages, narrow/full headless frames, folder-edit consent
revocation and focused keyboard input while busy. They use synthetic public state
only. The existing backend and native-provider tests cover the refresh/storage
protocol separately; a live-node successful GUI refresh is not claimed here.

Local verification passed **95 regular all-feature Rust tests**, 25 offline desktop
Rust tests, four Python checks, formatting, strict all-target/all-feature Clippy
and dependency advisory/license/source policy. CI exercises both Linux and macOS,
including the existing native OS-store and isolated Core compatibility tests.

Native macOS framebuffer previews checked the empty form and a synthetic result
at the minimum 360 × 560-point size. Scrollable content retains observation age
and scope text. Full Linux/macOS packaged lifecycle, physical power-loss testing,
accessibility review and independent security review remain release gates.

The old 235-line Rust demo is replaced by a 65-line entry/welcome screen and a
255-line connected module: **85 net production Rust physical lines**, for **2,529
total**, including comments/spacing and excluding test code/wiring. The original
dependency-free HTML study remains in `prototype/`. No dependencies, versions or
features are added: **16 production direct names and 473 lockfile entries** remain.
Native accessibility, Glow, Wayland and X11 are retained. Existing accessibility
maintenance exceptions still expire on 2026-12-01 UTC.

The subsequent [setup module](WATCH_ONLY_SETUP.md) implements reviewed public imports
without recovery phrases or private keys. Its verification and audit cost are
recorded separately. Next is a protected receive flow with discovery and gap-limit safeguards.
