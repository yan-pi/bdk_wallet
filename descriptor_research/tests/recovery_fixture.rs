#[allow(dead_code)]
#[path = "../benches/descriptor_metadata/fixture.rs"]
mod fixture;

#[path = "../benches/descriptor_metadata/history.rs"]
mod history;

#[allow(dead_code)]
#[path = "../benches/descriptor_metadata/recovery.rs"]
mod recovery;

use fixture::{recovery_cases, GapProfile, RecoveryCase};

#[test]
fn profiles_keep_same_count_but_have_different_gaps() {
    let bounded = RecoveryCase::new(10, GapProfile::Bounded, 30, 20, 10)
        .expect("bounded fixture is feasible");
    let unconstrained = RecoveryCase::new(10, GapProfile::Unconstrained, 30, 20, 10)
        .expect("unconstrained fixture is feasible");

    assert_eq!(bounded.used_count(), 4);
    assert_eq!(unconstrained.used_count(), 4);
    assert_eq!(bounded.metadata.used_indexes(), &[0, 10, 20, 30]);
    assert_eq!(unconstrained.metadata.used_indexes(), &[0, 1, 2, 30]);
    assert!(bounded.maximum_internal_empty_gap() < bounded.stop_gap);
    assert!(unconstrained.maximum_internal_empty_gap() >= unconstrained.stop_gap);
}

#[test]
fn bounded_rejects_infeasible_low_density_cases() {
    for density in [1, 5] {
        assert!(RecoveryCase::new(density, GapProfile::Bounded, 100_000, 20, 10).is_none());
        assert!(RecoveryCase::new(density, GapProfile::Unconstrained, 100_000, 20, 10).is_some());
    }
}

#[test]
fn matrix_has_expected_density_points() {
    let densities = |profile| {
        recovery_cases(profile)
            .into_iter()
            .map(|case| case.density_percent)
            .collect::<Vec<_>>()
    };

    assert_eq!(densities(GapProfile::Bounded), [10, 25, 50, 75, 100]);
    assert_eq!(
        densities(GapProfile::Unconstrained),
        [1, 5, 10, 25, 50, 75, 100]
    );
}

#[test]
fn full_density_uses_every_index_in_both_profiles() {
    let expected = (0..=100_000).collect::<Vec<_>>();

    for profile in [GapProfile::Bounded, GapProfile::Unconstrained] {
        let case = RecoveryCase::new(100, profile, 100_000, 20, 10)
            .expect("full density fixture is feasible");
        assert_eq!(case.used_count(), 100_001);
        assert_eq!(case.metadata.used_indexes(), expected.as_slice());
    }
}

#[test]
fn stop_gap_resets_after_activity_and_stops_at_threshold() {
    let complete = fixture::scan_stop_gap(6, &[0, 3], 3);
    assert_eq!(complete.scripts_examined, 7);
    assert_eq!(complete.discovered_indexes, [0, 3]);
    assert!(complete.complete);

    let partial = fixture::scan_stop_gap(6, &[0, 4], 3);
    assert_eq!(partial.scripts_examined, 4);
    assert_eq!(partial.discovered_indexes, [0]);
    assert!(!partial.complete);
}

#[test]
fn history_has_one_output_per_used_script() {
    let case = RecoveryCase::new(50, GapProfile::Bounded, 20, 2, 3).expect("fixture is feasible");
    let descriptor = fixture::descriptor();
    let history = history::SyntheticHistory::new(&case, &descriptor);

    assert_eq!(history.used_count(), case.used_count());

    for &index in case.metadata.used_indexes() {
        let script = descriptor
            .at_derivation_index(index)
            .expect("index must derive")
            .script_pubkey();
        let activity = history
            .activity(&script)
            .expect("used script must have activity");

        assert_eq!(activity.index, index);
        assert_eq!(activity.txout.script_pubkey, script);
    }
}

#[test]
fn metadata_is_complete_while_stop_gap_depends_on_distribution() {
    let descriptor = fixture::descriptor();

    for profile in [GapProfile::Bounded, GapProfile::Unconstrained] {
        let case = RecoveryCase::new(50, profile, 20, 2, 3).expect("fixture is feasible");
        let history = history::SyntheticHistory::new(&case, &descriptor);
        let (metadata, stop_gap) = recovery::validate_case(&case, &descriptor, &history);

        assert!(metadata.complete);
        assert_eq!(stop_gap.complete, profile == GapProfile::Bounded);
    }
}

#[test]
fn full_density_keeps_counts_distinct() {
    let case =
        RecoveryCase::new(100, GapProfile::Bounded, 30, 20, 10).expect("fixture is feasible");
    let descriptor = fixture::descriptor();
    let history = history::SyntheticHistory::new(&case, &descriptor);
    let (metadata, stop_gap) = recovery::validate_case(&case, &descriptor, &history);

    assert_eq!(case.used_count(), 31);
    assert_eq!(metadata.scripts_examined, 31);
    assert_eq!(metadata.materialized_spks, 31);
    assert_eq!(stop_gap.scripts_examined, 41);
    assert_eq!(stop_gap.materialized_spks, 51);
    assert!(metadata.complete);
    assert!(stop_gap.complete);
}
