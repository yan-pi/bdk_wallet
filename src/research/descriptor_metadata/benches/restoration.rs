use std::{sync::Arc, time::Duration};

use bdk_wallet::{
    bitcoin::{absolute, transaction, Amount, OutPoint, Transaction, TxOut, Txid},
    chain::tx_graph::TxUpdate,
    chain::{keychain_txout::KeychainTxOutIndex, Indexer},
    KeychainKind, SpkMetadata, Update, Wallet,
};
use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, PlottingBackend,
    SamplingMode, Throughput,
};
use descriptor_metadata_research::{
    apply_metadata, consume_descriptor_stop_gap, consume_sync_request,
    descriptor_full_scan_request, fixture_catalog, metadata_revealed_request,
    metadata_sparse_request_from_metadata, new_research_wallet, Fixture, FixtureId,
    PARALLEL_REQUESTS,
};

fn fixture(id: FixtureId) -> Fixture {
    fixture_catalog()
        .into_iter()
        .find(|fixture| fixture.id == id)
        .expect("canonical fixture must exist")
}

fn benchmark_descriptor_stop_gap(c: &mut Criterion) {
    let mut group = c.benchmark_group("descriptor_stop_gap");
    group
        .sample_size(30)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));

    for (id, stop_gap, expected_spks) in [
        (FixtureId::Empty, 20, 20),
        (FixtureId::Sparse1_20_50, 50, 110),
        (FixtureId::Revealed100Sparse, 50, 110),
        (FixtureId::Dense100, 5, 110),
        (FixtureId::Periodic10_1000, 20, 1_030),
    ] {
        let fixture = fixture(id);
        let oracle = fixture.history_oracle();
        group.throughput(Throughput::Elements(expected_spks));
        group.bench_with_input(
            BenchmarkId::new(id.as_str(), format!("gap_{stop_gap}")),
            &stop_gap,
            |bencher, &stop_gap| {
                bencher.iter_batched(
                    || {
                        let wallet = new_research_wallet();
                        descriptor_full_scan_request(&wallet)
                    },
                    |request| {
                        black_box(consume_descriptor_stop_gap(
                            request,
                            &oracle,
                            stop_gap,
                            PARALLEL_REQUESTS,
                        ))
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

fn benchmark_metadata_revealed(c: &mut Criterion) {
    let fixtures = [
        FixtureId::Empty,
        FixtureId::Sparse1_20_50,
        FixtureId::Revealed100Sparse,
        FixtureId::Dense100,
        FixtureId::Periodic10_1000,
    ];

    let mut apply_group = c.benchmark_group("metadata_revealed_apply");
    apply_group
        .sample_size(30)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));
    for id in fixtures {
        let fixture = fixture(id);
        let metadata = fixture
            .metadata()
            .expect("canonical metadata must be valid");
        apply_group.bench_function(id.as_str(), |bencher| {
            bencher.iter_batched(
                new_research_wallet,
                |mut wallet| {
                    apply_metadata(&mut wallet, &metadata);
                    black_box(wallet.derivation_index(KeychainKind::External))
                },
                BatchSize::SmallInput,
            );
        });
    }
    apply_group.finish();

    let mut request_group = c.benchmark_group("metadata_revealed_request");
    request_group
        .sample_size(30)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));
    for id in fixtures {
        let fixture = fixture(id);
        let metadata = fixture
            .metadata()
            .expect("canonical metadata must be valid");
        request_group.bench_function(id.as_str(), |bencher| {
            bencher.iter_batched(
                || {
                    let mut wallet = new_research_wallet();
                    apply_metadata(&mut wallet, &metadata);
                    wallet
                },
                |wallet| black_box(consume_sync_request(metadata_revealed_request(&wallet))),
                BatchSize::SmallInput,
            );
        });
    }
    request_group.finish();
}

fn benchmark_metadata_sparse_restore(c: &mut Criterion) {
    let mut group = c.benchmark_group("metadata_sparse_restore");
    group
        .sample_size(30)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));

    for id in [
        FixtureId::Empty,
        FixtureId::Sparse1_20_50,
        FixtureId::Revealed100Sparse,
        FixtureId::Dense100,
        FixtureId::Periodic10_1000,
        FixtureId::HighGap50000,
        FixtureId::Revealed50000Unused,
    ] {
        let fixture = fixture(id);
        let metadata = fixture
            .metadata()
            .expect("canonical metadata must be valid");
        group.bench_function(id.as_str(), |bencher| {
            bencher.iter_batched(
                new_research_wallet,
                |mut wallet| {
                    let request = metadata_sparse_request_from_metadata(&mut wallet, &metadata)
                        .expect("canonical metadata must restore");
                    black_box(consume_sync_request(request))
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn benchmark_encoding(c: &mut Criterion) {
    let fixtures = [
        FixtureId::Sparse1_20_50,
        FixtureId::Revealed100Sparse,
        FixtureId::Dense100,
        FixtureId::Periodic10_1000,
        FixtureId::HighGap50000,
        FixtureId::Revealed50000Unused,
    ];

    let mut encode_group = c.benchmark_group("metadata_encode_base64");
    encode_group
        .sample_size(50)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));
    for id in fixtures {
        let metadata = fixture(id)
            .metadata()
            .expect("canonical metadata must be valid");
        encode_group.bench_function(id.as_str(), |bencher| {
            bencher.iter(|| {
                black_box(
                    metadata
                        .encode_base64()
                        .expect("base64 encoding must succeed"),
                )
            });
        });
    }
    encode_group.finish();

    let mut decode_group = c.benchmark_group("metadata_decode_base64");
    decode_group
        .sample_size(50)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));
    for id in fixtures {
        let encoded = fixture(id)
            .metadata()
            .expect("canonical metadata must be valid")
            .encode_base64()
            .expect("base64 encoding must succeed")
            .expect("non-empty fixture must encode");
        decode_group.bench_function(id.as_str(), |bencher| {
            bencher.iter(|| {
                black_box(
                    SpkMetadata::decode_base64(black_box(&encoded), KeychainKind::External)
                        .expect("base64 decoding must succeed"),
                )
            });
        });
    }
    decode_group.finish();
}

fn sparse_update_input(fixture: &Fixture) -> (Wallet, Update, Txid) {
    let mut wallet = new_research_wallet();
    let metadata = fixture
        .metadata()
        .expect("canonical metadata must be valid");
    let index = fixture
        .used_indexes()
        .last()
        .copied()
        .expect("update fixture must have a used index");
    let script_pubkey = wallet
        .peek_address(KeychainKind::External, index)
        .address
        .script_pubkey();
    wallet
        .apply_spk_metadata_sparse(&metadata)
        .expect("canonical metadata must restore");
    let tx = Arc::new(Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: Vec::new(),
        output: vec![TxOut {
            value: Amount::from_sat(50_000),
            script_pubkey,
        }],
    });
    let txid = tx.compute_txid();
    let mut tx_update = TxUpdate::default();
    tx_update.txs.push(tx);
    tx_update.seen_ats.insert((txid, 1));

    (
        wallet,
        Update {
            tx_update,
            ..Default::default()
        },
        txid,
    )
}

