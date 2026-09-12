//! Public-vector compatibility check only; this is not a seed-import implementation.

use bip39::{Language, Mnemonic};
use monoform_wallet::{OfflineSeed, SeedError};

// BIP39 permits an empty passphrase. Reproduce with PBKDF2-HMAC-SHA512,
// NFKD(phrase), salt "mnemonic", 2,048 rounds, and a 64-byte output.
const EMPTY_PASSPHRASE_SEED: &str = "408b285c123836004f4b8842c89324c1f01382450c0d439af345ba7fc49acf705489c6fc77dbd4e3dc1dd8cc6bc9f043db8ada1e243c4a0eafb290d399480840";

fn seed_hex(seed: &[u8; 64]) -> String {
    seed.iter().map(|byte| format!("{byte:02x}")).collect()
}

// Trezor's public 256-bit vector; never use this phrase for funds.
// Source: https://github.com/trezor/python-mnemonic/blob/master/vectors.json
const PUBLIC_24_WORD_VECTOR: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
const PUBLIC_12_WORD_VECTOR: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const TREZOR_SEED: &str = "bda85446c68413707090a52022edd26a1c9462295029f2e60cd7c4f2bbd3097170af7a4d73245cafa9c3cca8d561a7c3de6f5d4a10be8ed2a5e608d68f92fcc8";

// The crate accepts other BIP39 lengths; Monoform's format intentionally does not.
fn accepts_monoform_format(phrase: &str) -> bool {
    phrase.split_whitespace().count() == 24 && Mnemonic::parse_in(Language::English, phrase).is_ok()
}

#[test]
fn published_24_word_vector_matches() {
    let Ok(phrase) = Mnemonic::parse_in(Language::English, PUBLIC_24_WORD_VECTOR) else {
        panic!("the published test vector must parse");
    };
    assert_eq!(seed_hex(&phrase.to_seed("TREZOR")), TREZOR_SEED);
    assert!(accepts_monoform_format(PUBLIC_24_WORD_VECTOR));
}

#[test]
fn rejects_other_lengths_and_invalid_checksum() {
    assert!(!accepts_monoform_format(PUBLIC_12_WORD_VECTOR));
    assert!(!accepts_monoform_format(
        &PUBLIC_24_WORD_VECTOR.replace("art", "abandon")
    ));
    assert!(!accepts_monoform_format("not a mnemonic"));
}

#[test]
fn empty_passphrase_matches_independent_check() {
    let Ok(phrase) = Mnemonic::parse_in(Language::English, PUBLIC_24_WORD_VECTOR) else {
        panic!("the published test vector must parse");
    };
    // This checks the seed value, not the yet-to-be-built import policy.
    assert_eq!(seed_hex(&phrase.to_seed("")), EMPTY_PASSPHRASE_SEED);
}

#[test]
fn monoform_core_uses_only_the_empty_passphrase() {
    let Ok(_seed) = OfflineSeed::from_phrase(PUBLIC_24_WORD_VECTOR.to_owned()) else {
        panic!("the public 24-word vector must be accepted");
    };

    // NFKD normalization must not change the account represented by the words.
    let wide = PUBLIC_24_WORD_VECTOR.replacen("abandon", "ａｂａｎｄｏｎ", 1);
    let Ok(_normalized_seed) = OfflineSeed::from_phrase(wide) else {
        panic!("the compatibility spelling must normalize");
    };
}

#[test]
fn monoform_core_rejects_bad_inputs_without_echoing_them() {
    assert!(matches!(
        OfflineSeed::from_phrase(PUBLIC_12_WORD_VECTOR.to_owned()),
        Err(SeedError::InvalidRecoveryPhrase)
    ));
    assert!(matches!(
        OfflineSeed::from_phrase(PUBLIC_24_WORD_VECTOR.replace("art", "abandon")),
        Err(SeedError::InvalidRecoveryPhrase)
    ));
    assert!(matches!(
        OfflineSeed::from_phrase("x".repeat(1025)),
        Err(SeedError::InvalidRecoveryPhrase)
    ));
    let Err(error) = OfflineSeed::from_phrase("private-example-not-a-phrase".to_owned()) else {
        panic!("invalid words must be rejected");
    };
    assert_eq!(format!("{error:?}"), "InvalidRecoveryPhrase");
}
