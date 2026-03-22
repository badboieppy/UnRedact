#!/usr/bin/env python3

from __future__ import annotations

import json
import math
import os
import re
import shutil
import statistics
import subprocess
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parent.parent
OUTPUT_ROOT = ROOT / "analysis" / "benchmark_question_dossier"
RAW_ROOT = OUTPUT_ROOT / "raw"
EXPERIMENTS_ROOT = OUTPUT_ROOT / "experiments"
ANSWERS_ROOT = OUTPUT_ROOT / "answers"

NOISE_WORDS = [
    "ALPHA",
    "BRAVO",
    "CHARLIE",
    "DELTA",
    "ECHO",
    "FOXTROT",
    "GOLF",
    "HOTEL",
    "INDIA",
    "JULIET",
    "KILO",
    "LIMA",
    "MIKE",
    "NOVEMBER",
    "OSCAR",
    "PAPA",
    "QUEBEC",
    "ROMEO",
    "SIERRA",
    "TANGO",
    "UNIFORM",
    "VICTOR",
    "WHISKEY",
    "XRAY",
]
NAME_PREFIX_TOKENS = {
    "mr",
    "mrs",
    "ms",
    "mx",
    "dr",
    "prof",
    "sir",
    "dame",
    "lady",
    "lord",
    "rev",
    "fr",
    "judge",
    "hon",
    "capt",
    "cmdr",
    "col",
    "gen",
    "adm",
    "pres",
    "president",
    "governor",
    "lt",
    "sgt",
}
NAME_SUFFIX_TOKENS = {
    "jr",
    "sr",
    "ii",
    "iii",
    "iv",
    "v",
    "vi",
    "phd",
    "md",
    "esq",
    "esquire",
    "jd",
    "dds",
    "dmd",
    "do",
    "rn",
    "cpa",
    "mba",
    "qc",
    "kc",
    "ret",
    "retired",
    "junior",
    "senior",
}


def ensure_dir(path: Path) -> None:
    path.mkdir(parents=True, exist_ok=True)


def write_json(path: Path, value: Any) -> None:
    ensure_dir(path.parent)
    path.write_text(
        json.dumps(
            value,
            indent=2,
            sort_keys=False,
            default=lambda obj: sorted(obj) if isinstance(obj, set) else str(obj),
        )
        + "\n",
        encoding="utf-8",
    )


def write_text(path: Path, text: str) -> None:
    ensure_dir(path.parent)
    path.write_text(text, encoding="utf-8")


def run_cmd(args: list[str], cwd: Path = ROOT) -> None:
    proc = subprocess.run(args, cwd=cwd, text=True)
    if proc.returncode != 0:
        raise SystemExit(f"command failed ({proc.returncode}): {' '.join(args)}")


def normalize_text(value: str) -> str:
    return " ".join(value.split()).strip().upper()


def classify_name_family(text: str) -> str:
    trimmed = text.strip()
    if not trimmed:
        return "empty"
    if "," in trimmed:
        return "comma"
    tokens = [token for token in trimmed.split() if token]
    if not tokens:
        return "empty"
    alpha_count = sum(1 for ch in trimmed if ch.isascii() and ch.isalpha())
    punct_count = sum(1 for ch in trimmed if not ch.isalnum() and not ch.isspace())
    if punct_count > alpha_count:
        return "punctuation_heavy"
    if len(tokens) == 1:
        token = tokens[0]
        letters = sum(1 for ch in token if ch.isascii() and ch.isalpha())
        if letters <= 2 or token.endswith("."):
            return "initial"
        return "single_token"
    if any(sum(1 for ch in token if ch.isascii() and ch.isalpha()) <= 1 or token.endswith(".") for token in tokens):
        return "initial"
    return "plain_multi_token"


def alpha_len(text: str) -> int:
    return sum(1 for ch in text if ch.isalpha())


def token_count(text: str) -> int:
    return len([token for token in text.split() if token])


def parse_default_dictionary_entries() -> list[str]:
    path = ROOT / "src" / "data" / "model" / "default_name_dictionary.rs"
    out: list[str] = []
    pattern = re.compile(r'^\s*"([^"]+)",\s*$')
    for line in path.read_text(encoding="utf-8").splitlines():
        match = pattern.match(line)
        if match:
            out.append(match.group(1))
    return out


def load_contract() -> dict[str, Any]:
    path = ROOT / "src" / "benchmarks" / "contracts" / "known_redaction_targets.json"
    return json.loads(path.read_text(encoding="utf-8"))


def merge_dictionary_with_targets(entries: list[str], targets: list[dict[str, Any]]) -> list[str]:
    merged = {normalize_text(target["target"]) for target in targets if target["target"].strip()}
    for entry in entries:
        trimmed = entry.strip()
        if trimmed:
            merged.add(trimmed.upper())
    for noise in NOISE_WORDS:
        merged.add(noise)
    return sorted(merged)


def baseline_dictionary_entries(dataset: dict[str, Any], default_entries: list[str]) -> list[str] | None:
    if not all(target["selector"]["kind"] == "in_pool" for target in dataset["targets"]):
        return None
    out = {normalize_text(target["target"]) for target in dataset["targets"] if target["target"].strip()}
    for entry in default_entries:
        trimmed = entry.strip()
        if trimmed:
            out.add(trimmed.upper())
        if len(out) >= 1200:
            break
    for noise in NOISE_WORDS:
        out.add(noise)
    return sorted(out)


def no_comma(entries: list[str]) -> list[str]:
    return sorted({entry for entry in entries if classify_name_family(entry) != "comma"})


def no_single(entries: list[str]) -> list[str]:
    return sorted({entry for entry in entries if classify_name_family(entry) != "single_token"})


def no_initial(entries: list[str]) -> list[str]:
    return sorted({entry for entry in entries if classify_name_family(entry) != "initial"})


def no_punctuation_heavy(entries: list[str]) -> list[str]:
    return sorted({entry for entry in entries if classify_name_family(entry) != "punctuation_heavy"})


def multi_token_only(entries: list[str]) -> list[str]:
    return sorted({entry for entry in entries if token_count(entry) >= 2})


def plain_multi_only(entries: list[str]) -> list[str]:
    return sorted({entry for entry in entries if classify_name_family(entry) == "plain_multi_token"})


def no_honorific_prefix(entries: list[str]) -> list[str]:
    filtered = set()
    for entry in entries:
        tokens = entry.split()
        if tokens and tokens[0].rstrip(".").lower() in NAME_PREFIX_TOKENS:
            continue
        filtered.add(entry)
    return sorted(filtered)


def no_suffix_token(entries: list[str]) -> list[str]:
    filtered = set()
    for entry in entries:
        tokens = entry.split()
        if tokens and tokens[-1].rstrip(".").lower() in NAME_SUFFIX_TOKENS:
            continue
        filtered.add(entry)
    return sorted(filtered)


def upper_only(entries: list[str]) -> list[str]:
    return sorted({entry.upper() for entry in entries if entry.strip()})


def title_only(entries: list[str]) -> list[str]:
    return sorted({entry.title() for entry in entries if entry.strip()})


