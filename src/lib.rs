//! Offline-only recovery-seed core. The desktop UI does not call this library yet.

// No unsupported-platform mock fallback may hold a storage encryption key.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod storage;

use bip32::{ChildNumber, ExtendedKey, KeyFingerprint, Prefix, XPrv, XPub};
use bip39::{Language, Mnemonic};
use bitcoin_hashes::{Hash, hash160, sha256};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

/// A BIP39 seed that cannot be printed or cloned through Monoform's API.
pub struct OfflineSeed(
    // Only offline testnet address derivation may read this; signing stays gated.
    Zeroizing<[u8; 64]>,
);

/// Static errors deliberately reveal no words or library error payloads.
#[derive(Debug, PartialEq, Eq)]
pub enum SeedError {
    InvalidRecoveryPhrase,
}

/// Bitcoin failures never include a seed, key, or underlying library payload.
#[derive(Debug, PartialEq, Eq)]
pub enum BitcoinError {
    InvalidIndex,
    DerivationFailed,
    DescriptorEncoding,
    InvalidPublicState,
}

/// Public testnet account material and its next in-memory address indices.
pub struct BitcoinTestnetAccount {
    fingerprint: KeyFingerprint,
    account: XPub,
    next_receive: u32,
    next_change: u32,
}

/// Bitcoin Core-compatible public descriptors. They contain no private key.
pub struct BitcoinTestnetDescriptors {
    pub receive: String,
    pub change: String,
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

    /// Return public state; temporary extended private keys do not escape.
    pub fn bitcoin_testnet_account(&self) -> Result<BitcoinTestnetAccount, BitcoinError> {
        let (fingerprint, account) = bitcoin_account(&self.0, 1)?;
        Ok(BitcoinTestnetAccount {
            fingerprint,
            account,
            next_receive: 0,
            next_change: 0,
        })
    }
}

impl BitcoinTestnetAccount {
    pub fn next_receive_address(&mut self) -> Result<String, BitcoinError> {
        next_address(&self.account, 0, &mut self.next_receive)
    }

    pub fn next_change_address(&mut self) -> Result<String, BitcoinError> {
        next_address(&self.account, 1, &mut self.next_change)
    }

    pub fn descriptors(&self) -> Result<BitcoinTestnetDescriptors, BitcoinError> {
        let key = format!("{}", self.account.to_extended_key(Prefix::TPUB));
        Ok(BitcoinTestnetDescriptors {
            receive: bitcoin_descriptor(self.fingerprint, &key, 0)?,
            change: bitcoin_descriptor(self.fingerprint, &key, 1)?,
        })
    }

    /// Encode watch-only state in memory; this neither encrypts nor saves it.
    /// Public keys expose address history: never log or publish this record.
    pub fn encode_public_state(&self) -> String {
        let [a, b, c, d] = self.fingerprint;
        let key = self.account.to_extended_key(Prefix::TPUB);
        let body = format!(
            "monoform:1:bitcoin-testnet4:{a:02x}{b:02x}{c:02x}{d:02x}:{key}:{}:{}",
            self.next_receive, self.next_change
        );
        // SHA256 detects accidental corruption, not tampering or cursor rollback.
        format!("{body}:{}", sha256::Hash::hash(body.as_bytes()))
    }

