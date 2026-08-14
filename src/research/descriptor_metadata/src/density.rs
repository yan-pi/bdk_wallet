//! Deterministic exact-density fixtures for the secondary sensitivity study.

use std::collections::BTreeSet;

use bdk_wallet::{chain::spk_client::FullScanRequest, KeychainKind, SpkMetadata, SpkMetadataError};

/// Result of sequential descriptor derivation with a fixed stop-gap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StopGapObservation {
    /// Whether every known-used index was reached.
    pub complete: bool,
    /// Number of descriptor SPKs derived before stopping.
    pub derived_spks: usize,
    /// Sorted indexes reported used by the deterministic oracle.
    pub discovered_indexes: Vec<u32>,
}

/// Configuration for an exact-density fixture sweep.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DensityConfig {
    index_count: u32,
    density_step: u8,
    layouts_per_density: u8,
    stop_gap: usize,
    base_seed: u64,
}

/// Invalid exact-density configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DensityConfigError {
    /// Index count must support every exact whole percentage.
    IndexCountNotPercentageAligned(u32),
    /// Density step must be non-zero and divide 100.
    InvalidDensityStep(u8),
    /// Intermediate densities require at least one layout.
    NoLayouts,
    /// Stop-gap must be non-zero.
    ZeroStopGap,
}

impl core::fmt::Display for DensityConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::IndexCountNotPercentageAligned(index_count) => write!(
                f,
                "index count {index_count} must be divisible by 100 for exact densities"
            ),
            Self::InvalidDensityStep(step) => {
                write!(f, "density step {step} must be non-zero and divide 100")
            }
            Self::NoLayouts => write!(f, "layouts per density must be non-zero"),
            Self::ZeroStopGap => write!(f, "stop-gap must be non-zero"),
        }
    }
}

impl std::error::Error for DensityConfigError {}

/// One deterministic exact-density wallet snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DensityFixture {
    density_percent: u8,
    layout: u8,
    seed: u64,
    index_count: u32,
    used_indexes: Vec<u32>,
}

impl DensityConfig {
    /// Construct a validated exact-density configuration.
    pub fn new(
        index_count: u32,
        density_step: u8,
        layouts_per_density: u8,
        stop_gap: usize,
        base_seed: u64,
    ) -> Result<Self, DensityConfigError> {
        if index_count == 0 || index_count / 100 * 100 != index_count {
            return Err(DensityConfigError::IndexCountNotPercentageAligned(
                index_count,
            ));
        }
        if density_step == 0 || 100 / density_step * density_step != 100 {
            return Err(DensityConfigError::InvalidDensityStep(density_step));
        }
        if layouts_per_density == 0 {
            return Err(DensityConfigError::NoLayouts);
        }
        if stop_gap == 0 {
            return Err(DensityConfigError::ZeroStopGap);
        }

        Ok(Self {
            index_count,
            density_step,
            layouts_per_density,
            stop_gap,
            base_seed,
        })
    }

    /// Return the number of indexes represented by each fixture.
    pub const fn index_count(self) -> u32 {
        self.index_count
    }

    /// Return the exact percentage interval.
    pub const fn density_step(self) -> u8 {
        self.density_step
    }

    /// Return layout count for intermediate densities.
    pub const fn layouts_per_density(self) -> u8 {
        self.layouts_per_density
    }

    /// Return the backend-neutral sequential stop-gap.
    pub const fn stop_gap(self) -> usize {
        self.stop_gap
    }

    /// Return the published deterministic base seed.
    pub const fn base_seed(self) -> u64 {
        self.base_seed
    }

    /// Iterate over every configured density, including endpoints.
    pub fn densities(self) -> impl Iterator<Item = u8> {
        (0..=100).step_by(usize::from(self.density_step))
    }

    /// Generate every configured exact-density fixture.
    pub fn fixtures(self) -> Vec<DensityFixture> {
        self.densities()
            .flat_map(|density_percent| {
                let layouts = if density_percent == 0 || density_percent == 100 {
                    1
                } else {
                    self.layouts_per_density
                };
                (0..layouts).map(move |layout| {
                    DensityFixture::new(
                        density_percent,
                        layout,
                        self.index_count,
                        self.layout_seed(density_percent, layout),
                    )
                })
            })
            .collect()
    }

