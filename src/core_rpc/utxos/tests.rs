//! Synthetic public outpoints/amounts and the published seed fixture; no real funds.
use super::*;
use crate::core_rpc::tests::{calls, chain, envelope, fixture, mock};
use serde_json::Value;

pub(in crate::core_rpc) fn hash(n: u8) -> String {
    format!("{n:064x}")
}
pub(in crate::core_rpc) fn ok(mut value: Value) -> (u16, String) {
    // Test fixtures must preserve raw decimal spelling too, rather than round-trip
    // monetary numbers through serde_json::Value's floating-point representation.
    fn extract(value: &mut Value, numbers: &mut Vec<(String, String)>) {
        match value {
            Value::String(text) if text.starts_with("raw-btc:") => {
                let raw = text[8..].to_owned();
                let marker = format!("__raw_btc_{}__", numbers.len());
                *text = marker.clone();
                numbers.push((format!("\"{marker}\""), raw));
            }
            Value::Array(items) => {
                for item in items {
                    extract(item, numbers);
                }
            }
            Value::Object(items) => {
                for item in items.values_mut() {
                    extract(item, numbers);
                }
            }
            _ => {}
        }
    }
    let mut numbers = Vec::new();
    extract(&mut value, &mut numbers);
    let mut body = envelope(value);
    for (marker, raw) in numbers {
        body = body.replace(&marker, &raw);
    }
    (200, body)
}
pub(in crate::core_rpc) fn scope() -> UtxoScope {
    UtxoScope {
        receive_count: 20,
        change_count: 20,
    }
}
pub(in crate::core_rpc) fn recent(height: u32, tip: u8) -> Value {
    let mut info = chain();
    info["blocks"] = json!(height);
    info["headers"] = json!(height);
    info["bestblockhash"] = json!(hash(tip));
    info["initialblockdownload"] = json!(false);
    info["time"] = json!(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| panic!("clock"))
            .as_secs()
    );
    info
}
pub(in crate::core_rpc) fn start() -> Vec<(u16, String)> {
    vec![ok(recent(102, 1)), ok(json!(GENESIS)), ok(json!(hash(1)))]
}
pub(in crate::core_rpc) fn finish() -> Vec<(u16, String)> {
    vec![
        ok(recent(102, 1)),
        ok(json!(GENESIS)),
        ok(json!(hash(1))),
        ok(json!(hash(1))),
    ]
}
fn amount(raw: &str) -> Value {
    json!({"amount":format!("raw-btc:{raw}")})
}
pub(in crate::core_rpc) fn output(
    branch: u32,
    index: u32,
    tx: u8,
    height: u32,
    coinbase: bool,
) -> Value {
    let generated = scoped_scripts(&fixture(), scope()).unwrap_or_else(|_| panic!("scripts"));
    let hex = generated
        .iter()
        .find(|(_, location)| **location == (branch, index))
        .map(|(hex, _)| hex.clone())
        .unwrap_or_else(|| panic!("script"));
    json!({"txid":hash(tx),"vout":0,"scriptPubKey":hex,"amount":1,"coinbase":coinbase,
        "height":height,"blockhash":hash(if height==102 {1} else {height as u8}),"confirmations":102-height+1})
}
pub(in crate::core_rpc) fn snapshot(outputs: Vec<Value>, total: u64) -> Value {
    json!({"success":true,"txouts":outputs.len(),"height":102,"bestblock":hash(1),"unspents":outputs,"total_amount":total})
}

// Private integration-fixture helper. Production still requires testnet4 readiness.
pub(in crate::core_rpc) fn live_confirmed(
    core: &CoreTestnet4,
    account: &BitcoinTestnetAccount,
) -> UtxoObservation {
    let mut budget = Budget {
        core,
        remaining: 80,
        started: Instant::now(),
    };
    let scan = budget
        .utxo_scan(account, scope())
        .unwrap_or_else(|_| panic!("fixture UTXO snapshot"));
    let scripts = scoped_scripts(account, scope()).unwrap_or_else(|_| panic!("fixture scripts"));
    let (balance, utxos) = budget
        .validate_utxos(&scan, &scripts)
        .unwrap_or_else(|_| panic!("fixture UTXO consistency"));
    UtxoObservation {
        anchor_height: scan.height,
        anchor_hash: scan.bestblock,
        observed_at: SystemTime::now(),
        scope: scope(),
        balance,
        utxos,
    }
}

