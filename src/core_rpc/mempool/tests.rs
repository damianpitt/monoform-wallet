//! Only synthetic metadata, published derivation fixtures and isolated Core test coins.
use super::*;
use crate::core_rpc::tests::{calls, fixture, mock};
use crate::core_rpc::utxos::tests::{finish, hash, ok, output, recent, scope, snapshot, start};
use serde_json::Value;

fn pool_replies(ids: &[u8], sequence: u64) -> Vec<(u16, String)> {
    vec![
        ok(json!({"loaded":true,"size":ids.len()})),
        ok(
            json!({"txids":ids.iter().map(|id|hash(*id)).collect::<Vec<_>>(),"mempool_sequence":sequence}),
        ),
    ]
}
fn script(branch: u32, index: u32) -> String {
    utxos::scoped_scripts(&fixture(), scope())
        .unwrap_or_else(|_| panic!("public scripts"))
        .into_iter()
        .find(|(_, location)| *location == (branch, index))
        .map(|(script, _)| script)
        .unwrap_or_else(|| panic!("script"))
}
fn receipt(tx: u8, vout: u32, branch: u32, index: u32, amount: &str) -> Value {
    json!({"type":"receive","txid":hash(tx),"vout":vout,"output_spk":{"hex":script(branch,index)},"amount":format!("raw-btc:{amount}")})
}
fn spend(tx: u8, vin: u32, prev: u8, vout: u32, branch: u32, index: u32, amount: &str) -> Value {
    json!({"type":"spend","spend_txid":hash(tx),"spend_vin":vin,"prevout_txid":hash(prev),"prevout_vout":vout,
        "prevout_spk":{"hex":script(branch,index)},"amount":format!("raw-btc:{amount}")})
}
fn parse(events: Vec<Value>) -> Result<Activity, CoreError> {
    let (_, body) = ok(json!({"activity":events}));
    let parsed: RpcResponse<Activity> =
        serde_json::from_str(&body).map_err(|_| CoreError::InvalidResponse)?;
    parsed.result.ok_or(CoreError::InvalidResponse)
}
fn events() -> Vec<Value> {
    // Child first: Core event order is not a topological-order contract.
    vec![
        spend(5, 0, 4, 0, 1, 0, "0.99999000"),
        receipt(5, 0, 0, 1, "0.99998000"),
        spend(4, 0, 3, 0, 0, 0, "1.00000000"),
        receipt(4, 0, 1, 0, "0.99999000"),
    ]
}
fn base() -> UtxoObservation {
    UtxoObservation {
        anchor_height: 102,
        anchor_hash: hash(1),
        observed_at: SystemTime::now(),
        scope: scope(),
        balance: ConfirmedBalance {
            total_sats: 100_000_000,
            immature_coinbase_sats: 0,
        },
        utxos: vec![ObservedUtxo {
            txid: hash(3),
            vout: 0,
            branch: 0,
            index: 0,
            amount_sats: 100_000_000,
            coinbase: false,
            height: 2,
            block_hash: hash(2),
            confirmations: 101,
        }],
    }
}
fn projection(
    events: Vec<Value>,
) -> Result<(Vec<PendingReceipt>, Vec<PendingSpend>, PendingSummary), MempoolError> {
    let scripts = utxos::scoped_scripts(&fixture(), scope()).unwrap_or_else(|_| panic!("scripts"));
    reconcile(
        &base(),
        &Pool {
            txids: [hash(4), hash(5)].into_iter().collect(),
            mempool_sequence: 7,
        },
        parse(events)?,
        &scripts,
    )
}
fn stable(ids: &[u8], events: Vec<Value>, has_coin: bool) -> Vec<(u16, String)> {
    let mut replies = vec![ok(recent(102, 1)), ok(json!(GENESIS))];
    replies.extend(pool_replies(ids, 7));
    replies.extend(start());
    replies.push(ok(snapshot(
        if has_coin {
            vec![output(0, 0, 3, 2, false)]
        } else {
            vec![]
        },
        if has_coin { 1 } else { 0 },
    )));
    if has_coin {
        replies.push(ok(json!(hash(2))));
    }
    replies.extend(finish());
    replies.push(ok(json!({"activity":events})));
    replies.extend([ok(recent(102, 1)), ok(json!(GENESIS)), ok(json!(hash(1)))]);
    replies.extend(pool_replies(ids, 7));
    replies
}

