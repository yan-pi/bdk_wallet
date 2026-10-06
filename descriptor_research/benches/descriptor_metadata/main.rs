use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};

mod fixture;
mod restore;

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(1))
        .sample_size(10);
    targets = restore::benchmarks
}

criterion_main!(benches);
