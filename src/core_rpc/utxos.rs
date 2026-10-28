//! Confirmed UTXOs for explicit public address prefixes, not history or spendability.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

const COIN: u64 = 100_000_000;
pub(super) const MAX_MONEY: u64 = 21_000_000 * COIN;
const MAX_UTXOS: usize = 64;

/// Counts (not final indices). Caller chooses ranges; zero UTXOs never prove discovery.
#[derive(Clone, Copy)]
pub struct UtxoScope {
    pub receive_count: u32,
    pub change_count: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum UtxoIncompleteReason {
    Syncing,
    StaleTip,
    ScanUnavailable,
    OutputLimit,
    RequestLimit,
    Deadline,
}

#[derive(Debug, PartialEq, Eq)]
pub enum UtxoError {
    InvalidScope,
    Incomplete(UtxoIncompleteReason),
    Reorg,
    Backend(CoreError),
}

impl From<CoreError> for UtxoError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::Rpc => Self::Incomplete(UtxoIncompleteReason::ScanUnavailable),
            CoreError::ChainChanged => Self::Reorg,
            CoreError::RequestLimit => Self::Incomplete(UtxoIncompleteReason::RequestLimit),
            CoreError::Deadline => Self::Incomplete(UtxoIncompleteReason::Deadline),
            _ => Self::Backend(error),
        }
    }
}

pub struct ConfirmedBalance {
    /// Includes coinbase outputs, and outputs spent only in the mempool.
    pub total_sats: u64,
    /// Conservative wallet policy: coinbases with at most 100 confirmations.
    pub immature_coinbase_sats: u64,
}

pub struct ObservedUtxo {
    pub txid: String,
    pub vout: u32,
    pub branch: u32,
    pub index: u32,
    pub amount_sats: u64,
    pub coinbase: bool,
    pub height: u32,
    pub block_hash: String,
    pub confirmations: u32,
}

/// No Debug: outpoints and amounts are wallet-private data, not logging payloads.
pub struct UtxoObservation {
    pub anchor_height: u32,
    pub anchor_hash: String,
    pub observed_at: SystemTime,
    pub scope: UtxoScope,
    pub balance: ConfirmedBalance,
    pub utxos: Vec<ObservedUtxo>,
}

impl CoreTestnet4 {
    /// Read-only, explicit account disclosure. No address issuance or saved-state writes.
    pub fn observe_utxos(
        &self,
        account: &BitcoinTestnetAccount,
        scope: UtxoScope,
    ) -> Result<UtxoObservation, UtxoError> {
        let mut budget = Budget {
            core: self,
            remaining: 80,
            started: Instant::now(),
        };
        budget.observe_utxos(account, scope)
    }
}

impl Budget<'_> {
    pub(super) fn observe_utxos(
        &mut self,
        account: &BitcoinTestnetAccount,
        scope: UtxoScope,
    ) -> Result<UtxoObservation, UtxoError> {
        validate_scope(account, scope)?;
        let before = self.status()?;
        ready(&before)?;
        if self.call::<String>("getblockhash", json!([before.blocks]))? != before.tip_hash {
            return Err(UtxoError::Reorg);
        }
        let scripts = scoped_scripts(account, scope)?;
        let scan = self.utxo_scan(account, scope)?;
        if scan.height < before.blocks
            || (scan.height == before.blocks && scan.bestblock != before.tip_hash)
        {
            return Err(UtxoError::Reorg);
        }
        // Allow normal tip growth while scanning, but require BOTH observed anchors
        // to remain active. Do not mix two different UTXO snapshots or rescan silently.
        let (balance, utxos) = self.validate_utxos(&scan, &scripts)?;
        let after = self.status()?;
        if after.blocks < scan.height
            || self.call::<String>("getblockhash", json!([before.blocks]))? != before.tip_hash
            || self.call::<String>("getblockhash", json!([scan.height]))? != scan.bestblock
        {
            return Err(UtxoError::Reorg);
        }
        ready(&after)?;
        Ok(UtxoObservation {
            anchor_height: scan.height,
            anchor_hash: scan.bestblock,
            observed_at: SystemTime::now(),
            scope,
            balance,
            utxos,
        })
    }
}

pub(super) fn validate_scope(
    account: &BitcoinTestnetAccount,
    scope: UtxoScope,
) -> Result<(), UtxoError> {
    if !(1..=10_000).contains(&scope.receive_count)
        || !(1..=10_000).contains(&scope.change_count)
        || scope.receive_count < account.next_receive
        || scope.change_count < account.next_change
    {
        return Err(UtxoError::InvalidScope);
    }
    Ok(())
}

pub(super) fn ready(status: &NodeStatus) -> Result<(), UtxoError> {
    // Pruning removes history, not the validated UTXO set; this API is not discovery.
    match status.state {
        NodeState::Syncing => Err(UtxoError::Incomplete(UtxoIncompleteReason::Syncing)),
        NodeState::Stale => Err(UtxoError::Incomplete(UtxoIncompleteReason::StaleTip)),
        NodeState::Recent => Ok(()),
    }
}

pub(super) fn scoped_scripts(
    account: &BitcoinTestnetAccount,
    scope: UtxoScope,
) -> Result<BTreeMap<String, (u32, u32)>, UtxoError> {
    let mut scripts = BTreeMap::new();
    for (branch, count) in [(0, scope.receive_count), (1, scope.change_count)] {
        for (hex, index) in discovery::scripts(account, branch, 0, count - 1)
            .map_err(|_| UtxoError::Backend(CoreError::Derivation))?
        {
            if scripts.insert(hex, (branch, index)).is_some() {
                return Err(UtxoError::Backend(CoreError::Derivation));
            }
        }
    }
    Ok(scripts)
}

