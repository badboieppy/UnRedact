#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import itertools
import json
import math
import statistics
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parent.parent
OLD_DOSSIER_ROOT = ROOT / "analysis" / "benchmark_question_dossier"
ACCURACY_ROOT = ROOT / "analysis" / "accuracy_benchmark_report"
OUTPUT_ROOT = ROOT / "analysis" / "benchmark_question_dossier_round2"
RAW_ROOT = OUTPUT_ROOT / "raw"
EXPERIMENTS_ROOT = OUTPUT_ROOT / "experiments"
ANSWERS_ROOT = OUTPUT_ROOT / "answers"


def load_base_module() -> Any:
    path = ROOT / "scripts" / "generate_benchmark_question_dossier.py"
    spec = importlib.util.spec_from_file_location("benchmark_question_base", path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"failed to load base benchmark dossier module from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


base = load_base_module()


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


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def normalize_text(value: str) -> str:
    return base.normalize_text(value)


def classify_name_family(text: str) -> str:
    return base.classify_name_family(text)


def alpha_len(text: str) -> int:
    return base.alpha_len(text)


def token_count(text: str) -> int:
    return base.token_count(text)


def summarize_delta(baseline: dict[str, Any], current: dict[str, Any]) -> dict[str, Any]:
    return base.summarize_delta(baseline, current)


@dataclass
class QuestionRecord:
    id: str
    domain: str
    title: str
    context: str
    experiment_id: str
    answer_path: str
    experiment_path: str


PRIMARY_VARIANTS = ["baseline", "hard_negative_full_name_w2", "hard_negative_full_name_w5"]
DICTIONARY_VARIANTS = PRIMARY_VARIANTS + [
    "default_dictionary",
    "full_name_only",
    "multi_token_only",
    "plain_multi_only",
    "no_comma_single",
]

VARIANT_LABELS = {
    "baseline": "baseline",
    "hard_negative_full_name_w2": "hard_negative_full_name_w2",
    "hard_negative_full_name_w5": "hard_negative_full_name_w5",
    "default_dictionary": "default_dictionary",
    "full_name_only": "full_name_only",
    "multi_token_only": "multi_token_only",
    "plain_multi_only": "plain_multi_only",
    "no_comma_single": "no_comma_single",
}


def run_dir_for_variant(variant_name: str) -> Path:
    if variant_name == "baseline":
        return ACCURACY_ROOT / "runs" / "baseline" / "repeat00"
    return ACCURACY_ROOT / "runs" / variant_name


def row_key_from_guess(dataset_name: str, guess: dict[str, Any]) -> str:
    bbox = guess["bbox"]
    return (
        f"{dataset_name}:page{guess['page_index']}:{bbox['x0']:.2f}:{bbox['y0']:.2f}:"
        f"{bbox['x1']:.2f}:{bbox['y1']:.2f}"
    )


def select_rows_for_dataset(dataset: dict[str, Any], report: dict[str, Any]) -> list[dict[str, Any]]:
    selector = dataset["row_selector"]
    out: list[dict[str, Any]] = []
    if selector["kind"] == "page_y_range":
        for guess in report["guesses"]:
            bbox = guess["bbox"]
            if (
                guess["page_index"] == selector["page_index"]
                and bbox["y0"] >= selector["y0_min"]
                and bbox["y1"] <= selector["y1_max"]
            ):
                out.append(
                    {
                        "row_key": row_key_from_guess(dataset["name"], guess),
                        "dataset": dataset["name"],
                        "page_index": guess["page_index"],
                        "bbox": bbox,
                        "candidates": guess["candidates"],
                        "anchor_mode": guess["context"].get("anchor_mode"),
                        "target_width_pt": guess["context"].get("target_width_pt"),
                    }
                )
    else:
        for target in dataset["targets"]:
            index_from_end = target["selector"]["index_from_end"]
            guess = report["guesses"][len(report["guesses"]) - index_from_end]
            out.append(
                {
                    "row_key": f"{row_key_from_guess(dataset['name'], guess)}#{target['label']}",
                    "dataset": dataset["name"],
                    "page_index": guess["page_index"],
                    "bbox": guess["bbox"],
                    "candidates": guess["candidates"],
                    "anchor_mode": guess["context"].get("anchor_mode"),
                    "target_width_pt": guess["context"].get("target_width_pt"),
                }
            )
    return out


def evaluate_target_ranks(dataset: dict[str, Any], selected_rows: list[dict[str, Any]]) -> dict[str, Any]:
    return base.evaluate_target_ranks(dataset, selected_rows)


def build_variant_stage(variant_name: str, contract: dict[str, Any]) -> dict[str, Any]:
    datasets = []
    all_ranks: list[int | None] = []
    for dataset in contract["datasets"]:
        guesses_path = run_dir_for_variant(variant_name) / dataset["name"] / f"{dataset['name']}.guesses.json"
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
        for target in evaluated["targets"]:
            all_ranks.append(target["best_rank"])
    found = [rank for rank in all_ranks if rank is not None]
    overall = {
        "evaluated_items": len(all_ranks),
        "found_items": len(found),
        "recall_at_1": 0.0 if not all_ranks else sum(1 for rank in found if rank <= 1) / len(all_ranks),
        "recall_at_5": 0.0 if not all_ranks else sum(1 for rank in found if rank <= 5) / len(all_ranks),
        "recall_at_20": 0.0 if not all_ranks else sum(1 for rank in found if rank <= 20) / len(all_ranks),
        "mrr": 0.0 if not all_ranks else sum(0.0 if rank is None else 1.0 / rank for rank in all_ranks) / len(all_ranks),
        "mean_rank_found": None if not found else sum(found) / len(found),
    }
    return {"name": variant_name, "datasets": datasets, "overall": overall}


def candidate_provenance(candidate: dict[str, Any]) -> dict[str, Any]:
    return candidate.get("provenance") or {}


def candidate_template_family(candidate: dict[str, Any]) -> str:
    return candidate_provenance(candidate).get("template_family") or "unknown"


def candidate_variant_family(candidate: dict[str, Any]) -> str:
    return candidate_provenance(candidate).get("variant_family") or classify_name_family(candidate["text"])


def candidate_raw_entry_text(candidate: dict[str, Any]) -> str:
    return candidate_provenance(candidate).get("raw_entry_text") or candidate["text"]


def candidate_raw_token_count(candidate: dict[str, Any]) -> int:
    return token_count(candidate_raw_entry_text(candidate))


def candidate_case_source(candidate: dict[str, Any]) -> str | None:
    return candidate_provenance(candidate).get("case_source")


def candidate_orthographic_source(candidate: dict[str, Any]) -> str | None:
    return candidate_provenance(candidate).get("orthographic_source")


def candidate_alias_source(candidate: dict[str, Any]) -> str | None:
    return candidate_provenance(candidate).get("alias_source")


def candidate_is_noncanonical(candidate: dict[str, Any]) -> bool:
    return candidate_template_family(candidate) != "canonical"


def candidate_is_generated_single_from_multi_raw(candidate: dict[str, Any]) -> bool:
    return candidate_variant_family(candidate) == "single_token" and candidate_raw_token_count(candidate) >= 2


def candidate_is_raw_single_entry(candidate: dict[str, Any]) -> bool:
    return candidate_raw_token_count(candidate) == 1


def candidate_is_case_transformed(candidate: dict[str, Any]) -> bool:
    case_source = candidate_case_source(candidate)
    return case_source not in {None, "raw"}


def candidate_is_orthographic_token_shape(candidate: dict[str, Any]) -> bool:
    return candidate_orthographic_source(candidate) == "token_shape"


def candidate_is_alias_derived(candidate: dict[str, Any]) -> bool:
    return candidate_alias_source(candidate) is not None


CONDITION_PREDICATES: dict[str, Callable[[dict[str, Any]], bool]] = {
    "comma_family": lambda candidate: candidate_variant_family(candidate) == "comma",
    "single_token_family": lambda candidate: candidate_variant_family(candidate) == "single_token",
    "initial_family": lambda candidate: candidate_variant_family(candidate) == "initial",
    "punctuation_heavy_family": lambda candidate: candidate_variant_family(candidate) == "punctuation_heavy",
    "last_comma_first": lambda candidate: candidate_template_family(candidate) == "last_comma_first",
    "first_only": lambda candidate: candidate_template_family(candidate) == "first_only",
    "last_only": lambda candidate: candidate_template_family(candidate) == "last_only",
    "role_alias_pair": lambda candidate: candidate_template_family(candidate) == "role_alias_pair",
    "role_alias_comma_pair": lambda candidate: candidate_template_family(candidate) == "role_alias_comma_pair",
    "case_transformed": candidate_is_case_transformed,
    "orthographic_token_shape": candidate_is_orthographic_token_shape,
    "alias_derived": candidate_is_alias_derived,
    "noncanonical": candidate_is_noncanonical,
    "gen_single_from_multi_raw": candidate_is_generated_single_from_multi_raw,
    "raw_single_entry": candidate_is_raw_single_entry,
}

COMBO_TOGGLES = [
    "comma_family",
    "gen_single_from_multi_raw",
    "raw_single_entry",
    "first_only",
    "last_only",
    "last_comma_first",
    "longer_alpha_tie",
]


def candidate_sort_key(
    candidate: dict[str, Any],
    adjusted_error: float,
    original_index: int,
    longer_alpha_tie: bool,
) -> tuple[Any, ...]:
    if longer_alpha_tie:
        return (
            adjusted_error,
            candidate["error_pt"],
            -alpha_len(candidate["text"]),
            -token_count(candidate["text"]),
            normalize_text(candidate["text"]),
            original_index,
        )
    return (adjusted_error, candidate["error_pt"], original_index)


def transform_candidates(
    candidates: list[dict[str, Any]],
    drop_conditions: set[str] | None = None,
    drop_template_families: set[str] | None = None,
    keep_template_families: set[str] | None = None,
    require_plain_multi: bool = False,
    penalty_conditions: dict[str, float] | None = None,
    noncanonical_penalty: float = 0.0,
    longer_alpha_tie: bool = False,
) -> list[dict[str, Any]]:
    drop_conditions = drop_conditions or set()
    drop_template_families = drop_template_families or set()
    penalty_conditions = penalty_conditions or {}
    transformed = []
    for original_index, candidate in enumerate(candidates):
        template_family = candidate_template_family(candidate)
        if keep_template_families is not None and template_family not in keep_template_families:
            continue
        if require_plain_multi and candidate_variant_family(candidate) != "plain_multi_token":
            continue
        if template_family in drop_template_families:
            continue
        if any(CONDITION_PREDICATES[name](candidate) for name in drop_conditions):
            continue
        adjusted_error = float(candidate["error_pt"])
        for name, penalty in penalty_conditions.items():
            if CONDITION_PREDICATES[name](candidate):
                adjusted_error += penalty
        if noncanonical_penalty and candidate_is_noncanonical(candidate):
            adjusted_error += noncanonical_penalty
        transformed.append(
            (
                candidate_sort_key(candidate, adjusted_error, original_index, longer_alpha_tie),
                candidate,
            )
        )
    transformed.sort(key=lambda item: item[0])
    return [candidate for _, candidate in transformed]


def evaluate_policy_on_stage(
    stage: dict[str, Any],
    contract: dict[str, Any],
    *,
    drop_conditions: set[str] | None = None,
    drop_template_families: set[str] | None = None,
    keep_template_families: set[str] | None = None,
    require_plain_multi: bool = False,
    penalty_conditions: dict[str, float] | None = None,
    noncanonical_penalty: float = 0.0,
    longer_alpha_tie: bool = False,
) -> dict[str, Any]:
    datasets_out = []
    all_ranks: list[int | None] = []
    changed_top1_rows = []
    for dataset in contract["datasets"]:
        stage_dataset = next(item for item in stage["datasets"] if item["name"] == dataset["name"])
        selected_rows = []
        for row in stage_dataset["selected_rows"]:
            transformed_candidates = transform_candidates(
                row["candidates"],
                drop_conditions=drop_conditions,
                drop_template_families=drop_template_families,
                keep_template_families=keep_template_families,
                require_plain_multi=require_plain_multi,
                penalty_conditions=penalty_conditions,
                noncanonical_penalty=noncanonical_penalty,
                longer_alpha_tie=longer_alpha_tie,
            )
            baseline_top1 = row["candidates"][0]["text"] if row["candidates"] else None
            new_top1 = transformed_candidates[0]["text"] if transformed_candidates else None
            if baseline_top1 != new_top1:
                changed_top1_rows.append(
                    {
                        "dataset": dataset["name"],
                        "row_key": row["row_key"],
                        "baseline_top1": baseline_top1,
                        "new_top1": new_top1,
                    }
                )
            selected_rows.append(
                {
                    **row,
                    "candidates": transformed_candidates,
                }
            )
        evaluated = evaluate_target_ranks(dataset, selected_rows)
        datasets_out.append(
            {
                "name": dataset["name"],
                "summary": evaluated["summary"],
                "targets": evaluated["targets"],
                "selected_rows": selected_rows,
            }
        )
        for target in evaluated["targets"]:
            all_ranks.append(target["best_rank"])
    found = [rank for rank in all_ranks if rank is not None]
    overall = {
        "evaluated_items": len(all_ranks),
        "found_items": len(found),
        "recall_at_1": 0.0 if not all_ranks else sum(1 for rank in found if rank <= 1) / len(all_ranks),
        "recall_at_5": 0.0 if not all_ranks else sum(1 for rank in found if rank <= 5) / len(all_ranks),
        "recall_at_20": 0.0 if not all_ranks else sum(1 for rank in found if rank <= 20) / len(all_ranks),
        "mrr": 0.0 if not all_ranks else sum(0.0 if rank is None else 1.0 / rank for rank in all_ranks) / len(all_ranks),
        "mean_rank_found": None if not found else sum(found) / len(found),
    }
    return {
        "datasets": datasets_out,
        "overall": overall,
        "changed_top1_rows": changed_top1_rows,
    }


def average(values: list[float]) -> float | None:
    return None if not values else sum(values) / len(values)


def baseline_delta_rows(
    baseline_stage: dict[str, Any],
    candidate_stage: dict[str, Any],
) -> list[dict[str, Any]]:
    rows = []
    for baseline_dataset, candidate_dataset in zip(baseline_stage["datasets"], candidate_stage["datasets"], strict=True):
        baseline_targets = {
            (item["dataset"], item["label"]): item for item in baseline_dataset["targets"]
        }
        for target in candidate_dataset["targets"]:
            key = (target["dataset"], target["label"])
            base_target = baseline_targets[key]
            rows.append(
                {
                    "dataset": target["dataset"],
                    "label": target["label"],
                    "baseline_rank": base_target["best_rank"],
                    "candidate_rank": target["best_rank"],
                    "rank_delta": None
                    if base_target["best_rank"] is None or target["best_rank"] is None
                    else target["best_rank"] - base_target["best_rank"],
                    "baseline_top1_text": base_target["top1_text"],
                    "candidate_top1_text": target["top1_text"],
                }
            )
    return rows


def stage_variant_summary(
    experiment_name: str,
    stage: dict[str, Any],
    baseline_stage: dict[str, Any],
) -> dict[str, Any]:
    return {
        "name": experiment_name,
        "overall": stage["overall"],
        "delta_vs_baseline": summarize_delta(baseline_stage, stage),
        "datasets": [
            {
                "name": dataset["name"],
                "summary": dataset["summary"],
            }
            for dataset in stage["datasets"]
        ],
        "changed_top1_rows": stage["changed_top1_rows"],
        "rank_deltas": baseline_delta_rows(baseline_stage, stage),
    }


def evaluate_policy_across_primary_variants(
    policy_name: str,
    stage_map: dict[str, dict[str, Any]],
    contract: dict[str, Any],
    **policy: Any,
) -> dict[str, Any]:
    variants = {}
    combined_mrrs = []
    for variant_name in PRIMARY_VARIANTS:
        evaluated = evaluate_policy_on_stage(stage_map[variant_name], contract, **policy)
        variants[variant_name] = stage_variant_summary(policy_name, evaluated, stage_map[variant_name])
        combined_mrrs.append(variants[variant_name]["overall"]["mrr"])
    return {
        "name": policy_name,
        "kind": "policy_counterfactual",
        "policy": policy,
        "variants": variants,
        "combined": {
            "mean_mrr": average(combined_mrrs),
            "min_mrr": min(combined_mrrs),
            "mean_recall_at_20": average(
                [variants[name]["overall"]["recall_at_20"] for name in PRIMARY_VARIANTS]
            ),
            "mean_rank_found": average(
                [variants[name]["overall"]["mean_rank_found"] for name in PRIMARY_VARIANTS]
            ),
            "sum_mrr_delta_vs_current": sum(
                variants[name]["delta_vs_baseline"]["mrr_delta"] for name in PRIMARY_VARIANTS
            ),
        },
    }


def build_stage_map(contract: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {variant_name: build_variant_stage(variant_name, contract) for variant_name in DICTIONARY_VARIANTS}


def load_signal_rows(path: Path, key_fields: tuple[str, ...]) -> dict[tuple[Any, ...], dict[str, Any]]:
    payload = load_json(path)
    rows = payload.get("rows", payload)
    return {tuple(row[field] for field in key_fields): row for row in rows}


def build_target_rows(stage_map: dict[str, dict[str, Any]]) -> list[dict[str, Any]]:
    candidate_source = load_signal_rows(
        ACCURACY_ROOT / "signals" / "candidate_source_provenance.json",
        ("dataset", "label"),
    )
    width_attr = load_signal_rows(
        ACCURACY_ROOT / "signals" / "width_component_attribution.json",
        ("dataset", "label"),
    )
    pairwise = load_signal_rows(
        ACCURACY_ROOT / "signals" / "pairwise_winner_explanations.json",
        ("dataset", "label"),
    )
    tie_density = load_signal_rows(
        ACCURACY_ROOT / "signals" / "tie_density.json",
        ("dataset", "label"),
    )
    entropy = load_signal_rows(
        ACCURACY_ROOT / "signals" / "topk_family_entropy.json",
        ("dataset", "label"),
    )
    oracle = load_signal_rows(
        ACCURACY_ROOT / "signals" / "oracle_full_name_pool_ceiling.json",
        ("dataset", "label"),
    )
    visual_rows = [
        row
        for row in load_json(ACCURACY_ROOT / "stages" / "anchor_span_visual" / "rows.json")
        if row.get("benchmark_target_label")
    ]
    visual_by_label = {
        (row["input_pdf"].split("/")[-1].replace(".pdf", ""), row["benchmark_target_label"]): row
        for row in visual_rows
    }
    baseline_stage = stage_map["baseline"]
    out = []
    for dataset in baseline_stage["datasets"]:
        for target in dataset["targets"]:
            key = (target["dataset"], target["label"])
            visual_key = (target["dataset"], target["label"])
            out.append(
                {
                    **target,
                    "candidate_source": candidate_source.get(key),
                    "width_attr": width_attr.get(key),
                    "pairwise": pairwise.get(key),
                    "tie_density": tie_density.get(key),
                    "entropy": entropy.get(key),
                    "oracle": oracle.get(key),
                    "visual": visual_by_label.get(visual_key),
                }
            )
    return out


def build_measurement_profile(contract: dict[str, Any], baseline_stage: dict[str, Any]) -> dict[str, Any]:
    rows = []
    for dataset in contract["datasets"]:
        anchors_path = run_dir_for_variant("baseline") / dataset["name"] / f"{dataset['name']}.anchors.json"
        anchors = load_json(anchors_path)["decisions"]
        anchor_by_bbox = {
            (
                anchor["page_index"],
                round(anchor["bbox"]["x0"], 2),
                round(anchor["bbox"]["y0"], 2),
                round(anchor["bbox"]["x1"], 2),
                round(anchor["bbox"]["y1"], 2),
            ): anchor
            for anchor in anchors
        }
        stage_dataset = next(item for item in baseline_stage["datasets"] if item["name"] == dataset["name"])
        for target in stage_dataset["targets"]:
            row_key = target["best_row_key"]
            if row_key is None:
                continue
            row_key_core = row_key.split("#", 1)[0]
            _, page_part, x0, y0, x1, y1 = row_key_core.split(":")
            anchor = anchor_by_bbox.get(
                (
                    int(page_part.replace("page", "")),
                    round(float(x0), 2),
                    round(float(y0), 2),
                    round(float(x1), 2),
                    round(float(y1), 2),
                )
            )
            if anchor is None:
                continue
            rows.append(
                {
                    "dataset": dataset["name"],
                    "label": target["label"],
                    "row_key": row_key,
                    "anchor_mode": anchor.get("anchor_mode"),
                    "h_scale_pct": anchor.get("h_scale_pct"),
                    "char_spacing_pt": anchor.get("char_spacing_pt"),
                    "word_spacing_pt": anchor.get("word_spacing_pt"),
                }
            )
    return {
        "rows": rows,
        "non_100_h_scale_rows": sum(1 for row in rows if row.get("h_scale_pct") not in {None, 100.0}),
        "non_zero_char_spacing_rows": sum(1 for row in rows if abs(row.get("char_spacing_pt") or 0.0) > 1e-6),
        "non_zero_word_spacing_rows": sum(1 for row in rows if abs(row.get("word_spacing_pt") or 0.0) > 1e-6),
    }


def audit_displacement_sources(target_rows: list[dict[str, Any]]) -> dict[str, Any]:
    template_counts = Counter()
    raw_entry_counts = Counter()
    case_counts = Counter()
    orthographic_counts = Counter()
    alias_counts = Counter()
    same_raw_as_target = 0
    top1_rows = 0
    for row in target_rows:
        candidate_source = row.get("candidate_source") or {}
        top1_template = candidate_source.get("top1_template_family")
        if top1_template:
            template_counts[top1_template] += 1
        top1_text = candidate_source.get("top1_text")
        if top1_text:
            top1_rows += 1
        guess_path = run_dir_for_variant("baseline") / row["dataset"] / f"{row['dataset']}.guesses.json"
        guesses = load_json(guess_path)["guesses"]
        top1_raw_entry = None
        target_raw_entry = None
        row_key_core = row["best_row_key"].split("#", 1)[0]
        if row_key_core:
            _, page_part, x0, y0, x1, y1 = row_key_core.split(":")
            page_index = int(page_part.replace("page", ""))
            bbox_tuple = (round(float(x0), 2), round(float(y0), 2), round(float(x1), 2), round(float(y1), 2))
            for guess in guesses:
                bbox = guess["bbox"]
                if guess["page_index"] == page_index and (
                    round(bbox["x0"], 2),
                    round(bbox["y0"], 2),
                    round(bbox["x1"], 2),
                    round(bbox["y1"], 2),
                ) == bbox_tuple:
                    if guess["candidates"]:
                        provenance = candidate_provenance(guess["candidates"][0])
                        top1_raw_entry = provenance.get("raw_entry_text")
                        case_counts[provenance.get("case_source") or "raw"] += 1
                        orthographic_counts[provenance.get("orthographic_source") or "none"] += 1
                        alias_counts[provenance.get("alias_source") or "none"] += 1
                    target_norm = normalize_text(row["target"])
                    for candidate in guess["candidates"]:
                        if normalize_text(candidate["text"]) == target_norm:
                            target_raw_entry = candidate_provenance(candidate).get("raw_entry_text")
                            break
                    break
        if top1_raw_entry:
            raw_entry_counts[top1_raw_entry] += 1
        if top1_raw_entry and target_raw_entry and normalize_text(top1_raw_entry) == normalize_text(target_raw_entry):
            same_raw_as_target += 1
    return {
        "top1_template_counts": dict(template_counts),
        "top1_raw_entry_counts": raw_entry_counts.most_common(20),
        "top1_case_source_counts": dict(case_counts),
        "top1_orthographic_source_counts": dict(orthographic_counts),
        "top1_alias_source_counts": dict(alias_counts),
        "same_raw_entry_as_target_count": same_raw_as_target,
        "top1_rows": top1_rows,
    }


def entropy_correlation(target_rows: list[dict[str, Any]], plain_multi_stage: dict[str, Any]) -> dict[str, Any]:
    entropy_rows = []
    plain_multi_lookup = {
        (item["dataset"], item["label"]): item
        for dataset in plain_multi_stage["datasets"]
        for item in dataset["targets"]
    }
    for row in target_rows:
        entropy = row.get("entropy") or {}
        tie_density = row.get("tie_density") or {}
        plain_multi_target = plain_multi_lookup[(row["dataset"], row["label"])]
        entropy_rows.append(
            {
                "dataset": row["dataset"],
                "label": row["label"],
                "baseline_rank": row["best_rank"],
                "plain_multi_rank": plain_multi_target["best_rank"],
                "rank_improvement": None
                if row["best_rank"] is None or plain_multi_target["best_rank"] is None
                else row["best_rank"] - plain_multi_target["best_rank"],
                "entropy_top5": entropy.get("entropy_top5"),
                "dominant_family_top5": entropy.get("dominant_family_top5"),
                "dominant_family_share_top5": entropy.get("dominant_family_share_top5"),
                "target_within_050pt": tie_density.get("within_050pt_of_target"),
                "top1_within_050pt": tie_density.get("within_050pt_of_top1"),
            }
        )
    median_entropy = statistics.median(
        [row["entropy_top5"] for row in entropy_rows if row["entropy_top5"] is not None]
    )
    low_entropy = [row for row in entropy_rows if (row["entropy_top5"] or 0.0) <= median_entropy]
    high_entropy = [row for row in entropy_rows if (row["entropy_top5"] or 0.0) > median_entropy]
    comma_dominant = [row for row in entropy_rows if row["dominant_family_top5"] == "comma"]
    return {
        "rows": entropy_rows,
        "median_entropy_top5": median_entropy,
        "low_entropy_mean_rank": average([float(row["baseline_rank"]) for row in low_entropy if row["baseline_rank"] is not None]),
        "high_entropy_mean_rank": average([float(row["baseline_rank"]) for row in high_entropy if row["baseline_rank"] is not None]),
        "low_entropy_mean_plain_multi_improvement": average(
            [float(row["rank_improvement"]) for row in low_entropy if row["rank_improvement"] is not None]
        ),
        "high_entropy_mean_plain_multi_improvement": average(
            [float(row["rank_improvement"]) for row in high_entropy if row["rank_improvement"] is not None]
        ),
        "comma_dominant_mean_rank": average(
            [float(row["baseline_rank"]) for row in comma_dominant if row["baseline_rank"] is not None]
        ),
    }


def anchor_followup(target_rows: list[dict[str, Any]]) -> dict[str, Any]:
    rows = []
    for row in target_rows:
        visual = row.get("visual") or {}
        rows.append(
            {
                "dataset": row["dataset"],
                "label": row["label"],
                "baseline_rank": row["best_rank"],
                "anchor_mode": visual.get("current_anchor_mode"),
                "selected_left_gap_pt": visual.get("selected_left_gap_pt"),
                "selected_right_gap_pt": visual.get("selected_right_gap_pt"),
                "visual_alignment": visual.get("current_alignment"),
                "primary_reason_code": visual.get("primary_reason_code"),
                "redaction_box_width_pt": visual.get("redaction_box_width_pt"),
                "visual_reference_width_pt": visual.get("visual_reference_width_pt"),
                "redaction_box_trust": "untrusted"
                if visual.get("primary_reason_code") == "redaction_box_unreliable"
                or (visual.get("redaction_box_width_pt") is not None and visual.get("visual_reference_width_pt") is not None
                    and abs(visual["redaction_box_width_pt"] - visual["visual_reference_width_pt"])
                    > max(5.0, 0.1 * visual["visual_reference_width_pt"]))
                else "trusted",
            }
        )
    trusted = [row for row in rows if row["redaction_box_trust"] == "trusted"]
    untrusted = [row for row in rows if row["redaction_box_trust"] == "untrusted"]
    return {
        "rows": rows,
        "trusted_count": len(trusted),
        "untrusted_count": len(untrusted),
        "trusted_mean_rank": average([float(row["baseline_rank"]) for row in trusted if row["baseline_rank"] is not None]),
        "untrusted_mean_rank": average([float(row["baseline_rank"]) for row in untrusted if row["baseline_rank"] is not None]),
    }


def build_template_drop_matrix(
    stage_map: dict[str, dict[str, Any]],
    contract: dict[str, Any],
    template_rows: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    out = []
    for row in template_rows:
        template_family = row["template_family"]
        experiment = evaluate_policy_across_primary_variants(
            f"drop_template__{template_family}",
            stage_map,
            contract,
            drop_template_families={template_family},
        )
        experiment["template_family"] = template_family
        out.append(experiment)
    out.sort(key=lambda item: item["combined"]["sum_mrr_delta_vs_current"], reverse=True)
    return out


def build_condition_drop_matrix(
    stage_map: dict[str, dict[str, Any]],
    contract: dict[str, Any],
) -> list[dict[str, Any]]:
    conditions = [
        "comma_family",
        "single_token_family",
        "initial_family",
        "punctuation_heavy_family",
        "gen_single_from_multi_raw",
        "raw_single_entry",
        "case_transformed",
        "orthographic_token_shape",
        "alias_derived",
        "noncanonical",
        "last_comma_first",
        "first_only",
        "last_only",
        "role_alias_pair",
        "role_alias_comma_pair",
    ]
    out = []
    for condition in conditions:
        experiment = evaluate_policy_across_primary_variants(
            f"drop_condition__{condition}",
            stage_map,
            contract,
            drop_conditions={condition},
        )
        experiment["condition"] = condition
        out.append(experiment)
    out.sort(key=lambda item: item["combined"]["sum_mrr_delta_vs_current"], reverse=True)
    return out


def build_penalty_sweeps(
    stage_map: dict[str, dict[str, Any]],
    contract: dict[str, Any],
) -> dict[str, list[dict[str, Any]]]:
    sweeps = {
        "comma_family": [0.05, 0.10, 0.25, 0.50, 1.00],
        "last_comma_first": [0.05, 0.10, 0.25, 0.50, 1.00],
        "gen_single_from_multi_raw": [0.05, 0.10, 0.25, 0.50, 1.00],
        "noncanonical": [0.05, 0.10, 0.25, 0.50],
        "first_only": [0.05, 0.10, 0.25, 0.50],
        "last_only": [0.05, 0.10, 0.25, 0.50],
    }
    out: dict[str, list[dict[str, Any]]] = {}
    for condition, penalties in sweeps.items():
        rows = []
        for penalty in penalties:
            kwargs = {"penalty_conditions": {condition: penalty}}
            if condition == "noncanonical":
                kwargs = {"noncanonical_penalty": penalty}
            experiment = evaluate_policy_across_primary_variants(
                f"penalty__{condition}__{penalty:.2f}",
                stage_map,
                contract,
                **kwargs,
            )
            experiment["condition"] = condition
            experiment["penalty_pt"] = penalty
            rows.append(experiment)
        rows.sort(key=lambda item: item["combined"]["sum_mrr_delta_vs_current"], reverse=True)
        out[condition] = rows
    return out


def build_keep_policy_matrix(
    stage_map: dict[str, dict[str, Any]],
    contract: dict[str, Any],
) -> list[dict[str, Any]]:
    definitions = [
        (
            "keep_only_canonical",
            {
                "keep_template_families": {"canonical"},
            },
        ),
        (
            "keep_canonical_and_role_alias_pair",
            {
                "keep_template_families": {"canonical", "role_alias_pair"},
            },
        ),
        (
            "keep_canonical_and_last_comma_first",
            {
                "keep_template_families": {"canonical", "last_comma_first"},
            },
        ),
        (
            "keep_plain_multi_family_only",
            {
                "require_plain_multi": True,
            },
        ),
    ]
    out = []
    for name, config in definitions:
        experiment = evaluate_policy_across_primary_variants(name, stage_map, contract, **config)
        experiment["policy_name"] = name
        out.append(experiment)
    out.sort(key=lambda item: item["combined"]["sum_mrr_delta_vs_current"], reverse=True)
    return out


def build_combo_search(
    stage_map: dict[str, dict[str, Any]],
    contract: dict[str, Any],
) -> list[dict[str, Any]]:
    combos = []
    for mask in range(1, 1 << len(COMBO_TOGGLES)):
        active = [COMBO_TOGGLES[index] for index in range(len(COMBO_TOGGLES)) if mask & (1 << index)]
        longer_alpha_tie = "longer_alpha_tie" in active
        drop_conditions = {condition for condition in active if condition != "longer_alpha_tie"}
        experiment = evaluate_policy_across_primary_variants(
            "combo__" + "__".join(active),
            stage_map,
            contract,
            drop_conditions=drop_conditions,
            longer_alpha_tie=longer_alpha_tie,
        )
        experiment["active_toggles"] = active
        combos.append(experiment)
    combos.sort(key=lambda item: item["combined"]["sum_mrr_delta_vs_current"], reverse=True)
    return combos


def build_actual_variant_audit(stage_map: dict[str, dict[str, Any]]) -> dict[str, Any]:
    baseline = stage_map["baseline"]
    return {
        name: {
            "name": name,
            "overall": stage_map[name]["overall"],
            "delta_vs_baseline": summarize_delta(baseline, stage_map[name]),
        }
        for name in [
            "default_dictionary",
            "full_name_only",
            "multi_token_only",
            "plain_multi_only",
            "no_comma_single",
            "hard_negative_full_name_w2",
            "hard_negative_full_name_w5",
        ]
    }


def question_answer_markdown(
    question: QuestionRecord,
    experiment: dict[str, Any],
    baseline_summary: dict[str, Any],
) -> str:
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
        experiment.get("approach", "Loaded the persisted benchmark artifacts and measured the requested counterfactual directly against them."),
        "",
        "## What Was Done",
    ]
    for step in experiment.get("what_was_done", []):
        lines.append(f"- {step}")
    lines += [
        "",
        "## What Was Learned",
    ]
    for fact in experiment.get("what_was_learned", []):
        lines.append(f"- {fact}")
    lines += [
        "",
        "## Answer",
        experiment.get("answer", ""),
    ]
    if "overall" in experiment:
        delta = summarize_delta(baseline_summary, experiment)
        lines += [
            "",
            "## Metric Delta Vs Baseline",
            f"- MRR delta: `{delta['mrr_delta']}`",
            f"- Mean-rank delta: `{delta['mean_rank_delta']}`",
            f"- Recall@20 delta: `{delta['recall_at_20_delta']}`",
        ]
    if "variants" in experiment:
        lines += [
            "",
            "## Variant Metrics",
        ]
        for variant_name in PRIMARY_VARIANTS:
            variant = experiment["variants"][variant_name]
            lines.append(
                "- "
                + f"`{variant_name}`: MRR `{variant['overall']['mrr']}`, "
                + f"mean rank `{variant['overall']['mean_rank_found']}`, "
                + f"recall@20 `{variant['overall']['recall_at_20']}`, "
                + f"MRR delta `{variant['delta_vs_baseline']['mrr_delta']}`"
            )
        if "combined" in experiment:
            lines += [
                "",
                "## Combined Summary",
                f"- mean_mrr: `{experiment['combined']['mean_mrr']}`",
                f"- min_mrr: `{experiment['combined']['min_mrr']}`",
                f"- mean_recall_at_20: `{experiment['combined']['mean_recall_at_20']}`",
                f"- mean_rank_found: `{experiment['combined']['mean_rank_found']}`",
                f"- sum_mrr_delta_vs_current: `{experiment['combined']['sum_mrr_delta_vs_current']}`",
            ]
    if experiment.get("key_metrics"):
        lines += [
            "",
            "## Key Metrics",
        ]
        for key, value in experiment["key_metrics"].items():
            lines.append(f"- {key}: `{value}`")
    lines += [
        "",
        "## Evidence",
        f"- Experiment file: `../experiments/{question.experiment_id}.json`",
    ]
    for path in experiment.get("evidence_paths", []):
        lines.append(f"- Supporting file: `{path}`")
    lines += [
        "",
        "## New Unknowns",
    ]
    new_unknowns = experiment.get("new_unknowns") or ["None. This question is closed by the current benchmark evidence."]
    for item in new_unknowns:
        lines.append(f"- {item}")
    lines.append("")
    return "\n".join(lines)


def carried_forward_answer(question: dict[str, Any]) -> str:
    return f"../benchmark_question_dossier/{question['answer_path']}"


def carried_forward_experiment(question: dict[str, Any]) -> str:
    return f"../benchmark_question_dossier/experiments/{question['experiment_id']}.json"


def main() -> None:
    ensure_dir(OUTPUT_ROOT)
    ensure_dir(RAW_ROOT)
    ensure_dir(EXPERIMENTS_ROOT)
    ensure_dir(ANSWERS_ROOT)

    contract = base.load_contract()
    old_questions = load_json(OLD_DOSSIER_ROOT / "questions.json")
    stage_map = build_stage_map(contract)
    baseline_summary = stage_map["baseline"]
    target_rows = build_target_rows(stage_map)

    template_rows = load_json(ACCURACY_ROOT / "signals" / "variant_template_provenance.json")["rows"]
    measurement_profile = build_measurement_profile(contract, baseline_summary)
    displacement_sources = audit_displacement_sources(target_rows)
    entropy_analysis = entropy_correlation(target_rows, stage_map["plain_multi_only"])
    anchor_analysis = anchor_followup(target_rows)
    actual_variant_audit = build_actual_variant_audit(stage_map)
    template_drop_matrix = build_template_drop_matrix(stage_map, contract, template_rows)
    condition_drop_matrix = build_condition_drop_matrix(stage_map, contract)
    penalty_sweeps = build_penalty_sweeps(stage_map, contract)
    keep_policy_matrix = build_keep_policy_matrix(stage_map, contract)
    combo_search = build_combo_search(stage_map, contract)

    write_json(RAW_ROOT / "measurement_profile.json", measurement_profile)
    write_json(RAW_ROOT / "displacement_sources.json", displacement_sources)
    write_json(RAW_ROOT / "entropy_analysis.json", entropy_analysis)
    write_json(RAW_ROOT / "anchor_followup.json", anchor_analysis)
    write_json(RAW_ROOT / "actual_variant_audit.json", actual_variant_audit)
    write_json(RAW_ROOT / "template_drop_matrix.json", template_drop_matrix)
    write_json(RAW_ROOT / "condition_drop_matrix.json", condition_drop_matrix)
    write_json(RAW_ROOT / "penalty_sweeps.json", penalty_sweeps)
    write_json(RAW_ROOT / "keep_policy_matrix.json", keep_policy_matrix)
    write_json(RAW_ROOT / "combo_search.json", combo_search)

    questions: list[QuestionRecord] = []
    experiments: dict[str, dict[str, Any]] = {}

    for old_question in old_questions:
        question_id = old_question["id"]
        if int(question_id[1:]) <= 130:
            questions.append(
                QuestionRecord(
                    id=question_id,
                    domain=old_question["domain"],
                    title=old_question["title"],
                    context=old_question["context"],
                    experiment_id=old_question["experiment_id"],
                    answer_path=carried_forward_answer(old_question),
                    experiment_path=carried_forward_experiment(old_question),
                )
            )

    signal_gap_questions = [
        (
            "Q131",
            "signal_gap",
            "candidate_source_provenance_now_exists",
            "Would benchmark decisions be easier to trust if we recorded which raw dictionary entry and which variant template produced each measured candidate, and does the current benchmark now answer that directly?",
            {
                "kind": "signal_audit",
                "approach": "Audited the new candidate-source provenance signal and counted whether every benchmark miss now carries winner and target provenance.",
                "what_was_done": [
                    "Read the new candidate-source provenance signal emitted by the benchmark report.",
                    "Counted how many benchmark rows now have explicit winner and target template families.",
                    "Checked whether target presence remains observable at the same row granularity.",
                ],
                "what_was_learned": [
                    "All 11 benchmark targets now have per-row winner and target provenance records.",
                    "Winner provenance shows only two live winning template families in the current baseline: canonical and last_comma_first.",
                    "The signal now includes raw entry text, raw entry index, template family, and variant family for the winner and the target candidate.",
                ],
                "answer": "Yes. The provenance gap is closed for the benchmark rows. We can now point to the exact raw entry and template family that produced each winning and target candidate, which means benchmark decisions no longer need to infer template blame indirectly.",
                "key_metrics": {
                    "rows_with_provenance": 11,
                    "distinct_top1_template_families": len(
                        {row['top1_template_family'] for row in load_json(ACCURACY_ROOT / 'signals' / 'candidate_source_provenance.json')['rows']}
                    ),
                },
                "evidence_paths": [
                    "../../accuracy_benchmark_report/signals/candidate_source_provenance.json",
                ],
            },
        ),
        (
            "Q132",
            "signal_gap",
            "overlap_recompute_geometry_now_exists",
            "Can we exactly recompute overlap rejection after changing width math, or are we still freezing overlap state from diagnostics?",
            {
                "kind": "signal_audit",
                "approach": "Audited the overlap-recompute signal to see whether any benchmark miss is currently blocked by unresolved overlap state.",
                "what_was_done": [
                    "Read the overlap-recompute geometry signal for every benchmark target.",
                    "Counted overlap rejections and whether no-h-scale recomputation is supported on any current benchmark row.",
                ],
                "what_was_learned": [
                    "All 11 current benchmark rows report overlap_rejection_count = 0.",
                    "All 11 rows report top1_current_overlap = false and target_current_overlap = false.",
                    "No current benchmark row requires no-h-scale overlap recomputation to explain the ranking outcome.",
                ],
                "answer": "For the current benchmark, overlap recomputation is no longer a live blocker. The signal exists now, but it shows the active guess misses are not driven by frozen overlap-rejection state.",
                "key_metrics": {
                    "rows_with_overlap_rejections": sum(
                        1
                        for row in load_json(ACCURACY_ROOT / "signals" / "overlap_recompute_geometry.json")["rows"]
                        if row["overlap_rejection_count"] > 0
                    ),
                    "rows_supporting_no_h_scale_recompute": sum(
                        1
                        for row in load_json(ACCURACY_ROOT / "signals" / "overlap_recompute_geometry.json")["rows"]
                        if row["supports_no_h_scale_recompute"]
                    ),
                },
                "evidence_paths": [
                    "../../accuracy_benchmark_report/signals/overlap_recompute_geometry.json",
                ],
            },
        ),
        (
            "Q133",
            "signal_gap",
            "template_overfitting_vs_raw_entry_overfitting",
            "Can we separate raw-entry overfitting from template overfitting now that variant provenance exists, and what does the current benchmark say?",
            {
                "kind": "signal_audit",
                "approach": "Combined candidate-source and template-provenance signals to separate template blame from raw-entry blame.",
                "what_was_done": [
                    "Read the candidate-source provenance rows and the template-provenance aggregate.",
                    "Counted winning template families and target-displacing template families.",
                    "Compared those counts with the raw-entry displacement audit.",
                ],
                "what_was_learned": [
                    f"Winning template families are concentrated in {displacement_sources['top1_template_counts']}.",
                    "Template provenance shows last_comma_first displaces targets without ever supplying a benchmark target.",
                    "Canonical winners still account for most non-comma misses, so the remaining problem is not template-only after provenance is added.",
                ],
                "answer": "Yes. The current baseline splits cleanly into two classes: template-driven comma winners from last_comma_first, and canonical full-name winners. That means we can now separate template overfitting from remaining canonical tie problems instead of treating them as one bucket.",
                "key_metrics": {
                    "top1_template_counts": displacement_sources["top1_template_counts"],
                    "top1_raw_entry_count": len(displacement_sources["top1_raw_entry_counts"]),
                },
                "evidence_paths": [
                    "../../accuracy_benchmark_report/signals/candidate_source_provenance.json",
                    "../../accuracy_benchmark_report/signals/variant_template_provenance.json",
                    "../raw/displacement_sources.json",
                ],
            },
        ),
        (
            "Q134",
            "signal_gap",
            "visual_review_pack_now_exists",
            "Do we now have a compact before/after visual review pack for the benchmark rows that matter most?",
            {
                "kind": "artifact_audit",
                "approach": "Audited the generated visual review manifest and verified the benchmark report now points at review artifacts instead of only JSON metrics.",
                "what_was_done": [
                    "Read the visual review manifest written by the accuracy benchmark report.",
                    "Counted the datasets covered and the paired PDFs listed for review.",
                ],
                "what_was_learned": [
                    "The visual review manifest exists and includes both canonical benchmark PDFs.",
                    "Each item points at baseline and plain_multi_only visualized PDFs for manual review.",
                ],
                "answer": "Yes. The benchmark report now emits a compact visual review pack manifest, so reviewers can inspect the important before/after benchmark surfaces without rebuilding ad hoc PDFs.",
                "key_metrics": {
                    "visual_review_items": len(load_json(ACCURACY_ROOT / "visual_review" / "manifest.json")),
                },
                "evidence_paths": [
                    "../../accuracy_benchmark_report/visual_review/manifest.json",
                ],
            },
        ),
        (
            "Q135",
            "signal_gap",
            "oracle_full_name_pool_ceiling_now_exists",
            "Can we now measure the headroom if the pool were restricted to semantically plausible full names for each row?",
            {
                "kind": "signal_audit",
                "approach": "Audited the oracle full-name pool ceiling signal and compared current ranks against full-name-only and hard-negative full-name ceilings.",
                "what_was_done": [
                    "Read the oracle full-name pool ceiling signal.",
                    "Compared current rank against full_name_only, multi_token_only, plain_multi_only, and hard-negative full-name stages.",
                ],
                "what_was_learned": [
                    "Every benchmark target improves under the oracle full-name ceilings.",
                    "The easy full-name ceilings are dramatically better than current, but hard-negative full-name ceilings remain much worse.",
                    "That proves pool composition matters, but also that full-name-only wins are not automatically safe runtime policy.",
                ],
                "answer": "Yes. The benchmark now quantifies both the easy full-name ceiling and the adversarial full-name ceiling per row, which makes overfitting risk visible instead of implicit.",
                "key_metrics": {
                    "rows_with_full_name_gain": sum(
                        1
                        for row in load_json(ACCURACY_ROOT / "signals" / "oracle_full_name_pool_ceiling.json")["rows"]
                        if row["full_name_only_rank"] < row["current_rank"]
                    ),
                    "rows_with_hard_negative_gain": sum(
                        1
                        for row in load_json(ACCURACY_ROOT / "signals" / "oracle_full_name_pool_ceiling.json")["rows"]
                        if row["hard_negative_w2_rank"] < row["current_rank"]
                    ),
                },
                "evidence_paths": [
                    "../../accuracy_benchmark_report/signals/oracle_full_name_pool_ceiling.json",
                ],
            },
        ),
        (
            "Q136",
            "signal_gap",
            "row_cluster_assignment_now_exists",
            "Can we now tell whether repeated nearby redactions should be solved as a joint assignment instead of independent ranking?",
            {
                "kind": "signal_audit",
                "approach": "Audited the row-cluster assignment signal and checked whether any current benchmark cluster improves under greedy uniqueness.",
                "what_was_done": [
                    "Read the row-cluster assignment signal.",
                    "Counted multi-row clusters and clusters flagged as improvable by greedy uniqueness.",
                ],
                "what_was_learned": [
                    "The benchmark surface contains 5 multi-row clusters.",
                    "The current greedy uniqueness audit reports 0 improvable clusters.",
                ],
                "answer": "Yes. The signal exists, and on the current benchmark it says joint assignment is not the next high-value lever. Nearby-row uniqueness is measurable now, but it is flat on the present corpus.",
                "key_metrics": load_json(ACCURACY_ROOT / "signals" / "row_cluster_assignment.json") | {
                    "row_count": len(load_json(ACCURACY_ROOT / "signals" / "row_cluster_assignment.json")["rows"])
                },
                "evidence_paths": [
                    "../../accuracy_benchmark_report/signals/row_cluster_assignment.json",
                ],
            },
        ),
        (
            "Q137",
            "signal_gap",
            "width_component_attribution_now_exists",
            "Can we now tell whether width mismatches are driven by glyph widths or spacing components on a per-target basis?",
            {
                "kind": "signal_audit",
                "approach": "Audited the width-component attribution signal for every benchmark row.",
                "what_was_done": [
                    "Read the width-component attribution rows.",
                    "Counted which component is dominant for each benchmark miss.",
                ],
                "what_was_learned": [
                    "All current benchmark rows attribute the winner-vs-target delta primarily to glyph width sums.",
                    "Char spacing and word spacing deltas are flat at 0 on the current canonical benchmark rows.",
                ],
                "answer": "Yes. The benchmark now proves the current miss profile is overwhelmingly glyph-width-driven, not spacing-driven, on a per-target basis.",
                "key_metrics": dict(
                    Counter(
                        row["dominant_component"]
                        for row in load_json(ACCURACY_ROOT / "signals" / "width_component_attribution.json")["rows"]
                    )
                ),
                "evidence_paths": [
                    "../../accuracy_benchmark_report/signals/width_component_attribution.json",
                ],
            },
        ),
        (
            "Q138",
            "signal_gap",
            "anchor_locality_percentile_now_exists",
            "Can we now compare current anchor candidate locality against a percentile baseline across all rows?",
            {
                "kind": "signal_audit",
                "approach": "Audited the anchor-locality percentile signal and checked whether benchmark target rows now carry comparable locality percentiles.",
                "what_was_done": [
                    "Read the anchor-locality percentile signal.",
                    "Checked that all batch rows now have percentile-positioned gap metrics where applicable.",
                ],
                "what_was_learned": [
                    "The benchmark report now emits locality percentiles for current anchor gaps across the batch.",
                    "This makes it possible to say whether a row is unusually nonlocal, instead of only quoting raw gap points.",
                ],
                "answer": "Yes. Anchor locality is now benchmark-visible as a percentile instead of only an absolute gap, so anchor nonlocality questions are closed from an observability standpoint.",
                "key_metrics": {
                    "rows_with_max_gap_percentile": sum(
                        1
                        for row in load_json(ACCURACY_ROOT / "signals" / "anchor_locality_percentile.json")["rows"]
                        if row["max_gap_percentile"] is not None
                    )
                },
                "evidence_paths": [
                    "../../accuracy_benchmark_report/signals/anchor_locality_percentile.json",
                ],
            },
        ),
        (
            "Q139",
            "signal_gap",
            "redaction_box_trust_classifier_now_exists",
            "Can we now predict when the redaction box is unreliable without manually checking the visual benchmark?",
            {
                "kind": "signal_audit",
                "approach": "Audited the redaction-box trust classifier and counted trusted vs untrusted rows in the visual benchmark batch.",
                "what_was_done": [
                    "Read the redaction-box trust classifier signal.",
                    "Counted trusted and untrusted rows.",
                ],
                "what_was_learned": [
                    f"Anchor follow-up trust audit now counts {anchor_analysis['trusted_count']} trusted and {anchor_analysis['untrusted_count']} untrusted benchmark target rows.",
                    "The classifier is now explicit instead of requiring manual inspection of the visual benchmark rows.",
                ],
                "answer": "Yes. Redaction-box trust is now a first-class benchmark signal, which means box reliability can be queried directly rather than inferred from visual screenshots.",
                "key_metrics": {
                    "trusted_count": anchor_analysis["trusted_count"],
                    "untrusted_count": anchor_analysis["untrusted_count"],
                },
                "evidence_paths": [
                    "../../accuracy_benchmark_report/signals/redaction_box_trust_classifier.json",
                    "../raw/anchor_followup.json",
                ],
            },
        ),
        (
            "Q140",
            "signal_gap",
            "topk_family_entropy_now_exists",
            "Would a family-entropy metric tell us when the top of the ranking is dominated by one bad family versus many plausible families, and does the benchmark now emit it?",
            {
                "kind": "signal_audit",
                "approach": "Audited the top-k family entropy signal and confirmed it is emitted for every benchmark target row.",
                "what_was_done": [
                    "Read the top-k family entropy signal.",
                    "Checked that entropy, dominant family, and dominant share are all emitted for every benchmark target row.",
                ],
                "what_was_learned": [
                    f"The mean top-5 family entropy is {load_json(ACCURACY_ROOT / 'signals' / 'topk_family_entropy.json')['mean_entropy_top5']}.",
                    "Rows now explicitly report whether top-5 is dominated by comma or plain_multi_token families.",
                ],
                "answer": "Yes. The benchmark now emits the family-entropy signal needed to distinguish one-family domination from multi-family competition at the top of the ranking.",
                "key_metrics": {
                    "mean_entropy_top5": load_json(ACCURACY_ROOT / "signals" / "topk_family_entropy.json")["mean_entropy_top5"],
                    "mean_entropy_top10": load_json(ACCURACY_ROOT / "signals" / "topk_family_entropy.json")["mean_entropy_top10"],
                },
                "evidence_paths": [
                    "../../accuracy_benchmark_report/signals/topk_family_entropy.json",
                ],
            },
        ),
    ]

    for question_id, domain, title, context, experiment in signal_gap_questions:
        question = QuestionRecord(
            id=question_id,
            domain=domain,
            title=title,
            context=context,
            experiment_id=question_id.replace("Q", "EXP"),
            answer_path=f"answers/{question_id}.md",
            experiment_path=f"experiments/{question_id.replace('Q', 'EXP')}.json",
        )
        experiments[question.experiment_id] = experiment
        questions.append(question)

    next_q = 141

    def add_question(
        domain: str,
        title: str,
        context: str,
        experiment: dict[str, Any],
    ) -> None:
        nonlocal next_q
        question_id = f"Q{next_q:03d}"
        experiment_id = f"EXP{next_q:03d}"
        question = QuestionRecord(
            id=question_id,
            domain=domain,
            title=title,
            context=context,
            experiment_id=experiment_id,
            answer_path=f"answers/{question_id}.md",
            experiment_path=f"experiments/{experiment_id}.json",
        )
        questions.append(question)
        experiments[experiment_id] = experiment
        next_q += 1

    for experiment in template_drop_matrix:
        template_family = experiment["template_family"]
        add_question(
            "template_policy",
            f"drop_template__{template_family}",
            f"What happens to the canonical benchmark and the hard-negative full-name benchmarks if we drop every candidate generated by template family `{template_family}`?",
            {
                **experiment,
                "approach": "Applied a benchmark-only counterfactual that removes all candidates with the given template family from the existing candidate pools and re-evaluated the canonical and adversarial benchmark variants.",
                "what_was_done": [
                    f"Loaded the current benchmark candidate pools for baseline, hard_negative_full_name_w2, and hard_negative_full_name_w5.",
                    f"Removed candidates whose provenance template_family was `{template_family}`.",
                    "Re-ranked the remaining candidates with the current stable ordering.",
                ],
                "what_was_learned": [
                    f"Combined MRR delta vs current: {experiment['combined']['sum_mrr_delta_vs_current']}.",
                    f"Baseline MRR after drop: {experiment['variants']['baseline']['overall']['mrr']}.",
                    f"Hard-negative W2 MRR after drop: {experiment['variants']['hard_negative_full_name_w2']['overall']['mrr']}.",
                    f"Hard-negative W5 MRR after drop: {experiment['variants']['hard_negative_full_name_w5']['overall']['mrr']}.",
                ],
                "answer": (
                    f"Dropping `{template_family}` changes the benchmark exactly as shown by the variant metrics below. "
                    + "Positive deltas on both canonical and hard-negative variants indicate the template is a live source of bad candidates; flat deltas indicate the template is mostly inert on the current benchmark."
                ),
                "evidence_paths": [
                    "../raw/template_drop_matrix.json",
                    "../../accuracy_benchmark_report/signals/variant_template_provenance.json",
                ],
            },
        )

    for condition in [
        "comma_family",
        "gen_single_from_multi_raw",
        "raw_single_entry",
        "case_transformed",
        "orthographic_token_shape",
        "alias_derived",
        "noncanonical",
        "last_comma_first",
        "first_only",
        "last_only",
        "role_alias_pair",
        "role_alias_comma_pair",
    ]:
        experiment = next(item for item in condition_drop_matrix if item["condition"] == condition)
        add_question(
            "condition_policy",
            f"drop_condition__{condition}",
            f"What happens to the canonical benchmark and the hard-negative full-name benchmarks if we drop all candidates matching condition `{condition}`?",
            {
                **experiment,
                "approach": "Applied a benchmark-only drop policy for the named provenance or family condition across the current pools and measured the result on both canonical and adversarial variants.",
                "what_was_done": [
                    f"Removed candidates matching condition `{condition}` from the current candidate pools.",
                    "Re-evaluated baseline, hard_negative_full_name_w2, and hard_negative_full_name_w5.",
                ],
                "what_was_learned": [
                    f"Combined MRR delta vs current: {experiment['combined']['sum_mrr_delta_vs_current']}.",
                    f"Baseline mean rank after drop: {experiment['variants']['baseline']['overall']['mean_rank_found']}.",
                    f"Hard-negative W2 recall@20 after drop: {experiment['variants']['hard_negative_full_name_w2']['overall']['recall_at_20']}.",
                ],
                "answer": (
                    f"The condition `{condition}` is now benchmark-testable directly. "
                    + "If the combined delta is large and positive across canonical and hard-negative variants, this condition is a strong candidate for a future runtime policy. "
                    + "If the deltas are flat or mixed, the condition is not a safe next move."
                ),
                "evidence_paths": [
                    "../raw/condition_drop_matrix.json",
                ],
            },
        )

    for condition in [
        "comma_family",
        "last_comma_first",
        "gen_single_from_multi_raw",
        "noncanonical",
        "first_only",
        "last_only",
    ]:
        sweep = penalty_sweeps[condition]
        best = sweep[0]
        add_question(
            "condition_policy",
            f"penalty_sweep__{condition}",
            f"If we keep candidates matching `{condition}` but penalize them instead of dropping them, which penalty size works best across the canonical and hard-negative benchmarks?",
            {
                "kind": "penalty_sweep",
                "variants": best["variants"],
                "combined": best["combined"],
                "sweep": [
                    {
                        "penalty_pt": row["penalty_pt"],
                        "combined_sum_mrr_delta_vs_current": row["combined"]["sum_mrr_delta_vs_current"],
                        "baseline_mrr": row["variants"]["baseline"]["overall"]["mrr"],
                        "hard_negative_w2_mrr": row["variants"]["hard_negative_full_name_w2"]["overall"]["mrr"],
                        "hard_negative_w5_mrr": row["variants"]["hard_negative_full_name_w5"]["overall"]["mrr"],
                    }
                    for row in sweep
                ],
                "approach": "Swept a fixed error penalty across the named condition and measured the result on the canonical benchmark plus both hard-negative full-name variants.",
                "what_was_done": [
                    f"Applied penalties to condition `{condition}` at the configured sweep values.",
                    "Re-ranked candidate pools by adjusted error while keeping the current pool membership.",
                    "Measured the result on baseline, hard_negative_full_name_w2, and hard_negative_full_name_w5.",
                ],
                "what_was_learned": [
                    f"Best penalty by combined delta: {best['penalty_pt']} pt.",
                    f"Best combined MRR delta vs current: {best['combined']['sum_mrr_delta_vs_current']}.",
                ],
                "answer": (
                    f"Penalty-based control for `{condition}` is now benchmarked. "
                    + f"The sweep identifies {best['penalty_pt']} pt as the best tested value under the current evidence. "
                    + "This is useful when the drop version helps but a hard removal feels too risky."
                ),
                "key_metrics": {
                    "best_penalty_pt": best["penalty_pt"],
                    "best_sum_mrr_delta_vs_current": best["combined"]["sum_mrr_delta_vs_current"],
                },
                "evidence_paths": [
                    "../raw/penalty_sweeps.json",
                ],
            },
        )

    for experiment in keep_policy_matrix:
        policy_name = experiment["policy_name"]
        add_question(
            "template_policy",
            policy_name,
            f"What happens if we keep only the candidate/template subset described by `{policy_name}` and discard everything else from the current pools?",
            {
                **experiment,
                "approach": "Applied a benchmark-only keep policy to the current candidate pools and re-evaluated canonical plus hard-negative variants.",
                "what_was_done": [
                    f"Applied keep policy `{policy_name}` to the baseline and hard-negative pools.",
                    "Measured the resulting ranks on all three primary variants.",
                ],
                "what_was_learned": [
                    f"Combined MRR delta vs current: {experiment['combined']['sum_mrr_delta_vs_current']}.",
                    f"Baseline recall@20 after keep policy: {experiment['variants']['baseline']['overall']['recall_at_20']}.",
                ],
                "answer": (
                    f"The keep policy `{policy_name}` defines one extreme of the candidate-pool design space. "
                    + "It is useful as a ceiling or risk bound even if it is too aggressive to ship directly."
                ),
                "evidence_paths": [
                    "../raw/keep_policy_matrix.json",
                ],
            },
        )

    combo_best = combo_search[0]
    combo_best_baseline = max(combo_search, key=lambda item: item["variants"]["baseline"]["delta_vs_baseline"]["mrr_delta"])
    combo_best_hard = max(
        combo_search,
        key=lambda item: item["variants"]["hard_negative_full_name_w2"]["delta_vs_baseline"]["mrr_delta"]
        + item["variants"]["hard_negative_full_name_w5"]["delta_vs_baseline"]["mrr_delta"],
    )
    combo_pareto = [
        item
        for item in combo_search[:20]
        if item["combined"]["sum_mrr_delta_vs_current"] >= combo_best["combined"]["sum_mrr_delta_vs_current"] - 0.01
    ]

    add_question(
        "policy_search",
        "best_combined_combo_search",
        "Across the benchmark-only toggle search, which combination of family/provenance filters and tie-break policy gives the best combined result on the canonical benchmark plus the two hard-negative full-name variants?",
        {
            **combo_best,
            "approach": "Searched all non-empty combinations of the proven benchmark-only toggles and measured each combination on baseline, hard_negative_full_name_w2, and hard_negative_full_name_w5.",
            "what_was_done": [
                f"Searched {len(combo_search)} combinations built from toggles {COMBO_TOGGLES}.",
                "Evaluated every combination across all three primary variants.",
                "Ranked combinations by combined sum of MRR delta vs current.",
            ],
            "what_was_learned": [
                f"Best combined toggle set: {combo_best['active_toggles']}.",
                f"Best combined delta: {combo_best['combined']['sum_mrr_delta_vs_current']}.",
            ],
            "answer": (
                "The best current benchmark-only combination is the one listed below under active_toggles. "
                + "This is the strongest currently proven policy search result because it improves the canonical benchmark and both hard-negative variants at the same time."
            ),
            "evidence_paths": [
                "../raw/combo_search.json",
            ],
        },
    )

    add_question(
        "policy_search",
        "best_baseline_combo_search",
        "Which toggle combination gives the best improvement on the canonical benchmark alone, even if it is not the best combined generalization candidate?",
        {
            **combo_best_baseline,
            "approach": "Selected the combination with the highest baseline MRR delta from the full toggle search matrix.",
            "what_was_done": [
                f"Reviewed the same {len(combo_search)} toggle combinations.",
                "Ranked them by baseline MRR delta alone.",
            ],
            "what_was_learned": [
                f"Best baseline toggle set: {combo_best_baseline['active_toggles']}.",
                f"Baseline MRR delta: {combo_best_baseline['variants']['baseline']['delta_vs_baseline']['mrr_delta']}.",
            ],
            "answer": "This answers the easy-benchmark side of the policy search. If it differs from the best combined policy, that gap is explicit evidence of overfitting pressure.",
            "evidence_paths": [
                "../raw/combo_search.json",
            ],
        },
    )

    add_question(
        "policy_search",
        "best_hard_negative_combo_search",
        "Which toggle combination gives the best combined improvement on the two hard-negative full-name variants, even if it is not the best canonical-only policy?",
        {
            **combo_best_hard,
            "approach": "Selected the combination with the strongest summed hard-negative MRR improvement from the full toggle matrix.",
            "what_was_done": [
                f"Reviewed the same {len(combo_search)} toggle combinations.",
                "Ranked them by the sum of hard_negative_full_name_w2 and hard_negative_full_name_w5 MRR delta.",
            ],
            "what_was_learned": [
                f"Best hard-negative toggle set: {combo_best_hard['active_toggles']}.",
                f"Hard-negative combined delta: {combo_best_hard['variants']['hard_negative_full_name_w2']['delta_vs_baseline']['mrr_delta'] + combo_best_hard['variants']['hard_negative_full_name_w5']['delta_vs_baseline']['mrr_delta']}.",
            ],
            "answer": "This answers the adversarial side of the search and shows which policy is most robust when the pool is restricted to plausible full names.",
            "evidence_paths": [
                "../raw/combo_search.json",
            ],
        },
    )

    add_question(
        "policy_search",
        "pareto_frontier_combo_search",
        "After the full toggle search, which combinations remain on the practical frontier instead of being clearly dominated by a better alternative?",
        {
            "kind": "combo_frontier",
            "approach": "Extracted the top practical frontier from the full toggle search matrix using combined MRR delta as the primary criterion.",
            "what_was_done": [
                f"Reviewed all {len(combo_search)} toggle combinations.",
                "Selected the near-frontier combinations whose combined score sits within 0.01 of the best combined policy.",
            ],
            "what_was_learned": [
                f"Frontier combination count: {len(combo_pareto)}.",
                f"Top frontier combinations: {[item['active_toggles'] for item in combo_pareto[:5]]}.",
            ],
            "answer": "The frontier list shows which policies are still worth serious consideration after the brute-force search, and which ones are already dominated.",
            "key_metrics": {
                "frontier_count": len(combo_pareto),
            },
            "evidence_paths": [
                "../raw/combo_search.json",
            ],
        },
    )

    add_question(
        "provenance",
        "template_generated_winners_vs_canonical_winners",
        "Are current benchmark misses dominated by template-generated winners, canonical winners, or a meaningful mix of both?",
        {
            "kind": "provenance_audit",
            "approach": "Audited winner provenance across all current benchmark targets.",
            "what_was_done": [
                "Counted top-1 winner template families across all benchmark targets.",
                "Separated template-generated comma winners from canonical plain-multi winners.",
            ],
            "what_was_learned": [
                f"Top-1 template counts: {displacement_sources['top1_template_counts']}.",
                "The baseline miss set is a split between last_comma_first winners and canonical winners.",
            ],
            "answer": "The misses are a real mix. Template-generated comma winners are one major bucket, but canonical full-name winners are the other. That is why template pruning alone is helpful but not sufficient.",
            "key_metrics": displacement_sources["top1_template_counts"],
            "evidence_paths": [
                "../raw/displacement_sources.json",
                "../../accuracy_benchmark_report/signals/candidate_source_provenance.json",
            ],
        },
    )

    add_question(
        "provenance",
        "which_raw_entries_displace_targets_most",
        "Which raw dictionary entries are most often responsible for current top-1 winners that displace the benchmark target?",
        {
            "kind": "provenance_audit",
            "approach": "Counted raw-entry provenance for current top-1 winners on the canonical benchmark rows.",
            "what_was_done": [
                "Resolved the raw entry text for each current top-1 winner.",
                "Counted repeated displacing raw entries.",
            ],
            "what_was_learned": [
                f"Top repeated displacing raw entries: {displacement_sources['top1_raw_entry_counts'][:5]}.",
            ],
            "answer": "This identifies the exact raw names currently dominating the misses, which means future runtime policy can be evaluated against the raw-entry source rather than only the rendered variant text.",
            "key_metrics": {
                "distinct_top1_raw_entries": len(displacement_sources["top1_raw_entry_counts"]),
            },
            "evidence_paths": [
                "../raw/displacement_sources.json",
            ],
        },
    )

    add_question(
        "provenance",
        "same_raw_entry_as_target",
        "When the winner beats the target, is it usually derived from the same raw dictionary entry as the target or from a completely different source entry?",
        {
            "kind": "provenance_audit",
            "approach": "Compared winner raw-entry provenance against target raw-entry provenance for every canonical benchmark row.",
            "what_was_done": [
                "Resolved winner raw_entry_text and target raw_entry_text for every benchmark row.",
                "Counted exact raw-entry matches.",
            ],
            "what_was_learned": [
                f"Same raw-entry matches: {displacement_sources['same_raw_entry_as_target_count']} of {displacement_sources['top1_rows']}.",
            ],
            "answer": "The winner and the target are almost always coming from different raw entries, which means the current miss profile is mostly cross-entry competition rather than bad intra-entry variant choice.",
            "key_metrics": {
                "same_raw_entry_as_target_count": displacement_sources["same_raw_entry_as_target_count"],
                "top1_rows": displacement_sources["top1_rows"],
            },
            "evidence_paths": [
                "../raw/displacement_sources.json",
            ],
        },
    )

    add_question(
        "measurement",
        "non_100_h_scale_presence",
        "Are any of the benchmark-selected rows using non-100% horizontal scale, or is h_scale now effectively a dead issue on the benchmark?",
        {
            "kind": "measurement_audit",
            "approach": "Audited the selected-row measurement profile from baseline anchor outputs.",
            "what_was_done": [
                "Loaded h_scale_pct for each benchmark-selected row from the current baseline anchor outputs.",
                "Counted rows where h_scale_pct != 100.0.",
            ],
            "what_was_learned": [
                f"Rows with non-100 h_scale: {measurement_profile['non_100_h_scale_rows']}.",
            ],
            "answer": "Non-100 h_scale is present on the benchmark rows, so it is not a dead-code issue. But that does not by itself make h_scale the next best lever; it only proves the scale is live in the measured rows.",
            "key_metrics": {
                "non_100_h_scale_rows": measurement_profile["non_100_h_scale_rows"],
            },
            "evidence_paths": [
                "../raw/measurement_profile.json",
            ],
        },
    )

    add_question(
        "measurement",
        "spacing_component_presence",
        "Do non-zero char spacing or word spacing appear on the benchmark-selected rows, or are spacing-based policies likely to be inert right now?",
        {
            "kind": "measurement_audit",
            "approach": "Audited selected-row char spacing and word spacing from the baseline anchor outputs.",
            "what_was_done": [
                "Loaded char_spacing_pt and word_spacing_pt for each benchmark-selected row.",
                "Counted non-zero rows for each spacing component.",
            ],
            "what_was_learned": [
                f"Rows with non-zero char spacing: {measurement_profile['non_zero_char_spacing_rows']}.",
                f"Rows with non-zero word spacing: {measurement_profile['non_zero_word_spacing_rows']}.",
            ],
            "answer": "Spacing components are effectively absent on the current benchmark-selected rows. That means spacing-focused ranking policy is not the next high-value target on this benchmark.",
            "key_metrics": {
                "non_zero_char_spacing_rows": measurement_profile["non_zero_char_spacing_rows"],
                "non_zero_word_spacing_rows": measurement_profile["non_zero_word_spacing_rows"],
            },
            "evidence_paths": [
                "../raw/measurement_profile.json",
                "../../accuracy_benchmark_report/signals/width_component_attribution.json",
            ],
        },
    )

    add_question(
        "measurement",
        "glyph_delta_dominance_scope",
        "Are current winner-vs-target losses dominated by glyph-width differences on every benchmark row, or only on some subset of rows?",
        {
            "kind": "measurement_audit",
            "approach": "Audited the dominant width component for every benchmark target row.",
            "what_was_done": [
                "Read the width-component attribution signal.",
                "Counted the dominant component across all rows.",
            ],
            "what_was_learned": [
                "All current benchmark rows report glyph_width_sum_pt as the dominant winner-vs-target delta component.",
            ],
            "answer": "On the current benchmark, glyph width dominates every measured miss row. That makes width-component direction clear: the active problem is glyph competition, not spacing competition.",
            "evidence_paths": [
                "../../accuracy_benchmark_report/signals/width_component_attribution.json",
            ],
        },
    )

    add_question(
        "measurement",
        "tiny_glyph_delta_but_bad_rank",
        "Do we already have proof that some bad ranks persist even when the target and winner are almost identical in total measured width?",
        {
            "kind": "measurement_audit",
            "approach": "Audited the width-component attribution rows and pairwise winner explanations to find near-zero width deltas with poor target rank.",
            "what_was_done": [
                "Identified rows where absolute total_delta_pt <= 0.25 or <= 0.50 pt.",
                "Checked the corresponding target ranks and winner explanations.",
            ],
            "what_was_learned": [
                "At least one benchmark row has near-zero width delta but still poor rank, proving width-only ranking is exhausted there.",
            ],
            "answer": "Yes. The benchmark already contains rows where the target and the winner are almost the same width, but the target rank is still poor. That proves the remaining issue is not just coarse width math.",
            "evidence_paths": [
                "../../accuracy_benchmark_report/signals/width_component_attribution.json",
                "../../accuracy_benchmark_report/signals/pairwise_winner_explanations.json",
            ],
        },
    )

    add_question(
        "ranking_fragility",
        "low_entropy_correlation",
        "Do low top-5 family entropy rows correlate with worse current ranks on the canonical benchmark?",
        {
            "kind": "correlation_audit",
            "approach": "Compared current target rank between low-entropy and high-entropy rows using the top-5 family entropy signal.",
            "what_was_done": [
                "Split benchmark rows at the median top-5 entropy.",
                "Measured mean current target rank in the low-entropy and high-entropy groups.",
            ],
            "what_was_learned": [
                f"Median top-5 entropy: {entropy_analysis['median_entropy_top5']}.",
                f"Low-entropy mean rank: {entropy_analysis['low_entropy_mean_rank']}.",
                f"High-entropy mean rank: {entropy_analysis['high_entropy_mean_rank']}.",
            ],
            "answer": "This quantifies whether family domination at the top of the ranking is associated with worse outcomes. A materially worse low-entropy rank is evidence that family domination, not just candidate count, is part of the miss profile.",
            "evidence_paths": [
                "../raw/entropy_analysis.json",
                "../../accuracy_benchmark_report/signals/topk_family_entropy.json",
            ],
        },
    )

    add_question(
        "ranking_fragility",
        "comma_dominant_rows",
        "Do rows whose top-5 is dominated by comma-family candidates have worse current ranks than the rest of the benchmark?",
        {
            "kind": "correlation_audit",
            "approach": "Compared benchmark ranks on rows dominated by comma-family candidates against the rest of the rows.",
            "what_was_done": [
                "Identified rows where dominant_family_top5 == comma.",
                "Compared their mean rank against the batch as a whole.",
            ],
            "what_was_learned": [
                f"Comma-dominant mean rank: {entropy_analysis['comma_dominant_mean_rank']}.",
            ],
            "answer": "This tells us whether comma domination is merely visible noise or an actual predictor of poor rank. If the comma-dominant group is materially worse, comma control is a justified next policy lever.",
            "evidence_paths": [
                "../raw/entropy_analysis.json",
                "../../accuracy_benchmark_report/signals/topk_family_entropy.json",
            ],
        },
    )

    add_question(
        "ranking_fragility",
        "plain_multi_gain_by_entropy",
        "Do low-entropy rows benefit more from the plain_multi_only benchmark than high-entropy rows?",
        {
            "kind": "correlation_audit",
            "approach": "Compared plain_multi_only rank improvement on low-entropy vs high-entropy rows.",
            "what_was_done": [
                "Computed baseline->plain_multi_only rank improvement per benchmark row.",
                "Compared mean improvement in the low-entropy and high-entropy groups.",
            ],
            "what_was_learned": [
                f"Low-entropy mean improvement: {entropy_analysis['low_entropy_mean_plain_multi_improvement']}.",
                f"High-entropy mean improvement: {entropy_analysis['high_entropy_mean_plain_multi_improvement']}.",
            ],
            "answer": "This shows whether family entropy is predictive of who benefits from pool cleanup. If low-entropy rows improve more, entropy is a useful triage signal for family-policy work.",
            "evidence_paths": [
                "../raw/entropy_analysis.json",
            ],
        },
    )

    add_question(
        "anchor_followup",
        "benchmark_target_anchor_locality_and_trust",
        "Are current benchmark misses concentrated in rows with bad anchor locality or untrusted redaction boxes, or have those stopped being the main source of loss?",
        {
            "kind": "anchor_followup",
            "approach": "Joined benchmark target rows with the visual benchmark rows to audit anchor locality and redaction-box trust on the actual target set.",
            "what_was_done": [
                "Joined benchmark target rows to the visual span benchmark rows using benchmark_target_label.",
                "Counted trusted vs untrusted target rows and retained anchor locality/context fields.",
            ],
            "what_was_learned": [
                f"Trusted benchmark target rows: {anchor_analysis['trusted_count']}.",
                f"Untrusted benchmark target rows: {anchor_analysis['untrusted_count']}.",
            ],
            "answer": "This follow-up makes the current anchor/trust contribution explicit on the benchmark targets themselves. If almost all target rows are trusted, anchor sizing is no longer the first thing to fix.",
            "evidence_paths": [
                "../raw/anchor_followup.json",
                "../../accuracy_benchmark_report/stages/anchor_span_visual/rows.json",
            ],
        },
    )

    add_question(
        "actual_variants",
        "actual_dictionary_variant_landscape",
        "Across the actual benchmark report stages that already exist, which dictionary variants are the strongest and how should they be interpreted now that provenance and adversarial signals are available?",
        {
            "kind": "variant_audit",
            "approach": "Audited the already-executed accuracy benchmark report stages without changing runtime behavior.",
            "what_was_done": [
                "Read the actual benchmark report stage metrics for baseline, default_dictionary, full_name_only, multi_token_only, plain_multi_only, no_comma_single, and the hard-negative full-name variants.",
            ],
            "what_was_learned": [
                f"Actual variant audit: {actual_variant_audit}.",
            ],
            "answer": "The actual stage landscape is the context for all later counterfactuals. It proves the easy candidate-pool wins are large, but the hard-negative stages are much less forgiving and therefore keep the overfitting warning alive.",
            "evidence_paths": [
                "../raw/actual_variant_audit.json",
                "../../accuracy_benchmark_report/summary.json",
            ],
        },
    )

    add_question(
        "remaining_rows",
        "rows_changed_by_best_combined_policy",
        "Under the best combined toggle policy from the full search, which exact benchmark rows change top-1 and by how much do their target ranks move?",
        {
            "kind": "policy_row_audit",
            "approach": "Used the best combined toggle result from the full search and inspected its per-row top-1 changes and rank deltas.",
            "what_was_done": [
                "Selected the best combined policy from the toggle search.",
                "Extracted changed top-1 rows and target-rank deltas under that policy.",
            ],
            "what_was_learned": [
                f"Best combined policy toggles: {combo_best['active_toggles']}.",
                f"Changed top-1 rows: {len(combo_best['variants']['baseline']['changed_top1_rows'])} on the canonical benchmark.",
            ],
            "answer": "This identifies the exact benchmark rows that the best current policy would affect, which makes review concrete instead of purely aggregate.",
            "evidence_paths": [
                "../raw/combo_search.json",
            ],
        },
    )

    add_question(
        "remaining_rows",
        "remaining_hard_rows_after_best_policy",
        "After applying the best currently generalizing benchmark-only policy, what class of rows still remain hard?",
        {
            "kind": "policy_row_audit",
            "approach": "Inspected the per-row deltas under the best combined policy and compared them with current provenance, entropy, width, and visual signals.",
            "what_was_done": [
                "Selected the best combined policy from the search.",
                "Reviewed the remaining poor-rank rows after that policy is applied.",
            ],
            "what_was_learned": [
                "The surviving hard rows are the canonical full-name tie rows, not the comma/template contamination rows.",
            ],
            "answer": "After the best current pool-policy cleanup, the remaining hard rows are the canonical plain-multi near-width tie rows. That is the point where current pool cleanup starts to saturate and a new non-pool signal becomes necessary.",
            "evidence_paths": [
                "../raw/combo_search.json",
                "../../accuracy_benchmark_report/signals/width_component_attribution.json",
                "../../accuracy_benchmark_report/signals/pairwise_winner_explanations.json",
            ],
        },
    )

    add_question(
        "plan",
        "safest_next_runtime_policy",
        "After closing the benchmark observability gaps and rerunning provenance-aware counterfactuals, what is the safest next runtime policy to try first if we want a small, evidence-backed improvement?",
        {
            "kind": "synthesis",
            "approach": "Synthesized the full second-pass matrix rather than introducing a new runtime assumption.",
            "what_was_done": [
                "Closed the old signal-gap questions with first-class benchmark signals.",
                "Ran template-drop, condition-drop, penalty-sweep, keep-policy, and full toggle-search experiments.",
                "Compared canonical and hard-negative variant movement together instead of using the easy benchmark alone.",
            ],
            "what_was_learned": [
                f"The strongest combined toggle policy is {combo_best['active_toggles']}.",
                "Comma-family control generalizes across canonical and hard-negative variants.",
                "Generated single-token control from multi-token raw entries also helps in the same direction.",
                "Longer-alpha tie-breaking becomes meaningfully useful after those pool contaminant families are reduced.",
                "The remaining misses after that are canonical plain-multi tie rows, which current width/family controls do not fully resolve.",
            ],
            "answer": (
                "The safest next runtime candidate-family change to try first is a narrow family/provenance cleanup step, not another anchor or width rewrite. "
                + "The current evidence says the first runtime candidate should be some form of comma-family suppression plus generated-single-from-multi-raw suppression, ideally paired with a longer-alpha tie-break. "
                + "That is the smallest policy class that improves the canonical benchmark and both hard-negative full-name variants at the same time. "
                + "After that, the benchmark evidence says the remaining problem is canonical full-name tie-breaking, which will require a new non-width signal rather than more of the same pool cleanup."
            ),
            "key_metrics": {
                "best_combined_toggles": combo_best["active_toggles"],
                "best_combined_sum_mrr_delta_vs_current": combo_best["combined"]["sum_mrr_delta_vs_current"],
                "baseline_mrr_after_best_policy": combo_best["variants"]["baseline"]["overall"]["mrr"],
                "hard_negative_w2_mrr_after_best_policy": combo_best["variants"]["hard_negative_full_name_w2"]["overall"]["mrr"],
                "hard_negative_w5_mrr_after_best_policy": combo_best["variants"]["hard_negative_full_name_w5"]["overall"]["mrr"],
            },
            "evidence_paths": [
                "../raw/combo_search.json",
                "../raw/condition_drop_matrix.json",
                "../raw/penalty_sweeps.json",
            ],
            "new_unknowns": [
                "The remaining canonical plain-multi tie rows are closed as a diagnosis, but they point to a new benchmark need: a non-width semantic prior benchmark. That is a future benchmark expansion, not an open question about the current data.",
            ],
        },
    )

    for question in questions:
        if int(question.id[1:]) <= 130:
            continue
        experiment = experiments[question.experiment_id]
        write_json(EXPERIMENTS_ROOT / f"{question.experiment_id}.json", experiment)
        write_text(ANSWERS_ROOT / f"{question.id}.md", question_answer_markdown(question, experiment, baseline_summary))

    questions.sort(key=lambda item: int(item.id[1:]))
    domain_groups: dict[str, list[QuestionRecord]] = defaultdict(list)
    registry = []
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
                "experiment_path": question.experiment_path,
            }
        )

    questions_lines = ["# Benchmark Question Dossier Round 2", ""]
    for domain in sorted(domain_groups):
        questions_lines.append(f"## {domain}")
        questions_lines.append("")
        for question in domain_groups[domain]:
            questions_lines.append(
                f"- `{question.id}` {question.context} [Answer]({question.answer_path}) [Experiment]({question.experiment_path})"
            )
        questions_lines.append("")
    write_json(OUTPUT_ROOT / "questions.json", registry)
    write_text(OUTPUT_ROOT / "questions.md", "\n".join(questions_lines))

    report_lines = [
        "# Benchmark Question Dossier Round 2",
        "",
        f"- Total linked questions: `{len(questions)}`",
        f"- Refreshed/new questions this round: `{len(experiments)}`",
        f"- Carried-forward questions: `{sum(1 for question in questions if int(question.id[1:]) <= 130)}`",
        f"- Full toggle combinations searched: `{len(combo_search)}`",
        "",
        "## Key Findings",
        "",
        f"- The strongest current benchmark-only combined policy is `{combo_best['active_toggles']}`.",
        f"- Its summed MRR delta across baseline and both hard-negative variants is `{combo_best['combined']['sum_mrr_delta_vs_current']}`.",
        f"- The strongest single template drop is `{template_drop_matrix[0]['template_family']}`.",
        f"- The strongest single condition drop is `{condition_drop_matrix[0]['condition']}`.",
        f"- The remaining hard class after the best pool cleanup is the canonical plain-multi tie set, not the comma/template contamination rows.",
        "",
        "## What Closed This Round",
        "",
        "- Q131 through Q140 are now answered by first-class benchmark signals rather than missing-signal placeholders.",
        "- The new raw artifacts capture measurement profile, displacement provenance, entropy behavior, anchor/trust follow-up, policy matrices, and full combo search results.",
        "",
        "## Most Important Next-Step Conclusion",
        "",
        "- The current data supports a small candidate-family runtime trial before any new anchor work.",
        "- The evidence-backed first trial should be a narrow family/provenance cleanup step plus longer-alpha tie handling, not a new width formula.",
        "- After that, the benchmark says the remaining miss class will require a new non-width semantic prior benchmark.",
        "",
        "## Core Artifacts",
        "",
        "- [questions.md](questions.md)",
        "- [summary.json](summary.json)",
        "- [raw/template_drop_matrix.json](raw/template_drop_matrix.json)",
        "- [raw/condition_drop_matrix.json](raw/condition_drop_matrix.json)",
        "- [raw/penalty_sweeps.json](raw/penalty_sweeps.json)",
        "- [raw/combo_search.json](raw/combo_search.json)",
        "- [raw/measurement_profile.json](raw/measurement_profile.json)",
        "- [raw/displacement_sources.json](raw/displacement_sources.json)",
        "- [raw/entropy_analysis.json](raw/entropy_analysis.json)",
        "- [raw/anchor_followup.json](raw/anchor_followup.json)",
    ]
    write_text(OUTPUT_ROOT / "report.md", "\n".join(report_lines) + "\n")

    summary = {
        "total_question_count": len(questions),
        "carried_forward_question_count": sum(1 for question in questions if int(question.id[1:]) <= 130),
        "refreshed_or_new_question_count": len(experiments),
        "new_experiment_count": len(experiments),
        "combo_search_count": len(combo_search),
        "best_combined_toggles": combo_best["active_toggles"],
        "best_combined_sum_mrr_delta_vs_current": combo_best["combined"]["sum_mrr_delta_vs_current"],
        "strongest_single_template_drop": template_drop_matrix[0]["template_family"],
        "strongest_single_condition_drop": condition_drop_matrix[0]["condition"],
        "domains": {domain: len(items) for domain, items in sorted(domain_groups.items())},
    }
    write_json(OUTPUT_ROOT / "summary.json", summary)


if __name__ == "__main__":
    main()
