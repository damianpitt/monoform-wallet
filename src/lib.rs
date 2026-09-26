//! Offline-only recovery-seed core. The desktop UI does not call this library yet.

use bip32::{ChildNumber, XPrv, XPub};
use bip39::{Language, Mnemonic};
use bitcoin_hashes::{Hash, hash160};
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

/// Address failures never include a seed, key, or derivation-library payload.
#[derive(Debug, PartialEq, Eq)]
pub enum BitcoinError {
    InvalidIndex,
    DerivationFailed,
}

/// Public testnet branches and their next in-memory address indices.
pub struct BitcoinTestnetAccount {
    receive: XPub,
    change: XPub,
    next_receive: u32,
    next_change: u32,
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

    /// Remove private derivation material before returning public account state.
    pub fn bitcoin_testnet_account(&self) -> Result<BitcoinTestnetAccount, BitcoinError> {
        let account = bitcoin_account(&self.0, 1)?;
        Ok(BitcoinTestnetAccount {
            receive: bitcoin_branch(&account, 0)?,
            change: bitcoin_branch(&account, 1)?,
            next_receive: 0,
            next_change: 0,
        })
    }
}

impl BitcoinTestnetAccount {
    pub fn next_receive_address(&mut self) -> Result<String, BitcoinError> {
        next_address(&self.receive, &mut self.next_receive)
    }

    pub fn next_change_address(&mut self) -> Result<String, BitcoinError> {
        next_address(&self.change, &mut self.next_change)
    }
}

fn next_address(branch: &XPub, index: &mut u32) -> Result<String, BitcoinError> {
    let address = bitcoin_address(branch, *index, bech32::hrp::TB)?;
    *index += 1;
    Ok(address)
}

fn bitcoin_address(branch: &XPub, index: u32, hrp: bech32::Hrp) -> Result<String, BitcoinError> {
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
fn bitcoin_account(seed: &[u8; 64], coin_type: u32) -> Result<XPub, BitcoinError> {
    // BIP84 account: m/84'/coin_type'/0'.
    let hardened = ChildNumber::HARDENED_FLAG;
    let account_path = [
        ChildNumber(84 | hardened),
        ChildNumber(coin_type | hardened),
        ChildNumber(hardened),
    ];
    let root = XPrv::new(seed).map_err(|_| BitcoinError::DerivationFailed)?;
    let account = account_path
        .into_iter()
        .try_fold(root, |key, number| key.derive_child(number))
        .map_err(|_| BitcoinError::DerivationFailed)?;
    Ok(account.public_key())
}

fn bitcoin_branch(account: &XPub, branch: u32) -> Result<XPub, BitcoinError> {
    account
        .derive_child(ChildNumber::new(branch, false).map_err(|_| BitcoinError::InvalidIndex)?)
        .map_err(|_| BitcoinError::DerivationFailed)
}

#[cfg(test)]
mod tests {
    use super::{BitcoinError, OfflineSeed, bitcoin_account, bitcoin_address, bitcoin_branch};
    use ::bip32::ChildNumber;
    use bip39::{Language, Mnemonic};
    use bitcoin::{
        Address, CompressedPublicKey, KnownHrp, Network, bip32 as bitcoin_bip32,
        secp256k1::Secp256k1,
    };

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

    #[test]
    fn published_bip84_mainnet_vectors_match_without_enabling_mainnet() {
        // BIP84's public 12-word fixture is test-only; Monoform import still requires 24.
        let fixture = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, fixture)
            .unwrap_or_else(|_| panic!("published BIP84 fixture must parse"));
        let seed = mnemonic.to_seed_normalized("");
        let account = bitcoin_account(&seed, 0)
            .unwrap_or_else(|_| panic!("published BIP84 account must derive"));
        for (branch, index, expected) in [
            (0, 0, "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu"),
            (0, 1, "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g"),
            (1, 0, "bc1q8c6fshw2dlwun7ekn9qwf37cu2rn755upcp6el"),
        ] {
            let branch = bitcoin_branch(&account, branch)
                .unwrap_or_else(|_| panic!("published BIP84 branch must derive"));
            assert_eq!(
                bitcoin_address(&branch, index, bech32::hrp::BC)
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
}
