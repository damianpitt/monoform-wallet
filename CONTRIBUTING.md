# Contributing

Thank you for helping build a small, comprehensible wallet.

## Before contributing

- Read the product charter, architecture, and threat model.
- Use an issue for bugs and a discussion/design proposal for new behavior.
- Never post a seed phrase, private key, real wallet data, or suspected vulnerability.
- Keep changes narrow. New features must fit the explicit product scope.

## Pull requests

A change should include tests, a concise rationale, user-visible documentation where
needed, and any threat-model impact. Cryptography, key storage, derivation, signing,
network trust, dependencies, or release changes require two maintainer approvals.

By contributing, you agree that your contribution is licensed under GPL-3.0-or-later
and certify the Developer Certificate of Origin below by adding a `Signed-off-by` line
to each commit (`git commit -s`).

## Developer Certificate of Origin

The project uses the standard Developer Certificate of Origin 1.1. By signing off,
you certify that you have the right to submit the contribution under this project's
license. See <https://developercertificate.org/>.

## Current contribution focus

During Phase 0, useful contributions include accessibility review, copy, transaction
review UX, threat analysis, build reproducibility research, and chain-specific test
vector research. Production signing code is deferred until its design proposal lands.
