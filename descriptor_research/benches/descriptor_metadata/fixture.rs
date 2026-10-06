use std::collections::BTreeSet;

use bdk_chain::{
    bitcoin::secp256k1::Secp256k1,
    keychain_txout::KeychainTxOutIndex,
    miniscript::{Descriptor, DescriptorPublicKey},
};
use bdk_wallet::{KeychainKind, SpkMetadata};

const BENCH_DESCRIPTOR: &str = "tr([ab28dc00/86h/1h/0h]tpubDCdDtzAMZZrkwKBxwNcGCqe4FRydeD9rfMisoi7qLdraG79YohRfPW4YgdKQhpgASdvh612xXNY5xYzoqnyCgPbkpK4LSVcH5Xv4cK7johH/0/*)";

const RESTORE_REVEALED_COUNT: [u32; 6] = [
    100,
    500,
    1_000,
    5_000,
    10_000,
    50_000,
]

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
    pub fn new(
        last_revealed: u32,
        used_indexes: Vec<u32>,
        lookahead: u32,
        stop_gap: u32,
    ) -> Self {
        let metadata = SpkMetadata::with_last_revealed(
            KeychainKind::External,
            Some(last_revealed),
            used_indexes,
        )
        .expect("benchmark metadata must be valid");

        Self {
            name,
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


    pub fn used_count(&self) -> usize {
        self.metadata.used_indexes().len()
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

pub restore_cases() -> Vec<BenchCase> {
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
        .expect("benchmark new index must insert descriptor");

    index
}