    /// Restore only the canonical v1 testnet4 account format, without a seed.
    /// Origin metadata cannot prove ancestry; callers must verify wallet identity.
    pub fn from_public_state(record: &str) -> Result<Self, BitcoinError> {
        let invalid = BitcoinError::InvalidPublicState;
        // Bound work and ensure ASCII before parsing attacker-controlled text.
        if record.len() > 256 || !record.is_ascii() {
            return Err(invalid);
        }
        let (body, checksum) = record.rsplit_once(':').ok_or(invalid)?;
        if checksum != sha256::Hash::hash(body.as_bytes()).to_string() {
            return Err(BitcoinError::InvalidPublicState);
        }
        let fields = body.split(':').collect::<Vec<_>>();
        let [
            "monoform",
            "1",
            "bitcoin-testnet4",
            fingerprint,
            key,
            receive,
            change,
        ] = fields.as_slice()
        else {
            return Err(BitcoinError::InvalidPublicState);
        };
        let fingerprint = u32::from_str_radix(fingerprint, 16)
            .map_err(|_| BitcoinError::InvalidPublicState)?
            .to_be_bytes();
        // Reject private-key text before Base58 decoding creates a key-byte copy.
        if !key.starts_with("tpub") {
            return Err(BitcoinError::InvalidPublicState);
        }
        let key = key
            .parse::<ExtendedKey>()
            .map_err(|_| BitcoinError::InvalidPublicState)?;
        // XPub conversion also accepts private keys upstream: reject them first.
        // Depth and child number check account 0; ancestors remain unverifiable.
        if key.prefix != Prefix::TPUB
            || key.attrs.depth != 3
            || key.attrs.child_number != ChildNumber(ChildNumber::HARDENED_FLAG)
        {
            return Err(BitcoinError::InvalidPublicState);
        }
        let account = XPub::try_from(key).map_err(|_| BitcoinError::InvalidPublicState)?;
        let next_receive = receive
            .parse::<u32>()
            .map_err(|_| BitcoinError::InvalidPublicState)?;
        let next_change = change
            .parse::<u32>()
            .map_err(|_| BitcoinError::InvalidPublicState)?;
        // 2^31 represents an exhausted branch, never a usable hardened address.
        if next_receive > ChildNumber::HARDENED_FLAG || next_change > ChildNumber::HARDENED_FLAG {
            return Err(BitcoinError::InvalidPublicState);
        }
        let restored = Self {
            fingerprint,
            account,
            next_receive,
            next_change,
        };
        // One spelling per record: reject padded numbers, whitespace and extra data.
        if restored.encode_public_state() != record {
            return Err(BitcoinError::InvalidPublicState);
        }
        Ok(restored)
    }
}

fn next_address(account: &XPub, branch: u32, index: &mut u32) -> Result<String, BitcoinError> {
    let address = bitcoin_address(bitcoin_branch(account, branch)?, *index, bech32::hrp::TB)?;
    *index += 1;
    Ok(address)
}

fn bitcoin_address(branch: XPub, index: u32, hrp: bech32::Hrp) -> Result<String, BitcoinError> {
    let child = ChildNumber::new(index, false).map_err(|_| BitcoinError::InvalidIndex)?;
    let key = branch
        .derive_child(child)
        .map_err(|_| BitcoinError::DerivationFailed)?
        .to_bytes();
    // BIP84 P2WPKH is witness v0 over HASH160 of the compressed public key.
    let program = hash160::Hash::hash(&key);
    bech32::segwit::encode_v0(hrp, program.as_byte_array())
        .map_err(|_| BitcoinError::DerivationFailed)
}

// The coin type is internal so production cannot select mainnet.
fn bitcoin_account(
    seed: &[u8; 64],
    coin_type: u32,
) -> Result<(KeyFingerprint, XPub), BitcoinError> {
    // BIP84 account: m/84'/coin_type'/0'.
    let hardened = ChildNumber::HARDENED_FLAG;
    let account_path = [
        ChildNumber(84 | hardened),
        ChildNumber(coin_type | hardened),
        ChildNumber(hardened),
    ];
    let root = XPrv::new(seed).map_err(|_| BitcoinError::DerivationFailed)?;
    let fingerprint = root.public_key().fingerprint();
    let account = account_path
        .into_iter()
        .try_fold(root, |key, number| key.derive_child(number))
        .map_err(|_| BitcoinError::DerivationFailed)?;
    Ok((fingerprint, account.public_key()))
}

fn bitcoin_branch(account: &XPub, branch: u32) -> Result<XPub, BitcoinError> {
    account
        .derive_child(ChildNumber::new(branch, false).map_err(|_| BitcoinError::InvalidIndex)?)
        .map_err(|_| BitcoinError::DerivationFailed)
}

fn bitcoin_descriptor(
    fingerprint: KeyFingerprint,
    account: &str,
    branch: u32,
) -> Result<String, BitcoinError> {
    let [a, b, c, d] = fingerprint;
    descriptor_checksum(&format!(
        "wpkh([{a:02x}{b:02x}{c:02x}{d:02x}/84h/1h/0h]{account}/{branch}/*)"
    ))
}

