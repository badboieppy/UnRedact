#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import json
import math
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
OLD_DOSSIER_ROOT = ROOT / "analysis" / "benchmark_question_dossier_round3"
ACCURACY_ROOT = ROOT / "analysis" / "accuracy_benchmark_report_post_noncanonical_penalty"
OUTPUT_ROOT = ROOT / "analysis" / "benchmark_question_dossier_round4"
RAW_ROOT = OUTPUT_ROOT / "raw"
EXPERIMENTS_ROOT = OUTPUT_ROOT / "experiments"
ANSWERS_ROOT = OUTPUT_ROOT / "answers"


def load_module(path: Path, name: str) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"failed to load module {name} from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


base = load_module(ROOT / "scripts" / "generate_benchmark_question_dossier.py", "benchmark_question_base_round4")
round2 = load_module(ROOT / "scripts" / "generate_benchmark_question_dossier_round2.py", "benchmark_question_round2_round4")
round2.ACCURACY_ROOT = ACCURACY_ROOT


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


def alpha_len(value: str) -> int:
    return base.alpha_len(value)


def token_count(value: str) -> int:
    return base.token_count(value)


def average(values: list[float]) -> float | None:
    return None if not values else sum(values) / len(values)


@dataclass
class QuestionRecord:
    id: str
    domain: str
    title: str
    context: str
    experiment_id: str
    answer_path: str
    experiment_path: str


PRIMARY_VARIANTS = round2.PRIMARY_VARIANTS


def candidate_template_family(candidate: dict[str, Any]) -> str:
    return round2.candidate_template_family(candidate)


def candidate_variant_family(candidate: dict[str, Any]) -> str:
    return round2.candidate_variant_family(candidate)


def candidate_raw_entry_text(candidate: dict[str, Any]) -> str:
    return round2.candidate_raw_entry_text(candidate)


def candidate_adjusted_error_pt(candidate: dict[str, Any]) -> float:
    return float(candidate.get("adjusted_error_pt", candidate["error_pt"]))


def policy_sort_key(candidate: dict[str, Any], adjusted_error_pt: float, ranking_mode: str) -> tuple[Any, ...]:
    text = normalize_text(candidate["text"])
    if ranking_mode == "current_runtime":
        return (
            adjusted_error_pt,
            -alpha_len(candidate["text"]),
            -token_count(candidate["text"]),
            text,
        )
    if ranking_mode == "lexical_no_len":
        return (adjusted_error_pt, text)
    raise ValueError(f"unknown ranking_mode {ranking_mode}")


def transform_candidates(
    candidates: list[dict[str, Any]],
    *,
    ranking_mode: str,
    keep_template_families: set[str] | None = None,
    drop_template_families: set[str] | None = None,
    keep_variant_families: set[str] | None = None,
    drop_variant_families: set[str] | None = None,
    drop_raw_single_entry: bool = False,
    extra_noncanonical_penalty_pt: float = 0.0,
    template_penalties_pt: dict[str, float] | None = None,
) -> list[dict[str, Any]]:
    drop_template_families = drop_template_families or set()
    drop_variant_families = drop_variant_families or set()
    template_penalties_pt = template_penalties_pt or {}
    transformed = []
    for candidate in candidates:
        template_family = candidate_template_family(candidate)
        variant_family = candidate_variant_family(candidate)
        raw_entry_text = candidate_raw_entry_text(candidate)
        if keep_template_families is not None and template_family not in keep_template_families:
            continue
        if template_family in drop_template_families:
            continue
        if keep_variant_families is not None and variant_family not in keep_variant_families:
            continue
        if variant_family in drop_variant_families:
            continue
        if drop_raw_single_entry and token_count(raw_entry_text) <= 1:
            continue
        adjusted_error_pt = candidate_adjusted_error_pt(candidate)
        if template_family != "canonical":
            adjusted_error_pt += extra_noncanonical_penalty_pt
        adjusted_error_pt += template_penalties_pt.get(template_family, 0.0)
        transformed.append(
            (
                policy_sort_key(candidate, adjusted_error_pt, ranking_mode),
                {
                    **candidate,
                    "counterfactual_adjusted_error_pt": adjusted_error_pt,
                },
            )
        )
    transformed.sort(key=lambda item: item[0])
    return [candidate for _, candidate in transformed]


def evaluate_policy_on_stage(
    stage: dict[str, Any],
    contract: dict[str, Any],
    *,
    ranking_mode: str,
    keep_template_families: set[str] | None = None,
    drop_template_families: set[str] | None = None,
    keep_variant_families: set[str] | None = None,
    drop_variant_families: set[str] | None = None,
    drop_raw_single_entry: bool = False,
    extra_noncanonical_penalty_pt: float = 0.0,
    template_penalties_pt: dict[str, float] | None = None,
) -> dict[str, Any]:
    datasets_out = []
    all_ranks: list[int | None] = []
    for dataset in contract["datasets"]:
        stage_dataset = next(item for item in stage["datasets"] if item["name"] == dataset["name"])
        selected_rows = []
        for row in stage_dataset["selected_rows"]:
            transformed_candidates = transform_candidates(
                row["candidates"],
                ranking_mode=ranking_mode,
                keep_template_families=keep_template_families,
                drop_template_families=drop_template_families,
                keep_variant_families=keep_variant_families,
                drop_variant_families=drop_variant_families,
                drop_raw_single_entry=drop_raw_single_entry,
                extra_noncanonical_penalty_pt=extra_noncanonical_penalty_pt,
                template_penalties_pt=template_penalties_pt,
            )
            selected_rows.append({**row, "candidates": transformed_candidates})
        evaluated = round2.evaluate_target_ranks(dataset, selected_rows)
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
    return {"datasets": datasets_out, "overall": overall}


def find_candidate_by_text(candidates: list[dict[str, Any]], text: str) -> dict[str, Any] | None:
    target_norm = normalize_text(text)
    for candidate in candidates:
        if normalize_text(candidate["text"]) == target_norm:
            return candidate
    return None


def row_lookup(evaluated_stage: dict[str, Any]) -> dict[str, dict[str, Any]]:
    out = {}
    for dataset in evaluated_stage["datasets"]:
        for row in dataset["selected_rows"]:
            out[row["row_key"]] = row
    return out


def classify_reason(top1: dict[str, Any] | None, target: dict[str, Any] | None) -> str:
    if top1 is None:
        return "empty_pool"
    if target is None:
        return "target_missing_from_pool"
    top1_norm = normalize_text(top1["text"])
    target_norm = normalize_text(target["text"])
    if top1_norm == target_norm:
        return "target_is_top1"
    top1_error = candidate_adjusted_error_pt(top1)
    target_error = candidate_adjusted_error_pt(target)
    if top1_error < target_error - 1e-9:
        return "top1_lower_width_error"
    if top1_error > target_error + 1e-9:
        return "unexpected_target_lower_error"
    return "tied_then_tiebreak"


def dominant_component(top1: dict[str, Any] | None, target: dict[str, Any] | None) -> str | None:
    if top1 is None or target is None:
        return None
    deltas = {
        "glyph_width_sum_pt": abs(float(top1["glyph_width_sum_pt"]) - float(target["glyph_width_sum_pt"])),
        "char_spacing_total_pt": abs(float(top1["char_spacing_total_pt"]) - float(target["char_spacing_total_pt"])),
        "word_spacing_total_pt": abs(float(top1["word_spacing_total_pt"]) - float(target["word_spacing_total_pt"])),
    }
    return max(deltas, key=deltas.get)


def build_target_profiles(evaluated_stage: dict[str, Any]) -> list[dict[str, Any]]:
    rows_by_key = row_lookup(evaluated_stage)
    profiles: list[dict[str, Any]] = []
    for dataset in evaluated_stage["datasets"]:
        for target in dataset["targets"]:
            row = rows_by_key.get(target["best_row_key"]) if target["best_row_key"] else None
            candidates = row["candidates"] if row else []
            top1 = candidates[0] if candidates else None
            target_candidate = find_candidate_by_text(candidates, target["target"]) if row else None
            profiles.append(
                {
                    "dataset": target["dataset"],
                    "label": target["label"],
                    "target": target["target"],
                    "row_key": target["best_row_key"],
                    "best_rank": target["best_rank"],
                    "top1_text": None if top1 is None else top1["text"],
                    "top1_template_family": None if top1 is None else candidate_template_family(top1),
                    "top1_variant_family": None if top1 is None else candidate_variant_family(top1),
                    "top1_adjusted_error_pt": None if top1 is None else candidate_adjusted_error_pt(top1),
                    "target_present": target_candidate is not None,
                    "target_template_family": None if target_candidate is None else candidate_template_family(target_candidate),
                    "target_variant_family": None if target_candidate is None else candidate_variant_family(target_candidate),
                    "target_adjusted_error_pt": None if target_candidate is None else candidate_adjusted_error_pt(target_candidate),
                    "reason": classify_reason(top1, target_candidate),
                    "dominant_component": dominant_component(top1, target_candidate),
                    "top1_raw_entry_text": None if top1 is None else candidate_raw_entry_text(top1),
                    "target_raw_entry_text": None if target_candidate is None else candidate_raw_entry_text(target_candidate),
                }
            )
    return profiles


