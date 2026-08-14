//! Pure adapters from Criterion estimates to density-series points.

use serde::Deserialize;

/// Series shown in the primary density scatter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Series {
    /// Backend-neutral sequential descriptor stop-gap derivation.
    StopGap,
    /// Exact sparse metadata API path.
    SparseApi,
}

impl Series {
    /// Stable Criterion function identifier.
    pub const fn id(self) -> &'static str {
        match self {
            Self::StopGap => "descriptor_stop_gap",
            Self::SparseApi => "sparse_api",
        }
    }

    /// Human-readable chart label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::StopGap => "Sequential descriptor stop-gap",
            Self::SparseApi => "Sparse metadata API",
        }
    }
}

/// One layout estimate read from Criterion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DensityPoint {
    /// Measured series.
    pub series: Series,
    /// Exact used-index density.
    pub density_percent: u8,
    /// Deterministic layout number.
    pub layout: u8,
    /// Criterion point estimate in nanoseconds.
    pub time_ns: f64,
    /// Whether stop-gap derivation discovered every known-used index.
    pub complete: bool,
}

#[derive(Deserialize)]
struct Estimates {
    mean: Estimate,
    slope: Option<Estimate>,
}

#[derive(Deserialize)]
struct Estimate {
    point_estimate: f64,
}

/// Parse Criterion's typical estimate, preferring slope when available.
pub fn criterion_estimate(json: &str) -> Result<f64, String> {
    let estimates = serde_json::from_str::<Estimates>(json)
        .map_err(|error| format!("invalid Criterion estimates: {error}"))?;
    let estimate = estimates.slope.unwrap_or(estimates.mean).point_estimate;
    if !estimate.is_finite() || estimate <= 0.0 {
        return Err("Criterion estimate must be positive and finite".to_owned());
    }
    Ok(estimate)
}

/// Return successful median estimates by density for one series.
pub fn median_series(points: &[DensityPoint], series: Series) -> Vec<(u8, f64)> {
    (0..=100)
        .filter_map(|density_percent| {
            let mut estimates = points
                .iter()
                .filter(|point| {
                    point.series == series
                        && point.density_percent == density_percent
                        && point.complete
                })
                .map(|point| point.time_ns)
                .collect::<Vec<_>>();
            if estimates.is_empty() {
                return None;
            }
            estimates.sort_by(f64::total_cmp);
            let middle = estimates.len() / 2;
            Some((
                density_percent,
                if estimates.len() % 2 == 0 {
                    (estimates[middle - 1] + estimates[middle]) / 2.0
                } else {
                    estimates[middle]
                },
            ))
        })
        .collect()
}
