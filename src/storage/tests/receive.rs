//! Public-vector account, synthetic loopback history, and isolated protected state.
use super::*;
use crate::core_rpc::{
    DiscoveryError, DiscoveryLimits, IncompleteReason, ReceiveError, complete_discovery,
    discovery_replies, mock_core, rpc_calls,
};

pub(super) fn replies(
    account: &BitcoinTestnetAccount,
    used: [Option<u32>; 2],
) -> Vec<(u16, String)> {
    let mut responses = discovery_replies(account, used, 102);
    let tip = responses[..2].to_vec();
    responses.extend(tip);
    responses
}

#[test]
fn fresh_receive_commits_both_discovery_counters_and_one_address_at_exact_budget() {
    let (directory, mut store, credential) = fixture();
    let responses = replies(&account(), [Some(3), Some(5)]);
    let cap = responses.len() as u32;
    let (core, _cookie, server) = mock_core(responses);
    let received = core
        .receive_address(
            &mut store,
            DiscoveryLimits {
                max_requests: cap,
                ..Default::default()
            },
        )
        .unwrap_or_else(|_| panic!("fresh protected receive"));
    let mut expected = account();
    expected.next_receive = 4;
    assert_eq!(
        Some(received.address.clone()),
        expected.next_receive_address().ok()
    );
    assert_eq!(received.index, 4);
    assert_eq!(received.anchor_height, 102);
    assert!(received.observed_at <= std::time::SystemTime::now());
    assert_eq!(counters(&store), (5, 6));
    assert_eq!(
        credential
            .0
            .lock()
            .unwrap_or_else(|_| panic!("lock"))
            .writes,
        4,
        "one commit after initialization, not separate discovery and reservation writes"
    );
    let calls = rpc_calls(server);
    assert_eq!(calls.len(), cap as usize);
    assert!(!calls.iter().any(|call| call["method"] == "getmempoolinfo"));
    drop(store);
    let reopened = connected(&directory.path().join("wallet"), credential)
        .unwrap_or_else(|_| panic!("reopen"));
    assert_eq!(counters(&reopened), (5, 6));
}

#[test]
fn twentieth_unused_address_is_allowed_twenty_first_is_denied_until_confirmed_usage() {
    let (_directory, mut store, credential) = fixture();
    let (mut saved, mut anchor) = store.load().unwrap_or_else(|_| panic!("load"));
    saved.next_receive = 19;
    assert!(store.commit(&saved, &mut anchor).is_ok());
    for (used, expected) in [(None, Some(19)), (None, None), (Some(19), Some(20))] {
        let snapshot = store
            .public_account()
            .unwrap_or_else(|_| panic!("snapshot"));
        let before = store.read_file(STATE).unwrap_or_else(|_| panic!("read"));
        let writes = credential
            .0
            .lock()
            .unwrap_or_else(|_| panic!("lock"))
            .writes;
        let (core, _cookie, server) = mock_core(replies(&snapshot, [used, Some(5)]));
        let received = core.receive_address(&mut store, Default::default());
        rpc_calls(server);
        if let Some(index) = expected {
            assert!(matches!(received, Ok(value) if value.index == index));
            assert_eq!(counters(&store), (index + 1, 6));
        } else {
            assert!(matches!(received, Err(ReceiveError::GapLimit)));
            assert_eq!(store.read_file(STATE).ok(), Some(before));
            assert_eq!(
                credential
                    .0
                    .lock()
                    .unwrap_or_else(|_| panic!("lock"))
                    .writes,
                writes
            );
            assert_eq!(counters(&store), (20, 6));
        }
    }
}