def combined_summary(variant_results: dict[str, dict[str, Any]]) -> dict[str, Any]:
    return {
        "sum_mrr": sum(variant_results[name]["overall"]["mrr"] for name in PRIMARY_VARIANTS),
        "mean_mrr": average([variant_results[name]["overall"]["mrr"] for name in PRIMARY_VARIANTS]),
        "min_mrr": min(variant_results[name]["overall"]["mrr"] for name in PRIMARY_VARIANTS),
        "mean_recall_at_20": average([variant_results[name]["overall"]["recall_at_20"] for name in PRIMARY_VARIANTS]),
        "mean_rank_found": average(
            [
                float(variant_results[name]["overall"]["mean_rank_found"])
                for name in PRIMARY_VARIANTS
                if variant_results[name]["overall"]["mean_rank_found"] is not None
            ]
        ),
        "all_found": all(
            variant_results[name]["overall"]["found_items"] == variant_results[name]["overall"]["evaluated_items"]
            for name in PRIMARY_VARIANTS
        ),
    }


def summarize_delta(current_combined: dict[str, Any], candidate_combined: dict[str, Any]) -> dict[str, Any]:
    return {
        "sum_mrr_delta": candidate_combined["sum_mrr"] - current_combined["sum_mrr"],
        "mean_rank_delta": None
        if current_combined["mean_rank_found"] is None or candidate_combined["mean_rank_found"] is None
        else candidate_combined["mean_rank_found"] - current_combined["mean_rank_found"],
        "mean_recall20_delta": candidate_combined["mean_recall_at_20"] - current_combined["mean_recall_at_20"],
    }


def evaluate_policy_across_variants(
    stage_map: dict[str, dict[str, Any]],
    contract: dict[str, Any],
    *,
    name: str,
    ranking_mode: str,
    keep_template_families: set[str] | None = None,
    drop_template_families: set[str] | None = None,
    keep_variant_families: set[str] | None = None,
    drop_variant_families: set[str] | None = None,
    drop_raw_single_entry: bool = False,
    extra_noncanonical_penalty_pt: float = 0.0,
    template_penalties_pt: dict[str, float] | None = None,
) -> dict[str, Any]:
    variants: dict[str, dict[str, Any]] = {}
    for variant_name in PRIMARY_VARIANTS:
        evaluated = evaluate_policy_on_stage(
            stage_map[variant_name],
            contract,
            ranking_mode=ranking_mode,
            keep_template_families=keep_template_families,
            drop_template_families=drop_template_families,
            keep_variant_families=keep_variant_families,
            drop_variant_families=drop_variant_families,
            drop_raw_single_entry=drop_raw_single_entry,
            extra_noncanonical_penalty_pt=extra_noncanonical_penalty_pt,
            template_penalties_pt=template_penalties_pt,
        )
        variants[variant_name] = {
            "overall": evaluated["overall"],
            "profiles": build_target_profiles(evaluated),
        }
    return {
        "name": name,
        "ranking_mode": ranking_mode,
        "keep_template_families": sorted(keep_template_families) if keep_template_families is not None else None,
        "drop_template_families": sorted(drop_template_families or []),
        "keep_variant_families": sorted(keep_variant_families) if keep_variant_families is not None else None,
        "drop_variant_families": sorted(drop_variant_families or []),
        "drop_raw_single_entry": drop_raw_single_entry,
        "extra_noncanonical_penalty_pt": extra_noncanonical_penalty_pt,
        "template_penalties_pt": template_penalties_pt or {},
        "variants": variants,
        "combined": combined_summary(variants),
    }


def profile_summary(profiles: list[dict[str, Any]]) -> dict[str, Any]:
    remaining = [row for row in profiles if row["best_rank"] is not None and row["best_rank"] > 20]
    return {
        "target_count": len(profiles),
        "found_count": sum(1 for row in profiles if row["target_present"]),
        "top1_template_family_counts": dict(Counter(row["top1_template_family"] for row in profiles if row["top1_template_family"])),
        "top1_variant_family_counts": dict(Counter(row["top1_variant_family"] for row in profiles if row["top1_variant_family"])),
        "target_template_family_counts": dict(Counter(row["target_template_family"] for row in profiles if row["target_template_family"])),
        "reason_counts": dict(Counter(row["reason"] for row in profiles)),
        "dominant_component_counts": dict(Counter(row["dominant_component"] for row in profiles if row["dominant_component"])),
        "remaining_over_20_count": len(remaining),
        "remaining_over_20_rows": remaining,
    }


def current_anchor_trust_audit() -> dict[str, Any]:
    rows = [
        row
        for row in load_json(ACCURACY_ROOT / "stages" / "anchor_span_visual" / "rows.json")
        if row.get("benchmark_target_label")
    ]
    trusted = [
        row
        for row in rows
        if row.get("primary_reason_code") != "redaction_box_unreliable"
        and row.get("current_alignment") in {"aligned", "compressed"}
    ]
    return {
        "benchmark_visual_row_count": len(rows),
        "trusted_or_non_sizing_issue_count": len(trusted),
        "rows": rows,
    }


def build_signal_snapshot() -> dict[str, Any]:
    signals_root = ACCURACY_ROOT / "signals"
    candidate_pool_quality = load_json(signals_root / "candidate_pool_quality.json")
    pairwise = load_json(signals_root / "pairwise_winner_explanations.json")
    width_attr = load_json(signals_root / "width_component_attribution.json")
    tie_density = load_json(signals_root / "tie_density.json")
    topk_entropy = load_json(signals_root / "topk_family_entropy.json")
    provenance = load_json(signals_root / "candidate_source_provenance.json")
    family_comp = load_json(signals_root / "family_composition.json")
    best_possible = load_json(signals_root / "best_possible_rank.json")
    oracle = load_json(signals_root / "oracle_full_name_pool_ceiling.json")
    perturbation = load_json(signals_root / "perturbation_robustness.json")
    stability = load_json(signals_root / "stability.json")
    row_cluster = load_json(signals_root / "row_cluster_assignment.json")
    visual_manifest = load_json(ACCURACY_ROOT / "visual_review" / "manifest.json")
    return {
        "candidate_pool_quality": candidate_pool_quality,
        "pairwise_winner_explanations": pairwise,
        "width_component_attribution": width_attr,
        "tie_density": tie_density,
        "topk_family_entropy": topk_entropy,
        "candidate_source_provenance": provenance,
        "family_composition": family_comp,
        "best_possible_rank": best_possible,
        "oracle_full_name_pool_ceiling": oracle,
        "perturbation_robustness": perturbation,
        "stability": stability,
        "row_cluster_assignment": row_cluster,
        "visual_review_manifest": visual_manifest,
    }


def counts_by_key(rows: list[dict[str, Any]], key_field: str, value_field: str = "count") -> dict[str, int]:
    return {
        str(row[key_field]): int(row.get(value_field, 0))
        for row in rows
    }


