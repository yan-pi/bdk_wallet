use std::collections::BTreeSet;

use bdk_chain::{
    bitcoin::secp256k1::Secp256k1,
    keychain_txout::KeychainTxOutIndex,
    miniscript::{Descriptor, DescriptorPublicKey},
};
use bdk_wallet::{KeychainKind, SpkMetadata};

const BENCH_DESCRIPTOR: &str = "tr([ab28dc00/86h/1h/0h]tpubDCdDtzAMZZrkwKBxwNcGCqe4FRydeD9rfMisoi7qLdraG79YohRfPW4YgdKQhpgASdvh612xXNY5xYzoqnyCgPbkpK4LSVcH5Xv4cK7johH/0/*)";

const RESTORE_LAST_REVEALED_VALUES: [u32; 6] = [100, 500, 1_000, 5_000, 10_000, 50_000];

const REVEAL_LOOKAHEAD: u32 = 20;
const RESTORE_STOP_GAP: u32 = 10;

#[derive(Clone, Debug)]
pub struct BenchCase {
    pub metadata: SpkMetadata,
    pub lookahead: u32,

    // just for the sake of comparission with the metadata approach
    pub stop_gap: u32,
}

impl BenchCase {
    pub fn new(last_revealed: u32, used_indexes: Vec<u32>, lookahead: u32, stop_gap: u32) -> Self {
        let metadata = SpkMetadata::with_last_revealed(
            KeychainKind::External,
            Some(last_revealed),
            used_indexes,
        )
        .expect("benchmark metadata must be valid");

        Self {
            metadata,
            lookahead,
            stop_gap,
        }
    }

    pub fn tail(last_revealed: u32) -> Self {
        Self::new(
            last_revealed,
            vec![0, last_revealed],
            REVEAL_LOOKAHEAD,
            RESTORE_STOP_GAP,
        )
    }

    pub fn last_revealed(&self) -> u32 {
        self.metadata
            .last_revealed()
            .expect("benchmark metadata must have a last_revealed")
    }

    pub fn revealed_count(&self) -> usize {
        self.last_revealed() as usize + 1
    }

    pub fn expected_sparse_materialized(&self) -> usize {
        let mut materialized_indexes = BTreeSet::new();

        for index in 0..self.lookahead {
            materialized_indexes.insert(index);
        }

        for &index in self.metadata.used_indexes() {
            materialized_indexes.insert(index);
        }

        materialized_indexes.len()
    }

    pub fn expected_dense_materialized(&self) -> usize {
        self.revealed_count() + self.lookahead as usize
    }
}

pub fn restore_cases() -> Vec<BenchCase> {
    RESTORE_LAST_REVEALED_VALUES
        .into_iter()
        .map(BenchCase::tail)
        .collect()
}

pub fn descriptor() -> Descriptor<DescriptorPublicKey> {
    Descriptor::<DescriptorPublicKey>::parse_descriptor(&Secp256k1::new(), BENCH_DESCRIPTOR)
        .expect("benchmark descriptor must parse")
        .0
}

pub fn new_index(
    case: &BenchCase,
    descriptor: &Descriptor<DescriptorPublicKey>,
) -> KeychainTxOutIndex<KeychainKind> {
    let mut index = KeychainTxOutIndex::new(case.lookahead, false);
    index
        .insert_descriptor(case.metadata.keychain(), descriptor.clone())
        .expect("benchmark descriptor must insert");
    index
}

pub const RECOVERY_DENSITIES: [u32; 7] = [1, 5, 10, 25, 50, 75, 100];
const RECOVERY_LAST_REVEALED: u32 = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GapProfile {
    Bounded,
    Unconstrained,
}