#[test]
fn public_api_uses_fresh_confirmed_state_one_budget_and_no_writes() {
    let account = fixture();
    let before = account.encode_public_state();
    let (client, _directory, server) = mock(stable(&[4, 5], events(), true));
    let result = client
        .reconcile_mempool(&account, scope())
        .unwrap_or_else(|_| panic!("stable reconciliation"));
    assert_eq!(result.confirmed.balance.total_sats, 100_000_000);
    assert_eq!(result.summary.confirmed_pending_spent_sats, 100_000_000);
    assert_eq!(result.summary.pending_unspent_receipts_sats, 99_998_000);
    assert_eq!(result.summary.projected_total_sats, 99_998_000);
    assert_eq!(result.mempool_sequence, 7);
    assert_eq!(result.spends.len(), 2);
    assert_eq!(
        result
            .receipts
            .iter()
            .filter(|r| r.spent_in_mempool)
            .count(),
        1
    );
    assert_eq!(account.encode_public_state(), before);
    let requests = calls(server);
    assert_eq!(requests.len(), 19);
    let activity = requests
        .iter()
        .find(|r| r["method"] == "getdescriptoractivity")
        .unwrap_or_else(|| panic!("activity call"));
    assert_eq!(activity["params"][0], json!([]));
    assert_eq!(activity["params"][2], true);
    assert_eq!(activity["params"][1][0]["range"], json!([0, 19]));
    for call in requests {
        assert!(matches!(
            call["method"].as_str(),
            Some(
                "getblockchaininfo"
                    | "getblockhash"
                    | "getmempoolinfo"
                    | "getrawmempool"
                    | "scantxoutset"
                    | "getdescriptoractivity"
            )
        ));
        if call["method"] == "getrawmempool" {
            assert_eq!(call["params"], json!([false, true]));
        }
    }
}

#[test]
fn terminal_receipts_are_order_independent_and_self_transfers_are_not_double_counted() {
    let original = events();
    for shift in 0..original.len() {
        let mut rotated = original.clone();
        rotated.rotate_left(shift);
        let (receipts, spends, summary) =
            projection(rotated).unwrap_or_else(|_| panic!("order-independent overlay"));
        assert_eq!(summary.projected_total_sats, 99_998_000);
        assert_eq!(spends.iter().filter(|s| s.spends_confirmed).count(), 1);
        assert_eq!(receipts.iter().filter(|r| !r.spent_in_mempool).count(), 1);
    }
}

#[test]
fn incoming_only_zero_and_dropped_transactions_are_fresh_not_cached_balances() {
    let incoming = vec![receipt(4, 0, 0, 0, "0.00000001")];
    for (ids, events, expected) in [(vec![4], incoming, 1), (vec![], vec![], 0)] {
        let (client, _directory, server) = mock(stable(&ids, events, false));
        let result = client
            .reconcile_mempool(&fixture(), scope())
            .unwrap_or_else(|_| panic!("fresh receipts"));
        assert_eq!(result.summary.projected_total_sats, expected);
        assert_eq!(result.confirmed.balance.total_sats, 0);
        calls(server);
    }
}

#[test]
fn replacement_eviction_or_sequence_changes_return_no_partial_overlay() {
    for (ids, sequence) in [(vec![5], 8), (vec![], 8), (vec![4], 8), (vec![5], 7)] {
        let mut replies = stable(&[4], vec![], false);
        let tail = replies.len() - 2;
        replies.truncate(tail);
        replies.extend(pool_replies(&ids, sequence));
        let (client, _directory, server) = mock(replies);
        assert!(matches!(
            client.reconcile_mempool(&fixture(), scope()),
            Err(MempoolError::SnapshotChanged)
        ));
        calls(server);
    }
}

