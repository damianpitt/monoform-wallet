//! Synthetic history on loopback plus the already published public seed fixture.
use super::*;
use crate::core_rpc::tests::{calls, chain, envelope, fixture, mock};
use serde_json::Value;

fn hash(number: u8) -> String {
    format!("{number:064x}")
}
fn ok(value: Value) -> (u16, String) {
    (200, envelope(value))
}
fn recent(height: u32) -> Value {
    let mut value = chain();
    value["blocks"] = json!(height);
    value["headers"] = json!(height);
    value["bestblockhash"] = json!(hash(1));
    value["initialblockdownload"] = json!(false);
    value["time"] = json!(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| panic!("test clock"))
            .as_secs()
    );
    value
}
fn start(info: Value) -> Vec<(u16, String)> {
    vec![ok(info), ok(json!(GENESIS)), ok(json!(hash(1)))]
}
fn finish() -> Vec<(u16, String)> {
    start(recent(2))
}
fn scan(from: u32, to: u32, relevant: Vec<String>) -> Value {
    json!({"from_height":from,"to_height":to,"relevant_blocks":relevant,"completed":true})
}
fn empty() -> (u16, String) {
    ok(scan(0, 2, vec![]))
}
fn event(kind: &str, branch: u32, index: u32, height: u32, number: u8) -> Value {
    let account = fixture();
    let script =
        scripts(&account, branch, index, index).unwrap_or_else(|_| panic!("public script"));
    let key = if kind == "receive" {
        "output_spk"
    } else {
        "prevout_spk"
    };
    json!({"type":kind,"height":height,"blockhash":hash(number),key:{"hex":script.keys().next()
        .unwrap_or_else(|| panic!("script entry"))}})
}

#[test]
fn empty_history_covers_both_branches_without_changing_saved_state() {
    let account = fixture();
    let before = account.encode_public_state();
    let mut replies = start(recent(2));
    replies.extend([empty(), empty()]);
    replies.extend(finish());
    let (client, _directory, server) = mock(replies);
    let result = client
        .discover(&account, DiscoveryLimits::default())
        .unwrap_or_else(|_| panic!("complete synthetic discovery"));
    assert_eq!(result.anchor_height, 2);
    assert_eq!(result.anchor_hash, hash(1));
    for branch in [result.receive, result.change] {
        assert_eq!(branch.examined, 20);
        assert_eq!(branch.last_used, None);
        assert_eq!(branch.next_index, 0);
    }
    assert_eq!(account.encode_public_state(), before);
    let requests = calls(server);
    assert_eq!(requests.len(), 8);
    for (branch, call) in requests[3..5].iter().enumerate() {
        assert_eq!(call["method"], "scanblocks");
        assert_eq!(call["params"][0], "start");
        assert_eq!(call["params"][1][0]["range"], json!([0, 19]));
        let descriptors = account
            .descriptors()
            .unwrap_or_else(|_| panic!("descriptors"));
        assert_eq!(
            call["params"][1][0]["desc"],
            if branch == 0 {
                descriptors.receive
            } else {
                descriptors.change
            }
        );
        assert_eq!(call["params"][2], 0);
        assert_eq!(call["params"][3], 2);
    }
}

