# Descriptor Metadata Restoration Study

## Status

Accepted research protocol. This document is the canonical specification for the
research crate in this directory. Implementation names, fixtures, metrics, and scope
must remain aligned with it.

Phase 1 measured deterministic SPK work and exposed the gap between the current eager
metadata path and a request-only sparse target. Phase 2 replaces that model with sparse
state restoration in the real `KeychainTxOutIndex`, including changeset persistence and
`IndexedTxGraph` response recognition.

The architectural decision for Phase 2 is recorded in the companion `bdk` checkout:
[ADR-0004: Decouple Logical Keychain Frontier from SPK Materialization](../../../../bdk/docs/adr/0004_sparse_keychain_restoration.md).

## Research Question

Given a descriptor and an exact `SpkMetadata` snapshot exported by the user, how much
script-pubkey derivation and request work can be avoided during wallet restoration when
compared with descriptor-only stop-gap scanning?

The primary subject is `SpkMetadata`. Network, public backend latency, transaction
download, `apply_update`, and persistence are excluded from the first study so they do
not confound SPK work. Phase 2 adds correctness coverage for `apply_update`, changeset,
and SQLite round trips while keeping network and disk I/O outside timed regions.

## Snapshot Contract

The metadata describes the wallet at the instant it was exported:

```text
last_revealed = highest address index revealed at export time
used_indexes  = indexes with known activity at export time
```

The first study does not claim that an old snapshot discovers payments received after
export at previously unused indexes. Stale-backup recovery is a separate safety study.

## Compared Strategies

### `descriptor_stop_gap`

Current descriptor-only restoration behavior. It consumes a real BDK
`FullScanRequest` in batches until `stop_gap` consecutive SPKs have no history.
The harness models Esplora's post-batch evaluation: it processes every response in a
concurrent batch before checking the stop condition. It does not model Electrum's
mid-batch early return.

```text
input: descriptor
work: derive and inspect a contiguous sequence from index 0
success: every used index in the fixture was reached
```

An incomplete run remains an incomplete result. Its lower cost is never compared as a
successful restore.

### `metadata_revealed`

Current metadata API behavior:

```text
decode metadata
apply_spk_metadata
start_sync_with_revealed_spks_at
```

`apply_spk_metadata` restores `last_revealed` through
`reveal_addresses_to`. The resulting request contains `0..=last_revealed`, not only
`used_indexes`. This strategy is deterministic but not sparse.

### `metadata_sparse_restore`

Real sparse restoration behavior:

```text
decode metadata
apply_spk_metadata_sparse
persist logical frontier and known-used indexes
start_sync_with_spk_metadata_at
```

This path installs known-used SPKs in `KeychainTxOutIndex`, preserves the logical
frontier without materializing historical gaps, builds a bounded request, survives
persist/reload before sync, and recognizes returned outputs through `IndexedTxGraph`.

## Decision Record

### Context

The previous harness mixed index simulation, single-shot `Instant` timings, random
Bernoulli fixtures, eager sparse request construction, a lazy full-request builder, CSV
aggregation, and Python plots. Those measurements did not represent equivalent restore
operations and introduced selection and timing noise.

### Decision

1. Use deterministic wallet-state fixtures as the primary corpus.
2. Count exact SPK work before measuring time.
3. Consume a real `FullScanRequest`; do not time its lazy builder as completed work.
4. Keep eager metadata behavior and real sparse restoration as separate strategies.
5. Use Criterion for repeated local measurements with setup outside timed regions.
6. Keep correctness tests independent from performance measurements.
7. Publish tables from exact counts and Criterion reports; do not maintain CSV or
   Python plotting pipelines.

### Consequences

The study establishes request counts, local scaling, sparse changeset restoration, and
synthetic transaction-update equivalence. It cannot establish public backend latency,
end-to-end restore time, or stale-backup safety.

## Fixtures

All indexes are zero-based and use the external keychain with the same WPKH descriptor.

