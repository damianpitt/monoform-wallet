//! Confirmed history only, from genesis to one pinned tip. No persistence or balances.
use super::*;
use bip32::ChildNumber;
use bitcoin_hashes::{Hash, hash160};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

const GAP: u32 = 20;
const BLOCK_CHUNK: u32 = 4096;

/// Explicit work limits; defaults are conservative, never an unlimited recovery scan.
pub struct DiscoveryLimits {
    pub max_addresses_per_branch: u32,
    pub max_height: u32,
    pub max_requests: u32,
}

impl Default for DiscoveryLimits {
    fn default() -> Self {
        Self {
            max_addresses_per_branch: 200,
            max_height: 1_000_000,
            max_requests: 256,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum IncompleteReason {
    Syncing,
    StaleTip,
    PrunedHistory,
    HistoryUnavailable,
    AddressLimit,
    AddressSpaceExhausted,
    BlockLimit,
    RequestLimit,
    Deadline,
}

/// Static errors; a failed pass exposes no partial address/counter recommendations.
#[derive(Debug, PartialEq, Eq)]
pub enum DiscoveryError {
    InvalidLimits,
    Incomplete(IncompleteReason),
    Reorg,
    Backend(CoreError),
}

// Shared transport failures acquire history-specific meaning only at this boundary.
impl From<CoreError> for DiscoveryError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::Rpc => incomplete(IncompleteReason::HistoryUnavailable),
            CoreError::ChainChanged => Self::Reorg,
            CoreError::RequestLimit => incomplete(IncompleteReason::RequestLimit),
            CoreError::Deadline => incomplete(IncompleteReason::Deadline),
            _ => Self::Backend(error),
        }
    }
}

pub struct BranchDiscovery {
    pub examined: u32,
    pub last_used: Option<u32>,
    /// Recommendation only: max(saved next index, last confirmed-used index + 1).
    pub next_index: u32,
}

/// Complete only under the documented gap policy, at this confirmed-chain anchor.
/// Neither this result nor its branch counters are automatically persisted.
pub struct Discovery {
    pub anchor_height: u32,
    pub anchor_hash: String,
    pub receive: BranchDiscovery,
    pub change: BranchDiscovery,
}

impl CoreTestnet4 {
    /// Explicit account disclosure to the user's local Core; requires blockfilterindex.
    /// Saved cursors remain untouched even on success, limits, node failure or reorg.
    pub fn discover(
        &self,
        account: &BitcoinTestnetAccount,
        limits: DiscoveryLimits,
    ) -> Result<Discovery, DiscoveryError> {
        if !(GAP..=10_000).contains(&limits.max_addresses_per_branch)
            || limits.max_height > 1_000_000
            || !(1..=512).contains(&limits.max_requests)
        {
            return Err(DiscoveryError::InvalidLimits);
        }
        for saved in [account.next_receive, account.next_change] {
            if saved + GAP > ChildNumber::HARDENED_FLAG {
                return Err(incomplete(IncompleteReason::AddressSpaceExhausted));
            }
            if saved + GAP > limits.max_addresses_per_branch {
                return Err(incomplete(IncompleteReason::AddressLimit));
            }
        }
        let mut budget = Budget {
            core: self,
            remaining: limits.max_requests,
            started: Instant::now(),
        };
        let anchor = budget.status()?;
        ready(&anchor)?;
        if anchor.blocks > limits.max_height {
            return Err(incomplete(IncompleteReason::BlockLimit));
        }
        // Pin before disclosing descriptors, then check again before returning success.
        if budget.call::<String>("getblockhash", json!([anchor.blocks]))? != anchor.tip_hash {
            return Err(DiscoveryError::Reorg);
        }
        let descriptors = account
            .descriptors()
            .map_err(|_| DiscoveryError::Backend(CoreError::Derivation))?;
        let receive = budget.branch(account, 0, &descriptors.receive, &anchor, &limits)?;
        let change = budget.branch(account, 1, &descriptors.change, &anchor, &limits)?;
        let after = budget.status()?;
        if after.blocks < anchor.blocks
            || budget.call::<String>("getblockhash", json!([anchor.blocks]))? != anchor.tip_hash
        {
            return Err(DiscoveryError::Reorg);
        }
        ready(&after)?;
        // A longer chain with the original anchor still active is allowed, not a reorg.
        Ok(Discovery {
            anchor_height: anchor.blocks,
            anchor_hash: anchor.tip_hash,
            receive,
            change,
        })
    }
}

fn incomplete(reason: IncompleteReason) -> DiscoveryError {
    DiscoveryError::Incomplete(reason)
}

