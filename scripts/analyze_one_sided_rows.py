#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class RowClusterEntry:
    row_id: str
    anchor_mode: str
    page_index: int
    bbox: dict[str, float]
    left_text: str | None
    right_text: str | None
    candidate_count: int


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--diagnostics-root",
        default="analysis/diagnostics",
        help="Directory containing per-file diagnostics output",
    )
    parser.add_argument(
        "--benchmark-json",
        default="analysis/benchmark/guess_accuracy.json",
        help="Benchmark JSON with target ranks",
    )
    parser.add_argument(
        "--datasets",
        nargs="+",
        default=["EFTA00101126", "EFTA00038617"],
        help="Dataset basenames to analyze",
    )
    parser.add_argument(
        "--adjacent-threshold-pt",
        type=float,
        default=10.0,
        help="Gap threshold used to call a neighboring same-line redaction 'adjacent'",
    )
    return parser.parse_args()


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def normalized_text(value: str) -> str:
    return " ".join(value.upper().split())


def target_matches_candidate(target: str, candidate: str) -> bool:
    candidate_normalized = normalized_text(candidate)
    return all(token in candidate_normalized for token in normalized_text(target).split())


def same_line(left: dict[str, Any], right: dict[str, Any]) -> bool:
    left_bbox = left["bbox"]
    right_bbox = right["bbox"]
    overlap = min(left_bbox["y1"], right_bbox["y1"]) - max(left_bbox["y0"], right_bbox["y0"])
    return overlap > 0.0 or abs(left_bbox["y1"] - right_bbox["y1"]) <= 3.0


def cluster_rows(rows: list[RowClusterEntry]) -> list[list[RowClusterEntry]]:
    remaining = list(rows)
    clusters: list[list[RowClusterEntry]] = []
    while remaining:
        seed = remaining.pop(0)
        cluster = [seed]
        keep: list[RowClusterEntry] = []
        for candidate in remaining:
            if candidate.page_index == seed.page_index and same_line(
                {"bbox": seed.bbox}, {"bbox": candidate.bbox}
            ):
                cluster.append(candidate)
            else:
                keep.append(candidate)
        cluster.sort(key=lambda row: (row.bbox["x0"], row.bbox["x1"]))
        clusters.append(cluster)
        remaining = keep
    clusters.sort(key=lambda cluster: (cluster[0].page_index, -cluster[0].bbox["y1"], cluster[0].bbox["x0"]))
    return clusters


def load_benchmark_targets(benchmark_json: Path) -> dict[str, list[dict[str, Any]]]:
    payload = load_json(benchmark_json)
    return {dataset["name"]: dataset["targets"] for dataset in payload["datasets"]}


def summarize_anchor_recovery(
    dataset: str,
    anchors: list[dict[str, Any]],
    redactions: list[dict[str, Any]],
    guesses: list[dict[str, Any]],
    adjacent_threshold_pt: float,
) -> None:
    rows = [
        RowClusterEntry(
            row_id=anchor["anchor_row_id"],
            anchor_mode=anchor["anchor_mode"],
            page_index=anchor["page_index"],
            bbox=anchor["bbox"],
            left_text=None if anchor["left"] is None else anchor["left"]["text"],
            right_text=None if anchor["right"] is None else anchor["right"]["text"],
            candidate_count=len(guess["candidates"]),
        )
        for anchor, _redaction, guess in zip(anchors, redactions, guesses)
    ]

    clusters = cluster_rows(rows)
    blocked_exact = 0
    blocked_adjacent = 0
    total = 0

    print(f"\n[{dataset}] Anchor Recovery")
    for cluster in clusters:
        if len(cluster) > 1:
            cluster_label = ", ".join(
                f"{row.row_id}:{row.anchor_mode}@{row.bbox['x0']:.1f}-{row.bbox['x1']:.1f}"
                for row in cluster
            )
            print(f"  cluster page {cluster[0].page_index}: {cluster_label}")
        for index, row in enumerate(cluster):
            total += 1
            if row.anchor_mode == "left_only":
                next_gap = None
                if index + 1 < len(cluster):
                    next_gap = cluster[index + 1].bbox["x0"] - row.bbox["x1"]
                    blocked_exact += 1
                    if next_gap <= adjacent_threshold_pt:
                        blocked_adjacent += 1
                print(
                    f"  {row.row_id}: left_only left={row.left_text!r} "
                    f"missing_right_gap_to_next_redaction={None if next_gap is None else round(next_gap, 3)} "
                    f"candidates={row.candidate_count}"
                )
            elif row.anchor_mode == "right_only":
                prev_gap = None
                if index > 0:
                    prev_gap = row.bbox["x0"] - cluster[index - 1].bbox["x1"]
                    blocked_exact += 1
                    if prev_gap <= adjacent_threshold_pt:
                        blocked_adjacent += 1
                print(
                    f"  {row.row_id}: right_only right={row.right_text!r} "
                    f"missing_left_gap_to_prev_redaction={None if prev_gap is None else round(prev_gap, 3)} "
                    f"candidates={row.candidate_count}"
                )
            else:
                print(f"  {row.row_id}: {row.anchor_mode} candidates={row.candidate_count}")

    print(
        f"  summary: one_sided_rows={total} "
        f"rows_with_same_line_redaction_on_missing_side={blocked_exact} "
        f"rows_with_adjacent_missing_side_gap<={adjacent_threshold_pt:g}pt={blocked_adjacent}"
    )