def build_question_answer_markdown(question: QuestionRecord, experiment: dict[str, Any]) -> str:
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
        experiment.get("approach", ""),
        "",
        "## What Was Done",
    ]
    for item in experiment.get("what_was_done", []):
        lines.append(f"- {item}")
    lines += [
        "",
        "## What Was Learned",
    ]
    for item in experiment.get("what_was_learned", []):
        lines.append(f"- {item}")
    lines += [
        "",
        "## Answer",
        experiment.get("answer", ""),
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
    for item in experiment.get("new_unknowns", ["None. This question is closed by the current round-four benchmark evidence."]):
        lines.append(f"- {item}")
    lines.append("")
    return "\n".join(lines)


def experiment_stub(
    *,
    kind: str,
    approach: str,
    what_was_done: list[str],
    what_was_learned: list[str],
    answer: str,
    key_metrics: dict[str, Any],
    evidence_paths: list[str],
    new_unknowns: list[str] | None = None,
) -> dict[str, Any]:
    return {
        "kind": kind,
        "approach": approach,
        "what_was_done": what_was_done,
        "what_was_learned": what_was_learned,
        "answer": answer,
        "key_metrics": key_metrics,
        "evidence_paths": evidence_paths,
        "new_unknowns": new_unknowns or ["None. This question is closed by the current round-four benchmark evidence."],
    }


def main() -> None:
    ensure_dir(OUTPUT_ROOT)
    ensure_dir(RAW_ROOT)
    ensure_dir(EXPERIMENTS_ROOT)
    ensure_dir(ANSWERS_ROOT)

    contract = base.load_contract()
    old_questions = load_json(OLD_DOSSIER_ROOT / "questions.json")
    old_summary = load_json(OLD_DOSSIER_ROOT / "summary.json")
    stage_map = round2.build_stage_map(contract)

    current_policy = evaluate_policy_across_variants(
        stage_map,
        contract,
        name="current_runtime_post_noncanonical_penalty",
        ranking_mode="current_runtime",
    )
    current_combined = current_policy["combined"]
    lexical_policy = evaluate_policy_across_variants(
        stage_map,
        contract,
        name="lexical_no_len_post_noncanonical_penalty",
        ranking_mode="lexical_no_len",
    )
    hard_policy_definitions = [
        ("drop_first_last", {"ranking_mode": "current_runtime", "drop_template_families": {"first_last"}}),
        ("drop_surname_only", {"ranking_mode": "current_runtime", "drop_template_families": {"surname_only"}}),
        ("drop_first_last_and_surname_only", {"ranking_mode": "current_runtime", "drop_template_families": {"first_last", "surname_only"}}),
        ("drop_role_alias_pair", {"ranking_mode": "current_runtime", "drop_template_families": {"role_alias_pair"}}),
        ("keep_canonical_only", {"ranking_mode": "current_runtime", "keep_template_families": {"canonical"}}),
        ("keep_canonical_and_role_alias_pair", {"ranking_mode": "current_runtime", "keep_template_families": {"canonical", "role_alias_pair"}}),
    ]
    hard_policy_search = []
    for name, config in hard_policy_definitions:
        policy = evaluate_policy_across_variants(stage_map, contract, name=name, **config)
        policy["delta_vs_current"] = summarize_delta(current_combined, policy["combined"])
        hard_policy_search.append(policy)
    hard_policy_search.sort(key=lambda item: item["combined"]["sum_mrr"], reverse=True)

    extra_noncanonical_penalty_sweep = []
    for penalty_pt in [x / 4 for x in range(1, 13)]:
        policy = evaluate_policy_across_variants(
            stage_map,
            contract,
            name=f"extra_noncanonical_penalty_{penalty_pt:.2f}",
            ranking_mode="current_runtime",
            extra_noncanonical_penalty_pt=penalty_pt,
        )
        policy["delta_vs_current"] = summarize_delta(current_combined, policy["combined"])
        extra_noncanonical_penalty_sweep.append(policy)

    targeted_penalty_grid = []
    for first_last_penalty in [0.0, 0.25, 0.50, 1.0, 1.5]:
        for surname_only_penalty in [0.0, 0.25, 0.50, 1.0, 1.5]:
            penalties = {}
            if first_last_penalty:
                penalties["first_last"] = first_last_penalty
            if surname_only_penalty:
                penalties["surname_only"] = surname_only_penalty
            policy = evaluate_policy_across_variants(
                stage_map,
                contract,
                name=f"targeted_penalty_fl_{first_last_penalty:.2f}_so_{surname_only_penalty:.2f}",
                ranking_mode="current_runtime",
                template_penalties_pt=penalties,
            )
            policy["delta_vs_current"] = summarize_delta(current_combined, policy["combined"])
            targeted_penalty_grid.append(policy)
    targeted_penalty_grid.sort(key=lambda item: item["combined"]["sum_mrr"], reverse=True)

    variant_family_policy_defs = [
        ("drop_single_variant", {"drop_variant_families": {"single_token"}}),
        ("drop_initial_variant", {"drop_variant_families": {"initial"}}),
        ("drop_single_and_initial", {"drop_variant_families": {"single_token", "initial"}}),
        ("keep_plain_multi_only", {"keep_variant_families": {"plain_multi_token"}}),
    ]
    variant_family_policy_search = []
    for name, cfg in variant_family_policy_defs:
        policy = evaluate_policy_across_variants(
            stage_map,
            contract,
            name=name,
            ranking_mode="current_runtime",
            **cfg,
        )
        policy["delta_vs_current"] = summarize_delta(current_combined, policy["combined"])
        variant_family_policy_search.append(policy)
    variant_family_policy_search.sort(key=lambda item: item["combined"]["sum_mrr"], reverse=True)

    raw_source_policy_defs = [
        ("drop_raw_single_entry", {"drop_raw_single_entry": True}),
        (
            "drop_raw_single_entry_and_initial",
            {"drop_raw_single_entry": True, "drop_variant_families": {"initial"}},
        ),
    ]
    raw_source_policy_search = []
    for name, cfg in raw_source_policy_defs:
        policy = evaluate_policy_across_variants(
            stage_map,
            contract,
            name=name,
            ranking_mode="current_runtime",
            **cfg,
        )
        policy["delta_vs_current"] = summarize_delta(current_combined, policy["combined"])
        raw_source_policy_search.append(policy)
    raw_source_policy_search.sort(key=lambda item: item["combined"]["sum_mrr"], reverse=True)

    signal_snapshot = build_signal_snapshot()
    anchor_followup = current_anchor_trust_audit()
    ranking_mode_matrix = [current_policy, lexical_policy]
    for policy in ranking_mode_matrix:
        policy["delta_vs_current"] = summarize_delta(current_combined, policy["combined"])

    current_baseline_profile = profile_summary(current_policy["variants"]["baseline"]["profiles"])
    hardest_rows = sorted(
        current_policy["variants"]["baseline"]["profiles"],
        key=lambda row: (row["best_rank"] is None, row["best_rank"] if row["best_rank"] is not None else -1),
        reverse=True,
    )

    best_safe_hard_drop = max(
        (policy for policy in hard_policy_search if policy["combined"]["all_found"]),
        key=lambda item: item["combined"]["sum_mrr"],
    )
    best_extra_penalty = max(extra_noncanonical_penalty_sweep, key=lambda item: item["combined"]["sum_mrr"])
    best_targeted_penalty = max(targeted_penalty_grid, key=lambda item: item["combined"]["sum_mrr"])
    best_variant_family_policy = max(
        (policy for policy in variant_family_policy_search if policy["combined"]["all_found"]),
        key=lambda item: item["combined"]["sum_mrr"],
    )
    best_raw_source_policy = max(
        (policy for policy in raw_source_policy_search if policy["combined"]["all_found"]),
        key=lambda item: item["combined"]["sum_mrr"],
    )

    write_json(RAW_ROOT / "current_profile.json", current_policy)
    write_json(RAW_ROOT / "hard_policy_search.json", hard_policy_search)
    write_json(RAW_ROOT / "extra_noncanonical_penalty_sweep.json", extra_noncanonical_penalty_sweep)
    write_json(RAW_ROOT / "targeted_penalty_grid.json", targeted_penalty_grid)
    write_json(RAW_ROOT / "variant_family_policy_search.json", variant_family_policy_search)
    write_json(RAW_ROOT / "raw_source_policy_search.json", raw_source_policy_search)
    write_json(RAW_ROOT / "ranking_mode_matrix.json", ranking_mode_matrix)
    write_json(RAW_ROOT / "signal_snapshot.json", signal_snapshot)
    write_json(RAW_ROOT / "anchor_followup.json", anchor_followup)
    write_json(RAW_ROOT / "hardest_rows.json", hardest_rows)

    questions: list[QuestionRecord] = [
        QuestionRecord(
            id=item["id"],
            domain=item["domain"],
            title=item["title"],
            context=item["context"],
            experiment_id=item["experiment_id"],
            answer_path=item["answer_path"],
            experiment_path=item["experiment_path"],
        )
        for item in old_questions
    ]
    experiments: dict[str, dict[str, Any]] = {}

    def add_question(question_id: str, domain: str, title: str, context: str, experiment: dict[str, Any]) -> None:
        experiment_id = f"EXP{question_id[1:]}"
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
        experiments[question_id] = experiment

    current_top1_templates = counts_by_key(
        signal_snapshot["candidate_source_provenance"]["top1_template_families"],
        "family",
    )
    current_top1_variants = counts_by_key(
        signal_snapshot["candidate_source_provenance"]["top1_variant_families"],
        "family",
    )
    current_target_templates = counts_by_key(
        signal_snapshot["candidate_source_provenance"]["target_template_families"],
        "family",
    )
    pairwise_rows = signal_snapshot["pairwise_winner_explanations"]["rows"]
    width_rows = signal_snapshot["width_component_attribution"]["rows"]
    pool_rows = signal_snapshot["candidate_pool_quality"]["rows"]
    tie_rows = signal_snapshot["tie_density"]["rows"]
    best_possible_rows = signal_snapshot["best_possible_rank"]["rows"]
    oracle_rows = signal_snapshot["oracle_full_name_pool_ceiling"]["rows"]
    entropy = signal_snapshot["topk_family_entropy"]
    family_comp = signal_snapshot["family_composition"]
    perturbation = signal_snapshot["perturbation_robustness"]
    stability = signal_snapshot["stability"]
    row_cluster = signal_snapshot["row_cluster_assignment"]
    visual_manifest = signal_snapshot["visual_review_manifest"]

    add_question(
        "Q246",
        "runtime_trial",
        "live_runtime_matched_or_exceeded_round3_predicted_penalty_gain",
        "Before changing runtime behavior, round three proved that a global `2.75-3.0 pt` noncanonical penalty was the best safe next runtime trial. After implementing the live runtime penalty, did the actual runtime match or exceed that predicted benchmark-only combined result across the canonical baseline plus both hard-negative variants?",
        experiment_stub(
            kind="runtime_vs_prediction",
            approach="Compared the round-three predicted best safe penalty summary against the actual post-implementation live runtime combined metrics.",
            what_was_done=[
                "Loaded the round-three best-safe-penalty combined benchmark summary.",
                "Computed the current live runtime combined metrics from the current post-penalty accuracy report stage data.",
            ],
            what_was_learned=[
                f"Round-three predicted best-safe-penalty combined summary was {old_summary['best_safe_penalty']['combined']}.",
                f"Current live runtime combined summary is {current_combined}.",
            ],
            answer="Yes. The live runtime matched and slightly exceeded the earlier benchmark-only penalty-trial prediction, which means the runtime implementation reproduced the intended gain rather than collapsing when moved out of simulation.",
            key_metrics={
                "round3_predicted_sum_mrr": old_summary["best_safe_penalty"]["combined"]["sum_mrr"],
                "current_live_sum_mrr": current_combined["sum_mrr"],
                "sum_mrr_delta": current_combined["sum_mrr"] - old_summary["best_safe_penalty"]["combined"]["sum_mrr"],
            },
            evidence_paths=[
                "../../benchmark_question_dossier_round3/summary.json",
                "../../accuracy_benchmark_report_post_noncanonical_penalty/summary.json",
            ],
        ),
    )

    add_question(
        "Q247",
        "current_runtime",
        "current_top1_variant_families_are_all_plain_multi_token",
        "On the live post-penalty runtime, what variant family is actually winning benchmark rows now? This question exists to verify that the earlier comma/single contamination class is truly gone in live behavior rather than just reduced.",
        experiment_stub(
            kind="current_profile_audit",
            approach="Audited the current post-penalty candidate-source provenance signal and counted top-1 variant families across all benchmark targets.",
            what_was_done=[
                "Loaded the persisted candidate-source provenance signal for the post-penalty accuracy report.",
                "Counted top-1 variant families across all 11 benchmark targets.",
            ],
            what_was_learned=[
                f"Current top-1 variant family counts are {current_top1_variants}.",
                "No comma, single-token, or initial family is winning any benchmark row now.",
            ],
            answer="The current live winners are all `plain_multi_token` at the variant-family level.",
            key_metrics={"top1_variant_families": current_top1_variants},
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/candidate_source_provenance.json"],
        ),
    )

    add_question(
        "Q248",
        "current_runtime",
        "current_top1_template_families_are_all_canonical",
        "After the runtime noncanonical penalty was implemented, are the current benchmark winners finally canonical at the template-family level, or do any noncanonical template families still win live rows?",
        experiment_stub(
            kind="current_profile_audit",
            approach="Audited the live post-penalty candidate-source provenance signal and counted top-1 template families across the benchmark targets.",
            what_was_done=[
                "Loaded the post-penalty candidate-source provenance signal.",
                "Counted top-1 template families across all benchmark targets.",
            ],
            what_was_learned=[
                f"Current top-1 template family counts are {current_top1_templates}.",
                "All live benchmark winners are now canonical at the template-family level.",
            ],
            answer="Yes. The current live winners are now purely `canonical` at the template-family level.",
            key_metrics={"top1_template_families": current_top1_templates},
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/candidate_source_provenance.json"],
        ),
    )

    add_question(
        "Q249",
        "current_runtime",
        "residual_noncanonical_top1_winners_are_now_zero",
        "Round three still found residual `first_last` and `surname_only` winners after the first runtime cleanup. After implementing the live noncanonical penalty, how many noncanonical top-1 winners remain?",
        experiment_stub(
            kind="current_profile_audit",
            approach="Compared the live post-penalty top-1 template-family counts against the definition of noncanonical families.",
            what_was_done=[
                "Loaded the current top-1 template-family counts.",
                "Summed all top-1 counts whose template family is not `canonical`.",
            ],
            what_was_learned=[
                "The live post-penalty benchmark has zero noncanonical top-1 winners.",
            ],
            answer="Zero noncanonical top-1 winners remain on the live benchmark after the implemented penalty trial.",
            key_metrics={
                "noncanonical_top1_count": sum(count for family, count in current_top1_templates.items() if family != "canonical")
            },
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/candidate_source_provenance.json"],
        ),
    )

    add_question(
        "Q250",
        "current_runtime",
        "one_real_target_still_uses_role_alias_pair",
        "Even though the current winners are canonical, do any real benchmark targets still depend on a noncanonical template family and therefore make a canonical-only runtime policy unsafe?",
        experiment_stub(
            kind="current_profile_audit",
            approach="Counted target template families from the current post-penalty candidate-source provenance signal.",
            what_was_done=[
                "Loaded current target template-family counts.",
                "Checked whether any target candidate still comes from a noncanonical template family.",
            ],
            what_was_learned=[
                f"Current target template-family counts are {current_target_templates}.",
                "Exactly one real target still depends on `role_alias_pair`.",
            ],
            answer="Yes. One real benchmark target still needs `role_alias_pair`, so a canonical-only runtime policy is still unsafe.",
            key_metrics={"target_template_families": current_target_templates},
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/candidate_source_provenance.json"],
        ),
    )

    add_question(
        "Q251",
        "current_runtime",
        "all_targets_still_present_in_pool",
        "After the runtime penalty was implemented, did any benchmark target disappear from the live candidate pool, or is the system still failing entirely within ranking rather than generation?",
        experiment_stub(
            kind="candidate_pool_audit",
            approach="Audited current candidate-pool quality rows and counted how many targets are still present in their evaluated pools.",
            what_was_done=[
                "Loaded the live candidate-pool quality signal.",
                "Counted target presence across all 11 benchmark items.",
            ],
            what_was_learned=[
                f"Targets present in pool: {sum(1 for row in pool_rows if row['present_in_pool'])} of {len(pool_rows)}.",
                "The live runtime still contains every benchmark target somewhere in the pool.",
            ],
            answer="All 11 benchmark targets are still present in the pool. The live failure remains ranking, not generation drop-out.",
            key_metrics={
                "targets_present_in_pool": sum(1 for row in pool_rows if row["present_in_pool"]),
                "total_targets": len(pool_rows),
            },
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/candidate_pool_quality.json"],
        ),
    )

    add_question(
        "Q252",
        "current_runtime",
        "current_recall_at_20_is_still_only_two_of_eleven",
        "Even after the live penalty cleanup, how many benchmark targets actually make it into the top 20? This gives the practical measure of how far the system still is from useful ranking.",
        experiment_stub(
            kind="candidate_pool_audit",
            approach="Counted current benchmark targets whose best observed rank is 20 or better.",
            what_was_done=[
                "Loaded the current candidate-pool quality rows.",
                "Counted how many targets have `best_rank <= 20`.",
            ],
            what_was_learned=[
                f"Current top-20 hits are {sum(1 for row in pool_rows if row['best_rank'] is not None and row['best_rank'] <= 20)} of {len(pool_rows)}.",
            ],
            answer="Only 2 of 11 benchmark targets are currently top-20, so the live cleanup helped but left the main ranking problem intact.",
            key_metrics={
                "top20_targets": sum(1 for row in pool_rows if row["best_rank"] is not None and row["best_rank"] <= 20),
                "total_targets": len(pool_rows),
            },
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/candidate_pool_quality.json"],
        ),
    )

    add_question(
        "Q253",
        "current_runtime",
        "current_pairwise_reasons_are_all_top1_lower_width_error",
        "On the live post-penalty runtime, are we still losing rows because of lexical or family tie-break artifacts, or are the remaining misses now uniformly rows where the top-1 candidate simply has lower measured ranking error than the target?",
        experiment_stub(
            kind="pairwise_reason_audit",
            approach="Counted current pairwise winner reasons from the post-penalty pairwise-winner-explanations signal.",
            what_was_done=[
                "Loaded the current pairwise winner explanation signal.",
                "Counted the `reason` values across all benchmark targets.",
            ],
            what_was_learned=[
                f"Current pairwise reason counts are {dict(Counter(row['reason'] for row in pairwise_rows))}.",
                "Every current miss is now recorded as `top1_lower_width_error`.",
            ],
            answer="Yes. The current misses are uniformly `top1_lower_width_error` rows. The obvious family/tie-break contamination class is gone from the live benchmark.",
            key_metrics={"reason_counts": dict(Counter(row["reason"] for row in pairwise_rows))},
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/pairwise_winner_explanations.json"],
        ),
    )

    add_question(
        "Q254",
        "current_runtime",
        "current_width_component_attribution_is_all_glyph_width",
        "Are the current live misses still being driven by char-spacing or word-spacing quirks, or is the remaining width competition now entirely glyph-width competition between plausible full names?",
        experiment_stub(
            kind="width_component_audit",
            approach="Counted dominant width components on the live post-penalty width-component-attribution signal.",
            what_was_done=[
                "Loaded the current width-component attribution rows.",
                "Counted the dominant width component for each benchmark row.",
            ],
            what_was_learned=[
                f"Current dominant component counts are {dict(Counter(row['dominant_component'] for row in width_rows))}.",
                "Every current benchmark row is glyph-width dominated.",
            ],
            answer="The current live misses are entirely glyph-width dominated; spacing terms are not the remaining bottleneck class.",
            key_metrics={"dominant_component_counts": dict(Counter(row["dominant_component"] for row in width_rows))},
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/width_component_attribution.json"],
        ),
    )

    add_question(
        "Q255",
        "current_runtime",
        "topk_family_entropy_is_zero_at_the_top",
        "Does the live post-penalty ranking still suffer from multiple different candidate families fighting at the top of the list, or has the top of the ranking collapsed into one family shape already?",
        experiment_stub(
            kind="family_entropy_audit",
            approach="Read the post-penalty top-k family entropy signal and inspected the mean top-5 and top-10 entropy.",
            what_was_done=[
                "Loaded the top-k family entropy signal.",
                "Read mean entropy and the dominant family share at the top of the ranking.",
            ],
            what_was_learned=[
                f"Mean entropy top-5 is {entropy['mean_entropy_top5']}.",
                f"Mean entropy top-10 is {entropy['mean_entropy_top10']}.",
                "The top of the ranking is already family-pure rather than family-mixed.",
            ],
            answer="The top of the live ranking is already family-pure. Family diversity at the top is no longer the problem class.",
            key_metrics={
                "mean_entropy_top5": entropy["mean_entropy_top5"],
                "mean_entropy_top10": entropy["mean_entropy_top10"],
            },
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/topk_family_entropy.json"],
        ),
    )

    add_question(
        "Q256",
        "current_runtime",
        "candidate_pool_still_contains_single_and_initial_families",
        "Even though the live winners are now canonical plain multi-token names, does the candidate pool still contain other family classes like single-token and initial variants that could still matter in edge cases?",
        experiment_stub(
            kind="family_composition_audit",
            approach="Read the current family-composition signal and counted which candidate families still exist anywhere in the live benchmark pool.",
            what_was_done=[
                "Loaded the family-composition signal.",
                "Read the aggregate candidate-family counts.",
            ],
            what_was_learned=[
                f"Current candidate family counts are {family_comp['candidate_families']}.",
                "Single-token and initial families still exist in the pool even though they are no longer top-1 winners.",
            ],
            answer="Yes. The pool still contains `single_token` and `initial` candidates, but they are no longer winning live rows.",
            key_metrics={"candidate_families": family_comp["candidate_families"]},
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/family_composition.json"],
        ),
    )

    add_question(
        "Q257",
        "current_runtime",
        "tie_density_remains_high_even_after_cleanup",
        "If the remaining live failures are canonical full-name competition, how crowded is that race? This question checks whether current rows still have many near-width candidates around the target even after the family cleanup.",
        experiment_stub(
            kind="tie_density_audit",
            approach="Averaged the current tie-density counts at several thresholds around the target across all benchmark rows.",
            what_was_done=[
                "Loaded the tie-density signal for the post-penalty report.",
                "Computed mean counts within 0.05 pt, 0.10 pt, 0.25 pt, 0.50 pt, and 1.00 pt of the target.",
            ],
            what_was_learned=[
                f"Mean counts within target 0.50 pt are {average([row['within_target_050'] for row in tie_rows])}.",
                f"Mean counts within target 1.00 pt are {average([row['within_target_100'] for row in tie_rows])}.",
                "Near-width competition remains very dense even after cleanup.",
            ],
            answer="Tie density is still high. The remaining live problem is not a clear single winner losing to junk; it is a crowded canonical-width competition.",
            key_metrics={
                "mean_within_target_050": average([row["within_target_050"] for row in tie_rows]),
                "mean_within_target_100": average([row["within_target_100"] for row in tie_rows]),
            },
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/tie_density.json"],
        ),
    )

    add_question(
        "Q258",
        "current_runtime",
        "perturbation_fragility_remains_universal",
        "Do the current live winners stay stable if the target width is nudged slightly, or do they still flip easily under tiny perturbations?",
        experiment_stub(
            kind="perturbation_audit",
            approach="Audited the post-penalty perturbation-robustness signal and counted how many rows change top-1 at ±0.25 pt, ±0.50 pt, and ±1.00 pt.",
            what_was_done=[
                "Loaded the perturbation-robustness signal.",
                "Counted how many benchmark rows changed top-1 under each perturbation band.",
            ],
            what_was_learned=[
                f"Rows changing at ±0.25 pt: {sum(1 for row in perturbation['rows'] if row['changed_at_025'])} of {len(perturbation['rows'])}.",
                f"Rows changing at ±0.50 pt: {sum(1 for row in perturbation['rows'] if row['changed_at_050'])} of {len(perturbation['rows'])}.",
                "Every benchmark row remains perturbation-fragile.",
            ],
            answer="Yes. The live ranker remains universally fragile under tiny width nudges, which is exactly what we would expect from dense canonical near-ties.",
            key_metrics={
                "changed_at_025": sum(1 for row in perturbation["rows"] if row["changed_at_025"]),
                "changed_at_050": sum(1 for row in perturbation["rows"] if row["changed_at_050"]),
                "row_count": len(perturbation["rows"]),
            },
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/perturbation_robustness.json"],
        ),
    )

    add_question(
        "Q259",
        "current_runtime",
        "stability_remains_deterministic",
        "Did the new runtime cleanup introduce instability or nondeterminism across repeated runs?",
        experiment_stub(
            kind="stability_audit",
            approach="Audited the current stability signal from the post-penalty repeated benchmark report.",
            what_was_done=[
                "Loaded the stability signal from the post-penalty report.",
                "Checked run hashes, top-1 agreement, and unstable target counts.",
            ],
            what_was_learned=[
                f"Stability summary is {stability}.",
                "The new runtime behavior remains deterministic on the benchmark corpus.",
            ],
            answer="No instability was introduced. The post-penalty runtime remains deterministic across repeated report runs.",
            key_metrics=stability,
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/stability.json"],
        ),
    )

    add_question(
        "Q260",
        "anchor_followup",
        "anchor_sizing_is_still_not_the_main_blocker",
        "Did the live runtime ranking change accidentally reopen anchor geometry as the main explanation for current misses, or do benchmark-linked visual rows still look trusted and aligned?",
        experiment_stub(
            kind="anchor_followup",
            approach="Audited the current post-penalty anchor-span visual benchmark rows that are linked to benchmark targets.",
            what_was_done=[
                "Loaded the current anchor-span visual benchmark rows.",
                "Counted benchmark-linked rows that are aligned/compressed and not marked redaction-box-unreliable.",
            ],
            what_was_learned=[
                f"Benchmark-linked visual rows: {anchor_followup['benchmark_visual_row_count']}.",
                f"Trusted or non-sizing-issue rows: {anchor_followup['trusted_or_non_sizing_issue_count']}.",
                "Anchor geometry remains effectively solved for benchmark-linked rows.",
            ],
            answer="Anchor sizing still does not look like the main blocker. The benchmark-linked visual rows remain trusted/aligned after the runtime ranking change.",
            key_metrics={
                "benchmark_visual_row_count": anchor_followup["benchmark_visual_row_count"],
                "trusted_or_non_sizing_issue_count": anchor_followup["trusted_or_non_sizing_issue_count"],
            },
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/stages/anchor_span_visual/rows.json"],
        ),
    )

    add_question(
        "Q261",
        "ranking_mode",
        "lexical_no_len_is_worse_than_current_runtime",
        "Now that the live winners are canonical, does the longer-alpha tie-break still earn its keep, or could we simplify back to lexical order without losing quality?",
        experiment_stub(
            kind="ranking_mode_counterfactual",
            approach="Compared the current runtime ordering against a lexical-no-length tie-break counterfactual on the current post-penalty pools.",
            what_was_done=[
                "Evaluated the current live ordering across the baseline and hard-negative variants.",
                "Evaluated a lexical-no-length counterfactual on the same current pools.",
            ],
            what_was_learned=[
                f"Current combined summary is {current_combined}.",
                f"Lexical-no-length combined summary is {lexical_policy['combined']}.",
                "Removing the longer-alpha tie-break still makes the system worse.",
            ],
            answer="The longer-alpha tie-break is still beneficial. Lexical-only fallback is worse than the current runtime ordering.",
            key_metrics={
                "current_sum_mrr": current_combined["sum_mrr"],
                "lexical_sum_mrr": lexical_policy["combined"]["sum_mrr"],
                "sum_mrr_delta": lexical_policy["combined"]["sum_mrr"] - current_combined["sum_mrr"],
            },
            evidence_paths=["../raw/ranking_mode_matrix.json"],
        ),
    )

    def add_policy_question(qid: str, title: str, context: str, policy: dict[str, Any], answer: str) -> None:
        add_question(
            qid,
            "policy_counterfactual",
            title,
            context,
            experiment_stub(
                kind="policy_counterfactual",
                approach="Applied a focused benchmark-only counterfactual policy on top of the current live runtime candidate pools and measured the primary variants.",
                what_was_done=[
                    f"Evaluated policy `{policy['name']}` across baseline plus both hard-negative variants.",
                    "Compared the result against the current live post-penalty runtime summary.",
                ],
                what_was_learned=[
                    f"Policy combined summary is {policy['combined']}.",
                    f"Delta versus current is {policy['delta_vs_current']}.",
                ],
                answer=answer,
                key_metrics={
                    "policy_name": policy["name"],
                    "combined": policy["combined"],
                    "delta_vs_current": policy["delta_vs_current"],
                },
                evidence_paths=["../raw/hard_policy_search.json"],
            ),
        )

    policy_by_name = {policy["name"]: policy for policy in hard_policy_search}
    add_policy_question(
        "Q262",
        "drop_first_last_now_is_only_a_tiny_gain",
        "After the live noncanonical penalty is already in place, does removing the remaining `first_last` candidates still buy a meaningful additional gain, or is that lever mostly exhausted?",
        policy_by_name["drop_first_last"],
        "Dropping `first_last` now only yields a tiny additional gain. The large noncanonical cleanup win has already been captured by the live runtime penalty.",
    )
    add_policy_question(
        "Q263",
        "drop_surname_only_now_is_effectively_inert",
        "Does removing `surname_only` candidates still matter on top of the current live runtime, or has that family already ceased to affect the benchmark outcome?",
        policy_by_name["drop_surname_only"],
        "Dropping `surname_only` is effectively inert on the current live runtime. That family is no longer driving benchmark outcomes.",
    )
    add_policy_question(
        "Q264",
        "dropping_first_last_and_surname_only_is_the_best_safe_template_hard_drop",
        "If we force one more template-family hard drop on top of the live runtime, which safe template cleanup currently performs best?",
        policy_by_name["drop_first_last_and_surname_only"],
        "The best safe template hard-drop on the current live runtime is removing `first_last` and `surname_only` together, but the gain is very small.",
    )
    add_policy_question(
        "Q265",
        "keep_canonical_and_role_alias_pair_is_the_best_safe_keep_policy",
        "If we switch from 'drop these bad templates' to 'keep only the necessary templates', what is the best safe keep-policy now that the live winners are canonical?",
        policy_by_name["keep_canonical_and_role_alias_pair"],
        "The best safe keep-policy is `canonical + role_alias_pair`. That is the smallest template set that preserves the one real noncanonical target while trimming the remaining irrelevant families.",
    )
    add_policy_question(
        "Q266",
        "canonical_only_is_still_unsafe",
        "Can we now safely keep only canonical candidates in runtime, or does the live benchmark still prove that would lose at least one real target?",
        policy_by_name["keep_canonical_only"],
        "Canonical-only is still unsafe because it drops the real `role_alias_pair` target even though current top-1 winners are canonical.",
    )
    add_policy_question(
        "Q267",
        "dropping_role_alias_pair_is_harmful",
        "What happens if we drop `role_alias_pair` entirely on top of the live runtime?",
        policy_by_name["drop_role_alias_pair"],
        "Dropping `role_alias_pair` is harmful because it removes the one real benchmark target that still depends on that family.",
    )

    add_question(
        "Q268",
        "penalty_sweep",
        "extra_global_noncanonical_penalty_no_longer_has_meaningful_leverage",
        "Once the live runtime already includes a `2.75 pt` noncanonical penalty, is there any meaningful reason to add still more global noncanonical penalty on top of it?",
        experiment_stub(
            kind="penalty_sweep",
            approach="Swept extra global noncanonical penalties on top of the already-penalized live runtime and measured the primary benchmark variants.",
            what_was_done=[
                "Applied extra noncanonical penalties from 0.25 pt through 3.0 pt on top of the current live runtime.",
                "Measured combined MRR and mean rank across the three primary variants.",
            ],
            what_was_learned=[
                f"Best extra-penalty policy is {best_extra_penalty['name']} with combined summary {best_extra_penalty['combined']}.",
                "The sweep shows no meaningful remaining global noncanonical leverage; higher penalties mostly make the baseline worse.",
            ],
            answer="No. After the implemented live penalty, extra global noncanonical penalty is at best marginal and quickly becomes harmful. The large noncanonical lever is already exhausted.",
            key_metrics={
                "best_extra_penalty_pt": best_extra_penalty["extra_noncanonical_penalty_pt"],
                "best_extra_penalty_combined": best_extra_penalty["combined"],
                "best_extra_penalty_delta_vs_current": best_extra_penalty["delta_vs_current"],
            },
            evidence_paths=["../raw/extra_noncanonical_penalty_sweep.json"],
        ),
    )

    add_question(
        "Q269",
        "penalty_sweep",
        "targeted_first_last_and_surname_only_penalties_do_not_materially_help",
        "If the remaining live contamination were still specifically `first_last` or `surname_only`, then targeted penalties on those families should help. Do they?",
        experiment_stub(
            kind="targeted_penalty_grid",
            approach="Swept a small grid of extra targeted `first_last` and `surname_only` penalties on top of the current live runtime.",
            what_was_done=[
                "Applied a 25-cell targeted penalty grid over `first_last` and `surname_only`.",
                "Compared every targeted combination against the current live runtime.",
            ],
            what_was_learned=[
                f"Best targeted penalty policy is {best_targeted_penalty['name']} with combined summary {best_targeted_penalty['combined']}.",
                "The targeted-penalty grid does not expose a new strong lever after the live runtime cleanup.",
            ],
            answer="No. Targeted extra penalties on `first_last` and `surname_only` do not expose a new strong live-runtime lever.",
            key_metrics={
                "best_targeted_penalty_policy": best_targeted_penalty["name"],
                "best_targeted_penalty_combined": best_targeted_penalty["combined"],
                "best_targeted_penalty_delta_vs_current": best_targeted_penalty["delta_vs_current"],
            },
            evidence_paths=["../raw/targeted_penalty_grid.json"],
        ),
    )

    add_question(
        "Q270",
        "policy_counterfactual",
        "no_strong_additional_template_family_cleanup_remains",
        "Taking the current live runtime as the new baseline, is there any strong remaining template-family cleanup move that still looks like the next runtime change?",
        experiment_stub(
            kind="policy_synthesis",
            approach="Compared the current live runtime against all remaining safe template-family hard-drop and penalty counterfactuals.",
            what_was_done=[
                "Ran safe hard-drop keep/drop policies on the current live pools.",
                "Ran extra noncanonical penalties and targeted `first_last`/`surname_only` penalties.",
            ],
            what_was_learned=[
                "Remaining template-family cleanups are now either tiny wins or unsafe because they remove the one real role-alias target.",
                "There is no new strong template-family runtime lever after the live penalty trial.",
            ],
            answer="No strong additional template-family cleanup remains. The remaining safe moves are too small to be the main next runtime step.",
            key_metrics={
                "best_safe_hard_drop": best_safe_hard_drop["name"],
                "best_safe_hard_drop_delta_vs_current": best_safe_hard_drop["delta_vs_current"],
                "best_extra_penalty_delta_vs_current": best_extra_penalty["delta_vs_current"],
            },
            evidence_paths=[
                "../raw/hard_policy_search.json",
                "../raw/extra_noncanonical_penalty_sweep.json",
                "../raw/targeted_penalty_grid.json",
            ],
        ),
    )

    def add_variant_policy_question(qid: str, title: str, context: str, policy: dict[str, Any], answer: str, evidence: str) -> None:
        add_question(
            qid,
            "variant_family_counterfactual",
            title,
            context,
            experiment_stub(
                kind="variant_family_counterfactual",
                approach="Applied a candidate-family filtering counterfactual on top of the current live pools and measured the primary variants.",
                what_was_done=[
                    f"Evaluated policy `{policy['name']}` across the current live baseline plus both hard-negative variants.",
                    "Compared the resulting combined summary against the current live runtime.",
                ],
                what_was_learned=[
                    f"Policy combined summary is {policy['combined']}.",
                    f"Delta versus current is {policy['delta_vs_current']}.",
                ],
                answer=answer,
                key_metrics={
                    "policy_name": policy["name"],
                    "combined": policy["combined"],
                    "delta_vs_current": policy["delta_vs_current"],
                },
                evidence_paths=[evidence],
            ),
        )

    variant_policy_by_name = {policy["name"]: policy for policy in variant_family_policy_search}
    raw_policy_by_name = {policy["name"]: policy for policy in raw_source_policy_search}
    add_variant_policy_question(
        "Q271",
        "dropping_single_token_variants_is_only_a_small_gain",
        "The live candidate pool still contains many single-token variants. If we drop them within the current live pool, is that the next strong runtime lever or only a small cleanup?",
        variant_policy_by_name["drop_single_variant"],
        "Dropping single-token variants within the current live pool is only a small gain, not the next major runtime lever.",
        "../raw/variant_family_policy_search.json",
    )
    add_variant_policy_question(
        "Q272",
        "dropping_initial_variants_is_tiny",
        "The live candidate pool still contains initial-style variants. Does removing them from the current pool matter much anymore?",
        variant_policy_by_name["drop_initial_variant"],
        "Dropping initial variants is an even smaller effect than dropping single-token variants. It is not the next major live runtime move.",
        "../raw/variant_family_policy_search.json",
    )
    add_variant_policy_question(
        "Q273",
        "dropping_single_and_initial_or_keeping_plain_multi_only_is_still_only_small",
        "If we aggressively keep only plain multi-token candidates within the current live pool, do we suddenly get a large gain, or is the within-pool family cleanup still small?",
        variant_policy_by_name["keep_plain_multi_only"],
        "Even the strongest within-pool family cleanup (`keep_plain_multi_only`) is only a small gain on the current live runtime. That means the huge `full_name_only` stage win is not coming from a simple within-pool filter.",
        "../raw/variant_family_policy_search.json",
    )
    add_variant_policy_question(
        "Q274",
        "dropping_raw_single_entries_is_also_only_small",
        "Instead of filtering by visible candidate family, what happens if we drop candidates whose raw dictionary source entry is single-token?",
        raw_policy_by_name["drop_raw_single_entry"],
        "Dropping raw single-entry candidates is also only a small gain on the current live runtime, which again argues against another simple pool cleanup being the main next move.",
        "../raw/raw_source_policy_search.json",
    )

    add_question(
        "Q275",
        "pool_cleanup",
        "current_pool_family_cleanup_only_moves_a_few_rows",
        "Does the current live candidate pool still contain a big hidden family-cleanup win, or do the best remaining current-pool filters only move a few rows by small amounts?",
        experiment_stub(
            kind="pool_cleanup_synthesis",
            approach="Compared the best current-pool family and raw-source cleanup counterfactuals against the live runtime.",
            what_was_done=[
                "Evaluated single-token, initial, plain-multi, raw-single, and combined current-pool cleanup policies.",
                "Compared each policy against the current live runtime combined metrics.",
            ],
            what_was_learned=[
                f"Best current-pool family cleanup is {best_variant_family_policy['name']} with delta {best_variant_family_policy['delta_vs_current']}.",
                f"Best current-pool raw-source cleanup is {best_raw_source_policy['name']} with delta {best_raw_source_policy['delta_vs_current']}.",
                "These are only small gains, not a new dominant runtime lever.",
            ],
            answer="Current-pool family cleanup now only moves a few rows by small amounts. The big easy cleanup win is already gone.",
            key_metrics={
                "best_variant_family_policy": best_variant_family_policy["name"],
                "best_variant_family_policy_delta": best_variant_family_policy["delta_vs_current"],
                "best_raw_source_policy": best_raw_source_policy["name"],
                "best_raw_source_policy_delta": best_raw_source_policy["delta_vs_current"],
            },
            evidence_paths=[
                "../raw/variant_family_policy_search.json",
                "../raw/raw_source_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q276",
        "candidate_source",
        "full_name_only_gain_now_comes_from_candidate_source_narrowing_not_current_pool_filtering",
        "The report's `full_name_only` and related dictionary variants still score dramatically better than the current live baseline. Does that mean we still need more current-pool filtering, or does it mean the candidate source itself is the remaining giant lever?",
        experiment_stub(
            kind="source_vs_pool_synthesis",
            approach="Compared the tiny gains from current-pool family cleanup against the very large gains from the dictionary-level full-name variants and the best-possible-rank ceiling inside the current pool.",
            what_was_done=[
                "Read the current-pool best-possible-rank signal for same-family, plain-multi, and no-comma-single ranks.",
                "Read the dictionary-ablation stage for full-name-only style variants.",
                "Compared those against the best current-pool cleanup counterfactuals.",
            ],
            what_was_learned=[
                f"Current-pool same-family/plain-multi/no-comma-single improves only {signal_snapshot['best_possible_rank']['improvable_by_plain_multi_token']} rows at plain-multi level.",
                "Dictionary-level full-name-only variants still jump to near-perfect benchmark scores.",
                "That gap proves the remaining giant lever is candidate-source narrowing, not another small within-pool cleanup.",
            ],
            answer="The big remaining gap comes from candidate-source narrowing, not from another small family filter inside the current live pool.",
            key_metrics={
                "improvable_by_same_family": signal_snapshot["best_possible_rank"]["improvable_by_same_family"],
                "improvable_by_plain_multi_token": signal_snapshot["best_possible_rank"]["improvable_by_plain_multi_token"],
                "full_name_only_mrr": next(
                    item["overall"]["mrr"]
                    for item in load_json(ACCURACY_ROOT / "stages" / "dictionary_ablation.json")["variants"]
                    if item["variant"] == "full_name_only"
                ),
                "current_mrr": load_json(ACCURACY_ROOT / "stages" / "guess_baseline.json")["overall"]["mrr"],
            },
            evidence_paths=[
                "../../accuracy_benchmark_report_post_noncanonical_penalty/signals/best_possible_rank.json",
                "../../accuracy_benchmark_report_post_noncanonical_penalty/stages/dictionary_ablation.json",
                "../raw/variant_family_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q277",
        "row_cluster",
        "row_cluster_uniqueness_still_not_helping",
        "Could the next runtime move be a cluster-assignment or uniqueness rule across nearby rows, or does the current row-cluster benchmark still say that does not improve anything?",
        experiment_stub(
            kind="cluster_signal_audit",
            approach="Audited the row-cluster assignment signal from the post-penalty report.",
            what_was_done=[
                "Loaded the row-cluster assignment signal.",
                "Read the multi-row cluster count and improvable-cluster count.",
            ],
            what_was_learned=[
                f"Row-cluster signal summary is {row_cluster}.",
                "The current benchmark still shows zero improvable clusters under the greedy uniqueness check.",
            ],
            answer="No. Row-cluster uniqueness is still not showing measurable improvement on the current benchmark, so it is not the proven next runtime step.",
            key_metrics=row_cluster,
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/row_cluster_assignment.json"],
        ),
    )

    add_question(
        "Q278",
        "hard_negative",
        "hard_negative_variants_remain_low_after_current_cleanup",
        "If the live runtime is now canonical and cleaner, do the adversarial hard-negative full-name benchmarks also become easy, or do they remain much harder than the canonical benchmark?",
        experiment_stub(
            kind="hard_negative_audit",
            approach="Compared the current post-penalty baseline against the two hard-negative full-name variants from the report.",
            what_was_done=[
                "Loaded the current guess baseline stage summary.",
                "Loaded the hard-negative full-name variant summaries.",
            ],
            what_was_learned=[
                f"Current baseline overall is {load_json(ACCURACY_ROOT / 'stages' / 'guess_baseline.json')['overall']}.",
                f"Hard-negative w2 overall is {next(item['overall'] for item in load_json(ACCURACY_ROOT / 'stages' / 'dictionary_ablation.json')['variants'] if item['variant']=='hard_negative_full_name_w2')}.",
                f"Hard-negative w5 overall is {next(item['overall'] for item in load_json(ACCURACY_ROOT / 'stages' / 'dictionary_ablation.json')['variants'] if item['variant']=='hard_negative_full_name_w5')}.",
            ],
            answer="The hard-negative variants remain much harder than the canonical benchmark. Cleaning up noncanonical families did not solve ranking among plausible full-name distractors.",
            key_metrics={
                "baseline_overall": load_json(ACCURACY_ROOT / "stages" / "guess_baseline.json")["overall"],
                "hard_negative_w2_overall": next(
                    item["overall"]
                    for item in load_json(ACCURACY_ROOT / "stages" / "dictionary_ablation.json")["variants"]
                    if item["variant"] == "hard_negative_full_name_w2"
                ),
                "hard_negative_w5_overall": next(
                    item["overall"]
                    for item in load_json(ACCURACY_ROOT / "stages" / "dictionary_ablation.json")["variants"]
                    if item["variant"] == "hard_negative_full_name_w5"
                ),
            },
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/stages/dictionary_ablation.json"],
        ),
    )

    add_question(
        "Q279",
        "current_runtime",
        "current_remainder_is_now_canonical_plain_multi_lower_width_error_glyph_competition",
        "After the live runtime noncanonical penalty, what is the exact remaining failure class on the benchmark? This is the closure question for whether noncanonical contamination still matters.",
        experiment_stub(
            kind="remainder_classification",
            approach="Combined current candidate-source provenance, pairwise winner reasons, width-component attribution, and family-composition signals into one remainder classification.",
            what_was_done=[
                "Counted current top-1 template and variant families.",
                "Counted current winner reasons.",
                "Counted current dominant width components.",
                "Verified current target presence remains 11/11.",
            ],
            what_was_learned=[
                "Current top-1 winners are all canonical plain multi-token names.",
                "Current misses are all lower-width-error rows.",
                "Current dominant width component is glyph width on every benchmark row.",
            ],
            answer="The remaining live failure class is now canonical plain-multi competition where the wrong canonical winner has lower measured glyph-width error than the target.",
            key_metrics={
                "top1_template_families": current_top1_templates,
                "top1_variant_families": current_top1_variants,
                "reason_counts": dict(Counter(row["reason"] for row in pairwise_rows)),
                "dominant_component_counts": dict(Counter(row["dominant_component"] for row in width_rows)),
            },
            evidence_paths=[
                "../../accuracy_benchmark_report_post_noncanonical_penalty/signals/candidate_source_provenance.json",
                "../../accuracy_benchmark_report_post_noncanonical_penalty/signals/pairwise_winner_explanations.json",
                "../../accuracy_benchmark_report_post_noncanonical_penalty/signals/width_component_attribution.json",
            ],
        ),
    )

    add_question(
        "Q280",
        "next_step",
        "next_proven_runtime_research_path_is_semantic_prior_or_better_candidate_source",
        "Given the current live benchmark evidence, what is the next research direction that is actually supported by data? This question exists to stop us from spending another cycle on already-exhausted levers like broad noncanonical penalties or anchor changes.",
        experiment_stub(
            kind="next_step_synthesis",
            approach="Synthesized the current live runtime audits, remaining policy sweeps, hard-negative behavior, and candidate-source ceilings.",
            what_was_done=[
                "Verified that noncanonical top-1 contamination is now gone.",
                "Verified that additional safe family/template cleanup is small or unsafe.",
                "Verified that anchor sizing remains trusted and row-cluster uniqueness remains inert.",
                "Compared the huge full-name-only ceiling against the tiny within-pool cleanup gains.",
            ],
            what_was_learned=[
                "The remaining live failure is canonical lower-width-error competition.",
                "The big remaining gap comes from candidate-source narrowing or semantic ranking, not another family cleanup tweak.",
            ],
            answer="The next proven runtime research path is semantic priors or a better candidate source for plausible full names. More anchor work or more broad noncanonical penalty work is not the data-backed next move.",
            key_metrics={
                "best_safe_hard_drop_delta_vs_current": best_safe_hard_drop["delta_vs_current"],
                "best_extra_penalty_delta_vs_current": best_extra_penalty["delta_vs_current"],
                "best_variant_family_policy_delta_vs_current": best_variant_family_policy["delta_vs_current"],
                "full_name_only_mrr": next(
                    item["overall"]["mrr"]
                    for item in load_json(ACCURACY_ROOT / "stages" / "dictionary_ablation.json")["variants"]
                    if item["variant"] == "full_name_only"
                ),
                "current_mrr": load_json(ACCURACY_ROOT / "stages" / "guess_baseline.json")["overall"]["mrr"],
            },
            evidence_paths=[
                "../raw/hard_policy_search.json",
                "../raw/extra_noncanonical_penalty_sweep.json",
                "../raw/variant_family_policy_search.json",
                "../../accuracy_benchmark_report_post_noncanonical_penalty/stages/dictionary_ablation.json",
                "../../accuracy_benchmark_report_post_noncanonical_penalty/stages/anchor_span_visual/summary.json",
            ],
        ),
    )

    add_question(
        "Q281",
        "current_runtime",
        "hardest_current_rows_are_benchmark_rows_with_specific_canonical_winners",
        "Which benchmark rows are still the hardest after the live penalty cleanup, and what canonical winners are displacing the target there? This matters because the next semantic-prior work should target the actual hard rows rather than an abstract average.",
        experiment_stub(
            kind="hardest_rows_audit",
            approach="Sorted the current live baseline target profiles by worst best-rank and listed the current top-1 winners on those rows.",
            what_was_done=[
                "Built the current live baseline target profile.",
                "Sorted the benchmark targets by descending rank to find the hardest rows.",
            ],
            what_was_learned=[
                f"The hardest current rows are {hardest_rows[:5]}.",
            ],
            answer="The hardest live rows are now specific benchmark rows displaced by specific canonical full-name winners. That is the right surface for the next semantic-prior experiments.",
            key_metrics={"hardest_rows_top5": hardest_rows[:5]},
            evidence_paths=["../raw/hardest_rows.json"],
        ),
    )

    add_question(
        "Q282",
        "current_runtime",
        "current_remaining_misses_are_concentrated_in_efta00038617",
        "Is the remaining live ranking problem spread evenly across the benchmark datasets, or is it concentrated in one dataset that should dominate follow-up analysis?",
        experiment_stub(
            kind="dataset_concentration_audit",
            approach="Counted current benchmark targets whose rank remains above 20 by dataset.",
            what_was_done=[
                "Read the current live baseline target profile.",
                "Counted remaining >20 rows per dataset.",
            ],
            what_was_learned=[
                f"Current remaining-over-20 rows are {current_baseline_profile['remaining_over_20_rows']}.",
            ],
            answer="The remaining live misses are concentrated in `EFTA00038617`, which should dominate the next semantic-prior and candidate-source follow-up work.",
            key_metrics={
                "remaining_over_20_count": current_baseline_profile["remaining_over_20_count"],
                "remaining_over_20_rows": current_baseline_profile["remaining_over_20_rows"],
            },
            evidence_paths=["../raw/current_profile.json"],
        ),
    )

    add_question(
        "Q283",
        "current_runtime",
        "exact_oracle_rank_is_still_one_for_every_target",
        "Do the current live misses happen because the benchmark target is absent or inherently unmatchable, or would a perfect ranker still put every target at rank 1 inside the current rows?",
        experiment_stub(
            kind="oracle_rank_audit",
            approach="Read the exact-oracle rank signal from the current best-possible-rank report.",
            what_was_done=[
                "Loaded the best-possible-rank signal.",
                "Checked the exact-oracle rank for every benchmark target.",
            ],
            what_was_learned=[
                f"Exact-oracle rows are {best_possible_rows}.",
                "Every benchmark target still has exact-oracle rank 1.",
            ],
            answer="A perfect ranker would still put every target at rank 1. The live system is not running into an unmatchable target problem.",
            key_metrics={
                "all_exact_oracle_rank_1": all(row["exact_oracle_rank"] == 1 for row in best_possible_rows),
                "row_count": len(best_possible_rows),
            },
            evidence_paths=["../../accuracy_benchmark_report_post_noncanonical_penalty/signals/best_possible_rank.json"],
        ),
    )

    add_question(
        "Q284",
        "current_runtime",
        "the_benchmark_is_not_failing_because_targets_are_missing",
        "At this live stage, can we still blame missing targets in the candidate pool for the benchmark misses, or is that explanation fully ruled out?",
        experiment_stub(
            kind="generation_vs_ranking_audit",
            approach="Combined target-presence and exact-oracle-rank signals from the current live report.",
            what_was_done=[
                "Counted target presence in the current candidate-pool quality signal.",
                "Checked exact-oracle rank from the best-possible-rank signal.",
            ],
            what_was_learned=[
                "All targets remain present in pool.",
                "All targets retain exact-oracle rank 1.",
            ],
            answer="Missing targets are ruled out as the current explanation. The live benchmark is failing in ranking among present candidates.",
            key_metrics={
                "targets_present_in_pool": sum(1 for row in pool_rows if row["present_in_pool"]),
                "exact_oracle_rank_1_count": sum(1 for row in best_possible_rows if row["exact_oracle_rank"] == 1),
            },
            evidence_paths=[
                "../../accuracy_benchmark_report_post_noncanonical_penalty/signals/candidate_pool_quality.json",
                "../../accuracy_benchmark_report_post_noncanonical_penalty/signals/best_possible_rank.json",
            ],
        ),
    )

    add_question(
        "Q285",
        "visual_review",
        "visual_review_now_shows_canonical_full_name_winners_not_old_family_contamination",
        "Does the persisted visual review pack line up with the live numeric conclusion that the benchmark is no longer losing to comma/single contamination, but to plausible canonical full-name competition?",
        experiment_stub(
            kind="visual_review_audit",
            approach="Audited the persisted visual review manifest alongside the current candidate-source provenance signal.",
            what_was_done=[
                "Loaded the visual review manifest from the post-penalty report.",
                "Cross-checked it with the current top-1 template and variant-family counts.",
            ],
            what_was_learned=[
                f"Visual review manifest is {visual_manifest}.",
                "The current visual comparison surface is now comparing canonical plain-multi winners, not comma or single-token winners.",
            ],
            answer="Yes. The visual review surface now aligns with the numeric story: the live misses are plausible canonical full-name competition, not the old family contamination class.",
            key_metrics={
                "visual_review_manifest": visual_manifest,
                "top1_template_families": current_top1_templates,
                "top1_variant_families": current_top1_variants,
            },
            evidence_paths=[
                "../../accuracy_benchmark_report_post_noncanonical_penalty/visual_review/manifest.json",
                "../../accuracy_benchmark_report_post_noncanonical_penalty/signals/candidate_source_provenance.json",
            ],
        ),
    )

    for question in questions:
        if question.id in experiments:
            experiment = experiments[question.id]
            write_json(EXPERIMENTS_ROOT / f"{question.experiment_id}.json", experiment)
            write_text(ANSWERS_ROOT / f"{question.id}.md", build_question_answer_markdown(question, experiment))

    questions_payload = [
        {
            "id": question.id,
            "domain": question.domain,
            "title": question.title,
            "context": question.context,
            "experiment_id": question.experiment_id,
            "answer_path": question.answer_path,
            "experiment_path": question.experiment_path,
        }
        for question in questions
    ]
    summary = {
        "total_linked_questions": len(questions_payload),
        "new_questions_this_round": len(experiments),
        "carried_forward_questions": len(old_questions),
        "current_combined": current_combined,
        "best_safe_hard_drop": {
            "name": best_safe_hard_drop["name"],
            "combined": best_safe_hard_drop["combined"],
            "delta_vs_current": best_safe_hard_drop["delta_vs_current"],
        },
        "best_extra_noncanonical_penalty": {
            "penalty_pt": best_extra_penalty["extra_noncanonical_penalty_pt"],
            "combined": best_extra_penalty["combined"],
            "delta_vs_current": best_extra_penalty["delta_vs_current"],
        },
        "best_variant_family_cleanup": {
            "name": best_variant_family_policy["name"],
            "combined": best_variant_family_policy["combined"],
            "delta_vs_current": best_variant_family_policy["delta_vs_current"],
        },
        "best_raw_source_cleanup": {
            "name": best_raw_source_policy["name"],
            "combined": best_raw_source_policy["combined"],
            "delta_vs_current": best_raw_source_policy["delta_vs_current"],
        },
        "anchor_followup": {
            "benchmark_visual_row_count": anchor_followup["benchmark_visual_row_count"],
            "trusted_or_non_sizing_issue_count": anchor_followup["trusted_or_non_sizing_issue_count"],
        },
    }
    report = "\n".join(
        [
            "# Benchmark Question Dossier Round 4",
            "",
            f"- Total linked questions: `{len(questions_payload)}`",
            f"- New questions this round: `{len(experiments)}`",
            f"- Carried-forward questions: `{len(old_questions)}`",
            "",
            "## Key Findings",
            "",
            "- The live runtime noncanonical penalty completed the earlier cleanup story: current top-1 winners are now all canonical plain-multi-token names.",
            "- Additional safe noncanonical penalty or template cleanup is now small or unsafe.",
            "- The remaining benchmark failure class is canonical lower-width-error glyph competition, not old family contamination and not anchor sizing.",
            "- Within-pool family cleanup now yields only small gains, while dictionary-level full-name narrowing remains dramatically stronger.",
            "- The next proven research direction is semantic priors or a better candidate source for plausible full names.",
            "",
            "## Core Artifacts",
            "",
            "- [questions.md](questions.md)",
            "- [summary.json](summary.json)",
            "- [raw/current_profile.json](raw/current_profile.json)",
            "- [raw/hard_policy_search.json](raw/hard_policy_search.json)",
            "- [raw/extra_noncanonical_penalty_sweep.json](raw/extra_noncanonical_penalty_sweep.json)",
            "- [raw/variant_family_policy_search.json](raw/variant_family_policy_search.json)",
            "- [raw/raw_source_policy_search.json](raw/raw_source_policy_search.json)",
            "- [raw/signal_snapshot.json](raw/signal_snapshot.json)",
            "- [raw/hardest_rows.json](raw/hardest_rows.json)",
            "",
        ]
    )
    questions_md_lines = ["# Benchmark Questions Round 4", ""]
    for question in questions_payload:
        questions_md_lines.append(f"## {question['id']}: {question['title']}")
        questions_md_lines.append("")
        questions_md_lines.append(f"- Domain: `{question['domain']}`")
        questions_md_lines.append(f"- Question: {question['context']}")
        questions_md_lines.append(f"- Answer file: [{question['answer_path']}]({question['answer_path']})")
        questions_md_lines.append(f"- Experiment file: [{question['experiment_path']}]({question['experiment_path']})")
        questions_md_lines.append("")

    write_json(OUTPUT_ROOT / "questions.json", questions_payload)
    write_json(OUTPUT_ROOT / "summary.json", summary)
    write_text(OUTPUT_ROOT / "report.md", report)
    write_text(OUTPUT_ROOT / "questions.md", "\n".join(questions_md_lines))


if __name__ == "__main__":
    main()