// BIP380's checksum detects public-text mistakes; it is not a security boundary.
fn descriptor_checksum(descriptor: &str) -> Result<String, BitcoinError> {
    const INPUT: &[u8] = b"0123456789()[],'/*abcdefgh@:$%{}IJKLMNOPQRSTUVWXYZ&+-.;<=>?!^_|~ijklmnopqrstuvwxyzABCDEFGH`#\"\\ ";
    const OUTPUT: &[u8] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    let (mut checksum, mut group, mut count) = (1_u64, 0_u64, 0_u8);

    for byte in descriptor.bytes() {
        let position = INPUT
            .iter()
            .position(|allowed| *allowed == byte)
            .ok_or(BitcoinError::DescriptorEncoding)?;
        checksum = descriptor_polymod(checksum, (position & 31) as u64);
        group = group * 3 + (position >> 5) as u64;
        count += 1;
        if count == 3 {
            checksum = descriptor_polymod(checksum, group);
            (group, count) = (0, 0);
        }
    }
    if count > 0 {
        checksum = descriptor_polymod(checksum, group);
    }
    for _ in 0..8 {
        checksum = descriptor_polymod(checksum, 0);
    }
    checksum ^= 1;
    let suffix = (0..8)
        .map(|index| OUTPUT[((checksum >> (5 * (7 - index))) & 31) as usize] as char)
        .collect::<String>();
    Ok(format!("{descriptor}#{suffix}"))
}

fn descriptor_polymod(checksum: u64, value: u64) -> u64 {
    const GENERATOR: [u64; 5] = [
        0xf5dee51989,
        0xa9fdca3312,
        0x1bab10e32d,
        0x3706b1677a,
        0x644d626ffd,
    ];
    let top = checksum >> 35;
    let mut next = ((checksum & 0x7ffffffff) << 5) ^ value;
    for (bit, generator) in GENERATOR.into_iter().enumerate() {
        if (top >> bit) & 1 != 0 {
            next ^= generator;
        }
    }
    next
}

#[cfg(test)]
mod tests {
    use super::{
        BitcoinError, BitcoinTestnetAccount, OfflineSeed, bitcoin_account, bitcoin_address,
        bitcoin_branch, descriptor_checksum,
    };
    use ::bip32::ChildNumber;
    use bip39::{Language, Mnemonic};
    use bitcoin::{
        Address, CompressedPublicKey, KnownHrp, Network, bip32 as bitcoin_bip32,
        secp256k1::Secp256k1,
    };

    // Public Trezor vector; this mnemonic must never protect real funds.
    const PUBLIC_PHRASE: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
    const EMPTY_SEED: &str = "408b285c123836004f4b8842c89324c1f01382450c0d439af345ba7fc49acf705489c6fc77dbd4e3dc1dd8cc6bc9f043db8ada1e243c4a0eafb290d399480840";
    const RECEIVE_DESCRIPTOR: &str = "wpkh([5436d724/84h/1h/0h]tpubDCWivZp6qaqCALCt8MyLqAb3awnWm4hfbBPjdZqirYFXYeZ5YsfbWVaPacULZTGtK1RPBSZ92UWNjnhL4fB9UVrF2FjgW8cgmBjxPBmB4iB/0/*)#hdxns7pj";
    const CHANGE_DESCRIPTOR: &str = "wpkh([5436d724/84h/1h/0h]tpubDCWivZp6qaqCALCt8MyLqAb3awnWm4hfbBPjdZqirYFXYeZ5YsfbWVaPacULZTGtK1RPBSZ92UWNjnhL4fB9UVrF2FjgW8cgmBjxPBmB4iB/1/*)#xerjdt32";
    // SHA256 independently checked with the system's shasum, not the codec.
    const PUBLIC_STATE: &str = "monoform:1:bitcoin-testnet4:5436d724:tpubDCWivZp6qaqCALCt8MyLqAb3awnWm4hfbBPjdZqirYFXYeZ5YsfbWVaPacULZTGtK1RPBSZ92UWNjnhL4fB9UVrF2FjgW8cgmBjxPBmB4iB:0:0:2d159afbde83ae83f819477437aabcd2299049210fda77d2d236211c5907f718";