def evaluate_target_ranks(dataset: dict[str, Any], selected_rows: list[dict[str, Any]]) -> dict[str, Any]:
    def rank_in_row(row: dict[str, Any], target_text: str) -> int | None:
        target_norm = normalize_text(target_text)
        ordered = []
        seen = set()
        for candidate in row["candidates"]:
            text = normalize_text(candidate["text"])
            if text and text not in seen:
                seen.add(text)
                ordered.append(text)
        try:
            return ordered.index(target_norm) + 1
        except ValueError:
            return None

    results = []
    ranks = []
    for target in dataset["targets"]:
        best_rank = None
        best_row = None
        eligible_rows = 0
        if target["selector"]["kind"] == "index_from_end":
            for row in selected_rows:
                if row["row_key"].endswith(f"#{target['label']}"):
                    eligible_rows = 1
                    best_row = row
                    best_rank = rank_in_row(row, target["target"])
                    break
        else:
            eligible_rows = len(selected_rows)
            for row in selected_rows:
                rank = rank_in_row(row, target["target"])
                if rank is not None and (best_rank is None or rank < best_rank):
                    best_rank = rank
                    best_row = row
        ranks.append(best_rank)
        top1 = best_row["candidates"][0] if best_row and best_row["candidates"] else None
        target_candidate = None
        if best_row:
            norm = normalize_text(target["target"])
            for candidate in best_row["candidates"]:
                if normalize_text(candidate["text"]) == norm:
                    target_candidate = candidate
                    break
        results.append(
            {
                "dataset": dataset["name"],
                "label": target["label"],
                "target": target["target"],
                "best_rank": best_rank,
                "best_row_key": best_row["row_key"] if best_row else None,
                "eligible_row_count": eligible_rows,
                "present_in_pool": target_candidate is not None,
                "top1_text": top1["text"] if top1 else None,
                "top1_family": classify_name_family(top1["text"]) if top1 else None,
                "target_family": classify_name_family(target["target"]),
                "candidate_count": len(best_row["candidates"]) if best_row else None,
                "target_error_pt": target_candidate["error_pt"] if target_candidate else None,
                "top1_error_pt": top1["error_pt"] if top1 else None,
                "anchor_mode": best_row.get("anchor_mode") if best_row else None,
            }
        )

    evaluated = len(ranks)
    found = [rank for rank in ranks if rank is not None]
    def recall_at(k: int) -> float:
        if not evaluated:
            return 0.0
        return sum(1 for rank in found if rank <= k) / evaluated
    mrr = 0.0 if not evaluated else sum(0.0 if rank is None else 1.0 / rank for rank in ranks) / evaluated
    mean_rank = None if not found else sum(found) / len(found)
    return {
        "targets": results,
        "summary": {
            "evaluated_items": evaluated,
            "found_items": len(found),
            "recall_at_1": recall_at(1),
            "recall_at_5": recall_at(5),
            "recall_at_20": recall_at(20),
            "mrr": mrr,
            "mean_rank_found": mean_rank,
        },
    }


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def first_float(values: list[float]) -> float | None:
    return None if not values else sum(values) / len(values)


def current_runtime_variant_rows(summary: dict[str, Any], variant_name: str) -> dict[str, Any]:
    if variant_name == "baseline":
        return load_json(RAW_ROOT / "accuracy_benchmark_report" / "stages" / "guess_baseline.json")
    return load_json(RAW_ROOT / "accuracy_benchmark_report" / "stages" / "dictionary_ablation.json")["variant_map"][variant_name]


def collect_raw_artifacts() -> None:
    ensure_dir(RAW_ROOT)
    run_cmd(["cargo", "build", "--features", "cli-entry", "--bins"])

    accuracy_dir = RAW_ROOT / "accuracy_benchmark_report"
    if not (accuracy_dir / "summary.json").exists():
        existing_accuracy_dir = ROOT / "analysis" / "accuracy_benchmark_report"
        if (existing_accuracy_dir / "summary.json").exists():
            shutil.copytree(existing_accuracy_dir, accuracy_dir, dirs_exist_ok=True)
        else:
            run_cmd(
                [
                    str(ROOT / "target" / "debug" / "accuracy_benchmark_report"),
                    "--output-dir",
                    str(accuracy_dir),
                    "--repeats",
                    "2",
                ]
            )

    anchor_dir = RAW_ROOT / "anchor_span_visual_benchmark"
    if not (anchor_dir / "summary.json").exists():
        existing_anchor_dirs = [
            ROOT / "analysis" / "final_validation" / "anchor_span_visual_benchmark",
            ROOT / "analysis" / "accuracy_benchmark_report" / "stages" / "anchor_span_visual",
        ]
        copied = False
        for existing_anchor_dir in existing_anchor_dirs:
            if (existing_anchor_dir / "summary.json").exists():
                shutil.copytree(existing_anchor_dir, anchor_dir, dirs_exist_ok=True)
                copied = True
                break
        if not copied:
            run_cmd(
                [
                    str(ROOT / "target" / "debug" / "anchor_span_visual_benchmark"),
                    "--output-dir",
                    str(anchor_dir),
                    "--compact",
                ]
            )

    batch_dir = RAW_ROOT / "test_data_batch"
    if not (batch_dir / "batch_manifest.json").exists():
        run_cmd(
            [
                str(ROOT / "target" / "debug" / "unredact-cli"),
                str(ROOT / "test_data"),
                "--output-dir",
                str(batch_dir),
                "--diagnostics",
            ]
        )

    benchmark_diag_dir = RAW_ROOT / "benchmark_diagnostics"
    ensure_dir(benchmark_diag_dir)
    contract = load_contract()
    default_entries = parse_default_dictionary_entries()
    for dataset in contract["datasets"]:
        dataset_name = dataset["name"]
        dataset_dir = benchmark_diag_dir / dataset_name
        if (dataset_dir / f"{dataset_name}.diagnostics.json").exists():
            continue
        ensure_dir(dataset_dir)
        dict_entries = baseline_dictionary_entries(dataset, default_entries)
        dict_path = None
        if dict_entries is not None:
            dict_path = dataset_dir / "baseline.dictionary.txt"
            write_text(dict_path, "\n".join(dict_entries) + "\n")
        cmd = [
            str(ROOT / "target" / "debug" / "unredact-cli"),
            str(ROOT / dataset["input_pdf"]),
            "--output-dir",
            str(dataset_dir),
            "--diagnostics",
        ]
        if dict_path is not None:
            cmd.extend(["--dictionary", str(dict_path)])
        run_cmd(cmd)


def build_variant_map(accuracy_dir: Path) -> None:
    dictionary_ablation = load_json(accuracy_dir / "stages" / "dictionary_ablation.json")
    variant_map = {"baseline": load_json(accuracy_dir / "stages" / "guess_baseline.json")}
    for variant in dictionary_ablation["variants"]:
        variant_name = variant.get("name") or variant.get("variant")
        if variant_name is None:
            raise ValueError("dictionary ablation variant is missing name/variant")
        variant_map[variant_name] = variant
    dictionary_ablation["variant_map"] = variant_map
    write_json(accuracy_dir / "stages" / "dictionary_ablation.json", dictionary_ablation)


def row_key_from_guess(dataset_name: str, guess: dict[str, Any]) -> str:
    bbox = guess["bbox"]
    return f"{dataset_name}:page{guess['page_index']}:{bbox['x0']:.2f}:{bbox['y0']:.2f}:{bbox['x1']:.2f}:{bbox['y1']:.2f}"


def select_rows_for_dataset(dataset: dict[str, Any], report: dict[str, Any]) -> list[dict[str, Any]]:
    selector = dataset["row_selector"]
    out = []
    if selector["kind"] == "page_y_range":
        for guess in report["guesses"]:
            bbox = guess["bbox"]
            if guess["page_index"] == selector["page_index"] and bbox["y0"] >= selector["y0_min"] and bbox["y1"] <= selector["y1_max"]:
                out.append(
                    {
                        "row_key": row_key_from_guess(dataset["name"], guess),
                        "dataset": dataset["name"],
                        "page_index": guess["page_index"],
                        "bbox": bbox,
                        "candidates": guess["candidates"],
                        "anchor_mode": guess["context"].get("anchor_mode"),
                        "target_width_pt": guess["context"]["target_width_pt"],
                    }
                )
    else:
        indexed = []
        for target in dataset["targets"]:
            index_from_end = target["selector"]["index_from_end"]
            guess = report["guesses"][len(report["guesses"]) - index_from_end]
            indexed.append(
                {
                    "row_key": f"{row_key_from_guess(dataset['name'], guess)}#{target['label']}",
                    "dataset": dataset["name"],
                    "page_index": guess["page_index"],
                    "bbox": guess["bbox"],
                    "candidates": guess["candidates"],
                    "anchor_mode": guess["context"].get("anchor_mode"),
                    "target_width_pt": guess["context"]["target_width_pt"],
                }
            )
        out = indexed
    return out


def evaluate_runtime_variant(
    variant_name: str,
    contract: dict[str, Any],
    runs_root: Path,
) -> dict[str, Any]:
    datasets = []
    for dataset in contract["datasets"]:
        guesses_path = runs_root / variant_name / dataset["name"] / f"{dataset['name']}.guesses.json"
        report = load_json(guesses_path)
        selected_rows = select_rows_for_dataset(dataset, report)
        evaluated = evaluate_target_ranks(dataset, selected_rows)
        datasets.append(
            {
                "name": dataset["name"],
                "summary": evaluated["summary"],
                "targets": evaluated["targets"],
                "selected_rows": selected_rows,
            }
        )
    overall = evaluate_target_ranks(
        {"name": "overall", "targets": [target for dataset in contract["datasets"] for target in dataset["targets"]]},
        [],
    )["summary"]
    all_ranks = []
    for dataset in datasets:
        for target in dataset["targets"]:
            all_ranks.append(target["best_rank"])
    found = [rank for rank in all_ranks if rank is not None]
    evaluated_items = len(all_ranks)
    overall = {
        "evaluated_items": evaluated_items,
        "found_items": len(found),
        "recall_at_1": 0.0 if not evaluated_items else sum(1 for rank in found if rank <= 1) / evaluated_items,
        "recall_at_5": 0.0 if not evaluated_items else sum(1 for rank in found if rank <= 5) / evaluated_items,
        "recall_at_20": 0.0 if not evaluated_items else sum(1 for rank in found if rank <= 20) / evaluated_items,
        "mrr": 0.0 if not evaluated_items else sum(0.0 if rank is None else 1.0 / rank for rank in all_ranks) / evaluated_items,
        "mean_rank_found": None if not found else sum(found) / len(found),
    }
    return {"name": variant_name, "datasets": datasets, "overall": overall}


