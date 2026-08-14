use descriptor_metadata_research::density::{
    benchmark_parameter, consume_sequential_stop_gap, sequential_stop_gap_complete, DensityConfig,
    DensityConfigError,
};
use descriptor_metadata_research::{descriptor_full_scan_request, new_research_wallet};

fn config() -> DensityConfig {
    DensityConfig::new(1_000, 5, 5, 20, 0x5eed).expect("default exact-density config must be valid")
}

#[test]
fn exact_density_has_fixed_cardinality_and_frontier() {
    for fixture in config().fixtures() {
        assert_eq!(
            fixture.used_indexes().len(),
            fixture.index_count() as usize * fixture.density_percent() as usize / 100
        );
        assert!(fixture
            .used_indexes()
            .windows(2)
            .all(|pair| pair[0] < pair[1]));
        assert_eq!(fixture.last_revealed(), 999);
    }
}

#[test]
fn exact_density_is_reproducible_with_distinct_layouts() {
    let first = config().fixtures();
    let second = config().fixtures();

    assert_eq!(first, second);
    let fifty_percent = first
        .iter()
        .filter(|fixture| fixture.density_percent() == 50)
        .map(|fixture| fixture.used_indexes())
        .collect::<Vec<_>>();
    assert_eq!(fifty_percent.len(), 5);
    assert!(fifty_percent.windows(2).all(|pair| pair[0] != pair[1]));
}

#[test]
fn density_endpoints_use_one_layout() {
    let fixtures = config().fixtures();

    assert_eq!(fixtures.len(), 97);
    assert_eq!(
        fixtures
            .iter()
            .filter(|fixture| fixture.density_percent() == 0)
            .count(),
        1
    );
    assert_eq!(
        fixtures
            .iter()
            .filter(|fixture| fixture.density_percent() == 100)
            .count(),
        1
    );
}

#[test]
fn criterion_parameter_is_stable_and_parseable() {
    let fixture = config()
        .fixtures()
        .into_iter()
        .find(|fixture| fixture.density_percent() == 5 && fixture.layout() == 2)
        .expect("configured fixture must exist");

    assert_eq!(benchmark_parameter(&fixture), "density_005_layout_02");
}

#[test]
fn density_config_rejects_inexact_percentage_grid() {
    assert_eq!(
        DensityConfig::new(999, 5, 5, 20, 0x5eed),
        Err(DensityConfigError::IndexCountNotPercentageAligned(999))
    );
}

#[test]
fn sequential_stop_gap_stops_at_exact_miss_count() {
    let wallet = new_research_wallet();
    let observation =
        consume_sequential_stop_gap(descriptor_full_scan_request(&wallet), &[0, 50_000], 20);

    assert!(!observation.complete);
    assert_eq!(observation.derived_spks, 21);
    assert_eq!(observation.discovered_indexes, vec![0]);
}

#[test]
fn sequential_stop_gap_resets_after_used_index() {
    let wallet = new_research_wallet();
    let observation =
        consume_sequential_stop_gap(descriptor_full_scan_request(&wallet), &[0, 19, 38], 20);

    assert!(observation.complete);
    assert_eq!(observation.derived_spks, 59);
    assert_eq!(observation.discovered_indexes, vec![0, 19, 38]);
    assert!(sequential_stop_gap_complete(&[0, 19, 38], 20));
    assert!(!sequential_stop_gap_complete(&[0, 50_000], 20));
}
