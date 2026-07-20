from __future__ import annotations

# /// script
# dependencies = ["matplotlib", "numpy", "pandas"]
# ///

from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "output"
CSV = OUTPUT / "results.csv"
PROBABILITY_CSV = OUTPUT / "probability_results.csv"

SCENARIO_ORDER = [
    "dense_contiguous_10",
    "dense_contiguous_100",
    "dense_contiguous_1000",
    "periodic_10_1000",
    "periodic_100_10000",
    "sparse_small",
    "sparse_large",
    "pathological_high",
    "empty",
]

SPARSE_SCENARIOS = [
    "periodic_100_10000",
    "sparse_small",
    "sparse_large",
    "pathological_high",
]

STRATEGIES = [
    ("full_scan_requested_spks", "full scan", "#1f77b4"),
    ("revealed_sync_spks", "metadata + revealed sync", "#ff7f0e"),
    ("sparse_sync_spks", "sparse metadata sync", "#2ca02c"),
]

TIMING_STRATEGIES = [
    ("full_scan_elapsed_ns", "full scan", "#1f77b4"),
    ("revealed_sync_elapsed_ns", "metadata + revealed sync", "#ff7f0e"),
    ("sparse_sync_elapsed_ns", "sparse metadata sync", "#2ca02c"),
]

PROBABILITY_STRATEGIES = [
    ("full_scan_stop_gap", "full scan stop-gap", "#1f77b4"),
    ("metadata_revealed_sync", "metadata + revealed sync", "#ff7f0e"),
    ("sparse_metadata_sync", "sparse metadata sync", "#2ca02c"),
]


def load_results() -> pd.DataFrame:
    df = pd.read_csv(CSV)
    df["scenario"] = pd.Categorical(
        df["scenario"], categories=SCENARIO_ORDER, ordered=True
    )
    df["discovery_ratio"] = df.apply(
        lambda row: (
            1.0
            if row["used_count"] == 0
            else row["full_scan_discovered_count"] / row["used_count"]
        ),
        axis=1,
    )
    return df.sort_values("scenario")


def representative_rows(
    df: pd.DataFrame, *, stop_gap: int, parallel_requests: int
) -> pd.DataFrame:
    return df[
        (df["stop_gap"] == stop_gap) & (df["parallel_requests"] == parallel_requests)
    ].sort_values("scenario")


def non_empty(rows: pd.DataFrame) -> pd.DataFrame:
    return rows[rows["used_count"] > 0]


