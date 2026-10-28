//! Fresh confirmed state plus a bounded, sequence-bracketed local mempool overlay.
use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
use utxos::{MAX_MONEY, satoshis};

const MAX_POOL: usize = 128;
const MAX_EVENTS: usize = 64;

#[derive(Debug, PartialEq, Eq)]
pub enum MempoolIncompleteReason {
    NotLoaded,
    PoolLimit,
    ActivityLimit,
    Unavailable,
    RequestLimit,
    Deadline,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MempoolError {
    Confirmed(UtxoError),
    Incomplete(MempoolIncompleteReason),
    SnapshotChanged,
    Reorg,
    Backend(CoreError),
}

impl From<UtxoError> for MempoolError {
    fn from(error: UtxoError) -> Self {
        Self::Confirmed(error)
    }
}
impl From<CoreError> for MempoolError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::Rpc => Self::Incomplete(MempoolIncompleteReason::Unavailable),
            CoreError::RequestLimit => Self::Incomplete(MempoolIncompleteReason::RequestLimit),
            CoreError::Deadline => Self::Incomplete(MempoolIncompleteReason::Deadline),
            CoreError::ChainChanged => Self::Reorg,
            _ => Self::Backend(error),
        }
    }
}

pub struct PendingReceipt {
    pub txid: String,
    pub vout: u32,
    pub branch: u32,
    pub index: u32,
    pub amount_sats: u64,
    pub spent_in_mempool: bool,
}
pub struct PendingSpend {
    pub txid: String,
    pub vin: u32,
    pub prevout_txid: String,
    pub prevout_vout: u32,
    pub branch: u32,
    pub index: u32,
    pub amount_sats: u64,
    pub spends_confirmed: bool,
}
pub struct PendingSummary {
    /// Gross confirmed inputs consumed, not payment value or fee attribution.
    pub confirmed_pending_spent_sats: u64,
    /// Only terminal pending wallet outputs; includes change and self-transfers.
    pub pending_unspent_receipts_sats: u64,
    /// Confirmed minus pending-spent confirmed inputs plus terminal pending outputs.
    /// Includes immature coins; never an available/spendable balance.
    pub projected_total_sats: u64,
}
/// Wallet-private data deliberately has no Debug/serialization/cache path.
pub struct MempoolObservation {
    pub confirmed: UtxoObservation,
    pub mempool_sequence: u64,
    pub observed_at: SystemTime,
    pub receipts: Vec<PendingReceipt>,
    pub spends: Vec<PendingSpend>,
    pub summary: PendingSummary,
}

impl CoreTestnet4 {
    /// Query fresh confirmed state under the SAME deadline as its pending overlay.
    /// Never accept a detached/mutable cached balance supplied by another account/UI.
    pub fn reconcile_mempool(
        &self,
        account: &BitcoinTestnetAccount,
        scope: UtxoScope,
    ) -> Result<MempoolObservation, MempoolError> {
        utxos::validate_scope(account, scope)?;
        let mut budget = Budget {
            core: self,
            remaining: 112,
            started: Instant::now(),
        };
        let before = budget.status()?;
        utxos::ready(&before)?;
        let pool = budget.pool()?;
        let confirmed = budget.observe_utxos(account, scope)?;
        if confirmed.anchor_height != before.blocks || confirmed.anchor_hash != before.tip_hash {
            return Err(MempoolError::SnapshotChanged);
        }
        let scripts = utxos::scoped_scripts(account, scope)?;
        let activity = budget.pending_activity(account, scope)?;
        let (receipts, spends, summary) = reconcile(&confirmed, &pool, activity, &scripts)?;
        let after = budget.status()?;
        if after.blocks < before.blocks
            || budget.call::<String>("getblockhash", json!([before.blocks]))? != before.tip_hash
        {
            return Err(MempoolError::Reorg);
        }
        utxos::ready(&after)?;
        if after.blocks != before.blocks
            || after.tip_hash != before.tip_hash
            || budget.pool()? != pool
        {
            return Err(MempoolError::SnapshotChanged);
        }
        Ok(MempoolObservation {
            confirmed,
            mempool_sequence: pool.mempool_sequence,
            observed_at: SystemTime::now(),
            receipts,
            spends,
            summary,
        })
    }
}

