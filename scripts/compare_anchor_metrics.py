#!/usr/bin/env python3

import argparse
import json
from collections import Counter
from pathlib import Path
from typing import Any


STRICT_PROFILE_FIELDS = (
    "run_font_name",
    "run_font_size_pt",
    "run_h_scale_pct",
    "run_char_spacing_pt",
    "run_word_spacing_pt",
)

IGNORED_COMPARE_FIELDS = {
    "anchor_side",
    "candidate_count",
    "candidate_rank",
}


def load_items(path: Path) -> list[dict[str, Any]]:
    data = json.loads(path.read_text())
    return data.get("items", [])


def is_close(left: Any, right: Any) -> bool:
    if isinstance(left, float) or isinstance(right, float):
        try:
            return abs(float(left) - float(right)) <= 1e-6
        except (TypeError, ValueError):
            return left == right
    return left == right


def format_value(value: Any) -> str:
    if isinstance(value, float):
        return f"{value:.6f}"
    return json.dumps(value, sort_keys=True)


def format_difference(left: Any, right: Any) -> str:
    if isinstance(left, (int, float)) and isinstance(right, (int, float)):
        return (
            f"left={format_value(left)} right={format_value(right)} "
            f"delta={float(right) - float(left):.6f}"
        )
    return f"left={format_value(left)} right={format_value(right)}"


def selected_anchor_metrics(items: list[dict[str, Any]], row_id: str, side: str) -> dict[str, Any] | None:
    selected = [
        item
        for item in items
        if item.get("stage") == "redaction_evidence"
        and item.get("row_id") == row_id
        and item.get("code") == "anchor_run_selected"
        and item.get("metrics", {}).get("anchor_side") == side
    ]
    if not selected:
        return None
    selected.sort(key=lambda item: json.dumps(item.get("metrics", {}), sort_keys=True))
    return selected[0].get("metrics", {})


def selected_bucket_metrics(items: list[dict[str, Any]], row_id: str) -> dict[str, Any] | None:
    selected = [
        item
        for item in items
        if item.get("stage") == "redaction_evidence"
        and item.get("row_id") == row_id
        and item.get("code") == "line_bucket_selected"
    ]
    if not selected:
        return None
    return selected[0].get("metrics", {})


def two_sided_rejection_metrics(items: list[dict[str, Any]], row_id: str) -> dict[str, Any] | None:
    selected = [
        item
        for item in items
        if item.get("stage") == "redaction_evidence"
        and item.get("row_id") == row_id
        and item.get("code") == "anchor_two_sided_rejected"
    ]
    if not selected:
        return None
    return selected[0].get("metrics", {})


def compare_metric_sets(left: dict[str, Any], right: dict[str, Any]) -> tuple[list[str], list[str]]:
    all_keys = sorted((set(left) | set(right)) - IGNORED_COMPARE_FIELDS)
    same = []
    different = []
    for key in all_keys:
        if key not in left or key not in right:
            different.append(f"{key}: left={format_value(left.get(key))} right={format_value(right.get(key))}")
        elif is_close(left[key], right[key]):
            same.append(f"{key}: {format_value(left[key])}")
        else:
            different.append(f"{key}: {format_difference(left[key], right[key])}")
    return same, different


def compare_strict_fields(left: dict[str, Any], right: dict[str, Any]) -> tuple[list[str], list[str]]:
    same = []
    different = []
    for key in STRICT_PROFILE_FIELDS:
        left_value = left.get(key)
        right_value = right.get(key)
        if is_close(left_value, right_value):
            same.append(f"{key}: {format_value(left_value)}")
        else:
            different.append(f"{key}: {format_difference(left_value, right_value)}")
    return same, different