#[test]
fn fully_spent_history_extends_both_gap_limits_and_never_uses_balance() {
    let account = fixture();
    let before = account.encode_public_state();
    let mut replies = start(recent(2));
    for (branch, index) in [(0, 3), (1, 5)] {
        replies.push(ok(scan(0, 2, vec![hash(2), hash(1)])));
        for (kind, height, number) in [("receive", 1, 2), ("spend", 2, 1)] {
            replies.push(ok(
                json!({"activity":[event(kind, branch, index, height, number)]}),
            ));
            replies.push(ok(json!(hash(number))));
        }
        replies.push(empty());
    }
    replies.extend(finish());
    let (client, _directory, server) = mock(replies);
    let result = client
        .discover(&account, DiscoveryLimits::default())
        .unwrap_or_else(|_| panic!("spent outputs still count as usage"));
    assert_eq!(result.receive.last_used, Some(3));
    assert_eq!(result.receive.next_index, 4);
    assert_eq!(result.change.last_used, Some(5));
    assert_eq!(result.change.next_index, 6);
    assert_eq!(result.receive.examined, 40);
    assert_eq!(result.change.examined, 40);
    assert_eq!(account.encode_public_state(), before);
    for request in calls(server) {
        assert!(matches!(
            request["method"].as_str(),
            Some("getblockchaininfo" | "getblockhash" | "scanblocks" | "getdescriptoractivity")
        ));
        if request["method"] == "getdescriptoractivity" {
            assert_eq!(request["params"][2], false);
        }
    }
}

#[test]
fn saved_counters_require_twenty_unused_addresses_beyond_them() {
    let mut account = fixture();
    for _ in 0..23 {
        account
            .next_receive_address()
            .unwrap_or_else(|_| panic!("public address"));
    }
    for _ in 0..1 {
        account
            .next_change_address()
            .unwrap_or_else(|_| panic!("public change"));
    }
    let before = account.encode_public_state();
    let mut replies = start(recent(2));
    replies.extend([empty(), empty(), empty(), empty(), empty()]);
    replies.extend(finish());
    let (client, _directory, server) = mock(replies);
    let limits = DiscoveryLimits {
        max_addresses_per_branch: 43,
        ..DiscoveryLimits::default()
    };
    let result = client
        .discover(&account, limits)
        .unwrap_or_else(|_| panic!("saved cursors"));
    assert_eq!(result.receive.examined, 43);
    assert_eq!(result.receive.next_index, 23);
    assert_eq!(result.change.examined, 40);
    assert_eq!(result.change.next_index, 1);
    assert_eq!(account.encode_public_state(), before);
    let requests = calls(server);
    assert_eq!(requests[5]["params"][1][0]["range"], json!([40, 42]));
}

#[test]
fn false_positive_filters_are_not_activity() {
    let mut replies = start(recent(2));
    replies.extend([
        ok(scan(0, 2, vec![hash(2)])),
        ok(json!({"activity":[]})),
        empty(),
    ]);
    replies.extend(finish());
    let (client, _directory, server) = mock(replies);
    let result = client
        .discover(&fixture(), DiscoveryLimits::default())
        .unwrap_or_else(|_| panic!("filter false positive"));
    assert_eq!(result.receive.last_used, None);
    assert_eq!(result.receive.examined, 20);
    calls(server);
}

#[test]
fn incomplete_nodes_and_height_limit_never_disclose_descriptors() {
    for (field, value, expected) in [
        ("pruned", json!(true), IncompleteReason::PrunedHistory),
        (
            "initialblockdownload",
            json!(true),
            IncompleteReason::Syncing,
        ),
        ("time", json!(1714777860_u64), IncompleteReason::StaleTip),
    ] {
        let mut info = recent(2);
        info[field] = value;
        let (client, _directory, server) = mock(vec![ok(info), ok(json!(GENESIS))]);
        assert!(
            matches!(client.discover(&fixture(),DiscoveryLimits::default()), Err(DiscoveryError::Incomplete(reason)) if reason == expected)
        );
        assert_eq!(calls(server).len(), 2);
    }
    let (client, _directory, server) = mock(vec![ok(recent(2)), ok(json!(GENESIS))]);
    assert!(matches!(
        client.discover(
            &fixture(),
            DiscoveryLimits {
                max_height: 1,
                ..DiscoveryLimits::default()
            }
        ),
        Err(DiscoveryError::Incomplete(IncompleteReason::BlockLimit))
    ));
    assert_eq!(calls(server).len(), 2);
}