fn ready(status: &NodeStatus) -> Result<(), DiscoveryError> {
    if status.pruned {
        return Err(incomplete(IncompleteReason::PrunedHistory));
    }
    match status.state {
        NodeState::Syncing => Err(incomplete(IncompleteReason::Syncing)),
        NodeState::Stale => Err(incomplete(IncompleteReason::StaleTip)),
        NodeState::Recent => Ok(()),
    }
}

impl Budget<'_> {
    fn branch(
        &mut self,
        account: &BitcoinTestnetAccount,
        branch: u32,
        descriptor: &str,
        anchor: &NodeStatus,
        limits: &DiscoveryLimits,
    ) -> Result<BranchDiscovery, DiscoveryError> {
        let saved = if branch == 0 {
            account.next_receive
        } else {
            account.next_change
        };
        let mut result = BranchDiscovery {
            examined: 0,
            last_used: None,
            next_index: saved,
        };
        while result.examined < limits.max_addresses_per_branch {
            let end = (result.examined + GAP).min(limits.max_addresses_per_branch) - 1;
            let scripts = scripts(account, branch, result.examined, end)?;
            let objects = json!([{ "desc": descriptor, "range": [result.examined, end] }]);
            for from in (0..=anchor.blocks).step_by(BLOCK_CHUNK as usize) {
                let to = (from + BLOCK_CHUNK - 1).min(anchor.blocks);
                let scan: Scan =
                    self.call("scanblocks", json!(["start", objects, from, to, "basic"]))?;
                if !scan.completed {
                    return Err(incomplete(IncompleteReason::HistoryUnavailable));
                }
                let unique = scan.relevant_blocks.iter().collect::<BTreeSet<_>>();
                if scan.from_height != from
                    || scan.to_height != to
                    || unique.len() != scan.relevant_blocks.len()
                    || unique.len() > (to - from + 1) as usize
                    || unique.iter().any(|hash| !valid_hash(hash))
                {
                    return Err(DiscoveryError::Backend(CoreError::InvalidResponse));
                }
                for hash in scan.relevant_blocks {
                    // Confirm filter candidates against full block/undo activity. False
                    // positives are NOT usage; mempool is deliberately excluded.
                    let activity: Activity =
                        self.call("getdescriptoractivity", json!([[hash], objects, false]))?;
                    for event in activity.activity {
                        let (height, blockhash, script) = match event {
                            Event::Receive {
                                height,
                                blockhash,
                                output_spk,
                            } => (height, blockhash, output_spk),
                            Event::Spend {
                                height,
                                blockhash,
                                prevout_spk,
                            } => (height, blockhash, prevout_spk),
                        };
                        let index = scripts
                            .get(&script.hex)
                            .ok_or(DiscoveryError::Backend(CoreError::InvalidResponse))?;
                        if height < from || height > to || blockhash != hash {
                            return Err(DiscoveryError::Backend(CoreError::InvalidResponse));
                        }
                        if self.call::<String>("getblockhash", json!([height]))? != hash {
                            return Err(DiscoveryError::Reorg);
                        }
                        result.last_used =
                            Some(result.last_used.map_or(*index, |last| last.max(*index)));
                    }
                }
            }
            // Advance examination only after the entire anchored history was covered.
            result.examined = end + 1;
            result.next_index = saved.max(result.last_used.map_or(0, |last| last + 1));
            if result.examined >= result.next_index + GAP {
                return Ok(result);
            }
        }
        Err(incomplete(IncompleteReason::AddressLimit))
    }
}

pub(super) fn scripts(
    account: &BitcoinTestnetAccount,
    branch: u32,
    from: u32,
    to: u32,
) -> Result<BTreeMap<String, u32>, DiscoveryError> {
    let key = bitcoin_branch(&account.account, branch)
        .map_err(|_| DiscoveryError::Backend(CoreError::Derivation))?;
    (from..=to)
        .map(|index| {
            let child = key
                .derive_child(ChildNumber(index))
                .map_err(|_| DiscoveryError::Backend(CoreError::Derivation))?;
            // P2WPKH script is OP_0 + 20-byte HASH160 of the compressed public key.
            Ok((
                format!("0014{}", hash160::Hash::hash(&child.to_bytes())),
                index,
            ))
        })
        .collect()
}

#[derive(Deserialize)]
struct Scan {
    from_height: u32,
    to_height: u32,
    relevant_blocks: Vec<String>,
    completed: bool,
}
#[derive(Deserialize)]
struct Activity {
    activity: Vec<Event>,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Event {
    Receive {
        height: u32,
        blockhash: String,
        output_spk: Script,
    },
    Spend {
        height: u32,
        blockhash: String,
        prevout_spk: Script,
    },
}
#[derive(Deserialize)]
struct Script {
    hex: String,
}

#[cfg(test)]
mod tests;
