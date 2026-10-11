#![cfg(all(feature = "core-rpc", any(target_os = "macos", target_os = "linux")))]
//! Opt-in check against an isolated Core node, never a user wallet or real seed.
use monoform_wallet::{
    OfflineSeed,
    core_rpc::{CoreTestnet4, NodeState},
};
use std::path::PathBuf;

#[test]
#[ignore = "requires an isolated wallet-disabled, peer-disabled testnet4 Core node"]
fn core_accepts_public_descriptors_and_matches_both_address_ranges() {
    let datadir = std::env::var_os("MONOFORM_TEST_CORE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set isolated Core directory"));
    let client = CoreTestnet4::new(&datadir).unwrap_or_else(|_| panic!("explicit directory"));
    let status = client
        .status()
        .unwrap_or_else(|_| panic!("Core identity/status"));
    assert_eq!(status.blocks, 0);
    assert_eq!(status.state, NodeState::Syncing);
    // Published Trezor fixture, not a recovery phrase belonging to a user.
    let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
    let seed =
        OfflineSeed::from_phrase(phrase.to_owned()).unwrap_or_else(|_| panic!("public fixture"));
    let account = seed
        .bitcoin_testnet_account()
        .unwrap_or_else(|_| panic!("public account"));
    drop(seed);
    let before = account.encode_public_state();
    assert_eq!(client.verify_account(&account), Ok(()));
    assert_eq!(account.encode_public_state(), before);
}
