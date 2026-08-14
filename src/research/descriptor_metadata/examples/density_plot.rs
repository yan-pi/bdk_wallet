use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use descriptor_metadata_research::{
    density::{benchmark_parameter, sequential_stop_gap_complete, DensityConfig},
    density_plot::{criterion_estimate, median_series, DensityPoint, Series},
};
use plotters::{coord::types::RangedCoordf64, prelude::*};

fn main() -> ExitCode {
    match run() {
        Ok(path) => {
            println!("Density scatter written to {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("density plot failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<PathBuf, String> {
    let config = config()?;
    let configured_target = env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"));
    let target_dir = if configured_target.is_absolute() {
        configured_target
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(configured_target)
    };
    let points = load_points(&target_dir, config)?;
    let output = target_dir.join("density-scatter.svg");
    render_svg(&output, &points, config)?;
    Ok(output)
}

fn config() -> Result<DensityConfig, String> {
    DensityConfig::new(
        env_value("DENSITY_INDEX_COUNT", 1_000)?,
        env_value("DENSITY_STEP", 5)?,
        env_value("DENSITY_LAYOUTS", 5)?,
        env_value("DENSITY_STOP_GAP", 20)?,
        env_value("DENSITY_BASE_SEED", 0x5eed)?,
    )
    .map_err(|error| error.to_string())
}

fn env_value<T>(name: &str, default: T) -> Result<T, String>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    env::var(name).map_or(Ok(default), |value| {
        value
            .parse::<T>()
            .map_err(|error| format!("invalid {name}={value}: {error}"))
    })
}

fn load_points(target_dir: &Path, config: DensityConfig) -> Result<Vec<DensityPoint>, String> {
    config
        .fixtures()
        .into_iter()
        .flat_map(|fixture| {
            [Series::SparseApi, Series::StopGap]
                .into_iter()
                .map(move |series| (fixture.clone(), series))
        })
        .map(|(fixture, series)| {
            let parameter = benchmark_parameter(&fixture);
            let estimates_path = target_dir.join(format!(
                "criterion/spk_derivation_by_density/{}/{parameter}/new/estimates.json",
                series.id()
            ));
            let estimates = fs::read_to_string(&estimates_path).map_err(|error| {
                format!(
                    "missing Criterion result {}: {error}; run density_sweep first with the same configuration",
                    estimates_path.display()
                )
            })?;
            Ok(DensityPoint {
                series,
                density_percent: fixture.density_percent(),
                layout: fixture.layout(),
                time_ns: criterion_estimate(&estimates)?,
                complete: series == Series::SparseApi
                    || sequential_stop_gap_complete(fixture.used_indexes(), config.stop_gap()),
            })
        })
        .collect()
}

fn render_svg(output: &Path, points: &[DensityPoint], config: DensityConfig) -> Result<(), String> {
    let limits = points
        .iter()
        .map(|point| point.time_ns)
        .fold(None::<(f64, f64)>, |limits, value| {
            Some(match limits {
                Some((minimum, maximum)) => (minimum.min(value), maximum.max(value)),
                None => (value, value),
            })
        })
        .ok_or_else(|| "no Criterion points to render".to_owned())?;
    let root = SVGBackend::new(output, (1_440, 900)).into_drawing_area();
    root.fill(&RGBColor(248, 246, 240))
        .map_err(|error| error.to_string())?;
    let mut chart = ChartBuilder::on(&root)
        .caption(
            format!(
                "Local SPK processing by exact density (n={}, stop-gap={})",
                config.index_count(),
                config.stop_gap()
            ),
            ("sans-serif", 36).into_font(),
        )
        .margin(30)
        .x_label_area_size(70)
        .y_label_area_size(110)
        .build_cartesian_2d(
            0f64..100f64,
            ((limits.0 / 2.0).max(1.0)..limits.1 * 2.0).log_scale(),
        )
        .map_err(|error| error.to_string())?;
    chart
        .configure_mesh()
        .x_desc("Exact used-index density (%)")
        .y_desc("Criterion local API time (no network)")
        .x_labels(11)
        .y_label_formatter(&|nanoseconds| format_duration(*nanoseconds))
        .axis_desc_style(("sans-serif", 22))
        .label_style(("sans-serif", 17))
        .draw()
        .map_err(|error| error.to_string())?;

    draw_sparse(&mut chart, points, config.layouts_per_density())?;
    draw_stop_gap(&mut chart, points, config.layouts_per_density())?;
    chart
        .configure_series_labels()
        .position(SeriesLabelPosition::UpperLeft)
        .background_style(RGBColor(248, 246, 240).mix(0.92))
        .border_style(RGBColor(75, 79, 80))
        .label_font(("sans-serif", 18))
        .draw()
        .map_err(|error| error.to_string())?;
    root.present().map_err(|error| error.to_string())
}

fn draw_sparse<DB: DrawingBackend>(
    chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, LogCoord<f64>>>,
    points: &[DensityPoint],
    layout_count: u8,
) -> Result<(), String> {
    let color = RGBColor(31, 122, 90);
    chart
        .draw_series(
            points
                .iter()
                .filter(|point| point.series == Series::SparseApi)
                .map(|point| {
                    Circle::new(
                        (x_position(point, layout_count), point.time_ns),
                        4,
                        color.mix(0.45).filled(),
                    )
                }),
        )
        .map_err(|error| error.to_string())?
        .label(Series::SparseApi.label())
        .legend(move |(x, y)| Circle::new((x + 10, y), 5, color.filled()));
    draw_median(chart, points, Series::SparseApi, color)
}

fn draw_stop_gap<DB: DrawingBackend>(
    chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, LogCoord<f64>>>,
    points: &[DensityPoint],
    layout_count: u8,
) -> Result<(), String> {
    let complete_color = RGBColor(213, 128, 36);
    chart
        .draw_series(
            points
                .iter()
                .filter(|point| point.series == Series::StopGap && point.complete)
                .map(|point| {
                    TriangleMarker::new(
                        (x_position(point, layout_count), point.time_ns),
                        6,
                        complete_color.mix(0.55).filled(),
                    )
                }),
        )
        .map_err(|error| error.to_string())?
        .label("Sequential stop-gap (complete)")
        .legend(move |(x, y)| TriangleMarker::new((x + 10, y), 6, complete_color.filled()));
    draw_median(chart, points, Series::StopGap, complete_color)?;

    let incomplete_color = RGBColor(177, 49, 46);
    chart
        .draw_series(
            points
                .iter()
                .filter(|point| point.series == Series::StopGap && !point.complete)
                .map(|point| {
                    Cross::new(
                        (x_position(point, layout_count), point.time_ns),
                        8,
                        ShapeStyle::from(&incomplete_color).stroke_width(3),
                    )
                }),
        )
        .map_err(|error| error.to_string())?
        .label("Sequential stop-gap (incomplete)")
        .legend(move |(x, y)| {
            Cross::new(
                (x + 10, y),
                7,
                ShapeStyle::from(&incomplete_color).stroke_width(3),
            )
        });
    Ok(())
}

fn draw_median<DB: DrawingBackend>(
    chart: &mut ChartContext<'_, DB, Cartesian2d<RangedCoordf64, LogCoord<f64>>>,
    points: &[DensityPoint],
    series: Series,
    color: RGBColor,
) -> Result<(), String> {
    chart
        .draw_series(LineSeries::new(
            median_series(points, series)
                .into_iter()
                .map(|(density, estimate)| (f64::from(density), estimate)),
            ShapeStyle::from(&color).stroke_width(3),
        ))
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn x_position(point: &DensityPoint, layout_count: u8) -> f64 {
    if point.density_percent == 0 || point.density_percent == 100 {
        return f64::from(point.density_percent);
    }
    let midpoint = f64::from(layout_count.saturating_sub(1)) / 2.0;
    f64::from(point.density_percent) + (f64::from(point.layout) - midpoint) * 0.32
}

fn format_duration(nanoseconds: f64) -> String {
    if nanoseconds >= 1_000_000_000.0 {
        format!("{:.1} s", nanoseconds / 1_000_000_000.0)
    } else if nanoseconds >= 1_000_000.0 {
        format!("{:.1} ms", nanoseconds / 1_000_000.0)
    } else if nanoseconds >= 1_000.0 {
        format!("{:.0} us", nanoseconds / 1_000.0)
    } else {
        format!("{nanoseconds:.0} ns")
    }
}