impl Budget<'_> {
    fn pool(&mut self) -> Result<Pool, MempoolError> {
        let info: PoolInfo = self.call("getmempoolinfo", json!([]))?;
        if !info.loaded {
            return Err(MempoolError::Incomplete(MempoolIncompleteReason::NotLoaded));
        }
        // Reject busy nodes before downloading a global transaction list.
        if info.size > MAX_POOL as u64 {
            return Err(MempoolError::Incomplete(MempoolIncompleteReason::PoolLimit));
        }
        let raw: RawPool = self.call("getrawmempool", json!([false, true]))?;
        if raw.txids.len() > MAX_POOL {
            return Err(MempoolError::Incomplete(MempoolIncompleteReason::PoolLimit));
        }
        let txids = raw.txids.iter().cloned().collect::<BTreeSet<_>>();
        if txids.len() != raw.txids.len() || txids.iter().any(|txid| !valid_hash(txid)) {
            return Err(invalid());
        }
        if txids.len() as u64 != info.size {
            return Err(MempoolError::SnapshotChanged);
        }
        Ok(Pool {
            txids,
            mempool_sequence: raw.mempool_sequence,
        })
    }

    fn pending_activity(
        &mut self,
        account: &BitcoinTestnetAccount,
        scope: UtxoScope,
    ) -> Result<Activity, MempoolError> {
        let descriptors = account.descriptors().map_err(|_| CoreError::Derivation)?;
        let activity: Activity = self.call(
            "getdescriptoractivity",
            json!([[],[
            {"desc":descriptors.receive,"range":[0,scope.receive_count-1]},
            {"desc":descriptors.change,"range":[0,scope.change_count-1]}
        ],true]),
        )?;
        if activity.activity.len() > MAX_EVENTS {
            return Err(MempoolError::Incomplete(
                MempoolIncompleteReason::ActivityLimit,
            ));
        }
        Ok(activity)
    }
}

fn invalid() -> MempoolError {
    MempoolError::Backend(CoreError::InvalidResponse)
}
fn add(total: u64, amount: u64) -> Result<u64, MempoolError> {
    total
        .checked_add(amount)
        .filter(|sum| *sum <= MAX_MONEY)
        .ok_or_else(invalid)
}

type ReceiptMap = BTreeMap<(String, u32), PendingReceipt>;

fn collect_activity(
    pool: &Pool,
    activity: Activity,
    scripts: &BTreeMap<String, (u32, u32)>,
) -> Result<(ReceiptMap, Vec<PendingSpend>), MempoolError> {
    let mut receipts = BTreeMap::new();
    let mut spends = Vec::new();
    let mut inputs = BTreeSet::new();
    for event in activity.activity {
        let (txid, script) = match event.kind {
            Kind::Receive => (event.txid.as_ref(), event.output_spk.as_ref()),
            Kind::Spend => (event.spend_txid.as_ref(), event.prevout_spk.as_ref()),
        };
        let txid = txid.ok_or_else(invalid)?;
        let script = script.ok_or_else(invalid)?;
        if !pool.txids.contains(txid) || event.height.is_some() || event.blockhash.is_some() {
            return Err(invalid());
        }
        let &(branch, index) = scripts.get(&script.hex).ok_or_else(invalid)?;
        match event.kind {
            Kind::Receive => {
                if event.spend_txid.is_some()
                    || event.spend_vin.is_some()
                    || event.prevout_txid.is_some()
                    || event.prevout_vout.is_some()
                    || event.prevout_spk.is_some()
                {
                    return Err(invalid());
                }
                let txid = event.txid.ok_or_else(invalid)?;
                let vout = event.vout.ok_or_else(invalid)?;
                let receipt = PendingReceipt {
                    txid: txid.clone(),
                    vout,
                    branch,
                    index,
                    amount_sats: event.amount,
                    spent_in_mempool: false,
                };
                if receipts.insert((txid, vout), receipt).is_some() {
                    return Err(invalid());
                }
            }
            Kind::Spend => {
                if event.txid.is_some() || event.vout.is_some() || event.output_spk.is_some() {
                    return Err(invalid());
                }
                let spend_txid = event.spend_txid.ok_or_else(invalid)?;
                let spend_vin = event.spend_vin.ok_or_else(invalid)?;
                let prevout_txid = event.prevout_txid.ok_or_else(invalid)?;
                let prevout_vout = event.prevout_vout.ok_or_else(invalid)?;
                if !valid_hash(&prevout_txid) || !inputs.insert((spend_txid.clone(), spend_vin)) {
                    return Err(invalid());
                }
                spends.push(PendingSpend {
                    txid: spend_txid,
                    vin: spend_vin,
                    prevout_txid,
                    prevout_vout,
                    branch,
                    index,
                    amount_sats: event.amount,
                    spends_confirmed: false,
                });
            }
        }
    }
    Ok((receipts, spends))
}