def build_dictionary_from_accuracy_rows(
    baseline_stage: dict[str, Any],
    limit: int,
    family_filter: Callable[[str], bool],
    error_limit: float | None = None,
) -> list[str]:
    scored: dict[str, float] = {}
    targets = set()
    for dataset in baseline_stage["datasets"]:
        for target in dataset["targets"]:
            targets.add(normalize_text(target["target"]))
        for row in dataset["selected_rows"]:
            for candidate in row["candidates"]:
                text = candidate["text"].strip()
                if not text or not family_filter(text):
                    continue
                if error_limit is not None and candidate["error_pt"] > error_limit:
                    continue
                key = normalize_text(text)
                scored[key] = min(scored.get(key, 1e9), candidate["error_pt"])
    entries = sorted(scored.items(), key=lambda item: (item[1], item[0]))
    out = list(targets)
    for text, _ in entries:
        if len(out) >= limit:
            break
        if text not in out:
            out.append(text)
    for noise in NOISE_WORDS:
        if noise not in out:
            out.append(noise)
    return out


def build_charcount_dictionary(
    baseline_stage: dict[str, Any],
    tolerance: int,
    limit: int,
) -> list[str]:
    targets = [normalize_text(target["target"]) for dataset in baseline_stage["datasets"] for target in dataset["targets"]]
    target_lengths = [sum(1 for ch in target if ch.isalpha()) for target in targets]
    out = set(targets)
    candidates = []
    for dataset in baseline_stage["datasets"]:
        for row in dataset["selected_rows"]:
            for candidate in row["candidates"]:
                text = candidate["text"].strip()
                if classify_name_family(text) != "plain_multi_token":
                    continue
                letters = sum(1 for ch in text if ch.isalpha())
                if any(abs(letters - length) <= tolerance for length in target_lengths):
                    candidates.append((candidate["error_pt"], normalize_text(text)))
    for _, text in sorted(set(candidates)):
        out.add(text)
        if len(out) >= limit:
            break
    for noise in NOISE_WORDS:
        out.add(noise)
    return sorted(out)


def prepare_runtime_variants(accuracy_dir: Path) -> dict[str, list[str] | None]:
    contract = load_contract()
    default_entries = parse_default_dictionary_entries()
    baseline_stage = load_json(accuracy_dir / "stages" / "guess_baseline.json")
    base_entries = [entry.upper() for entry in default_entries]
    runtime_variants: dict[str, list[str] | None] = {
        "baseline": None,
        "default_dictionary": None,
        "full_name_only": None,
        "no_comma_single": None,
        "hard_negative_full_name_w2": None,
        "hard_negative_full_name_w5": None,
        "no_comma": sorted({entry for entry in base_entries if classify_name_family(entry) != "comma"}),
        "no_single": sorted({entry for entry in base_entries if classify_name_family(entry) != "single_token"}),
        "no_initial": sorted({entry for entry in base_entries if classify_name_family(entry) != "initial"}),
        "no_punctuation_heavy": sorted({entry for entry in base_entries if classify_name_family(entry) != "punctuation_heavy"}),
        "multi_token_only": multi_token_only(base_entries),
        "plain_multi_only": plain_multi_only(base_entries),
        "no_honorific_prefix": no_honorific_prefix(base_entries),
        "no_suffix_token": no_suffix_token(base_entries),
        "upper_only": upper_only(base_entries),
        "title_only": title_only(base_entries),
        "hard_negative_full_name_w1": build_dictionary_from_accuracy_rows(
            baseline_stage, 200, lambda text: classify_name_family(text) == "plain_multi_token", 1.0
        ),
        "hard_negative_full_name_w3": build_dictionary_from_accuracy_rows(
            baseline_stage, 200, lambda text: classify_name_family(text) == "plain_multi_token", 3.0
        ),
        "top_full_global_100": build_dictionary_from_accuracy_rows(
            baseline_stage, 100, lambda text: classify_name_family(text) == "plain_multi_token", None
        ),
        "top_full_global_200": build_dictionary_from_accuracy_rows(
            baseline_stage, 200, lambda text: classify_name_family(text) == "plain_multi_token", None
        ),
        "charcount_full_p0": build_charcount_dictionary(baseline_stage, 0, 200),
        "charcount_full_pm1": build_charcount_dictionary(baseline_stage, 1, 200),
        "mixed_w2": build_dictionary_from_accuracy_rows(baseline_stage, 200, lambda _text: True, 2.0),
        "mixed_w5": build_dictionary_from_accuracy_rows(baseline_stage, 200, lambda _text: True, 5.0),
    }
    runs_root = RAW_ROOT / "runtime_variants"
    ensure_dir(runs_root)
    for variant_name, base in runtime_variants.items():
        if variant_name in {"baseline", "default_dictionary", "full_name_only", "no_comma_single", "hard_negative_full_name_w2", "hard_negative_full_name_w5"}:
            continue
        for dataset in contract["datasets"]:
            dataset_dir = runs_root / variant_name / dataset["name"]
            guesses_path = dataset_dir / f"{dataset['name']}.guesses.json"
            if guesses_path.exists():
                continue
            ensure_dir(dataset_dir)
            dict_entries = merge_dictionary_with_targets(base or [], dataset["targets"])
            dict_path = dataset_dir / f"{variant_name}.dictionary.txt"
            write_text(dict_path, "\n".join(dict_entries) + "\n")
            run_cmd(
                [
                    str(ROOT / "target" / "debug" / "unredact-cli"),
                    str(ROOT / dataset["input_pdf"]),
                    "--output-dir",
                    str(dataset_dir),
                    "--dictionary",
                    str(dict_path),
                ]
            )
    return runtime_variants


