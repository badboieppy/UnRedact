use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::redaction_types::Rect;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccuracyBenchmarkReportManifest {
    pub contract_id: String,
    pub schema_version: usize,
    pub canonical_target_count: usize,
    pub repeats: usize,
    pub dictionary_variants: Vec<String>,
    pub executed_stages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccuracyBenchmarkSummary {
    pub manifest: AccuracyBenchmarkReportManifest,
    pub baseline: VariantSummary,
    pub dictionary_ablation: DictionaryAblationSummary,
    pub candidate_pool_quality: CandidatePoolQualitySummary,
    pub family_composition: FamilyCompositionSummary,
    pub best_possible_rank: BestPossibleRankSummary,
    pub pairwise_winner_explanations: PairwiseWinnerSummary,
    pub tie_density: TieDensitySummary,
    pub perturbation_robustness: PerturbationRobustnessSummary,
    pub stability: StabilitySummary,
    pub candidate_source_provenance: CandidateSourceProvenanceSummary,
    pub variant_template_provenance: VariantTemplateProvenanceSummary,
    pub width_component_attribution: WidthComponentAttributionSummary,
    pub overlap_recompute_geometry: OverlapRecomputeGeometrySummary,
    pub oracle_full_name_pool_ceiling: OracleFullNamePoolCeilingSummary,
    pub row_cluster_assignment: RowClusterAssignmentSummary,
    pub anchor_locality_percentile: AnchorLocalityPercentileSummary,
    pub redaction_box_trust_classifier: RedactionBoxTrustClassifierSummary,
    pub topk_family_entropy: TopKFamilyEntropySummary,
    #[serde(default)]
    pub visual_review_pack: Option<VisualReviewPackSummary>,
    #[serde(default)]
    pub anchor_span_visual_summary_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccuracyBenchmarkOutputs {
    pub summary_path: PathBuf,
    pub markdown_path: PathBuf,
    pub definitions_path: PathBuf,
    pub manifest_path: PathBuf,
    pub baseline_stage_path: PathBuf,
    pub dictionary_ablation_path: PathBuf,
    pub candidate_pool_quality_path: PathBuf,
    pub family_composition_path: PathBuf,
    pub best_possible_rank_path: PathBuf,
    pub pairwise_winner_explanations_path: PathBuf,
    pub tie_density_path: PathBuf,
    pub perturbation_robustness_path: PathBuf,
    pub stability_path: PathBuf,
    pub candidate_source_provenance_path: PathBuf,
    pub variant_template_provenance_path: PathBuf,
    pub width_component_attribution_path: PathBuf,
    pub overlap_recompute_geometry_path: PathBuf,
    pub oracle_full_name_pool_ceiling_path: PathBuf,
    pub row_cluster_assignment_path: PathBuf,
    pub anchor_locality_percentile_path: PathBuf,
    pub redaction_box_trust_classifier_path: PathBuf,
    pub topk_family_entropy_path: PathBuf,
    #[serde(default)]
    pub visual_review_manifest_path: Option<PathBuf>,
    #[serde(default)]
    pub anchor_span_visual_summary_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    pub evaluated_items: usize,
    pub found_items: usize,
    pub recall_at_1: f64,
    pub recall_at_5: f64,
    pub recall_at_20: f64,
    pub mrr: f64,
    #[serde(default)]
    pub mean_rank_found: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateSummary {
    pub rows_total: usize,
    pub rows_with_candidates: usize,
    #[serde(default)]
    pub mean_count: Option<f64>,
    #[serde(default)]
    pub median_count: Option<f64>,
    #[serde(default)]
    pub p90_count: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct QualitySummary {
    pub rows_total: usize,
    pub anchored_rows: usize,
    pub anchor_two_sided_rows: usize,
    pub anchor_one_sided_rows: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetResult {
    pub dataset: String,
    pub label: String,
    pub target: String,
    #[serde(default)]
    pub best_rank: Option<usize>,
    #[serde(default)]
    pub best_row_key: Option<String>,
    pub eligible_row_count: usize,
    pub present_in_pool: bool,
    #[serde(default)]
    pub top1_text: Option<String>,
    #[serde(default)]
    pub top1_family: Option<String>,
    #[serde(default)]
    pub target_family: Option<String>,
    #[serde(default)]
    pub candidate_count: Option<usize>,
    #[serde(default)]
    pub target_error_pt: Option<f32>,
    #[serde(default)]
    pub top1_error_pt: Option<f32>,
    #[serde(default)]
    pub anchor_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantDatasetResult {
    pub name: String,
    pub summary: BenchmarkSummary,
    pub candidate_summary: CandidateSummary,
    pub quality_summary: QualitySummary,
    pub targets: Vec<TargetResult>,
    pub selected_rows: Vec<SelectedGuessRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantSummary {
    pub name: String,
    pub overall: BenchmarkSummary,
    pub datasets: Vec<VariantDatasetResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictionaryVariantResult {
    pub variant: String,
    pub overall: BenchmarkSummary,
    pub datasets: Vec<VariantDatasetResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictionaryAblationSummary {
    pub baseline_variant: String,
    pub variants: Vec<DictionaryVariantResult>,
    #[serde(default)]
    pub best_variant_by_mrr: Option<String>,
    #[serde(default)]
    pub best_variant_by_mean_rank: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidatePoolQualityRow {
    pub dataset: String,
    pub label: String,
    pub target: String,
    #[serde(default)]
    pub best_rank: Option<usize>,
    pub present_in_pool: bool,
    #[serde(default)]
    pub best_row_key: Option<String>,
    #[serde(default)]
    pub candidate_count: Option<usize>,
    #[serde(default)]
    pub better_than_target_count: Option<usize>,
    #[serde(default)]
    pub same_family_better_count: Option<usize>,
    #[serde(default)]
    pub top1_text: Option<String>,
    #[serde(default)]
    pub target_error_pt: Option<f32>,
    #[serde(default)]
    pub top1_error_pt: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidatePoolQualitySummary {
    pub rows: Vec<CandidatePoolQualityRow>,
    pub targets_total: usize,
    pub targets_present_in_pool: usize,
    pub targets_missing_from_pool: usize,
    pub targets_ranked_top_20: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyCompositionSummary {
    pub target_families: Vec<FamilyCount>,
    pub top1_families: Vec<FamilyCount>,
    pub candidate_families: Vec<FamilyCount>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyCount {
    pub family: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateSourceProvenanceRow {
    pub dataset: String,
    pub label: String,
    pub target: String,
    #[serde(default)]
    pub row_key: Option<String>,
    #[serde(default)]
    pub top1_text: Option<String>,
    #[serde(default)]
    pub top1_template_id: Option<String>,
    #[serde(default)]
    pub top1_template_family: Option<String>,
    #[serde(default)]
    pub top1_variant_family: Option<String>,
    #[serde(default)]
    pub target_template_id: Option<String>,
    #[serde(default)]
    pub target_template_family: Option<String>,
    #[serde(default)]
    pub target_variant_family: Option<String>,
    pub target_present_in_row: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateSourceProvenanceSummary {
    pub rows: Vec<CandidateSourceProvenanceRow>,
    pub top1_template_families: Vec<FamilyCount>,
    pub target_template_families: Vec<FamilyCount>,
    pub top1_variant_families: Vec<FamilyCount>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantTemplateProvenanceRow {
    pub template_id: String,
    pub template_family: String,
    pub candidate_count: usize,
    pub top1_count: usize,
    pub target_count: usize,
    pub displaced_target_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariantTemplateProvenanceSummary {
    pub rows: Vec<VariantTemplateProvenanceRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidthComponentAttributionRow {
    pub dataset: String,
    pub label: String,
    pub target: String,
    #[serde(default)]
    pub row_key: Option<String>,
    #[serde(default)]
    pub top1_text: Option<String>,
    #[serde(default)]
    pub target_total_pt: Option<f32>,
    #[serde(default)]
    pub top1_total_pt: Option<f32>,
    #[serde(default)]
    pub glyph_delta_pt: Option<f32>,
    #[serde(default)]
    pub char_spacing_delta_pt: Option<f32>,
    #[serde(default)]
    pub word_spacing_delta_pt: Option<f32>,
    #[serde(default)]
    pub total_delta_pt: Option<f32>,
    #[serde(default)]
    pub dominant_component: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidthComponentAttributionSummary {
    pub rows: Vec<WidthComponentAttributionRow>,
    pub dominant_components: Vec<FamilyCount>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlapRecomputeGeometryRow {
    pub dataset: String,
    pub label: String,
    pub target: String,
    #[serde(default)]
    pub row_key: Option<String>,
    #[serde(default)]
    pub top1_text: Option<String>,
    pub overlap_rejection_count: usize,
    #[serde(default)]
    pub top1_current_overlap: Option<bool>,
    #[serde(default)]
    pub target_current_overlap: Option<bool>,
    pub supports_no_h_scale_recompute: bool,
    #[serde(default)]
    pub top1_overlap_without_h_scale: Option<bool>,
    #[serde(default)]
    pub target_overlap_without_h_scale: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlapRecomputeGeometrySummary {
    pub rows: Vec<OverlapRecomputeGeometryRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OracleFullNamePoolCeilingRow {
    pub dataset: String,
    pub label: String,
    pub target: String,
    #[serde(default)]
    pub current_rank: Option<usize>,
    #[serde(default)]
    pub full_name_only_rank: Option<usize>,
    #[serde(default)]
    pub multi_token_only_rank: Option<usize>,
    #[serde(default)]
    pub plain_multi_only_rank: Option<usize>,
    #[serde(default)]
    pub hard_negative_w2_rank: Option<usize>,
    #[serde(default)]
    pub hard_negative_w5_rank: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OracleFullNamePoolCeilingSummary {
    pub rows: Vec<OracleFullNamePoolCeilingRow>,
    pub improvable_by_full_name_only: usize,
    pub improvable_by_hard_negative_w2: usize,
    pub improvable_by_hard_negative_w5: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowClusterAssignmentRow {
    pub cluster_id: String,
    pub dataset: String,
    pub page_index: u32,
    pub row_keys: Vec<String>,
    pub cluster_size: usize,
    pub independent_unique_top1_count: usize,
    pub greedy_unique_top1_count: usize,
    pub improved_row_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowClusterAssignmentSummary {
    pub rows: Vec<RowClusterAssignmentRow>,
    pub multi_row_clusters: usize,
    pub improvable_clusters: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorLocalityPercentileRow {
    pub row_key: String,
    pub input_pdf: String,
    pub page_index: u32,
    pub anchor_mode: String,
    #[serde(default)]
    pub selected_line_id: Option<String>,
    #[serde(default)]
    pub selected_left_gap_pt: Option<f32>,
    #[serde(default)]
    pub selected_right_gap_pt: Option<f32>,
    #[serde(default)]
    pub left_gap_percentile: Option<f64>,
    #[serde(default)]
    pub right_gap_percentile: Option<f64>,
    #[serde(default)]
    pub max_gap_percentile: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorLocalityPercentileSummary {
    pub rows: Vec<AnchorLocalityPercentileRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionBoxTrustClassifierRow {
    pub row_key: String,
    pub input_pdf: String,
    pub visual_reference_kind: String,
    pub redaction_box_width_pt: f32,
    #[serde(default)]
    pub visual_reference_width_pt: Option<f32>,
    #[serde(default)]
    pub absolute_delta_pt: Option<f32>,
    pub trust_class: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionBoxTrustClassifierSummary {
    pub rows: Vec<RedactionBoxTrustClassifierRow>,
    pub trust_counts: Vec<FamilyCount>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopKFamilyEntropyRow {
    pub dataset: String,
    pub label: String,
    pub target: String,
    #[serde(default)]
    pub row_key: Option<String>,
    #[serde(default)]
    pub entropy_top5: Option<f64>,
    #[serde(default)]
    pub entropy_top10: Option<f64>,
    #[serde(default)]
    pub entropy_top20: Option<f64>,
    #[serde(default)]
    pub dominant_family_top5: Option<String>,
    #[serde(default)]
    pub dominant_family_share_top5: Option<f64>,
    #[serde(default)]
    pub target_family: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopKFamilyEntropySummary {
    pub rows: Vec<TopKFamilyEntropyRow>,
    #[serde(default)]
    pub mean_entropy_top5: Option<f64>,
    #[serde(default)]
    pub mean_entropy_top10: Option<f64>,
    #[serde(default)]
    pub mean_entropy_top20: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualReviewPackSummary {
    pub manifest_path: PathBuf,
    pub item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BestPossibleRankRow {
    pub dataset: String,
    pub label: String,
    pub target: String,
    #[serde(default)]
    pub current_rank: Option<usize>,
    #[serde(default)]
    pub exact_oracle_rank: Option<usize>,
    #[serde(default)]
    pub same_family_rank: Option<usize>,
    #[serde(default)]
    pub plain_multi_token_rank: Option<usize>,
    #[serde(default)]
    pub no_comma_single_rank: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BestPossibleRankSummary {
    pub rows: Vec<BestPossibleRankRow>,
    pub improvable_by_same_family: usize,
    pub improvable_by_plain_multi_token: usize,
    pub improvable_by_no_comma_single: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairwiseWinnerExplanation {
    pub dataset: String,
    pub label: String,
    pub target: String,
    #[serde(default)]
    pub row_key: Option<String>,
    pub reason: String,
    #[serde(default)]
    pub top1_text: Option<String>,
    #[serde(default)]
    pub target_error_pt: Option<f32>,
    #[serde(default)]
    pub top1_error_pt: Option<f32>,
    #[serde(default)]
    pub error_delta_pt: Option<f32>,
    #[serde(default)]
    pub top1_family: Option<String>,
    #[serde(default)]
    pub target_family: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairwiseWinnerSummary {
    pub rows: Vec<PairwiseWinnerExplanation>,
    pub reasons: Vec<FamilyCount>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TieDensityRow {
    pub dataset: String,
    pub label: String,
    pub target: String,
    #[serde(default)]
    pub row_key: Option<String>,
    #[serde(default)]
    pub within_target_005: Option<usize>,
    #[serde(default)]
    pub within_target_010: Option<usize>,
    #[serde(default)]
    pub within_target_025: Option<usize>,
    #[serde(default)]
    pub within_target_050: Option<usize>,
    #[serde(default)]
    pub within_target_100: Option<usize>,
    #[serde(default)]
    pub within_top1_005: Option<usize>,
    #[serde(default)]
    pub within_top1_010: Option<usize>,
    #[serde(default)]
    pub within_top1_025: Option<usize>,
    #[serde(default)]
    pub within_top1_050: Option<usize>,
    #[serde(default)]
    pub within_top1_100: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TieDensitySummary {
    pub rows: Vec<TieDensityRow>,
    #[serde(default)]
    pub mean_within_target_050: Option<f64>,
    #[serde(default)]
    pub mean_within_top1_050: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerturbationRobustnessRow {
    pub dataset: String,
    pub label: String,
    pub target: String,
    #[serde(default)]
    pub row_key: Option<String>,
    #[serde(default)]
    pub baseline_top1: Option<String>,
    pub changed_at_025: bool,
    pub changed_at_050: bool,
    pub changed_at_100: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerturbationRobustnessSummary {
    pub rows: Vec<PerturbationRobustnessRow>,
    pub changed_at_025: usize,
    pub changed_at_050: usize,
    pub changed_at_100: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StabilityDatasetSummary {
    pub dataset: String,
    pub repeats: usize,
    pub all_hashes_identical: bool,
    pub top1_agreement_ratio: f64,
    #[serde(default)]
    pub mean_rank_stddev: Option<f64>,
    pub unstable_targets: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StabilitySummary {
    pub repeats: usize,
    pub all_hashes_identical: bool,
    pub per_dataset: Vec<StabilityDatasetSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualBenchmarkStageSummary {
    pub summary_path: PathBuf,
    pub rows_path: PathBuf,
    pub experiments_dir: PathBuf,
    pub crops_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectedGuessRow {
    pub row_key: String,
    pub dataset: String,
    pub page_index: u32,
    pub bbox: Rect,
    pub candidates: Vec<crate::types::guess_types::GuessCandidate>,
    #[serde(default)]
    pub anchor_mode: Option<String>,
    pub target_width_pt: f32,
    #[serde(default)]
    pub top1_text: Option<String>,
}