#[test]
fn chain_growth_and_reorgs_invalidate_a_pending_snapshot() {
    for reorg in [false, true] {
        let mut replies = stable(&[], vec![], false);
        replies.truncate(16);
        if reorg {
            replies[15] = ok(json!(hash(2)));
        } else {
            replies[13] = ok(recent(103, 2));
        }
        let (client, _directory, server) = mock(replies);
        let result = client.reconcile_mempool(&fixture(), scope());
        assert!(
            matches!(result, Err(MempoolError::Reorg)) && reorg
                || matches!(result, Err(MempoolError::SnapshotChanged)) && !reorg
        );
        calls(server);
    }
}

#[test]
fn unloaded_large_racing_or_malformed_pools_never_disclose_descriptors() {
    for (pool_replies, expected) in [
        (
            vec![ok(json!({"loaded":false,"size":0}))],
            MempoolError::Incomplete(MempoolIncompleteReason::NotLoaded),
        ),
        (
            vec![ok(json!({"loaded":true,"size":129}))],
            MempoolError::Incomplete(MempoolIncompleteReason::PoolLimit),
        ),
        (
            vec![
                ok(json!({"loaded":true,"size":2})),
                ok(json!({"txids":[hash(4)],"mempool_sequence":7})),
            ],
            MempoolError::SnapshotChanged,
        ),
        (
            vec![
                ok(json!({"loaded":true,"size":1})),
                ok(json!({"txids":[hash(4),hash(4)],"mempool_sequence":7})),
            ],
            invalid(),
        ),
        (
            vec![
                ok(json!({"loaded":true,"size":1})),
                ok(json!({"txids":["bad"],"mempool_sequence":7})),
            ],
            invalid(),
        ),
    ] {
        let mut replies = vec![ok(recent(102, 1)), ok(json!(GENESIS))];
        replies.extend(pool_replies);
        let count = replies.len();
        let (client, _directory, server) = mock(replies);
        assert!(
            matches!(client.reconcile_mempool(&fixture(),scope()),Err(error) if error==expected)
        );
        assert_eq!(calls(server).len(), count);
    }
}

#[test]
fn invalid_scope_and_wrong_network_fail_before_account_payloads() {
    let (client, _directory, server) = mock(vec![]);
    assert!(matches!(
        client.reconcile_mempool(
            &fixture(),
            UtxoScope {
                receive_count: 0,
                change_count: 20
            }
        ),
        Err(MempoolError::Confirmed(UtxoError::InvalidScope))
    ));
    assert!(calls(server).is_empty());
    let mut info = recent(102, 1);
    info["chain"] = json!("main");
    let (client, _directory, server) = mock(vec![ok(info)]);
    assert!(matches!(
        client.reconcile_mempool(&fixture(), scope()),
        Err(MempoolError::Backend(CoreError::WrongChain))
    ));
    assert_eq!(calls(server).len(), 1);
}

#[test]
fn duplicate_outpoints_inputs_or_missing_dependencies_are_rejected() {
    let original = events();
    for item in &original {
        let mut repeated = original.clone();
        repeated.push(item.clone());
        assert!(projection(repeated).is_err());
    }
    let mut double_spend = original.clone();
    double_spend.push(spend(5, 1, 3, 0, 0, 0, "1.00000000"));
    assert!(projection(double_spend).is_err());
    for bad in [
        spend(4, 0, 9, 0, 0, 0, "1.00000000"),
        spend(4, 0, 3, 0, 0, 0, "0.99999999"),
        spend(4, 0, 3, 0, 1, 0, "1.00000000"),
    ] {
        assert!(projection(vec![bad]).is_err());
    }
}