#[test]
fn address_limits_and_exhausted_cursors_never_reuse_an_address() {
    let (client, _directory, server) = mock(vec![]);
    let mut account = fixture();
    account.next_receive = ChildNumber::HARDENED_FLAG;
    assert!(matches!(
        client.discover(&account, DiscoveryLimits::default()),
        Err(DiscoveryError::Incomplete(
            IncompleteReason::AddressSpaceExhausted
        ))
    ));
    account.next_receive = 181;
    assert!(matches!(
        client.discover(&account, DiscoveryLimits::default()),
        Err(DiscoveryError::Incomplete(IncompleteReason::AddressLimit))
    ));
    assert_eq!(account.next_receive, 181);
    assert!(calls(server).is_empty());

    let account = fixture();
    let mut replies = start(recent(2));
    replies.extend([
        ok(scan(0, 2, vec![hash(2)])),
        ok(json!({"activity":[event("receive",0,19,1,2)]})),
        ok(json!(hash(2))),
    ]);
    let (client, _directory, server) = mock(replies);
    assert!(matches!(
        client.discover(
            &account,
            DiscoveryLimits {
                max_addresses_per_branch: 20,
                ..DiscoveryLimits::default()
            }
        ),
        Err(DiscoveryError::Incomplete(IncompleteReason::AddressLimit))
    ));
    assert_eq!(account.next_receive, 0);
    calls(server);
}

#[test]
fn request_and_deadline_budgets_fail_closed() {
    let (client, _directory, server) = mock(vec![ok(recent(2))]);
    assert!(matches!(
        client.discover(
            &fixture(),
            DiscoveryLimits {
                max_requests: 1,
                ..DiscoveryLimits::default()
            }
        ),
        Err(DiscoveryError::Incomplete(IncompleteReason::RequestLimit))
    ));
    assert_eq!(calls(server).len(), 1);
    let (client, _directory, server) = mock(vec![]);
    let mut budget = Budget {
        core: &client,
        remaining: 2,
        started: Instant::now() - DEADLINE,
    };
    assert!(matches!(
        budget.call::<String>("getblockhash", json!([0])),
        Err(DiscoveryError::Incomplete(IncompleteReason::Deadline))
    ));
    assert_eq!(budget.remaining, 2);
    assert!(calls(server).is_empty());
}

#[test]
fn invalid_limits_and_wrong_chain_fail_before_scanning() {
    let (client, _directory, server) = mock(vec![]);
    for limits in [
        DiscoveryLimits {
            max_addresses_per_branch: 19,
            ..DiscoveryLimits::default()
        },
        DiscoveryLimits {
            max_addresses_per_branch: 10_001,
            ..DiscoveryLimits::default()
        },
        DiscoveryLimits {
            max_height: 1_000_001,
            ..DiscoveryLimits::default()
        },
        DiscoveryLimits {
            max_requests: 0,
            ..DiscoveryLimits::default()
        },
        DiscoveryLimits {
            max_requests: 513,
            ..DiscoveryLimits::default()
        },
    ] {
        assert!(matches!(
            client.discover(&fixture(), limits),
            Err(DiscoveryError::InvalidLimits)
        ));
    }
    assert!(calls(server).is_empty());
    let mut info = recent(2);
    info["chain"] = json!("main");
    let (client, _directory, server) = mock(vec![ok(info)]);
    assert!(matches!(
        client.discover(&fixture(), DiscoveryLimits::default()),
        Err(DiscoveryError::Backend(CoreError::WrongChain))
    ));
    assert_eq!(calls(server).len(), 1);
}

#[test]
fn missing_history_and_aborted_scans_are_explicitly_incomplete() {
    for body in [
        json!({"jsonrpc":"2.0","id":"monoform","error":{"code":-1}}).to_string(),
        envelope(json!({"from_height":0,"to_height":2,"relevant_blocks":[],"completed":false})),
    ] {
        let mut replies = start(recent(2));
        replies.push((200, body));
        let (client, _directory, server) = mock(replies);
        assert!(matches!(
            client.discover(&fixture(), DiscoveryLimits::default()),
            Err(DiscoveryError::Incomplete(
                IncompleteReason::HistoryUnavailable
            ))
        ));
        calls(server);
    }
}