| ID | `last_revealed` | `used_indexes` | Purpose |
| --- | ---: | --- | --- |
| `empty` | none | `[]` | Empty snapshot |
| `sparse_1_20_50` | 50 | `[1,20,50]` | Source example |
| `revealed_100_sparse` | 100 | `[1,20,50]` | Separate revealed frontier from usage |
| `dense_100` | 100 | `[0..=100]` | Dense crossover |
| `periodic_10_1000` | 1000 | every tenth index | Bounded regular gaps |
| `high_gap_50000` | 50000 | `[0,50000]` | Pathological sparse frontier |
| `revealed_50000_unused` | 50000 | `[]` | Frontier cost without known activity |

Bernoulli occupancy is not part of the primary study.

## Hypotheses

```text
H1: decode(encode(metadata)) equals the original non-empty metadata; empty metadata
    encodes as no payload.
H2: successful descriptor_stop_gap work scales with the reached frontier plus
    terminal stop-gap and batch overfetch.
H3: metadata_sparse_restore requested SPKs equal used_count.
H4: metadata_revealed requested SPKs equal last_revealed + 1.
H5: actual-path local cost is dominated by SPK count while retaining each strategy's
    real traversal mode (sequential full-scan iteration or indexed sparse peeking).
H6: sparse wallet-state restore remains O(used_count + bounded lookahead), independent
    of last_revealed.
```

## Correctness Oracle

Each fixture is immutable ground truth. A strategy observation records:

```text
complete
requested_spks
discovered_indexes
batch_overfetch
restored_last_revealed
used_markers_restored
```

Required invariants:

```text
used_indexes are sorted and unique
highest used index <= last_revealed
descriptor_stop_gap is complete iff discovered_indexes == used_indexes
metadata_revealed restores last_revealed and all used markers
metadata_sparse_restore contains exactly used_indexes
known-used indexes and sparse mode survive changeset and SQLite round trips
IndexedTxGraph recognizes materialized high-index outputs
```

## Primary Metrics

Exact, deterministic metrics:

```text
completion outcome
requested SPKs
discovered used indexes
batch overfetch
SPK reduction factor for successful strategies
materialized SPKs after restore
changeset reload time
sparse TxUpdate application time
```

Expected boundary counts with `parallel_requests = 10`:

| Fixture | Strategy/configuration | Expected SPKs |
| --- | --- | ---: |
| `sparse_1_20_50` | `descriptor_stop_gap`, gap 50 | 110 |
| `sparse_1_20_50` | `metadata_revealed` | 51 |
| `sparse_1_20_50` | `metadata_sparse_restore` | 3 |
| `high_gap_50000` | `descriptor_stop_gap`, gap 20 | 30, incomplete |
| `high_gap_50000` | `descriptor_stop_gap`, gap 50000 | 100010 |
| `high_gap_50000` | `metadata_revealed` | 50001 |
| `high_gap_50000` | `metadata_sparse_restore` | 2 |

## Criterion Measurements

Criterion measures components independently:

```text
descriptor_stop_gap: consume and derive a real FullScanRequest
metadata_revealed_apply: apply metadata to a fresh wallet
metadata_revealed_request: build and consume the revealed request after apply
metadata_sparse_restore: restore index state and consume the bounded request
metadata_sparse_changeset_reload: reconstruct sparse state from a changeset
metadata_sparse_update: apply a synthetic transaction update to sparse state
metadata_encode_base64: serialize and encode metadata
metadata_decode_base64: decode and validate metadata
```

Fixture construction, history-oracle construction, metadata validation, logging,
formatting, networking, disk, and Python are outside timed sections. Benchmarks use
`black_box`, batched setup, Criterion warmup, repeated samples, and HTML reports
generated under `target/criterion`.

The descriptor and sparse strategies intentionally retain their real traversal modes.
The benchmark does not assume equal per-SPK cost; exact SPK count remains the primary
machine-independent metric.

Strategy timings are post-decode component timings. Base64 decode is measured in its
own group rather than folded into derivation, apply, or request consumption.

## Project Layout

