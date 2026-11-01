//! Public fixture keys, real discovery API on synthetic loopback, protected storage.
use super::*;
use crate::core_rpc::complete_discovery;

#[test]
fn discovery_saves_both_branches_and_resumes_the_expected_addresses() {
    let (directory, mut store, credential) = fixture();
    let snapshot = store
        .public_account()
        .unwrap_or_else(|_| panic!("snapshot"));
    let before = snapshot.encode_public_state();
    let result = complete_discovery(&snapshot, [Some(3), Some(5)]);
    assert_eq!(result.receive().next_index, 4);
    assert_eq!(result.change().next_index, 6);
    assert_eq!(counters(&store), (0, 0));
    assert_eq!(snapshot.encode_public_state(), before);
    assert_eq!(store.apply_discovery(&result), Ok(()));
    assert_eq!(counters(&store), (4, 6));
    drop(store);
    let mut reopened = connected(&directory.path().join("wallet"), credential)
        .unwrap_or_else(|_| panic!("reopen"));
    let mut expected = account();
    expected.next_receive = 4;
    expected.next_change = 6;
    assert_eq!(
        reopened.reserve_receive_address(),
        expected
            .next_receive_address()
            .map_err(|_| StorageError::Derivation)
    );
    assert_eq!(
        reopened.reserve_change_address(),
        expected
            .next_change_address()
            .map_err(|_| StorageError::Derivation)
    );
    assert_eq!(
        reopened
            .public_account()
            .unwrap_or_else(|_| panic!("snapshot"))
            .encode_public_state(),
        expected.encode_public_state()
    );
}

#[test]
fn older_results_merge_upward_and_reapplication_does_not_write() {
    let (_directory, mut store, credential) = fixture();
    let result = complete_discovery(&account(), [Some(3), Some(5)]);
    // Later reservations can exceed the old result's examined address window.
    for _ in 0..47 {
        assert!(store.reserve_receive_address().is_ok());
    }
    assert!(store.apply_discovery(&result).is_ok());
    assert_eq!(counters(&store), (47, 6));
    let old = complete_discovery(&account(), [Some(0), Some(1)]);
    let empty = complete_discovery(&account(), [None, None]);
    let encrypted = store.read_file(STATE).unwrap_or_else(|_| panic!("read"));
    let writes = credential
        .0
        .lock()
        .unwrap_or_else(|_| panic!("lock"))
        .writes;
    for repeated in [&result, &old, &empty] {
        assert_eq!(store.apply_discovery(repeated), Ok(()));
        assert_eq!(counters(&store), (47, 6));
        assert_eq!(
            store.read_file(STATE).unwrap_or_else(|_| panic!("read")),
            encrypted
        );
    }
    assert_eq!(
        credential
            .0
            .lock()
            .unwrap_or_else(|_| panic!("lock"))
            .writes,
        writes
    );
}

#[test]
fn account_matching_checks_origin_full_key_chain_code_and_parent_metadata() {
    let (_directory, mut store, credential) = fixture();
    let before = store.read_file(STATE).unwrap_or_else(|_| panic!("read"));
    let writes = credential
        .0
        .lock()
        .unwrap_or_else(|_| panic!("lock"))
        .writes;
    for kind in 0..4 {
        let mut different = account();
        match kind {
            0 => different.fingerprint[0] ^= 1,
            1 => {
                // Public repeated-0x01 entropy fixture: a different key with a simulated
                // colliding 32-bit origin fingerprint must still be rejected.
                let phrase = bip39::Mnemonic::from_entropy(&[1; 32])
                    .unwrap_or_else(|_| panic!("public entropy"));
                let other = OfflineSeed::from_phrase(phrase.to_string())
                    .unwrap_or_else(|_| panic!("public phrase"))
                    .bitcoin_testnet_account()
                    .unwrap_or_else(|_| panic!("public key"));
                different.account = other.account;
            }
            _ => {
                let mut key = different.account.to_extended_key(bip32::Prefix::TPUB);
                if kind == 2 {
                    key.attrs.chain_code[0] ^= 1;
                } else {
                    key.attrs.parent_fingerprint[0] ^= 1;
                }
                different.account = bip32::XPub::try_from(key)
                    .unwrap_or_else(|_| panic!("valid alternate public metadata"));
            }
        }
        let result = complete_discovery(&different, [Some(3), Some(5)]);
        assert_eq!(
            store.apply_discovery(&result),
            Err(StorageError::AccountMismatch)
        );
        assert_eq!(counters(&store), (0, 0));
        assert_eq!(
            store.read_file(STATE).unwrap_or_else(|_| panic!("read")),
            before
        );
    }
    assert_eq!(
        credential
            .0
            .lock()
            .unwrap_or_else(|_| panic!("lock"))
            .writes,
        writes
    );
}

#[test]
fn interrupted_application_recovers_both_old_or_both_new_counters() {
    for after_rename in [false, true] {
        let (directory, mut store, credential) = fixture();
        let result = complete_discovery(&account(), [Some(3), Some(5)]);
        {
            let mut state = credential.0.lock().unwrap_or_else(|_| panic!("lock"));
            state.fail_write = Some(state.writes + if after_rename { 2 } else { 1 });
        }
        assert_eq!(store.apply_discovery(&result), Err(StorageError::KeyStore));
        drop(store);
        let mut reopened = connected(&directory.path().join("wallet"), credential)
            .unwrap_or_else(|_| panic!("reopen"));
        assert_eq!(
            counters(&reopened),
            if after_rename { (4, 6) } else { (0, 0) }
        );
        assert_eq!(reopened.apply_discovery(&result), Ok(()));
        assert_eq!(counters(&reopened), (4, 6));
    }
}

#[test]
fn unavailable_credentials_and_file_rollback_never_accept_recommendations() {
    let (directory, mut store, credential) = fixture();
    let result = complete_discovery(&account(), [Some(3), Some(5)]);
    let before = store
        .read_file(STATE)
        .unwrap_or_else(|_| panic!("read"))
        .unwrap_or_else(|| panic!("state exists"));
    credential
        .0
        .lock()
        .unwrap_or_else(|_| panic!("lock"))
        .unavailable = true;
    assert_eq!(store.apply_discovery(&result), Err(StorageError::KeyStore));
    assert_eq!(
        store.read_file(STATE).unwrap_or_else(|_| panic!("read")),
        Some(before.clone())
    );
    credential
        .0
        .lock()
        .unwrap_or_else(|_| panic!("lock"))
        .unavailable = false;
    assert!(store.reserve_receive_address().is_ok());
    write_artifact(&directory.path().join("wallet").join(STATE), &before);
    assert_eq!(store.apply_discovery(&result), Err(StorageError::Rollback));
}

#[test]
fn exhausted_current_counters_are_not_reduced_by_an_older_scan() {
    let (_directory, mut store, _credential) = fixture();
    let result = complete_discovery(&account(), [Some(3), Some(5)]);
    let (mut current, mut anchor) = store.load().unwrap_or_else(|_| panic!("load"));
    current.next_receive = bip32::ChildNumber::HARDENED_FLAG;
    assert!(store.commit(&current, &mut anchor).is_ok());
    assert!(store.apply_discovery(&result).is_ok());
    assert_eq!(counters(&store), (bip32::ChildNumber::HARDENED_FLAG, 6));
    assert_eq!(
        store.reserve_receive_address(),
        Err(StorageError::Derivation)
    );
    assert!(store.reserve_change_address().is_ok());
}