    fn layout_seed(self, density_percent: u8, layout: u8) -> u64 {
        self.base_seed ^ (u64::from(density_percent) << 32) ^ u64::from(layout)
    }
}

impl DensityFixture {
    fn new(density_percent: u8, layout: u8, index_count: u32, seed: u64) -> Self {
        let used_count = index_count as usize * usize::from(density_percent) / 100;
        let mut indexes = (0..index_count).collect::<Vec<_>>();
        shuffle(&mut indexes, seed);
        indexes.truncate(used_count);
        indexes.sort_unstable();

        Self {
            density_percent,
            layout,
            seed,
            index_count,
            used_indexes: indexes,
        }
    }

    /// Return exact used-index density.
    pub const fn density_percent(&self) -> u8 {
        self.density_percent
    }

    /// Return stable layout number within this density.
    pub const fn layout(&self) -> u8 {
        self.layout
    }

    /// Return the deterministic seed for this layout.
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// Return the number of indexes represented by this snapshot.
    pub const fn index_count(&self) -> u32 {
        self.index_count
    }

    /// Return the fixed logical frontier.
    pub const fn last_revealed(&self) -> u32 {
        self.index_count - 1
    }

    /// Return sorted exact-cardinality used indexes.
    pub fn used_indexes(&self) -> &[u32] {
        &self.used_indexes
    }

    /// Build validated sparse metadata for this snapshot.
    pub fn metadata(&self) -> Result<SpkMetadata, SpkMetadataError> {
        SpkMetadata::with_last_revealed(
            KeychainKind::External,
            Some(self.last_revealed()),
            self.used_indexes.clone(),
        )
    }
}

/// Return stable Criterion parameter identity.
pub fn benchmark_parameter(fixture: &DensityFixture) -> String {
    format!(
        "density_{:03}_layout_{:02}",
        fixture.density_percent, fixture.layout
    )
}

/// Derive SPKs sequentially until `stop_gap` consecutive indexes are unused.
pub fn consume_sequential_stop_gap(
    mut request: FullScanRequest<KeychainKind>,
    used_indexes: &[u32],
    stop_gap: usize,
) -> StopGapObservation {
    let used_indexes = used_indexes.iter().copied().collect::<BTreeSet<_>>();
    let expected_indexes = used_indexes.iter().copied().collect::<Vec<_>>();
    let mut consecutive_unused = 0usize;
    let mut derived_spks = 0usize;
    let mut discovered_indexes = Vec::new();

    for (index, spk) in request.iter_spks(KeychainKind::External) {
        std::hint::black_box(spk);
        derived_spks = derived_spks.saturating_add(1);
        if used_indexes.contains(&index) {
            consecutive_unused = 0;
            discovered_indexes.push(index);
        } else {
            consecutive_unused = consecutive_unused.saturating_add(1);
        }
        if consecutive_unused >= stop_gap {
            break;
        }
    }

    StopGapObservation {
        complete: discovered_indexes == expected_indexes,
        derived_spks,
        discovered_indexes,
    }
}

/// Return whether sequential stop-gap reaches every known-used index.
pub fn sequential_stop_gap_complete(used_indexes: &[u32], stop_gap: usize) -> bool {
    let mut next_expected = 0usize;
    let mut consecutive_unused = 0usize;

    for index in 0.. {
        if used_indexes.get(next_expected) == Some(&index) {
            next_expected += 1;
            consecutive_unused = 0;
        } else {
            consecutive_unused = consecutive_unused.saturating_add(1);
        }
        if consecutive_unused >= stop_gap {
            return next_expected == used_indexes.len();
        }
    }
    unreachable!("sequential stop-gap must terminate")
}

fn shuffle<T>(values: &mut [T], seed: u64) {
    let mut state = seed;
    for index in (1..values.len()).rev() {
        let other = splitmix64(&mut state) as usize % (index + 1);
        values.swap(index, other);
    }
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
