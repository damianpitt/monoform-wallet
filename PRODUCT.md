# Product charter

## Promise

Your keys. Your assets. Three actions. No business model hidden in the interface.

> Sovereign Money, One form, Three Chains

## Intended user

A person who already understands that losing a recovery phrase can mean losing
funds, and wants a calm everyday interface without exchange features. The first
release is not designed for first-time crypto users or high-value treasury custody.

## The interface

The home screen answers only:

- What do I own?
- How do I receive it?
- What exactly am I about to send?

Every asset view uses the same information hierarchy. Advanced chain details remain
available in transaction review, but never compete with the address, amount, fee,
and final total.

## Non-goals

- Buying, selling, swapping, bridging, staking, lending, or yield
- Fiat accounts, cards, identity checks, referrals, or affiliate links
- Dapp browsing, arbitrary contract signing, NFTs, or token discovery
- Cloud backups, email accounts, social recovery, or custodial recovery
- Price predictions, news, engagement prompts, or notifications unrelated to funds
- Supporting every network or token

## The Monochrome Principle

Monochrome is the design and engineering philosophy: remove everything that does not
help someone receive, send, or verify. The interface uses black, white, neutral tones,
and at most one functional accent. The code follows the same rule.

- One primary implementation language: Rust.
- One consistent transaction ceremony across supported networks.
- No runtime plugins, downloaded code, remote feature flags, or remote token lists.
- Every feature reports its production-code and dependency cost.
- Tests may be extensive; safety work is never removed to win a line-count claim.
- Complexity hidden in a dependency still counts as complexity.

## Asset policy

The permanent base-chain scope is Bitcoin, Monero, and Ethereum mainnet. New networks
are not added by popularity vote. A proposal must document the
network's custody model, derivation scheme, transaction semantics, reliable data
sources, privacy consequences, maintenance burden, and independent review plan.
Tokens require an explicit allowlist committed in source; remote token lists are
not accepted.

ERC-20 support is limited to ordinary balance, history, receive, and transfer behavior.
Arbitrary token import, approvals, permits, NFTs, dapps, swaps, staking, bridges, and
blind contract signing are outside the product.

One future ZK rollup may be selected as a low-cost stablecoin rail. It must publish
transaction data to Ethereum, provide a practical censorship or forced-exit path, have
transparent upgrade authority, and carry issuer-native allowlisted stablecoins. Monoform
will not implement the bridge.

Lightning is a future Bitcoin capability, not part of the initial release. Connecting
to a user-operated node is evaluated before embedding channel management in Monoform.

## One recovery phrase

Monoform uses one dedicated English 24-word BIP39 phrase to derive its Bitcoin,
Monero, and Ethereum accounts. The initial format has no additional BIP39 passphrase.
The derivation scheme is fixed, versioned, documented, and covered by public test
vectors so recovery does not depend on the continued existence of the application.

The phrase is generated offline outside Monoform. If a hardware wallet is used to
generate it, the device is acting only as an offline seed generator. Entering those
words into Monoform creates a software wallet and removes hardware-only protection.
Users must never import a phrase already used to protect hardware-wallet funds.

## A hard rule for alpha

Early builds use public test networks and marked test mnemonics only. Mainnet is a
release gate, not a configuration toggle.
