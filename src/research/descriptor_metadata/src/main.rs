use std::{collections::BTreeSet, fs, path::Path, time::Instant};

use bdk_wallet::{
    bitcoin::{Network, ScriptBuf},
    chain::spk_client::SyncRequest,
    test_utils::get_test_wpkh_and_change_desc,
    KeychainKind, SpkMetadata, Wallet,
};

const DEFAULT_PROBABILITY_SAMPLE_COUNT: u32 = 100;
const DEFAULT_PROBABILITY_MAX_INDEXES: &[u32] = &[1_000, 10_000];
const DEFAULT_PROBABILITIES: &[f64] = &[1.0, 0.5, 0.2, 0.1, 0.05, 0.01, 0.005, 0.001, 0.0001];
const PROBABILITY_STOP_GAP: usize = 20;
const PROBABILITY_PARALLEL_REQUESTS: usize = 10;

const FULL_SCAN_STRATEGY: &str = "full_scan_stop_gap";
const REVEALED_SYNC_STRATEGY: &str = "metadata_revealed_sync";
const SPARSE_SYNC_STRATEGY: &str = "sparse_metadata_sync";

#[derive(Debug)]
struct Scenario {
    name: &'static str,
    indexes: Vec<u32>,
}

#[derive(Debug)]
struct StopGapResult {
    requested_spks: usize,
    discovered_indexes: Vec<u32>,
    last_active_index: Option<u32>,
}

#[derive(Debug, serde::Serialize)]
struct ScenarioResult {
    scenario: String,
    stop_gap: usize,
    parallel_requests: usize,
    used_count: usize,
    max_index: Option<u32>,
    density: f64,

    full_scan_requested_spks: usize,
    full_scan_discovered_count: usize,
    full_scan_last_active_index: Option<u32>,
    full_scan_elapsed_ns: u128,

    revealed_sync_spks: usize,
    revealed_sync_elapsed_ns: u128,

    sparse_sync_spks: usize,
    sparse_sync_elapsed_ns: u128,

    base64_len: usize,
    bech32_len: Option<usize>,
    bech32_error: Option<String>,
    base64_encode_elapsed_ns: u128,
    bech32_encode_elapsed_ns: u128,
}

struct ScenarioPrecomputedMetrics<'a> {
    revealed_sync_spks: usize,
    revealed_sync_elapsed_ns: u128,

    sparse_sync_spks: usize,
    sparse_sync_elapsed_ns: u128,

    base64_len: usize,
    base64_encode_elapsed_ns: u128,

    bech32_len: Option<usize>,
    bech32_encode_elapsed_ns: u128,
    bech32_error: &'a Option<String>,
}

/// This is the one that goes into the probability_results.csv file. It contains the results of
/// running a single sample of a probabilistic load test.
#[derive(Debug, serde::Serialize)]
struct ProbabilityLoadResult {
    sample_id: u32,
    seed: u64,
    max_index: u32,
    p_used: f64,

    actual_used_count: usize,
    actual_density: f64,

    strategy: &'static str,
    stop_gap: usize,
    parallel_requests: usize,
    wallet_load_elapsed_ns: u128,
    requested_spks: usize,
    discovered_count: usize,
    complete: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("output");
    fs::create_dir_all(&output_dir)?;

    write_probability_results(&output_dir)?;

    if std::env::var("BDK_RESEARCH_WRITE_DETERMINISTIC").as_deref() == Ok("1") {
        write_deterministic_results(&output_dir)?;
    }

    Ok(())
}