def save(fig: plt.Figure, filename: str, generated: list[Path]) -> None:
    path = OUTPUT / filename
    fig.tight_layout()
    fig.savefig(path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    generated.append(path)


def plot_grouped_spk_counts(
    rows: pd.DataFrame,
    *,
    title: str,
    filename: str,
    generated: list[Path],
) -> None:
    labels = rows["scenario"].astype(str).tolist()
    x = np.arange(len(rows))
    width = 0.24

    fig, ax = plt.subplots(figsize=(15, 7))
    for offset_index, (column, label, color) in enumerate(STRATEGIES):
        offset = (offset_index - 1) * width
        values = rows[column].astype(float).replace(0, np.nan)
        bars = ax.bar(x + offset, values, width=width, label=label, color=color)

        if column == "full_scan_requested_spks":
            for bar, row in zip(bars, rows.itertuples(index=False)):
                if np.isnan(bar.get_height()):
                    continue
                ax.annotate(
                    f"{row.full_scan_discovered_count}/{row.used_count}",
                    xy=(bar.get_x() + bar.get_width() / 2, bar.get_height()),
                    xytext=(0, 4),
                    textcoords="offset points",
                    ha="center",
                    va="bottom",
                    rotation=90,
                    fontsize=8,
                )

    ax.set_yscale("log")
    ax.set_ylim(bottom=1)
    ax.set_ylabel("SPKs derived or requested (log scale)")
    ax.set_xticks(x)
    ax.set_xticklabels(labels, rotation=30, ha="right")
    ax.set_title(title)
    ax.legend(loc="upper left")
    ax.grid(axis="y", which="both", alpha=0.25)
    ax.text(
        0.99,
        0.02,
        "labels above full-scan bars show discovered used indexes / total used indexes",
        transform=ax.transAxes,
        ha="right",
        va="bottom",
        fontsize=9,
    )

    save(fig, filename, generated)


def plot_discovery_by_stop_gap(df: pd.DataFrame, generated: list[Path]) -> None:
    rows = df[
        (df["parallel_requests"] == 10)
        & (df["scenario"].astype(str).isin(SPARSE_SCENARIOS))
    ].copy()
    stop_gaps = sorted(rows["stop_gap"].unique())
    positions = np.arange(len(stop_gaps))

    fig, ax = plt.subplots(figsize=(14, 7))
    for scenario in SPARSE_SCENARIOS:
        group = rows[rows["scenario"].astype(str) == scenario].sort_values("stop_gap")
        y = [
            group[group["stop_gap"] == stop_gap]["discovery_ratio"].iloc[0]
            for stop_gap in stop_gaps
        ]
        ax.plot(positions, y, marker="o", linewidth=2, label=scenario)

    ax.axhline(1.0, color="black", linestyle="--", linewidth=1, alpha=0.45)
    ax.set_xticks(positions)
    ax.set_xticklabels([str(stop_gap) for stop_gap in stop_gaps])
    ax.set_ylim(-0.03, 1.08)
    ax.set_xlabel("stop_gap (categorical spacing)")
    ax.set_ylabel("full-scan discovered used indexes / total used indexes")
    ax.set_title("Full-scan correctness by stop-gap (parallel_requests=10)")
    ax.legend(loc="lower right")
    ax.grid(axis="y", alpha=0.25)

    save(fig, "discovery_by_stop_gap.png", generated)


def plot_time_by_density(rows: pd.DataFrame, generated: list[Path]) -> None:
    rows = non_empty(rows)

    fig, ax = plt.subplots(figsize=(14, 7))
    for column, label, color in TIMING_STRATEGIES:
        ax.scatter(
            rows["density"],
            rows[column] / 1_000_000,
            marker="o",
            s=70,
            color=color,
            label=label,
            alpha=0.85,
        )

    for row in rows.itertuples(index=False):
        ax.annotate(
            str(row.scenario).replace("_", "\n"),
            xy=(row.density, row.sparse_sync_elapsed_ns / 1_000_000),
            xytext=(4, 4),
            textcoords="offset points",
            fontsize=7,
            alpha=0.75,
        )

    ax.set_xscale("log")
    ax.set_yscale("log")
    ax.set_xlabel("density = used_count / (max_index + 1), log scale")
    ax.set_ylabel("elapsed ms (log scale)")
    ax.set_title(
        "Directional timing vs density (single sample, stop_gap=5, parallel_requests=10)"
    )
    ax.legend(loc="upper right")
    ax.grid(axis="both", which="both", alpha=0.25)

    save(fig, "time_by_density.png", generated)


def plot_time_by_scenario(rows: pd.DataFrame, generated: list[Path]) -> None:
    rows = non_empty(rows)
    labels = rows["scenario"].astype(str).tolist()
    x = np.arange(len(rows))
    width = 0.24

    fig, ax = plt.subplots(figsize=(15, 7))
    for offset_index, (column, label, color) in enumerate(TIMING_STRATEGIES):
        offset = (offset_index - 1) * width
        ax.bar(
            x + offset, rows[column] / 1_000_000, width=width, label=label, color=color
        )

    ax.set_yscale("log")
    ax.set_ylabel("elapsed ms (log scale)")
    ax.set_xticks(x)
    ax.set_xticklabels(labels, rotation=30, ha="right")
    ax.set_title(
        "Directional timing by scenario (single sample, stop_gap=5, parallel_requests=10)"
    )
    ax.legend(loc="upper left")
    ax.grid(axis="y", which="both", alpha=0.25)

    save(fig, "time_by_scenario.png", generated)


def plot_encoding_sizes(rows: pd.DataFrame, generated: list[Path]) -> None:
    labels = rows["scenario"].astype(str).tolist()
    x = np.arange(len(rows))
    width = 0.34

    fig, ax = plt.subplots(figsize=(15, 7))
    ax.bar(
        x - width / 2, rows["base64_len"], width=width, label="base64", color="#1f77b4"
    )
    ax.bar(
        x + width / 2, rows["bech32_len"], width=width, label="bech32", color="#ff7f0e"
    )

    max_height = max(rows["base64_len"].max(), rows["bech32_len"].max(skipna=True))
    marker_y = max_height * 0.08
    for index, row in enumerate(rows.itertuples(index=False)):
        if pd.isna(row.bech32_len) and pd.notna(row.bech32_error):
            ax.scatter(
                index + width / 2, marker_y, marker="x", color="red", s=70, zorder=5
            )
            ax.annotate(
                "bech32\ntoo long",
                xy=(index + width / 2, marker_y),
                xytext=(0, 8),
                textcoords="offset points",
                ha="center",
                va="bottom",
                fontsize=8,
                color="red",
            )

    ax.set_ylabel("encoded length, characters")
    ax.set_xticks(x)
    ax.set_xticklabels(labels, rotation=30, ha="right")
    ax.set_title(
        "Encoded metadata size (representative rows, stop_gap=5, parallel_requests=10)"
    )
    ax.legend(loc="upper left")
    ax.grid(axis="y", alpha=0.25)

    save(fig, "encoding_sizes.png", generated)


def plot_discovery_heatmap(df: pd.DataFrame, generated: list[Path]) -> None:
    rows = df[
        (df["parallel_requests"] == 10)
        & (df["scenario"].astype(str).isin(SPARSE_SCENARIOS))
    ].copy()
    heatmap = rows.pivot(index="scenario", columns="stop_gap", values="discovery_ratio")
    heatmap = heatmap.reindex(SPARSE_SCENARIOS)

    fig, ax = plt.subplots(figsize=(10, 5))
    image = ax.imshow(heatmap, vmin=0, vmax=1, cmap="RdYlGn", aspect="auto")

    ax.set_xticks(np.arange(len(heatmap.columns)))
    ax.set_xticklabels([str(column) for column in heatmap.columns])
    ax.set_yticks(np.arange(len(heatmap.index)))
    ax.set_yticklabels([str(index) for index in heatmap.index])
    ax.set_xlabel("stop_gap")
    ax.set_title("Full-scan discovery ratio heatmap (parallel_requests=10)")

    for y_index, scenario in enumerate(heatmap.index):
        for x_index, stop_gap in enumerate(heatmap.columns):
            value = heatmap.loc[scenario, stop_gap]
            ax.text(
                x_index,
                y_index,
                f"{value:.2f}",
                ha="center",
                va="center",
                color="black",
                fontsize=9,
            )

    fig.colorbar(image, ax=ax, label="discovery ratio")

    save(fig, "discovery_heatmap.png", generated)


def load_probability_results() -> pd.DataFrame:
    df = pd.read_csv(PROBABILITY_CSV)
    df = df[df["p_used"] > 0].copy()
    df["wallet_load_elapsed_ms"] = df["wallet_load_elapsed_ns"] / 1_000_000
    return df


def summarize_probability_results(df: pd.DataFrame) -> pd.DataFrame:
    group_columns = ["max_index", "p_used", "strategy"]
    return (
        df.groupby(group_columns, as_index=False)
        .agg(
            samples=("sample_id", "count"),
            median_load_ms=("wallet_load_elapsed_ms", "median"),
            p05_load_ms=(
                "wallet_load_elapsed_ms",
                lambda values: values.quantile(0.05),
            ),
            p95_load_ms=(
                "wallet_load_elapsed_ms",
                lambda values: values.quantile(0.95),
            ),
            median_requested_spks=("requested_spks", "median"),
            p05_requested_spks=("requested_spks", lambda values: values.quantile(0.05)),
            p95_requested_spks=("requested_spks", lambda values: values.quantile(0.95)),
            completion_rate=("complete", "mean"),
            median_actual_used_count=("actual_used_count", "median"),
        )
        .sort_values(["max_index", "p_used", "strategy"])
    )


def plot_wallet_load_time_by_probability(
    summary: pd.DataFrame, generated: list[Path]
) -> None:
    max_indexes = sorted(summary["max_index"].unique())
    fig, axes = plt.subplots(
        1,
        len(max_indexes),
        figsize=(7 * len(max_indexes), 6),
        sharey=True,
        squeeze=False,
    )

    for ax, max_index in zip(axes[0], max_indexes):
        rows = summary[summary["max_index"] == max_index]
        for strategy, label, color in PROBABILITY_STRATEGIES:
            group = rows[rows["strategy"] == strategy].sort_values("p_used")
            if group.empty:
                continue

            x = group["p_used"].to_numpy(dtype=float)
            median = group["median_load_ms"].to_numpy(dtype=float)
            p05 = group["p05_load_ms"].to_numpy(dtype=float)
            p95 = group["p95_load_ms"].to_numpy(dtype=float)
            completion = group["completion_rate"].to_numpy(dtype=float)

            ax.plot(x, median, marker="o", linewidth=2, color=color, label=label)
            ax.fill_between(x, p05, p95, color=color, alpha=0.12)

            incomplete = completion < 0.999
            if incomplete.any():
                ax.scatter(
                    x[incomplete],
                    median[incomplete],
                    facecolors="none",
                    edgecolors=color,
                    linewidths=1.8,
                    s=95,
                    zorder=5,
                )

        ax.set_xscale("log")
        ax.set_yscale("log")
        ax.set_xlabel("P(index is used), log scale")
        ax.set_title(f"max_index={max_index}")
        ax.grid(axis="both", which="both", alpha=0.25)

    axes[0][0].set_ylabel("wallet load time, median ms (log scale)")
    axes[0][-1].legend(loc="upper left")
    fig.suptitle(
        "Wallet load time vs probability of index use\n"
        "50 deterministic samples per probability; shaded band = p05..p95; hollow full-scan markers = incomplete load",
        y=1.05,
    )

    save(fig, "wallet_load_time_by_probability.png", generated)


def plot_completion_rate_by_probability(
    summary: pd.DataFrame, generated: list[Path]
) -> None:
    max_indexes = sorted(summary["max_index"].unique())
    fig, axes = plt.subplots(
        1,
        len(max_indexes),
        figsize=(7 * len(max_indexes), 5),
        sharey=True,
        squeeze=False,
    )

    for ax, max_index in zip(axes[0], max_indexes):
        rows = summary[summary["max_index"] == max_index]
        for strategy, label, color in PROBABILITY_STRATEGIES:
            group = rows[rows["strategy"] == strategy].sort_values("p_used")
            if group.empty:
                continue

            ax.plot(
                group["p_used"],
                group["completion_rate"],
                marker="o",
                linewidth=2,
                color=color,
                label=label,
            )

        ax.set_xscale("log")
        ax.set_ylim(-0.03, 1.05)
        ax.set_xlabel("P(index is used), log scale")
        ax.set_title(f"max_index={max_index}")
        ax.grid(axis="y", alpha=0.25)

    axes[0][0].set_ylabel("complete loads / samples")
    axes[0][-1].legend(loc="lower right")
    fig.suptitle("Wallet load completeness vs probability of index use", y=1.03)

    save(fig, "wallet_load_completion_by_probability.png", generated)


def plot_requested_spks_by_probability(
    summary: pd.DataFrame, generated: list[Path]
) -> None:
    max_indexes = sorted(summary["max_index"].unique())
    fig, axes = plt.subplots(
        1,
        len(max_indexes),
        figsize=(7 * len(max_indexes), 6),
        sharey=True,
        squeeze=False,
    )

    for ax, max_index in zip(axes[0], max_indexes):
        rows = summary[summary["max_index"] == max_index]
        for strategy, label, color in PROBABILITY_STRATEGIES:
            group = rows[rows["strategy"] == strategy].sort_values("p_used")
            if group.empty:
                continue

            x = group["p_used"].to_numpy(dtype=float)
            median = group["median_requested_spks"].to_numpy(dtype=float)
            p05 = group["p05_requested_spks"].to_numpy(dtype=float)
            p95 = group["p95_requested_spks"].to_numpy(dtype=float)
            median = np.maximum(median, 1.0)
            p05 = np.maximum(p05, 1.0)
            p95 = np.maximum(p95, 1.0)

            ax.plot(x, median, marker="o", linewidth=2, color=color, label=label)
            ax.fill_between(x, p05, p95, color=color, alpha=0.12)

        ax.set_xscale("log")
        ax.set_yscale("log")
        ax.set_xlabel("P(index is used), log scale")
        ax.set_title(f"max_index={max_index}")
        ax.grid(axis="both", which="both", alpha=0.25)

    axes[0][0].set_ylabel("requested SPKs, median (log scale)")
    axes[0][-1].legend(loc="upper left")
    fig.suptitle("Requested SPKs vs probability of index use", y=1.03)

    save(fig, "requested_spks_by_probability.png", generated)


def main() -> None:
    generated: list[Path] = []

    if PROBABILITY_CSV.exists():
        probability_df = load_probability_results()
        probability_summary = summarize_probability_results(probability_df)
        plot_wallet_load_time_by_probability(probability_summary, generated)
        plot_completion_rate_by_probability(probability_summary, generated)
        plot_requested_spks_by_probability(probability_summary, generated)

    if CSV.exists():
        df = load_results()
        representative = representative_rows(df, stop_gap=5, parallel_requests=10)
        high_gap = representative_rows(df, stop_gap=1000, parallel_requests=10)

        plot_grouped_spk_counts(
            non_empty(representative),
            title="SPK counts by strategy (stop_gap=5, parallel_requests=10)",
            filename="request_counts.png",
            generated=generated,
        )
        plot_grouped_spk_counts(
            high_gap[high_gap["scenario"].astype(str).isin(SPARSE_SCENARIOS)],
            title="Sparse/high-gap SPK counts with large stop-gap (stop_gap=1000, parallel_requests=10)",
            filename="request_counts_sparse.png",
            generated=generated,
        )
        plot_discovery_by_stop_gap(df, generated)
        plot_discovery_heatmap(df, generated)
        plot_time_by_density(representative, generated)
        plot_time_by_scenario(representative, generated)
        plot_encoding_sizes(representative, generated)

    for path in generated:
        print(f"wrote {path}")


if __name__ == "__main__":
    main()