#[test]
fn inactive_block_rpc_is_reported_as_reorg_without_partial_results() {
    let mut replies = start(recent(2));
    replies.push(ok(scan(0, 2, vec![hash(2)])));
    replies.push((200,json!({"jsonrpc":"2.0","id":"monoform","error":{"code":-8,"message":"Block is not in main chain"}}).to_string()));
    let (client, _directory, server) = mock(replies);
    assert!(matches!(
        client.discover(&fixture(), DiscoveryLimits::default()),
        Err(DiscoveryError::Reorg)
    ));
    calls(server);
}

#[test]
fn latest_usage_resets_the_gap_across_address_batches() {
    let mut replies = start(recent(2));
    for index in [19, 39] {
        replies.extend([
            ok(scan(0, 2, vec![hash(2)])),
            ok(json!({"activity":[event("receive",0,index,1,2)]})),
            ok(json!(hash(2))),
        ]);
    }
    replies.extend([empty(), empty()]);
    replies.extend(finish());
    let (client, _directory, server) = mock(replies);
    let result = client
        .discover(&fixture(), DiscoveryLimits::default())
        .unwrap_or_else(|_| panic!("gap reset"));
    assert_eq!(result.receive.last_used, Some(39));
    assert_eq!(result.receive.next_index, 40);
    assert_eq!(result.receive.examined, 60);
    calls(server);
}

#[test]
fn malformed_scan_results_and_activity_cannot_establish_unused_addresses() {
    for invalid in [
        scan(1, 2, vec![]),
        scan(0, 1, vec![]),
        scan(0, 2, vec![hash(2), hash(2)]),
        scan(0, 2, vec!["bad".to_owned()]),
        json!({"completed":true}),
    ] {
        let mut replies = start(recent(2));
        replies.push(ok(invalid));
        let (client, _directory, server) = mock(replies);
        assert!(matches!(
            client.discover(&fixture(), DiscoveryLimits::default()),
            Err(DiscoveryError::Backend(CoreError::InvalidResponse))
        ));
        calls(server);
    }
    let mut wrong_script = event("receive", 0, 0, 1, 2);
    wrong_script["output_spk"]["hex"] = json!("0014bad");
    let mut unconfirmed = event("receive", 0, 0, 1, 2);
    unconfirmed
        .as_object_mut()
        .unwrap_or_else(|| panic!("object"))
        .remove("height");
    for invalid in [
        wrong_script,
        unconfirmed,
        event("receive", 0, 0, 3, 2),
        event("receive", 0, 0, 1, 3),
        event("unknown", 0, 0, 1, 2),
    ] {
        let mut replies = start(recent(2));
        replies.extend([
            ok(scan(0, 2, vec![hash(2)])),
            ok(json!({"activity":[invalid]})),
        ]);
        let (client, _directory, server) = mock(replies);
        assert!(matches!(
            client.discover(&fixture(), DiscoveryLimits::default()),
            Err(DiscoveryError::Backend(CoreError::InvalidResponse))
        ));
        calls(server);
    }
}

#[test]
fn reorg_before_scan_during_activity_or_before_return_discards_results() {
    let account = fixture();
    let before = account.encode_public_state();
    let mut at_start = start(recent(2));
    at_start[2] = ok(json!(hash(3)));
    let mut at_event = start(recent(2));
    at_event.extend([
        ok(scan(0, 2, vec![hash(2)])),
        ok(json!({"activity":[event("receive",0,0,1,2)]})),
        ok(json!(hash(3))),
    ]);
    let mut at_end = start(recent(2));
    at_end.extend([empty(), empty()]);
    at_end.extend(finish());
    *at_end.last_mut().unwrap_or_else(|| panic!("last")) = ok(json!(hash(3)));
    let mut shorter = start(recent(2));
    shorter.extend([empty(), empty(), ok(recent(1)), ok(json!(GENESIS))]);
    for replies in [at_start, at_event, at_end, shorter] {
        let (client, _directory, server) = mock(replies);
        assert!(matches!(
            client.discover(&account, DiscoveryLimits::default()),
            Err(DiscoveryError::Reorg)
        ));
        assert_eq!(account.encode_public_state(), before);
        calls(server);
    }
}