fn write_deterministic_results(output_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let scenarios = scenarios();
    let stop_gaps = [5, 20, 50, 100, 1000];
    let parallel_requests = [1, 5, 10, 25];

    let mut writer = csv::Writer::from_path(output_dir.join("results.csv"))?;

    for scenario in &scenarios {
        let keychain = KeychainKind::External;
        let wallet = research_wallet();
        let metadata = SpkMetadata::new(keychain, scenario.indexes.clone());
        let active_spks = active_spks_from_indexes(&wallet, keychain, &scenario.indexes);

        let mut revealed_wallet = research_wallet();
        let (revealed_sync_spks, revealed_sync_elapsed_ns) =
            measure_ns(|| revealed_sync_spk_count(&mut revealed_wallet, &metadata));

        let (sparse_sync_spks, sparse_sync_elapsed_ns) =
            measure_ns(|| sparse_sync_spk_count(&wallet, &metadata));

        let (base64_len, base64_encode_elapsed_ns) = measure_ns(|| {
            metadata
                .encode_base64()
                .expect("base64 encoding should succeed")
                .map(|encoded| encoded.len())
                .unwrap_or_default()
        });

        let (bech32_result, bech32_encode_elapsed_ns) = measure_ns(|| metadata.encode_bech32());
        let (bech32_len, bech32_error) = match bech32_result {
            Ok(Some(encoded)) => (Some(encoded.len()), None),
            Ok(None) => (None, None),
            Err(err) => (None, Some(format!("{err:?}"))),
        };

        let precomputed = ScenarioPrecomputedMetrics {
            revealed_sync_spks,
            revealed_sync_elapsed_ns,
            sparse_sync_spks,
            sparse_sync_elapsed_ns,
            base64_len,
            base64_encode_elapsed_ns,
            bech32_len,
            bech32_error: &bech32_error,
            bech32_encode_elapsed_ns,
        };

        for &stop_gap in &stop_gaps {
            for &parallel_request in &parallel_requests {
                writer.serialize(run_scenario(
                    scenario,
                    &wallet,
                    &active_spks,
                    stop_gap,
                    parallel_request,
                    &precomputed,
                ))?;
            }
        }
    }

    writer.flush()?;
    println!("wrote {}", output_dir.join("results.csv").display());

    Ok(())
}

fn write_probability_results(output_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let sample_count = probability_sample_count();
    let max_indexes = probability_max_indexes();
    let probabilities = probability_values();

    let mut writer = csv::Writer::from_path(output_dir.join("probability_results.csv"))?;

    for max_index in max_indexes {
        for &p_used in &probabilities {
            for sample_id in 0..sample_count {
                for result in run_probability_load_sample(
                    max_index,
                    p_used,
                    sample_id,
                    PROBABILITY_STOP_GAP,
                    PROBABILITY_PARALLEL_REQUESTS,
                ) {
                    writer.serialize(result)?;
                }
            }
        }
    }

    writer.flush()?;
    println!(
        "wrote {}",
        output_dir.join("probability_results.csv").display()
    );

    Ok(())
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "empty",
            indexes: vec![],
        },
        Scenario {
            name: "dense_contiguous_10",
            indexes: (0..=10).collect(),
        },
        Scenario {
            name: "dense_contiguous_100",
            indexes: (0..=100).collect(),
        },
        Scenario {
            name: "dense_contiguous_1000",
            indexes: (0..=1000).collect(),
        },
        Scenario {
            name: "periodic_10_1000",
            indexes: (0..=1000).step_by(10).collect(),
        },
        Scenario {
            name: "periodic_100_10000",
            indexes: (0..=10_000).step_by(100).collect(),
        },
        Scenario {
            name: "sparse_small",
            indexes: vec![0, 20, 50],
        },
        Scenario {
            name: "sparse_large",
            indexes: vec![0, 20, 50, 100, 1000],
        },
        Scenario {
            name: "pathological_high",
            indexes: vec![0, 50_000],
        },
    ]
}

fn run_scenario(
    scenario: &Scenario,
    wallet: &Wallet,
    active_spks: &BTreeSet<ScriptBuf>,
    stop_gap: usize,
    parallel_requests: usize,
    precomputed: &ScenarioPrecomputedMetrics<'_>,
) -> ScenarioResult {
    let keychain = KeychainKind::External;
    let (full_scan, full_scan_elapsed_ns) = measure_ns(|| {
        run_esplora_stop_gap(wallet, keychain, active_spks, stop_gap, parallel_requests)
    });

    ScenarioResult {
        scenario: scenario.name.to_string(),
        stop_gap,
        parallel_requests,
        used_count: scenario.indexes.len(),
        max_index: scenario.indexes.iter().copied().max(),
        density: density(&scenario.indexes),

        full_scan_requested_spks: full_scan.requested_spks,
        full_scan_discovered_count: full_scan.discovered_indexes.len(),
        full_scan_last_active_index: full_scan.last_active_index,
        full_scan_elapsed_ns,

        revealed_sync_spks: precomputed.revealed_sync_spks,
        revealed_sync_elapsed_ns: precomputed.revealed_sync_elapsed_ns,

        sparse_sync_spks: precomputed.sparse_sync_spks,
        sparse_sync_elapsed_ns: precomputed.sparse_sync_elapsed_ns,

        base64_len: precomputed.base64_len,
        bech32_len: precomputed.bech32_len,
        bech32_error: precomputed.bech32_error.clone(),
        base64_encode_elapsed_ns: precomputed.base64_encode_elapsed_ns,
        bech32_encode_elapsed_ns: precomputed.bech32_encode_elapsed_ns,
    }
}