#[test]
fn cycles_and_impossible_pending_metadata_fail_closed() {
    let cycle = vec![
        receipt(4, 0, 0, 0, "1"),
        receipt(5, 0, 0, 1, "1"),
        spend(4, 0, 5, 0, 0, 1, "1"),
        spend(5, 0, 4, 0, 0, 0, "1"),
    ];
    assert!(projection(cycle).is_err());
    assert!(projection(vec![receipt(4, 0, 0, 0, "1"), spend(4, 0, 4, 0, 0, 0, "1")]).is_err());
    let mut immature = base();
    immature.utxos[0].coinbase = true;
    immature.utxos[0].confirmations = 99;
    let scripts = utxos::scoped_scripts(&fixture(), scope()).unwrap_or_else(|_| panic!("scripts"));
    let pool = Pool {
        txids: [hash(4)].into_iter().collect(),
        mempool_sequence: 7,
    };
    assert!(
        reconcile(
            &immature,
            &pool,
            parse(vec![spend(4, 0, 3, 0, 0, 0, "1")]).unwrap_or_else(|_| panic!("event")),
            &scripts
        )
        .is_err()
    );
    for (field, value) in [
        ("height", json!(102)),
        ("blockhash", json!(hash(1))),
        ("txid", json!(hash(8))),
        ("vout", json!(-1)),
        ("output_spk", json!({"hex":"0014bad"})),
        ("spend_txid", json!(hash(4))),
    ] {
        let mut event = receipt(4, 0, 0, 0, "1");
        event[field] = value;
        assert!(projection(vec![event]).is_err());
    }
    let mut mixed = spend(4, 0, 3, 0, 0, 0, "1");
    mixed["vout"] = json!(0);
    assert!(projection(vec![mixed]).is_err());
}

#[test]
fn exact_amounts_supply_limits_and_shared_deadline_are_preserved() {
    for raw in ["0.000000001", "1e-8", "-1", "21000000.00000001", r#""1.0""#] {
        assert!(projection(vec![receipt(4, 0, 0, 0, raw)]).is_err());
    }
    assert!(
        projection(vec![
            receipt(4, 0, 0, 0, "21000000"),
            receipt(5, 0, 0, 1, "1")
        ])
        .is_err()
    );
    let (client, _directory, server) = mock(vec![]);
    let mut budget = Budget {
        core: &client,
        remaining: 112,
        started: Instant::now() - DEADLINE,
    };
    assert!(matches!(
        budget.pool(),
        Err(MempoolError::Incomplete(MempoolIncompleteReason::Deadline))
    ));
    assert_eq!(budget.remaining, 112);
    assert!(calls(server).is_empty());
}

#[test]
fn excessive_pending_activity_is_not_a_partial_success() {
    let compact = json!({"type":"receive","txid":"x","vout":0,"output_spk":{"hex":"x"},"amount":0});
    let (client, _directory, server) =
        mock(vec![ok(json!({"activity":vec![compact;MAX_EVENTS+1]}))]);
    let mut budget = Budget {
        core: &client,
        remaining: 112,
        started: Instant::now(),
    };
    assert!(matches!(
        budget.pending_activity(&fixture(), scope()),
        Err(MempoolError::Incomplete(
            MempoolIncompleteReason::ActivityLimit
        ))
    ));
    calls(server);
}