#[test]
fn exact_decimal_parser_never_rounds_or_accepts_fractional_satoshis() {
    #[derive(Deserialize)]
    struct Amount {
        #[serde(deserialize_with = "satoshis")]
        amount: u64,
    }
    for (raw, expected) in [
        ("0", 0),
        ("0.00000000", 0),
        ("0.00000001", 1),
        ("0.1", 10_000_000),
        ("0.10000001", 10_000_001),
        ("1.23456789", 123_456_789),
        ("20999999.99999999", MAX_MONEY - 1),
        ("21000000.00000000", MAX_MONEY),
    ] {
        let decoded: Amount = serde_json::from_str(&format!(r#"{{"amount":{raw}}}"#))
            .unwrap_or_else(|_| panic!("exact amount"));
        assert_eq!(decoded.amount, expected);
        // Independent established Bitcoin amount parser, not f64 arithmetic.
        let independent = bitcoin::Amount::from_str_in(raw, bitcoin::Denomination::Bitcoin)
            .unwrap_or_else(|_| panic!("independent amount parser"));
        assert_eq!(decoded.amount, independent.to_sat());
    }
    for raw in [
        "-0",
        "-1",
        "1e-8",
        "0.000000001",
        "0.000000010",
        "21000000.00000001",
        "9999999999999999999999",
        "null",
        "true",
        r#""0.00000001""#,
        "{}",
        "[]",
    ] {
        assert!(serde_json::from_str::<Amount>(&format!(r#"{{"amount":{raw}}}"#)).is_err());
    }
    for raw in ["", ".1", "1.", "01.0", "+1", "1..2", "NaN", "inf"] {
        assert_eq!(parse_satoshis(raw), None);
    }
}

#[test]
fn sums_receive_change_and_immature_coinbases_without_mutating_counters() {
    let mut account = fixture();
    account
        .next_receive_address()
        .unwrap_or_else(|_| panic!("public address"));
    let before = account.encode_public_state();
    let mut receive = output(0, 0, 3, 2, false);
    let mut change = output(1, 0, 4, 3, true);
    let mut immature = output(1, 1, 5, 4, true);
    // This has 101 confirmations and is NOT part of the conservative immature bucket.
    receive["amount"] = amount("0.00000001")["amount"].clone();
    change["amount"] = amount("0.10000001")["amount"].clone();
    immature["amount"] = amount("0.2")["amount"].clone();
    let mut scan = snapshot(vec![receive, change, immature], 0);
    scan["total_amount"] = amount("0.30000002")["amount"].clone();
    let mut replies = start();
    replies.push(ok(scan));
    replies.extend([ok(json!(hash(2))), ok(json!(hash(3))), ok(json!(hash(4)))]);
    replies.extend(finish());
    let (client, _directory, server) = mock(replies);
    let result = client
        .observe_utxos(&account, scope())
        .unwrap_or_else(|_| panic!("complete scoped balance"));
    assert_eq!(result.balance.total_sats, 30_000_002);
    assert_eq!(result.balance.immature_coinbase_sats, 30_000_001);
    assert_eq!(result.utxos.len(), 3);
    assert_eq!(result.utxos[0].amount_sats, 1);
    assert_eq!((result.utxos[1].branch, result.utxos[1].index), (1, 0));
    assert_eq!(result.anchor_hash, hash(1));
    assert_eq!(result.anchor_height, 102);
    assert_eq!(account.encode_public_state(), before);
    let requests = calls(server);
    assert_eq!(requests[3]["method"], "scantxoutset");
    assert_eq!(requests[3]["params"][0], "start");
    assert_eq!(requests[3]["params"][1][0]["range"], json!([0, 19]));
    assert_eq!(requests[3]["params"][1][1]["range"], json!([0, 19]));
    assert!(!requests.iter().any(|call|call["method"] == "scanblocks" || call["method"] == "getdescriptoractivity"));
}

#[test]
fn empty_snapshot_is_only_zero_for_its_scope_and_pruning_is_allowed() {
    let mut initial = recent(102, 1);
    initial["pruned"] = json!(true);
    let mut replies = vec![
        ok(initial),
        ok(json!(GENESIS)),
        ok(json!(hash(1))),
        ok(snapshot(vec![], 0)),
    ];
    replies.extend(finish());
    let (client, _directory, server) = mock(replies);
    let result = client
        .observe_utxos(&fixture(), scope())
        .unwrap_or_else(|_| panic!("pruned UTXO state"));
    assert_eq!(result.balance.total_sats, 0);
    assert!(result.utxos.is_empty());
    assert_eq!(result.scope.receive_count, 20);
    calls(server);
}

#[test]
fn coinbase_maturity_bucket_has_an_exact_100_101_confirmation_boundary() {
    let mut replies = start();
    replies.extend([
        ok(snapshot(
            vec![output(0, 0, 3, 2, true), output(1, 0, 4, 3, true)],
            2,
        )),
        ok(json!(hash(2))),
        ok(json!(hash(3))),
    ]);
    replies.extend(finish());
    let (client, _directory, server) = mock(replies);
    let result = client
        .observe_utxos(&fixture(), scope())
        .unwrap_or_else(|_| panic!("maturity boundary"));
    assert_eq!(result.utxos[0].confirmations, 101);
    assert_eq!(result.utxos[1].confirmations, 100);
    assert_eq!(result.balance.total_sats, 2 * COIN);
    assert_eq!(result.balance.immature_coinbase_sats, COIN);
    calls(server);
}

#[test]
fn internally_conflicting_creation_metadata_is_rejected() {
    let first = output(0, 0, 3, 2, false);
    let mut second = output(1, 0, 4, 3, false);
    second["blockhash"] = json!(hash(2));
    let mut replies = start();
    replies.extend([ok(snapshot(vec![first, second], 2)), ok(json!(hash(2)))]);
    let (client, _directory, server) = mock(replies);
    assert!(matches!(
        client.observe_utxos(&fixture(), scope()),
        Err(UtxoError::Backend(CoreError::InvalidResponse))
    ));
    calls(server);
    let mut old = output(0, 0, 3, 2, false);
    old["blockhash"] = json!(hash(1));
    let mut replies = start();
    replies.push(ok(snapshot(vec![old], 1)));
    let (client, _directory, server) = mock(replies);
    assert!(matches!(
        client.observe_utxos(&fixture(), scope()),
        Err(UtxoError::Backend(CoreError::InvalidResponse))
    ));
    calls(server);
}

#[test]
fn invalid_or_undersized_scope_never_contacts_core() {
    let (client, _directory, server) = mock(vec![]);
    let mut account = fixture();
    account.next_receive = 21;
    for requested in [
        scope(),
        UtxoScope {
            receive_count: 0,
            change_count: 20,
        },
        UtxoScope {
            receive_count: 21,
            change_count: 0,
        },
        UtxoScope {
            receive_count: 10_001,
            change_count: 20,
        },
        UtxoScope {
            receive_count: 21,
            change_count: 10_001,
        },
    ] {
        assert!(matches!(
            client.observe_utxos(&account, requested),
            Err(UtxoError::InvalidScope)
        ));
    }
    assert_eq!(account.next_receive, 21);
    assert!(calls(server).is_empty());
}

#[test]
fn syncing_stale_or_wrong_chain_prevents_descriptor_disclosure() {
    for (field, value, expected) in [
        (
            "initialblockdownload",
            json!(true),
            UtxoError::Incomplete(UtxoIncompleteReason::Syncing),
        ),
        (
            "time",
            json!(1714777860_u64),
            UtxoError::Incomplete(UtxoIncompleteReason::StaleTip),
        ),
    ] {
        let mut info = recent(102, 1);
        info[field] = value;
        let (client, _directory, server) = mock(vec![ok(info), ok(json!(GENESIS))]);
        assert!(matches!(client.observe_utxos(&fixture(),scope()),Err(error) if error == expected));
        assert_eq!(calls(server).len(), 2);
    }
    let mut info = recent(102, 1);
    info["chain"] = json!("main");
    let (client, _directory, server) = mock(vec![ok(info)]);
    assert!(matches!(
        client.observe_utxos(&fixture(), scope()),
        Err(UtxoError::Backend(CoreError::WrongChain))
    ));
    assert_eq!(calls(server).len(), 1);
}

#[test]
fn incomplete_busy_and_excessive_scans_do_not_return_partial_balance() {
    let mut unfinished = snapshot(vec![], 0);
    unfinished["success"] = json!(false);
    let oversized = snapshot(vec![output(0, 0, 3, 2, false); MAX_UTXOS + 1], 0);
    let compact = json!({"txid":"x","vout":0,"scriptPubKey":"x","amount":0,
        "coinbase":false,"height":1,"blockhash":"x","confirmations":102});
    for (reply,expected) in [
        (ok(unfinished),UtxoError::Incomplete(UtxoIncompleteReason::ScanUnavailable)),
        ((200,json!({"jsonrpc":"2.0","id":"monoform","error":{"code":-8,"message":"Scan already in progress"}}).to_string()),UtxoError::Incomplete(UtxoIncompleteReason::ScanUnavailable)),
        (ok(oversized),UtxoError::Backend(CoreError::InvalidResponse)),
        (ok(snapshot(vec![compact;MAX_UTXOS+1],0)),UtxoError::Incomplete(UtxoIncompleteReason::OutputLimit)),
    ] {
        let mut replies=start(); replies.push(reply);
        let (client,_directory,server)=mock(replies);
        assert!(matches!(client.observe_utxos(&fixture(),scope()),Err(error) if error == expected));
        assert_eq!(calls(server).len(),4);
    }
    // Request and deadline failures remain explicit even without contacting a node.
    let (client, _directory, server) = mock(vec![]);
    let error = UtxoError::from(CoreError::RequestLimit);
    assert_eq!(
        error,
        UtxoError::Incomplete(UtxoIncompleteReason::RequestLimit)
    );
    let mut budget = Budget {
        core: &client,
        remaining: 1,
        started: Instant::now() - DEADLINE,
    };
    assert!(matches!(
        budget.utxo_scan(&fixture(), scope()),
        Err(UtxoError::Incomplete(UtxoIncompleteReason::Deadline))
    ));
    calls(server);
}

#[test]
fn wrong_scripts_bad_metadata_and_duplicate_outpoints_are_rejected() {
    let valid = output(0, 0, 3, 2, false);
    for (field, value) in [
        ("scriptPubKey", json!("0014bad")),
        ("txid", json!("bad")),
        ("blockhash", json!("bad")),
        ("height", json!(0)),
        ("height", json!(103)),
        ("confirmations", json!(100)),
        ("coinbase", json!(null)),
        ("vout", json!(-1)),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        let mut replies = start();
        replies.push(ok(snapshot(vec![invalid], 1)));
        let (client, _directory, server) = mock(replies);
        assert!(matches!(
            client.observe_utxos(&fixture(), scope()),
            Err(UtxoError::Backend(CoreError::InvalidResponse))
        ));
        calls(server);
    }
    let mut replies = start();
    replies.extend([
        ok(snapshot(vec![valid.clone(), valid], 2)),
        ok(json!(hash(2))),
    ]);
    let (client, _directory, server) = mock(replies);
    assert!(matches!(
        client.observe_utxos(&fixture(), scope()),
        Err(UtxoError::Backend(CoreError::InvalidResponse))
    ));
    calls(server);
}

#[test]
fn amount_and_sum_failures_never_produce_a_balance() {
    let output = output(0, 0, 3, 2, false);
    for total in [0, 2, 21_000_001] {
        let mut replies = start();
        replies.push(ok(snapshot(vec![output.clone()], total)));
        if total <= 21_000_000 {
            replies.push(ok(json!(hash(2))));
        }
        let (client, _directory, server) = mock(replies);
        assert!(matches!(
            client.observe_utxos(&fixture(), scope()),
            Err(UtxoError::Backend(CoreError::InvalidResponse))
        ));
        calls(server);
    }
    for raw in ["0.000000001", "-1", "1e-8", r#""1.0""#, "21000000.00000001"] {
        let mut invalid = output.clone();
        invalid["amount"] = amount(raw)["amount"].clone();
        let mut replies = start();
        replies.push(ok(snapshot(vec![invalid], 0)));
        let (client, _directory, server) = mock(replies);
        assert!(matches!(
            client.observe_utxos(&fixture(), scope()),
            Err(UtxoError::Backend(CoreError::InvalidResponse))
        ));
        calls(server);
    }
    let mut largest = output;
    largest["amount"] = json!(21_000_000);
    let mut other = largest.clone();
    other["txid"] = json!(hash(4));
    let mut replies = start();
    replies.extend([
        ok(snapshot(vec![largest, other], 21_000_000)),
        ok(json!(hash(2))),
        ok(json!(hash(2))),
    ]);
    let (client, _directory, server) = mock(replies);
    assert!(matches!(
        client.observe_utxos(&fixture(), scope()),
        Err(UtxoError::Backend(CoreError::InvalidResponse))
    ));
    calls(server);
}

#[test]
fn scan_anchor_changes_and_creation_block_changes_discard_all_results() {
    let mut scan = snapshot(vec![], 0);
    scan["bestblock"] = json!(hash(3));
    let mut short = snapshot(vec![], 0);
    short["height"] = json!(101);
    let mut at_event = start();
    at_event.extend([
        ok(snapshot(vec![output(0, 0, 3, 2, false)], 1)),
        ok(json!(hash(3))),
    ]);
    let mut at_end = start();
    at_end.push(ok(snapshot(vec![], 0)));
    at_end.extend(finish());
    *at_end.last_mut().unwrap_or_else(|| panic!("last")) = ok(json!(hash(3)));
    let mut before = start();
    before[2] = ok(json!(hash(3)));
    for replies in [
        [start(), vec![ok(scan)]].concat(),
        [start(), vec![ok(short)]].concat(),
        at_event,
        at_end,
        before,
    ] {
        let account = fixture();
        let state = account.encode_public_state();
        let (client, _directory, server) = mock(replies);
        assert!(matches!(
            client.observe_utxos(&account, scope()),
            Err(UtxoError::Reorg)
        ));
        assert_eq!(account.encode_public_state(), state);
        calls(server);
    }
}

#[test]
fn normal_chain_growth_can_return_a_newer_but_checked_scan_anchor() {
    let mut scan = snapshot(vec![], 0);
    scan["height"] = json!(103);
    scan["bestblock"] = json!(hash(3));
    let mut replies = start();
    replies.push(ok(scan));
    replies.extend([
        ok(recent(104, 4)),
        ok(json!(GENESIS)),
        ok(json!(hash(1))),
        ok(json!(hash(3))),
    ]);
    let (client, _directory, server) = mock(replies);
    let result = client
        .observe_utxos(&fixture(), scope())
        .unwrap_or_else(|_| panic!("checked chain growth"));
    assert_eq!(result.anchor_height, 103);
    assert_eq!(result.anchor_hash, hash(3));
    calls(server);
}

#[test]
#[ignore = "requires isolated wallet-disabled testnet4 and regtest Core nodes"]
fn core_utxo_scan_schemas_match_and_nonempty_amounts_are_exact() {
    let datadir = std::env::var_os("MONOFORM_TEST_CORE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("isolated directory"));
    let core = CoreTestnet4::new(&datadir).unwrap_or_else(|_| panic!("core"));
    let account = fixture();
    if std::env::var_os("MONOFORM_REGTEST_FIXTURE").is_none() {
        assert!(matches!(
            core.observe_utxos(&account, scope()),
            Err(UtxoError::Incomplete(UtxoIncompleteReason::Syncing))
        ));
        let mut budget = Budget {
            core: &core,
            remaining: 80,
            started: Instant::now(),
        };
        let scan = budget
            .utxo_scan(&account, scope())
            .unwrap_or_else(|_| panic!("empty Core UTXO schema"));
        assert_eq!(scan.height, 0);
        assert_eq!(scan.bestblock, GENESIS);
        assert!(scan.unspents.is_empty());
        let scripts = scoped_scripts(&account, scope()).unwrap_or_else(|_| panic!("scripts"));
        let (balance, utxos) = budget
            .validate_utxos(&scan, &scripts)
            .unwrap_or_else(|_| panic!("exact zero"));
        assert_eq!(balance.total_sats, 0);
        assert!(utxos.is_empty());
    } else {
        // Test-only access to regtest's cookie. Public construction still fixes testnet4.
        let mut core = core;
        core.cookie = datadir.join("regtest/.cookie");
        assert!(matches!(
            core.observe_utxos(&account, scope()),
            Err(UtxoError::Backend(CoreError::WrongChain))
        ));
        let mut budget = Budget {
            core: &core,
            remaining: 80,
            started: Instant::now(),
        };
        let scan = budget
            .utxo_scan(&account, scope())
            .unwrap_or_else(|_| panic!("nonempty Core snapshot"));
        assert_eq!(scan.height, 102);
        let scripts = scoped_scripts(&account, scope()).unwrap_or_else(|_| panic!("scripts"));
        let (balance, utxos) = budget
            .validate_utxos(&scan, &scripts)
            .unwrap_or_else(|_| panic!("real Core UTXO validation"));
        assert_eq!(utxos.len(), 2);
        assert_eq!(balance.total_sats, 100 * COIN);
        assert_eq!(balance.immature_coinbase_sats, 50 * COIN);
        assert!(
            utxos.iter().any(|output| output.branch == 0
                && output.index == 0
                && output.confirmations == 102)
        );
        assert!(
            utxos
                .iter()
                .any(|output| output.branch == 1 && output.index == 0 && output.confirmations == 1)
        );
    }
}