fn benchmark_sparse_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("metadata_sparse_update");
    group
        .sample_size(30)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));

    for id in [FixtureId::Sparse1_20_50, FixtureId::HighGap50000] {
        let fixture = fixture(id);
        group.bench_function(id.as_str(), |bencher| {
            bencher.iter_batched(
                || sparse_update_input(&fixture),
                |(mut wallet, update, txid)| {
                    wallet
                        .apply_update(update)
                        .expect("sparse update must apply");
                    black_box(wallet.get_tx(txid).is_some());
                    black_box(
                        wallet
                            .list_unspent()
                            .any(|output| output.outpoint == OutPoint::new(txid, 0)),
                    );
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn benchmark_sparse_changeset_reload(c: &mut Criterion) {
    let mut group = c.benchmark_group("metadata_sparse_changeset_reload");
    group
        .sample_size(30)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));

    for id in [FixtureId::Sparse1_20_50, FixtureId::HighGap50000] {
        let fixture = fixture(id);
        let wallet = new_research_wallet();
        let descriptor = wallet.public_descriptor(KeychainKind::External).clone();
        let mut index = KeychainTxOutIndex::new(25, false);
        index
            .insert_descriptor(KeychainKind::External, descriptor.clone())
            .expect("descriptor must insert");
        let _ = index
            .restore_keychain_state(
                KeychainKind::External,
                fixture.last_revealed(),
                fixture.used_indexes().iter().copied(),
            )
            .expect("canonical state must restore");
        let changeset = index.initial_changeset();

        group.bench_function(id.as_str(), |bencher| {
            bencher.iter_batched(
                || (changeset.clone(), descriptor.clone()),
                |(changeset, descriptor)| {
                    let mut restored = KeychainTxOutIndex::from_changeset(25, false, changeset);
                    restored
                        .insert_descriptor(KeychainKind::External, descriptor)
                        .expect("descriptor must insert after changeset");
                    black_box(restored.inner().all_spks().len());
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn benchmark_high_frontier(c: &mut Criterion) {
    let fixture = fixture(FixtureId::HighGap50000);
    let metadata = fixture
        .metadata()
        .expect("canonical metadata must be valid");
    let oracle = fixture.history_oracle();
    let mut group = c.benchmark_group("high_frontier");
    group
        .sample_size(10)
        .sampling_mode(SamplingMode::Flat)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(20));

    group.throughput(Throughput::Elements(100_010));
    group.bench_function("descriptor_stop_gap", |bencher| {
        bencher.iter_batched(
            || {
                let wallet = new_research_wallet();
                descriptor_full_scan_request(&wallet)
            },
            |request| {
                black_box(consume_descriptor_stop_gap(
                    request,
                    &oracle,
                    50_000,
                    PARALLEL_REQUESTS,
                ))
            },
            BatchSize::LargeInput,
        );
    });

    group.throughput(Throughput::Elements(50_001));
    group.bench_function("metadata_revealed_apply", |bencher| {
        bencher.iter_batched(
            new_research_wallet,
            |mut wallet| {
                apply_metadata(&mut wallet, &metadata);
                black_box(wallet.derivation_index(KeychainKind::External))
            },
            BatchSize::LargeInput,
        );
    });

    group.throughput(Throughput::Elements(50_001));
    group.bench_function("metadata_revealed_request", |bencher| {
        bencher.iter_batched(
            || {
                let mut wallet = new_research_wallet();
                apply_metadata(&mut wallet, &metadata);
                wallet
            },
            |wallet| black_box(consume_sync_request(metadata_revealed_request(&wallet))),
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().plotting_backend(PlottingBackend::Plotters);
    targets =
        benchmark_descriptor_stop_gap,
        benchmark_metadata_revealed,
        benchmark_metadata_sparse_restore,
        benchmark_encoding,
        benchmark_sparse_update,
        benchmark_sparse_changeset_reload,
        benchmark_high_frontier
}
criterion_main!(benches);