// Test-only CLI can create/sign/submit worthless regtest fixture transactions.
// No such method is added to Monoform's production RPC allowlist.
fn fixture_rpc(datadir: &Path, method: &str, params: &[String]) -> Value {
    let bin = std::env::var_os("MONOFORM_TEST_CORE_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("verified Core tools"));
    assert!(bin.is_absolute() && datadir.is_absolute());
    let output = std::process::Command::new(bin.join("bitcoin-cli"))
        .args([
            "-regtest",
            "-conf=/dev/null",
            "-rpcconnect=127.0.0.1",
            "-rpcport=48332",
            "-rpcclienttimeout=10",
        ])
        .arg(format!("-datadir={}", datadir.display()))
        .arg(method)
        .args(params)
        .output()
        .unwrap_or_else(|_| panic!("fixture RPC transport"));
    assert!(
        output.status.success(),
        "fixture RPC failed (payload suppressed)"
    );
    // bitcoin-cli prints string results as plain text, unlike its JSON objects.
    if matches!(method, "createrawtransaction" | "sendrawtransaction") {
        return Value::String(
            String::from_utf8(output.stdout)
                .unwrap_or_else(|_| panic!("fixture RPC text"))
                .trim()
                .to_owned(),
        );
    }
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| panic!("fixture RPC JSON"))
}
fn fixture_key(branch: u32) -> String {
    // Published Trezor vector, dedicated to public tests; never protect real funds.
    let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
    let seed = crate::OfflineSeed::from_phrase(words.to_owned())
        .unwrap_or_else(|_| panic!("public vector"));
    let mut key = bip32::XPrv::new(seed.0.as_slice()).unwrap_or_else(|_| panic!("fixture root"));
    for (index, hardened) in [
        (84, true),
        (1, true),
        (0, true),
        (branch, false),
        (0, false),
    ] {
        let child =
            bip32::ChildNumber::new(index, hardened).unwrap_or_else(|_| panic!("fixture index"));
        key = key
            .derive_child(child)
            .unwrap_or_else(|_| panic!("fixture child"));
    }
    let secret = bitcoin::secp256k1::SecretKey::from_slice(&key.to_bytes())
        .unwrap_or_else(|_| panic!("fixture scalar"));
    bitcoin::PrivateKey::new(secret, bitcoin::NetworkKind::Test).to_wif()
}
fn fixture_transaction(
    datadir: &Path,
    prevout: &str,
    vout: u32,
    address: &str,
    value: &str,
    branch: u32,
) -> (String, String) {
    let inputs = json!([{"txid":prevout,"vout":vout,"sequence":4294967293_u32}]).to_string();
    // Keep exact monetary text even in test funding transactions.
    let outputs = format!(r#"{{"{address}":{value}}}"#);
    let unsigned = fixture_rpc(datadir, "createrawtransaction", &[inputs, outputs]);
    let signed = fixture_rpc(
        datadir,
        "signrawtransactionwithkey",
        &[
            unsigned
                .as_str()
                .unwrap_or_else(|| panic!("unsigned fixture"))
                .to_owned(),
            json!([fixture_key(branch)]).to_string(),
        ],
    );
    assert_eq!(signed["complete"], true);
    let hex = signed["hex"]
        .as_str()
        .unwrap_or_else(|| panic!("signed fixture"))
        .to_owned();
    let txid = fixture_rpc(datadir, "sendrawtransaction", std::slice::from_ref(&hex));
    (
        txid.as_str()
            .unwrap_or_else(|| panic!("fixture txid"))
            .to_owned(),
        hex,
    )
}

#[test]
#[ignore = "requires isolated wallet-disabled, peer-disabled testnet4/regtest Core fixtures"]
fn core_pending_chains_replacement_and_confirmation_match() {
    use crate::core_rpc::utxos::tests::live_confirmed;
    let datadir = std::env::var_os("MONOFORM_TEST_CORE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("isolated directory"));
    let mut core = CoreTestnet4::new(&datadir).unwrap_or_else(|_| panic!("fixture client"));
    let account = fixture();
    if std::env::var_os("MONOFORM_REGTEST_FIXTURE").is_none() {
        assert!(matches!(
            core.reconcile_mempool(&account, scope()),
            Err(MempoolError::Confirmed(UtxoError::Incomplete(
                UtxoIncompleteReason::Syncing
            )))
        ));
        let mut budget = Budget {
            core: &core,
            remaining: 112,
            started: Instant::now(),
        };
        let pool = budget
            .pool()
            .unwrap_or_else(|_| panic!("empty Core pool schema"));
        assert!(pool.txids.is_empty());
        let activity = budget
            .pending_activity(&account, scope())
            .unwrap_or_else(|_| panic!("empty Core activity schema"));
        assert!(activity.activity.is_empty());
        return;
    }
    core.cookie = datadir.join("regtest/.cookie");
    assert!(matches!(
        core.reconcile_mempool(&account, scope()),
        Err(MempoolError::Backend(CoreError::WrongChain))
    ));
    assert_eq!(
        fixture_rpc(&datadir, "getblockchaininfo", &[])["chain"],
        "regtest"
    );
    let confirmed = live_confirmed(&core, &account);
    assert_eq!(confirmed.balance.total_sats, 10_000_000_000);
    let coin = confirmed
        .utxos
        .iter()
        .find(|coin| coin.branch == 0 && coin.index == 0)
        .unwrap_or_else(|| panic!("mature fixture coin"));
    let descriptors = account
        .descriptors()
        .unwrap_or_else(|_| panic!("descriptors"));
    let change: [String; 1] = core
        .rpc("deriveaddresses", json!([descriptors.change, [0, 0]]))
        .unwrap_or_else(|_| panic!("change fixture"));
    let receive: [String; 1] = core
        .rpc("deriveaddresses", json!([descriptors.receive, [1, 1]]))
        .unwrap_or_else(|_| panic!("receipt fixture"));
    let (parent, _) = fixture_transaction(
        &datadir,
        &coin.txid,
        coin.vout,
        &change[0],
        "49.99999000",
        0,
    );
    fixture_transaction(&datadir, &parent, 0, &receive[0], "49.99998000", 1);
    let scripts = utxos::scoped_scripts(&account, scope()).unwrap_or_else(|_| panic!("scripts"));
    let mut budget = Budget {
        core: &core,
        remaining: 112,
        started: Instant::now(),
    };
    let pool_before = budget
        .pool()
        .unwrap_or_else(|_| panic!("Core parent/child pool"));
    assert_eq!(pool_before.txids.len(), 2);
    let activity = budget
        .pending_activity(&account, scope())
        .unwrap_or_else(|_| panic!("Core parent/child activity"));
    let (receipts, spends, summary) = reconcile(&confirmed, &pool_before, activity, &scripts)
        .unwrap_or_else(|_| panic!("Core descendant projection"));
    assert_eq!(summary.confirmed_pending_spent_sats, 5_000_000_000);
    assert_eq!(summary.pending_unspent_receipts_sats, 4_999_998_000);
    assert_eq!(summary.projected_total_sats, 9_999_998_000);
    assert_eq!(spends.len(), 2);
    assert_eq!(receipts.iter().filter(|r| r.spent_in_mempool).count(), 1);
    assert!(budget.pool().unwrap_or_else(|_| panic!("stable pool")) == pool_before);
    // Higher-fee replacement evicts BOTH the parent and its pending child.
    fixture_transaction(
        &datadir,
        &coin.txid,
        coin.vout,
        &change[0],
        "49.99990000",
        0,
    );
    let replaced = budget.pool().unwrap_or_else(|_| panic!("replacement pool"));
    assert!(replaced != pool_before);
    assert_eq!(replaced.txids.len(), 1);
    let activity = budget
        .pending_activity(&account, scope())
        .unwrap_or_else(|_| panic!("replacement activity"));
    let (_, _, summary) = reconcile(&confirmed, &replaced, activity, &scripts)
        .unwrap_or_else(|_| panic!("replacement projection"));
    assert_eq!(summary.projected_total_sats, 9_999_990_000);
    let outside: [String; 1] = core
        .rpc("deriveaddresses", json!([descriptors.receive, [99, 99]]))
        .unwrap_or_else(|_| panic!("outside fixture"));
    fixture_rpc(
        &datadir,
        "generatetoaddress",
        &["1".to_owned(), outside[0].clone()],
    );
    let confirmed = live_confirmed(&core, &account);
    let pool = budget.pool().unwrap_or_else(|_| panic!("confirmed pool"));
    assert!(pool.txids.is_empty());
    let activity = budget
        .pending_activity(&account, scope())
        .unwrap_or_else(|_| panic!("confirmed activity"));
    let (_, _, summary) = reconcile(&confirmed, &pool, activity, &scripts)
        .unwrap_or_else(|_| panic!("post-confirmation projection"));
    assert_eq!(confirmed.balance.total_sats, 9_999_990_000);
    assert_eq!(summary.projected_total_sats, confirmed.balance.total_sats);
    assert_eq!(summary.pending_unspent_receipts_sats, 0);
}
