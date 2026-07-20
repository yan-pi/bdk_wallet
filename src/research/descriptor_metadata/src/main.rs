use std::{collections::BTreeSet, fs, path::Path, time::Instant};

use bdk_wallet::{
    bitcoin::{Network, ScriptBuf},
    chain::spk_client::SyncRequest,
    test_utils::get_test_wpkh_and_change_desc,
    KeychainKind, SpkMetadata, Wallet,
};

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
    full_scan_requested_spks: usize,
    full_scan_discovered_count: usize,
    full_scan_last_active_index: Option<u32>,
    revealed_sync_spks: usize,
    sparse_sync_spks: usize,
    density: f64,
    full_scan_elapsed_ns: u128,
    revealed_sync_elapsed_ns: u128,
    sparse_sync_elapsed_ns: u128,
    base64_encode_elapsed_ns: u128,
    bech32_encode_elapsed_ns: u128,
    base64_len: usize,
    bech32_len: Option<usize>,
    bech32_error: Option<String>,
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scenarios = scenarios();
    let stop_gaps = [5, 20, 50, 100, 1000];
    let parallel_requests = [1, 5, 10, 25];

    let output_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("output");
    fs::create_dir_all(&output_dir)?;

    let mut writer = csv::Writer::from_path(output_dir.join("results.csv"))?;

    for scenario in &scenarios {
        let keychain = KeychainKind::External;
        let wallet = research_wallet();
        let metadata = SpkMetadata::new(keychain, scenario.indexes.clone());
        let active_spks = active_spks_from_indexes(&wallet, keychain, &scenario.indexes);
        let (revealed_sync_spks, revealed_sync_elapsed_ns) =
            measure_ns(|| revealed_sync_spk_count(keychain, &scenario.indexes));
        let (sparse_sync_spks, sparse_sync_elapsed_ns) =
            measure_ns(|| sparse_sync_spk_count(&wallet, &metadata));

        let (base64_result, base64_encode_elapsed_ns) = measure_ns(|| metadata.encode_base64());
        let base64_len = base64_result?
            .map(|encoded| encoded.len())
            .unwrap_or_default();

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
            bech32_encode_elapsed_ns,
            bech32_error: &bech32_error,
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

fn scenarios() -> Vec<Scenario> {
    vec![
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
        base64_encode_elapsed_ns: precomputed.base64_encode_elapsed_ns,
        bech32_len: precomputed.bech32_len,
        bech32_encode_elapsed_ns: precomputed.bech32_encode_elapsed_ns,
        bech32_error: precomputed.bech32_error.clone(),
    }
}

fn density(indexes: &[u32]) -> f64 {
    let Some(max_index) = indexes.iter().copied().max() else {
        return 0.0;
    };

    indexes.len() as f64 / (max_index as f64 + 1.0)
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

fn revealed_sync_spk_count(keychain: KeychainKind, indexes: &[u32]) -> usize {
    let mut wallet = research_wallet();
    let metadata = SpkMetadata::new(keychain, indexes.to_vec());

    wallet.apply_spk_metadata(&metadata);

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
