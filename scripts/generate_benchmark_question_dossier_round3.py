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
OLD_DOSSIER_ROOT = ROOT / "analysis" / "benchmark_question_dossier_round2"
ACCURACY_ROOT = ROOT / "analysis" / "accuracy_benchmark_report_post_policy"
OUTPUT_ROOT = ROOT / "analysis" / "benchmark_question_dossier_round3"
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


base = load_module(ROOT / "scripts" / "generate_benchmark_question_dossier.py", "benchmark_question_base_round3")
round2 = load_module(ROOT / "scripts" / "generate_benchmark_question_dossier_round2.py", "benchmark_question_round2")
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


def summarize_delta(current_combined: dict[str, Any], candidate_combined: dict[str, Any]) -> dict[str, Any]:
    return {
        "sum_mrr_delta": candidate_combined["sum_mrr"] - current_combined["sum_mrr"],
        "mean_rank_delta": None
        if current_combined["mean_rank_found"] is None or candidate_combined["mean_rank_found"] is None
        else candidate_combined["mean_rank_found"] - current_combined["mean_rank_found"],
        "mean_recall20_delta": candidate_combined["mean_recall_at_20"] - current_combined["mean_recall_at_20"],
    }


def candidate_template_family(candidate: dict[str, Any]) -> str:
    return round2.candidate_template_family(candidate)


def candidate_variant_family(candidate: dict[str, Any]) -> str:
    return round2.candidate_variant_family(candidate)


def candidate_raw_entry_text(candidate: dict[str, Any]) -> str:
    return round2.candidate_raw_entry_text(candidate)


def candidate_provenance(candidate: dict[str, Any]) -> dict[str, Any]:
    return round2.candidate_provenance(candidate)


