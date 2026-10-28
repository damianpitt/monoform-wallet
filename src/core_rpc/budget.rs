//! Shared read-only request/deadline accounting; context-specific errors stay in adapters.
use super::*;
use std::time::Instant;

pub(super) struct RpcBudget<'a> {
    pub(super) core: &'a CoreTestnet4,
    pub(super) remaining: u32,
    pub(super) started: Instant,
}

impl RpcBudget<'_> {
    pub(super) fn call<T: DeserializeOwned>(
        &mut self,
        method: &'static str,
        params: serde_json::Value,
    ) -> Result<T, CoreError> {
        let left = DEADLINE
            .checked_sub(self.started.elapsed())
            .filter(|left| !left.is_zero())
            .ok_or(CoreError::Deadline)?;
        if self.remaining == 0 {
            return Err(CoreError::RequestLimit);
        }
        self.remaining -= 1;
        let result = self.core.rpc_timeout(method, params, left.min(TIMEOUT));
        if self.started.elapsed() >= DEADLINE {
            return Err(CoreError::Deadline);
        }
        result
    }

    pub(super) fn status(&mut self) -> Result<NodeStatus, CoreError> {
        let info: ChainInfo = self.call("getblockchaininfo", json!([]))?;
        if info.chain != "testnet4" {
            return Err(CoreError::WrongChain);
        }
        let genesis = self.call("getblockhash", json!([0]))?;
        CoreTestnet4::checked_status(info, genesis)
    }
}