```text
src/research/descriptor_metadata/
|-- Cargo.toml
|-- README.md
|-- src/lib.rs
|-- tests/restoration.rs
`-- benches/restoration.rs
```

There is no executable, CSV output, generated plot, or Python environment.

## Validated Results

The protocol implementation was validated on 2026-08-02 using the release benchmark
profile on macOS arm64. Exact counts are machine-independent; Criterion timings are
local evidence and should not be generalized to backend or end-to-end restore latency.

| Fixture | Strategy | Request/state basis | Criterion estimate interval |
| --- | --- | ---: | ---: |
| `sparse_1_20_50` | `descriptor_stop_gap`, gap 50 | 110 | 2.402-2.423 ms |
| `sparse_1_20_50` | `metadata_revealed_apply` | 51-index frontier | 1.713-1.721 ms |
| `sparse_1_20_50` | `metadata_revealed_request` | 51 | 6.494-6.684 us |
| `sparse_1_20_50` | `metadata_sparse_restore` | 3 | 37.950-38.395 us |
| `high_gap_50000` | `descriptor_stop_gap`, gap 50000 | 100010 | 2.280-2.312 s |
| `high_gap_50000` | `metadata_revealed_apply` | 50001-index frontier | 1.746-1.754 s |
| `high_gap_50000` | `metadata_revealed_request` | 50001 | 3.029-3.389 ms |
| `high_gap_50000` | `metadata_sparse_restore` | 2 | 37.669-38.220 us |

With the wallet's bounded lookahead of 25, sparse materialization is:

| Fixture | Requested SPKs | Materialized SPKs |
| --- | ---: | ---: |
| `sparse_1_20_50` | 3 | 26 (`0..24` and `50`) |
| `high_gap_50000` | 2 | 26 (`0..24` and `50000`) |
| `revealed_50000_unused` | 0 | 25 (`0..24`) |
| `dense_100` | 101 | 101 (`0..100`) |

Additional isolated Phase 2 timings:

| Fixture | Changeset reload | Sparse `TxUpdate` |
| --- | ---: | ---: |
| `sparse_1_20_50` | 900.7-905.9 us | 7.175-7.371 us |
| `high_gap_50000` | 905.3-909.0 us | 7.114-7.521 us |

For `[1,20,50]`, sparse restore removes 36.7x of the SPK work and is about 63x
faster than the successful descriptor stop-gap component. For `[0,50000]`, it removes
50005x of the SPK work and is about 60400x faster. Sparse restore, changeset reload, and
update application remain effectively unchanged when the frontier grows from 50 to
50,000 at comparable sparse cardinality.

An isolated A/B measurement after restoring the eager materialized-iterator fast path
reduced the high-frontier revealed request from 4.092-4.406 ms to 3.029-3.389 ms, an
18.7-29.5% statistically significant improvement. The corresponding sparse restore
groups reported no statistically significant change.

## Commands

```bash
cargo test --manifest-path src/research/descriptor_metadata/Cargo.toml
cargo test --release --manifest-path src/research/descriptor_metadata/Cargo.toml -- --ignored
cargo clippy --manifest-path src/research/descriptor_metadata/Cargo.toml --all-targets -- -D warnings
cargo bench --manifest-path src/research/descriptor_metadata/Cargo.toml
```

The ignored release test validates all expensive `high_gap_50000` strategy boundaries.

## Roadmap

- [x] Replace the exploratory executable with a library harness.
- [x] Add deterministic fixtures and independent observations.
- [x] Add RED tests for the three strategy contracts and boundary counts.
- [x] Consume real BDK full and sync requests.
- [x] Add isolated Criterion benchmark groups.
- [x] Remove CSV, output images, and Python plotting.
- [x] Validate tests, expensive boundaries, formatting, and clippy.
- [x] Record measured results only after the protocol implementation is green.

## Phase 2 Roadmap

- [x] Record the `KeychainTxOutIndex` incongruence and proposed state model in ADR-0004.
- [x] Resolve `bdk_chain` and `bdk_core` from the compatible local `bdk` checkout.
- [x] Add RED `bdk_chain` tests for sparse restore, changesets, and high-index updates.
- [x] Decouple the logical frontier from contiguous SPK materialization.
- [x] Persist sparse-restored mode and known-used indexes.
- [x] Add a real wallet sparse metadata apply and sync path.
- [x] Replace the request-only target with the real index-backed path.
- [x] Re-run correctness checks and Criterion before recording Phase 2 results.

## Follow-ups

These are intentionally outside the first study:

- stale metadata and post-export payments;
- external and internal keychains together;
- Taproot, multisig, and descriptor-complexity sensitivity;
- public backend integration;
- complete disk-and-backend user-journey timing;
- Gungraun instruction and allocation regression tracking;