    fn public_account() -> BitcoinTestnetAccount {
        OfflineSeed::from_phrase(PUBLIC_PHRASE.to_owned())
            .unwrap_or_else(|_| panic!("public fixture must parse"))
            .bitcoin_testnet_account()
            .unwrap_or_else(|_| panic!("public fixture must derive"))
    }

    // Recompute integrity so invalid-field tests exercise validation, not checksum rejection.
    fn sealed(body: &str) -> String {
        use bitcoin_hashes::{Hash, sha256};
        format!("{body}:{}", sha256::Hash::hash(body.as_bytes()))
    }

    fn assert_invalid_state(record: &str) {
        assert!(matches!(
            BitcoinTestnetAccount::from_public_state(record),
            Err(BitcoinError::InvalidPublicState)
        ));
    }

    #[test]
    fn public_state_round_trip_resumes_both_branches_without_seed() {
        // The helper has already dropped its seed; only public material remains.
        let mut original = public_account();
        assert_eq!(original.encode_public_state(), PUBLIC_STATE);
        for _ in 0..3 {
            assert!(original.next_receive_address().is_ok());
        }
        assert!(original.next_change_address().is_ok());
        let record = original.encode_public_state();
        let mut restored = BitcoinTestnetAccount::from_public_state(&record)
            .unwrap_or_else(|_| panic!("public state must restore"));
        assert_eq!(restored.encode_public_state(), record);
        assert_eq!(
            restored.next_receive_address(),
            original.next_receive_address()
        );
        assert_eq!(
            restored.next_change_address(),
            original.next_change_address()
        );
        assert_eq!(
            restored.descriptors().map(|d| d.receive),
            Ok(RECEIVE_DESCRIPTOR.to_owned())
        );
        assert_eq!(
            restored.descriptors().map(|d| d.change),
            Ok(CHANGE_DESCRIPTOR.to_owned())
        );
    }

    #[test]
    fn public_state_rejects_corruption_truncation_and_unbounded_input() {
        for end in 0..PUBLIC_STATE.len() {
            assert_invalid_state(&PUBLIC_STATE[..end]);
        }
        for index in 0..PUBLIC_STATE.len() {
            let mut corrupted = PUBLIC_STATE.as_bytes().to_vec();
            corrupted[index] ^= 1;
            let corrupted = String::from_utf8(corrupted)
                .unwrap_or_else(|_| panic!("mutated ASCII must remain UTF-8"));
            assert_invalid_state(&corrupted);
        }
        for invalid in [
            "".to_owned(),
            "x".repeat(257),
            "Ü".repeat(64),
            format!("{PUBLIC_STATE}\n"),
        ] {
            assert_invalid_state(&invalid);
        }
    }

    #[test]
    fn public_state_rejects_unknown_formats_and_noncanonical_fields() {
        let (body, _) = PUBLIC_STATE
            .rsplit_once(':')
            .unwrap_or_else(|| panic!("public state must contain checksum"));
        let fields = body.split(':').collect::<Vec<_>>();
        for (index, invalid) in [
            (0, "other"),
            (1, "2"),
            (1, "01"),
            (2, "bitcoin-mainnet"),
            (2, "bitcoin-testnet3"),
            (2, "bitcoin-signet"),
            (3, "5436D724"),
            (3, "05436d724"),
            (3, "5436d72"),
            (3, "zzzzzzzz"),
            (4, "tpub-invalid"),
            (4, ""),
            (5, "-1"),
            (5, "+0"),
            (5, "00"),
            (5, "2147483649"),
            (5, "4294967296"),
            (6, "-1"),
            (6, " 0"),
            (6, "00"),
            (6, "2147483649"),
            (6, "4294967296"),
        ] {
            let mut modified = fields.clone();
            modified[index] = invalid;
            assert_invalid_state(&sealed(&modified.join(":")));
        }
        assert_invalid_state(&sealed(&fields[..6].join(":")));
        assert_invalid_state(&sealed(&format!("{body}:extra")));
    }