#[test]
fn changing_or_unready_final_tip_never_reserves_or_saves_discovery() {
    for case in 0..5 {
        let (_directory, mut store, credential) = fixture();
        let mut responses = replies(&account(), [Some(3), Some(5)]);
        let length = responses.len();
        let mut response: serde_json::Value = serde_json::from_str(&responses[length - 2].1)
            .unwrap_or_else(|_| panic!("public response"));
        let expected = match case {
            0 => {
                response["result"]["blocks"] = serde_json::json!(103);
                response["result"]["headers"] = serde_json::json!(103);
                ReceiveError::SnapshotChanged
            }
            1 => {
                response["result"]["bestblockhash"] = serde_json::json!(format!("{:064x}", 9));
                ReceiveError::SnapshotChanged
            }
            2 => {
                response["result"]["initialblockdownload"] = serde_json::json!(true);
                ReceiveError::Discovery(DiscoveryError::Incomplete(IncompleteReason::Syncing))
            }
            3 => {
                response["result"]["pruned"] = serde_json::json!(true);
                ReceiveError::Discovery(DiscoveryError::Incomplete(IncompleteReason::PrunedHistory))
            }
            _ => {
                response["result"]["time"] = serde_json::json!(1);
                ReceiveError::Discovery(DiscoveryError::Incomplete(IncompleteReason::StaleTip))
            }
        };
        responses[length - 2].1 = response.to_string();
        let before = store.read_file(STATE).unwrap_or_else(|_| panic!("read"));
        let (core, _cookie, server) = mock_core(responses);
        assert!(
            matches!(core.receive_address(&mut store, Default::default()), Err(error) if error == expected)
        );
        rpc_calls(server);
        assert_eq!(store.read_file(STATE).ok(), Some(before));
        assert_eq!(
            credential
                .0
                .lock()
                .unwrap_or_else(|_| panic!("lock"))
                .writes,
            2
        );
    }
}

#[test]
fn missing_history_wrong_chain_and_reorg_return_no_address_or_counter_updates() {
    for case in 0..3 {
        let (_directory, mut store, credential) = fixture();
        let mut responses = replies(&account(), [None, None]);
        let expected = match case {
            0 => {
                responses.truncate(4);
                responses[3].1 = responses[3]
                    .1
                    .replace("\"completed\":true", "\"completed\":false");
                ReceiveError::Discovery(DiscoveryError::Incomplete(
                    IncompleteReason::HistoryUnavailable,
                ))
            }
            1 => {
                responses.truncate(1);
                responses[0].1 = responses[0].1.replace("testnet4", "main");
                ReceiveError::Discovery(DiscoveryError::Backend(
                    crate::core_rpc::CoreError::WrongChain,
                ))
            }
            _ => {
                responses.truncate(3);
                responses[2].1 = responses[2]
                    .1
                    .replace(&format!("{:064x}", 1), &format!("{:064x}", 9));
                ReceiveError::Discovery(DiscoveryError::Reorg)
            }
        };
        let (core, _cookie, server) = mock_core(responses);
        assert!(
            matches!(core.receive_address(&mut store, Default::default()), Err(error) if error == expected)
        );
        rpc_calls(server);
        assert_eq!(counters(&store), (0, 0));
        assert_eq!(
            credential
                .0
                .lock()
                .unwrap_or_else(|_| panic!("lock"))
                .writes,
            2
        );
    }
}

#[test]
fn discovery_and_final_tip_share_one_request_cap() {
    let (_directory, mut store, credential) = fixture();
    let mut responses = replies(&account(), [Some(3), Some(5)]);
    let cap = responses.len() as u32 - 1;
    responses.truncate(cap as usize);
    let (core, _cookie, server) = mock_core(responses);
    assert!(matches!(
        core.receive_address(
            &mut store,
            DiscoveryLimits {
                max_requests: cap,
                ..Default::default()
            }
        ),
        Err(ReceiveError::Discovery(DiscoveryError::Incomplete(
            IncompleteReason::RequestLimit
        )))
    ));
    assert_eq!(rpc_calls(server).len(), cap as usize);
    assert_eq!(counters(&store), (0, 0));
    assert_eq!(
        credential
            .0
            .lock()
            .unwrap_or_else(|_| panic!("lock"))
            .writes,
        2
    );
}

#[test]
fn full_key_binding_and_saved_counters_cannot_be_bypassed_by_an_old_scan() {
    let (_directory, mut store, _credential) = fixture();
    let mut different = account();
    let mut key = different.account.to_extended_key(bip32::Prefix::TPUB);
    key.attrs.chain_code[0] ^= 1;
    different.account = bip32::XPub::try_from(key).unwrap_or_else(|_| panic!("public metadata"));
    let wrong = complete_discovery(&different, [None, None]);
    assert!(matches!(
        store.reserve_discovered_receive(&wrong),
        Err(StorageError::AccountMismatch)
    ));
    let old = complete_discovery(&account(), [Some(3), Some(5)]);
    let (mut saved, mut anchor) = store.load().unwrap_or_else(|_| panic!("load"));
    saved.next_receive = 47;
    assert!(store.commit(&saved, &mut anchor).is_ok());
    let before = store.read_file(STATE).unwrap_or_else(|_| panic!("read"));
    assert!(matches!(
        store.reserve_discovered_receive(&old),
        Err(StorageError::GapLimit)
    ));
    assert_eq!(counters(&store), (47, 0));
    assert_eq!(store.read_file(STATE).ok(), Some(before));
}