impl Budget<'_> {
    fn utxo_scan(
        &mut self,
        account: &BitcoinTestnetAccount,
        scope: UtxoScope,
    ) -> Result<UtxoScan, UtxoError> {
        let descriptors = account
            .descriptors()
            .map_err(|_| UtxoError::Backend(CoreError::Derivation))?;
        let scan: UtxoScan = self.call(
            "scantxoutset",
            json!(["start",[
                {"desc":descriptors.receive,"range":[0,scope.receive_count-1]},
                {"desc":descriptors.change,"range":[0,scope.change_count-1]}
            ]]),
        )?;
        if !scan.success {
            return Err(UtxoError::Incomplete(UtxoIncompleteReason::ScanUnavailable));
        }
        if scan.unspents.len() > MAX_UTXOS {
            return Err(UtxoError::Incomplete(UtxoIncompleteReason::OutputLimit));
        }
        if !valid_hash(&scan.bestblock) || scan.txouts < scan.unspents.len() as u64 {
            return Err(UtxoError::Backend(CoreError::InvalidResponse));
        }
        Ok(scan)
    }

    fn validate_utxos(
        &mut self,
        scan: &UtxoScan,
        scripts: &BTreeMap<String, (u32, u32)>,
    ) -> Result<(ConfirmedBalance, Vec<ObservedUtxo>), UtxoError> {
        let mut seen = BTreeSet::new();
        let mut creation_heights = BTreeMap::new();
        let mut balance = ConfirmedBalance {
            total_sats: 0,
            immature_coinbase_sats: 0,
        };
        let mut utxos = Vec::new();
        for output in &scan.unspents {
            let &(branch, index) = scripts
                .get(&output.script_pub_key)
                .ok_or(CoreError::InvalidResponse)?;
            if !valid_hash(&output.txid)
                || !valid_hash(&output.blockhash)
                || !seen.insert((&output.txid, output.vout))
                || output.height == 0
                || output.height > scan.height
                || output.confirmations != scan.height - output.height + 1
                || ((output.height == scan.height) != (output.blockhash == scan.bestblock))
            {
                return Err(UtxoError::Backend(CoreError::InvalidResponse));
            }
            // Multiple outputs may share a block, but one block hash has one height.
            if creation_heights
                .insert(&output.blockhash, output.height)
                .is_some_and(|height| height != output.height)
            {
                return Err(UtxoError::Backend(CoreError::InvalidResponse));
            }
            if self.call::<String>("getblockhash", json!([output.height]))? != output.blockhash {
                return Err(UtxoError::Reorg);
            }
            balance.total_sats = balance
                .total_sats
                .checked_add(output.amount)
                .filter(|total| *total <= MAX_MONEY)
                .ok_or(CoreError::InvalidResponse)?;
            if output.coinbase && output.confirmations <= 100 {
                // A nonnegative subset of the checked total cannot overflow.
                balance.immature_coinbase_sats += output.amount;
            }
            utxos.push(ObservedUtxo {
                txid: output.txid.clone(),
                vout: output.vout,
                branch,
                index,
                amount_sats: output.amount,
                coinbase: output.coinbase,
                height: output.height,
                block_hash: output.blockhash.clone(),
                confirmations: output.confirmations,
            });
        }
        // Independently sum exact per-output values; never trust Core's total alone.
        if balance.total_sats != scan.total_amount {
            return Err(UtxoError::Backend(CoreError::InvalidResponse));
        }
        Ok((balance, utxos))
    }
}

// RawValue preserves the original JSON numeric lexeme: no f64 conversion or rounding.
pub(super) fn satoshis<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let raw = Box::<serde_json::value::RawValue>::deserialize(deserializer)?;
    parse_satoshis(raw.get()).ok_or_else(|| serde::de::Error::custom("invalid Bitcoin amount"))
}

fn parse_satoshis(raw: &str) -> Option<u64> {
    let (whole, fraction) = raw.split_once('.').unwrap_or((raw, ""));
    if whole.is_empty()
        || whole.len() > 8
        || (whole.len() > 1 && whole.starts_with('0'))
        || fraction.len() > 8
        || (raw.contains('.') && fraction.is_empty())
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    // Reject signs, exponents, sub-satoshi precision and values beyond MAX_MONEY.
    let integer = whole.parse::<u64>().ok()?;
    let fractional = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>().ok()?
    };
    let value = integer
        .checked_mul(COIN)?
        .checked_add(fractional * 10_u64.pow(8 - fraction.len() as u32))?;
    (value <= MAX_MONEY).then_some(value)
}

#[derive(Deserialize)]
struct UtxoScan {
    success: bool,
    txouts: u64,
    height: u32,
    bestblock: String,
    unspents: Vec<RawUtxo>,
    #[serde(deserialize_with = "satoshis")]
    total_amount: u64,
}

#[derive(Deserialize)]
struct RawUtxo {
    txid: String,
    vout: u32,
    #[serde(rename = "scriptPubKey")]
    script_pub_key: String,
    #[serde(deserialize_with = "satoshis")]
    amount: u64,
    coinbase: bool,
    height: u32,
    blockhash: String,
    confirmations: u32,
}

#[cfg(test)]
pub(super) mod tests;