    #[test]
    fn public_state_rejects_private_mainnet_wrong_depth_and_invalid_curve_keys() {
        use bip32::{Prefix, XPrv};
        let original = public_account();
        let seed = OfflineSeed::from_phrase(PUBLIC_PHRASE.to_owned())
            .unwrap_or_else(|_| panic!("public fixture must parse"));
        let root = XPrv::new(&seed.0[..]).unwrap_or_else(|_| panic!("public root must derive"));
        let hardened = ChildNumber::HARDENED_FLAG;
        let private_account = [84 | hardened, 1 | hardened, hardened]
            .into_iter()
            .try_fold(root, |key, child| key.derive_child(ChildNumber(child)))
            .unwrap_or_else(|_| panic!("public fixture account must derive"));
        let (body, _) = PUBLIC_STATE
            .rsplit_once(':')
            .unwrap_or_else(|| panic!("public state must contain checksum"));
        let fields = body.split(':').collect::<Vec<_>>();
        let valid = original.account.to_extended_key(Prefix::TPUB);
        let mut wrong_depth = valid.clone();
        wrong_depth.attrs.depth = 2;
        let mut wrong_account = valid.clone();
        wrong_account.attrs.child_number = ChildNumber(ChildNumber::HARDENED_FLAG | 1);
        let mut nonhardened_account = valid.clone();
        nonhardened_account.attrs.child_number = ChildNumber(0);
        let mut invalid_point = valid;
        invalid_point.key_bytes = [0xff; 33];
        invalid_point.key_bytes[0] = 2;
        // Valid Base58 checksums must not disguise private material or invalid origins.
        for key in [
            private_account.to_extended_key(Prefix::TPRV),
            private_account.to_extended_key(Prefix::XPRV),
            original.account.to_extended_key(Prefix::XPUB),
            wrong_depth,
            wrong_account,
            nonhardened_account,
            invalid_point,
        ] {
            let text = key.to_string();
            let mut modified = fields.clone();
            modified[4] = &text;
            assert_invalid_state(&sealed(&modified.join(":")));
        }
    }

    #[test]
    fn public_state_preserves_exhaustion_without_advancing_failed_cursors() {
        let mut account = public_account();
        account.next_receive = ChildNumber::HARDENED_FLAG - 1;
        account.next_change = ChildNumber::HARDENED_FLAG - 1;
        let mut restored = BitcoinTestnetAccount::from_public_state(&account.encode_public_state())
            .unwrap_or_else(|_| panic!("last valid indices must restore"));
        assert!(restored.next_receive_address().is_ok());
        assert!(restored.next_change_address().is_ok());
        let exhausted = restored.encode_public_state();
        let mut restored = BitcoinTestnetAccount::from_public_state(&exhausted)
            .unwrap_or_else(|_| panic!("exhausted branches must restore"));
        assert_eq!(
            restored.next_receive_address(),
            Err(BitcoinError::InvalidIndex)
        );
        assert_eq!(
            restored.next_change_address(),
            Err(BitcoinError::InvalidIndex)
        );
        assert_eq!(restored.encode_public_state(), exhausted);
    }

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