// Two passes make dependency chains independent of Core's event ordering.
fn reconcile(
    confirmed: &UtxoObservation,
    pool: &Pool,
    activity: Activity,
    scripts: &BTreeMap<String, (u32, u32)>,
) -> Result<(Vec<PendingReceipt>, Vec<PendingSpend>, PendingSummary), MempoolError> {
    let (mut receipts, mut spends) = collect_activity(pool, activity, scripts)?;
    let coins = confirmed
        .utxos
        .iter()
        .map(|coin| ((coin.txid.clone(), coin.vout), coin))
        .collect::<BTreeMap<_, _>>();
    if coins
        .keys()
        .any(|outpoint| pool.txids.contains(&outpoint.0) || receipts.contains_key(outpoint))
    {
        return Err(invalid());
    }
    let mut consumed = BTreeSet::new();
    let mut edges = Vec::new();
    let mut confirmed_spent = 0;
    for spend in &mut spends {
        let outpoint = (spend.prevout_txid.clone(), spend.prevout_vout);
        if !consumed.insert(outpoint.clone()) {
            return Err(invalid());
        }
        let (amount, branch, index) = if let Some(coin) = coins.get(&outpoint) {
            // At least 100 confirmations are needed for inclusion in the next block.
            // This differs from the conservative <=100 immature display bucket.
            if coin.coinbase && coin.confirmations < 100 {
                return Err(invalid());
            }
            spend.spends_confirmed = true;
            confirmed_spent = add(confirmed_spent, spend.amount_sats)?;
            (coin.amount_sats, coin.branch, coin.index)
        } else if let Some(receipt) = receipts.get_mut(&outpoint) {
            receipt.spent_in_mempool = true;
            edges.push((receipt.txid.clone(), spend.txid.clone()));
            (receipt.amount_sats, receipt.branch, receipt.index)
        } else {
            return Err(invalid());
        };
        if (spend.amount_sats, spend.branch, spend.index) != (amount, branch, index) {
            return Err(invalid());
        }
    }
    // Repeatedly remove edges whose parent has no wallet-relevant pending parents.
    // A remainder with no progress is a cycle, impossible in genuine transactions.
    while !edges.is_empty() {
        let children = edges
            .iter()
            .map(|(_, child)| child.clone())
            .collect::<BTreeSet<_>>();
        let old = edges.len();
        edges.retain(|(parent, _)| children.contains(parent));
        if edges.len() == old {
            return Err(invalid());
        }
    }
    let pending_receipts = receipts
        .values()
        .filter(|receipt| !receipt.spent_in_mempool)
        .try_fold(0, |sum, receipt| add(sum, receipt.amount_sats))?;
    let remainder = confirmed
        .balance
        .total_sats
        .checked_sub(confirmed_spent)
        .ok_or_else(invalid)?;
    let summary = PendingSummary {
        confirmed_pending_spent_sats: confirmed_spent,
        pending_unspent_receipts_sats: pending_receipts,
        projected_total_sats: add(remainder, pending_receipts)?,
    };
    Ok((receipts.into_values().collect(), spends, summary))
}

#[derive(Deserialize)]
struct PoolInfo {
    loaded: bool,
    size: u64,
}
#[derive(PartialEq, Eq)]
struct Pool {
    txids: BTreeSet<String>,
    mempool_sequence: u64,
}
#[derive(Deserialize)]
struct RawPool {
    txids: Vec<String>,
    mempool_sequence: u64,
}
#[derive(Deserialize)]
struct Activity {
    activity: Vec<Event>,
}
#[derive(Deserialize)]
struct Script {
    hex: String,
}
#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Receive,
    Spend,
}

// Keep amounts in a flat wire struct. Serde's internally-tagged enum buffering
// loses the original numeric lexeme needed by RawValue's exact satoshi decoder.
// The selected kind's required/forbidden fields are checked before projection.
#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: Kind,
    #[serde(deserialize_with = "satoshis")]
    amount: u64,
    height: Option<u32>,
    blockhash: Option<String>,
    txid: Option<String>,
    vout: Option<u32>,
    output_spk: Option<Script>,
    spend_txid: Option<String>,
    spend_vin: Option<u32>,
    prevout_txid: Option<String>,
    prevout_vout: Option<u32>,
    prevout_spk: Option<Script>,
}

#[cfg(test)]
mod tests;