def summarize_dataset(path: Path) -> dict[str, Any]:
    items = load_items(path)
    row_ids = sorted(
        {
            item.get("row_id")
            for item in items
            if item.get("stage") == "redaction_evidence" and item.get("row_id")
        }
    )
    rows = []
    diff_counter = Counter()
    strict_diff_counter = Counter()
    for row_id in row_ids:
        left = selected_anchor_metrics(items, row_id, "left")
        right = selected_anchor_metrics(items, row_id, "right")
        if left is None or right is None:
            rows.append(
                {
                    "row_id": row_id,
                    "status": "missing_selected_anchor",
                    "has_left": left is not None,
                    "has_right": right is not None,
                }
            )
            continue
        same_metrics, different_metrics = compare_metric_sets(left, right)
        strict_same, strict_different = compare_strict_fields(left, right)
        for entry in different_metrics:
            diff_counter[entry.split(":", 1)[0]] += 1
        for entry in strict_different:
            strict_diff_counter[entry.split(":", 1)[0]] += 1
        rows.append(
            {
                "row_id": row_id,
                "status": "compared",
                "line_bucket": selected_bucket_metrics(items, row_id),
                "two_sided_rejection": two_sided_rejection_metrics(items, row_id),
                "same_metrics": same_metrics,
                "different_metrics": different_metrics,
                "strict_profile_same": strict_same,
                "strict_profile_different": strict_different,
                "left_selected": left,
                "right_selected": right,
            }
        )
    return {
        "diagnostics_path": str(path),
        "row_count": len(row_ids),
        "rows_with_both_selected_anchors": sum(row["status"] == "compared" for row in rows),
        "rows_missing_selected_anchor": sum(row["status"] != "compared" for row in rows),
        "different_metric_counts": dict(sorted(diff_counter.items())),
        "strict_profile_different_counts": dict(sorted(strict_diff_counter.items())),
        "rows": rows,
    }


def render_summary(dataset: dict[str, Any]) -> str:
    lines = []
    lines.append(f"Diagnostics: {dataset['diagnostics_path']}")
    lines.append(f"rows={dataset['row_count']}")
    lines.append(f"rows_with_both_selected_anchors={dataset['rows_with_both_selected_anchors']}")
    lines.append(f"rows_missing_selected_anchor={dataset['rows_missing_selected_anchor']}")
    lines.append("strict_profile_different_counts:")
    if dataset["strict_profile_different_counts"]:
        for key, count in dataset["strict_profile_different_counts"].items():
            lines.append(f"  {key}: {count}")
    else:
        lines.append("  none")
    lines.append("different_metric_counts:")
    if dataset["different_metric_counts"]:
        for key, count in dataset["different_metric_counts"].items():
            lines.append(f"  {key}: {count}")
    else:
        lines.append("  none")
    for row in dataset["rows"]:
        lines.append("")
        lines.append(f"ROW {row['row_id']}")
        if row["status"] != "compared":
            lines.append(
                f"  status={row['status']} has_left={row['has_left']} has_right={row['has_right']}"
            )
            continue
        bucket = row["line_bucket"] or {}
        lines.append(
            "  line_bucket="
            f"{bucket.get('line_id')} "
            f"left_candidates={bucket.get('bucket_left_candidate_count')} "
            f"right_candidates={bucket.get('bucket_right_candidate_count')}"
        )
        rejection = row["two_sided_rejection"] or {}
        if rejection:
            lines.append(
                "  two_sided_rejection="
                f"{rejection.get('measurement_error_reason_code')} "
                f"attempted={rejection.get('attempted_mode')} "
                f"resolved={rejection.get('resolved_mode')}"
            )
        else:
            lines.append("  two_sided_rejection=none")
        lines.append("  strict_profile_same:")
        for entry in row["strict_profile_same"]:
            lines.append(f"    {entry}")
        lines.append("  strict_profile_different:")
        if row["strict_profile_different"]:
            for entry in row["strict_profile_different"]:
                lines.append(f"    {entry}")
        else:
            lines.append("    none")
        lines.append("  same_metrics:")
        for entry in row["same_metrics"]:
            lines.append(f"    {entry}")
        lines.append("  different_metrics:")
        for entry in row["different_metrics"]:
            lines.append(f"    {entry}")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("diagnostics", nargs="+", type=Path)
    parser.add_argument("--json-out", type=Path)
    parser.add_argument("--text-out", type=Path)
    args = parser.parse_args()

    datasets = [summarize_dataset(path) for path in args.diagnostics]
    text = "\n\n".join(render_summary(dataset) for dataset in datasets)

    if args.json_out is not None:
        args.json_out.write_text(json.dumps({"datasets": datasets}, indent=2, sort_keys=True))
    if args.text_out is not None:
        args.text_out.write_text(text)
    if args.json_out is None and args.text_out is None:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
