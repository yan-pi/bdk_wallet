# Descriptor Metadata — Criterion Implementation Plan

## Operating mode and scope

This document is an architecture plan only. No benchmark source, Cargo manifest, existing benchmark, or production code was changed for this task. No long benchmark suite was run.

The plan is intentionally explicit about what is timed, what is not timed, which state is reset, and which conclusions are valid. The implementation should not begin until the decisions in [Decisions Before Implementation](#decisions-before-implementation) are approved.

---

## 1. Current repository state

### HEADs

The current branches still point at the validated commits:

| Repository | Branch | HEAD | Status relative to validated reference |
|---|---|---|---|
| `yan-pi/bdk` | `research/sparse-keychain-restoration` | `03df61edfe430ca4520d480a7896210e0bdf7004` | unchanged |
| `yan-pi/bdk_wallet` | `research/spk-benchmark-v2` | `51dedcc332b8a7b40be9d9d5317aa7b6930ab31c` | unchanged |

The `bdk_wallet` worktree contains the previously created, untracked `STATE_VALIDATION_REPORT.md`. It does not contain benchmark code or manifest edits. The `bdk` worktree has no relevant benchmark changes.

### Existing benchmark infrastructure

#### `yan-pi/bdk`

Existing benchmark targets are:

```text
crates/chain/benches/canonicalization.rs
crates/chain/benches/indexer.rs
crates/electrum/benches/test_sync.rs
```

The relevant manifest entries are:

```toml
# crates/chain/Cargo.toml
criterion = { version = "0.2" }
[[bench]]
name = "canonicalization"
harness = false
[[bench]]
name = "indexer"
harness = false

# crates/electrum/Cargo.toml
criterion = { version = "0.2" }
[[bench]]
name = "test_sync"
harness = false
```

The lockfile resolves Criterion `0.2.11`.

- [`crates/chain/benches/indexer.rs`](https://github.com/yan-pi/bdk/blob/03df61edfe430ca4520d480a7896210e0bdf7004/crates/chain/benches/indexer.rs) measures a fixed `IndexedTxGraph` changeset reconstruction and balance check. It uses fixed constants (`LOOKAHEAD=10`, `LAST_REVEALED=500`, `TX_CT=21`) and does not compare sparse metadata with a dense control. It is useful as historical indexer benchmark code, but should not be reused as the descriptor-metadata suite.
- [`crates/chain/benches/canonicalization.rs`](https://github.com/yan-pi/bdk/blob/03df61edfe430ca4520d480a7896210e0bdf7004/crates/chain/benches/canonicalization.rs) measures transaction graph canonicalization scenarios. It is unrelated and should remain unchanged.
- [`crates/electrum/benches/test_sync.rs`](https://github.com/yan-pi/bdk/blob/03df61edfe430ca4520d480a7896210e0bdf7004/crates/electrum/benches/test_sync.rs) exercises a live test environment and Electrum I/O. It is not suitable for deterministic metadata benchmarks and should remain historical.

The low-level APIs needed by the research are public: `KeychainTxOutIndex::restore_keychain_state`, `reveal_to_target`, `from_changeset`, `inner().all_spks()`, `known_used_spks`, `revealed_spks`, and the `SyncRequest` builder. Relevant implementation is in [`keychain_txout.rs`](https://github.com/yan-pi/bdk/blob/03df61edfe430ca4520d480a7896210e0bdf7004/crates/chain/src/indexer/keychain_txout.rs) and [`spk_client.rs`](https://github.com/yan-pi/bdk/blob/03df61edfe430ca4520d480a7896210e0bdf7004/crates/core/src/spk_client.rs).

#### `yan-pi/bdk_wallet`

There is currently:

```text
no benches/ directory
no [[bench]] entry
no direct Criterion dependency
```

The wallet has the required public APIs and deterministic test helpers:

- `SpkMetadata` and its Elias–Fano/base64/bech32 methods in [`src/types.rs`](https://github.com/yan-pi/bdk_wallet/blob/51dedcc332b8a7b40be9d9d5317aa7b6930ab31c/src/types.rs);
- `Wallet::apply_spk_metadata_sparse` and dense `Wallet::apply_spk_metadata` in [`src/wallet/mod.rs`](https://github.com/yan-pi/bdk_wallet/blob/51dedcc332b8a7b40be9d9d5317aa7b6930ab31c/src/wallet/mod.rs#L1950-L2010);
- sparse and revealed request builders in [`src/wallet/mod.rs`](https://github.com/yan-pi/bdk_wallet/blob/51dedcc332b8a7b40be9d9d5317aa7b6930ab31c/src/wallet/mod.rs#L2747-L2805);
- `Wallet::apply_update` in [`src/wallet/mod.rs`](https://github.com/yan-pi/bdk_wallet/blob/51dedcc332b8a7b40be9d9d5317aa7b6930ab31c/src/wallet/mod.rs#L2250-L2275);
- wallet SQLite persistence through `ChangeSet` and `WalletPersister`;
- public `bdk_wallet::test_utils` behind the existing `test-utils` feature.

The wallet lockfile already contains Criterion `0.5.1`, but the package manifest does not directly request it. The proposed benchmark target will make that dependency explicit only after approval.

### Ownership recommendation

Put the new research harness in **`bdk_wallet`**, while benchmarking both wallet-level and public `bdk_chain` APIs from that one target.

Reasons:

1. `SpkMetadata`, its codecs, wallet application, wallet sync requests, update application, and SQLite reload all belong to `bdk_wallet`.
2. The benchmark must pin and exercise both repositories together. The wallet manifest already patches `bdk_chain` and `bdk_core` to the sibling research checkout.
3. The needed low-level chain APIs are public, so a wallet benchmark can measure them without private access or production instrumentation.
4. One harness prevents two fixture generators, two naming systems, and two incompatible Criterion result trees.
5. It avoids changing the existing `bdk_chain` Criterion `0.2.11` dependency merely to add a second copy of the same research suite.

The existing `bdk` benchmarks remain unchanged and historical. A separate chain-native target can be added later if maintainers require crate-local performance ownership, but it is not required for this research pass.

### Proposed Criterion version

Use Criterion `0.5.1` for the new `bdk_wallet` target with the `html_reports` feature. This provides the current output/report behavior and is already present in the wallet lockfile. The old `bdk_chain` `0.2.11` target remains independent.

This version choice is a decision requiring approval because adding the direct dependency will update `bdk_wallet/Cargo.toml` and potentially `Cargo.lock` during implementation.

---

## 2. Criterion concepts used by this project

The descriptions below use the current Criterion model documented in the [Criterion user guide](https://bheisler.github.io/criterion.rs/book/).

### `criterion_group!` and `criterion_main!`

- **What they do:** `criterion_group!` registers benchmark functions and optional configuration; `criterion_main!` generates the benchmark executable entry point.
- **Why we need them:** Each benchmark target must expose a stable list of groups to `cargo bench`.
- **Where used:** The single `descriptor_metadata` target will register restore, materialization, request, update, persistence, codec, and end-to-end group functions.
- **Mistake prevented:** A benchmark function that compiles but is not registered will never run.

### `Criterion`

- **What it does:** Owns the run configuration and creates benchmark groups.
- **Why we need it:** Configure warm-up, measurement duration, sample size, significance level, and command-line filtering.
- **Where used:** Top-level group functions receive `&mut Criterion`; default settings will be conservative for the quick suite and overridden for expensive groups.
- **Mistake prevented:** Comparing one case with different statistical settings without recording that difference.

### `BenchmarkGroup`

- **What it does:** Groups related cases and allows group-level configuration and throughput metadata.
- **Why we need it:** Frontier sweeps, used-count sweeps, and distribution sweeps need separate settings and readable reports.
- **Where used:** Examples include `restore`, `request`, `codec`, and `persistence`.
- **Mistake prevented:** Treating a dense `F=500_000` case like a microbenchmark and causing an impractical default run.

### `BenchmarkId`

- **What it does:** Gives each input case a stable, human-readable result identity.
- **Why we need it:** Every result must expose `F`, `R`, `U`, `L`, distribution, cache mode, and operation without reading source.
- **Where used:** Every parameterized `bench_with_input` call.
- **Mistake prevented:** Overwriting or confusing results when only one numeric input is shown.

### `bench_function`

- **What it does:** Benchmarks a named closure without a parameter passed by Criterion.
- **Why we need it:** Use for fixed canonical cases or request consumption over a prebuilt fixture.
- **Where used:** A fixed end-to-end case and fixed codec baseline may use it.
- **Mistake prevented:** Accidentally rebuilding a fixture on every invocation when the intended subject is only the operation.

### `bench_with_input`

- **What it does:** Associates a benchmark operation with an input value and a `BenchmarkId`.
- **Why we need it:** It is the main form for sweeping `BenchCase` values.
- **Where used:** Restore, materialization, request, update, persistence, and codec parameter matrices.
- **Mistake prevented:** Capturing a loop variable incorrectly or failing to associate metrics with the exact fixture dimensions.

### `Bencher` and `b.iter(...)`

- **What they do:** `Bencher` controls the repeated measurement; `b.iter(|| operation())` times repeated calls to the closure.
- **Why we need them:** Use `iter` only when the operation is repeatable without mutation or when the closure itself constructs a fresh state as part of the intended subject.
- **Where used:** Pure codec encoders/decoders and request consumption over an immutable prepared input are candidates.
- **Mistake prevented:** Reporting one call's wall-clock noise as a benchmark result.
- **Important limitation:** A mutable one-shot operation cannot use `b.iter` on the same object. The first call changes the state and later iterations no longer measure the same operation.

### `b.iter_with_setup(...)`

- **What it does:** Runs setup before each measured operation.
- **Why we need it:** It is conceptually useful for explaining the desired reset boundary.
- **Where used:** Prefer not to use it in new code. Current Criterion documentation treats it as a backward-compatible older timing loop equivalent to `iter_batched` with `PerIteration`.
- **Mistake prevented:** It makes a fresh state possible, but its forced per-iteration setup can add considerable overhead and obscure a microbenchmark.

### `b.iter_batched(...)`

- **What it does:** Generates owned setup input outside the timed routine, then times the routine on that input according to a `BatchSize`.
- **Why we need it:** It is the default for `restore_keychain_state`, `apply_update`, wallet metadata application, and reload operations that mutate or consume state.
- **Where used:**
  - setup: clone/create a fresh wallet/indexer/update;
  - timed routine: apply exactly one operation;
  - result: discard the mutated state after the timed call.
- **Mistake prevented:** Reusing a mutated object and measuring a different operation on every iteration.

### `b.iter_batched_ref(...)`

- **What it does:** Sets up an owned value outside timing and passes a mutable reference to the timed routine.
- **Why we need it:** Useful when the setup object is large and ownership transfer/drop behavior should not be part of the operation.
- **Where used:** Potentially SQLite/reload wrappers or large request fixtures where the routine mutates a reusable container and the setup closure can restore it.
- **Mistake prevented:** Timing a large move/drop or forcing unnecessary allocations into the subject.
- **Caution:** It must still restore the logical initial state for every routine invocation; a reference alone does not solve mutation.

### `black_box`

- **What it does:** Hides values from optimizer assumptions while preserving the operation result.
- **Why we need it:** Prevent the compiler from proving that a deterministic fixture, decoded metadata, or returned request is unused.
- **Where used:** Wrap benchmark inputs and observable outputs. For mutations, black-box the returned changeset/request/count or assert it outside timing in a validation pass.
- **Mistake prevented:** Measuring optimized-away work.
- **Caution:** `black_box` does not reset state, make a fixture random, or make unequal cases equivalent.

### `BatchSize`

- **What it does:** Controls how many setup values are made per timed batch.
- **Planned use:**
  - `SmallInput` for ordinary indexers, metadata, requests, and small updates;
  - `LargeInput` when dense states or serialized payloads make many setup values too memory-heavy;
  - `PerIteration` only for resource-sensitive SQLite cases or very large states where one fresh setup is required.
- **Mistake prevented:** Either measuring setup accidentally or exhausting memory by prebuilding too many large states.
- **Tradeoff:** A batch amortizes setup and drops, but the timed closure still runs multiple logical operations. The reported time is normalized per operation.

### Sample size, warm-up time, and measurement time

- **Sample size:** Number of statistical samples, not number of logical operations. Criterion may run many operations in one sample.
- **Warm-up time:** Lets Criterion estimate operation cost and choose iteration counts before collecting samples. It is not included in the reported result.
- **Measurement time:** Approximate time budget for collecting samples; expensive cases can exceed it because one iteration cannot be split.
- **Why needed here:** Dense frontier and revealed-range cases have very different costs. A single global configuration would either under-sample microbenchmarks or make large cases too expensive.
- **Mistake prevented:** Treating Criterion's default sample count as a fixed number of calls or assuming every benchmark consumes exactly the configured duration.

### Confidence intervals

Criterion reports an estimate and interval, commonly using bootstrap resampling. The interval describes statistical uncertainty under the run conditions; it is not a guarantee that another machine or another fixture will fall inside it.

Use the typical/slope estimate for trend summaries, retain the interval, and do not call two cases meaningfully different merely because their point estimates differ.

### Outliers

Criterion classifies unusually high or low samples. Outliers can reflect process scheduling, CPU contention, filesystem behavior, page faults, or a real data-dependent tail.

For this project:

- sparse restore should have relatively uniform work when setup is outside timing;
- SQLite reload may have real filesystem/cache effects;
- a high outlier rate means the result should be reported with caution, not silently deleted.

### Throughput

Throughput converts the per-iteration estimate into work per second for a declared amount of bytes or elements.

Use it only where the unit is meaningful:

- codec: bytes encoded/decoded and metadata entries;
- request: SPKs requested/consumed;
- update: outputs processed if the fixture defines that unit.

Do not use throughput to disguise unequal semantic work. A dense case and sparse case can both report SPKs/sec, but their structural metrics must be shown beside the timing.

### Criterion output directories

Criterion writes under `target/criterion/<benchmark-name>/`, with `new/`, optional `base/`, `change/`, and `report/` directories. Current Criterion documentation shows `raw.csv`, `estimates.json`, `sample.json`, `tukey.json`, and HTML/SVG reports.

The JSON files are implementation details. `raw.csv` is the stable machine-readable Criterion artifact. The HTML report is useful for inspection but insufficient as the only research record.

### Baseline comparison

Criterion baselines are saved statistical results for the same benchmark IDs. Commands such as `--save-baseline` and `--baseline` compare a new run with a prior run.

Criterion's baseline is **not** the research stop-gap baseline. The distinctions are:

- Criterion baseline: a prior measurement of the same executable/ID, normally used to detect regression.
- Dense control: an actual BDK eager/frontier operation measured in the same run.
- Stop-gap baseline: a defined recovery algorithm/model that scans unknown scripts until a stop gap.
- Sparse candidate: metadata-assisted restoration/request/update flow.

A Criterion baseline must never be named `dense`, `sparse`, or `stop-gap` unless the benchmark operation itself has that meaning.

---

## 3. Criterion measurement model in this project

Consider one case:

```text
operation = sparse restore
F = 50_000
U = 10
L = 25
D = tail-heavy
```

### One logical benchmark operation

One logical operation is:

```text
apply one exact metadata snapshot to one fresh, descriptor-installed indexer
```

It records one frontier and one set of ten known-used indexes. Its expected postcondition is a sparse index with bounded initial materialization plus the missing known-used SPKs.

### One Criterion iteration

One Criterion iteration is one invocation of the timed routine. With `iter_batched`, it is conceptually:

```text
setup: create or obtain a fresh indexer              # not timed
start timer
restore_keychain_state(indexer, F, used_indexes)     # timed
stop timer
tear down returned state                             # generally outside routine timing
```

The exact setup batch size may cause Criterion to create multiple setup values before timing a batch of routine calls, but each routine call must still start from equivalent state.

### One statistical sample

A sample is a timed block containing one or more iterations. Criterion chooses a calibration factor during warm-up. If the operation is fast, a sample may contain many restore calls; if it is expensive, it may contain one.

Criterion measures the elapsed time for the complete sample and divides by the number of iterations in that sample. It does not independently timestamp every logical operation.

### Run sequence

1. **Warm-up:** Criterion calls the operation repeatedly to estimate its cost and stabilize the timing loop. These calls are not reported as measurement samples.
2. **Calibration:** It chooses iteration counts for samples so the configured measurement time is useful.
3. **Sampling:** It collects the configured number of samples. Sample iteration counts generally increase as Criterion accumulates evidence.
4. **Analysis:** It estimates typical time/slope, mean, median, deviation, confidence intervals, and outliers.
5. **Comparison:** If a Criterion baseline exists, it computes change statistics against the matching benchmark ID.
6. **Output:** It writes raw CSV, estimates/sample JSON, and optional HTML/SVG reports.

The setup fixture must be deterministic and equivalent for every iteration. `black_box` should keep the returned changeset or postcondition observable. A separate untimed validation invocation must verify that the sparse and dense controls produce the intended logical state before any timing result is interpreted.

---

## 4. Fixture model

### Proposed type

The fixture should preserve the generic research dimensions while making BDK's projection explicit:

```rust
struct BenchCase {
    frontier: u32,              // F = last_revealed
    revealed_count: usize,      // R; required to equal frontier + 1 for BDK cases
    used_indexes: Vec<u32>,     // exact U indexes, sorted and unique
    lookahead: u32,              // L
    distribution: Distribution,  // D
    persist_spks: bool,
    keychains: KeychainShape,
}
```

Derived values should not be independently stored:

```text
used_count = used_indexes.len()
revealed_count = frontier + 1 under current BDK contiguous semantics
```

`revealed_count` remains in the type because the generic Descriptor Metadata model must not be erased. Fixture validation should reject a BDK case where `revealed_count != frontier + 1`; a future generic model could use the field differently without changing the benchmark vocabulary.

`KeychainShape` should initially support:

```text
ExternalOnly
ExternalAndInternalSameCase
```

The first implementation can benchmark external-only low-level cases and two-keychain wallet cases separately. It must not silently mix per-keychain counts into one total.

### Fixture invariants

For every generated case:

```text
used_indexes are sorted and unique
all used_indexes <= frontier
used_count <= revealed_count
frontier <= BIP32_MAX_INDEX
last_revealed = Some(frontier) for non-empty BDK cases
```

The canonical validated fixture is:

```text
F = 50_000
R = 50_001
U = 2
used_indexes = [0, 50_000]
L = 25
D = TailHeavy
```

### Fixture construction layers

Fixtures should be built in named layers so the benchmark boundary is visible:

1. `DescriptorFixture`: fixed descriptor string, parsed once per benchmark case outside timing.
2. `IndexFixture`: a descriptor-installed `KeychainTxOutIndex` with selected lookahead/cache mode.
3. `MetadataFixture`: validated `SpkMetadata` and encoded representations.
4. `WalletFixture`: a fresh wallet or persisted snapshot with external/internal descriptors.
5. `UpdateFixture`: deterministic `Update`/`TxUpdate` containing selected high-index and irrelevant outputs.
6. `PersistenceFixture`: pre-populated SQLite connection/file or serialized `ChangeSet`.

No fixture generator should use current time, OS randomness, network access, or descriptor derivation inside the timed closure unless that is explicitly the subject.

---

## 5. Deterministic index distributions

All distributions must be deterministic and record their name and parameters in the BenchmarkId. Prefer formulas over random sampling. If sampling without replacement is necessary, use a fixed documented seed such as `0xD3_5C_A1_9E` and record it in provenance; the seed is part of the fixture definition, not an untracked implementation detail.

### Dense

```text
used = 0..U
```

For `U=R`, this represents a fully dense used/revealed prefix. For small `U`, it represents activity concentrated at the beginning.

- **Behavior:** early addresses are active.
- **Hypothesis:** exposes fixed per-entry work and dense codec compression.
- **Caveat:** it does not mean the entire frontier is materialized unless `U=R` or the dense control is used.

### UniformSparse

Generate `U` unique positions spread across `[0,F]`, for example by deterministic evenly spaced quantiles with a fixed offset, or a fixed-seed sample without replacement.

- **Behavior:** activity is distributed through the logical range.
- **Hypothesis:** tests gap-size effects in metadata codecs and sparse derivation at representative positions.
- **Caveat:** uniform positions do not model an actual wallet's address-use process by themselves.

### Clustered

Choose deterministic cluster centers and place indexes in short runs around those centers, clamped to `[0,F]` and deduplicated.

- **Behavior:** multiple nearby addresses are used in a few activity bursts.
- **Hypothesis:** tests cache locality, duplicate-gap compression, and clustered materialization.
- **Caveat:** cluster width and center count must be included in the fixture ID.

### TailHeavy

Reserve a fixed fraction of indexes near `F`, always including `F` for cases with `U>0`, and place the remainder near the beginning or at deterministic tail offsets.

- **Behavior:** a wallet has a very high logical frontier with recent activity near the end.
- **Hypothesis:** tests the central sparse-recovery case and high-index recognition.
- **Canonical example:** `[0, F]` for `U=2`.
- **Caveat:** the high-index output must be materialized before `apply_update` if recognition is the subject.

### AdversarialStopGap

Place known activity at index `0` and near `F`, with a long empty interval and no accidental activity in the proposed stop-gap region.

- **Behavior:** a stop-gap scanner cannot stop early without risking missing the tail activity.
- **Hypothesis:** exposes the difference between exact metadata recovery and frontier/stop-gap scanning.
- **Caveat:** this is a controlled recovery model, not a claim about an actual backend history unless the simulated history oracle is specified.

---

## 6. Timed and untimed boundaries

The rule is: **construct immutable fixture recipes outside timing; reset mutable state in Criterion setup; time only the named operation.**

### A. Sparse restore

**UNTIMED SETUP**

- select and validate `BenchCase`;
- parse the descriptor;
- create an indexer with `lookahead=L` and `persist_spks` mode;
- insert the descriptor;
- generate and validate `used_indexes`;
- prepare the fixture recipe or cloneable pre-operation indexer;
- prepare expected structural metrics.

**TIMED OPERATION**

```text
restore_keychain_state(keychain, Some(F), used_indexes.iter().copied())
```

**RESET**

Use `iter_batched` with a fresh indexer or an equivalent clone of a pristine descriptor-installed indexer. Do not call restore repeatedly on one object.

**What it supports:** cost of applying exact metadata to an already descriptor-installed indexer, including known-used SPK materialization and changeset construction.

**What it does not support:** cost of parsing descriptors, constructing metadata, SQLite I/O, or dense range restoration.

### B. Dense/eager control

The actual BDK operation is `KeychainTxOutIndex::reveal_to_target(keychain, F)` for the low-level control and `Wallet::apply_spk_metadata`/`reveal_addresses_to` for the wallet-level control. It is not a synthetic loop.

**UNTIMED SETUP**

- parse descriptor;
- create the same descriptor-installed indexer with the same `L` and cache mode;
- create the same logical frontier and used-index fixture;
- ensure the dense control starts from the same pre-operation descriptor state.

**TIMED OPERATION**

```text
reveal_to_target(keychain, F)
```

or, at wallet level:

```text
apply_spk_metadata(&metadata)
```

**RESET**

Fresh state per operation with `iter_batched`.

**Fairness boundary:** both paths begin after descriptor installation and use the same descriptor and lookahead. The dense control intentionally materializes `0..=F` (plus any implementation-defined lookahead) while sparse restore materializes only bounded setup plus `used_indexes`. They are not equal physical work; that is the research hypothesis. They must be compared only after confirming equal logical frontier, known-used state, descriptor, and keychain configuration.

### C. Materialization/reconstruction

Do not call private `materialize_sparse_state` directly. The public reconstruction paths are:

```text
KeychainTxOutIndex::from_changeset(...) + insert_descriptor(...)
Wallet::load(...) from a persisted ChangeSet
```

**UNTIMED SETUP**

- create a dense `ChangeSet` and sparse `ChangeSet` representing equivalent logical metadata;
- parse/hold the descriptor outside timing;
- prepare cloneable changesets and configuration.

**TIMED OPERATION**

Measure separate cases:

1. dense changeset application followed by descriptor insertion;
2. sparse changeset application followed by descriptor insertion;
3. wallet-level sparse metadata application after descriptor creation;
4. wallet-level dense metadata application after descriptor creation.

**RESET**

Use `iter_batched` with owned changesets and descriptors. For large dense changesets use `LargeInput` and document allocation/reset overhead as excluded setup.

**What it supports:** public reconstruction-path cost.

**What it does not support:** a private function's isolated loop cost or a generalized materialization primitive that does not exist in the API.

### D. Request construction and consumption

The current builder methods consume their iterators while constructing the request: `known_used_spks_from_indexer` calls `spks_with_indexes`, which extends the request's vector. Therefore construction and consumption are distinct.

#### Request construction

**UNTIMED SETUP**

- prepare an indexer/wallet with equivalent state;
- prepare a fixed chain tip and expected transaction set;
- do not construct the request yet.

**TIMED OPERATION**

```text
wallet.start_sync_with_spk_metadata_at(0).build()
wallet.start_sync_with_revealed_spks_at(0).build()
```

or the corresponding low-level `SyncRequest::builder_at(...).spks_with_indexes(...).build()` calls.

This measures iterator derivation/consumption into the request vector, allocation, and request construction.

#### Request consumption

**UNTIMED SETUP**

- build a fresh request outside timing;
- verify its expected SPK count;
- prepare a fresh request for each timed operation because iteration consumes it.

**TIMED OPERATION**

```text
request.iter_spks_with_expected_txids().count()
```

or consume each item through the exact client-facing iterator operation.

**RESET**

Use `iter_batched` to create a fresh request per operation. Do not clone a consumed request unless cloning is explicitly part of the subject.

**What it supports:** exact sparse request cardinality versus frontier-proportional revealed request work. It does not measure backend/network history lookup.

### E. Update application

**UNTIMED SETUP**

- create a fresh wallet with descriptors;
- apply sparse metadata to put the high SPK in the index;
- build a deterministic `Update` with one or more known relevant outputs and configurable irrelevant outputs;
- precompute transaction IDs, scripts, anchors, and chain update.

**TIMED OPERATION**

```text
wallet.apply_update(update.clone())
```

The update can have variants:

```text
irrelevant_outputs = 0, 1, 10, 100
relevant_index = 0, F, or both
```

**RESET**

`iter_batched` fresh wallet plus an owned update. The update must have the same transaction graph semantics in every iteration.

**What it supports:** update/indexing cost after sparse or dense state preparation. It does not measure network acquisition or transaction construction outside the supplied update.

### F. Persistence/reload

Split persistence into distinct experiments:

1. in-memory `ChangeSet` reconstruction;
2. SQLite write/persist;
3. SQLite read/reload;
4. optional wallet-level load orchestration.

**UNTIMED SETUP**

- create a deterministic logical state and changeset;
- for SQLite read tests, prepare a populated database snapshot;
- for write tests, prepare a fresh transaction/database state;
- decide whether opening a file/connection is included.

**TIMED OPERATION**

- ChangeSet reconstruction: `KeychainTxOutIndex::from_changeset` plus descriptor insertion, or wallet load from a supplied changeset.
- SQLite write: `ChangeSet::persist_to_sqlite` or `wallet.persist` in a transaction.
- SQLite read: `ChangeSet::from_sqlite` or `Wallet::load().load_wallet` from an already-open populated connection.

**RESET**

Use `iter_batched` for owned in-memory state. Use a fresh transaction or fresh connection per iteration for resource-sensitive SQLite cases. `PerIteration` is acceptable here because a reused transaction can change cache and cursor state; the setup overhead must be clearly excluded or measured in a separate open-and-load case.

**What it supports:** the named storage operation only. A SQLite benchmark that excludes opening the database must not be reported as full application startup time.

### G. Metadata codec

**UNTIMED SETUP**

- generate sorted used indexes and synthetic revealed indexes;
- construct `SpkMetadata` once;
- precompute base64/bech32 strings and raw baseline buffers for decode cases;
- calculate byte counts outside timing.

**TIMED OPERATION**

Separate:

```text
SpkMetadata::encode_elias_fano
SpkMetadata::encode_base64
SpkMetadata::decode_base64
SpkMetadata::encode_bech32
SpkMetadata::decode_bech32
```

and benchmark-only baselines:

```text
raw little-endian u32
sorted delta-u32
sorted delta-varint
bit-packed bitmap over [0,F]
```

The generic `EF(revealed_spk)` case is synthetic because BDK does not store an arbitrary revealed set; BDK's revealed set is derivable from `0..=F`. It must be labeled as a generic-model comparison, not an implementation path.

**RESET**

Codec operations are pure. Use `b.iter` with a prebuilt input and `black_box` the returned object. For decode, do not reuse a mutable decoder state.

**What it supports:** codec CPU and transport-size comparisons under controlled index distributions. It does not support wallet restore cost or network transfer time.

### H. End-to-end metadata-assisted load

The primary end-to-end case should be deterministic and offline:

```text
encoded metadata
 -> decode metadata
 -> create/load descriptor wallet
 -> apply sparse metadata
 -> construct known-used sync request
 -> consume request against a deterministic no-network request consumer
 -> apply a deterministic synthetic response/update
 -> validate equivalent logical wallet state
```

**UNTIMED SETUP**

- fixed descriptors and chain tip;
- encoded metadata strings;
- deterministic synthetic backend response/update;
- expected logical state and structural metrics;
- no network client, test daemon, or wall-clock query.

**TIMED OPERATION**

The complete offline pipeline above. If a narrower result is needed, also expose sub-pipelines with explicit names.

**Stop-gap baseline**

The stop-gap baseline must be a separately named deterministic model, not hidden inside the sparse benchmark. It should:

1. derive/request scripts from index 0 upward;
2. supply a deterministic history oracle with the same relevant indexes;
3. stop only after the configured empty-history gap;
4. apply the resulting update to a fresh wallet;
5. validate the same logical frontier and known-used state.

The baseline may use BDK's actual revealed/full-scan request abstractions, but the stop-gap decision itself is client/backend policy and is not implemented by `SyncRequest`. If a helper simulates the oracle, label the result `stop-gap-model`, not `bdk-network-sync`.

**What it supports:** end-to-end CPU/state-reconstruction comparison under an explicit offline oracle.

**What it cannot support:** real backend latency, network bandwidth, server indexing behavior, or a universal claim that metadata recovery is faster in production.

---

## 7. Mutable-state and iteration isolation strategy

| Operation | Mutates/consumes | Planned loop | Why |
|---|---|---|---|
| `restore_keychain_state` | indexer, changeset stage | `iter_batched` fresh indexer | restore is one-shot for a given snapshot |
| `reveal_to_target` | indexer | `iter_batched` fresh indexer | later calls become no-ops or do less work |
| `apply_spk_metadata_sparse` | wallet indexer/stage | `iter_batched` fresh wallet | staged state accumulates |
| `apply_spk_metadata` | wallet indexer/stage | `iter_batched` fresh wallet | dense revelation is monotonic |
| `apply_update` | wallet graph/index/stage | `iter_batched` fresh wallet and update | graph/index state changes |
| request construction | consumes logical iterator into request | fresh source/indexer or wallet per iteration | construction may derive/consume scripts |
| request iteration | consumes request queue | fresh request per iteration | `next_spk` removes items |
| `ChangeSet` reconstruction | creates fresh state | `iter_batched` owned changeset | isolates allocation and state creation |
| SQLite read | consumes transaction/cursors and OS cache | fresh transaction/connection where required | avoids hidden state reuse |
| pure codec encode/decode | no logical mutation | `iter` or `iter_batched` | input remains repeatable |

The plan does **not** clone a large post-operation dense state inside the timed closure. If cloning itself is a research question, it gets a separate benchmark ID.

---

## 8. Structural instrumentation

Timing alone cannot establish the sparse claim. Each case must produce a structural record from a separate validation execution or from fixture-derived facts, not from extra work inside the timed closure.

| Metric | Current observability | Collection plan | Timer placement |
|---|---|---|---|
| logical frontier | public `last_revealed_index`, wallet `derivation_index` | read after validation operation | outside timer |
| `R` / logical revealed entries | derived as `F+1` under BDK semantics | fixture metric; label as derivable | outside timer |
| known-used entries | public `known_used_keychain_indexes` at chain level; metadata at wallet level | count exact indexes | outside timer |
| SPKs materialized | public chain `inner().all_spks().len()` | count before/after on low-level cases | outside timer |
| SPKs requested | public `SyncRequest::progress().total_spks()` | build request in validation pass and record | outside timer |
| SPKs derived | no production counter; infer only in fresh `persist_spks=false` cases from materialized delta, or derive from cache delta | record as `inferred`; cache-hit versus derivation is not distinguishable with current public API | outside timer |
| metadata transport bytes | public base64/bech32 string lengths and decoded payload length | record each encoding separately | outside timer |
| raw codec bytes | benchmark-local encoders | record buffer lengths | outside timer |
| changeset field entries | public `ChangeSet` fields | count frontier, sparse marker, known-used, cache, tx/chain entries | outside timer |
| changeset serialized bytes | `serde_json::to_vec` under existing serde support | record JSON size and call it `serde_json_bytes` | outside timer; not SQLite bytes |
| SQLite row counts | existing schema is known; public read changeset can be counted | record logical row counts where practical | outside timer |
| physical database bytes | not a stable public API metric | do not claim unless a separate file-size experiment is approved | not collected initially |

### No production counters by default

Do not add counters to `KeychainTxOutIndex` or `Wallet` solely for benchmarking. The current public chain API is sufficient for materialized-SPK counts. Wallet private physical materialization is not directly observable; wallet-level reports should use request counts, metadata counts, changeset/cache counts, and the low-level chain cases for physical materialization.

If a later requirement demands exact wallet-level derivation counts, the preferred order is:

1. expose an existing state fact through a benchmark/test-only helper;
2. add a narrowly scoped test-only inspection API;
3. only then consider production counters.

No such change is approved by this plan.

### Structural output format

Write one row per benchmark case to a separate research CSV, for example:

```text
suite,group,operation,F,R,U,L,D,cache,keychains,
logical_entries,known_used_entries,spks_derived_inferred,
spks_materialized,spks_requested,metadata_bytes,
changeset_entries,changeset_json_bytes,notes
```

The CSV is not Criterion's timing data. It is a companion artifact keyed by the exact BenchmarkId.

---

## 9. Proposed benchmark groups

### `restore`

Cases:

```text
restore/sparse
restore/dense-control
restore/wallet-sparse
restore/wallet-dense-control
```

Primary variables: `F`, `U`, `L`, `D`, cache mode. The sparse case measures exact metadata restore; the dense control uses actual BDK eager revelation.

### `materialization`

Cases:

```text
materialization/from-changeset/sparse
materialization/from-changeset/dense
materialization/wallet-load/sparse
materialization/wallet-load/dense
```

This group measures public reconstruction paths, not private helper loops.

### `request`

Cases:

```text
request/build/known-used
request/build/revealed
request/consume/known-used
request/consume/revealed
```

Construction and consumption must not be merged into one unnamed timing.

### `update`

Cases:

```text
update/apply/relevant-high-index
update/apply/irrelevant-only
update/apply/mixed
```

Vary relevant index and irrelevant output count while preserving transaction validity.

### `persistence`

Cases:

```text
persistence/changeset-reconstruct
persistence/sqlite-write
persistence/sqlite-read
persistence/wallet-load
```

SQLite connection-open cost is excluded from read-only reload unless a separate `open-and-load` ID is added.

### `codec`

Cases:

```text
codec/ef-used
codec/base64-used
codec/bech32-used
codec/raw-u32
codec/delta-u32
codec/delta-varint
codec/bitmap-used
codec/synthetic-ef-revealed
```

Each case records both time and encoded size.

### `end_to_end`

Cases:

```text
end_to_end/metadata-assisted
end_to_end/dense-frontier-control
end_to_end/stop-gap-model
```

All are offline, deterministic, and validated for equivalent logical state before speedup claims.

---

## 10. Controlled parameter matrix

Do not run the full Cartesian product.

### Frontier experiment

Hold `U`, `L`, `D`, cache, and keychain shape constant:

```text
F = 50, 500, 5_000, 50_000, 500_000
U = 2 or 10
L = 25
D = TailHeavy
```

Purpose: determine whether sparse restore/request cost follows `F` or remains tied primarily to `U`, while showing where revealed-range work follows `F`.

`F=500_000` is manual/full-suite only.

### Used-count experiment

Hold `F=50_000`, `L=25`, and `D=UniformSparse` constant:

```text
U = 1, 3, 10, 100, 1_000, 10_000
```

Reject values where `U > F+1`. Purpose: estimate per-known-used-entry work and codec growth.

### Distribution experiment

Hold:

```text
F = 50_000
U = 10
L = 25
```

Run:

```text
Dense
UniformSparse
Clustered
TailHeavy
AdversarialStopGap
```

Purpose: expose gap, locality, high-index, and stop-gap effects.

### Lookahead experiment

Hold:

```text
F = 50_000
U = 2
D = TailHeavy
```

Run:

```text
L = 0, 1, 10, 25, 100
```

Purpose: show setup/materialization cost and distinguish lookahead from logical frontier.

### Canonical equivalence cases

Always retain these even if the full matrix is filtered:

```text
F=50,U=1,L=25,D=Dense
F=50_000,U=2,L=25,D=TailHeavy
F=50_000,U=10,L=25,D=UniformSparse
F=500_000,U=2,L=25,D=AdversarialStopGap
```

### Quick suite

The default developer run should contain:

```text
F = 50, 5_000, 50_000
U = 2, 10
L = 0, 25
D = Dense, TailHeavy
```

It should exclude `F=500_000` dense/revealed and SQLite file benchmarks unless explicitly filtered.

### Full research suite

The full suite should run the controlled one-factor experiments, all five distributions, cache on/off where meaningful, both keychain shapes, and the canonical `F=500_000` cases. It should be invoked by explicit command/filter, never as an accidental default `cargo bench`.

---

## 11. Benchmark naming

Use group names for the operation and IDs for all case dimensions:

```text
<strategy>/F=<frontier>/R=<revealed>/U=<used>/L=<lookahead>/D=<distribution>/cache=<on|off>/keys=<shape>
```

Examples:

```text
sparse/F=50000/R=50001/U=2/L=25/D=tail/cache=off/keys=external
sparse/F=50000/R=50001/U=10/L=25/D=uniform/cache=on/keys=external
 dense/F=50000/R=50001/U=2/L=25/D=tail/cache=off/keys=external
build-known-used/F=50000/R=50001/U=10/L=25/D=tail/cache=off/keys=two
build-revealed/F=50000/R=50001/U=10/L=25/D=tail/cache=off/keys=two
apply-high/F=50000/R=50001/U=2/L=25/D=tail/irrelevant=10
codec-ef-used/F=50000/R=50001/U=100/L=25/D=clustered
reload-sqlite/F=50000/R=50001/U=2/L=25/D=tail/cache=on
```

Criterion renders the group and ID in reports. The strings are deliberately verbose because the raw result must remain interpretable after copying out of `target/criterion`.

A helper should construct IDs in one place so spelling and field order cannot drift.

---

## 12. Runtime expectations and configuration

These are qualitative expectations, not timing claims.

| Group | Expected scale | Reason |
|---|---|---|
| pure metadata/varint encoding | microseconds to milliseconds | work follows `U` and payload size |
| sparse restore | microseconds to milliseconds | bounded setup plus `U` materialization |
| known-used request | microseconds to milliseconds | request count follows `U` |
| dense restore/materialization | milliseconds to seconds | work follows `F` |
| revealed request | milliseconds to seconds | iterator construction/consumption follows `F` |
| synthetic update | microseconds to milliseconds | depends on graph/output count and indexed matches |
| SQLite read/write | milliseconds or higher | transaction, serialization, and OS/database effects |
| end-to-end stop-gap/dense control | potentially very expensive | may derive/request a large frontier plus stop-gap |

### Suggested quick configuration

Use a separate quick profile or group settings approximately as follows:

```text
warm_up_time: 1 second
measurement_time: 2 seconds
sample_size: 10–20
```

The exact setting should be visible in the benchmark source and captured in provenance. This is intended for smoke validation and local iteration, not publication-quality precision.

### Suggested full configuration

```text
warm_up_time: 3 seconds
measurement_time: 5 seconds for ordinary groups
sample_size: 20–50
```

Use lower sample sizes for expensive dense/frontier cases and explicit filters. Do not force the full suite to finish under one universal duration.

### No invented results

The plan does not predict the eventual medians, speedup, or confidence intervals. Any performance conclusion must cite the retained raw run and its structural metrics.

---

## 13. Reproducibility and provenance

Every retained run must capture:

```text
bdk HEAD and branch
bdk_wallet HEAD and branch
working-tree status / source patch hash
rustc --version --verbose
cargo --version
Cargo.lock hash
Criterion version and enabled features
profile and optimization settings
OS and kernel/version
architecture
CPU model and core information
benchmark command and filters
Criterion warm-up, measurement, sample-size configuration
fixture matrix and distribution seed/version
```

### What Criterion already stores

Criterion stores benchmark IDs, sample measurements, iteration counts, estimates, outlier/statistical data, baseline comparison data, and reports under `target/criterion`. `raw.csv` is the preferred machine-readable timing artifact; JSON files and HTML are useful but JSON formats are internal implementation details.

### What must be captured separately

Criterion does not prove the two repository commits, feature flags, local path patches, CPU state, or fixture generator version. Capture these in:

```text
benchmark-results/<run-id>/PROVENANCE.md
benchmark-results/<run-id>/STRUCTURAL_METRICS.csv
benchmark-results/<run-id>/criterion-output.txt
benchmark-results/<run-id>/source.patch
```

The run ID should include a UTC timestamp and short commit identifiers, for example:

```text
2026-08-03T120000Z-bdk03df61e-bw51dedcc
```

The raw Criterion tree should be copied beneath the run directory without rewriting IDs. Preserve the exact command line and whether the run was quick or full.

### Environment controls

The report should record whether the machine was idle, whether CPU scaling/thermal throttling was enabled, and whether SQLite used an in-memory database, a filesystem temporary database, or a persistent file. These facts affect interpretation and must not be hidden.

---

## 14. Criterion output and research artifacts

Expected Criterion structure for one benchmark name:

```text
target/criterion/<group>/<id>/
    new/
        raw.csv
        estimates.json
        sample.json
        tukey.json
    base/                 # when a Criterion baseline exists
        ...
    change/               # when compared with a baseline
        estimates.json
    report/
        index.html
        *.svg
```

Preserve at minimum:

```text
raw.csv
estimates.json
sample.json
tukey.json when present
report/index.html and plots when useful
criterion-output.txt
STRUCTURAL_METRICS.csv
PROVENANCE.md
source.patch and its SHA-256
```

Do not depend on HTML alone. Do not parse private JSON as the only long-term data source; use `raw.csv` plus the companion provenance/metrics CSV.

The benchmark implementation should not write research artifacts from inside timed closures. Artifact collection belongs in a wrapper or post-run command so filesystem writes do not contaminate measurements.

---

## 15. What each family can and cannot conclude

| Family | Can conclude | Cannot conclude |
|---|---|---|
| sparse restore | cost of exact metadata application under specified `F/U/L/D/cache` | network recovery time, all-wallet startup time, or arbitrary generic metadata semantics |
| dense control | cost of the actual eager BDK frontier path | that dense is a universal stop-gap implementation or that both paths have equal physical work |
| materialization | public reconstruction-path cost and physical count differences | private helper cost not exposed by the API |
| known-used request | cost/cardinality of exact used-SPK request construction/consumption | backend request latency or server-side search cost |
| revealed request | frontier-proportional request construction/consumption | proof that revealed-range behavior is wrong; it is current compatibility behavior |
| update application | cost of applying the same deterministic graph/output update | network fetch, transaction validation outside the supplied update, or general wallet sync performance |
| persistence/reload | measured in-memory or SQLite operation under a declared boundary | full application startup unless connection/file-open work is included |
| codec | encoding/decoding time and size for named representations | wallet restoration cost, cryptographic security, or generic protocol adoption |
| end-to-end | offline CPU/state pipeline under equal logical postconditions | production speedup across backends, machines, or network conditions |

No result should be called a speedup unless:

1. the sparse and comparison states have equal descriptors, frontier, keychains, and known-used facts;
2. the resulting ownership/update state is checked;
3. requested/materialized/derived work counts are reported;
4. the timed boundaries are the same kind of boundary;
5. the raw Criterion intervals and outliers are retained.

---

## 16. Proposed repository diff

This is the planned implementation diff, not a change made in this planning pass.

```text
bdk_wallet/
├── Cargo.toml                         # add direct Criterion 0.5.1 dev dependency and bench target
├── Cargo.lock                         # expected dependency lock update during implementation
└── benches/
    └── descriptor_metadata/
        ├── main.rs                    # criterion_group!/criterion_main!, group registration
        ├── fixture.rs                  # BenchCase, distributions, descriptors, updates
        ├── metrics.rs                  # structural metrics and CSV-row representation
        ├── ids.rs                      # stable BenchmarkId formatting
        ├── restore.rs                  # sparse and dense restore groups
        ├── materialization.rs          # public changeset/reconstruction groups
        ├── request.rs                  # build and consume request groups
        ├── update.rs                   # deterministic Wallet::apply_update groups
        ├── persistence.rs              # changeset and SQLite groups
        ├── codec.rs                    # EF, transport, and baseline codecs
        └── end_to_end.rs               # offline metadata/dense/stop-gap pipelines
```

Proposed manifest shape, for approval only:

```toml
[dev-dependencies]
criterion = { version = "0.5.1", features = ["html_reports"] }

[[bench]]
name = "descriptor_metadata"
path = "benches/descriptor_metadata/main.rs"
harness = false
```

No new benchmark target is proposed in `bdk` for the first implementation. Existing `bdk` benchmark files remain unchanged. This keeps one source of fixture truth and avoids changing the old Criterion `0.2.11` harness.

If maintainers require chain-local ownership later, the alternative diff would add `crates/chain/benches/descriptor_metadata.rs` and decide separately whether to retain or upgrade its Criterion dependency. That alternative is not the recommended first implementation.

---

## 17. Implementation sequence

Every step should leave an independently runnable target. The commands below are planned commands, not executed in this pass.

### Step 1 — Minimal Criterion target

**Files:** `bdk_wallet/Cargo.toml`, `benches/descriptor_metadata/main.rs`.

**Behavior:** one fixed pure benchmark and one fixed no-op/validation target; register groups with `criterion_group!` and `criterion_main!`.

**Run:**

```text
cargo bench --bench descriptor_metadata -- --quick
```

If Criterion does not provide a `--quick` flag in the selected version, use a named filter and the configured quick profile.

**Verify:** target compiles, one result appears under `target/criterion`, and the output identifies the expected group/ID.

### Step 2 — Shared deterministic fixtures

**Files:** `fixture.rs`, `ids.rs`.

**Behavior:** generate and validate canonical cases and distributions; no benchmark timing yet.

**Run:** compile the bench target and add unit-style fixture assertions if supported by the bench crate.

**Verify:** exact indexes, `R=F+1`, sorted/unique `U`, seed, and expected count are printed or asserted.

### Step 3 — Sparse restore and dense control

**Files:** `restore.rs`.

**Behavior:** add low-level public indexer cases and wallet-level dense/sparse cases with `iter_batched` reset.

**Run:** filter to `restore` and one canonical ID.

**Verify:** separate untimed postcondition checks show sparse materialization is bounded and dense materialization reaches the frontier.

### Step 4 — Structural metrics

**Files:** `metrics.rs`, group modules.

**Behavior:** collect materialized/requested/changeset/codec metrics outside timed closures.

**Run:** canonical restore and request filters.

**Verify:** metrics rows match independently asserted counts; no metrics file write occurs inside a timed closure.

### Step 5 — Frontier and used-count sweeps

**Files:** `restore.rs`, `ids.rs`.

**Behavior:** add one-factor sweeps for `F` and `U`, retaining canonical IDs.

**Run:** explicit filters for frontier or used-count experiments.

**Verify:** `F=500_000` is not included in the default quick command.

### Step 6 — Request construction and consumption

**Files:** `request.rs`.

**Behavior:** benchmark known-used and revealed request construction separately from request queue consumption.

**Run:** filter `request/build` or `request/consume`.

**Verify:** known-used request cardinality equals `U`; revealed request cardinality follows `R` and no network code runs.

### Step 7 — Update application

**Files:** `update.rs`.

**Behavior:** apply deterministic high-index/mixed/irrelevant-output updates to fresh sparse and control wallets.

**Run:** filter `update` and one irrelevant-output count.

**Verify:** ownership and graph outcomes match expected state after each operation.

### Step 8 — Persistence/reload

**Files:** `persistence.rs`.

**Behavior:** add in-memory changeset reconstruction, SQLite write, SQLite read, and wallet-load cases with explicit file/connection boundaries.

**Run:** filter one persistence operation at a time.

**Verify:** reload state and structural metrics match the source snapshot; identify whether open cost is included.

### Step 9 — Codec benchmarks

**Files:** `codec.rs`.

**Behavior:** measure actual BDK metadata codecs and clearly labeled research-only baselines.

**Run:** filter `codec` and one `U`/distribution case.

**Verify:** every round trip is asserted outside timing and encoded byte lengths are recorded.

### Step 10 — End-to-end scaffold

**Files:** `end_to_end.rs`.

**Behavior:** add metadata-assisted, dense-frontier, and stop-gap-model offline pipelines.

**Run:** canonical end-to-end filter only; full stop-gap cases remain manual.

**Verify:** all variants produce equal logical postconditions before comparing timing.

### Step 11 — Provenance capture and retained run

**Files:** research artifact directory outside source benchmark code; optionally a later wrapper script.

**Behavior:** copy Criterion output, raw CSV, structural metrics, environment, commands, and source patch hash.

**Run:** explicit quick/full command with a run ID.

**Verify:** another operator can identify both commits, exact fixture, exact command, and raw data without reading unpinned source.

---

## 18. Approval decisions before implementation

# Decisions Before Implementation

Please approve or revise each item:

- [ ] **Benchmark location:** one cross-repository research harness under `bdk_wallet/benches/descriptor_metadata/`, using public `bdk_chain` APIs; leave existing `bdk` benches unchanged.
- [ ] **Criterion version:** direct Criterion `0.5.1` with `html_reports` in `bdk_wallet`; retain old `bdk_chain` Criterion `0.2.11` for historical benches.
- [ ] **Fixture representation:** `BenchCase` keeps explicit `F`, `R`, exact `used_indexes`, `U` derived from the vector, `L`, `D`, cache mode, and keychain shape.
- [ ] **BDK projection rule:** enforce `R=F+1` for BDK cases while retaining `R` in the generic research fixture model.
- [ ] **Distributions:** Dense, UniformSparse, Clustered, TailHeavy, and AdversarialStopGap with deterministic formulas/fixed recorded seed.
- [ ] **Reset strategy:** `iter_batched` for mutable indexer/wallet/update operations; fresh request per request-consumption iteration; limited `PerIteration` use for SQLite resources.
- [ ] **Dense control:** use actual `reveal_to_target` and wallet `apply_spk_metadata`/`reveal_addresses_to`, not a synthetic loop.
- [ ] **Instrumentation:** use public counts and validation passes; do not add production counters solely for benchmarking.
- [ ] **Codec scope:** actual BDK EF/base64/bech32 paths plus explicitly labeled raw/delta/bitmap research baselines; synthetic EF revealed is not presented as BDK wire behavior.
- [ ] **Persistence scope:** separate changeset reconstruction, SQLite write, SQLite read, and wallet load; explicitly exclude or include database-open cost per ID.
- [ ] **End-to-end scope:** offline deterministic metadata-assisted, dense-frontier, and stop-gap-model pipelines; no network I/O.
- [ ] **Parameter matrix:** factor-at-a-time sweeps plus canonical cases, not the full Cartesian product; `F=500_000` manual/full only.
- [ ] **Naming:** dimension-complete group/BenchmarkId strings such as `sparse/F=50000/R=50001/U=2/L=25/D=tail/cache=off/keys=external`.
- [ ] **Quick/full profiles:** short developer profile versus explicit full research profile with expensive cases filtered.
- [ ] **Raw results:** preserve Criterion `raw.csv`, reports, command output, structural metrics, provenance, and source patch under `benchmark-results/<run-id>/` after implementation.
- [ ] **Equivalence gate:** no speedup claim until logical state, ownership/update behavior, requested/materialized counts, and timed boundaries are shown equivalent.

---

## Final recommendation

Approve the single `bdk_wallet` research harness, Criterion `0.5.1`, public low-level chain API coverage, explicit mutable-state reset, separate structural metrics, and offline end-to-end controls. This produces one consistent benchmark vocabulary while preserving the distinction central to the validated implementation:

```text
logical frontier != physical SPK materialization != historical known-used state
```

No benchmark implementation should be written until the approval checklist is resolved.
