//! Emit only a published test fixture for the opt-in Bitcoin Core checker.
use monoform_wallet::{BitcoinError, OfflineSeed};

fn main() -> Result<(), BitcoinError> {
    // Public Trezor vector: never use these words to protect real funds.
    let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
    let seed =
        OfflineSeed::from_phrase(phrase.to_owned()).map_err(|_| BitcoinError::DerivationFailed)?;
    let mut account = seed.bitcoin_testnet_account()?;
    drop(seed);
    let descriptors = account.descriptors()?;
    // These library-generated strings have a fixed ASCII alphabet, so need no JSON escaping.
    println!(
        r#"{{"descriptors":["{}","{}"],"addresses":[["{}","{}"],["{}","{}"]]}}"#,
        descriptors.receive,
        descriptors.change,
        account.next_receive_address()?,
        account.next_receive_address()?,
        account.next_change_address()?,
        account.next_change_address()?,
    );
    Ok(())
}