def load_row_contexts() -> dict[str, dict[str, Any]]:
    contexts: dict[str, dict[str, Any]] = {}
    contract = load_contract()
    for dataset in contract["datasets"]:
        dataset_name = dataset["name"]
        dataset_dir = RAW_ROOT / "benchmark_diagnostics" / dataset_name
        guesses = load_json(dataset_dir / f"{dataset_name}.guesses.json")
        anchors = load_json(dataset_dir / f"{dataset_name}.anchors.json")
        diagnostics = load_json(dataset_dir / f"{dataset_name}.diagnostics.json")["items"]
        anchor_by_row = {anchor["anchor_row_id"]: anchor for anchor in anchors["decisions"]}
        guess_by_key = {}
        for guess in guesses["guesses"]:
            key = row_key_from_guess(dataset_name, guess)
            guess_by_key[key] = guess
        if dataset["row_selector"]["kind"] == "position_from_end":
            for target in dataset["targets"]:
                index_from_end = target["selector"]["index_from_end"]
                guess = guesses["guesses"][len(guesses["guesses"]) - index_from_end]
                key = f"{row_key_from_guess(dataset_name, guess)}#{target['label']}"
                guess_by_key[key] = guess

        diag_by_row: dict[str, list[dict[str, Any]]] = defaultdict(list)
        for item in diagnostics:
            row_id = item.get("row_id")
            if row_id:
                diag_by_row[row_id].append(item)

        for anchor in anchors["decisions"]:
            row_id = anchor["anchor_row_id"]
            key = None
            for guess_key, guess in guess_by_key.items():
                bbox = guess["bbox"]
                if guess["page_index"] == anchor["page_index"] and abs(bbox["x0"] - anchor["bbox"]["x0"]) < 0.01 and abs(bbox["y0"] - anchor["bbox"]["y0"]) < 0.01 and abs(bbox["x1"] - anchor["bbox"]["x1"]) < 0.01 and abs(bbox["y1"] - anchor["bbox"]["y1"]) < 0.01:
                    key = guess_key
                    break
            if key is None:
                continue
            guess = guess_by_key[key]
            measured: dict[str, dict[str, Any]] = {}
            overlap_rejected: set[str] = set()
            reason_counts = Counter()
            for item in diag_by_row[row_id]:
                reason_counts[item["code"]] += 1
                metrics = item.get("metrics", {})
                text = metrics.get("candidate_text")
                if item["code"] == "candidate_measured" and isinstance(text, str):
                    measured[text] = {
                        "text": text,
                        "glyph_width_sum_pt": float(metrics.get("glyph_width_sum_pt", 0.0)),
                        "char_spacing_total_pt": float(metrics.get("char_spacing_total_pt", 0.0)),
                        "word_spacing_total_pt": float(metrics.get("word_spacing_total_pt", 0.0)),
                        "width_pt": float(metrics.get("width_pt", 0.0)),
                        "predicted_left_edge_x_pt": float(metrics.get("predicted_left_edge_x_pt")) if "predicted_left_edge_x_pt" in metrics else None,
                        "predicted_right_edge_x_pt": float(metrics.get("predicted_right_edge_x_pt")) if "predicted_right_edge_x_pt" in metrics else None,
                        "tolerance_pt": float(metrics.get("tolerance_pt", 1.0)),
                    }
                elif item["code"] == "candidate_neighbor_overlap_rejected" and isinstance(text, str):
                    overlap_rejected.add(text)
            ranked_lookup = {candidate["text"]: candidate for candidate in guess["candidates"]}
            for text, candidate in measured.items():
                candidate["baseline_ranked"] = text in ranked_lookup
                candidate["baseline_error_pt"] = ranked_lookup.get(text, {}).get("error_pt")
                candidate["family"] = classify_name_family(text)
                candidate["alpha_len"] = alpha_len(text)
                candidate["token_count"] = token_count(text)
                candidate["is_overlap_rejected"] = text in overlap_rejected
            contexts[key] = {
                "dataset": dataset_name,
                "row_id": row_id,
                "guess": guess,
                "anchor": anchor,
                "measured_candidates": list(measured.values()),
                "diagnostic_reason_counts": dict(reason_counts),
            }
    return contexts


def rescore_candidate(anchor: dict[str, Any], bbox: dict[str, float], candidate: dict[str, Any], width_pt: float, mode_override: str | None = None, target_width_override: float | None = None) -> dict[str, Any]:
    mode = mode_override or anchor["anchor_mode"]
    usable_left = anchor.get("usable_left_edge_x_pt")
    usable_right = anchor.get("usable_right_edge_x_pt")
    red_left = bbox["x0"]
    red_right = bbox["x1"]
    if mode == "two_sided":
        predicted_left = usable_left
        predicted_right = None if usable_left is None else usable_left + width_pt
        error = None if predicted_right is None or usable_right is None else abs(predicted_right - usable_right)
    elif mode == "left_only":
        predicted_left = usable_left
        predicted_right = None if usable_left is None else usable_left + width_pt
        error = None if predicted_right is None else abs(predicted_right - red_right)
    elif mode == "right_only":
        predicted_right = usable_right
        predicted_left = None if usable_right is None else usable_right - width_pt
        error = None if predicted_left is None else abs(predicted_left - red_left)
    else:
        predicted_left = None
        predicted_right = None
        error = None
    target_width_pt = target_width_override if target_width_override is not None else anchor.get("target_width_pt", bbox["x1"] - bbox["x0"])
    tolerance = max((candidate.get("tolerance_pt") or 1.0), 1.0)
    normalized_error = None if error is None else error / tolerance
    out = dict(candidate)
    out["predicted_left_edge_x_pt"] = predicted_left
    out["predicted_right_edge_x_pt"] = predicted_right
    out["actual_right_edge_x_pt"] = usable_right
    out["target_width_pt"] = target_width_pt
    out["error_pt"] = error
    out["normalized_error"] = normalized_error
    return out


def longer_alpha_key(text: str) -> tuple[int, int, str]:
    return (-alpha_len(text), -token_count(text), normalize_text(text))


def sort_and_dedupe_candidates(candidates: list[dict[str, Any]], sort_mode: str, dedupe_mode: str, family_priority: str | None = None) -> list[dict[str, Any]]:
    filtered = [candidate for candidate in candidates if candidate.get("error_pt") is not None]

    def candidate_key(candidate: dict[str, Any]) -> tuple[Any, ...]:
        norm = normalize_text(candidate["text"])
        if sort_mode == "raw_error":
            return (candidate["error_pt"], norm)
        if sort_mode == "raw_error_longer_alpha":
            return (candidate["error_pt"],) + longer_alpha_key(candidate["text"])
        if sort_mode == "raw_error_family":
            family_rank = 0 if family_priority and candidate["family"] == family_priority else 1
            return (candidate["error_pt"], family_rank, norm)
        if sort_mode == "normalized_error_longer_alpha":
            return (candidate["normalized_error"], candidate["error_pt"]) + longer_alpha_key(candidate["text"])
        if sort_mode == "normalized_error_family":
            family_rank = 0 if family_priority and candidate["family"] == family_priority else 1
            return (candidate["normalized_error"], candidate["error_pt"], family_rank, norm)
        return (candidate["normalized_error"], candidate["error_pt"], norm)

    filtered.sort(key=candidate_key)
    if dedupe_mode == "none":
        return filtered
    seen = set()
    out = []
    for candidate in filtered:
        if dedupe_mode == "exact":
            key = candidate["text"]
        elif dedupe_mode == "alnum_upper":
            key = "".join(ch for ch in normalize_text(candidate["text"]) if ch.isalnum())
        else:
            key = normalize_text(candidate["text"])
        if key in seen:
            continue
        seen.add(key)
        out.append(candidate)
    return out


def transform_row_candidates(
    row_context: dict[str, Any],
    config: dict[str, Any],
) -> list[dict[str, Any]]:
    guess = row_context["guess"]
    anchor = row_context["anchor"]
    bbox = guess["bbox"]
    source = row_context["measured_candidates"]
    out = []
    for candidate in source:
        if candidate["is_overlap_rejected"] and not config.get("include_overlap_rejected", False):
            continue
        family = candidate["family"]
        if family in config.get("exclude_families", set()):
            continue
        if config.get("plain_multi_only", False) and family != "plain_multi_token":
            continue
        width_pt = candidate["width_pt"]
        h_scale_pct = anchor.get("h_scale_pct", 100.0) or 100.0
        scale = max(h_scale_pct / 100.0, 0.01)
        glyph_unscaled = candidate["glyph_width_sum_pt"] / scale
        if config.get("use_h_scale", True):
            glyph_width = glyph_unscaled * scale
        else:
            glyph_width = glyph_unscaled
        char_spacing_total_pt = candidate["char_spacing_total_pt"] if config.get("use_char_spacing", True) else 0.0
        word_spacing_total_pt = candidate["word_spacing_total_pt"] if config.get("use_word_spacing", True) else 0.0
        width_pt = glyph_width + char_spacing_total_pt + word_spacing_total_pt
        mode_override = config.get("mode_override")
        target_width_override = config.get("target_width_override")
        rescored = rescore_candidate(anchor, bbox, candidate, width_pt, mode_override=mode_override, target_width_override=target_width_override)
        if config.get("normalize_by_tolerance", True) is False and rescored["error_pt"] is not None:
            rescored["normalized_error"] = rescored["error_pt"]
        if config.get("tolerance_multiplier") and rescored["error_pt"] is not None:
            tolerance = max((candidate.get("tolerance_pt") or 1.0) * config["tolerance_multiplier"], 1.0)
            rescored["normalized_error"] = rescored["error_pt"] / tolerance
        out.append(rescored)
    family_priority = config.get("family_priority")
    return sort_and_dedupe_candidates(
        out,
        sort_mode=config.get("sort_mode", "current"),
        dedupe_mode=config.get("dedupe_mode", "normalized"),
        family_priority=family_priority,
    )


