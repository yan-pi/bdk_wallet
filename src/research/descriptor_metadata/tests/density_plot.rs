use descriptor_metadata_research::density_plot::{
    criterion_estimate, median_series, DensityPoint, Series,
};

fn point(series: Series, density: u8, layout: u8, time_ns: f64, complete: bool) -> DensityPoint {
    DensityPoint {
        series,
        density_percent: density,
        layout,
        time_ns,
        complete,
    }
}

#[test]
fn criterion_estimate_prefers_slope_and_falls_back_to_mean() {
    let slope = r#"{
        "mean":{"confidence_interval":{"lower_bound":90.0,"upper_bound":110.0},"point_estimate":100.0},
        "slope":{"confidence_interval":{"lower_bound":180.0,"upper_bound":220.0},"point_estimate":200.0}
    }"#;
    let mean = r#"{
        "mean":{"confidence_interval":{"lower_bound":90.0,"upper_bound":110.0},"point_estimate":100.0},
        "slope":null
    }"#;

    assert_eq!(criterion_estimate(slope).expect("slope must parse"), 200.0);
    assert_eq!(criterion_estimate(mean).expect("mean must parse"), 100.0);
}

#[test]
fn descriptor_median_excludes_incomplete_stops() {
    let points = vec![
        point(Series::StopGap, 5, 0, 1_000.0, false),
        point(Series::StopGap, 5, 1, 3_000.0, true),
        point(Series::SparseApi, 5, 0, 2_000.0, true),
    ];

    assert_eq!(median_series(&points, Series::StopGap), vec![(5, 3_000.0)]);
}
