#!/usr/bin/env python3

import json
from collections import Counter, defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
CONTRACT_PATH = ROOT / "src/benchmarks/contracts/known_redaction_targets.json"
DATASETS = {
    "EFTA00101126": ROOT
    / "analysis/current_accuracy/efta00101126/EFTA00101126.guesses.json",
    "EFTA00038617": ROOT
    / "analysis/current_accuracy/efta00038617/EFTA00038617.guesses.json",
}


def load_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


def best_target_hit(report: dict, target_text: str):
    target_upper = target_text.strip().upper()
    best = None
    for guess, anchor in zip(report["guesses"], report["anchors"]):
        for index, candidate in enumerate(guess["candidates"], start=1):
            if candidate["text"].strip().upper() != target_upper:
                continue
            item = {
                "rank": index,
                "guess": guess,
                "anchor": anchor,
                "candidate": candidate,
            }
            if best is None or index < best["rank"]:
                best = item
            break
    return best


def summarize_dataset(name: str, report: dict, targets: list[dict]) -> str:
    lines = [f"## {name}"]
    anchor_modes = Counter(anchor["anchor_mode"] for anchor in report["anchors"])
    selection_reasons = Counter(anchor.get("selection_reason") for anchor in report["anchors"])
    lines.append(f"anchor_modes={dict(anchor_modes)}")
    lines.append(f"selection_reasons={dict(selection_reasons)}")

    repeated_top1 = Counter(
        guess["candidates"][0]["text"]
        for guess in report["guesses"]
        if guess["candidates"]
    )
    lines.append(f"repeated_top1={repeated_top1.most_common(10)}")

    mode_stats = defaultdict(list)
    one_sided_width_anomalies = []
    for guess, anchor in zip(report["guesses"], report["anchors"]):
        if not guess["candidates"]:
            continue
        top = guess["candidates"][0]
        target_width = guess["context"]["target_width_pt"]
        mode_stats[anchor["anchor_mode"]].append(len(guess["candidates"]))
        if anchor["anchor_mode"] in {"left_only", "right_only"}:
            width_ratio = top["width_pt"] / max(target_width, 0.001)
            if width_ratio >= 1.25:
                one_sided_width_anomalies.append(
                    (
                        anchor["anchor_row_id"],
                        anchor["anchor_mode"],
                        round(target_width, 3),
                        round(top["width_pt"], 3),
                        round(width_ratio, 3),
                        round(top["error_pt"], 3),
                        top["text"],
                    )
                )
    for mode, counts in sorted(mode_stats.items()):
        mean_count = sum(counts) / len(counts)
        lines.append(f"candidate_count[{mode}] mean={mean_count:.1f} rows={len(counts)}")
    if one_sided_width_anomalies:
        lines.append("one_sided_width_anomalies:")
        for anomaly in one_sided_width_anomalies[:10]:
            lines.append(
                "  row={} mode={} target_width={} top1_width={} ratio={} error={} top1={!r}".format(
                    *anomaly
                )
            )

    lines.append("targets:")
    for target in targets:
        best = best_target_hit(report, target["target"])
        if best is None:
            lines.append(f"  {target['target']}: not_found")
            continue
        guess = best["guess"]
        anchor = best["anchor"]
        candidate = best["candidate"]
        top1 = guess["candidates"][0]
        top1_error = top1["error_pt"]
        within_0_1 = sum(
            1 for item in guess["candidates"] if item["error_pt"] <= top1_error + 0.1
        )
        within_0_5 = sum(
            1 for item in guess["candidates"] if item["error_pt"] <= top1_error + 0.5
        )
        within_1_0 = sum(
            1 for item in guess["candidates"] if item["error_pt"] <= top1_error + 1.0
        )
        lines.append(
            "  target={!r} row={} rank={} mode={} reason={} seed={} target_width={:.3f} target_candidate_width={:.3f} top1={!r} top1_error={:.3f} target_error={:.3f} error_gap={:.3f} within_0.1={} within_0.5={} within_1.0={}".format(
                target["target"],
                anchor["anchor_row_id"],
                best["rank"],
                anchor["anchor_mode"],
                anchor.get("selection_reason"),
                anchor.get("measurement_seed_side"),
                guess["context"]["target_width_pt"],
                candidate["width_pt"],
                top1["text"],
                top1_error,
                candidate["error_pt"],
                candidate["error_pt"] - top1_error,
                within_0_1,
                within_0_5,
                within_1_0,
            )
        )
    return "\n".join(lines)


def main():
    contracts = load_json(CONTRACT_PATH)
    contract_by_name = {dataset["name"]: dataset for dataset in contracts["datasets"]}
    sections = []
    for name, path in DATASETS.items():
        report = load_json(path)
        sections.append(
            summarize_dataset(name, report, contract_by_name[name]["targets"])
        )
    output = "\n\n".join(sections) + "\n"
    print(output, end="")


if __name__ == "__main__":
    main()
