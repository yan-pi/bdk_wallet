use std::{env, time::Duration};

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, PlottingBackend,
};
use descriptor_metadata_research::{
    consume_sync_request,
    density::{benchmark_parameter, consume_sequential_stop_gap, DensityConfig},
    descriptor_full_scan_request, metadata_sparse_request_from_metadata, new_research_wallet,
};

fn env_value<T>(name: &str, default: T) -> T
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    env::var(name).map_or(default, |value| {
        value
            .parse::<T>()
            .unwrap_or_else(|error| panic!("invalid {name}={value}: {error}"))
    })
}

fn config() -> DensityConfig {
    DensityConfig::new(
        env_value("DENSITY_INDEX_COUNT", 5_000),
        env_value("DENSITY_STEP", 5),
        env_value("DENSITY_LAYOUTS", 5),
        env_value("DENSITY_STOP_GAP", 20),
        env_value("DENSITY_BASE_SEED", 0x5eed),
    )
    .expect("density sweep configuration must be valid")
}

fn benchmark_density(criterion: &mut Criterion) {
    let config = config();
    let mut group = criterion.benchmark_group("spk_derivation_by_density");
    group
        .sample_size(env_value("DENSITY_SAMPLE_SIZE", 20))
        .warm_up_time(Duration::from_millis(env_value("DENSITY_WARMUP_MS", 500)))
        .measurement_time(Duration::from_millis(env_value(
            "DENSITY_MEASUREMENT_MS",
            1_000,
        )));

    for fixture in config.fixtures() {
        let parameter = benchmark_parameter(&fixture);
        let metadata = fixture
            .metadata()
            .expect("exact-density metadata must be valid");

        group.bench_with_input(
            BenchmarkId::new("sparse_api", &parameter),
            &fixture,
            |bencher, _fixture| {
                bencher.iter_batched(
                    new_research_wallet,
                    |mut wallet| {
                        let request = metadata_sparse_request_from_metadata(&mut wallet, &metadata)
                            .expect("exact-density sparse state must restore");
                        black_box(consume_sync_request(request))
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("descriptor_stop_gap", parameter),
            &fixture,
            |bencher, fixture| {
                bencher.iter_batched(
                    || {
                        let wallet = new_research_wallet();
                        descriptor_full_scan_request(&wallet)
                    },
                    |request| {
                        black_box(consume_sequential_stop_gap(
                            request,
                            fixture.used_indexes(),
                            config.stop_gap(),
                        ))
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().plotting_backend(PlottingBackend::Plotters);
    targets = benchmark_density
}
criterion_main!(benches);
