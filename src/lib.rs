//! Offline-only recovery-seed core. The desktop UI does not call this library yet.

use bip39::{Language, Mnemonic};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

/// A BIP39 seed that cannot be printed or cloned through Monoform's API.
pub struct OfflineSeed(
    // No production adapter may read this until the BIP32 boundary is reviewed.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "address derivation is the next gated stage")
    )]
    Zeroizing<[u8; 64]>,
);

/// Static errors deliberately reveal no words or library error payloads.
#[derive(Debug, PartialEq, Eq)]
pub enum SeedError {
    InvalidRecoveryPhrase,
}

impl OfflineSeed {
    /// Accept only the locked English 24-word format with an empty passphrase.
    /// The caller transfers ownership of the phrase; no UI path exists yet.
    pub fn from_phrase(phrase: String) -> Result<Self, SeedError> {
        // Bound untrusted input before Unicode normalization can expand it.
        let phrase = Zeroizing::new(phrase);
        if phrase.len() > 1024 {
            return Err(SeedError::InvalidRecoveryPhrase);
        }

        // Both the original and NFKD copy are wiped on drop; avoid an unguarded Cow.
        let normalized = Zeroizing::new(phrase.nfkd().collect::<String>());
        if normalized.len() > 1024 || normalized.split_whitespace().count() != 24 {
            return Err(SeedError::InvalidRecoveryPhrase);
        }

        let mnemonic = Mnemonic::parse_in_normalized(Language::English, &normalized)
            .map_err(|_| SeedError::InvalidRecoveryPhrase)?;
        // Empty is part of the wallet format, not a caller-supplied choice.
        Ok(Self(Zeroizing::new(mnemonic.to_seed_normalized(""))))
    }
}

#[cfg(test)]
mod tests {
    use super::OfflineSeed;

    // Public Trezor vector; this mnemonic must never protect real funds.
    const PUBLIC_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
    const EMPTY_SEED: &str = "408b285c123836004f4b8842c89324c1f01382450c0d439af345ba7fc49acf705489c6fc77dbd4e3dc1dd8cc6bc9f043db8ada1e243c4a0eafb290d399480840";

    #[test]
    fn derives_locked_empty_passphrase_seed() {
        let Ok(seed) = OfflineSeed::from_phrase(PUBLIC_PHRASE.to_owned()) else {
            panic!("public vector must parse");
        };
        let actual = seed
            .0
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(actual, EMPTY_SEED);
    }
}