#[test]
fn locked_credentials_limits_and_exhausted_cursors_fail_before_disclosure() {
    let (_directory, mut store, credential) = fixture();
    let (core, _cookie, server) = mock_core(vec![]);
    credential
        .0
        .lock()
        .unwrap_or_else(|_| panic!("lock"))
        .unavailable = true;
    assert!(matches!(
        core.receive_address(&mut store, Default::default()),
        Err(ReceiveError::Load(StorageError::KeyStore))
    ));
    credential
        .0
        .lock()
        .unwrap_or_else(|_| panic!("lock"))
        .unavailable = false;
    assert!(matches!(
        core.receive_address(
            &mut store,
            DiscoveryLimits {
                max_requests: 0,
                ..Default::default()
            }
        ),
        Err(ReceiveError::Discovery(DiscoveryError::InvalidLimits))
    ));
    for (index, reason) in [
        (200, IncompleteReason::AddressLimit),
        (
            bip32::ChildNumber::HARDENED_FLAG,
            IncompleteReason::AddressSpaceExhausted,
        ),
    ] {
        let (mut saved, mut anchor) = store.load().unwrap_or_else(|_| panic!("load"));
        saved.next_receive = index;
        assert!(store.commit(&saved, &mut anchor).is_ok());
        assert!(
            matches!(core.receive_address(&mut store, Default::default()), Err(ReceiveError::Discovery(DiscoveryError::Incomplete(error))) if error == reason)
        );
        assert_eq!(counters(&store).0, index);
    }
    assert!(rpc_calls(server).is_empty());
}

#[test]
fn interrupted_reservation_recovers_atomically_and_retry_never_rewinds() {
    for after_rename in [false, true] {
        let (directory, mut store, credential) = fixture();
        {
            let mut state = credential.0.lock().unwrap_or_else(|_| panic!("lock"));
            state.fail_write = Some(state.writes + if after_rename { 2 } else { 1 });
        }
        let (core, _cookie, server) = mock_core(replies(&account(), [Some(3), Some(5)]));
        assert!(matches!(
            core.receive_address(&mut store, Default::default()),
            Err(ReceiveError::Commit(StorageError::KeyStore))
        ));
        rpc_calls(server);
        drop(store);
        let mut reopened = connected(&directory.path().join("wallet"), credential)
            .unwrap_or_else(|_| panic!("reopen"));
        assert_eq!(
            counters(&reopened),
            if after_rename { (5, 6) } else { (0, 0) }
        );
        let snapshot = reopened
            .public_account()
            .unwrap_or_else(|_| panic!("snapshot"));
        let (core, _cookie, server) = mock_core(replies(&snapshot, [Some(3), Some(5)]));
        let received = core
            .receive_address(&mut reopened, Default::default())
            .unwrap_or_else(|_| panic!("retry"));
        rpc_calls(server);
        assert_eq!(received.index, if after_rename { 5 } else { 4 });
        assert_eq!(counters(&reopened), (received.index + 1, 6));
    }
}

#[test]
#[ignore = "waits 61 seconds to verify a late acknowledged receive commit returns no address"]
fn slow_receive_commit_returns_no_address_after_persisting_reservation() {
    let (_directory, mut store, credential) = fixture();
    {
        let mut state = credential.0.lock().unwrap_or_else(|_| panic!("lock"));
        state.pause_after_write = Some(state.writes + 2);
    }
    let (core, _cookie, server) = mock_core(replies(&account(), [Some(3), Some(5)]));
    assert!(matches!(
        core.receive_address(&mut store, Default::default()),
        Err(ReceiveError::CommittedDeadline)
    ));
    rpc_calls(server);
    assert_eq!(counters(&store), (5, 6));
}
