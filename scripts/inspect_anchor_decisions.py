#!/usr/bin/env python3

import argparse
import json
from pathlib import Path


INTERESTING_CODES = {
    "line_bucket_candidate_considered",
    "line_bucket_selected",
    "anchor_run_candidate_considered",
    "anchor_run_selected",
    "anchor_run_missing",
    "anchor_two_sided_rejected",
    "anchor_mode_fallback_applied",
    "row_backend_ready",
}


def load_items(path: Path) -> list[dict]:
    data = json.loads(path.read_text())
    return data.get("items", [])


def sort_key(item: dict) -> tuple:
    metrics = item.get("metrics", {})
    return (
        item.get("page_index"),
        item.get("row_id"),
        item.get("code"),
        metrics.get("candidate_rank", -1),
        metrics.get("line_id", ""),
        json.dumps(metrics, sort_keys=True),
    )


def print_item(item: dict) -> None:
    print(f"{item['code']}")
    print(json.dumps(item.get("metrics", {}), indent=2, sort_keys=True))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("diagnostics", type=Path)
    parser.add_argument("row_ids", nargs="+")
    args = parser.parse_args()

    items = [
        item
        for item in load_items(args.diagnostics)
        if item.get("stage") == "redaction_evidence"
        and item.get("code") in INTERESTING_CODES
        and item.get("row_id") in set(args.row_ids)
    ]
    items.sort(key=sort_key)

    for row_id in args.row_ids:
        print(f"\nROW {row_id}")
        row_items = [item for item in items if item.get("row_id") == row_id]
        if not row_items:
            print("  no matching evidence diagnostics")
            continue
        for item in row_items:
            print_item(item)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
