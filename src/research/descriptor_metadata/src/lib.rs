//! Deterministic harness for descriptor-metadata restoration research.

pub mod density;
pub mod density_plot;

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use bdk_wallet::{
    bitcoin::Network,
    chain::keychain_txout::RestoreKeychainError,
    chain::spk_client::{FullScanRequest, SyncItem, SyncRequest},
    test_utils::get_test_wpkh_and_change_desc,
    KeychainKind, SpkMetadata, SpkMetadataError, Wallet,
};

/// Number of SPKs consumed before evaluating a stop condition.
pub const PARALLEL_REQUESTS: usize = 10;

/// Stable identifier for a canonical research fixture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureId {
    /// Empty metadata snapshot.
    Empty,
    /// Source example with used indexes 1, 20, and 50.
    Sparse1_20_50,
    /// Source indexes with a revealed frontier of 100.
    Revealed100Sparse,
    /// Every index from 0 through 100 is used.
    Dense100,
    /// Every tenth index from 0 through 1000 is used.
    Periodic10_1000,
    /// Two used indexes separated by 49,999 unused indexes.
    HighGap50000,
    /// A revealed frontier of 50,000 with no known activity.
    Revealed50000Unused,
}

/// Immutable wallet-state snapshot used by every strategy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fixture {
    /// Stable fixture identifier.
    pub id: FixtureId,
    last_revealed: Option<u32>,
    used_indexes: Vec<u32>,
}

impl Fixture {
    /// Return the highest address index revealed when the snapshot was exported.
    pub fn last_revealed(&self) -> Option<u32> {
        self.last_revealed
    }

    /// Return the sorted indexes with known activity at export time.
    pub fn used_indexes(&self) -> &[u32] {
        &self.used_indexes
    }

    /// Build validated metadata for this snapshot.
    pub fn metadata(&self) -> Result<SpkMetadata, SpkMetadataError> {
        SpkMetadata::with_last_revealed(
            KeychainKind::External,
            self.last_revealed,
            self.used_indexes.clone(),
        )
    }

    /// Build the independent activity oracle used by descriptor scanning.
    pub fn history_oracle(&self) -> HistoryOracle {
        HistoryOracle {
            used_indexes: self.used_indexes.iter().copied().collect(),
            expected_indexes: self.used_indexes.clone(),
        }
    }
}

impl FixtureId {
    /// Return the stable identifier used in reports and benchmark names.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Sparse1_20_50 => "sparse_1_20_50",
            Self::Revealed100Sparse => "revealed_100_sparse",
            Self::Dense100 => "dense_100",
            Self::Periodic10_1000 => "periodic_10_1000",
            Self::HighGap50000 => "high_gap_50000",
            Self::Revealed50000Unused => "revealed_50000_unused",
        }
    }
}

/// Result of consuming a descriptor full-scan request with stop-gap semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptorStopGapObservation {
    /// Whether every fixture index with history was discovered.
    pub complete: bool,
    /// Number of real descriptor SPKs consumed from the request.
    pub requested_spks: usize,
    /// Sorted indexes for which the oracle reported history.
    pub discovered_indexes: Vec<u32>,
    /// Trailing empty SPKs consumed beyond the configured stop-gap.
    pub batch_overfetch: usize,
}

/// Result of the current apply-metadata and revealed-range request path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataRevealedObservation {
    /// Number of SPKs consumed from the revealed-range request.
    pub requested_spks: usize,
    /// Highest revealed index after applying metadata.
    pub restored_last_revealed: Option<u32>,
    /// Whether every exported used index was restored as used.
    pub used_markers_restored: bool,
}

/// Result of the real sparse restore request containing exported used indexes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataSparseObservation {
    /// Number of SPKs consumed from the bounded request.
    pub requested_spks: usize,
    /// Indexes included in the bounded request.
    pub requested_indexes: Vec<u32>,
}

/// Immutable history oracle prepared outside timed benchmark regions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryOracle {
    used_indexes: BTreeSet<u32>,
    expected_indexes: Vec<u32>,
}

/// Error returned by the real sparse restoration path.
#[derive(Debug)]
pub enum ResearchError {
    /// Fixture metadata is inconsistent.
    Metadata(SpkMetadataError),
    /// The keychain index could not restore the snapshot.
    Restore(RestoreKeychainError),
}

