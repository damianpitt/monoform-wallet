//! Fresh confirmed discovery and one protected receive reservation, under one budget.
use super::*;
use crate::storage::{PublicStateStore, StorageError};
use std::time::Instant;

#[derive(Debug, PartialEq, Eq)]
pub enum ReceiveError {
    Load(StorageError),
    Discovery(DiscoveryError),
    SnapshotChanged,
    GapLimit,
    Deadline,
    /// Reload/recover before assuming which counters survived; no address returned.
    Commit(StorageError),
    /// Reservation saved, but too late to return an accepted address.
    CommittedDeadline,
}

/// Privacy-sensitive, transient result. Never Debug, serialized or automatically copied.
pub struct ReceiveAddress {
    pub address: String,
    pub index: u32,
    pub anchor_height: u32,
    pub observed_at: SystemTime,
}

impl CoreTestnet4 {
    /// Disclose descriptors only after user consent; hold the store lock throughout.
    /// No address escapes before fresh discovery, gap checks and acknowledged saving.
    pub fn receive_address(
        &self,
        store: &mut PublicStateStore,
        limits: DiscoveryLimits,
    ) -> Result<ReceiveAddress, ReceiveError> {
        let mut budget = Budget {
            core: self,
            remaining: limits.max_requests,
            started: Instant::now(),
        };
        budget.receive(store, limits)
    }
}

impl Budget<'_> {
    pub(super) fn receive(
        &mut self,
        store: &mut PublicStateStore,
        limits: DiscoveryLimits,
    ) -> Result<ReceiveAddress, ReceiveError> {
        let account = store.public_account().map_err(ReceiveError::Load)?;
        let discovery = self
            .discover(&account, limits)
            .map_err(ReceiveError::Discovery)?;
        // Discovery permits tip growth. Issuance requires that exact tip still current,
        // recent and unpruned before any new reservation, without resetting the budget.
        let tip = self
            .status()
            .map_err(|error| ReceiveError::Discovery(error.into()))?;
        discovery::ready(&tip).map_err(ReceiveError::Discovery)?;
        if tip.blocks != discovery.anchor_height() || tip.tip_hash != discovery.anchor_hash() {
            return Err(ReceiveError::SnapshotChanged);
        }
        self.remaining_time().map_err(|_| ReceiveError::Deadline)?;
        let (address, index) = store
            .reserve_discovered_receive(&discovery)
            .map_err(|error| match error {
                StorageError::GapLimit => ReceiveError::GapLimit,
                other => ReceiveError::Commit(other),
            })?;
        // OS calls cannot be forcibly cancelled. A late successful save consumes an
        // index but returns no address; retry must reload and never rewind the cursor.
        self.remaining_time()
            .map_err(|_| ReceiveError::CommittedDeadline)?;
        Ok(ReceiveAddress {
            address,
            index,
            anchor_height: tip.blocks,
            observed_at: tip.observed_at,
        })
    }
}