    #[test]
    fn published_bip84_mainnet_vectors_match_without_enabling_mainnet() {
        // BIP84's public 12-word fixture is test-only; Monoform import still requires 24.
        let fixture = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, fixture)
            .unwrap_or_else(|_| panic!("published BIP84 fixture must parse"));
        let seed = mnemonic.to_seed_normalized("");
        let (_, account) = bitcoin_account(&seed, 0)
            .unwrap_or_else(|_| panic!("published BIP84 account must derive"));
        for (branch, index, expected) in [
            (0, 0, "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu"),
            (0, 1, "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g"),
            (1, 0, "bc1q8c6fshw2dlwun7ekn9qwf37cu2rn755upcp6el"),
        ] {
            let branch = bitcoin_branch(&account, branch)
                .unwrap_or_else(|_| panic!("published BIP84 branch must derive"));
            assert_eq!(
                bitcoin_address(branch, index, bech32::hrp::BC)
                    .unwrap_or_else(|_| panic!("published address must encode")),
                expected
            );
        }
    }

    #[test]
    fn tracks_public_testnet_receive_and_change_branches() {
        let seed = OfflineSeed::from_phrase(PUBLIC_PHRASE.to_owned())
            .unwrap_or_else(|_| panic!("public 24-word fixture must parse"));
        let mut account = seed
            .bitcoin_testnet_account()
            .unwrap_or_else(|_| panic!("public testnet account must derive"));
        let addresses = [
            account.next_receive_address(),
            account.next_receive_address(),
            account.next_change_address(),
        ]
        .map(|address| address.unwrap_or_else(|_| panic!("testnet address must derive")));

        // Compare the 24-word testnet result with rust-bitcoin's independent BIP32.
        let secp = Secp256k1::new();
        let root = bitcoin_bip32::Xpriv::new_master(Network::Testnet, &seed.0[..])
            .unwrap_or_else(|_| panic!("public fixture root must derive"));
        for ((branch, index), address) in [(0, 0), (0, 1), (1, 0)].into_iter().zip(addresses) {
            let path = format!("m/84'/1'/0'/{branch}/{index}")
                .parse::<bitcoin_bip32::DerivationPath>()
                .unwrap_or_else(|_| panic!("fixed testnet path must parse"));
            let child = root
                .derive_priv(&secp, &path)
                .unwrap_or_else(|_| panic!("public fixture child must derive"));
            let key = CompressedPublicKey::from_private_key(&secp, &child.to_priv())
                .unwrap_or_else(|_| panic!("derived key must be compressed"));
            assert_eq!(
                address,
                Address::p2wpkh(&key, KnownHrp::Testnets).to_string()
            );
        }

        account.next_receive = ChildNumber::HARDENED_FLAG;
        assert_eq!(
            account.next_receive_address(),
            Err(BitcoinError::InvalidIndex)
        );
    }

    #[test]
    fn exports_bitcoin_core_watch_only_descriptors() {
        assert_eq!(
            descriptor_checksum("raw(deadbeef)"),
            Ok("raw(deadbeef)#89f8spxm".to_owned())
        );
        assert_eq!(
            descriptor_checksum("raw(Ü)"),
            Err(BitcoinError::DescriptorEncoding)
        );

        let seed = OfflineSeed::from_phrase(PUBLIC_PHRASE.to_owned())
            .unwrap_or_else(|_| panic!("public 24-word fixture must parse"));
        let account = seed
            .bitcoin_testnet_account()
            .unwrap_or_else(|_| panic!("public testnet account must derive"));
        let descriptors = account
            .descriptors()
            .unwrap_or_else(|_| panic!("public descriptors must encode"));
        assert_eq!(descriptors.receive, RECEIVE_DESCRIPTOR);
        assert_eq!(descriptors.change, CHANGE_DESCRIPTOR);

        // rust-bitcoin independently confirms the origin fingerprint and account tpub.
        let secp = Secp256k1::new();
        let root = bitcoin_bip32::Xpriv::new_master(Network::Testnet, &seed.0[..])
            .unwrap_or_else(|_| panic!("public fixture root must derive"));
        let path = "m/84'/1'/0'"
            .parse::<bitcoin_bip32::DerivationPath>()
            .unwrap_or_else(|_| panic!("fixed account path must parse"));
        let child = root
            .derive_priv(&secp, &path)
            .unwrap_or_else(|_| panic!("public fixture account must derive"));
        let tpub = bitcoin_bip32::Xpub::from_priv(&secp, &child);
        let fingerprint = root.fingerprint(&secp);

        for (branch, actual) in [(0, descriptors.receive), (1, descriptors.change)] {
            let body = format!("wpkh([{fingerprint}/84h/1h/0h]{tpub}/{branch}/*)");
            assert_eq!(
                actual,
                descriptor_checksum(&body)
                    .unwrap_or_else(|_| panic!("independent descriptor must encode"))
            );
            assert!(!actual.contains("prv"));
        }
    }
}