impl std::fmt::Display for ResearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Metadata(error) => error.fmt(f),
            Self::Restore(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ResearchError {}

impl From<SpkMetadataError> for ResearchError {
    fn from(error: SpkMetadataError) -> Self {
        Self::Metadata(error)
    }
}

impl From<RestoreKeychainError> for ResearchError {
    fn from(error: RestoreKeychainError) -> Self {
        Self::Restore(error)
    }
}

/// Return the canonical deterministic fixture corpus.
pub fn fixture_catalog() -> Vec<Fixture> {
    vec![
        Fixture {
            id: FixtureId::Empty,
            last_revealed: None,
            used_indexes: Vec::new(),
        },
        Fixture {
            id: FixtureId::Sparse1_20_50,
            last_revealed: Some(50),
            used_indexes: vec![1, 20, 50],
        },
        Fixture {
            id: FixtureId::Revealed100Sparse,
            last_revealed: Some(100),
            used_indexes: vec![1, 20, 50],
        },
        Fixture {
            id: FixtureId::Dense100,
            last_revealed: Some(100),
            used_indexes: (0..=100).collect(),
        },
        Fixture {
            id: FixtureId::Periodic10_1000,
            last_revealed: Some(1_000),
            used_indexes: (0..=1_000).step_by(10).collect(),
        },
        Fixture {
            id: FixtureId::HighGap50000,
            last_revealed: Some(50_000),
            used_indexes: vec![0, 50_000],
        },
        Fixture {
            id: FixtureId::Revealed50000Unused,
            last_revealed: Some(50_000),
            used_indexes: Vec::new(),
        },
    ]
}

/// Construct a fresh in-memory wallet with the study's fixed WPKH descriptors.
pub fn new_research_wallet() -> Wallet {
    let (descriptor, change_descriptor) = get_test_wpkh_and_change_desc();
    Wallet::create(descriptor, change_descriptor)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("fixed research descriptors must create a wallet")
}

/// Build the lazy descriptor full-scan request consumed by the baseline.
pub fn descriptor_full_scan_request(wallet: &Wallet) -> FullScanRequest<KeychainKind> {
    wallet.start_full_scan_at(0).build()
}

/// Consume a real full-scan request using deterministic stop-gap history.
pub fn consume_descriptor_stop_gap(
    mut request: FullScanRequest<KeychainKind>,
    oracle: &HistoryOracle,
    stop_gap: usize,
    parallel_requests: usize,
) -> DescriptorStopGapObservation {
    let stop_gap = stop_gap.max(1);
    let batch_size = parallel_requests.max(1);
    let mut requested_spks = 0usize;
    let mut consecutive_unused = 0usize;
    let mut discovered_indexes = Vec::new();

    loop {
        let batch = request
            .iter_spks(KeychainKind::External)
            .take(batch_size)
            .collect::<Vec<_>>();
        if batch.is_empty() {
            break;
        }

        requested_spks = requested_spks.saturating_add(batch.len());
        for (index, spk) in batch {
            std::hint::black_box(spk);
            if oracle.used_indexes.contains(&index) {
                consecutive_unused = 0;
                discovered_indexes.push(index);
            } else {
                consecutive_unused = consecutive_unused.saturating_add(1);
            }
        }

        if consecutive_unused >= stop_gap {
            break;
        }
    }

    DescriptorStopGapObservation {
        complete: discovered_indexes == oracle.expected_indexes,
        requested_spks,
        discovered_indexes,
        batch_overfetch: consecutive_unused.saturating_sub(stop_gap),
    }
}

/// Run the descriptor-only baseline from a fresh wallet.
pub fn descriptor_stop_gap(
    fixture: &Fixture,
    stop_gap: usize,
    parallel_requests: usize,
) -> DescriptorStopGapObservation {
    let wallet = new_research_wallet();
    let oracle = fixture.history_oracle();
    consume_descriptor_stop_gap(
        descriptor_full_scan_request(&wallet),
        &oracle,
        stop_gap,
        parallel_requests,
    )
}

/// Apply fixture metadata to a wallet using the current contiguous reveal API.
pub fn apply_fixture_metadata(
    wallet: &mut Wallet,
    fixture: &Fixture,
) -> Result<(), SpkMetadataError> {
    apply_metadata(wallet, &fixture.metadata()?);
    Ok(())
}

/// Apply already validated metadata using the current contiguous reveal API.
pub fn apply_metadata(wallet: &mut Wallet, metadata: &SpkMetadata) {
    wallet.apply_spk_metadata(metadata);
}

/// Build the current request containing every revealed external SPK.
pub fn metadata_revealed_request(wallet: &Wallet) -> SyncRequest<(KeychainKind, u32)> {
    wallet.start_sync_with_revealed_spks_at(0).build()
}

/// Restore fixture state and build a request containing the snapshot's used indexes.
pub fn metadata_sparse_request(
    wallet: &mut Wallet,
    fixture: &Fixture,
) -> Result<SyncRequest<(KeychainKind, u32)>, ResearchError> {
    Ok(metadata_sparse_request_from_metadata(
        wallet,
        &fixture.metadata()?,
    )?)
}

/// Apply validated metadata and build the real index-backed sparse request.
pub fn metadata_sparse_request_from_metadata(
    wallet: &mut Wallet,
    metadata: &SpkMetadata,
) -> Result<SyncRequest<(KeychainKind, u32)>, RestoreKeychainError> {
    wallet.apply_spk_metadata_sparse(metadata)?;
    Ok(wallet.start_sync_with_spk_metadata_at(0).build())
}

/// Consume every SPK in a bounded sync request.
pub fn consume_sync_request(mut request: SyncRequest<(KeychainKind, u32)>) -> usize {
    request
        .iter_spks_with_expected_txids()
        .fold(0usize, |consumed, item| {
            std::hint::black_box(item.spk);
            consumed.saturating_add(1)
        })
}

/// Observe the current apply-metadata and revealed-range request path.
pub fn metadata_revealed(
    fixture: &Fixture,
) -> Result<MetadataRevealedObservation, SpkMetadataError> {
    let mut wallet = new_research_wallet();
    apply_fixture_metadata(&mut wallet, fixture)?;

    let restored_last_revealed = wallet.derivation_index(KeychainKind::External);
    let unused_indexes = wallet
        .list_unused_addresses(KeychainKind::External)
        .map(|address| address.index)
        .collect::<BTreeSet<_>>();
    let used_markers_restored = fixture
        .used_indexes
        .iter()
        .all(|index| !unused_indexes.contains(index));
    let requested_spks = consume_sync_request(metadata_revealed_request(&wallet));

    Ok(MetadataRevealedObservation {
        requested_spks,
        restored_last_revealed,
        used_markers_restored,
    })
}

/// Observe the real sparse state restore and bounded request.
pub fn metadata_sparse_restore(
    fixture: &Fixture,
) -> Result<MetadataSparseObservation, ResearchError> {
    let mut wallet = new_research_wallet();
    let metadata = fixture.metadata()?;
    let requested_indexes = Arc::new(Mutex::new(Vec::new()));
    let inspected_indexes = Arc::clone(&requested_indexes);
    wallet.apply_spk_metadata_sparse(&metadata)?;
    let request = wallet
        .start_sync_with_spk_metadata_at(0)
        .inspect(move |item, _progress| {
            if let SyncItem::Spk((KeychainKind::External, index), _) = item {
                inspected_indexes
                    .lock()
                    .expect("metadata index observer mutex must not be poisoned")
                    .push(index);
            }
        })
        .build();
    let requested_spks = consume_sync_request(request);
    let requested_indexes = requested_indexes
        .lock()
        .expect("metadata index observer mutex must not be poisoned")
        .clone();

    Ok(MetadataSparseObservation {
        requested_spks,
        requested_indexes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_stop_gap_uses_esplora_post_batch_evaluation() {
        let fixture = Fixture {
            id: FixtureId::Sparse1_20_50,
            last_revealed: Some(6),
            used_indexes: vec![0, 6],
        };

        let observation = descriptor_stop_gap(&fixture, 5, 10);

        assert!(observation.complete);
        assert_eq!(observation.requested_spks, 20);
    }
}