def summarize_scoring_quality(
    dataset: str,
    targets: list[dict[str, Any]],
    anchors: list[dict[str, Any]],
    guesses: list[dict[str, Any]],
) -> None:
    print(f"\n[{dataset}] One-Sided Scoring")
    for target in targets:
        best_row_summary: dict[str, Any] | None = None
        for anchor, guess in zip(anchors, guesses):
            candidates = guess["candidates"]
            if not candidates:
                continue
            top_candidate = candidates[0]
            within_0_1 = sum(
                1 for candidate in candidates if candidate["error_pt"] - top_candidate["error_pt"] <= 0.1
            )
            within_0_5 = sum(
                1 for candidate in candidates if candidate["error_pt"] - top_candidate["error_pt"] <= 0.5
            )
            within_1_0 = sum(
                1 for candidate in candidates if candidate["error_pt"] - top_candidate["error_pt"] <= 1.0
            )
            for rank, candidate in enumerate(candidates, start=1):
                if not target_matches_candidate(target["target"], candidate["text"]):
                    continue
                row_summary = {
                    "row_id": anchor["anchor_row_id"],
                    "anchor_mode": anchor["anchor_mode"],
                    "rank": rank,
                    "candidate_count": len(candidates),
                    "target_candidate": candidate["text"],
                    "target_error_pt": round(candidate["error_pt"], 6),
                    "top1_candidate": top_candidate["text"],
                    "top1_error_pt": round(top_candidate["error_pt"], 6),
                    "error_gap_pt": round(candidate["error_pt"] - top_candidate["error_pt"], 6),
                    "near_top1_within_0_1_pt": within_0_1,
                    "near_top1_within_0_5_pt": within_0_5,
                    "near_top1_within_1_0_pt": within_1_0,
                }
                if best_row_summary is None or row_summary["rank"] < best_row_summary["rank"]:
                    best_row_summary = row_summary
                break
        print(f"  {target['target']}: {best_row_summary}")


def main() -> int:
    args = parse_args()
    targets_by_dataset = load_benchmark_targets(Path(args.benchmark_json))
    diagnostics_root = Path(args.diagnostics_root)

    for dataset in args.datasets:
        dataset_dir = diagnostics_root / dataset.lower()
        anchors = load_json(dataset_dir / f"{dataset}.anchors.json")["decisions"]
        guesses = load_json(dataset_dir / f"{dataset}.guesses.json")["guesses"]
        redactions = load_json(dataset_dir / f"{dataset}.redactions.json")["redactions"]

        summarize_anchor_recovery(
            dataset,
            anchors,
            redactions,
            guesses,
            args.adjacent_threshold_pt,
        )
        summarize_scoring_quality(dataset, targets_by_dataset[dataset], anchors, guesses)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
