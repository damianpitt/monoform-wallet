//! Explicit, blocking, read-only Core RPC. No wallet imports, signing or broadcast.
mod budget;
mod discovery;
mod mempool;
mod utxos;
use crate::{BitcoinTestnetAccount, bitcoin_address, bitcoin_branch};
use base64::{Engine, engine::general_purpose::STANDARD};
use budget::RpcBudget as Budget;
pub use discovery::{
    BranchDiscovery, Discovery, DiscoveryError, DiscoveryLimits, IncompleteReason,
};
pub use mempool::{
    MempoolError, MempoolIncompleteReason, MempoolObservation, PendingReceipt, PendingSpend,
    PendingSummary,
};
use rustix::fs::{Mode, OFlags};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;
use std::{
    fs::File,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub use utxos::{
    ConfirmedBalance, ObservedUtxo, UtxoError, UtxoIncompleteReason, UtxoObservation, UtxoScope,
};
use zeroize::Zeroizing;

// Core's testnet4 default RPC port and independently published genesis identity.
const ENDPOINT: &str = "http://127.0.0.1:48332/";
const GENESIS: &str = "00000000da84f2bafbbc53dee25a72ae507ff4914b867c565be350b0da8bf043";
const MAX_BODY: u64 = 16 * 1024;
const TIMEOUT: Duration = Duration::from_secs(10);
const TIP_AGE: u64 = 2 * 60 * 60;
const DEADLINE: Duration = Duration::from_secs(60);

/// No underlying error text, credentials, paths or descriptors escape.
#[derive(Debug, PartialEq, Eq)]
pub enum CoreError {
    UnsafeCookie,
    Authentication,
    Unavailable,
    WrongChain,
    InvalidResponse,
    Rpc,
    IncompatibleDescriptor,
    Derivation,
    ChainChanged,
    RequestLimit,
    Deadline,
}

/// Recent is a node-reported observation, not proof of the global chain tip.
#[derive(Debug, PartialEq, Eq)]
pub enum NodeState {
    Syncing,
    Stale,
    Recent,
}

#[derive(Debug)]
pub struct NodeStatus {
    pub blocks: u32,
    pub headers: u32,
    pub tip_hash: String,
    pub tip_time: u64,
    pub pruned: bool,
    pub state: NodeState,
    pub observed_at: SystemTime,
}

/// Fixed loopback endpoint, explicit Core data directory, no retained cookie.
/// Call off the UI thread; construction does not contact a node.
pub struct CoreTestnet4 {
    cookie: PathBuf,
    endpoint: String,
    agent: ureq::Agent,
}

impl CoreTestnet4 {
    pub fn new(datadir: &Path) -> Result<Self, CoreError> {
        if !datadir.is_absolute() {
            return Err(CoreError::UnsafeCookie);
        }
        Ok(Self {
            cookie: datadir.join("testnet4/.cookie"),
            endpoint: ENDPOINT.to_owned(),
            agent: ureq::Agent::config_builder()
                // Never inherit proxy settings or follow a credential-bearing redirect.
                .proxy(None)
                .max_redirects(0)
                .http_status_as_error(false)
                .timeout_global(Some(TIMEOUT))
                .max_response_header_size(4096)
                .max_idle_connections(0)
                .build()
                .into(),
        })
    }

    /// Check chain name AND genesis before returning any usable observation.
    pub fn status(&self) -> Result<NodeStatus, CoreError> {
        let info: ChainInfo = self.rpc("getblockchaininfo", json!([]))?;
        if info.chain != "testnet4" {
            return Err(CoreError::WrongChain);
        }
        let genesis: String = self.rpc("getblockhash", json!([0]))?;
        Self::checked_status(info, genesis)
    }

    // Shared by discovery's request/deadline budget; never skip the identity checks.
    fn checked_status(info: ChainInfo, genesis: String) -> Result<NodeStatus, CoreError> {
        if info.chain != "testnet4" {
            return Err(CoreError::WrongChain);
        }
        if genesis != GENESIS {
            return Err(CoreError::WrongChain);
        }
        let observed_at = SystemTime::now();
        let now = observed_at
            .duration_since(UNIX_EPOCH)
            .map_err(|_| CoreError::InvalidResponse)?
            .as_secs();
        if info.headers < info.blocks
            || !valid_hash(&info.bestblockhash)
            || info.time > now.saturating_add(TIP_AGE)
            || ((info.blocks == 0) != (info.bestblockhash == GENESIS))
        {
            return Err(CoreError::InvalidResponse);
        }
        let state = if info.initialblockdownload || info.headers > info.blocks {
            NodeState::Syncing
        } else if now.saturating_sub(info.time) > TIP_AGE {
            NodeState::Stale
        } else {
            NodeState::Recent
        };
        Ok(NodeStatus {
            blocks: info.blocks,
            headers: info.headers,
            tip_hash: info.bestblockhash,
            tip_time: info.time,
            pruned: info.pruned,
            state,
            observed_at,
        })
    }

    /// Explicitly disclose public descriptors to this local node, only after chain checks.
    /// Compare indices 0 and 1 of both branches without issuing or saving addresses.
    pub fn verify_account(&self, account: &BitcoinTestnetAccount) -> Result<(), CoreError> {
        self.status()?;
        let descriptors = account.descriptors().map_err(|_| CoreError::Derivation)?;
        for (branch, descriptor) in [descriptors.receive, descriptors.change].iter().enumerate() {
            let info: DescriptorInfo = self.rpc("getdescriptorinfo", json!([descriptor]))?;
            if info.hasprivatekeys
                || !info.isrange
                || !info.issolvable
                || descriptor.rsplit_once('#').map(|(_, sum)| sum) != Some(&info.checksum)
            {
                return Err(CoreError::IncompatibleDescriptor);
            }
            // An exact array bounds the schema; extra or missing addresses are rejected.
            let addresses: [String; 2] =
                self.rpc("deriveaddresses", json!([descriptor, [0, 1]]))?;
            for (index, address) in addresses.iter().enumerate() {
                let local = bitcoin_address(
                    bitcoin_branch(&account.account, branch as u32)
                        .map_err(|_| CoreError::Derivation)?,
                    index as u32,
                    bech32::hrp::TB,
                )
                .map_err(|_| CoreError::Derivation)?;
                if *address != local {
                    return Err(CoreError::IncompatibleDescriptor);
                }
            }
        }
        Ok(())
    }

    // Private methods cannot expose arbitrary RPC or caller-supplied URLs to consumers.
    fn rpc<T: DeserializeOwned>(
        &self,
        method: &'static str,
        params: serde_json::Value,
    ) -> Result<T, CoreError> {
        self.rpc_timeout(
            method,
            params,
            self.agent.config().timeouts().global.unwrap_or(TIMEOUT),
        )
    }

    fn rpc_timeout<T: DeserializeOwned>(
        &self,
        method: &'static str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<T, CoreError> {
        // Defense against accidentally adding a mutating method in a future refactor.
        if !matches!(
            method,
            "getblockchaininfo"
                | "getblockhash"
                | "getdescriptorinfo"
                | "deriveaddresses"
                | "scanblocks"
                | "getdescriptoractivity"
                | "scantxoutset"
                | "getmempoolinfo"
                | "getrawmempool"
        ) || (matches!(method, "scanblocks" | "scantxoutset") && params[0] != "start")
        {
            return Err(CoreError::Rpc);
        }
        // Re-read on every call: Core rotates its cookie on restart. No fallback password.
        let cookie = read_cookie(&self.cookie)?;
        let mut authorization = Zeroizing::new(String::from("Basic "));
        STANDARD.encode_string(&*cookie, &mut authorization);
        let mut header = ureq::http::HeaderValue::from_str(&authorization)
            .map_err(|_| CoreError::Authentication)?;
        // Third-party header buffers are not zeroized; suppress their Debug representation.
        header.set_sensitive(true);
        let body = json!({"jsonrpc": "2.0", "id": "monoform", "method": method, "params": params})
            .to_string();
        let mut response = self
            .agent
            .post(&self.endpoint)
            .config()
            .timeout_global(Some(timeout))
            .build()
            .header("authorization", header)
            .header("content-type", "application/json")
            .send(body.as_bytes())
            .map_err(|_| CoreError::Unavailable)?;
        match response.status().as_u16() {
            200 => {}
            401 | 403 => return Err(CoreError::Authentication),
            _ => return Err(CoreError::Unavailable),
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_vec()
            .map_err(|error| match error {
                ureq::Error::Timeout(_) | ureq::Error::Io(_) => CoreError::Unavailable,
                _ => CoreError::InvalidResponse,
            })?;
        let envelope: RpcResponse<T> =
            serde_json::from_slice(&bytes).map_err(|_| CoreError::InvalidResponse)?;
        if envelope.jsonrpc != "2.0" || envelope.id != "monoform" {
            return Err(CoreError::InvalidResponse);
        }
        if let Some(error) = envelope.error {
            if envelope.result.is_some() {
                return Err(CoreError::InvalidResponse);
            }
            // Our bounded, locally constructed ranges/hashes are valid. Core -8
            // here means an inactive block or height lost while the chain changed.
            if error.code == -8
                && matches!(
                    method,
                    "getblockhash" | "getdescriptoractivity" | "scanblocks"
                )
            {
                return Err(CoreError::ChainChanged);
            }
            // Core warming up is unavailable, never an empty successful observation.
            return Err(if error.code == -28 {
                CoreError::Unavailable
            } else {
                CoreError::Rpc
            });
        }
        envelope.result.ok_or(CoreError::InvalidResponse)
    }
}

fn read_cookie(path: &Path) -> Result<Zeroizing<Vec<u8>>, CoreError> {
    // Trusted parent directories remain an OS boundary; reject final symlinks/FIFOs.
    let fd = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| CoreError::Authentication)?;
    let file = File::from(fd);
    let metadata = file.metadata().map_err(|_| CoreError::Authentication)?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o077 != 0
        || metadata.len() > 256
    {
        return Err(CoreError::UnsafeCookie);
    }
    // Fixed capacity avoids reallocations leaving old cookie fragments behind.
    let mut bytes = Zeroizing::new(Vec::with_capacity(257));
    file.take(257)
        .read_to_end(&mut bytes)
        .map_err(|_| CoreError::Authentication)?;
    // Core's standard cookie is this fixed username and a 32-byte lowercase hex token.
    if bytes.len() != 75
        || !bytes.starts_with(b"__cookie__:")
        || !bytes[11..]
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(CoreError::Authentication);
    }
    Ok(bytes)
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

// Known fields are typed (including duplicate rejection); future Core fields may be ignored.
#[derive(Deserialize)]
struct ChainInfo {
    chain: String,
    blocks: u32,
    headers: u32,
    bestblockhash: String,
    time: u64,
    initialblockdownload: bool,
    pruned: bool,
}

#[derive(Deserialize)]
struct DescriptorInfo {
    checksum: String,
    isrange: bool,
    issolvable: bool,
    hasprivatekeys: bool,
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    jsonrpc: String,
    id: String,
    result: Option<T>,
    error: Option<RpcFailure>,
}

#[derive(Deserialize)]
struct RpcFailure {
    code: i64,
}

#[cfg(test)]
mod tests;
