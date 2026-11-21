//! One explicit watch-only refresh; no partial balance result or automatic retry.
use super::*;
use crate::storage::{PublicStateStore, StorageError};
use std::time::Instant;

#[derive(Debug, PartialEq, Eq)]
pub enum RefreshError {
    Load(StorageError),
    Discovery(DiscoveryError),
    Observation(MempoolError),
    SnapshotChanged,
    Deadline,
    /// Commit phase failed: reload before assuming which counters survived.
    Commit(StorageError),
    /// Counters committed successfully, but the observation exceeded the deadline.
    CommittedDeadline,
}

/// Transient wallet-private data. Neither balances nor history records are saved.
pub struct WatchOnlyRefresh {
    pub discovery: Discovery,
    pub observation: MempoolObservation,
}

impl CoreTestnet4 {
    /// Explicit descriptor disclosure and local counter saving, off the UI thread.
    /// `limits.max_requests` caps ALL RPCs; nested stages never reset the deadline.
    pub fn refresh_watch_only(
        &self,
        store: &mut PublicStateStore,
        limits: DiscoveryLimits,
    ) -> Result<WatchOnlyRefresh, RefreshError> {
        let mut budget = Budget {
            core: self,
            remaining: limits.max_requests,
            started: Instant::now(),
        };
        budget.refresh(store, limits)
    }
}

impl Budget<'_> {
    pub(super) fn refresh(
        &mut self,
        store: &mut PublicStateStore,
        limits: DiscoveryLimits,
    ) -> Result<WatchOnlyRefresh, RefreshError> {
        let account = store.public_account().map_err(RefreshError::Load)?;
        let discovery = self
            .discover(&account, limits)
            .map_err(RefreshError::Discovery)?;
        // Observe the complete examined prefixes, including the unused stopping gap.
        let scope = UtxoScope {
            receive_count: discovery.receive().examined,
            change_count: discovery.change().examined,
        };
        let observation = self
            .reconcile_mempool(&discovery.recommendation, scope)
            .map_err(RefreshError::Observation)?;
        // Even normal tip growth requires an explicit retry: do not label a later
        // balance with history coverage established at an earlier anchor.
        if discovery.anchor_height() != observation.confirmed.anchor_height
            || discovery.anchor_hash() != observation.confirmed.anchor_hash
        {
            return Err(RefreshError::SnapshotChanged);
        }
        self.remaining_time().map_err(|_| RefreshError::Deadline)?;
        store
            .apply_discovery(&discovery)
            .map_err(RefreshError::Commit)?;
        // OS I/O cannot be forcibly timed out. Report late completion explicitly.
        self.remaining_time()
            .map_err(|_| RefreshError::CommittedDeadline)?;
        Ok(WatchOnlyRefresh {
            discovery,
            observation,
        })
    }
}
