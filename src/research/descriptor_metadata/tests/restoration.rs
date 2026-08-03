use descriptor_metadata_research::{
    descriptor_stop_gap, fixture_catalog, metadata_revealed, metadata_sparse_restore, FixtureId,
    PARALLEL_REQUESTS,
};

fn fixture(id: FixtureId) -> descriptor_metadata_research::Fixture {
    fixture_catalog()
        .into_iter()
        .find(|fixture| fixture.id == id)
        .expect("canonical fixture must exist")
}

#[test]
fn sparse_source_full_scan_requires_110_spks() {
    let observation =
        descriptor_stop_gap(&fixture(FixtureId::Sparse1_20_50), 50, PARALLEL_REQUESTS);

    assert!(observation.complete);
    assert_eq!(observation.requested_spks, 110);
    assert_eq!(observation.discovered_indexes, vec![1, 20, 50]);
}

#[test]
fn sparse_source_current_metadata_requests_revealed_range() {
    let observation = metadata_revealed(&fixture(FixtureId::Sparse1_20_50))
        .expect("canonical metadata must be valid");

    assert_eq!(observation.requested_spks, 51);
    assert_eq!(observation.restored_last_revealed, Some(50));
    assert!(observation.used_markers_restored);
}

#[test]
fn sparse_source_restore_requests_only_used_indexes() {
    let observation = metadata_sparse_restore(&fixture(FixtureId::Sparse1_20_50))
        .expect("canonical metadata must be valid");

    assert_eq!(observation.requested_spks, 3);
    assert_eq!(observation.requested_indexes, vec![1, 20, 50]);
}

#[test]
fn every_sparse_restore_requests_exact_fixture_indexes() {
    for fixture in fixture_catalog() {
        let observation =
            metadata_sparse_restore(&fixture).expect("canonical metadata must be valid");

        assert_eq!(observation.requested_indexes, fixture.used_indexes());
    }
}

#[test]
fn revealed_frontier_is_independent_from_used_indexes() {
    let fixture = fixture(FixtureId::Revealed100Sparse);

    assert_eq!(
        metadata_revealed(&fixture)
            .expect("canonical metadata must be valid")
            .requested_spks,
        101
    );
    assert_eq!(
        metadata_sparse_restore(&fixture)
            .expect("canonical metadata must be valid")
            .requested_spks,
        3
    );
}

#[test]
fn insufficient_stop_gap_remains_an_incomplete_result() {
    let observation = descriptor_stop_gap(&fixture(FixtureId::HighGap50000), 20, PARALLEL_REQUESTS);

    assert!(!observation.complete);
    assert_eq!(observation.requested_spks, 30);
    assert_eq!(observation.discovered_indexes, vec![0]);
}

#[test]
#[ignore = "derives contiguous high-frontier descriptor SPKs; run in release mode"]
fn high_gap_paths_have_documented_boundaries() {
    let fixture = fixture(FixtureId::HighGap50000);
    let full = descriptor_stop_gap(&fixture, 50_000, PARALLEL_REQUESTS);

    assert!(full.complete);
    assert_eq!(full.requested_spks, 100_010);
    assert_eq!(full.discovered_indexes, vec![0, 50_000]);
    assert_eq!(
        metadata_revealed(&fixture)
            .expect("canonical metadata must be valid")
            .requested_spks,
        50_001
    );
    assert_eq!(
        metadata_sparse_restore(&fixture)
            .expect("canonical metadata must be valid")
            .requested_spks,
        2
    );
}

#[test]
fn dense_usage_converges_to_the_revealed_range() {
    let fixture = fixture(FixtureId::Dense100);
    let full = descriptor_stop_gap(&fixture, 5, PARALLEL_REQUESTS);
    let current = metadata_revealed(&fixture).expect("canonical metadata must be valid");
    let target = metadata_sparse_restore(&fixture).expect("canonical metadata must be valid");

    assert!(full.complete);
    assert_eq!(full.requested_spks, 110);
    assert_eq!(current.requested_spks, 101);
    assert_eq!(target.requested_spks, 101);
}

#[test]
fn base64_round_trip_preserves_every_canonical_snapshot() {
    for fixture in fixture_catalog() {
        let metadata = fixture.metadata().expect("fixture metadata must be valid");
        let encoded = metadata.encode_base64().expect("encoding must succeed");

        if metadata.is_empty() {
            assert!(encoded.is_none());
            continue;
        }

        let decoded = bdk_wallet::SpkMetadata::decode_base64(
            &encoded.expect("non-empty metadata must encode"),
            bdk_wallet::KeychainKind::External,
        )
        .expect("decoding must succeed");
        assert_eq!(decoded.last_revealed(), metadata.last_revealed());
        assert_eq!(decoded.used_indexes(), metadata.used_indexes());
    }
}

#[test]
fn empty_snapshot_requests_no_metadata_spks() {
    let fixture = fixture(FixtureId::Empty);

    assert_eq!(
        metadata_revealed(&fixture)
            .expect("canonical metadata must be valid")
            .requested_spks,
        0
    );
    assert_eq!(
        metadata_sparse_restore(&fixture)
            .expect("canonical metadata must be valid")
            .requested_spks,
        0
    );
}