impl GapProfile {
    pub fn name(self) -> &'static str {
        match self {
            GapProfile::Bounded => "gap_bounded",
            GapProfile::Unconstrained => "gap_unconstrained",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecoveryCase {
    pub metadata: SpkMetadata,
    pub density_percent: u32,
    pub profile: GapProfile,
    pub lookahead: u32,
    pub stop_gap: u32,
}

impl RecoveryCase {
    pub fn new(
        density_percent: u32,
        profile: GapProfile,
        last_revealed: u32,
        lookahead: u32,
        stop_gap: u32,
    ) -> Option<Self> {
        if !(1..=100).contains(&density_percent) || stop_gap == 0 {
            return None;
        }

        let revealed_count = u64::from(last_revealed) + 1;
        let used_count = (revealed_count * u64::from(density_percent)).div_ceil(100);

        if used_count < 2 || used_count > revealed_count {
            return None;
        }

        let intervals = used_count - 1;
        let used_indexes = match profile {
            GapProfile::Bounded => {
                if intervals * u64::from(stop_gap) < u64::from(last_revealed) {
                    return None;
                }

                (0..used_count)
                    .map(|position| (position * u64::from(last_revealed) / intervals) as u32)
                    .collect()
            }
            GapProfile::Unconstrained => {
                let mut indexes = (0..intervals).map(|index| index as u32).collect::<Vec<_>>();
                indexes.push(last_revealed);
                indexes
            }
        };
        let metadata = SpkMetadata::with_last_revealed(
            KeychainKind::External,
            Some(last_revealed),
            used_indexes,
        )
        .expect("benchmark metadata must be valid");

        Some(Self {
            metadata,
            density_percent,
            profile,
            lookahead,
            stop_gap,
        })
    }

    pub fn last_revealed(&self) -> u32 {
        self.metadata
            .last_revealed()
            .expect("fixture must have a frontier")
    }

    pub fn revealed_count(&self) -> usize {
        self.last_revealed() as usize + 1
    }

    pub fn used_count(&self) -> usize {
        self.metadata.used_indexes().len()
    }

    pub fn effective_density_percent(&self) -> f64 {
        100.0 * self.used_count() as f64 / self.revealed_count() as f64
    }

    pub fn maximum_internal_empty_gap(&self) -> u32 {
        self.metadata
            .used_indexes()
            .windows(2)
            .map(|pair| pair[1] - pair[0] - 1)
            .max()
            .unwrap_or(0)
    }

    pub fn expected_sparse_materialized(&self) -> usize {
        (0..self.lookahead)
            .chain(self.metadata.used_indexes().iter().copied())
            .collect::<BTreeSet<_>>()
            .len()
    }
}

pub fn recovery_cases(profile: GapProfile) -> Vec<RecoveryCase> {
    RECOVERY_DENSITIES
        .into_iter()
        .filter_map(|density| {
            RecoveryCase::new(
                density,
                profile,
                RECOVERY_LAST_REVEALED,
                REVEAL_LOOKAHEAD,
                RESTORE_STOP_GAP,
            )
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub struct StopGapSummary {
    pub scripts_examined: usize,
    pub discovered_indexes: Vec<u32>,
    pub last_active_index: Option<u32>,
    pub complete: bool,
}

pub fn scan_stop_gap(last_revealed: u32, used_indexes: &[u32], stop_gap: u32) -> StopGapSummary {
    assert!(stop_gap > 0);
    assert!(used_indexes.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(used_indexes.iter().all(|index| *index <= last_revealed));

    let scan_end = last_revealed
        .checked_add(stop_gap)
        .expect("fixture scan range must fit");

    let mut discovered_indexes = Vec::new();
    let mut consecutive_empty = 0;
    let mut scripts_examined = 0;

    for index in 0..=scan_end {
        scripts_examined += 1;

        if used_indexes.binary_search(&index).is_ok() {
            discovered_indexes.push(index);
            consecutive_empty = 0;
        } else {
            consecutive_empty += 1;
            if consecutive_empty == stop_gap {
                break;
            }
        }
    }

    StopGapSummary {
        scripts_examined,
        last_active_index: discovered_indexes.last().copied(),
        complete: discovered_indexes.len() == used_indexes.len(),
        discovered_indexes,
    }
}
