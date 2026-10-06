use std::hint::black_box;

use bdk_chain::{
    bitcoin::ScriptBuf,
    keychain_txout::{ChangeSet, KeychainTxOutIndex},
    miniscript::{Descriptor, DescriptorPublicKey},
};
use bdk_wallet::KeychainKind;
use criterion::{measurement::WallTime, AxisScale, BatchSize, BenchmarkId, BenchmarkGroup, Criterion, PlotConfiguration};

use crate::fixture::{descriptor, new_index, restore_cases, BenchCase};

type Index = KeychainTxOutIndex<KeychainKind>;

type SparseOutput = (Index, ChangeSet);

type DenseOutput = (Index, Vec<(u32, ScriptBuf)>, ChangeSet);

fn do_sparse_restore(mut index: Index, case: &BenchCase) -> SparseOutput {
    let changeset = index
        .restore_keychain_state(
            case.metadata.keychain(),
            case.metadata.last_revealed(),
            case.metadata.used_indexes().iter().copied(),
        )
        .expect("sparse restore must succeed");

    (index, changeset)
}

fn do_dense_restore(mut index: Index, case: &BenchCase) -> DenseOutput {
    let (revealed_spks, changeset) = index
        .reveal_to_target(case.metadata.keychain(), case.last_revealed())
        .expect("dense restore must succeed");

    for &used_index in case.metadata.used_indexes() {
        let _ = index.mark_used(case.metadata.keychain(), used_index);
    }

    (index, revealed_spks, changeset)
}

fn validate_logical_state(index: &Index, case: &BenchCase){
    assert_eq!(index.last_revealed_index(case.metadata.keychain()), case.metadata.last_revealed(),)

    let know_used = index
        .known_used_keychain_indexes(case.metadata.keychain()),
        .collect::<Vec<_>>();

    assert_eq!(
        know_used.as_slice(),
        case.metadata.used_indexes(),
    );
}

pub fn sparse_restore(c: &mut Criterion) {
    let cases = [BenchCase::smoke(), BenchCase::pathological_high()];

    let mut group = c.benchmark_group("restore/sparse");

    for case in cases {
        let descriptor = descriptor();

        let validation_index = do_sparse_restore(new_index(&case, &descriptor), &case).0;

        assert_eq!(
            validation_index.inner().all_spks().len(),
            case.expected_sparse_materialized(),
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(case.id()),
            &case,
            move |bencher, case| {
                bencher.iter_batched(
                    || new_index(case, &descriptor),
                    |index| black_box(do_sparse_restore(index, case)),
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

pub fn dense_restore(c: &mut Criterion) {
    let cases = [BenchCase::smoke(), BenchCase::pathological_high()];

    let mut group = c.benchmark_group("restore/dense");

    for case in cases {
        let descriptor = descriptor();

        let (validation_index, revealed_spks, _changeset) =
            do_dense_restore(new_index(&case, &descriptor), &case);

        assert_eq!(revealed_spks.len(), case.revealed_count(),);

        assert_eq!(
            validation_index.inner().all_spks().len(),
            case.expected_dense_materialized(),
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(case.id()),
            &case,
            move |bencher, case| {
                bencher.iter_batched(
                    || new_index(case, &descriptor),
                    |index| black_box(do_dense_restore(index, case)),
                    BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}