fn density(indexes: &[u32]) -> f64 {
    let Some(max_index) = indexes.iter().copied().max() else {
        return 0.0;
    };

    indexes.len() as f64 / (max_index as f64 + 1.0)
}

fn probability_sample_count() -> u32 {
    std::env::var("BDK_RESEARCH_SAMPLES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|sample_count| *sample_count > 0)
        .unwrap_or(DEFAULT_PROBABILITY_SAMPLE_COUNT)
}

fn probability_max_indexes() -> Vec<u32> {
    parse_u32_list_env("BDK_RESEARCH_MAX_INDEXES")
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| DEFAULT_PROBABILITY_MAX_INDEXES.to_vec())
}

fn probability_values() -> Vec<f64> {
    parse_f64_list_env("BDK_RESEARCH_PROBABILITIES")
        .map(|values| {
            values
                .into_iter()
                .filter(|value| value.is_finite() && *value >= 0.0 && *value <= 1.0)
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| DEFAULT_PROBABILITIES.to_vec())
}

fn parse_u32_list_env(name: &str) -> Option<Vec<u32>> {
    std::env::var(name).ok().map(|raw| {
        raw.split(',')
            .filter_map(|value| value.trim().parse::<u32>().ok())
            .collect()
    })
}

fn parse_f64_list_env(name: &str) -> Option<Vec<f64>> {
    std::env::var(name).ok().map(|raw| {
        raw.split(',')
            .filter_map(|value| value.trim().parse::<f64>().ok())
            .collect()
    })
}

fn run_probability_load_sample(
    max_index: u32,
    p_used: f64,
    sample_id: u32,
    stop_gap: usize,
    parallel_requests: usize,
) -> Vec<ProbabilityLoadResult> {
    let seed = probability_seed(max_index, p_used, sample_id);
    let indexes = probabilistic_indexes(max_index, p_used, seed);
    let keychain = KeychainKind::External;
    let fixture_wallet = research_wallet();
    let active_spks = active_spks_from_indexes(&fixture_wallet, keychain, &indexes);
    let actual_used_count = indexes.len();
    let actual_density = probabilistic_density(actual_used_count, max_index);

    let (full_scan, full_scan_elapsed_ns) = measure_ns(|| {
        let wallet = research_wallet();
        run_esplora_stop_gap(&wallet, keychain, &active_spks, stop_gap, parallel_requests)
    });

    let (revealed_sync_spks, revealed_sync_elapsed_ns) = measure_ns(|| {
        let mut wallet = research_wallet();
        let metadata = SpkMetadata::new(keychain, indexes.clone());
        revealed_sync_spk_count(&mut wallet, &metadata)
    });

    let (sparse_sync_spks, sparse_sync_elapsed_ns) = measure_ns(|| {
        let wallet = research_wallet();
        let metadata = SpkMetadata::new(keychain, indexes.clone());
        sparse_sync_spk_count(&wallet, &metadata)
    });

    vec![
        ProbabilityLoadResult {
            sample_id,
            seed,
            max_index,
            p_used,
            actual_used_count,
            actual_density,
            strategy: FULL_SCAN_STRATEGY,
            stop_gap,
            parallel_requests,
            wallet_load_elapsed_ns: full_scan_elapsed_ns,
            requested_spks: full_scan.requested_spks,
            discovered_count: full_scan.discovered_indexes.len(),
            complete: full_scan.discovered_indexes.len() == actual_used_count,
        },
        ProbabilityLoadResult {
            sample_id,
            seed,
            max_index,
            p_used,
            actual_used_count,
            actual_density,
            strategy: REVEALED_SYNC_STRATEGY,
            stop_gap,
            parallel_requests,
            wallet_load_elapsed_ns: revealed_sync_elapsed_ns,
            requested_spks: revealed_sync_spks,
            discovered_count: actual_used_count,
            complete: true,
        },
        ProbabilityLoadResult {
            sample_id,
            seed,
            max_index,
            p_used,
            actual_used_count,
            actual_density,
            strategy: SPARSE_SYNC_STRATEGY,
            stop_gap,
            parallel_requests,
            wallet_load_elapsed_ns: sparse_sync_elapsed_ns,
            requested_spks: sparse_sync_spks,
            discovered_count: actual_used_count,
            complete: true,
        },
    ]
}

fn probability_seed(max_index: u32, p_used: f64, sample_id: u32) -> u64 {
    mix64(
        u64::from(max_index)
            ^ p_used.to_bits().rotate_left(17)
            ^ u64::from(sample_id).wrapping_mul(0xD1B5_4A32_D192_ED03),
    )
}

fn probabilistic_indexes(max_index: u32, probability: f64, seed: u64) -> Vec<u32> {
    if !probability.is_finite() || probability <= 0.0 {
        return Vec::new();
    }

    if probability >= 1.0 {
        return (0..=max_index).collect();
    }

    (0..=max_index)
        .filter(|index| {
            let mixed = mix64(seed ^ u64::from(*index).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            unit_f64_from_u64(mixed) < probability
        })
        .collect()
}

fn probabilistic_density(used_count: usize, max_index: u32) -> f64 {
    used_count as f64 / (max_index as f64 + 1.0)
}

fn mix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn unit_f64_from_u64(value: u64) -> f64 {
    const SCALE: f64 = 1.0 / ((1u64 << 53) as f64);

    ((value >> 11) as f64) * SCALE
}

fn measure_ns<T>(f: impl FnOnce() -> T) -> (T, u128) {
    let start = Instant::now();
    let value = f();

    (value, start.elapsed().as_nanos())
}

fn research_wallet() -> Wallet {
    let (desc, change_desc) = get_test_wpkh_and_change_desc();

    Wallet::create(desc, change_desc)
        .network(Network::Regtest)
        .create_wallet_no_persist()
        .expect("research wallet descriptors should be valid")
}

fn active_spks_from_indexes(
    wallet: &Wallet,
    keychain: KeychainKind,
    indexes: &[u32],
) -> BTreeSet<ScriptBuf> {
    indexes
        .iter()
        .map(|index| {
            wallet
                .peek_address(keychain, *index)
                .address
                .script_pubkey()
        })
        .collect()
}

fn run_esplora_stop_gap(
    wallet: &Wallet,
    keychain: KeychainKind,
    active_spks: &BTreeSet<ScriptBuf>,
    stop_gap: usize,
    parallel_requests: usize,
) -> StopGapResult {
    let mut request = wallet.start_full_scan_at(0).build();
    let mut spks = request.iter_spks(keychain);
    let gap_limit = stop_gap.max(1);

    let mut requested_spks = 0usize;
    let mut consecutive_unused = 0usize;
    let mut discovered_indexes = Vec::new();
    let mut last_active_index = None;

    loop {
        let batch = spks.by_ref().take(parallel_requests).collect::<Vec<_>>();

        if batch.is_empty() {
            break;
        }

        requested_spks += batch.len();

        for (index, spk) in batch {
            if active_spks.contains(&spk) {
                consecutive_unused = 0;
                discovered_indexes.push(index);
                last_active_index = Some(index);
            } else {
                consecutive_unused = consecutive_unused.saturating_add(1);
            }
        }

        if consecutive_unused >= gap_limit {
            break;
        }
    }

    StopGapResult {
        requested_spks,
        discovered_indexes,
        last_active_index,
    }
}

fn revealed_sync_spk_count(wallet: &mut Wallet, metadata: &SpkMetadata) -> usize {
    wallet.apply_spk_metadata(metadata);

    wallet
        .start_sync_with_revealed_spks_at(0)
        .build()
        .progress()
        .total_spks()
}

fn sparse_sync_spk_count(wallet: &Wallet, metadata: &SpkMetadata) -> usize {
    let spks = metadata.used_indexes().iter().map(|index| {
        let address = wallet.peek_address(metadata.keychain(), *index);
        (
            (metadata.keychain(), *index),
            address.address.script_pubkey(),
        )
    });

    SyncRequest::<(KeychainKind, u32)>::builder_at(0)
        .spks_with_indexes(spks)
        .build()
        .progress()
        .total_spks()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probabilistic_indexes_are_deterministic_for_same_seed() {
        let first = probabilistic_indexes(100, 0.25, 42);
        let second = probabilistic_indexes(100, 0.25, 42);

        assert_eq!(first, second);
    }

    #[test]
    fn probabilistic_indexes_select_all_indexes_when_probability_is_one() {
        let indexes = probabilistic_indexes(3, 1.0, 42);

        assert_eq!(indexes, vec![0, 1, 2, 3]);
    }

    #[test]
    fn probabilistic_indexes_select_no_indexes_when_probability_is_zero() {
        let indexes = probabilistic_indexes(3, 0.0, 42);

        assert!(indexes.is_empty());
    }

    #[test]
    fn probabilistic_density_uses_configured_index_space() {
        let density = probabilistic_density(2, 9);

        assert_eq!(density, 0.2);
    }

    #[test]
    fn probability_seed_changes_with_sample_id() {
        let first = probability_seed(10_000, 0.01, 0);
        let second = probability_seed(10_000, 0.01, 1);

        assert_ne!(first, second);
    }
}