#[test]
fn chain_growth_keeps_original_anchor_and_block_chunks_are_bounded() {
    let mut replies = start(recent(4096));
    for _ in 0..2 {
        replies.extend([ok(scan(0, 4095, vec![])), ok(scan(4096, 4096, vec![]))]);
    }
    let mut after = recent(4097);
    after["bestblockhash"] = json!(hash(3));
    replies.extend([ok(after), ok(json!(GENESIS)), ok(json!(hash(1)))]);
    let (client, _directory, server) = mock(replies);
    let result = client
        .discover(&fixture(), DiscoveryLimits::default())
        .unwrap_or_else(|_| panic!("growing chain"));
    assert_eq!(result.anchor_height, 4096);
    assert_eq!(result.anchor_hash, hash(1));
    let requests = calls(server);
    for call in &requests[3..7] {
        assert_eq!(call["method"], "scanblocks");
        assert!(
            call["params"][3].as_u64().unwrap_or(0) - call["params"][2].as_u64().unwrap_or(0)
                < u64::from(BLOCK_CHUNK)
        );
    }
}

#[test]
fn scripts_match_independent_bitcoin_address_implementation() {
    use std::str::FromStr;
    let mut account = fixture();
    for branch in 0..2 {
        let generated = scripts(&account, branch, 0, 19).unwrap_or_else(|_| panic!("scripts"));
        for index in 0..20 {
            let address = if branch == 0 {
                account.next_receive_address()
            } else {
                account.next_change_address()
            }
            .unwrap_or_else(|_| panic!("public address"));
            let address = bitcoin::Address::from_str(&address)
                .unwrap_or_else(|_| panic!("parse"))
                .require_network(bitcoin::Network::Testnet4)
                .unwrap_or_else(|_| panic!("network"));
            let hex = address
                .script_pubkey()
                .as_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            assert_eq!(generated.get(&hex), Some(&index));
        }
    }
}

#[test]
#[ignore = "requires the isolated wallet-disabled testnet4 Core fixture node"]
fn core_genesis_filter_scan_and_activity_schema_match() {
    let datadir = std::env::var_os("MONOFORM_TEST_CORE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("isolated directory"));
    let core = CoreTestnet4::new(&datadir).unwrap_or_else(|_| panic!("client"));
    assert!(matches!(
        core.discover(&fixture(), DiscoveryLimits::default()),
        Err(DiscoveryError::Incomplete(IncompleteReason::Syncing))
    ));
    let mut budget = Budget {
        core: &core,
        remaining: 32,
        started: Instant::now(),
    };
    let status = budget.status().unwrap_or_else(|_| panic!("identity"));
    assert_eq!(status.blocks, 0);
    let account = fixture();
    let descriptor = account
        .descriptors()
        .unwrap_or_else(|_| panic!("descriptor"))
        .receive;
    let result = budget
        .branch(
            &account,
            0,
            &descriptor,
            &status,
            &DiscoveryLimits::default(),
        )
        .unwrap_or_else(|_| panic!("genesis scan schema"));
    assert_eq!(result.examined, 20);
    assert_eq!(result.last_used, None);
    let activity: Activity = budget
        .call(
            "getdescriptoractivity",
            json!([[GENESIS],[{"desc":descriptor,"range":[0,19]}],false]),
        )
        .unwrap_or_else(|_| panic!("Core activity schema"));
    assert!(activity.activity.is_empty());
}
