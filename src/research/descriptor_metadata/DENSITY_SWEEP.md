# Exact-Density SPK Sweep

## Scope

This secondary experiment measures how local SPK processing changes as the exact
fraction of used derivation indexes grows from 0% to 100%.

It compares two local paths:

1. Sequential descriptor derivation with a fixed stop-gap.
2. Sparse metadata application followed by construction and consumption of its bounded
   SPK request.

Criterion is the only timing engine. The experiment performs no network requests and
does not model remote history lookup, transaction download, persistence, or update
application.

The canonical fixed-fixture restoration study remains in [`README.md`](README.md) and
`benches/restoration.rs`. This sweep is additive and must not be merged into its result
table.

## Exact-Density Fixtures

The default configuration is:

```text
index_count = 1000
density_step = 5%
layouts_per_intermediate_density = 5
stop_gap = 20
base_seed = 0x5eed
```

For each density, every layout contains exactly:

```text
used_count = index_count * density / 100
```

The seed changes positions, not cardinality. Indexes are sorted before metadata is
constructed. Every fixture has `last_revealed = index_count - 1`. The 0% and 100%
endpoints use one layout because all seeds produce the same index set.

This is not Bernoulli occupancy. Bernoulli probability remains an optional later study
and must use a separate Criterion group and plot.

## Timed Paths

### Sequential Stop-Gap

Untimed setup creates a fresh wallet and `FullScanRequest`. Criterion then measures:

```text
derive the next descriptor SPK
check its index against the deterministic activity oracle
reset the miss count when used
stop after exactly stop_gap consecutive unused indexes
```

There is no batching behavior. A run is complete only when every oracle-known used index
was reached. Incomplete runs are premature stops, not successful restorations.

### Sparse Metadata API

Untimed setup creates validated metadata and a fresh wallet. Criterion then measures:

```text
apply_spk_metadata_sparse
start_sync_with_spk_metadata_at
consume the bounded SPK request
```

This is a local API-path measurement, not a pure cryptographic derivation microbenchmark.

## Measurement

Run from the research crate directory:

```bash
CARGO_TARGET_DIR="$PWD/target/density-current" \
  cargo bench --bench density_sweep
```

Or from the `bdk_wallet` root:

```bash
CARGO_TARGET_DIR="$PWD/src/research/descriptor_metadata/target/density-current" \
  cargo bench \
  --manifest-path src/research/descriptor_metadata/Cargo.toml \
  --bench density_sweep
```

The benchmark writes only Criterion artifacts. It does not generate CSV, JSON, or a
parallel timing database.

Configuration variables:

| Variable | Default | Meaning |
| --- | ---: | --- |
| `DENSITY_INDEX_COUNT` | `1000` | Index count, divisible by 100 |
| `DENSITY_STEP` | `5` | Percentage step, dividing 100 |
| `DENSITY_LAYOUTS` | `5` | Layouts per intermediate density |
| `DENSITY_STOP_GAP` | `20` | Sequential consecutive-miss limit |
| `DENSITY_BASE_SEED` | `24301` | Deterministic base seed (`0x5eed`) |
| `DENSITY_SAMPLE_SIZE` | `20` | Criterion sample size |
| `DENSITY_WARMUP_MS` | `500` | Criterion warmup per benchmark |
| `DENSITY_MEASUREMENT_MS` | `1000` | Criterion measurement target |

Use the same fixture variables for measurement and plotting.

## Presentation

The Plotters example is a read-only adapter over Criterion data. It does not run a
benchmark, measure elapsed time, delete Criterion output, or derive SPKs.

From the research crate directory:

```bash
CARGO_TARGET_DIR="$PWD/target/density-current" \
  cargo run --release --example density_plot
open target/density-current/density-scatter.svg
```

The scatter uses:

| Marker | Meaning |
| --- | --- |
| Green circle | Sparse metadata API layout |
| Orange triangle | Complete sequential stop-gap layout |
| Red cross | Incomplete sequential stop-gap layout |
| Solid line | Median across successful layouts |

Axes:

```text
X: exact used-index density (%)
Y: Criterion local API time, logarithmic, no network
```

Incomplete stop-gap points are shown but excluded from the successful median line.

The dedicated `CARGO_TARGET_DIR` isolates the current density run from canonical and
historical Criterion artifacts. The adapter fails if any expected current-run estimate
is missing rather than falling back to another target directory.

## Limitations

- The two timed paths perform different local work and should be interpreted as API-path
  scaling, not identical end-to-end restoration operations.
- Fixture generation, wallet construction, networking, persistence, and plotting are
  outside timed regions.
- Exact-density topology variance is represented by five deterministic layouts, not a
  statistical model of real wallet activity.
- Stop-gap incompleteness is a correctness outcome. A shorter incomplete run is not a
  performance win.
- Results are machine-local. Exact fixture cardinalities and completion outcomes are the
  machine-independent evidence.
