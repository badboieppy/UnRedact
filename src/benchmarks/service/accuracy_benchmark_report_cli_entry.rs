use std::path::PathBuf;

use crate::benchmarks::logic::accuracy_benchmark_report_component::{
    encode_json, run_accuracy_benchmark_report, write_report_artifact,
    AccuracyBenchmarkReportRequest,
};
use crate::benchmarks::types::accuracy_benchmark_report_types::AccuracyBenchmarkOutputs;

#[derive(Debug, Clone, PartialEq)]
pub struct AccuracyBenchmarkReportCliRequest {
    pub output_dir: PathBuf,
    pub repeats: usize,
    pub compact: bool,
}

#[inline]
pub fn run(req: AccuracyBenchmarkReportCliRequest) -> Result<AccuracyBenchmarkOutputs, String> {
    let run = run_accuracy_benchmark_report(AccuracyBenchmarkReportRequest {
        output_dir: req.output_dir.clone(),
        repeats: req.repeats,
        compact: req.compact,
    })?;

    let summary_path = req.output_dir.join("summary.json");
    let markdown_path = req.output_dir.join("summary.md");
    let definitions_path = req.output_dir.join("definitions.md");
    let manifest_path = req.output_dir.join("manifest.json");
    let baseline_stage_path = req.output_dir.join("stages").join("guess_baseline.json");
    let dictionary_ablation_path = req
        .output_dir
        .join("stages")
        .join("dictionary_ablation.json");
    let candidate_pool_quality_path = req
        .output_dir
        .join("signals")
        .join("candidate_pool_quality.json");
    let family_composition_path = req
        .output_dir
        .join("signals")
        .join("family_composition.json");
    let best_possible_rank_path = req
        .output_dir
        .join("signals")
        .join("best_possible_rank.json");
    let pairwise_winner_explanations_path = req
        .output_dir
        .join("signals")
        .join("pairwise_winner_explanations.json");
    let tie_density_path = req.output_dir.join("signals").join("tie_density.json");
    let perturbation_robustness_path = req
        .output_dir
        .join("signals")
        .join("perturbation_robustness.json");
    let stability_path = req.output_dir.join("signals").join("stability.json");
    let candidate_source_provenance_path = req
        .output_dir
        .join("signals")
        .join("candidate_source_provenance.json");
    let variant_template_provenance_path = req
        .output_dir
        .join("signals")
        .join("variant_template_provenance.json");
    let width_component_attribution_path = req
        .output_dir
        .join("signals")
        .join("width_component_attribution.json");
    let overlap_recompute_geometry_path = req
        .output_dir
        .join("signals")
        .join("overlap_recompute_geometry.json");
    let oracle_full_name_pool_ceiling_path = req
        .output_dir
        .join("signals")
        .join("oracle_full_name_pool_ceiling.json");
    let row_cluster_assignment_path = req
        .output_dir
        .join("signals")
        .join("row_cluster_assignment.json");
    let anchor_locality_percentile_path = req
        .output_dir
        .join("signals")
        .join("anchor_locality_percentile.json");
    let redaction_box_trust_classifier_path = req
        .output_dir
        .join("signals")
        .join("redaction_box_trust_classifier.json");
    let topk_family_entropy_path = req
        .output_dir
        .join("signals")
        .join("topk_family_entropy.json");

    write_report_artifact(
        &summary_path,
        encode_json(&run.summary, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &manifest_path,
        encode_json(&run.manifest, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &baseline_stage_path,
        encode_json(&run.baseline_stage, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &dictionary_ablation_path,
        encode_json(&run.dictionary_ablation, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &candidate_pool_quality_path,
        encode_json(&run.candidate_pool_quality, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &family_composition_path,
        encode_json(&run.family_composition, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &best_possible_rank_path,
        encode_json(&run.best_possible_rank, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &pairwise_winner_explanations_path,
        encode_json(&run.pairwise_winner_explanations, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &tie_density_path,
        encode_json(&run.tie_density, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &perturbation_robustness_path,
        encode_json(&run.perturbation_robustness, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &stability_path,
        encode_json(&run.stability, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &candidate_source_provenance_path,
        encode_json(&run.candidate_source_provenance, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &variant_template_provenance_path,
        encode_json(&run.variant_template_provenance, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &width_component_attribution_path,
        encode_json(&run.width_component_attribution, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &overlap_recompute_geometry_path,
        encode_json(&run.overlap_recompute_geometry, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &oracle_full_name_pool_ceiling_path,
        encode_json(&run.oracle_full_name_pool_ceiling, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &row_cluster_assignment_path,
        encode_json(&run.row_cluster_assignment, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &anchor_locality_percentile_path,
        encode_json(&run.anchor_locality_percentile, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &redaction_box_trust_classifier_path,
        encode_json(&run.redaction_box_trust_classifier, req.compact)?.as_slice(),
    )?;
    write_report_artifact(
        &topk_family_entropy_path,
        encode_json(&run.topk_family_entropy, req.compact)?.as_slice(),
    )?;
    write_report_artifact(&markdown_path, run.summary_markdown.as_bytes())?;
    write_report_artifact(&definitions_path, run.definitions_markdown.as_bytes())?;

    Ok(AccuracyBenchmarkOutputs {
        summary_path,
        markdown_path,
        definitions_path,
        manifest_path,
        baseline_stage_path,
        dictionary_ablation_path,
        candidate_pool_quality_path,
        family_composition_path,
        best_possible_rank_path,
        pairwise_winner_explanations_path,
        tie_density_path,
        perturbation_robustness_path,
        stability_path,
        candidate_source_provenance_path,
        variant_template_provenance_path,
        width_component_attribution_path,
        overlap_recompute_geometry_path,
        oracle_full_name_pool_ceiling_path,
        row_cluster_assignment_path,
        anchor_locality_percentile_path,
        redaction_box_trust_classifier_path,
        topk_family_entropy_path,
        visual_review_manifest_path: run
            .visual_review_pack
            .as_ref()
            .map(|summary| summary.manifest_path.clone()),
        anchor_span_visual_summary_path: run.visual_stage.map(|stage| stage.summary_path),
    })
}