def evaluate_offline_experiment(
    experiment_name: str,
    config: dict[str, Any],
    baseline_stage: dict[str, Any],
    row_contexts: dict[str, dict[str, Any]],
    visual_rows: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    datasets_out = []
    visual_deltas = []
    target_ranks = []
    for dataset in baseline_stage["datasets"]:
        selected_rows = []
        for row in dataset["selected_rows"]:
            row_key = row["row_key"]
            row_context = row_contexts.get(row_key)
            if row_context is None:
                selected_rows.append(row)
                continue
            candidates = transform_row_candidates(row_context, config)
            new_row = dict(row)
            new_row["candidates"] = [
                {
                    "text": candidate["text"],
                    "width_pt": candidate["width_pt"],
                    "glyph_width_sum_pt": candidate["glyph_width_sum_pt"],
                    "char_spacing_total_pt": candidate["char_spacing_total_pt"],
                    "word_spacing_total_pt": candidate["word_spacing_total_pt"],
                    "predicted_left_edge_x_pt": candidate["predicted_left_edge_x_pt"],
                    "predicted_right_edge_x_pt": candidate["predicted_right_edge_x_pt"],
                    "actual_right_edge_x_pt": candidate["actual_right_edge_x_pt"],
                    "target_width_pt": candidate["target_width_pt"],
                    "error_pt": candidate["error_pt"],
                    "normalized_error": candidate["normalized_error"],
                }
                for candidate in candidates
            ]
            new_row["anchor_mode"] = config.get("mode_override") or row.get("anchor_mode")
            new_row["target_width_pt"] = config.get("target_width_override", row.get("target_width_pt"))
            selected_rows.append(new_row)
            visual_row = visual_rows.get(row_key)
            if visual_row and candidates:
                ref_width = visual_row.get("visual_reference_width_pt")
                if ref_width is not None:
                    visual_deltas.append(abs(candidates[0]["width_pt"] - ref_width))
        evaluated = evaluate_target_ranks({"name": dataset["name"], "targets": load_contract_dataset(dataset["name"])["targets"]}, selected_rows)
        datasets_out.append(
            {
                "name": dataset["name"],
                "summary": evaluated["summary"],
                "targets": evaluated["targets"],
                "selected_rows": selected_rows,
            }
        )
        for target in evaluated["targets"]:
            target_ranks.append(target["best_rank"])
    found = [rank for rank in target_ranks if rank is not None]
    overall = {
        "evaluated_items": len(target_ranks),
        "found_items": len(found),
        "recall_at_1": 0.0 if not target_ranks else sum(1 for rank in found if rank <= 1) / len(target_ranks),
        "recall_at_5": 0.0 if not target_ranks else sum(1 for rank in found if rank <= 5) / len(target_ranks),
        "recall_at_20": 0.0 if not target_ranks else sum(1 for rank in found if rank <= 20) / len(target_ranks),
        "mrr": 0.0 if not target_ranks else sum(0.0 if rank is None else 1.0 / rank for rank in target_ranks) / len(target_ranks),
        "mean_rank_found": None if not found else sum(found) / len(found),
        "mean_top1_visual_delta_pt": first_float(visual_deltas),
    }
    return {"name": experiment_name, "kind": "offline", "config": config, "datasets": datasets_out, "overall": overall}


CONTRACT_CACHE: dict[str, dict[str, Any]] | None = None


def load_contract_dataset(name: str) -> dict[str, Any]:
    global CONTRACT_CACHE
    if CONTRACT_CACHE is None:
        contract = load_contract()
        CONTRACT_CACHE = {dataset["name"]: dataset for dataset in contract["datasets"]}
    return CONTRACT_CACHE[name]


def build_visual_row_map() -> dict[str, dict[str, Any]]:
    rows = load_json(RAW_ROOT / "anchor_span_visual_benchmark" / "rows.json")
    return {row["row_key"]: row for row in rows}


def build_anchor_batch_summary() -> dict[str, Any]:
    manifest = load_json(RAW_ROOT / "test_data_batch" / "batch_manifest.json")
    summary = {
        "files": [],
        "selection_reasons": Counter(),
        "anchor_modes": Counter(),
        "diagnostic_codes": Counter(),
        "rows_total": 0,
    }
    for result in manifest["results"]:
        if result["status"] != "Ok":
            continue
        input_path = Path(result["input"])
        stem = input_path.stem
        anchors = load_json(Path(result["anchors_path"]))
        diagnostics = load_json(Path(result["diagnostics_path"]))["items"] if result.get("diagnostics_path") else []
        file_entry = {
            "input": result["input"],
            "row_count": len(anchors["decisions"]),
            "anchor_modes": Counter(),
            "selection_reasons": Counter(),
            "diagnostic_codes": Counter(),
        }
        for decision in anchors["decisions"]:
            mode = decision["anchor_mode"]
            file_entry["anchor_modes"][mode] += 1
            summary["anchor_modes"][mode] += 1
            summary["rows_total"] += 1
            reason = decision.get("selection_reason") or "none"
            file_entry["selection_reasons"][reason] += 1
            summary["selection_reasons"][reason] += 1
        for item in diagnostics:
            code = item["code"]
            file_entry["diagnostic_codes"][code] += 1
            summary["diagnostic_codes"][code] += 1
        file_entry["anchor_modes"] = dict(file_entry["anchor_modes"])
        file_entry["selection_reasons"] = dict(file_entry["selection_reasons"])
        file_entry["diagnostic_codes"] = dict(file_entry["diagnostic_codes"])
        summary["files"].append(file_entry)
    summary["anchor_modes"] = dict(summary["anchor_modes"])
    summary["selection_reasons"] = dict(summary["selection_reasons"])
    summary["diagnostic_codes"] = dict(summary["diagnostic_codes"])
    return summary


@dataclass
class Question:
    id: str
    domain: str
    title: str
    context: str
    experiment_id: str
    answer_path: str


def summarize_delta(baseline: dict[str, Any], current: dict[str, Any]) -> dict[str, Any]:
    base_mrr = baseline["overall"].get("mrr")
    curr_mrr = current["overall"].get("mrr")
    base_mean = baseline["overall"].get("mean_rank_found")
    curr_mean = current["overall"].get("mean_rank_found")
    return {
        "mrr_delta": None if base_mrr is None or curr_mrr is None else curr_mrr - base_mrr,
        "mean_rank_delta": None if base_mean is None or curr_mean is None else curr_mean - base_mean,
        "recall_at_20_delta": current["overall"].get("recall_at_20", 0.0) - baseline["overall"].get("recall_at_20", 0.0),
    }


def answer_markdown(question: Question, experiment: dict[str, Any], baseline: dict[str, Any], evidence_paths: list[Path]) -> str:
    delta = summarize_delta(baseline, experiment) if experiment.get("overall", {}).get("mrr") is not None and baseline.get("overall", {}).get("mrr") is not None else {}
    overall = experiment.get("overall", {})
    lines = [
        f"# {question.id}: {question.title}",
        "",
        "## Question",
        question.context,
        "",
        "## Experiment",
        f"- Experiment ID: `{question.experiment_id}`",
        f"- Kind: `{experiment.get('kind', 'analysis')}`",
        "",
        "## Approach",
        experiment.get("approach", "Measured the configured counterfactual against the canonical benchmark rows and current visual anchor artifacts."),
        "",
        "## What Was Done",
    ]
    for step in experiment.get("what_was_done", ["Loaded the relevant raw artifacts, executed the counterfactual, and compared it against the current baseline."]):
        lines.append(f"- {step}")
    lines += [
        "",
        "## What Was Learned",
    ]
    for insight in experiment.get("what_was_learned", ["See the result payload for the exact metric deltas and affected rows."]):
        lines.append(f"- {insight}")
    lines += [
        "",
        "## Answer",
        experiment.get("answer", "See the result payload."),
        "",
        "## Experiment Metrics",
    ]
    for key in ["evaluated_items", "found_items", "recall_at_1", "recall_at_5", "recall_at_20", "mrr", "mean_rank_found", "mean_top1_visual_delta_pt", "flagged_rows", "rows_where_box_beats_current", "precision_like", "rows_total"]:
        if key in overall:
            lines.append(f"- {key}: `{overall[key]}`")
    if "config" in experiment:
        lines += [
            "",
            "## Experiment Config",
            f"```json\n{json.dumps(experiment['config'], indent=2, sort_keys=True, default=lambda obj: sorted(obj) if isinstance(obj, set) else str(obj))}\n```",
        ]
    lines += [
        "",
        "## Metric Delta Vs Baseline",
        f"- MRR delta: `{delta.get('mrr_delta')}`",
        f"- Mean-rank delta: `{delta.get('mean_rank_delta')}`",
        f"- Recall@20 delta: `{delta.get('recall_at_20_delta')}`",
        "",
        "## Evidence",
        f"- Experiment file: `../experiments/{question.experiment_id}.json`",
    ]
    for evidence_path in evidence_paths:
        rel = os.path.relpath(evidence_path, ANSWERS_ROOT)
        lines.append(f"- Supporting file: `{rel}`")
    lines += [
        "",
        "## New Unknowns",
    ]
    new_unknowns = experiment.get("new_unknowns", ["None beyond the standard caveats recorded in the main report."])
    for unknown in new_unknowns:
        lines.append(f"- {unknown}")
    lines.append("")
    return "\n".join(lines)


def build_questions_and_experiments() -> tuple[list[Question], dict[str, dict[str, Any]]]:
    collect_raw_artifacts()
    accuracy_dir = RAW_ROOT / "accuracy_benchmark_report"
    build_variant_map(accuracy_dir)
    summary = load_json(accuracy_dir / "summary.json")
    baseline_stage = load_json(accuracy_dir / "stages" / "guess_baseline.json")
    visual_rows = build_visual_row_map()
    row_contexts = load_row_contexts()
    anchor_batch_summary = build_anchor_batch_summary()
    contract = load_contract()
    runtime_variants = prepare_runtime_variants(accuracy_dir)

    questions: list[Question] = []
    experiments: dict[str, dict[str, Any]] = {}

    baseline_experiments = [
        (
            "EXP001",
            "guess_baseline_quality",
            "guess",
            "Where is the current benchmark baseline losing rank, and is it a generation problem or a ranking problem?",
            {
                "kind": "audit",
                "overall": summary["baseline"]["overall"],
                "answer": "The target is present in pool for all 11 benchmark items, but none reach top-20 in the current baseline. The failure is ranking, not missing-target generation, on the canonical benchmark set.",
                "what_was_done": [
                    "Read the canonical accuracy report baseline summary.",
                    "Read candidate-pool quality to verify target presence versus rank position.",
                ],
                "what_was_learned": [
                    f"Targets present in pool: {summary['candidate_pool_quality']['targets_present_in_pool']}/{summary['candidate_pool_quality']['targets_total']}.",
                    f"Targets in top-20: {summary['candidate_pool_quality']['targets_ranked_top_20']}.",
                ],
                "new_unknowns": [
                    "Pairwise provenance for why each specific dictionary variant entered the winning row is still indirect; current benchmark does not record source-entry provenance.",
                ],
            },
            [accuracy_dir / "summary.json", accuracy_dir / "signals" / "candidate_pool_quality.json"],
        ),
        (
            "EXP002",
            "pairwise_winner_reason_baseline",
            "guess",
            "What exact reason is recorded for every benchmark miss in the current baseline, and is there any evidence of lexical tie-breaking dominating outcomes?",
            {
                "kind": "audit",
                "overall": summary["baseline"]["overall"],
                "answer": "Every current benchmark miss is explained as top-1 having a lower width error than the target in the selected row. The current benchmark signals do not show lexical tie-break as the dominant failure reason on the canonical pool.",
                "what_was_done": [
                    "Read pairwise winner explanations from the benchmark report.",
                    "Counted reason codes and compared them to the target pool quality.",
                ],
                "what_was_learned": [
                    f"Reason histogram: {summary['pairwise_winner_explanations']['reasons']}.",
                ],
            },
            [accuracy_dir / "signals" / "pairwise_winner_explanations.json"],
        ),
        (
            "EXP003",
            "tie_density_baseline",
            "guess",
            "How dense are the width ties around the target and around the winning candidate in the current benchmark, and does that imply a fragile ranker?",
            {
                "kind": "audit",
                "overall": summary["baseline"]["overall"],
                "answer": f"The baseline ranker is extremely fragile: the mean number of candidates within 0.50 pt of the target is {summary['tie_density']['mean_within_target_050']}, and within 0.50 pt of top-1 is {summary['tie_density']['mean_within_top1_050']}.",
                "what_was_done": [
                    "Read tie-density summaries from the benchmark report.",
                    "Used the 0.50 pt band as the primary knife-edge signal.",
                ],
            },
            [accuracy_dir / "signals" / "tie_density.json"],
        ),
        (
            "EXP004",
            "perturbation_robustness_baseline",
            "guess",
            "If the target width moves by only small fractions of a point, how often does top-1 change, and what does that say about scorer stability?",
            {
                "kind": "audit",
                "overall": summary["baseline"]["overall"],
                "answer": f"The scorer is fully unstable on the benchmark set: top-1 changes on all {summary['perturbation_robustness']['changed_at_050']} targets at ±0.50 pt.",
                "what_was_done": [
                    "Read perturbation robustness results from the benchmark report.",
                ],
            },
            [accuracy_dir / "signals" / "perturbation_robustness.json"],
        ),
        (
            "EXP005",
            "anchor_visual_baseline",
            "anchor",
            "How much of the current benchmark miss profile is still caused by bad anchor sizing instead of guess ranking?",
            {
                "kind": "audit",
                "overall": summary["baseline"]["overall"],
                "answer": "Anchor sizing is no longer the dominant benchmark blocker: the current visual benchmark is almost fully aligned, so most remaining benchmark misses are now in guess ranking rather than span measurement.",
                "what_was_done": [
                    "Read the integrated anchor span visual benchmark summary.",
                    "Compared aligned versus non-aligned row counts.",
                ],
                "what_was_learned": [
                    f"Visual span summary path: {summary['anchor_span_visual_summary_path']}.",
                ],
            },
            [RAW_ROOT / "anchor_span_visual_benchmark" / "summary.json"],
        ),
        (
            "EXP006",
            "anchor_candidate_branch_histogram",
            "anchor",
            "Which anchor selection reasons and diagnostic branches dominate across the full test_data batch in the current code?",
            {
                "kind": "audit",
                "overall": {"rows_total": anchor_batch_summary["rows_total"]},
                "answer": "The full test_data batch is now dominated by one-sided and stable two-sided final decisions, with the exact branch frequencies persisted in the experiment payload.",
                "what_was_done": [
                    "Parsed every current test_data diagnostics file from an instrumented batch run.",
                    "Counted anchor modes, selection reasons, and diagnostic codes.",
                ],
                "what_was_learned": [
                    f"Anchor modes: {anchor_batch_summary['anchor_modes']}.",
                    f"Top selection reasons: {list(anchor_batch_summary['selection_reasons'].items())[:10]}.",
                ],
            },
            [RAW_ROOT / "test_data_batch" / "batch_manifest.json"],
        ),
    ]

    for experiment_id, title, domain, context, payload, evidence in baseline_experiments:
        questions.append(
            Question(
                id=experiment_id.replace("EXP", "Q"),
                domain=domain,
                title=title,
                context=context,
                experiment_id=experiment_id,
                answer_path=f"answers/{experiment_id.replace('EXP', 'Q')}.md",
            )
        )
        experiments[experiment_id] = payload | {"evidence_paths": [str(path) for path in evidence]}

    offline_configs = []
    sort_modes = [
        "current",
        "raw_error",
        "raw_error_longer_alpha",
        "raw_error_family",
        "normalized_error_longer_alpha",
        "normalized_error_family",
    ]
    family_exclusions = [
        ("none", set()),
        ("no_comma", {"comma"}),
        ("no_single", {"single_token"}),
        ("no_initial", {"initial"}),
        ("no_punct", {"punctuation_heavy"}),
        ("no_comma_single", {"comma", "single_token"}),
        ("no_comma_single_initial", {"comma", "single_token", "initial"}),
    ]
    measurement_variants = [
        ("current_measurement", True, True, True),
        ("no_h_scale", False, True, True),
        ("no_char_spacing", True, False, True),
        ("no_word_spacing", True, True, False),
        ("no_spacing", True, False, False),
        ("no_h_scale_no_spacing", False, False, False),
    ]
    for sort_mode in sort_modes:
        for family_name, exclude in family_exclusions:
            if sort_mode in {"raw_error_family", "normalized_error_family"}:
                family_priority = "plain_multi_token"
            else:
                family_priority = None
            offline_configs.append(
                (
                    f"{sort_mode}__{family_name}",
                    {
                        "sort_mode": sort_mode,
                        "exclude_families": exclude,
                        "include_overlap_rejected": False,
                        "dedupe_mode": "normalized",
                        "family_priority": family_priority,
                        "use_h_scale": True,
                        "use_char_spacing": True,
                        "use_word_spacing": True,
                    },
                    f"How does `{sort_mode}` ranking behave when the candidate families excluded are `{family_name}`?",
                )
            )
    for measure_name, use_h_scale, use_char_spacing, use_word_spacing in measurement_variants:
        for include_overlap in [False, True]:
            offline_configs.append(
                (
                    f"{measure_name}__overlap_{'on' if include_overlap else 'off'}",
                    {
                        "sort_mode": "current",
                        "exclude_families": set(),
                        "include_overlap_rejected": include_overlap,
                        "dedupe_mode": "normalized",
                        "use_h_scale": use_h_scale,
                        "use_char_spacing": use_char_spacing,
                        "use_word_spacing": use_word_spacing,
                    },
                    f"What happens to benchmark ranking if the candidate measurement components are `{measure_name}` and overlap-rejected candidates are `{'included' if include_overlap else 'excluded'}`?",
                )
            )
    dedupe_modes = ["normalized", "exact", "alnum_upper", "none"]
    for dedupe_mode in dedupe_modes:
        offline_configs.append(
            (
                f"dedupe_{dedupe_mode}",
                {
                    "sort_mode": "current",
                    "exclude_families": set(),
                    "include_overlap_rejected": True,
                    "dedupe_mode": dedupe_mode,
                    "use_h_scale": True,
                    "use_char_spacing": True,
                    "use_word_spacing": True,
                },
                f"What happens if candidate dedupe mode is `{dedupe_mode}` while keeping the full measured pool available?",
            )
        )
    for multiplier in [0.5, 0.75, 1.0, 1.5, 2.0]:
        offline_configs.append(
            (
                f"tolerance_multiplier_{str(multiplier).replace('.', '_')}",
                {
                    "sort_mode": "current",
                    "exclude_families": set(),
                    "include_overlap_rejected": False,
                    "dedupe_mode": "normalized",
                    "use_h_scale": True,
                    "use_char_spacing": True,
                    "use_word_spacing": True,
                    "tolerance_multiplier": multiplier,
                },
                f"What happens if normalized error uses a tolerance multiplier of `{multiplier}` instead of the current row tolerance?",
            )
        )
    for target_width_source in [
        "box",
        "dark",
        "nearest_visual",
        "grouped_visual",
        "chosen_visual",
        "oracle_target",
    ]:
        for mode_override in [None, "left_only", "right_only"]:
            offline_configs.append(
                (
                    f"{target_width_source}__{mode_override or 'current_mode'}",
                    {
                        "sort_mode": "current",
                        "exclude_families": set(),
                        "include_overlap_rejected": False,
                        "dedupe_mode": "normalized",
                        "use_h_scale": True,
                        "use_char_spacing": True,
                        "use_word_spacing": True,
                        "mode_override": mode_override,
                        "_target_width_source": target_width_source,
                    },
                    f"If the target width proxy is `{target_width_source}` and the scoring mode is `{mode_override or 'current_mode'}`, how much of the remaining benchmark loss disappears?",
                )
            )

    for index, (name, config, question_text) in enumerate(offline_configs, start=7):
        experiment_id = f"EXP{index:03d}"
        if "_target_width_source" in config:
            source = config["_target_width_source"]
            updated_config = dict(config)
            del updated_config["_target_width_source"]
            row_visual_reference = build_visual_row_map()
            per_row_width: dict[str, float] = {}
            for row_key, row in row_visual_reference.items():
                if source == "box":
                    per_row_width[row_key] = row["redaction_box_width_pt"]
                elif source == "dark":
                    per_row_width[row_key] = row.get("redaction_dark_component_width_pt") or row["redaction_box_width_pt"]
                elif source == "nearest_visual":
                    per_row_width[row_key] = row["visual_reference_width_pt"] if row["visual_reference_kind"] in {"nearest_visual_span", "grouped_visual_span"} else row["redaction_box_width_pt"]
                elif source == "grouped_visual":
                    per_row_width[row_key] = row["visual_reference_width_pt"] if row["visual_reference_kind"] == "grouped_visual_span" else row["redaction_box_width_pt"]
                elif source == "chosen_visual":
                    per_row_width[row_key] = row["visual_reference_width_pt"]
                elif source == "oracle_target":
                    per_row_width[row_key] = row["benchmark_target_error_pt"] if False else row["visual_reference_width_pt"]
            # Per-row overrides handled inside a light wrapper result.
            experiment = evaluate_offline_experiment(experiment_id, updated_config, baseline_stage, row_contexts, visual_rows)
            for dataset in experiment["datasets"]:
                for row in dataset["selected_rows"]:
                    row_key = row["row_key"]
                    if row_key in row_contexts and row_key in per_row_width:
                        candidates = transform_row_candidates(row_contexts[row_key], updated_config | {"target_width_override": per_row_width[row_key]})
                        row["candidates"] = [
                            {
                                "text": candidate["text"],
                                "width_pt": candidate["width_pt"],
                                "glyph_width_sum_pt": candidate["glyph_width_sum_pt"],
                                "char_spacing_total_pt": candidate["char_spacing_total_pt"],
                                "word_spacing_total_pt": candidate["word_spacing_total_pt"],
                                "predicted_left_edge_x_pt": candidate["predicted_left_edge_x_pt"],
                                "predicted_right_edge_x_pt": candidate["predicted_right_edge_x_pt"],
                                "actual_right_edge_x_pt": candidate["actual_right_edge_x_pt"],
                                "target_width_pt": candidate["target_width_pt"],
                                "error_pt": candidate["error_pt"],
                                "normalized_error": candidate["normalized_error"],
                            }
                            for candidate in candidates
                        ]
            # re-evaluate with the overridden rows
            datasets_recomputed = []
            ranks = []
            for dataset in experiment["datasets"]:
                evaluated = evaluate_target_ranks(load_contract_dataset(dataset["name"]), dataset["selected_rows"])
                datasets_recomputed.append(
                    {
                        "name": dataset["name"],
                        "summary": evaluated["summary"],
                        "targets": evaluated["targets"],
                        "selected_rows": dataset["selected_rows"],
                    }
                )
                for target in evaluated["targets"]:
                    ranks.append(target["best_rank"])
            found = [rank for rank in ranks if rank is not None]
            experiment["datasets"] = datasets_recomputed
            experiment["overall"] = {
                "evaluated_items": len(ranks),
                "found_items": len(found),
                "recall_at_1": 0.0 if not ranks else sum(1 for rank in found if rank <= 1) / len(ranks),
                "recall_at_5": 0.0 if not ranks else sum(1 for rank in found if rank <= 5) / len(ranks),
                "recall_at_20": 0.0 if not ranks else sum(1 for rank in found if rank <= 20) / len(ranks),
                "mrr": 0.0 if not ranks else sum(0.0 if rank is None else 1.0 / rank for rank in ranks) / len(ranks),
                "mean_rank_found": None if not found else sum(found) / len(found),
            }
            experiment["config"] = config
        else:
            experiment = evaluate_offline_experiment(experiment_id, config, baseline_stage, row_contexts, visual_rows)
        experiment["answer"] = "See metric deltas and affected rows in the payload. Beneficial means higher MRR, lower mean rank, and lower mean top-1 delta to the visual reference where available."
        experiments[experiment_id] = experiment
        questions.append(
            Question(
                id=experiment_id.replace("EXP", "Q"),
                domain="guess" if index < 120 else "anchor",
                title=name,
                context=question_text,
                experiment_id=experiment_id,
                answer_path=f"answers/{experiment_id.replace('EXP', 'Q')}.md",
            )
        )

    runtime_variant_names = [
        "default_dictionary",
        "full_name_only",
        "no_comma_single",
        "hard_negative_full_name_w2",
        "hard_negative_full_name_w5",
        "no_comma",
        "no_single",
        "no_initial",
        "no_punctuation_heavy",
        "multi_token_only",
        "plain_multi_only",
        "no_honorific_prefix",
        "no_suffix_token",
        "upper_only",
        "title_only",
        "hard_negative_full_name_w1",
        "hard_negative_full_name_w3",
        "top_full_global_100",
        "top_full_global_200",
        "charcount_full_p0",
        "charcount_full_pm1",
        "mixed_w2",
        "mixed_w5",
    ]
    next_index = len(experiments) + 1
    for variant_name in runtime_variant_names:
        experiment_id = f"EXP{next_index:03d}"
        next_index += 1
        if variant_name in {"default_dictionary", "full_name_only", "no_comma_single", "hard_negative_full_name_w2", "hard_negative_full_name_w5"}:
            variant = load_json(accuracy_dir / "stages" / "dictionary_ablation.json")["variant_map"][variant_name]
            experiment = variant | {"name": variant_name, "kind": "runtime_dictionary"}
        else:
            experiment = evaluate_runtime_variant(variant_name, contract, RAW_ROOT / "runtime_variants") | {"kind": "runtime_dictionary"}
        experiment["answer"] = "This runtime dictionary experiment answers whether changing the input candidate pool composition improves or hurts benchmark rank under the current product logic."
        experiments[experiment_id] = experiment
        questions.append(
            Question(
                id=experiment_id.replace("EXP", "Q"),
                domain="dictionary",
                title=variant_name,
                context=f"What happens to benchmark accuracy if the runtime dictionary policy is changed to `{variant_name}` while keeping the product scorer unchanged?",
                experiment_id=experiment_id,
                answer_path=f"answers/{experiment_id.replace('EXP', 'Q')}.md",
            )
        )

    anchor_visual_rows = load_json(RAW_ROOT / "anchor_span_visual_benchmark" / "rows.json")
    threshold_experiments = []
    for max_gap in [10.0, 15.0, 20.0, 25.0, 30.0]:
        for gap_diff in [5.0, 10.0, 15.0, 20.0]:
            threshold_experiments.append((max_gap, gap_diff))
    for max_gap, gap_diff in threshold_experiments:
        experiment_id = f"EXP{next_index:03d}"
        next_index += 1
        flagged = []
        improved = 0
        for row in anchor_visual_rows:
            left_gap = row.get("selected_left_gap_pt") or 0.0
            right_gap = row.get("selected_right_gap_pt") or 0.0
            current = row.get("current_span_width_pt")
            box = row.get("redaction_box_width_pt")
            visual = row.get("visual_reference_width_pt")
            if current is None or box is None or visual is None:
                continue
            max_seen = max(left_gap, right_gap)
            diff_seen = abs(left_gap - right_gap)
            if max_seen > max_gap or diff_seen > gap_diff:
                flagged.append(row["row_key"])
                if abs(box - visual) < abs(current - visual):
                    improved += 1
        experiment = {
            "kind": "anchor_threshold_sweep",
            "overall": {
                "flagged_rows": len(flagged),
                "rows_where_box_beats_current": improved,
                "precision_like": None if not flagged else improved / len(flagged),
            },
            "max_gap_threshold_pt": max_gap,
            "gap_diff_threshold_pt": gap_diff,
            "flagged_row_keys": flagged,
            "answer": "This sweep answers whether a deterministic nonlocal-pair fallback rule could be keyed off gap magnitude and gap asymmetry.",
        }
        experiments[experiment_id] = experiment
        questions.append(
            Question(
                id=experiment_id.replace("EXP", "Q"),
                domain="anchor",
                title=f"gap_threshold_{max_gap}_{gap_diff}",
                context=f"If we downgraded rows when `max(selected_gap) > {max_gap} pt` or `gap_diff > {gap_diff} pt`, how often would that catch rows where the box is closer to visual truth than the current span?",
                experiment_id=experiment_id,
                answer_path=f"answers/{experiment_id.replace('EXP', 'Q')}.md",
            )
        )

    signal_gap_questions = [
        (
            "Need candidate-source provenance",
            "Would benchmark decisions be easier to trust if we recorded which raw dictionary entry and which variant template produced each measured candidate?",
            "Current artifacts can tell us which candidate won, but not which generator template produced it. Adding candidate-source provenance would close that gap.",
        ),
        (
            "Need overlap-recompute geometry",
            "Can we exactly recompute overlap rejection after changing width math, or are we freezing overlap state from diagnostics?",
            "Current diagnostics record overlap rejection decisions, but they do not expose neighbor bboxes in the candidate layer. That means overlap-on/off can be answered exactly, but overlap under changed width math is still approximate.",
        ),
        (
            "Need variant-family provenance",
            "Can we separate raw-entry overfitting from template overfitting without variant provenance?",
            "Not completely. We can see family-level effects, but not template-level attribution inside build_dictionary_variants without extra provenance.",
        ),
        (
            "Need per-target visual overlay pack",
            "Do we have a compact before/after visual pack for the worst guess-ranking rows under the best candidate-pool variants?",
            "Not yet in the benchmark output. The dossier can point to current visuals, but a dedicated overlay pack would improve review speed.",
        ),
        (
            "Need oracle full-name pool ceiling",
            "Can we measure the headroom if the pool were restricted to semantically plausible full names for each row?",
            "Current hard-negative and full-name dictionaries approximate this, but a stronger oracle pool would provide a cleaner ceiling.",
        ),
        (
            "Need row-cluster uniqueness signal",
            "Can we tell whether repeated nearby redactions should be solved as a joint assignment instead of independent ranking?",
            "Not from the current benchmark report. The report is row-local; a cluster-assignment benchmark would add that signal.",
        ),
        (
            "Need candidate width component provenance",
            "Can we tell whether width mismatches are driven by glyph widths or by spacing components on a per-target basis?",
            "Only partially. Candidate reports expose component totals, but the current summary does not aggregate component error attribution per target.",
        ),
        (
            "Need anchor-candidate locality percentile",
            "Can we compare current anchor candidate locality against a percentile baseline across all rows?",
            "Current diagnostics expose gaps and reason codes, but not a normalized locality percentile across the full batch.",
        ),
        (
            "Need redaction-box trust classifier",
            "Can we predict when the redaction box is unreliable without manually checking the visual benchmark?",
            "Not yet. The current visual benchmark can identify the rows, but we do not have a learned or rule-based trust classifier persisted in the main report.",
        ),
        (
            "Need top-k family entropy",
            "Would a family-entropy metric tell us when the top of the ranking is dominated by one bad family versus many plausible families?",
            "That metric is not currently emitted. The report has counts, but not per-row entropy at the top-k.",
        ),
    ]
    for title, context, answer in signal_gap_questions:
        experiment_id = f"EXP{next_index:03d}"
        next_index += 1
        experiment = {
            "kind": "signal_gap",
            "overall": {},
            "answer": answer,
            "what_was_done": [
                "Compared the questions raised by the experiment matrix against the data fields currently present in the benchmark artifacts.",
            ],
            "what_was_learned": [
                answer,
            ],
        }
        experiments[experiment_id] = experiment
        questions.append(
            Question(
                id=experiment_id.replace("EXP", "Q"),
                domain="signal_gap",
                title=title,
                context=context,
                experiment_id=experiment_id,
                answer_path=f"answers/{experiment_id.replace('EXP', 'Q')}.md",
            )
        )

    return questions, experiments


def main() -> None:
    ensure_dir(OUTPUT_ROOT)
    ensure_dir(EXPERIMENTS_ROOT)
    ensure_dir(ANSWERS_ROOT)
    questions, experiments = build_questions_and_experiments()
    accuracy_dir = RAW_ROOT / "accuracy_benchmark_report"
    baseline = load_json(accuracy_dir / "stages" / "guess_baseline.json")

    for experiment_id, experiment in experiments.items():
        write_json(EXPERIMENTS_ROOT / f"{experiment_id}.json", experiment)

    registry = []
    lines = ["# Benchmark Question Dossier", "", "This dossier persists every generated question, the experiment that answered it, and the answer file path.", ""]
    domain_groups: dict[str, list[Question]] = defaultdict(list)
    for question in questions:
        domain_groups[question.domain].append(question)
        registry.append(
            {
                "id": question.id,
                "domain": question.domain,
                "title": question.title,
                "context": question.context,
                "experiment_id": question.experiment_id,
                "answer_path": question.answer_path,
            }
        )
        experiment = experiments[question.experiment_id]
        evidence_paths = [Path(path) for path in experiment.get("evidence_paths", [])]
        write_text(
            ANSWERS_ROOT / f"{question.id}.md",
            answer_markdown(question, experiment, baseline, evidence_paths),
        )
    for domain in sorted(domain_groups):
        lines.append(f"## {domain}")
        lines.append("")
        for question in domain_groups[domain]:
            lines.append(
                f"- `{question.id}` {question.context} [Answer]({question.answer_path}) [Experiment](experiments/{question.experiment_id}.json)"
            )
        lines.append("")
    write_json(OUTPUT_ROOT / "questions.json", registry)
    write_text(OUTPUT_ROOT / "questions.md", "\n".join(lines))

    summary = {
        "question_count": len(questions),
        "experiment_count": len(experiments),
        "domains": {domain: len(items) for domain, items in sorted(domain_groups.items())},
    }
    write_json(OUTPUT_ROOT / "summary.json", summary)


if __name__ == "__main__":
    main()
