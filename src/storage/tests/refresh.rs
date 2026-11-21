//! Coordinated refresh against synthetic Core and an isolated public-state store.
use super::*;
use crate::core_rpc::{
    CoreError, DiscoveryError, DiscoveryLimits, IncompleteReason, MempoolError,
    MempoolIncompleteReason, RefreshError, discovery_replies, mock_core, pending_events,
    pending_replies, rpc_calls,
};

pub(super) fn replies(
    account: &BitcoinTestnetAccount,
    used: [Option<u32>; 2],
    pending: bool,
) -> Vec<(u16, String)> {
    let mut replies = discovery_replies(account, used, 102);
    replies.extend(pending_replies(
        if pending { &[4, 5] } else { &[] },
        if pending { pending_events() } else { vec![] },
        true,
    ));
    replies
}

#[test]
fn one_refresh_binds_examined_ranges_exact_balances_and_atomic_counters() {
    let (directory, mut store, credential) = fixture();
    let responses = replies(&account(), [Some(3), Some(5)], true);
    let cap = responses.len() as u32;
    let (core, _cookie, server) = mock_core(responses);
    let result = core
        .refresh_watch_only(
            &mut store,
            DiscoveryLimits {
                max_requests: cap,
                ..Default::default()
            },
        )
        .unwrap_or_else(|_| panic!("complete refresh"));
    assert_eq!(counters(&store), (4, 6));
    assert_eq!(
        result.discovery.anchor_height(),
        result.observation.confirmed.anchor_height
    );
    assert_eq!(
        result.discovery.anchor_hash(),
        result.observation.confirmed.anchor_hash
    );
    assert_eq!(result.observation.confirmed.scope.receive_count, 40);
    assert_eq!(result.observation.confirmed.scope.change_count, 40);
    assert_eq!(result.observation.confirmed.balance.total_sats, 100_000_000);
    assert_eq!(result.observation.summary.projected_total_sats, 99_998_000);
    let calls = rpc_calls(server);
    assert_eq!(calls.len(), cap as usize);
    let pending = calls
        .iter()
        .find(|call| call["method"] == "getdescriptoractivity" && call["params"][2] == true)
        .unwrap_or_else(|| panic!("pending call"));
    assert_eq!(pending["params"][1][0]["range"], serde_json::json!([0, 39]));
    assert_eq!(pending["params"][1][1]["range"], serde_json::json!([0, 39]));
    drop(store);
    let reopened = connected(&directory.path().join("wallet"), credential)
        .unwrap_or_else(|_| panic!("reopen"));
    assert_eq!(counters(&reopened), (4, 6));
}