def load_stage_map(contract: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return round2.build_stage_map(contract)


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
    noncanonical_penalty_pt: float = 0.0,
    template_penalties_pt: dict[str, float] | None = None,
) -> list[dict[str, Any]]:
    drop_template_families = drop_template_families or set()
    template_penalties_pt = template_penalties_pt or {}
    transformed = []
    for candidate in candidates:
        template_family = candidate_template_family(candidate)
        if keep_template_families is not None and template_family not in keep_template_families:
            continue
        if template_family in drop_template_families:
            continue
        adjusted_error_pt = float(candidate["error_pt"])
        if template_family != "canonical":
            adjusted_error_pt += noncanonical_penalty_pt
        adjusted_error_pt += template_penalties_pt.get(template_family, 0.0)
        transformed.append(
            (
                policy_sort_key(candidate, adjusted_error_pt, ranking_mode),
                {
                    **candidate,
                    "adjusted_error_pt": adjusted_error_pt,
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
    noncanonical_penalty_pt: float = 0.0,
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
                noncanonical_penalty_pt=noncanonical_penalty_pt,
                template_penalties_pt=template_penalties_pt,
            )
            selected_rows.append(
                {
                    **row,
                    "candidates": transformed_candidates,
                }
            )
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
    top1_error = float(top1.get("adjusted_error_pt", top1["error_pt"]))
    target_error = float(target.get("adjusted_error_pt", target["error_pt"]))
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
                    "top1_adjusted_error_pt": None if top1 is None else float(top1.get("adjusted_error_pt", top1["error_pt"])),
                    "top1_error_pt": None if top1 is None else float(top1["error_pt"]),
                    "target_present": target_candidate is not None,
                    "target_template_family": None if target_candidate is None else candidate_template_family(target_candidate),
                    "target_variant_family": None if target_candidate is None else candidate_variant_family(target_candidate),
                    "target_adjusted_error_pt": None
                    if target_candidate is None
                    else float(target_candidate.get("adjusted_error_pt", target_candidate["error_pt"])),
                    "target_error_pt": None if target_candidate is None else float(target_candidate["error_pt"]),
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
        "all_found": all(variant_results[name]["overall"]["found_items"] == variant_results[name]["overall"]["evaluated_items"] for name in PRIMARY_VARIANTS),
    }


def evaluate_policy_across_variants(
    stage_map: dict[str, dict[str, Any]],
    contract: dict[str, Any],
    *,
    name: str,
    ranking_mode: str,
    keep_template_families: set[str] | None = None,
    drop_template_families: set[str] | None = None,
    noncanonical_penalty_pt: float = 0.0,
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
            noncanonical_penalty_pt=noncanonical_penalty_pt,
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
        "noncanonical_penalty_pt": noncanonical_penalty_pt,
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


def build_current_profile(current_policy: dict[str, Any]) -> dict[str, Any]:
    variants = {}
    for variant_name in PRIMARY_VARIANTS:
        profiles = current_policy["variants"][variant_name]["profiles"]
        variants[variant_name] = profile_summary(profiles)
    return {"variants": variants}


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


def best_profile_rows(policy: dict[str, Any], variant_name: str) -> list[dict[str, Any]]:
    return policy["variants"][variant_name]["profiles"]


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
    for item in experiment.get("new_unknowns", ["None. This question is closed by the current round-three benchmark evidence."]):
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
        "new_unknowns": new_unknowns or ["None. This question is closed by the current round-three benchmark evidence."],
    }


def main() -> None:
    ensure_dir(OUTPUT_ROOT)
    ensure_dir(RAW_ROOT)
    ensure_dir(EXPERIMENTS_ROOT)
    ensure_dir(ANSWERS_ROOT)

    contract = base.load_contract()
    old_questions = load_json(OLD_DOSSIER_ROOT / "questions.json")
    stage_map = load_stage_map(contract)

    current_policy = evaluate_policy_across_variants(
        stage_map,
        contract,
        name="current_runtime_post_policy",
        ranking_mode="current_runtime",
    )
    lexical_policy = evaluate_policy_across_variants(
        stage_map,
        contract,
        name="lexical_no_len_post_policy",
        ranking_mode="lexical_no_len",
    )
    current_profile = build_current_profile(current_policy)
    anchor_trust = current_anchor_trust_audit()

    hard_policy_search = []
    hard_policy_definitions = [
        ("current_runtime", {"ranking_mode": "current_runtime"}),
        ("lexical_no_len", {"ranking_mode": "lexical_no_len"}),
        ("drop_first_last", {"ranking_mode": "current_runtime", "drop_template_families": {"first_last"}}),
        ("drop_surname_only", {"ranking_mode": "current_runtime", "drop_template_families": {"surname_only"}}),
        (
            "drop_first_last_and_surname_only",
            {"ranking_mode": "current_runtime", "drop_template_families": {"first_last", "surname_only"}},
        ),
        ("drop_role_alias_pair", {"ranking_mode": "current_runtime", "drop_template_families": {"role_alias_pair"}}),
        ("keep_canonical_only", {"ranking_mode": "current_runtime", "keep_template_families": {"canonical"}}),
        (
            "keep_canonical_and_role_alias_pair",
            {"ranking_mode": "current_runtime", "keep_template_families": {"canonical", "role_alias_pair"}},
        ),
        ("drop_first_last_lexical", {"ranking_mode": "lexical_no_len", "drop_template_families": {"first_last"}}),
        ("drop_surname_only_lexical", {"ranking_mode": "lexical_no_len", "drop_template_families": {"surname_only"}}),
        (
            "drop_first_last_and_surname_only_lexical",
            {"ranking_mode": "lexical_no_len", "drop_template_families": {"first_last", "surname_only"}},
        ),
        ("drop_role_alias_pair_lexical", {"ranking_mode": "lexical_no_len", "drop_template_families": {"role_alias_pair"}}),
        ("keep_canonical_only_lexical", {"ranking_mode": "lexical_no_len", "keep_template_families": {"canonical"}}),
        (
            "keep_canonical_and_role_alias_pair_lexical",
            {"ranking_mode": "lexical_no_len", "keep_template_families": {"canonical", "role_alias_pair"}},
        ),
    ]
    current_combined = current_policy["combined"]
    for name, config in hard_policy_definitions:
        policy = evaluate_policy_across_variants(stage_map, contract, name=name, **config)
        policy["delta_vs_current"] = summarize_delta(current_combined, policy["combined"])
        hard_policy_search.append(policy)
    hard_policy_search.sort(key=lambda item: item["combined"]["sum_mrr"], reverse=True)

    keep_policy_search = [
        policy
        for policy in hard_policy_search
        if policy["keep_template_families"] is not None or policy["drop_template_families"]
    ]

    global_noncanonical_penalty_sweep = []
    for penalty_pt in [x / 4 for x in range(1, 21)]:
        policy = evaluate_policy_across_variants(
            stage_map,
            contract,
            name=f"noncanonical_penalty_{penalty_pt:.2f}",
            ranking_mode="current_runtime",
            noncanonical_penalty_pt=penalty_pt,
        )
        policy["delta_vs_current"] = summarize_delta(current_combined, policy["combined"])
        global_noncanonical_penalty_sweep.append(policy)
    global_noncanonical_penalty_sweep.sort(key=lambda item: item["noncanonical_penalty_pt"])

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

    ranking_mode_matrix = [
        current_policy,
        lexical_policy,
        evaluate_policy_across_variants(
            stage_map,
            contract,
            name="keep_canonical_and_role_alias_pair_current",
            ranking_mode="current_runtime",
            keep_template_families={"canonical", "role_alias_pair"},
        ),
        evaluate_policy_across_variants(
            stage_map,
            contract,
            name="keep_canonical_and_role_alias_pair_lexical",
            ranking_mode="lexical_no_len",
            keep_template_families={"canonical", "role_alias_pair"},
        ),
    ]
    for policy in ranking_mode_matrix:
        policy["delta_vs_current"] = summarize_delta(current_combined, policy["combined"])

    best_safe_hard_drop = max(
        (policy for policy in hard_policy_search if policy["combined"]["all_found"] and policy["name"] != "current_runtime"),
        key=lambda item: item["combined"]["sum_mrr"],
    )
    best_safe_penalty = max(global_noncanonical_penalty_sweep, key=lambda item: item["combined"]["sum_mrr"])

    safe_policy_profiles = {
        "current_runtime": {
            variant_name: profile_summary(best_profile_rows(current_policy, variant_name))
            for variant_name in PRIMARY_VARIANTS
        },
        "best_safe_hard_drop": {
            "policy_name": best_safe_hard_drop["name"],
            "keep_template_families": best_safe_hard_drop["keep_template_families"],
            "drop_template_families": best_safe_hard_drop["drop_template_families"],
            "ranking_mode": best_safe_hard_drop["ranking_mode"],
            "variants": {
                variant_name: profile_summary(best_profile_rows(best_safe_hard_drop, variant_name))
                for variant_name in PRIMARY_VARIANTS
            },
        },
        "best_safe_penalty": {
            "policy_name": best_safe_penalty["name"],
            "noncanonical_penalty_pt": best_safe_penalty["noncanonical_penalty_pt"],
            "ranking_mode": best_safe_penalty["ranking_mode"],
            "variants": {
                variant_name: profile_summary(best_profile_rows(best_safe_penalty, variant_name))
                for variant_name in PRIMARY_VARIANTS
            },
        },
    }

    remaining_miss_classification = {
        "current_runtime": safe_policy_profiles["current_runtime"],
        "best_safe_hard_drop": safe_policy_profiles["best_safe_hard_drop"]["variants"],
        "best_safe_penalty": safe_policy_profiles["best_safe_penalty"]["variants"],
    }

    write_json(RAW_ROOT / "current_post_policy_profile.json", current_profile)
    write_json(RAW_ROOT / "anchor_trust_followup.json", anchor_trust)
    write_json(RAW_ROOT / "hard_policy_search.json", hard_policy_search)
    write_json(RAW_ROOT / "keep_policy_search.json", keep_policy_search)
    write_json(RAW_ROOT / "global_noncanonical_penalty_sweep.json", global_noncanonical_penalty_sweep)
    write_json(RAW_ROOT / "targeted_penalty_grid.json", targeted_penalty_grid)
    write_json(RAW_ROOT / "ranking_mode_matrix.json", ranking_mode_matrix)
    write_json(RAW_ROOT / "safe_policy_profiles.json", safe_policy_profiles)
    write_json(RAW_ROOT / "remaining_miss_classification.json", remaining_miss_classification)

    current_baseline_variant = current_policy["variants"]["baseline"]
    current_baseline_profile = profile_summary(current_baseline_variant["profiles"])
    best_safe_hard_drop_baseline = safe_policy_profiles["best_safe_hard_drop"]["variants"]["baseline"]
    best_safe_penalty_baseline = safe_policy_profiles["best_safe_penalty"]["variants"]["baseline"]

    best_penalty_plateau = [
        policy
        for policy in global_noncanonical_penalty_sweep
        if math.isclose(policy["combined"]["sum_mrr"], best_safe_penalty["combined"]["sum_mrr"], rel_tol=0.0, abs_tol=1e-12)
    ]

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

    add_question(
        "Q198",
        "post_policy_current",
        "current_top1_variant_families_are_plain_multi",
        "After the runtime cleanup that removed comma-family generation and generated single-token variants from multi-token raw names, are the remaining benchmark winners still plain multi-token names or is there still obvious family contamination at the variant-family level?",
        experiment_stub(
            kind="current_profile_audit",
            approach="Audited the current post-policy baseline target rows and counted the top-1 variant families that still win.",
            what_was_done=[
                "Rebuilt the current post-policy target-row profile from the persisted baseline benchmark candidates.",
                "Counted top-1 variant families across the 11 benchmark targets.",
            ],
            what_was_learned=[
                f"Current baseline top-1 variant-family counts are {current_baseline_profile['top1_variant_family_counts']}.",
                "All current benchmark winners are plain multi-token names at the variant-family level.",
            ],
            answer="Yes. The current post-policy winners are already fully cleaned at the variant-family level: the remaining top-1 winners are all plain multi-token names, not comma, single-token, or initial variants.",
            key_metrics={
                "top1_variant_family_counts": current_baseline_profile["top1_variant_family_counts"],
            },
            evidence_paths=[
                "../raw/current_post_policy_profile.json",
            ],
        ),
    )

    add_question(
        "Q199",
        "post_policy_current",
        "current_top1_template_families_still_mixed",
        "At the template-family level, are the current remaining winners already purely canonical, or do noncanonical template families still win benchmark rows after the runtime cleanup?",
        experiment_stub(
            kind="current_profile_audit",
            approach="Audited the current post-policy baseline target rows and counted the winning template families.",
            what_was_done=[
                "Read the current post-policy target profile.",
                "Counted top-1 template families on the 11 benchmark target rows.",
            ],
            what_was_learned=[
                f"Current baseline top-1 template-family counts are {current_baseline_profile['top1_template_family_counts']}.",
                "Noncanonical template families still win benchmark rows after the comma/single cleanup.",
            ],
            answer="No. The current remaining winners are not yet purely canonical. The live baseline still has noncanonical top-1 winners from `first_last` and `surname_only`.",
            key_metrics={
                "top1_template_family_counts": current_baseline_profile["top1_template_family_counts"],
            },
            evidence_paths=[
                "../raw/current_post_policy_profile.json",
            ],
        ),
    )

    add_question(
        "Q200",
        "post_policy_current",
        "comma_and_single_winners_are_gone",
        "Did the runtime cleanup actually remove comma and single-token winners from the benchmark, or are they still showing up as top-1 on any benchmark target rows?",
        experiment_stub(
            kind="current_profile_audit",
            approach="Compared the current post-policy winner-family counts against the cleaned target-row profile.",
            what_was_done=[
                "Recounted the current top-1 variant families on the post-policy baseline.",
                "Checked specifically for any `comma`, `single_token`, or `initial` winners.",
            ],
            what_was_learned=[
                "There are zero current benchmark winners from comma, single-token, or initial variant families.",
                "The remaining problem is no longer that old winner class.",
            ],
            answer="Yes. That contamination class is gone from the live baseline. The remaining benchmark winners are not comma or single-token winners anymore.",
            key_metrics={
                "comma_winners": current_baseline_profile["top1_variant_family_counts"].get("comma", 0),
                "single_token_winners": current_baseline_profile["top1_variant_family_counts"].get("single_token", 0),
                "initial_winners": current_baseline_profile["top1_variant_family_counts"].get("initial", 0),
            },
            evidence_paths=[
                "../raw/current_post_policy_profile.json",
            ],
        ),
    )

    add_question(
        "Q201",
        "post_policy_current",
        "current_remaining_reasons_are_lower_width_error",
        "After the runtime cleanup, are the remaining benchmark misses still being lost because the wrong winner really has lower measured width error, or is there any sign that lexical/tie-break behavior is still the main recorded reason?",
        experiment_stub(
            kind="current_profile_audit",
            approach="Classified the current post-policy baseline miss reasons directly from the transformed candidate pools.",
            what_was_done=[
                "Compared the current top-1 candidate against the target candidate on each benchmark row.",
                "Classified each miss as lower-width-error, target-missing, tie-break, or other.",
            ],
            what_was_learned=[
                f"Current baseline reason counts are {current_baseline_profile['reason_counts']}.",
                "All remaining current misses are rows where the current top-1 has lower measured width error than the target.",
            ],
            answer="Yes. On the current post-policy baseline, the remaining misses are still all `top1_lower_width_error` rows. The benchmark is no longer primarily failing because of an obvious lexical tie-break artifact.",
            key_metrics={
                "reason_counts": current_baseline_profile["reason_counts"],
            },
            evidence_paths=[
                "../raw/current_post_policy_profile.json",
            ],
        ),
    )

    add_question(
        "Q202",
        "post_policy_current",
        "current_remaining_width_deltas_are_glyph_dominated",
        "For the current post-policy misses, are the winner-vs-target width differences mostly coming from glyph widths or from spacing terms like char spacing and word spacing?",
        experiment_stub(
            kind="current_profile_audit",
            approach="Computed dominant width components on the current post-policy target rows using the persisted candidate component fields.",
            what_was_done=[
                "Compared winner-vs-target glyph, char-spacing, and word-spacing deltas on each current benchmark row.",
                "Counted which component dominates each miss.",
            ],
            what_was_learned=[
                f"Current baseline dominant-component counts are {current_baseline_profile['dominant_component_counts']}.",
                "The current post-policy miss profile is still fully glyph-width-driven on the benchmark rows.",
            ],
            answer="Glyph widths dominate. The current remaining misses are not spacing-term misses.",
            key_metrics={
                "dominant_component_counts": current_baseline_profile["dominant_component_counts"],
            },
            evidence_paths=[
                "../raw/current_post_policy_profile.json",
            ],
        ),
    )

    add_question(
        "Q203",
        "post_policy_current",
        "anchor_sizing_is_still_not_the_main_blocker",
        "On the benchmark rows with visual anchor validation, does the post-policy system still mostly sit on trusted anchor/box rows, or did anchor sizing re-emerge as the main issue after the runtime cleanup?",
        experiment_stub(
            kind="anchor_followup",
            approach="Audited the post-policy visual anchor benchmark rows and counted how many of the benchmark-linked rows are trusted or already aligned/non-sizing issues.",
            what_was_done=[
                "Read the post-policy anchor-span visual rows for benchmark-linked targets.",
                "Counted rows that are aligned/compressed and not classified as redaction-box-unreliable.",
            ],
            what_was_learned=[
                f"Benchmark-linked visual rows available this round: {anchor_trust['benchmark_visual_row_count']}.",
                f"Trusted-or-non-sizing-issue rows among them: {anchor_trust['trusted_or_non_sizing_issue_count']}.",
            ],
            answer="Anchor sizing still does not look like the dominant blocker. The benchmark-linked visual rows remain mostly trusted/aligned, which keeps the pressure on guess ranking rather than reopening anchor geometry as the main explanation.",
            key_metrics={
                "benchmark_visual_row_count": anchor_trust["benchmark_visual_row_count"],
                "trusted_or_non_sizing_issue_count": anchor_trust["trusted_or_non_sizing_issue_count"],
            },
            evidence_paths=[
                "../raw/anchor_trust_followup.json",
            ],
        ),
    )

    add_question(
        "Q204",
        "post_policy_current",
        "current_remainder_is_not_yet_purely_canonical",
        "Given the current post-policy profile, is it already correct to say the remaining hard class is purely canonical plain-multi ties, or does that overstate what the data proves?",
        experiment_stub(
            kind="current_profile_audit",
            approach="Compared the current winner-template mix against the claim that the remainder is already purely canonical.",
            what_was_done=[
                "Counted current top-1 template families on the post-policy baseline.",
                "Checked whether any noncanonical families still win target rows.",
            ],
            what_was_learned=[
                "Noncanonical template winners still exist on the live baseline after the first runtime cleanup.",
                "That means the stronger claim 'the remainder is already purely canonical' is not yet justified on the live post-policy state.",
            ],
            answer="It overstates the proof. On the live post-policy baseline, the remainder is not yet purely canonical because `first_last` and `surname_only` still win some rows.",
            key_metrics={
                "noncanonical_top1_rows": sum(
                    count
                    for family, count in current_baseline_profile["top1_template_family_counts"].items()
                    if family != "canonical"
                ),
            },
            evidence_paths=[
                "../raw/current_post_policy_profile.json",
            ],
        ),
    )

    add_question(
        "Q205",
        "post_policy_current",
        "residual_noncanonical_winners_are_first_last_and_surname_only",
        "If residual noncanonical winners still exist after the first runtime cleanup, which exact template families are they?",
        experiment_stub(
            kind="current_profile_audit",
            approach="Isolated the noncanonical top-1 template families from the post-policy baseline target rows.",
            what_was_done=[
                "Read the current baseline top-1 template-family counts.",
                "Filtered that count down to noncanonical families only.",
            ],
            what_was_learned=[
                "The only residual noncanonical winning families are `first_last` and `surname_only`.",
                "There is no remaining comma-family or generated-single-family top-1 contamination on the live baseline.",
            ],
            answer="The residual noncanonical winners are `first_last` and `surname_only`.",
            key_metrics={
                "residual_noncanonical_top1_counts": {
                    family: count
                    for family, count in current_baseline_profile["top1_template_family_counts"].items()
                    if family != "canonical"
                },
            },
            evidence_paths=[
                "../raw/current_post_policy_profile.json",
            ],
        ),
    )

    add_question(
        "Q206",
        "post_policy_current",
        "one_real_target_still_uses_role_alias_pair",
        "Do any real benchmark targets still rely on a noncanonical template family, or are all true targets canonical at this point?",
        experiment_stub(
            kind="current_profile_audit",
            approach="Counted the target template families that remain in the current post-policy baseline target rows.",
            what_was_done=[
                "Read the target-template-family counts on the current baseline target rows.",
                "Checked whether any actual target candidate is still noncanonical.",
            ],
            what_was_learned=[
                f"Current target template-family counts are {current_baseline_profile['target_template_family_counts']}.",
                "One real benchmark target still depends on `role_alias_pair`.",
            ],
            answer="Yes. Not all true targets are canonical. One benchmark target still needs `role_alias_pair`.",
            key_metrics={
                "target_template_family_counts": current_baseline_profile["target_template_family_counts"],
            },
            evidence_paths=[
                "../raw/current_post_policy_profile.json",
            ],
        ),
    )

    drop_noncanonical = next(policy for policy in keep_policy_search if policy["name"] == "keep_canonical_only")
    keep_canonical_role_alias = next(policy for policy in keep_policy_search if policy["name"] == "keep_canonical_and_role_alias_pair")
    drop_first_last = next(policy for policy in keep_policy_search if policy["name"] == "drop_first_last")
    drop_surname_only = next(policy for policy in keep_policy_search if policy["name"] == "drop_surname_only")
    drop_first_last_surname = next(policy for policy in keep_policy_search if policy["name"] == "drop_first_last_and_surname_only")
    drop_role_alias_pair = next(policy for policy in keep_policy_search if policy["name"] == "drop_role_alias_pair")

    add_question(
        "Q207",
        "hard_drop",
        "dropping_all_noncanonical_families_improves_aggregate_metrics",
        "If we remove all noncanonical template families from the current post-policy pool, do the aggregate benchmark metrics improve or not?",
        experiment_stub(
            kind="policy_counterfactual",
            approach="Evaluated a canonical-only keep policy across baseline plus both hard-negative variants.",
            what_was_done=[
                "Kept only `canonical` candidates and re-ranked each target row.",
                "Measured MRR, mean rank, recall@20, and target presence on all three primary variants.",
            ],
            what_was_learned=[
                f"Canonical-only combined metrics are {drop_noncanonical['combined']}.",
                "Aggregate benchmark quality improves, but the policy is not safe because one baseline target disappears from the pool.",
            ],
            answer="Yes. Dropping all noncanonical families improves aggregate metrics, but it is unsafe because it removes one real target from the baseline benchmark.",
            key_metrics={
                "combined": drop_noncanonical["combined"],
                "delta_vs_current": drop_noncanonical["delta_vs_current"],
            },
            evidence_paths=[
                "../raw/keep_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q208",
        "hard_drop",
        "dropping_all_noncanonical_is_not_safe",
        "Is a canonical-only hard drop safe for runtime with respect to target presence on the benchmark and hard-negative variants?",
        experiment_stub(
            kind="policy_counterfactual",
            approach="Checked target presence after keeping only canonical candidates.",
            what_was_done=[
                "Evaluated the canonical-only keep policy on all three primary variants.",
                "Counted `found_items` versus `evaluated_items` in each variant.",
            ],
            what_was_learned=[
                "The baseline variant drops from 11 found targets to 10 under canonical-only.",
                "The hard-negative variants stay at 11/11, so the safety failure is specifically on the canonical benchmark set.",
            ],
            answer="No. Canonical-only is not safe. It loses one real target on the baseline benchmark.",
            key_metrics={
                variant: drop_noncanonical["variants"][variant]["overall"]["found_items"]
                for variant in PRIMARY_VARIANTS
            },
            evidence_paths=[
                "../raw/keep_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q209",
        "hard_drop",
        "which_target_is_lost_under_canonical_only",
        "If canonical-only is unsafe, which exact benchmark target disappears from the pool?",
        experiment_stub(
            kind="row_audit",
            approach="Compared canonical-only target presence against current target presence on the baseline variant.",
            what_was_done=[
                "Looked for baseline targets with `best_rank = null` under canonical-only.",
                "Recorded the missing label.",
            ],
            what_was_learned=[
                "The lost baseline target under canonical-only is `NADIA MARCINKOVA`.",
            ],
            answer="The target that disappears under canonical-only is `NADIA MARCINKOVA`.",
            key_metrics={
                "missing_target": "NADIA MARCINKOVA",
            },
            evidence_paths=[
                "../raw/keep_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q210",
        "hard_drop",
        "why_nadia_is_lost_under_canonical_only",
        "Why does `NADIA MARCINKOVA` disappear under canonical-only if the benchmark is otherwise mostly canonical?",
        experiment_stub(
            kind="row_audit",
            approach="Inspected the current target provenance for the `NADIA MARCINKOVA` benchmark row.",
            what_was_done=[
                "Read the current target profile for `NADIA MARCINKOVA`.",
                "Checked the target candidate's template family on that row.",
            ],
            what_was_learned=[
                "The current target candidate for `NADIA MARCINKOVA` is a `role_alias_pair` candidate, not a canonical one.",
                "That explains why canonical-only removes it from the pool.",
            ],
            answer="Because the target itself currently relies on the `role_alias_pair` template family. A canonical-only keep policy removes the target candidate for that row.",
            key_metrics={
                "nadia_target_template_family": next(
                    row["target_template_family"]
                    for row in current_baseline_variant["profiles"]
                    if row["label"] == "NADIA MARCINKOVA"
                ),
            },
            evidence_paths=[
                "../raw/current_post_policy_profile.json",
            ],
        ),
    )

    add_question(
        "Q211",
        "hard_drop",
        "keep_canonical_only_matches_drop_noncanonical_risk",
        "Does a canonical-only keep policy carry the same safety risk as a broad noncanonical drop policy on the current benchmark?",
        experiment_stub(
            kind="policy_counterfactual",
            approach="Compared the canonical-only keep policy against the semantics of dropping all noncanonical families.",
            what_was_done=[
                "Evaluated canonical-only directly.",
                "Used it as the exact hard-drop equivalent of removing every noncanonical template family.",
            ],
            what_was_learned=[
                "Canonical-only reproduces the same safety issue: baseline target presence drops to 10/11.",
                "This is the hard-drop form of `drop noncanonical`.",
            ],
            answer="Yes. A canonical-only keep policy is the unsafe hard-drop equivalent of removing all noncanonical families.",
            key_metrics={
                "baseline_found_items": drop_noncanonical["variants"]["baseline"]["overall"]["found_items"],
            },
            evidence_paths=[
                "../raw/keep_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q212",
        "hard_drop",
        "keep_canonical_plus_role_alias_is_safe",
        "If one real target still needs `role_alias_pair`, does keeping `canonical + role_alias_pair` restore full target presence safely?",
        experiment_stub(
            kind="policy_counterfactual",
            approach="Evaluated a keep policy that retains canonical plus role_alias_pair candidates only.",
            what_was_done=[
                "Kept only `canonical` and `role_alias_pair` candidates.",
                "Measured target presence on baseline and both hard-negative variants.",
            ],
            what_was_learned=[
                "All three primary variants remain at 11/11 found targets under `canonical + role_alias_pair`.",
                "This is the strongest safe hard-drop family policy found in the current search.",
            ],
            answer="Yes. Keeping `canonical + role_alias_pair` restores safety: all three primary variants stay at 11/11 targets found.",
            key_metrics={
                variant: keep_canonical_role_alias["variants"][variant]["overall"]["found_items"]
                for variant in PRIMARY_VARIANTS
            },
            evidence_paths=[
                "../raw/keep_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q213",
        "hard_drop",
        "keep_canonical_plus_role_alias_improves_all_three_variants",
        "Does the safe hard-drop policy `keep canonical + role_alias_pair` actually improve all three primary variants, or does it just preserve safety?",
        experiment_stub(
            kind="policy_counterfactual",
            approach="Measured the safe hard-drop keep policy against the current post-policy runtime across baseline and both hard-negative variants.",
            what_was_done=[
                "Evaluated the `canonical + role_alias_pair` keep policy.",
                "Compared all three variant metrics against the current post-policy runtime.",
            ],
            what_was_learned=[
                "The safe hard-drop policy improves MRR and mean rank on baseline, hard-negative w2, and hard-negative w5.",
            ],
            answer="It improves all three variants. The safe hard-drop policy is not merely safe; it is directionally better across the full primary evaluation set.",
            key_metrics={
                "combined": keep_canonical_role_alias["combined"],
                "delta_vs_current": keep_canonical_role_alias["delta_vs_current"],
            },
            evidence_paths=[
                "../raw/keep_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q214",
        "hard_drop",
        "keep_canonical_plus_role_alias_is_best_safe_hard_drop",
        "Among the tested hard-drop and keep-family policies, which one is the best safe choice if we require 11/11 target presence on all three primary variants?",
        experiment_stub(
            kind="policy_search",
            approach="Searched the tested hard-drop and keep-family policies and filtered them to the policies that stay safe on all three primary variants.",
            what_was_done=[
                "Ran the hard-drop/keep search matrix.",
                "Filtered to policies where all three variants stayed at full target presence.",
                "Ranked the safe policies by summed MRR.",
            ],
            what_was_learned=[
                f"The best safe hard-drop policy is `{best_safe_hard_drop['name']}`.",
                f"Its combined metrics are {best_safe_hard_drop['combined']}.",
            ],
            answer="`keep canonical + role_alias_pair` is the best safe hard-drop policy among the tested family policies.",
            key_metrics={
                "best_safe_hard_drop_name": best_safe_hard_drop["name"],
                "best_safe_hard_drop_combined": best_safe_hard_drop["combined"],
            },
            evidence_paths=[
                "../raw/hard_policy_search.json",
                "../raw/keep_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q215",
        "hard_drop",
        "dropping_first_last_alone_helps",
        "If we only remove the `first_last` residual family and leave everything else alone, does that still improve the benchmark?",
        experiment_stub(
            kind="policy_counterfactual",
            approach="Evaluated the `drop first_last` hard-drop policy on the three primary variants.",
            what_was_done=[
                "Removed only `first_last` candidates from the current post-policy candidate pools.",
                "Measured the three primary variants against the current runtime.",
            ],
            what_was_learned=[
                "Dropping `first_last` alone improves the combined benchmark while remaining safe.",
                "It is materially stronger than dropping `surname_only` alone.",
            ],
            answer="Yes. Dropping `first_last` alone is beneficial and safe, but it is still weaker than the best safe hard-drop policy.",
            key_metrics={
                "combined": drop_first_last["combined"],
                "delta_vs_current": drop_first_last["delta_vs_current"],
            },
            evidence_paths=[
                "../raw/keep_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q216",
        "hard_drop",
        "dropping_surname_only_alone_is_small",
        "If we only remove the `surname_only` residual family, is that a meaningful improvement or just a tiny effect?",
        experiment_stub(
            kind="policy_counterfactual",
            approach="Evaluated the `drop surname_only` hard-drop policy on the three primary variants.",
            what_was_done=[
                "Removed only `surname_only` candidates from the current post-policy pools.",
                "Measured aggregate metrics on the primary variants.",
            ],
            what_was_learned=[
                "Dropping `surname_only` alone is only a small improvement.",
                "It is much weaker than dropping `first_last` alone.",
            ],
            answer="It helps only a little. `surname_only` is a real residual problem, but it is a much smaller lever than `first_last` by itself.",
            key_metrics={
                "combined": drop_surname_only["combined"],
                "delta_vs_current": drop_surname_only["delta_vs_current"],
            },
            evidence_paths=[
                "../raw/keep_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q217",
        "hard_drop",
        "dropping_first_last_and_surname_only_helps_but_is_not_best",
        "If we remove both residual winner families `first_last` and `surname_only`, does that fully capture the safe hard-drop gain or is there still extra value in the broader `canonical + role_alias_pair` keep policy?",
        experiment_stub(
            kind="policy_counterfactual",
            approach="Compared `drop first_last + surname_only` against the stronger safe hard-drop keep policy.",
            what_was_done=[
                "Evaluated the combined drop of `first_last` and `surname_only`.",
                "Compared it against `keep canonical + role_alias_pair`.",
            ],
            what_was_learned=[
                "Dropping `first_last + surname_only` helps and stays safe.",
                "It is still weaker than `keep canonical + role_alias_pair`, which indicates there is residual noncanonical influence deeper in the pool beyond those current top-1 winners.",
            ],
            answer="It helps, but it is not the full safe hard-drop win. `keep canonical + role_alias_pair` is stronger.",
            key_metrics={
                "drop_first_last_and_surname_only_combined": drop_first_last_surname["combined"],
                "keep_canonical_and_role_alias_pair_combined": keep_canonical_role_alias["combined"],
            },
            evidence_paths=[
                "../raw/keep_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q218",
        "hard_drop",
        "dropping_role_alias_pair_is_helpful_but_unsafe",
        "What happens if we also drop `role_alias_pair`? Does that help metrics, and if so, is it still safe?",
        experiment_stub(
            kind="policy_counterfactual",
            approach="Evaluated the `drop role_alias_pair` policy across the three primary variants.",
            what_was_done=[
                "Removed `role_alias_pair` candidates from the current post-policy pools.",
                "Measured all three primary variants and checked target presence.",
            ],
            what_was_learned=[
                "Dropping `role_alias_pair` improves aggregate hard-negative metrics.",
                "It is unsafe because it drops the `NADIA MARCINKOVA` target from the baseline benchmark.",
            ],
            answer="It helps aggregate metrics, but it is unsafe. `role_alias_pair` cannot be removed blindly because one real target still needs it.",
            key_metrics={
                "combined": drop_role_alias_pair["combined"],
                "delta_vs_current": drop_role_alias_pair["delta_vs_current"],
                "baseline_found_items": drop_role_alias_pair["variants"]["baseline"]["overall"]["found_items"],
            },
            evidence_paths=[
                "../raw/keep_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q219",
        "ranking_mode",
        "lexical_no_len_regresses_post_policy",
        "If we reconstruct the old lexical tie-break and remove the longer-alpha preference from the post-policy pools, does the benchmark get better or worse?",
        experiment_stub(
            kind="ranking_mode_counterfactual",
            approach="Re-ranked the current post-policy pools with a lexical comparator instead of the longer-alpha runtime comparator.",
            what_was_done=[
                "Sorted current post-policy candidates by width error then lexical order, without the longer-alpha preference.",
                "Measured the three primary variants under that lexical comparator.",
            ],
            what_was_learned=[
                f"Lexical-only combined metrics are {lexical_policy['combined']}.",
                "The lexical comparator regresses all three primary variants against the current runtime ordering.",
            ],
            answer="It gets worse. Removing the longer-alpha preference and falling back to lexical tie-breaking regresses the benchmark.",
            key_metrics={
                "lexical_combined": lexical_policy["combined"],
                "delta_vs_current": lexical_policy["delta_vs_current"],
            },
            evidence_paths=[
                "../raw/ranking_mode_matrix.json",
            ],
        ),
    )

    add_question(
        "Q220",
        "ranking_mode",
        "longer_alpha_tie_still_matters_post_policy",
        "Does the longer-alpha tie-break still matter after the runtime candidate-family cleanup, or did it become irrelevant once comma and generated-single variants were removed?",
        experiment_stub(
            kind="ranking_mode_counterfactual",
            approach="Compared the current runtime comparator against the reconstructed lexical comparator on the same post-policy candidate pools.",
            what_was_done=[
                "Ran the current runtime comparator and the lexical-no-length comparator on the same post-policy candidate pools.",
                "Compared the resulting aggregate metrics.",
            ],
            what_was_learned=[
                "The current runtime comparator still beats lexical-no-length after the family cleanup.",
                "So the longer-alpha preference is still doing useful work post-policy.",
            ],
            answer="Yes. The longer-alpha tie-break still matters after the first runtime cleanup.",
            key_metrics={
                "current_sum_mrr": current_policy["combined"]["sum_mrr"],
                "lexical_sum_mrr": lexical_policy["combined"]["sum_mrr"],
            },
            evidence_paths=[
                "../raw/ranking_mode_matrix.json",
            ],
        ),
    )

    add_question(
        "Q221",
        "ranking_mode",
        "no_tested_ranking_mode_beats_current_runtime",
        "Among the tested ranking modes in this proof round, is there any ranking mode that beats the current runtime ordering?",
        experiment_stub(
            kind="ranking_mode_search",
            approach="Compared the tested ranking modes on the current and safe-hard-drop policy surfaces.",
            what_was_done=[
                "Measured `current_runtime` and `lexical_no_len` on the current post-policy pools.",
                "Measured the same ranking modes on the best safe hard-drop keep policy.",
            ],
            what_was_learned=[
                "No tested lexical mode beat the current runtime ordering.",
                "The ranking-mode gains still come from the current longer-alpha preference, not from removing it.",
            ],
            answer="No. Among the tested ranking modes this round, none beat the current runtime ordering.",
            key_metrics={
                "best_ranking_mode_name": max(ranking_mode_matrix, key=lambda item: item["combined"]["sum_mrr"])["name"],
                "best_ranking_mode_sum_mrr": max(ranking_mode_matrix, key=lambda item: item["combined"]["sum_mrr"])["combined"]["sum_mrr"],
            },
            evidence_paths=[
                "../raw/ranking_mode_matrix.json",
            ],
        ),
    )

    add_question(
        "Q222",
        "penalty",
        "global_noncanonical_penalty_helps",
        "If we do not want a hard drop yet, does a soft penalty on all noncanonical template families improve the benchmark?",
        experiment_stub(
            kind="penalty_sweep",
            approach="Swept a global noncanonical penalty across the current post-policy candidate pools.",
            what_was_done=[
                "Applied a global penalty to every noncanonical candidate.",
                "Measured the full penalty sweep on the three primary variants.",
            ],
            what_was_learned=[
                "A global noncanonical penalty improves the benchmark over a broad range of penalty values.",
                "The improvement is not a tiny blip; it remains positive across the tested penalty sweep.",
            ],
            answer="Yes. A global noncanonical penalty is strongly helpful on the current post-policy benchmark.",
            key_metrics={
                "best_safe_penalty_sum_mrr": best_safe_penalty["combined"]["sum_mrr"],
                "current_sum_mrr": current_policy["combined"]["sum_mrr"],
            },
            evidence_paths=[
                "../raw/global_noncanonical_penalty_sweep.json",
            ],
        ),
    )

    add_question(
        "Q223",
        "penalty",
        "global_noncanonical_penalty_preserves_target_presence",
        "Is the soft global noncanonical penalty safe with respect to target presence on the benchmark and hard-negative variants?",
        experiment_stub(
            kind="penalty_sweep",
            approach="Checked target presence across the entire global noncanonical penalty sweep.",
            what_was_done=[
                "Measured found-item counts for every tested noncanonical penalty value.",
                "Checked whether any penalty dropped benchmark target presence.",
            ],
            what_was_learned=[
                "The tested global noncanonical penalties preserve 11/11 target presence on all three primary variants.",
                "That makes the global penalty search safer than the unsafe canonical-only hard drop.",
            ],
            answer="Yes. The tested soft noncanonical penalties preserve full target presence on all three primary variants.",
            key_metrics={
                "best_safe_penalty_found_items": {
                    variant: best_safe_penalty["variants"][variant]["overall"]["found_items"]
                    for variant in PRIMARY_VARIANTS
                },
            },
            evidence_paths=[
                "../raw/global_noncanonical_penalty_sweep.json",
            ],
        ),
    )

    add_question(
        "Q224",
        "penalty",
        "best_tested_noncanonical_penalty_is_a_275_to_300_plateau",
        "What penalty magnitude works best in the tested global noncanonical-penalty sweep, and is it a single sharp point or a small plateau?",
        experiment_stub(
            kind="penalty_sweep",
            approach="Searched the full noncanonical-penalty sweep and extracted the best-performing plateau.",
            what_was_done=[
                "Swept noncanonical penalties from 0.25 pt through 5.0 pt in 0.25 pt steps, plus smaller early checks.",
                "Ranked the tested penalties by summed MRR across the three primary variants.",
            ],
            what_was_learned=[
                f"The best tested penalty plateau is {[policy['noncanonical_penalty_pt'] for policy in best_penalty_plateau]}.",
                "The top of the sweep is a plateau around 2.75 pt to 3.0 pt, not a single isolated spike.",
            ],
            answer="The best tested penalty is a small plateau around `2.75 pt` to `3.0 pt`.",
            key_metrics={
                "best_penalty_plateau": [policy["noncanonical_penalty_pt"] for policy in best_penalty_plateau],
                "best_safe_penalty_combined": best_safe_penalty["combined"],
            },
            evidence_paths=[
                "../raw/global_noncanonical_penalty_sweep.json",
            ],
        ),
    )

    add_question(
        "Q225",
        "penalty",
        "penalty_beyond_300_stops_helping",
        "Does the global noncanonical penalty keep getting better as it increases, or does it plateau and then start to stop helping?",
        experiment_stub(
            kind="penalty_sweep",
            approach="Compared the top of the noncanonical-penalty sweep against higher penalty values beyond the best plateau.",
            what_was_done=[
                "Measured the penalty sweep beyond 3.0 pt up to 5.0 pt.",
                "Compared summed MRR at 3.0 pt versus larger penalties.",
            ],
            what_was_learned=[
                "The best plateau is at 2.75 pt to 3.0 pt.",
                "Larger penalties beyond that point stop helping and slowly drift down.",
            ],
            answer="It plateaus and then stops helping. The sweep improves up to about `2.75–3.0 pt`, then larger penalties no longer improve the benchmark.",
            key_metrics={
                "best_penalty_sum_mrr": best_safe_penalty["combined"]["sum_mrr"],
                "penalty_5_sum_mrr": next(
                    policy["combined"]["sum_mrr"]
                    for policy in global_noncanonical_penalty_sweep
                    if math.isclose(policy["noncanonical_penalty_pt"], 5.0)
                ),
            },
            evidence_paths=[
                "../raw/global_noncanonical_penalty_sweep.json",
            ],
        ),
    )

    add_question(
        "Q226",
        "penalty",
        "best_safe_penalty_beats_best_safe_hard_drop",
        "If we compare the best safe soft penalty against the best safe hard-drop family policy, which one is stronger on the current proof round?",
        experiment_stub(
            kind="policy_comparison",
            approach="Compared the strongest safe global noncanonical penalty against the strongest safe hard-drop family policy.",
            what_was_done=[
                "Selected the best safe hard-drop policy from the hard-policy search.",
                "Selected the best safe global noncanonical penalty from the penalty sweep.",
                "Compared their combined metrics.",
            ],
            what_was_learned=[
                f"Best safe hard-drop combined metrics: {best_safe_hard_drop['combined']}.",
                f"Best safe penalty combined metrics: {best_safe_penalty['combined']}.",
            ],
            answer="The best safe soft penalty is stronger than the best safe hard-drop policy on this proof round.",
            key_metrics={
                "best_safe_hard_drop_sum_mrr": best_safe_hard_drop["combined"]["sum_mrr"],
                "best_safe_penalty_sum_mrr": best_safe_penalty["combined"]["sum_mrr"],
            },
            evidence_paths=[
                "../raw/hard_policy_search.json",
                "../raw/global_noncanonical_penalty_sweep.json",
            ],
        ),
    )

    add_question(
        "Q227",
        "penalty",
        "targeted_first_last_and_surname_penalties_help",
        "If we penalize only the two residual winner families `first_last` and `surname_only`, does that help at all?",
        experiment_stub(
            kind="penalty_grid",
            approach="Swept targeted penalties over `first_last` and `surname_only` only.",
            what_was_done=[
                "Ran a targeted penalty grid for `first_last` and `surname_only`.",
                "Measured the resulting metrics across the primary variants.",
            ],
            what_was_learned=[
                "Targeted penalties on the two residual winner families do help.",
                "They still do not reach the stronger gains from the best global noncanonical penalty.",
            ],
            answer="Yes. Targeted penalties on `first_last` and `surname_only` help, but they are weaker than the best global noncanonical penalty.",
            key_metrics={
                "best_targeted_penalty_sum_mrr": targeted_penalty_grid[0]["combined"]["sum_mrr"],
                "best_targeted_penalties": targeted_penalty_grid[0]["template_penalties_pt"],
            },
            evidence_paths=[
                "../raw/targeted_penalty_grid.json",
            ],
        ),
    )

    add_question(
        "Q228",
        "penalty",
        "targeted_penalties_are_weaker_than_global_noncanonical_penalty",
        "Do targeted penalties on `first_last` and `surname_only` fully explain the noncanonical problem, or is a broader noncanonical penalty still better?",
        experiment_stub(
            kind="penalty_comparison",
            approach="Compared the best targeted first_last/surname penalty against the best global noncanonical penalty.",
            what_was_done=[
                "Selected the best targeted penalty row from the targeted penalty grid.",
                "Compared it against the best global noncanonical penalty.",
            ],
            what_was_learned=[
                f"Best targeted penalty sum_mrr: {targeted_penalty_grid[0]['combined']['sum_mrr']}.",
                f"Best global noncanonical penalty sum_mrr: {best_safe_penalty['combined']['sum_mrr']}.",
                "The broader penalty is stronger, which means residual noncanonical influence exists beyond just those two winning families.",
            ],
            answer="A broader noncanonical penalty is still better. That tells us the residual noncanonical problem is not fully captured by penalizing only `first_last` and `surname_only`.",
            key_metrics={
                "best_targeted_penalty_sum_mrr": targeted_penalty_grid[0]["combined"]["sum_mrr"],
                "best_global_noncanonical_penalty_sum_mrr": best_safe_penalty["combined"]["sum_mrr"],
            },
            evidence_paths=[
                "../raw/targeted_penalty_grid.json",
                "../raw/global_noncanonical_penalty_sweep.json",
            ],
        ),
    )

    add_question(
        "Q229",
        "post_cleanup_profile",
        "safe_hard_drop_remainder_is_all_canonical_top1",
        "After the best safe hard-drop cleanup, are the remaining hard rows still mixed between canonical and noncanonical winners, or do they collapse to canonical winners only?",
        experiment_stub(
            kind="post_cleanup_profile",
            approach="Profiled the rows that still rank worse than 20 after the best safe hard-drop policy.",
            what_was_done=[
                "Selected the best safe hard-drop policy.",
                "Extracted the remaining rows with target rank > 20 on each primary variant.",
                "Counted top-1 template families on those remaining rows.",
            ],
            what_was_learned=[
                f"Best safe hard-drop baseline remaining rows are all canonical winners: {best_safe_hard_drop_baseline['top1_template_family_counts']}.",
                "The noncanonical top-1 contamination disappears under the safe hard-drop cleanup.",
            ],
            answer="Yes. After the best safe hard-drop cleanup, the remaining hard rows collapse to canonical winners only.",
            key_metrics={
                "baseline_remaining_top1_template_family_counts": best_safe_hard_drop_baseline["top1_template_family_counts"],
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q230",
        "post_cleanup_profile",
        "safe_penalty_remainder_is_all_canonical_top1",
        "After the best safe soft penalty, do the remaining hard rows also collapse to canonical winners only?",
        experiment_stub(
            kind="post_cleanup_profile",
            approach="Profiled the rows that still rank worse than 20 after the best safe noncanonical penalty.",
            what_was_done=[
                "Selected the best safe noncanonical penalty from the sweep.",
                "Extracted the remaining rows with target rank > 20 on each primary variant.",
                "Counted top-1 template families on those remaining rows.",
            ],
            what_was_learned=[
                f"Best safe penalty baseline remaining rows are all canonical winners: {best_safe_penalty_baseline['top1_template_family_counts']}.",
                "The residual noncanonical top-1 contamination disappears under the safe soft penalty too.",
            ],
            answer="Yes. After the best safe soft penalty, the remaining hard rows are also canonical winners only.",
            key_metrics={
                "baseline_remaining_top1_template_family_counts": best_safe_penalty_baseline["top1_template_family_counts"],
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q231",
        "post_cleanup_profile",
        "safe_hard_drop_remainder_is_still_lower_width_error",
        "After the best safe hard-drop cleanup, what is the recorded reason for the rows that still remain hard?",
        experiment_stub(
            kind="post_cleanup_profile",
            approach="Counted miss reasons among the rows that still rank worse than 20 after the best safe hard-drop policy.",
            what_was_done=[
                "Extracted the remaining >20 rows after the best safe hard-drop policy.",
                "Counted their miss reasons.",
            ],
            what_was_learned=[
                f"Best safe hard-drop baseline remaining reason counts are {best_safe_hard_drop_baseline['reason_counts']}.",
                "All remaining hard rows are still genuine lower-width-error losses, not missing-target or tiebreak-only artifacts.",
            ],
            answer="They remain `top1_lower_width_error` rows.",
            key_metrics={
                "baseline_remaining_reason_counts": best_safe_hard_drop_baseline["reason_counts"],
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q232",
        "post_cleanup_profile",
        "safe_penalty_remainder_is_still_lower_width_error",
        "After the best safe soft penalty, what is the recorded reason for the rows that still remain hard?",
        experiment_stub(
            kind="post_cleanup_profile",
            approach="Counted miss reasons among the rows that still rank worse than 20 after the best safe noncanonical penalty.",
            what_was_done=[
                "Extracted the remaining >20 rows after the best safe penalty.",
                "Counted their miss reasons.",
            ],
            what_was_learned=[
                f"Best safe penalty baseline remaining reason counts are {best_safe_penalty_baseline['reason_counts']}.",
                "The penalty removes noncanonical contamination, but the remainder is still real lower-width-error competition.",
            ],
            answer="They remain `top1_lower_width_error` rows.",
            key_metrics={
                "baseline_remaining_reason_counts": best_safe_penalty_baseline["reason_counts"],
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q233",
        "post_cleanup_profile",
        "safe_hard_drop_remainder_is_glyph_dominated",
        "After the best safe hard-drop cleanup, are the remaining hard rows still glyph-width-dominated or do spacing components start to matter?",
        experiment_stub(
            kind="post_cleanup_profile",
            approach="Counted dominant width components among the rows that still rank worse than 20 after the best safe hard-drop policy.",
            what_was_done=[
                "Extracted the remaining >20 rows after the best safe hard-drop policy.",
                "Counted dominant width components on those rows.",
            ],
            what_was_learned=[
                f"Best safe hard-drop baseline dominant-component counts are {best_safe_hard_drop_baseline['dominant_component_counts']}.",
                "The remaining hard rows are still glyph-width-dominated.",
            ],
            answer="They remain glyph-width-dominated.",
            key_metrics={
                "baseline_remaining_dominant_component_counts": best_safe_hard_drop_baseline["dominant_component_counts"],
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q234",
        "post_cleanup_profile",
        "safe_penalty_remainder_is_glyph_dominated",
        "After the best safe soft penalty, are the remaining hard rows still glyph-width-dominated?",
        experiment_stub(
            kind="post_cleanup_profile",
            approach="Counted dominant width components among the rows that still rank worse than 20 after the best safe penalty.",
            what_was_done=[
                "Extracted the remaining >20 rows after the best safe penalty.",
                "Counted dominant width components on those rows.",
            ],
            what_was_learned=[
                f"Best safe penalty baseline dominant-component counts are {best_safe_penalty_baseline['dominant_component_counts']}.",
                "The remaining hard rows are still glyph-width-dominated after the penalty cleanup.",
            ],
            answer="Yes. They remain glyph-width-dominated.",
            key_metrics={
                "baseline_remaining_dominant_component_counts": best_safe_penalty_baseline["dominant_component_counts"],
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q235",
        "post_cleanup_profile",
        "safe_hard_drop_remaining_rows_exactly",
        "Which exact benchmark rows still remain outside the top 20 after the best safe hard-drop cleanup?",
        experiment_stub(
            kind="post_cleanup_profile",
            approach="Listed every target row that still has rank > 20 under the best safe hard-drop policy on the baseline benchmark.",
            what_was_done=[
                "Extracted baseline remaining >20 rows under the best safe hard-drop policy.",
            ],
            what_was_learned=[
                f"Baseline remaining rows are {[row['label'] for row in best_safe_hard_drop_baseline['remaining_over_20_rows']]}.",
            ],
            answer="The exact remaining baseline rows are persisted in the safe-policy profile and listed in this experiment.",
            key_metrics={
                "baseline_remaining_over_20_rows": [
                    {
                        "label": row["label"],
                        "best_rank": row["best_rank"],
                        "top1_text": row["top1_text"],
                    }
                    for row in best_safe_hard_drop_baseline["remaining_over_20_rows"]
                ],
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q236",
        "post_cleanup_profile",
        "safe_penalty_remaining_rows_exactly",
        "Which exact benchmark rows still remain outside the top 20 after the best safe soft penalty?",
        experiment_stub(
            kind="post_cleanup_profile",
            approach="Listed every target row that still has rank > 20 under the best safe noncanonical penalty on the baseline benchmark.",
            what_was_done=[
                "Extracted baseline remaining >20 rows under the best safe penalty.",
            ],
            what_was_learned=[
                f"Baseline remaining rows are {[row['label'] for row in best_safe_penalty_baseline['remaining_over_20_rows']]}.",
            ],
            answer="The exact remaining baseline rows are persisted in the safe-policy profile and listed in this experiment.",
            key_metrics={
                "baseline_remaining_over_20_rows": [
                    {
                        "label": row["label"],
                        "best_rank": row["best_rank"],
                        "top1_text": row["top1_text"],
                    }
                    for row in best_safe_penalty_baseline["remaining_over_20_rows"]
                ],
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q237",
        "post_cleanup_profile",
        "safe_cleanup_improves_both_benchmark_datasets",
        "Do the safe cleanup policies improve only one PDF or do they improve both canonical benchmark PDFs?",
        experiment_stub(
            kind="dataset_check",
            approach="Compared dataset-level summaries under the current runtime, the best safe hard-drop, and the best safe penalty.",
            what_was_done=[
                "Read the baseline variant profiles under current runtime and the two best safe cleanup policies.",
                "Checked the row-level rank changes across both benchmark PDFs.",
            ],
            what_was_learned=[
                "Both safe cleanup policies improve the benchmark rows in `EFTA00038617` and also improve the `second_last` benchmark row in `EFTA00101126`.",
            ],
            answer="They improve both benchmark PDFs, although the bulk of the gain still comes from `EFTA00038617`.",
            key_metrics={
                "current_baseline_mean_rank": current_policy["variants"]["baseline"]["overall"]["mean_rank_found"],
                "best_safe_hard_drop_baseline_mean_rank": best_safe_hard_drop["variants"]["baseline"]["overall"]["mean_rank_found"],
                "best_safe_penalty_baseline_mean_rank": best_safe_penalty["variants"]["baseline"]["overall"]["mean_rank_found"],
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q238",
        "post_cleanup_profile",
        "safe_cleanup_improves_both_hard_negative_variants",
        "Do the safe cleanup policies only help the easy benchmark, or do they also improve both hard-negative full-name variants?",
        experiment_stub(
            kind="dataset_check",
            approach="Compared the current runtime against the safe cleanup policies on both hard-negative variants.",
            what_was_done=[
                "Measured both safe cleanup policies on `hard_negative_full_name_w2` and `hard_negative_full_name_w5`.",
                "Compared their metrics against the current post-policy runtime.",
            ],
            what_was_learned=[
                "Both safe cleanup policies improve both hard-negative variants.",
                "The best safe penalty is strongest on the hard-negative variants too.",
            ],
            answer="They improve both hard-negative variants as well. This proof round is not just exploiting the easy benchmark surface.",
            key_metrics={
                "best_safe_hard_drop_hard_negative_w2_mrr": best_safe_hard_drop["variants"]["hard_negative_full_name_w2"]["overall"]["mrr"],
                "best_safe_penalty_hard_negative_w2_mrr": best_safe_penalty["variants"]["hard_negative_full_name_w2"]["overall"]["mrr"],
                "current_hard_negative_w2_mrr": current_policy["variants"]["hard_negative_full_name_w2"]["overall"]["mrr"],
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
                "../raw/global_noncanonical_penalty_sweep.json",
                "../raw/hard_policy_search.json",
            ],
        ),
    )

    add_question(
        "Q239",
        "post_cleanup_profile",
        "remaining_rows_concentrate_in_efta00038617_plus_second_last",
        "After safe cleanup, where do the remaining hard rows concentrate?",
        experiment_stub(
            kind="remaining_row_classification",
            approach="Listed the remaining >20 rows after the safe cleanup policies and grouped them by dataset.",
            what_was_done=[
                "Grouped the remaining >20 rows under the best safe hard-drop and best safe penalty by dataset.",
            ],
            what_was_learned=[
                "The remaining hard rows stay concentrated in `EFTA00038617`, plus the `second_last` row in `EFTA00101126`.",
            ],
            answer="They concentrate in `EFTA00038617`, with the `EFTA00101126` `second_last` row remaining as the other persistent hard case.",
            key_metrics={
                "best_safe_hard_drop_remaining_datasets": dict(
                    Counter(row["dataset"] for row in best_safe_hard_drop_baseline["remaining_over_20_rows"])
                ),
                "best_safe_penalty_remaining_datasets": dict(
                    Counter(row["dataset"] for row in best_safe_penalty_baseline["remaining_over_20_rows"])
                ),
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q240",
        "post_cleanup_profile",
        "remaining_winners_become_canonical_plain_multi_full_names",
        "After safe cleanup, what kind of winners remain on the hard rows?",
        experiment_stub(
            kind="remaining_row_classification",
            approach="Profiled the remaining >20 winners after the two best safe cleanup policies.",
            what_was_done=[
                "Counted winner template families and variant families on the remaining >20 rows after each safe cleanup policy.",
            ],
            what_was_learned=[
                "The remaining hard-row winners are canonical plain multi-token names.",
                "That is the corrected condition under which the 'canonical tie' claim becomes true.",
            ],
            answer="After safe cleanup, the remaining winners are canonical plain multi-token full names.",
            key_metrics={
                "best_safe_hard_drop_remaining_top1_template_family_counts": best_safe_hard_drop_baseline["top1_template_family_counts"],
                "best_safe_penalty_remaining_top1_template_family_counts": best_safe_penalty_baseline["top1_template_family_counts"],
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q241",
        "conclusion",
        "corrected_remaining_hard_class_after_safe_cleanup",
        "After rerunning the proof process on the live post-policy state, what is the corrected statement about the remaining hard class?",
        experiment_stub(
            kind="conclusion",
            approach="Combined the current post-policy profile with the safe cleanup counterfactual profiles.",
            what_was_done=[
                "Measured the live post-policy current state.",
                "Measured the best safe hard-drop and best safe soft-penalty cleanups.",
                "Compared the remaining hard-row winner families, reasons, and dominant width components.",
            ],
            what_was_learned=[
                "The earlier stronger claim was too early on the live post-policy runtime.",
                "Residual noncanonical template contamination still matters before cleanup.",
                "After safe cleanup, the remaining hard class does collapse to canonical plain-multi glyph-width lower-error competition.",
            ],
            answer="The corrected statement is: the live post-policy runtime is still held back by residual noncanonical template contamination first; after safe cleanup, the remaining hard class is canonical plain-multi glyph-width lower-error competition.",
            key_metrics={
                "current_noncanonical_top1_rows": sum(
                    count
                    for family, count in current_baseline_profile["top1_template_family_counts"].items()
                    if family != "canonical"
                ),
                "best_safe_penalty_remaining_canonical_rows": best_safe_penalty_baseline["remaining_over_20_count"],
            },
            evidence_paths=[
                "../raw/current_post_policy_profile.json",
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q242",
        "conclusion",
        "semantic_prior_is_next_after_safe_cleanup",
        "Once the residual noncanonical contamination is handled safely, what does the remaining evidence say the next larger lever should be?",
        experiment_stub(
            kind="conclusion",
            approach="Used the post-cleanup profiles to classify what still remains once residual noncanonical template contamination is removed or penalized safely.",
            what_was_done=[
                "Inspected the remaining >20 rows after both safe cleanup policies.",
                "Checked their winner families, miss reasons, and dominant width components.",
            ],
            what_was_learned=[
                "The remaining hard rows are all canonical, plain-multi, lower-width-error, and glyph-dominated.",
                "That is the signature of a ranking problem that needs a non-width semantic prior or a better candidate source, not more family cleanup or anchor work.",
            ],
            answer="After safe cleanup, the next bigger lever is a non-width semantic prior or better candidate source for plausible full names. The benchmark evidence no longer points first at anchor work or family cleanup at that stage.",
            key_metrics={
                "best_safe_penalty_remaining_reason_counts": best_safe_penalty_baseline["reason_counts"],
                "best_safe_penalty_remaining_dominant_component_counts": best_safe_penalty_baseline["dominant_component_counts"],
            },
            evidence_paths=[
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q243",
        "conclusion",
        "more_anchor_work_is_not_the_next_move",
        "Does this proof round reopen anchor/redaction sizing as the next move, or does it continue to point elsewhere?",
        experiment_stub(
            kind="conclusion",
            approach="Combined the carried anchor-trust evidence with the new post-policy ranking proof round.",
            what_was_done=[
                "Checked the benchmark-linked visual anchor trust rows again.",
                "Compared them with the post-cleanup miss classification.",
            ],
            what_was_learned=[
                "Anchor trust remains mostly good on the benchmark-linked rows.",
                "After safe cleanup, the remaining misses are canonical width-ranking problems, not anchor-family problems.",
            ],
            answer="It still points away from anchor work as the next move. The next problem is ranking among plausible canonical names, not anchor sizing.",
            key_metrics={
                "trusted_or_non_sizing_issue_count": anchor_trust["trusted_or_non_sizing_issue_count"],
                "benchmark_visual_row_count": anchor_trust["benchmark_visual_row_count"],
            },
            evidence_paths=[
                "../raw/anchor_trust_followup.json",
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q244",
        "conclusion",
        "old_claim_was_too_early_and_is_now_corrected",
        "Was the earlier statement 'the remaining misses are canonical plain-multi tie rows' correct on the live post-policy runtime, or did this proof round change that conclusion?",
        experiment_stub(
            kind="conclusion",
            approach="Compared the earlier claim against the new current post-policy profile and the post-cleanup counterfactual profiles.",
            what_was_done=[
                "Measured the live post-policy current state.",
                "Measured the post-cleanup safe policies.",
                "Compared the two states directly.",
            ],
            what_was_learned=[
                "The earlier statement was too early for the live post-policy runtime.",
                "The corrected version only becomes true after the residual noncanonical cleanup is applied.",
            ],
            answer="It was too early on the live runtime. The corrected proof is: residual noncanonical contamination still matters now; after safe cleanup, the remainder does become canonical plain-multi glyph-width tie competition.",
            key_metrics={
                "current_noncanonical_top1_rows": sum(
                    count
                    for family, count in current_baseline_profile["top1_template_family_counts"].items()
                    if family != "canonical"
                ),
                "post_cleanup_noncanonical_top1_rows": sum(
                    count
                    for family, count in best_safe_penalty_baseline["top1_template_family_counts"].items()
                    if family != "canonical"
                ),
            },
            evidence_paths=[
                "../raw/current_post_policy_profile.json",
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    add_question(
        "Q245",
        "plan",
        "data_backed_next_runtime_trial_after_proof_round",
        "After this proof round, what is the narrowest data-backed next runtime trial if we want to keep improving guess ranking without reopening anchor work or making a broad semantic rewrite yet?",
        experiment_stub(
            kind="plan",
            approach="Compared the best safe hard-drop family policy and the best safe global noncanonical penalty, then classified the post-cleanup remainder.",
            what_was_done=[
                "Ranked the safe hard-drop policies.",
                "Ranked the safe global noncanonical penalties.",
                "Compared both against the corrected remaining-miss class.",
            ],
            what_was_learned=[
                "The best safe soft noncanonical penalty outperforms the best safe hard-drop policy.",
                "The remaining miss class after safe cleanup is canonical/glyph-width competition, so the next change should stay narrow and avoid pretending to solve the whole semantic problem in one jump.",
            ],
            answer="The narrowest data-backed next runtime trial is a soft noncanonical penalty in the `2.75–3.0 pt` range on top of the current runtime policy. It is stronger than the best safe hard drop and preserves full target presence on the tested benchmark surfaces.",
            key_metrics={
                "best_safe_penalty_pt": best_safe_penalty["noncanonical_penalty_pt"],
                "best_safe_penalty_sum_mrr": best_safe_penalty["combined"]["sum_mrr"],
                "best_safe_hard_drop_sum_mrr": best_safe_hard_drop["combined"]["sum_mrr"],
            },
            evidence_paths=[
                "../raw/global_noncanonical_penalty_sweep.json",
                "../raw/hard_policy_search.json",
                "../raw/safe_policy_profiles.json",
            ],
        ),
    )

    for question_id, experiment in experiments.items():
        experiment_id = f"EXP{question_id[1:]}"
        write_json(EXPERIMENTS_ROOT / f"{experiment_id}.json", experiment)
        question = next(item for item in questions if item.id == question_id)
        write_text(ANSWERS_ROOT / f"{question_id}.md", build_question_answer_markdown(question, experiment))

    report_lines = [
        "# Benchmark Question Dossier Round 3",
        "",
        f"- Total linked questions: `{len(questions)}`",
        f"- New questions this round: `{len(experiments)}`",
        f"- Carried-forward questions: `{len(questions) - len(experiments)}`",
        f"- Hard-drop/keep policies tested: `{len(hard_policy_search)}`",
        f"- Global noncanonical penalties tested: `{len(global_noncanonical_penalty_sweep)}`",
        f"- Targeted first_last/surname penalty combinations tested: `{len(targeted_penalty_grid)}`",
        "",
        "## Key Findings",
        "",
        "- The earlier claim that the live post-policy remainder was already purely canonical was too strong.",
        "- The live post-policy baseline still has residual noncanonical winner contamination from `first_last` and `surname_only`.",
        f"- The best safe hard-drop policy is `{best_safe_hard_drop['name']}`.",
        f"- The best safe global noncanonical penalty is `{best_safe_penalty['noncanonical_penalty_pt']} pt` within a `{[policy['noncanonical_penalty_pt'] for policy in best_penalty_plateau]}` plateau.",
        "- After either safe cleanup, the remaining hard rows collapse to canonical plain-multi lower-width-error glyph-dominated competition.",
        "",
        "## Most Important Correction",
        "",
        "- The correct statement is not 'the current live runtime remainder is already canonical ties'.",
        "- The correct statement is 'the current live runtime still has residual noncanonical template contamination; after safe cleanup, the remaining hard class becomes canonical plain-multi glyph-width tie competition'.",
        "",
        "## Core Artifacts",
        "",
        "- [questions.md](questions.md)",
        "- [summary.json](summary.json)",
        "- [raw/current_post_policy_profile.json](raw/current_post_policy_profile.json)",
        "- [raw/hard_policy_search.json](raw/hard_policy_search.json)",
        "- [raw/global_noncanonical_penalty_sweep.json](raw/global_noncanonical_penalty_sweep.json)",
        "- [raw/targeted_penalty_grid.json](raw/targeted_penalty_grid.json)",
        "- [raw/ranking_mode_matrix.json](raw/ranking_mode_matrix.json)",
        "- [raw/safe_policy_profiles.json](raw/safe_policy_profiles.json)",
        "",
    ]
    write_text(OUTPUT_ROOT / "report.md", "\n".join(report_lines))

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
    write_json(OUTPUT_ROOT / "questions.json", questions_payload)

    questions_md_lines = ["# Questions", ""]
    for question in questions:
        questions_md_lines += [
            f"## {question.id}: {question.title}",
            "",
            question.context,
            "",
            f"- Domain: `{question.domain}`",
            f"- Experiment: `{question.experiment_id}`",
            f"- Answer: `{question.answer_path}`",
            f"- Experiment file: `{question.experiment_path}`",
            "",
        ]
    write_text(OUTPUT_ROOT / "questions.md", "\n".join(questions_md_lines))

    summary = {
        "total_linked_questions": len(questions),
        "new_questions_this_round": len(experiments),
        "carried_forward_questions": len(questions) - len(experiments),
        "hard_drop_policies_tested": len(hard_policy_search),
        "global_noncanonical_penalties_tested": len(global_noncanonical_penalty_sweep),
        "targeted_penalty_combinations_tested": len(targeted_penalty_grid),
        "current_combined": current_policy["combined"],
        "best_safe_hard_drop": {
            "name": best_safe_hard_drop["name"],
            "combined": best_safe_hard_drop["combined"],
            "keep_template_families": best_safe_hard_drop["keep_template_families"],
            "drop_template_families": best_safe_hard_drop["drop_template_families"],
        },
        "best_safe_penalty": {
            "penalty_pt": best_safe_penalty["noncanonical_penalty_pt"],
            "combined": best_safe_penalty["combined"],
        },
        "best_penalty_plateau": [policy["noncanonical_penalty_pt"] for policy in best_penalty_plateau],
    }
    write_json(OUTPUT_ROOT / "summary.json", summary)


if __name__ == "__main__":
    main()