#[test]
fn repeated_refresh_observes_dropped_pending_activity_without_rewriting_counters() {
    let (_directory, mut store, credential) = fixture();
    let (core, _cookie, server) = mock_core(replies(&account(), [Some(3), Some(5)], true));
    let first = core
        .refresh_watch_only(&mut store, Default::default())
        .unwrap_or_else(|_| panic!("first refresh"));
    rpc_calls(server);
    let encrypted = store.read_file(STATE).unwrap_or_else(|_| panic!("read"));
    let writes = credential
        .0
        .lock()
        .unwrap_or_else(|_| panic!("lock"))
        .writes;
    let snapshot = store
        .public_account()
        .unwrap_or_else(|_| panic!("snapshot"));
    let (core, _cookie, server) = mock_core(replies(&snapshot, [None, None], false));
    let fresh = core
        .refresh_watch_only(&mut store, Default::default())
        .unwrap_or_else(|_| panic!("fresh refresh"));
    rpc_calls(server);
    assert_eq!(first.observation.summary.projected_total_sats, 99_998_000);
    assert_eq!(fresh.observation.summary.projected_total_sats, 100_000_000);
    assert!(fresh.observation.spends.is_empty());
    assert_eq!(counters(&store), (4, 6));
    assert_eq!(
        store.read_file(STATE).unwrap_or_else(|_| panic!("read")),
        encrypted
    );
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
fn one_request_cap_covers_discovery_and_pending_work_without_reset() {
    for cap in [7, 8] {
        let (_directory, mut store, credential) = fixture();
        let mut responses = discovery_replies(&account(), [None, None], 102);
        responses.truncate(cap as usize);
        let (core, _cookie, server) = mock_core(responses);
        let result = core.refresh_watch_only(
            &mut store,
            DiscoveryLimits {
                max_requests: cap,
                ..Default::default()
            },
        );
        let expected = if cap == 7 {
            RefreshError::Discovery(DiscoveryError::Incomplete(IncompleteReason::RequestLimit))
        } else {
            RefreshError::Observation(MempoolError::Incomplete(
                MempoolIncompleteReason::RequestLimit,
            ))
        };
        assert!(matches!(result, Err(error) if error == expected));
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
}

#[test]
fn initial_storage_and_invalid_limits_fail_before_network_disclosure() {
    let (_directory, mut store, credential) = fixture();
    let (core, _cookie, server) = mock_core(vec![]);
    credential
        .0
        .lock()
        .unwrap_or_else(|_| panic!("lock"))
        .unavailable = true;
    assert!(matches!(
        core.refresh_watch_only(&mut store, Default::default()),
        Err(RefreshError::Load(StorageError::KeyStore))
    ));
    credential
        .0
        .lock()
        .unwrap_or_else(|_| panic!("lock"))
        .unavailable = false;
    assert!(matches!(
        core.refresh_watch_only(
            &mut store,
            DiscoveryLimits {
                max_requests: 0,
                ..Default::default()
            }
        ),
        Err(RefreshError::Discovery(DiscoveryError::InvalidLimits))
    ));
    assert!(rpc_calls(server).is_empty());
    assert_eq!(counters(&store), (0, 0));
}

#[test]
fn discovery_observation_and_cross_stage_anchor_failures_do_not_commit() {
    for case in 0..4 {
        let (_directory, mut store, credential) = fixture();
        let before = store.read_file(STATE).unwrap_or_else(|_| panic!("read"));
        let mut responses = discovery_replies(
            &account(),
            [Some(3), Some(5)],
            if case == 2 { 101 } else { 102 },
        );
        let expected = match case {
            0 => {
                responses.truncate(4);
                responses[3] = (
                    200,
                    "{\"jsonrpc\":\"2.0\",\"id\":\"monoform\",\"error\":{\"code\":-1}}".to_owned(),
                );
                RefreshError::Discovery(DiscoveryError::Incomplete(
                    IncompleteReason::HistoryUnavailable,
                ))
            }
            1 => {
                responses.push((500, "{}".to_owned()));
                RefreshError::Observation(MempoolError::Backend(CoreError::Unavailable))
            }
            _ => {
                // A later stable balance tip, or a different hash at the same height.
                if case == 3 {
                    for (_, body) in &mut responses {
                        *body = body.replace(&format!("{:064x}", 1), &format!("{:064x}", 9));
                    }
                }
                responses.extend(pending_replies(&[], vec![], true));
                RefreshError::SnapshotChanged
            }
        };
        let (core, _cookie, server) = mock_core(responses);
        let failed = core
            .refresh_watch_only(&mut store, Default::default())
            .err()
            .unwrap_or_else(|| panic!("case {case} must fail"));
        assert_eq!(failed, expected, "case {case}");
        rpc_calls(server);
        assert_eq!(counters(&store), (0, 0));
        assert_eq!(
            store.read_file(STATE).unwrap_or_else(|_| panic!("read")),
            before
        );
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
fn commit_errors_are_distinct_and_retry_recovers_both_counters() {
    for after_rename in [false, true] {
        let (directory, mut store, credential) = fixture();
        {
            let mut state = credential.0.lock().unwrap_or_else(|_| panic!("lock"));
            state.fail_write = Some(state.writes + if after_rename { 2 } else { 1 });
        }
        let (core, _cookie, server) = mock_core(replies(&account(), [Some(3), Some(5)], false));
        assert!(matches!(
            core.refresh_watch_only(&mut store, Default::default()),
            Err(RefreshError::Commit(StorageError::KeyStore))
        ));
        rpc_calls(server);
        drop(store);
        let mut reopened = connected(&directory.path().join("wallet"), credential)
            .unwrap_or_else(|_| panic!("reopen"));
        assert_eq!(
            counters(&reopened),
            if after_rename { (4, 6) } else { (0, 0) }
        );
        let snapshot = reopened
            .public_account()
            .unwrap_or_else(|_| panic!("snapshot"));
        let (core, _cookie, server) = mock_core(replies(&snapshot, [Some(3), Some(5)], false));
        assert!(
            core.refresh_watch_only(&mut reopened, Default::default())
                .is_ok()
        );
        rpc_calls(server);
        assert_eq!(counters(&reopened), (4, 6));
    }
}

#[test]
#[ignore = "waits 61 seconds to verify an OS write returning after the real deadline"]
fn slow_commit_reports_deadline_after_persisting() {
    let (_directory, mut store, credential) = fixture();
    {
        let mut state = credential.0.lock().unwrap_or_else(|_| panic!("lock"));
        state.pause_after_write = Some(state.writes + 2);
    }
    let (core, _cookie, server) = mock_core(replies(&account(), [Some(3), Some(5)], false));
    assert!(matches!(
        core.refresh_watch_only(&mut store, Default::default()),
        Err(RefreshError::CommittedDeadline)
    ));
    rpc_calls(server);
    assert_eq!(counters(&store), (4, 6));
}
