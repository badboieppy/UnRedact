use crate::data::fonts_data::FontsData;
use crate::data::redactions_data::RedactionsData;
use crate::logic::time::Instant;
use crate::logic::types::{
    BytesPipelineOutputs, BytesPipelineRequest, PipelineConfig, VisualizationPayload,
};
use crate::types::file_types::FontDetectionReport;
use crate::types::guess_types::GuessReport;
use crate::types::redaction_types::{RedactionFinderConfig, RedactionMode, RedactionReport};

const INCLUDE_FULL_PAGE_RECTS: bool = false;

struct RedactionStageOutput {
    redactions: RedactionReport,
    elapsed_ms: u128,
}

struct FontStageOutput {
    fonts: FontDetectionReport,
    elapsed_ms: u128,
}

struct GuessStageOutput {
    guesses: GuessReport,
    elapsed_ms: u128,
}

struct VisualizationPayloadStageOutput {
    payload: Option<VisualizationPayload>,
}

#[inline]
pub fn run_redaction_guessing_component(
    req: BytesPipelineRequest,
) -> Result<BytesPipelineOutputs, String> {
    let BytesPipelineRequest {
        input_name,
        pdf_bytes,
        dictionary_entries,
        dictionary_diagnostics,
        cfg,
    } = req;
    let component_started = Instant::now();
    let redactions_data = RedactionsData::new();
    let fonts_data = FontsData::new();

    let redaction_stage = run_redaction_stage(&input_name, &pdf_bytes, &cfg, &redactions_data)?;
    let font_stage = run_font_stage(&input_name, &pdf_bytes, cfg.include_details, &fonts_data)?;
    let mut guess_stage = run_guess_stage(
        &input_name,
        &pdf_bytes,
        &redaction_stage.redactions,
        &dictionary_entries,
        &dictionary_diagnostics,
        &cfg,
    )?;
    guess_stage.guesses.diagnostics.push(format!(
        "timing_ms stage=redactions value={}",
        redaction_stage.elapsed_ms
    ));
    guess_stage.guesses.diagnostics.push(format!(
        "timing_ms stage=fonts value={}",
        font_stage.elapsed_ms
    ));
    guess_stage.guesses.diagnostics.push(format!(
        "timing_ms stage=guess value={}",
        guess_stage.elapsed_ms
    ));

    let visualization_stage = build_visualization_payload_stage(
        &input_name,
        pdf_bytes,
        &cfg,
        &fonts_data,
        &mut guess_stage.guesses,
    )?;

    guess_stage.guesses.diagnostics.push(format!(
        "timing_ms stage=orchestrator_total value={}",
        component_started.elapsed().as_millis()
    ));

    Ok(BytesPipelineOutputs {
        redactions: redaction_stage.redactions,
        fonts: font_stage.fonts,
        guesses: guess_stage.guesses,
        visualization_payload: visualization_stage.payload,
        visualized_pdf_bytes: None,
    })
}

fn run_redaction_stage(
    input_name: &str,
    pdf_bytes: &[u8],
    cfg: &PipelineConfig,
    redactions_data: &RedactionsData,
) -> Result<RedactionStageOutput, String> {
    let started = Instant::now();
    let redaction_cfg = RedactionFinderConfig {
        include_details: cfg.include_details,
        mode: RedactionMode::All,
        include_full_page_rects: INCLUDE_FULL_PAGE_RECTS,
        enable_image_analysis: cfg.enable_image_analysis,
        raster_dpi: cfg.raster_dpi,
    };
    let output = if cfg.enable_image_analysis {
        let renderer = redactions_data.build_renderer(pdf_bytes)?;
        run_redaction_scan_from_bytes(pdf_bytes, Some(&renderer), redaction_cfg)?
    } else {
        run_redaction_scan_from_bytes(pdf_bytes, None, redaction_cfg)?
    };
    Ok(RedactionStageOutput {
        redactions: build_report_from_input_name(input_name, output),
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn run_font_stage(
    input_name: &str,
    pdf_bytes: &[u8],
    include_details: bool,
    fonts_data: &FontsData,
) -> Result<FontStageOutput, String> {
    let started = Instant::now();
    let fonts = fonts_data.detect_fonts_from_bytes(input_name, pdf_bytes, include_details)?;
    Ok(FontStageOutput {
        fonts,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn run_guess_stage(
    input_name: &str,
    pdf_bytes: &[u8],
    redactions: &RedactionReport,
    dictionary_entries: &[String],
    dictionary_diagnostics: &[String],
    cfg: &PipelineConfig,
) -> Result<GuessStageOutput, String> {
    let started = Instant::now();
    let guesses = run_guess_from_bytes(RunGuessFromBytesRequest {
        pdf_name: input_name,
        pdf_bytes,
        redactions,
        dictionary: dictionary_entries,
        diagnostics: dictionary_diagnostics,
        cfg: &cfg.guess,
    })?;
    Ok(GuessStageOutput {
        guesses,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn build_visualization_payload_stage(
    input_name: &str,
    pdf_bytes: Vec<u8>,
    cfg: &PipelineConfig,
    fonts_data: &FontsData,
    guess_report: &mut GuessReport,
) -> Result<VisualizationPayloadStageOutput, String> {
    if !cfg.visualize {
        return Ok(VisualizationPayloadStageOutput { payload: None });
    }
    let started = Instant::now();
    let font_runs = fonts_data
        .load_font_runs_from_bytes(input_name, &pdf_bytes)?
        .report;
    guess_report.diagnostics.push(format!(
        "timing_ms stage=visualize_payload value={}",
        started.elapsed().as_millis()
    ));
    Ok(VisualizationPayloadStageOutput {
        payload: Some(VisualizationPayload {
            pdf_bytes,
            font_runs,
        }),
    })
}

mod guess_impl {
    use lopdf::{Dictionary, Document, Object};
    use std::sync::OnceLock;

    use super::visual_guess_score_impl::{apply_visual_scores_from_bytes, VisualGuessScoreConfig};
    use crate::dependency::pdf_font_run_accessor::build_font_run_report_from_input_name;
    use crate::logic::time::Instant;
    use crate::types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect as FontRect};
    use crate::types::guess_types::{
        GuessCandidate, GuessConfig, GuessContext, GuessReport, RedactionGuess,
    };
    use crate::types::redaction_types::{
        Rect, RedactionKind, RedactionOccurrence, RedactionReport,
    };

    pub struct RunGuessFromBytesRequest<'a> {
        pub pdf_name: &'a str,
        pub pdf_bytes: &'a [u8],
        pub redactions: &'a RedactionReport,
        pub dictionary: &'a [String],
        pub diagnostics: &'a [String],
        pub cfg: &'a GuessConfig,
    }

    #[inline]
    pub fn run_from_bytes(req: RunGuessFromBytesRequest<'_>) -> Result<GuessReport, String> {
        let started = Instant::now();
        let font_runs_started = Instant::now();
        let font_runs = build_font_run_report_from_input_name(req.pdf_name, req.pdf_bytes)?;
        let font_runs_ms = font_runs_started.elapsed().as_millis();
        let width_tables_started = Instant::now();
        let width_tables = build_pdf_width_table_map_from_bytes(req.pdf_bytes).unwrap_or_default();
        let width_tables_ms = width_tables_started.elapsed().as_millis();
        let mut diagnostics = req.diagnostics.to_vec();
        diagnostics.push(format!(
            "timing_ms stage=guess_font_runs value={font_runs_ms}"
        ));
        diagnostics.push(format!(
            "timing_ms stage=guess_width_tables value={width_tables_ms}"
        ));
        let inputs = BuildReportWithFontsInputs {
            input_redactions_label: format!("memory://{}.redactions.json", req.pdf_name),
            input_fonts_label: format!("memory://{}.fonts.json", req.pdf_name),
            redactions: req.redactions.clone(),
            dictionary: req.dictionary.to_vec(),
            diagnostics,
            font_runs,
            width_tables,
            pdf_bytes: Some(req.pdf_bytes),
        };
        let mut report = build_report_from_parts_with_fonts_inputs(inputs, req.cfg);
        report.diagnostics.push(format!(
            "timing_ms stage=guess_run_from_bytes_total value={}",
            started.elapsed().as_millis()
        ));
        Ok(report)
    }

    struct BuildReportWithFontsInputs<'a> {
        input_redactions_label: String,
        input_fonts_label: String,
        redactions: RedactionReport,
        dictionary: Vec<String>,
        diagnostics: Vec<String>,
        font_runs: FontRunReport,
        width_tables: std::collections::BTreeMap<WidthTableKey, WidthTable>,
        pdf_bytes: Option<&'a [u8]>,
    }

    fn build_report_from_parts_with_fonts_inputs(
        inputs: BuildReportWithFontsInputs<'_>,
        cfg: &GuessConfig,
    ) -> GuessReport {
        let guess_anchor_started = Instant::now();
        let (mut guesses, guess_diagnostics) = build_anchor_validated_guesses(
            &inputs.redactions.redactions,
            &inputs.dictionary,
            &inputs.font_runs,
            &inputs.width_tables,
            cfg,
        );
        let guess_anchor_ms = guess_anchor_started.elapsed().as_millis();
        let mut all_diagnostics = inputs.diagnostics;
        all_diagnostics.extend(guess_diagnostics);
        all_diagnostics.push(format!(
            "timing_ms stage=guess_anchor_rows value={guess_anchor_ms}"
        ));
        if cfg.visual_score {
            let visual_started = Instant::now();
            let visual_cfg = VisualGuessScoreConfig {
                enabled: cfg.visual_score,
                dpi: cfg.visual_score_dpi,
                min_ink_pixels: FIXED_VISUAL_MIN_INK_PIXELS,
                drop_threshold: FIXED_VISUAL_DROP_THRESHOLD,
            };
            let visual_result = if let Some(pdf_bytes) = inputs.pdf_bytes {
                apply_visual_scores_from_bytes(
                    pdf_bytes,
                    &inputs.redactions,
                    &inputs.font_runs,
                    &mut guesses,
                    visual_cfg,
                )
            } else {
                all_diagnostics.push("visual_score=disabled_missing_pdf_bytes".to_owned());
                Ok(Vec::new())
            };
            match visual_result {
                Ok(visual_diagnostics) => all_diagnostics.extend(visual_diagnostics),
                Err(error) => {
                    all_diagnostics.push(format!("visual_score_failed:{error}"));
                }
            }
            all_diagnostics.push(format!(
                "timing_ms stage=guess_visual_score value={}",
                visual_started.elapsed().as_millis()
            ));
        } else {
            all_diagnostics.push("visual_score=disabled".to_owned());
        }
        annotate_guess_confidence(&mut guesses);
        GuessReport {
            input_redactions: inputs.input_redactions_label,
            input_fonts: inputs.input_fonts_label,
            guesses,
            diagnostics: all_diagnostics,
        }
    }

    fn annotate_guess_confidence(guesses: &mut [RedactionGuess]) {
        for guess in guesses {
            let base = if !guess.exact_matches.is_empty() {
                1.0_f64
            } else {
                guess
                    .candidates
                    .first()
                    .map(|candidate| candidate.score as f64)
                    .unwrap_or(0.0_f64)
            };
            let anchor = if !guess.context.has_anchor_pair {
                0.35_f64
            } else if guess.context.anchor_mode.as_deref() == Some("two_sided") {
                1.0_f64
            } else {
                0.78_f64
            };
            let width = match guess
                .context
                .candidate_width_source
                .as_deref()
                .or(guess.context.anchor_width_source.as_deref())
            {
                Some("asset") => 1.0_f64,
                Some("pdf_width_table") => 0.93_f64,
                Some("core_font") => 0.82_f64,
                Some("fallback") => 0.64_f64,
                _ => 0.75_f64,
            };
            let visual = guess
                .visual_mean_abs_diff
                .map(|value| (1.0_f64 - (value as f64 / 0.28_f64)).clamp(0.30_f64, 1.0_f64))
                .unwrap_or(0.78_f64);
            let fallback_penalty = if guess.context.width_fallback_reason.is_some() {
                0.06_f64
            } else {
                0.0_f64
            };
            let confidence =
                (base * 0.50_f64 + anchor * 0.20_f64 + width * 0.20_f64 + visual * 0.10_f64
                    - fallback_penalty)
                    .clamp(0.0_f64, 1.0_f64);
            guess.context.confidence_score = Some(confidence as f32);
            guess.context.confidence_factors = Some(format!(
                "base={base:.3};anchor={anchor:.3};width={width:.3};visual={visual:.3};fallback_penalty={fallback_penalty:.3}"
            ));
        }
    }

    #[derive(Debug, Clone)]
    enum AnchorMode {
        TwoSided,
        LeftOnly,
        RightOnly,
    }

    impl AnchorMode {
        fn as_str(&self) -> &'static str {
            match self {
                AnchorMode::TwoSided => "two_sided",
                AnchorMode::LeftOnly => "left_only",
                AnchorMode::RightOnly => "right_only",
            }
        }
    }

    #[derive(Debug, Clone)]
    struct AnchorPairData {
        left_anchor_text: String,
        right_anchor_text: String,
        left_x: f64,
        right_x: f64,
        font_key: String,
        font_name: String,
        font_size_pt: f32,
        h_scale_pct: f32,
        left_bbox: FontRect,
        right_bbox: FontRect,
        epsilon_pt: f64,
        row_bias_pt: f64,
        mode: AnchorMode,
    }

    #[derive(Debug, Clone)]
    struct ScoredDictionaryCandidate {
        text: String,
        raw_error_pt: f64,
        effective_error_pt: f64,
        word_count: u32,
        width_pt: f64,
        width_source: WidthSource,
    }

    #[derive(Debug, Clone, Copy, Default)]
    struct CandidateFunnelMetrics {
        scanned: usize,
        after_char_units: usize,
        after_context: usize,
        after_shape: usize,
        after_anchor: usize,
        after_box: usize,
        scored: usize,
    }

    #[derive(Debug, Clone)]
    struct JointAssignmentOption {
        text: String,
        key: String,
        base_cost: f64,
        start_x_pt: f64,
        end_x_pt: f64,
    }

    #[derive(Debug, Clone)]
    struct JointAssignmentBeamState {
        cost: f64,
        selected: Vec<Option<String>>,
        used_keys: Vec<String>,
        prev_start_x_pt: f64,
        prev_end_x_pt: f64,
    }

    #[derive(Debug, Clone)]
    struct PairCandidate {
        left_idx: usize,
        right_idx: usize,
        font_penalty: u8,
        hint_penalty: u8,
        contains_center_penalty: u8,
        baseline_distance: f64,
        y_distance: f64,
        x_distance: f64,
        gap_width: f64,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct WidthTableKey {
        page_index: u32,
        font_key: String,
    }

    #[derive(Debug, Clone)]
    struct WidthTable {
        first_char: u16,
        widths: Vec<f64>,
    }

    const DEFAULT_METRICS_DPI: f32 = 200.0_f32;
    const FIXED_MAX_CANDIDATES: usize = 50;
    const FIXED_TOLERANCE_PT: f64 = 4.0_f64;
    const FIXED_VISUAL_MIN_INK_PIXELS: u32 = 64_u32;
    const FIXED_VISUAL_DROP_THRESHOLD: Option<f32> = None;
    const GLYPH_UNITS_SCALE: f64 = 64.0_f64;
    const MULTI_SPAN_GAP_RATIO_THRESHOLD: f64 = 2.0_f64;
    const MULTI_SPAN_ANCHOR_PRIOR_WEIGHT: f64 = 0.15_f64;
    const MULTI_SPAN_BOX_PRIOR_WEIGHT: f64 = 0.70_f64;
    const SINGLE_SPAN_BOX_PRIOR_WEIGHT: f64 = 0.12_f64;
    const RASTER_WIDTH_NOISE_PT: f64 = 2.50_f64;
    const MULTI_SPAN_BOX_ERROR_RATIO: f64 = 0.45_f64;
    const MULTI_SPAN_BOX_ERROR_PAD_PT: f64 = 2.5_f64;
    const CLUSTER_CONSENSUS_MAX_GAP_RATIO: f64 = 1.9_f64;
    const MULTI_SPAN_WIDTH_BAND_LIMIT: usize = 900;
    const SINGLE_SPAN_WIDTH_BAND_LIMIT: usize = 700;
    const JOINT_ASSIGNMENT_MIN_GROUP_ROWS: usize = 2;
    const JOINT_ASSIGNMENT_MAX_ROWS: usize = 14;
    const JOINT_ASSIGNMENT_MAX_OPTIONS_PER_ROW: usize = 24;
    const JOINT_ASSIGNMENT_OPTION_SCAN_LIMIT: usize = 500;
    const JOINT_ASSIGNMENT_BEAM_WIDTH: usize = 160;
    const JOINT_ASSIGNMENT_DUPLICATE_PENALTY: f64 = 6.0_f64;
    const JOINT_ASSIGNMENT_OVERLAP_MARGIN_PT: f64 = 0.75_f64;
    const JOINT_ASSIGNMENT_OVERLAP_PENALTY: f64 = 2.8_f64;
    const JOINT_ASSIGNMENT_MAX_GROUP_GAP_PT: f64 = 140.0_f64;
    const JOINT_ASSIGNMENT_NULL_DELTA: f64 = 0.75_f64;
    const JOINT_ASSIGNMENT_NULL_MIN_BEST_COST: f64 = 1.4_f64;
    const MAX_NAME_VARIANTS_PER_ENTRY: usize = 24;
    const NAME_PREFIX_TOKENS: [&str; 24] = [
        "mr",
        "mrs",
        "ms",
        "miss",
        "mx",
        "dr",
        "prof",
        "sir",
        "madam",
        "lady",
        "lord",
        "rev",
        "fr",
        "hon",
        "judge",
        "capt",
        "captain",
        "lt",
        "col",
        "gen",
        "adm",
        "pres",
        "president",
        "governor",
    ];
    const NAME_SUFFIX_TOKENS: [&str; 24] = [
        "jr", "sr", "ii", "iii", "iv", "v", "vi", "phd", "md", "esq", "esquire", "jd", "dds",
        "dmd", "do", "rn", "cpa", "mba", "qc", "kc", "ret", "retired", "junior", "senior",
    ];
    const NAME_SURNAME_PARTICLE_TOKENS: [&str; 28] = [
        "al", "ap", "ben", "bin", "da", "dal", "de", "del", "dela", "della", "der", "di", "dos",
        "du", "el", "ibn", "la", "le", "st", "st.", "ter", "van", "vanden", "vander", "von", "zu",
        "zum", "zur",
    ];

    #[derive(Debug, Clone, Copy)]
    struct MeasuredWidth {
        pt: f64,
        source: WidthSource,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum WidthSource {
        Asset,
        PdfWidthTable,
        CoreFont,
        Fallback,
    }

    impl WidthSource {
        fn as_str(self) -> &'static str {
            match self {
                WidthSource::Asset => "asset",
                WidthSource::PdfWidthTable => "pdf_width_table",
                WidthSource::CoreFont => "core_font",
                WidthSource::Fallback => "fallback",
            }
        }
    }

    struct WidthMeasureContext<'a> {
        page_index: u32,
        asset: Option<&'a FontAsset>,
        width_tables: &'a std::collections::BTreeMap<WidthTableKey, WidthTable>,
        h_scale_pct: f32,
    }

    struct TextMeasureInput<'a> {
        page_index: u32,
        font_key: &'a str,
        font_name: &'a str,
        font_size_pt: f32,
        h_scale_pct: f32,
        text: &'a str,
        metrics_dpi: f32,
    }

    struct RowCalibration {
        epsilon_pt: f64,
        bias_pt: f64,
    }

    struct AnchorHints<'a> {
        left_text: Option<&'a str>,
        left_x: Option<f64>,
        right_text: Option<&'a str>,
        right_x: Option<f64>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct GuessClusterKey {
        page_index: u32,
        left_anchor_text: String,
        right_anchor_text: String,
        font_key: String,
        font_size_bits: u32,
        h_scale_bits: u32,
    }

    fn build_anchor_validated_guesses(
        redactions: &[RedactionOccurrence],
        dictionary: &[String],
        font_runs: &FontRunReport,
        width_tables: &std::collections::BTreeMap<WidthTableKey, WidthTable>,
        cfg: &GuessConfig,
    ) -> (Vec<RedactionGuess>, Vec<String>) {
        let assets = font_runs
            .assets
            .iter()
            .map(|asset| (asset.font_key.clone(), asset.clone()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut by_page = std::collections::BTreeMap::<u32, Vec<&FontTextRun>>::new();
        for run in &font_runs.runs {
            by_page.entry(run.page_index).or_default().push(run);
        }
        for runs in by_page.values_mut() {
            runs.sort_by(|left_run, right_run| {
                left_run
                    .bbox
                    .x0
                    .partial_cmp(&right_run.bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        left_run
                            .bbox
                            .y0
                            .partial_cmp(&right_run.bbox.y0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| left_run.text.cmp(&right_run.text))
            });
        }

        let mut cache = WidthCache::new();
        let mut diagnostics = vec![format!(
            "font_run_count={} font_asset_count={} width_table_count={}",
            font_runs.runs.len(),
            font_runs.assets.len(),
            width_tables.len()
        )];
        let mut guesses = Vec::with_capacity(redactions.len());

        for (index, redaction) in redactions.iter().enumerate() {
            let (left_text, right_text, left_bbox, right_bbox) = extract_context(redaction);
            let page_runs = by_page
                .get(&redaction.page_index)
                .cloned()
                .unwrap_or_default();
            let anchor = select_anchor_pair(redaction, &page_runs, &assets, width_tables);
            let Some(anchor) = anchor else {
                diagnostics.push(format!(
                    "redaction_index={index} page_index={} anchored_row=false reason=missing_anchor",
                    redaction.page_index
                ));
                guesses.push(RedactionGuess {
                    page_index: redaction.page_index,
                    bbox: redaction.bbox,
                    candidates: Vec::new(),
                    exact_matches: Vec::new(),
                    context: GuessContext {
                        left_anchor_text: left_text,
                        right_anchor_text: right_text,
                        gap_pt: compute_gap_pt(
                            redaction.bbox,
                            left_bbox,
                            right_bbox,
                            redaction
                                .underlying_text
                                .first()
                                .map(|hit| hit.text.as_str())
                                .unwrap_or_default(),
                            redaction
                                .underlying_text
                                .get(1)
                                .map(|hit| hit.text.as_str())
                                .unwrap_or_default(),
                        ) as f32,
                        char_width_pt: estimate_char_width_pt(
                            redaction
                                .underlying_text
                                .first()
                                .map(|hit| hit.text.as_str())
                                .unwrap_or_default(),
                            redaction
                                .underlying_text
                                .get(1)
                                .map(|hit| hit.text.as_str())
                                .unwrap_or_default(),
                            left_bbox,
                            right_bbox,
                            redaction.bbox,
                        ) as f32,
                        tol_pt: FIXED_TOLERANCE_PT as f32,
                        anchor_left_x: None,
                        anchor_right_x: None,
                        anchor_font_key: None,
                        anchor_font_name: None,
                        anchor_font_size_pt: None,
                        anchor_h_scale_pct: None,
                        anchor_row_bias_pt: None,
                        anchor_mode: None,
                        anchor_width_source: None,
                        space_width_source: None,
                        candidate_width_source: None,
                        width_fallback_reason: None,
                        confidence_score: None,
                        confidence_factors: None,
                        has_anchor_pair: false,
                    },
                    visual_compared_pixels: None,
                    visual_mean_abs_diff: None,
                    visual_changed_pixel_ratio: None,
                    visual_reason: None,
                    visual_dropped: false,
                });
                continue;
            };

            let (guess, funnel) = build_guess_for_anchor(
                redaction,
                dictionary,
                cfg,
                &anchor,
                &assets,
                width_tables,
                &mut cache,
            );
            diagnostics.push(format!(
                "redaction_index={index} page_index={} anchor_mode={} funnel_scanned={} funnel_after_char_units={} funnel_after_context={} funnel_after_shape={} funnel_after_anchor={} funnel_after_box={} funnel_scored={}",
                redaction.page_index,
                anchor.mode.as_str(),
                funnel.scanned,
                funnel.after_char_units,
                funnel.after_context,
                funnel.after_shape,
                funnel.after_anchor,
                funnel.after_box,
                funnel.scored,
            ));
            guesses.push(guess);
        }

        apply_cluster_consensus(&mut guesses);
        let jointly_assigned = apply_row_joint_assignment(&mut guesses);
        apply_row_sequence_consensus(&mut guesses, &jointly_assigned);
        for (index, guess) in guesses.iter().enumerate() {
            if !guess.context.has_anchor_pair {
                continue;
            }
            let top = if let Some(first) = guess.exact_matches.first() {
                first.clone()
            } else {
                guess
                    .candidates
                    .first()
                    .map(|candidate| candidate.text.clone())
                    .unwrap_or_default()
            };
            diagnostics.push(format!(
                "redaction_index={index} page_index={} anchored_row=true exact_count={} candidate_count={} top_guess={} left_anchor=[{}] right_anchor=[{}] tol_pt={} anchor_mode={} anchor_width_source={} space_width_source={} candidate_width_source={} width_fallback_reason={}",
                guess.page_index,
                guess.exact_matches.len(),
                guess.candidates.len(),
                top,
                guess.context.left_anchor_text,
                guess.context.right_anchor_text,
                guess.context.tol_pt,
                guess.context
                    .anchor_mode
                    .as_deref()
                    .unwrap_or("unknown"),
                guess.context
                    .anchor_width_source
                    .as_deref()
                    .unwrap_or("unknown"),
                guess.context
                    .space_width_source
                    .as_deref()
                    .unwrap_or("unknown"),
                guess.context
                    .candidate_width_source
                    .as_deref()
                    .unwrap_or("unknown"),
                guess.context
                    .width_fallback_reason
                    .as_deref()
                    .unwrap_or("none"),
            ));
        }

        (guesses, diagnostics)
    }

    fn apply_cluster_consensus(guesses: &mut [RedactionGuess]) {
        let mut clusters = std::collections::BTreeMap::<GuessClusterKey, Vec<usize>>::new();
        for (index, guess) in guesses.iter().enumerate() {
            if !guess.context.has_anchor_pair || guess.candidates.is_empty() {
                continue;
            }
            if !is_two_sided_anchor_context(guess) {
                continue;
            }
            let redaction_width = (guess.bbox.width().abs() as f64).max(1.0_f64);
            let gap_ratio = (guess.context.gap_pt as f64).abs() / redaction_width;
            if gap_ratio >= CLUSTER_CONSENSUS_MAX_GAP_RATIO {
                continue;
            }
            let Some(font_key) = guess.context.anchor_font_key.clone() else {
                continue;
            };
            let Some(font_size_pt) = guess.context.anchor_font_size_pt else {
                continue;
            };
            let Some(h_scale_pct) = guess.context.anchor_h_scale_pct else {
                continue;
            };
            let key = GuessClusterKey {
                page_index: guess.page_index,
                left_anchor_text: guess.context.left_anchor_text.clone(),
                right_anchor_text: guess.context.right_anchor_text.clone(),
                font_key,
                font_size_bits: font_size_pt.to_bits(),
                h_scale_bits: h_scale_pct.to_bits(),
            };
            clusters.entry(key).or_default().push(index);
        }

        for indices in clusters.values() {
            if indices.len() < 2 {
                continue;
            }

            let mut aggregate = std::collections::BTreeMap::<String, (f64, u32)>::new();
            for index in indices {
                let guess = &guesses[*index];
                let denom = (guess.context.tol_pt as f64).max(0.0001_f64);
                for candidate in &guess.candidates {
                    let entry = aggregate
                        .entry(candidate.text.clone())
                        .or_insert((0.0_f64, 0_u32));
                    entry.0 += (candidate.error_pt as f64) / denom;
                    entry.1 += 1;
                }
            }

            let cluster_size = indices.len() as u32;
            for index in indices {
                let guess = &mut guesses[*index];
                let mut local_error = std::collections::BTreeMap::<String, f64>::new();
                for candidate in &guess.candidates {
                    local_error.insert(candidate.text.clone(), candidate.error_pt as f64);
                }

                guess.candidates.sort_by(|left_candidate, right_candidate| {
                    let left_local = left_candidate.error_pt as f64;
                    let right_local = right_candidate.error_pt as f64;
                    let left_consensus = aggregate
                        .get(&left_candidate.text)
                        .map(|(score, count)| {
                            (*score / *count as f64)
                                + (cluster_size.saturating_sub(*count) as f64 * 1.5_f64)
                        })
                        .unwrap_or(f64::INFINITY);
                    let right_consensus = aggregate
                        .get(&right_candidate.text)
                        .map(|(score, count)| {
                            (*score / *count as f64)
                                + (cluster_size.saturating_sub(*count) as f64 * 1.5_f64)
                        })
                        .unwrap_or(f64::INFINITY);
                    left_consensus
                        .partial_cmp(&right_consensus)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| {
                            left_local
                                .partial_cmp(&right_local)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .then_with(|| {
                            left_candidate
                                .score
                                .partial_cmp(&right_candidate.score)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .then_with(|| left_candidate.word_count.cmp(&right_candidate.word_count))
                        .then_with(|| left_candidate.text.cmp(&right_candidate.text))
                });

                guess.exact_matches.sort_by(|left_text, right_text| {
                    let left_local = local_error.get(left_text).copied().unwrap_or(f64::INFINITY);
                    let right_local = local_error
                        .get(right_text)
                        .copied()
                        .unwrap_or(f64::INFINITY);
                    let left_consensus = aggregate
                        .get(left_text)
                        .map(|(score, count)| {
                            (*score / *count as f64)
                                + (cluster_size.saturating_sub(*count) as f64 * 1.5_f64)
                        })
                        .unwrap_or(f64::INFINITY);
                    let right_consensus = aggregate
                        .get(right_text)
                        .map(|(score, count)| {
                            (*score / *count as f64)
                                + (cluster_size.saturating_sub(*count) as f64 * 1.5_f64)
                        })
                        .unwrap_or(f64::INFINITY);
                    left_consensus
                        .partial_cmp(&right_consensus)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| {
                            left_local
                                .partial_cmp(&right_local)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .then_with(|| left_text.cmp(right_text))
                });
            }
        }
    }

    fn is_multi_span_row_guess(guess: &RedactionGuess) -> bool {
        if !guess.context.has_anchor_pair {
            return false;
        }
        if !is_two_sided_anchor_context(guess) {
            return false;
        }
        let width = guess.bbox.width().abs() as f64;
        if width <= 0.0_f64 {
            return false;
        }
        (guess.context.gap_pt as f64).abs() / width >= MULTI_SPAN_GAP_RATIO_THRESHOLD
    }

    fn is_two_sided_anchor_context(guess: &RedactionGuess) -> bool {
        guess
            .context
            .anchor_mode
            .as_deref()
            .map(|mode| mode == "two_sided")
            .unwrap_or(true)
    }

    fn apply_row_joint_assignment(
        guesses: &mut [RedactionGuess],
    ) -> std::collections::BTreeSet<usize> {
        let mut rows = std::collections::BTreeMap::<(u32, i32), Vec<usize>>::new();
        for (index, guess) in guesses.iter().enumerate() {
            if !guess.context.has_anchor_pair || guess.candidates.is_empty() {
                continue;
            }
            let center_y = ((guess.bbox.y0 + guess.bbox.y1) * 0.5_f32) as f64;
            let y_bucket = (center_y / 6.0_f64).round() as i32;
            rows.entry((guess.page_index, y_bucket))
                .or_default()
                .push(index);
        }

        let mut promotions = Vec::<(usize, String)>::new();
        for indices in rows.values_mut() {
            if indices.len() < JOINT_ASSIGNMENT_MIN_GROUP_ROWS {
                continue;
            }
            indices.sort_by(|left_idx, right_idx| {
                guesses[*left_idx]
                    .bbox
                    .x0
                    .partial_cmp(&guesses[*right_idx].bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        guesses[*left_idx]
                            .bbox
                            .x1
                            .partial_cmp(&guesses[*right_idx].bbox.x1)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });
            let groups = collect_contiguous_multi_span_groups(indices, guesses);
            for group in groups {
                if let Some(selected) = solve_joint_assignment_group(guesses, &group) {
                    for (guess_index, selected_text) in
                        group.iter().copied().zip(selected.into_iter())
                    {
                        if let Some(text) = selected_text {
                            promotions.push((guess_index, text));
                        }
                    }
                }
            }
        }

        let mut assigned = std::collections::BTreeSet::<usize>::new();
        for (guess_index, selected_text) in promotions {
            if let Some(guess) = guesses.get_mut(guess_index) {
                promote_text_to_front(guess, &selected_text);
                assigned.insert(guess_index);
            }
        }
        assigned
    }

    fn collect_contiguous_multi_span_groups(
        indices: &[usize],
        guesses: &[RedactionGuess],
    ) -> Vec<Vec<usize>> {
        let mut groups = Vec::<Vec<usize>>::new();
        let mut current = Vec::<usize>::new();

        for guess_index in indices.iter().copied() {
            let guess = &guesses[guess_index];
            if !is_joint_assignment_candidate_row(guess) {
                if current.len() >= JOINT_ASSIGNMENT_MIN_GROUP_ROWS {
                    groups.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                continue;
            }

            if current.is_empty() {
                current.push(guess_index);
                continue;
            }

            let prev_index = *current.last().unwrap_or(&guess_index);
            let prev = &guesses[prev_index];
            let x_gap = (guess.bbox.x0 as f64 - prev.bbox.x1 as f64).max(0.0_f64);
            let contiguous = x_gap <= JOINT_ASSIGNMENT_MAX_GROUP_GAP_PT
                && joint_assignment_rows_are_compatible(prev, guess);
            if contiguous {
                current.push(guess_index);
            } else {
                if current.len() >= JOINT_ASSIGNMENT_MIN_GROUP_ROWS {
                    groups.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
                current.push(guess_index);
            }
        }

        if current.len() >= JOINT_ASSIGNMENT_MIN_GROUP_ROWS {
            groups.push(current);
        }
        groups
            .into_iter()
            .filter(|group| {
                group.len() <= JOINT_ASSIGNMENT_MAX_ROWS
                    && group_has_joint_assignment_signal(group, guesses)
            })
            .collect::<Vec<_>>()
    }

    fn is_joint_assignment_candidate_row(guess: &RedactionGuess) -> bool {
        guess.context.has_anchor_pair
            && !guess.candidates.is_empty()
            && is_two_sided_anchor_context(guess)
    }

    fn group_has_joint_assignment_signal(group: &[usize], guesses: &[RedactionGuess]) -> bool {
        group.iter().copied().any(|guess_index| {
            let guess = &guesses[guess_index];
            is_multi_span_row_guess(guess)
                || is_list_like_context(
                    &guess.context.left_anchor_text,
                    &guess.context.right_anchor_text,
                )
        })
    }

    fn joint_assignment_rows_are_compatible(left: &RedactionGuess, right: &RedactionGuess) -> bool {
        let same_font_key = match (
            left.context.anchor_font_key.as_deref(),
            right.context.anchor_font_key.as_deref(),
        ) {
            (Some(l), Some(r)) => l == r,
            _ => true,
        };
        let similar_font_size = match (
            left.context.anchor_font_size_pt,
            right.context.anchor_font_size_pt,
        ) {
            (Some(l), Some(r)) => (l - r).abs() <= 0.75_f32,
            _ => true,
        };
        let similar_h_scale = match (
            left.context.anchor_h_scale_pct,
            right.context.anchor_h_scale_pct,
        ) {
            (Some(l), Some(r)) => (l - r).abs() <= 8.0_f32,
            _ => true,
        };
        let close_row_bias = match (
            left.context.anchor_row_bias_pt,
            right.context.anchor_row_bias_pt,
        ) {
            (Some(l), Some(r)) => (l - r).abs() <= 5.0_f32,
            _ => true,
        };
        same_font_key && similar_font_size && similar_h_scale && close_row_bias
    }

    fn solve_joint_assignment_group(
        guesses: &[RedactionGuess],
        group: &[usize],
    ) -> Option<Vec<Option<String>>> {
        if group.len() < JOINT_ASSIGNMENT_MIN_GROUP_ROWS || group.len() > JOINT_ASSIGNMENT_MAX_ROWS
        {
            return None;
        }

        let mut options_by_row = Vec::<Vec<JointAssignmentOption>>::with_capacity(group.len());
        let mut null_costs = Vec::<f64>::with_capacity(group.len());
        let mut allow_null_by_row = Vec::<bool>::with_capacity(group.len());
        for guess_index in group.iter().copied() {
            let guess = guesses.get(guess_index)?;
            let options = build_joint_assignment_options(
                guess,
                JOINT_ASSIGNMENT_OPTION_SCAN_LIMIT,
                JOINT_ASSIGNMENT_MAX_OPTIONS_PER_ROW,
            );
            if options.is_empty() {
                return None;
            }
            let best_cost = options
                .first()
                .map(|option| option.base_cost)
                .unwrap_or(5.0_f64);
            null_costs.push(best_cost + JOINT_ASSIGNMENT_NULL_DELTA);
            allow_null_by_row.push(best_cost >= JOINT_ASSIGNMENT_NULL_MIN_BEST_COST);
            options_by_row.push(options);
        }

        let mut beam = vec![JointAssignmentBeamState {
            cost: 0.0_f64,
            selected: Vec::new(),
            used_keys: Vec::new(),
            prev_start_x_pt: f64::NEG_INFINITY,
            prev_end_x_pt: f64::NEG_INFINITY,
        }];
        let duplicate_penalty_amount = if group.len() >= 3 {
            JOINT_ASSIGNMENT_DUPLICATE_PENALTY
        } else {
            0.0_f64
        };

        for ((row_options, null_cost), allow_null) in options_by_row
            .iter()
            .zip(null_costs.iter().copied())
            .zip(allow_null_by_row.iter().copied())
        {
            let mut next = Vec::<JointAssignmentBeamState>::new();
            for state in &beam {
                if allow_null {
                    let mut selected_skip = state.selected.clone();
                    selected_skip.push(None);
                    next.push(JointAssignmentBeamState {
                        cost: state.cost + null_cost,
                        selected: selected_skip,
                        used_keys: state.used_keys.clone(),
                        prev_start_x_pt: state.prev_start_x_pt,
                        prev_end_x_pt: state.prev_end_x_pt,
                    });
                }
                for option in row_options {
                    let mut cost = state.cost + option.base_cost;
                    if duplicate_penalty_amount > 0.0_f64
                        && state.used_keys.iter().any(|key| key == &option.key)
                    {
                        cost += duplicate_penalty_amount;
                    }
                    if state.prev_end_x_pt.is_finite() {
                        let overlap_pt = (state.prev_end_x_pt - option.start_x_pt
                            + JOINT_ASSIGNMENT_OVERLAP_MARGIN_PT)
                            .max(0.0_f64);
                        cost += overlap_pt * JOINT_ASSIGNMENT_OVERLAP_PENALTY;
                        if option.start_x_pt + 0.5_f64 < state.prev_start_x_pt {
                            cost += 2.5_f64;
                        }
                    }

                    let mut selected = state.selected.clone();
                    selected.push(Some(option.text.clone()));
                    let mut used_keys = state.used_keys.clone();
                    if !used_keys.iter().any(|key| key == &option.key) {
                        used_keys.push(option.key.clone());
                    }
                    next.push(JointAssignmentBeamState {
                        cost,
                        selected,
                        used_keys,
                        prev_start_x_pt: option.start_x_pt,
                        prev_end_x_pt: option.end_x_pt,
                    });
                }
            }

            if next.is_empty() {
                return None;
            }
            next.sort_by(|left, right| {
                left.cost
                    .partial_cmp(&right.cost)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            next.truncate(JOINT_ASSIGNMENT_BEAM_WIDTH);
            beam = next;
        }

        beam.into_iter().next().map(|state| state.selected)
    }

    fn build_joint_assignment_options(
        guess: &RedactionGuess,
        scan_limit: usize,
        max_options: usize,
    ) -> Vec<JointAssignmentOption> {
        if guess.candidates.is_empty() {
            return Vec::new();
        }
        let mut options = Vec::<JointAssignmentOption>::new();
        let mut seen = std::collections::BTreeSet::<String>::new();
        let scan = guess.candidates.len().min(scan_limit);
        for (rank, candidate) in guess.candidates.iter().take(scan).enumerate() {
            let text = candidate.text.trim();
            if text.is_empty() {
                continue;
            }
            if !seen.insert(text.to_owned()) {
                continue;
            }
            let context_penalty = punctuation_context_penalty(
                &guess.context.left_anchor_text,
                &guess.context.right_anchor_text,
                text,
            );
            let width_penalty = candidate_width_penalty_pt(guess, text);
            let rank_penalty = ((rank + 2) as f64).ln() * 0.10_f64;
            let exact_bonus = if guess.exact_matches.iter().any(|value| value == text) {
                -0.25_f64
            } else {
                0.0_f64
            };
            let anchor_overlap_penalty = anchor_overlap_penalty_pt(
                &guess.context.left_anchor_text,
                &guess.context.right_anchor_text,
                text,
            );
            let base_cost = (candidate.error_pt as f64)
                + context_penalty
                + width_penalty
                + rank_penalty
                + exact_bonus
                + anchor_overlap_penalty;
            let (start_x_pt, end_x_pt) =
                estimate_candidate_interval_pt(guess, text, candidate.width_pt);
            options.push(JointAssignmentOption {
                text: text.to_owned(),
                key: normalize_candidate_key(text),
                base_cost,
                start_x_pt,
                end_x_pt,
            });
        }
        options.sort_by(|left, right| {
            left.base_cost
                .partial_cmp(&right.base_cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        options.truncate(max_options);
        options
    }

    fn estimate_candidate_interval_pt(
        guess: &RedactionGuess,
        text: &str,
        measured_width_pt: Option<f32>,
    ) -> (f64, f64) {
        let char_width = (guess.context.char_width_pt as f64).max(0.1_f64);
        let fallback_width = candidate_char_units(text) * char_width;
        let width_pt = measured_width_pt
            .map(|value| value as f64)
            .filter(|value| value.is_finite() && *value > 0.0_f64)
            .unwrap_or(fallback_width)
            .max(0.1_f64);
        let start = match guess.context.anchor_mode.as_deref() {
            Some("right_only") => {
                let right_x = guess
                    .context
                    .anchor_right_x
                    .map(|value| value as f64)
                    .unwrap_or(guess.bbox.x1 as f64);
                (right_x - width_pt).min(guess.bbox.x1 as f64 - 0.1_f64)
            }
            Some("left_only") | Some("two_sided") | None | Some(_) => guess.bbox.x0 as f64,
        };
        (start, start + width_pt)
    }

    fn apply_row_sequence_consensus(
        guesses: &mut [RedactionGuess],
        skip_indices: &std::collections::BTreeSet<usize>,
    ) {
        let mut rows = std::collections::BTreeMap::<(u32, i32), Vec<usize>>::new();
        for (index, guess) in guesses.iter().enumerate() {
            if skip_indices.contains(&index) {
                continue;
            }
            if !guess.context.has_anchor_pair || guess.candidates.is_empty() {
                continue;
            }
            let center_y = ((guess.bbox.y0 + guess.bbox.y1) * 0.5_f32) as f64;
            let y_bucket = (center_y / 6.0_f64).round() as i32;
            rows.entry((guess.page_index, y_bucket))
                .or_default()
                .push(index);
        }

        for indices in rows.values_mut() {
            if indices.len() < 2 {
                continue;
            }
            indices.sort_by(|left_idx, right_idx| {
                guesses[*left_idx]
                    .bbox
                    .x0
                    .partial_cmp(&guesses[*right_idx].bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        guesses[*left_idx]
                            .bbox
                            .x1
                            .partial_cmp(&guesses[*right_idx].bbox.x1)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });

            let mut used = std::collections::BTreeSet::<String>::new();
            let duplicate_penalty_amount = if indices.len() >= 3 { 3.0_f64 } else { 0.0_f64 };
            for guess_index in indices.iter().copied() {
                let guess = &mut guesses[guess_index];
                if guess.candidates.is_empty() {
                    continue;
                }
                let mut best: Option<(String, f64)> = None;
                let max_scan = guess.candidates.len().min(80);
                for (rank, candidate) in guess.candidates.iter().take(max_scan).enumerate() {
                    let key = normalize_candidate_key(&candidate.text);
                    let duplicate_penalty =
                        if duplicate_penalty_amount > 0.0_f64 && used.contains(&key) {
                            duplicate_penalty_amount
                        } else {
                            0.0_f64
                        };
                    let width_penalty = candidate_width_penalty_pt(guess, &candidate.text);
                    let rank_penalty = rank as f64 * 0.05_f64;
                    let cost = candidate.error_pt as f64
                        + duplicate_penalty
                        + width_penalty
                        + rank_penalty;
                    match &best {
                        None => best = Some((candidate.text.clone(), cost)),
                        Some((_, best_cost)) if cost < *best_cost => {
                            best = Some((candidate.text.clone(), cost))
                        }
                        _ => {}
                    }
                }
                let Some((selected_text, _)) = best else {
                    continue;
                };
                used.insert(normalize_candidate_key(&selected_text));
                promote_text_to_front(guess, &selected_text);
            }
        }
    }

    fn normalize_candidate_key(value: &str) -> String {
        value
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_uppercase()
    }

    fn candidate_width_penalty_pt(guess: &RedactionGuess, text: &str) -> f64 {
        let char_width = (guess.context.char_width_pt as f64).max(0.1_f64);
        let target = guess.bbox.width().abs() as f64;
        if target <= 0.0_f64 {
            return 0.0_f64;
        }
        let estimated = candidate_char_units(text) * char_width;
        ((estimated - target).abs() / target.max(1.0_f64)).min(3.0_f64)
    }

    fn candidate_char_units(text: &str) -> f64 {
        let glyph_count = text
            .chars()
            .filter(|ch| !ch.is_whitespace() && *ch != ',')
            .count()
            .max(1) as f64;
        let spaces = text.chars().filter(|ch| ch.is_whitespace()).count() as f64;
        glyph_count + spaces * 0.45_f64
    }

    fn char_unit_band(target_width_pt: f64, char_width_pt: f64, tolerance_pt: f64) -> (f64, f64) {
        if !target_width_pt.is_finite()
            || target_width_pt <= 0.0_f64
            || !char_width_pt.is_finite()
            || char_width_pt <= 0.0_f64
        {
            return (1.0_f64, f64::INFINITY);
        }
        let target_units = target_width_pt / char_width_pt.max(0.1_f64);
        let slack = (tolerance_pt.abs() / char_width_pt.max(0.1_f64)).max(2.0_f64) + 2.0_f64;
        let lower = (target_units - slack).max(1.0_f64);
        let upper = (target_units + slack).max(lower + 1.0_f64);
        (lower, upper)
    }

    fn anchor_overlap_penalty_pt(
        left_anchor_text: &str,
        right_anchor_text: &str,
        candidate: &str,
    ) -> f64 {
        let anchor_tokens = tokenize_alpha_words(left_anchor_text)
            .into_iter()
            .chain(tokenize_alpha_words(right_anchor_text))
            .collect::<std::collections::BTreeSet<_>>();
        if anchor_tokens.is_empty() {
            return 0.0_f64;
        }
        let candidate_tokens = tokenize_alpha_words(candidate);
        if candidate_tokens.is_empty() {
            return 0.0_f64;
        }
        let mut matches = 0_u32;
        for token in &candidate_tokens {
            if anchor_tokens.contains(token) {
                matches += 1;
            }
        }
        if matches == 0 {
            return 0.0_f64;
        }
        let mut penalty = matches as f64 * 0.45_f64;
        if matches >= 2 {
            penalty += 0.35_f64;
        }
        penalty
    }

    fn tokenize_alpha_words(value: &str) -> Vec<String> {
        value
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '\'' && ch != '-')
            .map(|token| token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric()))
            .filter(|token| token.len() >= 3)
            .map(|token| token.to_ascii_lowercase())
            .collect::<Vec<_>>()
    }

    fn promote_text_to_front(guess: &mut RedactionGuess, selected_text: &str) {
        if let Some(pos) = guess
            .candidates
            .iter()
            .position(|candidate| candidate.text == selected_text)
        {
            let chosen = guess.candidates.remove(pos);
            guess.candidates.insert(0, chosen);
        }
        if let Some(pos) = guess
            .exact_matches
            .iter()
            .position(|value| value == selected_text)
        {
            let chosen = guess.exact_matches.remove(pos);
            guess.exact_matches.insert(0, chosen);
        }
    }

    fn effective_box_error_pt(
        redaction: &RedactionOccurrence,
        candidate_width_pt: f64,
        target_width_pt: f64,
    ) -> f64 {
        let raw = (candidate_width_pt - target_width_pt).abs();
        if matches!(redaction.kind, RedactionKind::RasterDarkRegion) {
            (raw - RASTER_WIDTH_NOISE_PT).max(0.0_f64)
        } else {
            raw
        }
    }

    fn build_guess_for_anchor(
        redaction: &RedactionOccurrence,
        dictionary: &[String],
        _cfg: &GuessConfig,
        anchor: &AnchorPairData,
        assets: &std::collections::BTreeMap<String, FontAsset>,
        width_tables: &std::collections::BTreeMap<WidthTableKey, WidthTable>,
        cache: &mut WidthCache,
    ) -> (RedactionGuess, CandidateFunnelMetrics) {
        let mut funnel = CandidateFunnelMetrics::default();
        let asset = assets.get(&anchor.font_key);
        let fallback_char_width = estimate_char_width_pt(
            &anchor.left_anchor_text,
            &anchor.right_anchor_text,
            Some(Rect::new(
                anchor.left_bbox.x0,
                anchor.left_bbox.y0,
                anchor.left_bbox.x1,
                anchor.left_bbox.y1,
            )),
            Some(Rect::new(
                anchor.right_bbox.x0,
                anchor.right_bbox.y0,
                anchor.right_bbox.x1,
                anchor.right_bbox.y1,
            )),
            redaction.bbox,
        );
        let fallback_space_width = (fallback_char_width * 0.5_f64).max(0.5_f64);

        let measure_width = |text: &str| {
            let measured = measure_text_width_from_sources(
                &TextMeasureInput {
                    page_index: redaction.page_index,
                    font_key: &anchor.font_key,
                    font_name: &anchor.font_name,
                    font_size_pt: anchor.font_size_pt,
                    h_scale_pct: anchor.h_scale_pct,
                    text,
                    metrics_dpi: DEFAULT_METRICS_DPI,
                },
                asset,
                width_tables,
            );
            measured.or_else(|| {
                Some(fallback_measured_width(
                    text,
                    fallback_char_width,
                    fallback_space_width,
                    DEFAULT_METRICS_DPI,
                ))
            })
        };

        let key = WidthKey {
            page_index: redaction.page_index,
            font_key: anchor.font_key.clone(),
            font_size_bits: anchor.font_size_pt.to_bits(),
            h_scale_bits: anchor.h_scale_pct.to_bits(),
            metrics_dpi_bits: DEFAULT_METRICS_DPI.to_bits(),
        };
        let has_width_table_for_anchor = width_tables.contains_key(&WidthTableKey {
            page_index: redaction.page_index,
            font_key: anchor.font_key.clone(),
        });
        let left_anchor_text = anchor.left_anchor_text.trim();
        let left_width = measure_width(left_anchor_text).unwrap_or_else(|| {
            fallback_measured_width(
                left_anchor_text,
                fallback_char_width,
                fallback_space_width,
                DEFAULT_METRICS_DPI,
            )
        });
        let space_width = measure_width(" ").unwrap_or_else(|| {
            fallback_measured_width(
                " ",
                fallback_char_width,
                fallback_space_width,
                DEFAULT_METRICS_DPI,
            )
        });
        let redaction_width_pt = (redaction.bbox.width().abs() as f64).max(1.0_f64);
        let anchor_gap_pt = (anchor.right_x - anchor.left_x).abs().max(1.0_f64);
        let gap_ratio = anchor_gap_pt / redaction_width_pt;
        let multi_span_mode = matches!(anchor.mode, AnchorMode::TwoSided)
            && gap_ratio >= MULTI_SPAN_GAP_RATIO_THRESHOLD;
        let (min_char_units, max_char_units) = char_unit_band(
            redaction_width_pt,
            fallback_char_width.max(0.1_f64),
            anchor.epsilon_pt.max(FIXED_TOLERANCE_PT),
        );
        let candidate_width_index = build_row_candidate_width_index(
            dictionary,
            &key,
            cache,
            min_char_units,
            max_char_units,
            &measure_width,
        );
        if candidate_width_index.is_empty() {
            return (
                RedactionGuess {
                    page_index: redaction.page_index,
                    bbox: redaction.bbox,
                    candidates: Vec::new(),
                    exact_matches: Vec::new(),
                    context: GuessContext {
                        left_anchor_text: anchor.left_anchor_text.clone(),
                        right_anchor_text: anchor.right_anchor_text.clone(),
                        gap_pt: (anchor.right_x - anchor.left_x) as f32,
                        char_width_pt: fallback_char_width as f32,
                        tol_pt: anchor.epsilon_pt as f32,
                        anchor_left_x: Some(anchor.left_x as f32),
                        anchor_right_x: Some(anchor.right_x as f32),
                        anchor_font_key: Some(anchor.font_key.clone()),
                        anchor_font_name: Some(anchor.font_name.clone()),
                        anchor_font_size_pt: Some(anchor.font_size_pt),
                        anchor_h_scale_pct: Some(anchor.h_scale_pct),
                        anchor_row_bias_pt: Some(anchor.row_bias_pt as f32),
                        anchor_mode: Some(anchor.mode.as_str().to_owned()),
                        anchor_width_source: None,
                        space_width_source: None,
                        candidate_width_source: None,
                        width_fallback_reason: None,
                        confidence_score: None,
                        confidence_factors: None,
                        has_anchor_pair: true,
                    },
                    visual_compared_pixels: None,
                    visual_mean_abs_diff: None,
                    visual_changed_pixel_ratio: None,
                    visual_reason: None,
                    visual_dropped: false,
                },
                funnel,
            );
        }
        let mut scored = Vec::new();
        let anchor_filter_limit_pt =
            (anchor.epsilon_pt.max(1.0_f64) * 4.0_f64).max(FIXED_TOLERANCE_PT.max(4.0_f64));
        let box_filter_limit_pt = (redaction_width_pt * MULTI_SPAN_BOX_ERROR_RATIO
            + MULTI_SPAN_BOX_ERROR_PAD_PT)
            .max(anchor.epsilon_pt.max(2.5_f64));
        let list_like_context =
            is_list_like_context(&anchor.left_anchor_text, &anchor.right_anchor_text);
        if multi_span_mode {
            let lower_width = (redaction_width_pt - box_filter_limit_pt).max(0.0_f64);
            let upper_width = redaction_width_pt + box_filter_limit_pt;
            let ranged =
                candidate_width_entries_in_range(&candidate_width_index, lower_width, upper_width);
            let mut band = if ranged.is_empty() {
                candidate_width_index.as_slice()
            } else {
                ranged
            };
            band = trim_width_band_around_target(
                band,
                redaction_width_pt,
                MULTI_SPAN_WIDTH_BAND_LIMIT,
            );
            for entry in band {
                funnel.scanned += 1;
                let trimmed = entry.text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let char_units = candidate_char_units(trimmed);
                if char_units < min_char_units || char_units > max_char_units {
                    continue;
                }
                funnel.after_char_units += 1;
                if !passes_context_filter(
                    &anchor.left_anchor_text,
                    &anchor.right_anchor_text,
                    trimmed,
                ) {
                    continue;
                }
                funnel.after_context += 1;
                if list_like_context && !looks_like_alpha_phrase_candidate(trimmed) {
                    continue;
                }
                funnel.after_shape += 1;

                let predicted_right = anchor.left_x
                    + left_width.pt
                    + space_width.pt
                    + entry.width_pt
                    + space_width.pt
                    + anchor.row_bias_pt;
                let anchor_err = (predicted_right - anchor.right_x).abs();
                funnel.after_anchor += 1;
                let box_err = effective_box_error_pt(redaction, entry.width_pt, redaction_width_pt);
                if box_err > box_filter_limit_pt {
                    continue;
                }
                funnel.after_box += 1;
                let raw_err = (box_err * MULTI_SPAN_BOX_PRIOR_WEIGHT)
                    + (anchor_err * MULTI_SPAN_ANCHOR_PRIOR_WEIGHT);
                let context_penalty = punctuation_context_penalty(
                    &anchor.left_anchor_text,
                    &anchor.right_anchor_text,
                    trimmed,
                );
                let effective_err = raw_err + context_penalty;
                scored.push(ScoredDictionaryCandidate {
                    text: trimmed.to_owned(),
                    raw_error_pt: raw_err,
                    effective_error_pt: effective_err,
                    word_count: trimmed.split_whitespace().count() as u32,
                    width_pt: entry.width_pt,
                    width_source: entry.source,
                });
            }
        } else {
            let single_span_width_slack_pt = (anchor.epsilon_pt.max(FIXED_TOLERANCE_PT) * 1.75_f64)
                .max(redaction_width_pt * 0.45_f64)
                .max(12.0_f64);
            let lower_width = (redaction_width_pt - single_span_width_slack_pt).max(0.0_f64);
            let upper_width = redaction_width_pt + single_span_width_slack_pt;
            let ranged =
                candidate_width_entries_in_range(&candidate_width_index, lower_width, upper_width);
            let mut band = if ranged.is_empty() {
                candidate_width_index.as_slice()
            } else {
                ranged
            };
            band = trim_width_band_around_target(
                band,
                redaction_width_pt,
                SINGLE_SPAN_WIDTH_BAND_LIMIT,
            );
            for entry in band {
                let trimmed = entry.text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                funnel.scanned += 1;
                let char_units = candidate_char_units(trimmed);
                if char_units < min_char_units || char_units > max_char_units {
                    continue;
                }
                funnel.after_char_units += 1;
                if !passes_context_filter(
                    &anchor.left_anchor_text,
                    &anchor.right_anchor_text,
                    trimmed,
                ) {
                    continue;
                }
                funnel.after_context += 1;
                if list_like_context && !looks_like_alpha_phrase_candidate(trimmed) {
                    continue;
                }
                funnel.after_shape += 1;
                let box_err = effective_box_error_pt(redaction, entry.width_pt, redaction_width_pt);
                let side_alignment_err = match anchor.mode {
                    AnchorMode::TwoSided => {
                        let predicted_right = anchor.left_x
                            + left_width.pt
                            + space_width.pt
                            + entry.width_pt
                            + space_width.pt
                            + anchor.row_bias_pt;
                        (predicted_right - anchor.right_x).abs()
                    }
                    AnchorMode::LeftOnly => {
                        let predicted_start =
                            anchor.left_x + left_width.pt + space_width.pt + anchor.row_bias_pt;
                        (predicted_start - redaction.bbox.x0 as f64).abs()
                    }
                    AnchorMode::RightOnly => {
                        let predicted_end = anchor.right_x - space_width.pt + anchor.row_bias_pt;
                        (predicted_end - redaction.bbox.x1 as f64).abs()
                    }
                };
                let side_alignment_limit = match anchor.mode {
                    AnchorMode::TwoSided => anchor_filter_limit_pt,
                    AnchorMode::LeftOnly | AnchorMode::RightOnly => {
                        anchor_filter_limit_pt * 2.5_f64
                    }
                };
                if side_alignment_err > side_alignment_limit {
                    continue;
                }
                funnel.after_anchor += 1;
                funnel.after_box += 1;
                let raw_err = match anchor.mode {
                    AnchorMode::TwoSided => {
                        side_alignment_err + (box_err * SINGLE_SPAN_BOX_PRIOR_WEIGHT)
                    }
                    AnchorMode::LeftOnly | AnchorMode::RightOnly => {
                        box_err + (side_alignment_err * 0.20_f64)
                    }
                };
                let context_penalty = punctuation_context_penalty(
                    &anchor.left_anchor_text,
                    &anchor.right_anchor_text,
                    trimmed,
                );
                let effective_err = raw_err + context_penalty;
                scored.push(ScoredDictionaryCandidate {
                    text: trimmed.to_owned(),
                    raw_error_pt: raw_err,
                    effective_error_pt: effective_err,
                    word_count: trimmed.split_whitespace().count() as u32,
                    width_pt: entry.width_pt,
                    width_source: entry.source,
                });
            }
        }
        funnel.scored = scored.len();
        scored.sort_by(|left_candidate, right_candidate| {
            left_candidate
                .effective_error_pt
                .partial_cmp(&right_candidate.effective_error_pt)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left_candidate.word_count.cmp(&right_candidate.word_count))
                .then_with(|| left_candidate.text.cmp(&right_candidate.text))
        });

        let epsilon = anchor.epsilon_pt.max(0.0);
        let exact_scored = scored
            .iter()
            .filter(|candidate| candidate.effective_error_pt <= epsilon)
            .cloned()
            .collect::<Vec<_>>();
        let exact_matches = exact_scored
            .iter()
            .map(|candidate| candidate.text.clone())
            .collect::<Vec<_>>();

        let selected = if exact_scored.is_empty() {
            scored
                .iter()
                .take(FIXED_MAX_CANDIDATES)
                .cloned()
                .collect::<Vec<_>>()
        } else {
            exact_scored
                .iter()
                .take(FIXED_MAX_CANDIDATES)
                .cloned()
                .collect::<Vec<_>>()
        };

        let denom = if exact_scored.is_empty() {
            FIXED_TOLERANCE_PT.max(0.0001)
        } else {
            epsilon.max(0.0001)
        };
        let candidates = selected
            .iter()
            .map(|candidate| GuessCandidate {
                text: candidate.text.clone(),
                score: (1.0 - (candidate.effective_error_pt / denom)).clamp(0.0, 1.0) as f32,
                error_pt: candidate.raw_error_pt as f32,
                word_count: candidate.word_count,
                width_pt: Some(candidate.width_pt as f32),
            })
            .collect::<Vec<_>>();
        let candidate_width_source = selected
            .first()
            .map(|candidate| candidate.width_source.as_str().to_owned());
        let mut width_fallback_parts = Vec::<&str>::new();
        if asset.is_none() {
            width_fallback_parts.push("font_asset_missing");
        }
        if !has_width_table_for_anchor {
            width_fallback_parts.push("width_table_missing");
        }
        let width_fallback_reason = if width_fallback_parts.is_empty() {
            None
        } else {
            Some(width_fallback_parts.join("+"))
        };

        let char_width = if !anchor.left_anchor_text.trim().is_empty() {
            let chars = anchor.left_anchor_text.trim().chars().count().max(1) as f64;
            (left_width.pt / chars).max(0.0)
        } else {
            fallback_char_width
        };

        let guess = RedactionGuess {
            page_index: redaction.page_index,
            bbox: redaction.bbox,
            candidates,
            exact_matches,
            context: GuessContext {
                left_anchor_text: anchor.left_anchor_text.clone(),
                right_anchor_text: anchor.right_anchor_text.clone(),
                gap_pt: (anchor.right_x - anchor.left_x) as f32,
                char_width_pt: char_width as f32,
                tol_pt: epsilon as f32,
                anchor_left_x: Some(anchor.left_x as f32),
                anchor_right_x: Some(anchor.right_x as f32),
                anchor_font_key: Some(anchor.font_key.clone()),
                anchor_font_name: Some(anchor.font_name.clone()),
                anchor_font_size_pt: Some(anchor.font_size_pt),
                anchor_h_scale_pct: Some(anchor.h_scale_pct),
                anchor_row_bias_pt: Some(anchor.row_bias_pt as f32),
                anchor_mode: Some(anchor.mode.as_str().to_owned()),
                anchor_width_source: Some(left_width.source.as_str().to_owned()),
                space_width_source: Some(space_width.source.as_str().to_owned()),
                candidate_width_source,
                width_fallback_reason,
                confidence_score: None,
                confidence_factors: None,
                has_anchor_pair: true,
            },
            visual_compared_pixels: None,
            visual_mean_abs_diff: None,
            visual_changed_pixel_ratio: None,
            visual_reason: None,
            visual_dropped: false,
        };
        (guess, funnel)
    }

    fn select_anchor_pair(
        redaction: &RedactionOccurrence,
        runs: &[&FontTextRun],
        assets: &std::collections::BTreeMap<String, FontAsset>,
        width_tables: &std::collections::BTreeMap<WidthTableKey, WidthTable>,
    ) -> Option<AnchorPairData> {
        let red_center_y = rect_center_y(&redaction.bbox);
        let red_center_x = ((redaction.bbox.x0 + redaction.bbox.x1) * 0.5) as f64;
        let left_hint_hit = redaction
            .underlying_text
            .first()
            .filter(|hit| !hit.text.trim().is_empty());
        let right_hint_hit = redaction
            .underlying_text
            .get(1)
            .filter(|hit| !hit.text.trim().is_empty());
        let left_hint = left_hint_hit.map(|hit| hit.text.trim());
        let right_hint = right_hint_hit.map(|hit| hit.text.trim());
        let hints = AnchorHints {
            left_text: left_hint,
            left_x: left_hint_hit.map(|hit| hit.bbox.x0 as f64),
            right_text: right_hint,
            right_x: right_hint_hit.map(|hit| hit.bbox.x0 as f64),
        };

        let mut row_runs = collect_row_runs_for_anchor(redaction, runs, true);
        if row_runs.is_empty() {
            row_runs = collect_row_runs_for_anchor(redaction, runs, false);
        }
        if row_runs.is_empty() {
            return None;
        }
        row_runs.sort_by(|left_run, right_run| {
            left_run
                .bbox
                .x0
                .partial_cmp(&right_run.bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left_run
                        .bbox
                        .y0
                        .partial_cmp(&right_run.bbox.y0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left_run.text.cmp(&right_run.text))
        });

        if let (Some(left_hint_text), Some(right_hint_text)) = (left_hint, right_hint) {
            let same_run = row_runs.iter().copied().find(|run| {
                let run_text = run.text.trim();
                text_matches(run_text, left_hint_text) && text_matches(run_text, right_hint_text)
            });
            if let Some(run) = same_run {
                let asset = assets.get(&run.font_key);
                let (left_anchor_text, left_x) = resolve_anchor_text_and_x(
                    redaction.page_index,
                    run,
                    Some(left_hint_text),
                    left_hint_hit.map(|hit| hit.bbox.x0 as f64),
                    asset,
                    width_tables,
                );
                let (right_anchor_text, right_x) = resolve_anchor_text_and_x(
                    redaction.page_index,
                    run,
                    Some(right_hint_text),
                    right_hint_hit.map(|hit| hit.bbox.x0 as f64),
                    asset,
                    width_tables,
                );
                if right_x > left_x {
                    let measure_ctx = WidthMeasureContext {
                        page_index: redaction.page_index,
                        asset,
                        width_tables,
                        h_scale_pct: run.h_scale_pct,
                    };
                    let calibration = estimate_row_epsilon(&row_runs, run, redaction, &measure_ctx);
                    return Some(AnchorPairData {
                        left_anchor_text,
                        right_anchor_text,
                        left_x,
                        right_x,
                        font_key: run.font_key.clone(),
                        font_name: run.font_name.clone(),
                        font_size_pt: run.font_size_pt,
                        h_scale_pct: run.h_scale_pct,
                        left_bbox: run.bbox,
                        right_bbox: run.bbox,
                        epsilon_pt: calibration.epsilon_pt,
                        row_bias_pt: calibration.bias_pt,
                        mode: AnchorMode::TwoSided,
                    });
                }
            }
        }

        let mut pairs = Vec::new();
        for left_idx in 0..row_runs.len() {
            for right_idx in (left_idx + 1)..row_runs.len() {
                let left_run = row_runs[left_idx];
                let right_run = row_runs[right_idx];
                let left_end = left_run.bbox.x1 as f64;
                let right_start = right_run.bbox.x0 as f64;
                if right_start <= left_end {
                    continue;
                }
                let font_penalty = if left_run.font_key == right_run.font_key
                    && (left_run.font_size_pt - right_run.font_size_pt).abs() <= 2.0
                {
                    0
                } else {
                    1
                };
                let left_hint_penalty = match left_hint {
                    Some(hint) => u8::from(!text_matches(left_run.text.trim(), hint)),
                    None => 0,
                };
                let right_hint_penalty = match right_hint {
                    Some(hint) => u8::from(!text_matches(right_run.text.trim(), hint)),
                    None => 0,
                };
                let hint_penalty = left_hint_penalty + right_hint_penalty;
                let contains_center_penalty =
                    u8::from(red_center_x < left_end || red_center_x > right_start);
                let y_distance = (run_center_y(left_run) - red_center_y).abs()
                    + (run_center_y(right_run) - red_center_y).abs();
                let baseline_distance = (left_run.bbox.y1 - redaction.bbox.y1).abs()
                    + (right_run.bbox.y1 - redaction.bbox.y1).abs();
                let x_distance = (redaction.bbox.x0 as f64 - left_end).abs()
                    + (right_start - redaction.bbox.x1 as f64).abs();
                let gap_width = right_start - left_end;
                pairs.push(PairCandidate {
                    left_idx,
                    right_idx,
                    font_penalty,
                    hint_penalty,
                    contains_center_penalty,
                    baseline_distance: baseline_distance as f64,
                    y_distance: y_distance as f64,
                    x_distance,
                    gap_width,
                });
            }
        }
        if pairs.is_empty() {
            return recover_one_sided_anchor(redaction, &row_runs, &hints, assets, width_tables);
        }

        pairs.sort_by(|left_pair, right_pair| {
            left_pair
                .font_penalty
                .cmp(&right_pair.font_penalty)
                .then_with(|| left_pair.hint_penalty.cmp(&right_pair.hint_penalty))
                .then_with(|| {
                    left_pair
                        .contains_center_penalty
                        .cmp(&right_pair.contains_center_penalty)
                })
                .then_with(|| {
                    left_pair
                        .baseline_distance
                        .partial_cmp(&right_pair.baseline_distance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    left_pair
                        .y_distance
                        .partial_cmp(&right_pair.y_distance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    left_pair
                        .x_distance
                        .partial_cmp(&right_pair.x_distance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    left_pair
                        .gap_width
                        .partial_cmp(&right_pair.gap_width)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        let selected_pair = pairs
            .iter()
            .find(|pair| pair.font_penalty == 0)
            .or_else(|| pairs.first())?;

        let left_run = row_runs[selected_pair.left_idx];
        let right_run = row_runs[selected_pair.right_idx];
        let asset = assets
            .get(&left_run.font_key)
            .or_else(|| assets.get(&right_run.font_key));
        let (left_anchor_text, left_x) = resolve_anchor_text_and_x(
            redaction.page_index,
            left_run,
            left_hint,
            left_hint_hit.map(|hit| hit.bbox.x0 as f64),
            asset,
            width_tables,
        );
        let (right_anchor_text, right_x) = resolve_anchor_text_and_x(
            redaction.page_index,
            right_run,
            right_hint,
            right_hint_hit.map(|hit| hit.bbox.x0 as f64),
            asset,
            width_tables,
        );
        if left_anchor_text.trim().is_empty() || right_anchor_text.trim().is_empty() {
            return recover_one_sided_anchor(redaction, &row_runs, &hints, assets, width_tables);
        }
        if right_x <= left_x {
            return recover_one_sided_anchor(redaction, &row_runs, &hints, assets, width_tables);
        }

        let measure_ctx = WidthMeasureContext {
            page_index: redaction.page_index,
            asset,
            width_tables,
            h_scale_pct: left_run.h_scale_pct,
        };
        let calibration = estimate_row_epsilon(&row_runs, left_run, redaction, &measure_ctx);
        Some(AnchorPairData {
            left_anchor_text,
            right_anchor_text,
            left_x,
            right_x,
            font_key: left_run.font_key.clone(),
            font_name: left_run.font_name.clone(),
            font_size_pt: left_run.font_size_pt,
            h_scale_pct: left_run.h_scale_pct,
            left_bbox: left_run.bbox,
            right_bbox: right_run.bbox,
            epsilon_pt: calibration.epsilon_pt,
            row_bias_pt: calibration.bias_pt,
            mode: AnchorMode::TwoSided,
        })
    }

    fn recover_one_sided_anchor(
        redaction: &RedactionOccurrence,
        row_runs: &[&FontTextRun],
        hints: &AnchorHints<'_>,
        assets: &std::collections::BTreeMap<String, FontAsset>,
        width_tables: &std::collections::BTreeMap<WidthTableKey, WidthTable>,
    ) -> Option<AnchorPairData> {
        let left_only = row_runs
            .iter()
            .copied()
            .filter(|run| run.bbox.x1 <= redaction.bbox.x0 + 1.5_f32)
            .min_by(|left_run, right_run| {
                let left_gap = (redaction.bbox.x0 as f64 - left_run.bbox.x1 as f64).max(0.0_f64);
                let right_gap = (redaction.bbox.x0 as f64 - right_run.bbox.x1 as f64).max(0.0_f64);
                left_gap
                    .partial_cmp(&right_gap)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        (run_center_y(left_run) - rect_center_y(&redaction.bbox))
                            .abs()
                            .partial_cmp(
                                &(run_center_y(right_run) - rect_center_y(&redaction.bbox)).abs(),
                            )
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });
        let right_only = row_runs
            .iter()
            .copied()
            .filter(|run| run.bbox.x0 >= redaction.bbox.x1 - 1.5_f32)
            .min_by(|left_run, right_run| {
                let left_gap = (left_run.bbox.x0 as f64 - redaction.bbox.x1 as f64).max(0.0_f64);
                let right_gap = (right_run.bbox.x0 as f64 - redaction.bbox.x1 as f64).max(0.0_f64);
                left_gap
                    .partial_cmp(&right_gap)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        (run_center_y(left_run) - rect_center_y(&redaction.bbox))
                            .abs()
                            .partial_cmp(
                                &(run_center_y(right_run) - rect_center_y(&redaction.bbox)).abs(),
                            )
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });

        if let Some(left_run) = left_only {
            let asset = assets.get(&left_run.font_key);
            let (left_anchor_text, left_x) = resolve_anchor_text_and_x(
                redaction.page_index,
                left_run,
                hints.left_text,
                hints.left_x,
                asset,
                width_tables,
            );
            let right_anchor_text = hints.right_text.unwrap_or_default().to_owned();
            let right_x = (redaction.bbox.x1 as f64 + 0.5_f64).max(left_x + 1.0_f64);
            if !left_anchor_text.trim().is_empty() && right_x > left_x {
                let measure_ctx = WidthMeasureContext {
                    page_index: redaction.page_index,
                    asset,
                    width_tables,
                    h_scale_pct: left_run.h_scale_pct,
                };
                let mut calibration =
                    estimate_row_epsilon(row_runs, left_run, redaction, &measure_ctx);
                calibration.epsilon_pt = (calibration.epsilon_pt * 1.75_f64).max(4.0_f64);
                return Some(AnchorPairData {
                    left_anchor_text,
                    right_anchor_text,
                    left_x,
                    right_x,
                    font_key: left_run.font_key.clone(),
                    font_name: left_run.font_name.clone(),
                    font_size_pt: left_run.font_size_pt,
                    h_scale_pct: left_run.h_scale_pct,
                    left_bbox: left_run.bbox,
                    right_bbox: left_run.bbox,
                    epsilon_pt: calibration.epsilon_pt,
                    row_bias_pt: calibration.bias_pt,
                    mode: AnchorMode::LeftOnly,
                });
            }
        }

        if let Some(right_run) = right_only {
            let asset = assets.get(&right_run.font_key);
            let (right_anchor_text, right_x) = resolve_anchor_text_and_x(
                redaction.page_index,
                right_run,
                hints.right_text,
                hints.right_x,
                asset,
                width_tables,
            );
            let left_anchor_text = hints.left_text.unwrap_or_default().to_owned();
            let left_x = (redaction.bbox.x0 as f64 - 0.5_f64).min(right_x - 1.0_f64);
            if !right_anchor_text.trim().is_empty() && right_x > left_x {
                let measure_ctx = WidthMeasureContext {
                    page_index: redaction.page_index,
                    asset,
                    width_tables,
                    h_scale_pct: right_run.h_scale_pct,
                };
                let mut calibration =
                    estimate_row_epsilon(row_runs, right_run, redaction, &measure_ctx);
                calibration.epsilon_pt = (calibration.epsilon_pt * 1.75_f64).max(4.0_f64);
                return Some(AnchorPairData {
                    left_anchor_text,
                    right_anchor_text,
                    left_x,
                    right_x,
                    font_key: right_run.font_key.clone(),
                    font_name: right_run.font_name.clone(),
                    font_size_pt: right_run.font_size_pt,
                    h_scale_pct: right_run.h_scale_pct,
                    left_bbox: right_run.bbox,
                    right_bbox: right_run.bbox,
                    epsilon_pt: calibration.epsilon_pt,
                    row_bias_pt: calibration.bias_pt,
                    mode: AnchorMode::RightOnly,
                });
            }
        }

        None
    }

    fn resolve_anchor_text_and_x(
        page_index: u32,
        run: &FontTextRun,
        hint: Option<&str>,
        hint_x: Option<f64>,
        asset: Option<&FontAsset>,
        width_tables: &std::collections::BTreeMap<WidthTableKey, WidthTable>,
    ) -> (String, f64) {
        let run_text = run.text.trim();
        if run_text.is_empty() {
            return (String::new(), run.bbox.x0 as f64);
        }
        let Some(hint_text) = hint else {
            return (run_text.to_owned(), run.bbox.x0 as f64);
        };
        if hint_text.is_empty() {
            return (run_text.to_owned(), run.bbox.x0 as f64);
        }
        if run_text == hint_text {
            return (hint_text.to_owned(), run.bbox.x0 as f64);
        }
        if let Some(prefix_bytes) = run_text.find(hint_text) {
            let prefix = &run_text[..prefix_bytes];
            let offset = prefix_width_from_run(run, prefix_bytes)
                .or_else(|| {
                    measure_text_width_from_sources(
                        &TextMeasureInput {
                            page_index,
                            font_key: &run.font_key,
                            font_name: &run.font_name,
                            font_size_pt: run.font_size_pt,
                            h_scale_pct: run.h_scale_pct,
                            text: prefix,
                            metrics_dpi: DEFAULT_METRICS_DPI,
                        },
                        asset,
                        width_tables,
                    )
                    .map(|value| value.pt)
                })
                .unwrap_or_else(|| {
                    let run_chars = run_text.chars().count().max(1) as f64;
                    let prefix_chars = prefix.chars().count() as f64;
                    ((run.bbox.x1 - run.bbox.x0).abs() as f64) * (prefix_chars / run_chars)
                });
            return (hint_text.to_owned(), run.bbox.x0 as f64 + offset);
        }
        if hint_text.contains(run_text) {
            return (run_text.to_owned(), run.bbox.x0 as f64);
        }
        (hint_text.to_owned(), hint_x.unwrap_or(run.bbox.x0 as f64))
    }

    fn prefix_width_from_run(run: &FontTextRun, prefix_bytes: usize) -> Option<f64> {
        if run.char_advances_pt.is_empty() {
            return None;
        }
        if prefix_bytes == 0 {
            return Some(0.0_f64);
        }
        let prefix_char_count = run.text.get(..prefix_bytes)?.chars().count();
        if prefix_char_count == 0 {
            return Some(0.0_f64);
        }
        if prefix_char_count > run.char_advances_pt.len() {
            return None;
        }
        let width_pt = run
            .char_advances_pt
            .iter()
            .take(prefix_char_count)
            .map(|value| *value as f64)
            .sum::<f64>();
        (width_pt.is_finite() && width_pt >= 0.0_f64).then_some(width_pt)
    }

    fn estimate_row_epsilon(
        runs: &[&FontTextRun],
        left_run: &FontTextRun,
        redaction: &RedactionOccurrence,
        measure_ctx: &WidthMeasureContext<'_>,
    ) -> RowCalibration {
        let fallback_char_width = estimate_char_width_pt(
            left_run.text.trim(),
            "",
            Some(Rect::new(
                left_run.bbox.x0,
                left_run.bbox.y0,
                left_run.bbox.x1,
                left_run.bbox.y1,
            )),
            None,
            redaction.bbox,
        )
        .max(0.1_f64);
        let space_width = measure_text_width_from_sources(
            &TextMeasureInput {
                page_index: measure_ctx.page_index,
                font_key: &left_run.font_key,
                font_name: &left_run.font_name,
                font_size_pt: left_run.font_size_pt,
                h_scale_pct: measure_ctx.h_scale_pct,
                text: " ",
                metrics_dpi: DEFAULT_METRICS_DPI,
            },
            measure_ctx.asset,
            measure_ctx.width_tables,
        )
        .map(|value| value.pt)
        .unwrap_or(0.5_f64 * fallback_char_width);
        let mut row = runs
            .iter()
            .copied()
            .filter(|run| {
                run.font_key == left_run.font_key
                    && (run.font_size_pt - left_run.font_size_pt).abs() <= 0.25
                    && (run_center_y(run) - run_center_y(left_run)).abs() <= 2.0
            })
            .collect::<Vec<_>>();
        row.sort_by(|left_entry, right_entry| {
            left_entry
                .bbox
                .x0
                .partial_cmp(&right_entry.bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left_entry.text.cmp(&right_entry.text))
        });

        let mut residuals = Vec::new();
        for window in row.windows(2) {
            let current = window[0];
            let next = window[1];
            let current_text = current.text.trim();
            if current_text.is_empty() {
                continue;
            }
            let current_width = measure_text_width_from_sources(
                &TextMeasureInput {
                    page_index: measure_ctx.page_index,
                    font_key: &current.font_key,
                    font_name: &current.font_name,
                    font_size_pt: current.font_size_pt,
                    h_scale_pct: current.h_scale_pct,
                    text: current_text,
                    metrics_dpi: DEFAULT_METRICS_DPI,
                },
                measure_ctx.asset,
                measure_ctx.width_tables,
            )
            .map(|value| value.pt)
            .unwrap_or_else(|| {
                let chars = current_text.chars().count().max(1) as f64;
                chars * fallback_char_width
            });
            let current_space_width = measure_text_width_from_sources(
                &TextMeasureInput {
                    page_index: measure_ctx.page_index,
                    font_key: &current.font_key,
                    font_name: &current.font_name,
                    font_size_pt: current.font_size_pt,
                    h_scale_pct: current.h_scale_pct,
                    text: " ",
                    metrics_dpi: DEFAULT_METRICS_DPI,
                },
                measure_ctx.asset,
                measure_ctx.width_tables,
            )
            .map(|value| value.pt)
            .unwrap_or(space_width);
            let predicted_next = current.bbox.x0 as f64 + current_width + current_space_width;
            let residual = next.bbox.x0 as f64 - predicted_next;
            if residual.is_finite() {
                residuals.push(residual);
            }
        }
        residuals.sort_by(|left_value, right_value| {
            left_value
                .partial_cmp(right_value)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if residuals.is_empty() {
            return RowCalibration {
                epsilon_pt: 2.0_f64,
                bias_pt: 0.0_f64,
            };
        }

        let median_index = ((residuals.len() as f64) * 0.5_f64).floor() as usize;
        let bias = residuals[median_index.min(residuals.len().saturating_sub(1))];
        let mut centered = residuals
            .iter()
            .map(|value| (value - bias).abs())
            .collect::<Vec<_>>();
        centered.sort_by(|left_value, right_value| {
            left_value
                .partial_cmp(right_value)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let epsilon = if centered.is_empty() {
            2.0_f64
        } else {
            let idx = ((centered.len() as f64) * 0.75_f64).floor() as usize;
            centered[idx.min(centered.len().saturating_sub(1))]
        };
        RowCalibration {
            epsilon_pt: epsilon.clamp(3.5_f64, 8.0_f64),
            bias_pt: bias,
        }
    }

    fn measure_text_width_pt(
        asset: &FontAsset,
        text: &str,
        font_size_pt: f32,
        h_scale_pct: f32,
        metrics_dpi: f32,
    ) -> Option<MeasuredWidth> {
        let face = rustybuzz::Face::from_slice(&asset.bytes, 0)?;
        let units_per_em = asset.units_per_em.max(1) as f32;
        let scale = (h_scale_pct as f64 / 100.0_f64).max(0.01_f64);
        let font_size = font_size_pt.abs().max(1.0_f32);
        let width_pt = (advance_pt(&face, text, font_size, units_per_em) as f64) * scale;
        if !width_pt.is_finite() || width_pt <= 0.0_f64 {
            return None;
        }
        Some(measured_width_from_points(
            width_pt,
            metrics_dpi,
            WidthSource::Asset,
        ))
    }

    fn measure_text_width_from_sources(
        input: &TextMeasureInput<'_>,
        asset: Option<&FontAsset>,
        width_tables: &std::collections::BTreeMap<WidthTableKey, WidthTable>,
    ) -> Option<MeasuredWidth> {
        if input.text.is_empty() {
            return Some(measured_width_from_points(
                0.0_f64,
                input.metrics_dpi,
                WidthSource::Fallback,
            ));
        }
        if let Some(asset_value) = asset {
            if let Some(width) = measure_text_width_pt(
                asset_value,
                input.text,
                input.font_size_pt,
                input.h_scale_pct,
                input.metrics_dpi,
            ) {
                if width.pt.is_finite() && width.pt > 0.0_f64 {
                    return Some(width);
                }
            }
        }
        let key = WidthTableKey {
            page_index: input.page_index,
            font_key: input.font_key.to_owned(),
        };
        if let Some(table) = width_tables.get(&key) {
            if let Some(width) = width_from_table(table, input.text, input.font_size_pt) {
                if width.is_finite() && width > 0.0_f64 {
                    let scale = (input.h_scale_pct as f64 / 100.0_f64).max(0.01_f64);
                    return Some(measured_width_from_points(
                        width * scale,
                        input.metrics_dpi,
                        WidthSource::PdfWidthTable,
                    ));
                }
            }
        }
        width_from_core_font(input.font_name, input.text, input.font_size_pt).and_then(|width| {
            let scale = (input.h_scale_pct as f64 / 100.0_f64).max(0.01_f64);
            let width_pt = width * scale;
            (width_pt.is_finite() && width_pt > 0.0_f64).then_some(measured_width_from_points(
                width_pt,
                input.metrics_dpi,
                WidthSource::CoreFont,
            ))
        })
    }

    fn build_pdf_width_table_map_from_bytes(
        pdf_bytes: &[u8],
    ) -> Result<std::collections::BTreeMap<WidthTableKey, WidthTable>, String> {
        let doc = Document::load_mem(pdf_bytes).map_err(|error| error.to_string())?;
        let pages = doc.get_pages();
        let mut map = std::collections::BTreeMap::new();

        for (page_no, page_id) in pages {
            let page_index = page_no.saturating_sub(1);
            let (resources_opt, _unused_pages) = doc
                .get_page_resources(page_id)
                .map_err(|error| error.to_string())?;
            let resources = match resources_opt {
                Some(resources) => resources,
                None => continue,
            };
            let font_object = match resources.get(b"Font").ok() {
                Some(object) => object,
                None => continue,
            };
            let font_dict = match deref_to_width_dict(&doc, font_object)
                .or_else(|| object_to_width_dict(font_object))
            {
                Some(dictionary) => dictionary,
                None => continue,
            };
            for (key_bytes, value_object) in font_dict.iter() {
                let font_key = String::from_utf8_lossy(key_bytes).to_string();
                let dict = match deref_to_width_dict(&doc, value_object)
                    .or_else(|| object_to_width_dict(value_object))
                {
                    Some(dictionary) => dictionary,
                    None => continue,
                };
                let width_dict = match resolve_width_target_dict(&doc, dict) {
                    Some(dictionary) => dictionary,
                    None => continue,
                };
                let first_char = width_dict
                    .get(b"FirstChar")
                    .ok()
                    .and_then(object_to_width_u16);
                let widths = width_dict
                    .get(b"Widths")
                    .ok()
                    .and_then(object_to_width_f64_array);
                let (first_char, widths) = match (first_char, widths) {
                    (Some(first), Some(widths)) if !widths.is_empty() => (first, widths),
                    _ => continue,
                };
                map.insert(
                    WidthTableKey {
                        page_index,
                        font_key,
                    },
                    WidthTable { first_char, widths },
                );
            }
        }

        Ok(map)
    }

    fn resolve_width_target_dict<'a>(
        doc: &'a Document,
        dict: &'a Dictionary,
    ) -> Option<&'a Dictionary> {
        if dict.has(b"Widths") {
            return Some(dict);
        }
        let subtype = dict.get(b"Subtype").ok().and_then(object_to_width_name);
        if subtype.as_deref() == Some("Type0") {
            let descendants = dict
                .get(b"DescendantFonts")
                .ok()
                .and_then(object_to_width_array);
            let first = descendants
                .and_then(|array| array.first())
                .and_then(|object| deref_to_width_dict(doc, object));
            if let Some(descendant) = first {
                if descendant.has(b"Widths") {
                    return Some(descendant);
                }
            }
        }
        None
    }

    fn width_from_table(table: &WidthTable, text: &str, font_size_pt: f32) -> Option<f64> {
        let mut sum = 0.0_f64;
        let mut any = false;
        for ch in text.chars() {
            let codepoint = ch as u32;
            if codepoint > u16::MAX as u32 {
                continue;
            }
            let codepoint = codepoint as u16;
            if codepoint < table.first_char {
                continue;
            }
            let index = (codepoint - table.first_char) as usize;
            if index >= table.widths.len() {
                continue;
            }
            sum += table.widths[index] * ((font_size_pt as f64) / 1000.0_f64);
            any = true;
        }
        any.then_some(sum)
    }

    fn width_from_core_font(font_name: &str, text: &str, font_size_pt: f32) -> Option<f64> {
        let normalized = font_name.to_ascii_lowercase();
        let table: fn(char) -> i32 = if normalized.contains("times") && normalized.contains("roman")
        {
            times_roman_width as fn(char) -> i32
        } else if normalized.contains("helvetica") {
            helvetica_width as fn(char) -> i32
        } else {
            return None;
        };
        let mut total_units = 0.0_f64;
        for ch in text.chars() {
            total_units += table(ch) as f64;
        }
        Some(total_units * ((font_size_pt as f64) / 1000.0_f64))
    }

    fn object_to_width_u16(object: &Object) -> Option<u16> {
        match object {
            Object::Integer(value) => (*value).try_into().ok(),
            Object::Real(value) => (*value as i64).try_into().ok(),
            _ => None,
        }
    }

    fn object_to_width_f64(object: &Object) -> Option<f64> {
        match object {
            Object::Real(value) => Some(*value as f64),
            Object::Integer(value) => Some(*value as f64),
            _ => None,
        }
    }

    fn object_to_width_f64_array(object: &Object) -> Option<Vec<f64>> {
        match object {
            Object::Array(values) => {
                let mut out = Vec::with_capacity(values.len());
                for item in values {
                    if let Some(value) = object_to_width_f64(item) {
                        out.push(value);
                    }
                }
                Some(out)
            }
            _ => None,
        }
    }

    fn object_to_width_array(object: &Object) -> Option<&Vec<Object>> {
        match object {
            Object::Array(values) => Some(values),
            _ => None,
        }
    }

    fn object_to_width_name(object: &Object) -> Option<String> {
        match object {
            Object::Name(name_bytes) => Some(String::from_utf8_lossy(name_bytes).to_string()),
            _ => None,
        }
    }

    fn object_to_width_dict(object: &Object) -> Option<&Dictionary> {
        match object {
            Object::Dictionary(dictionary) => Some(dictionary),
            _ => None,
        }
    }

    fn deref_to_width_dict<'doc>(
        doc: &'doc Document,
        object: &'doc Object,
    ) -> Option<&'doc Dictionary> {
        match object {
            Object::Reference(object_id) => match doc.get_object(*object_id).ok()? {
                Object::Dictionary(dictionary) => Some(dictionary),
                _ => None,
            },
            Object::Dictionary(dictionary) => Some(dictionary),
            _ => None,
        }
    }

    fn run_center_y(run: &FontTextRun) -> f32 {
        (run.bbox.y0 + run.bbox.y1) * 0.5
    }

    fn rect_center_y(rect: &Rect) -> f32 {
        (rect.y0 + rect.y1) * 0.5
    }

    fn advance_pt(
        face: &rustybuzz::Face<'_>,
        text: &str,
        font_size: f32,
        units_per_em: f32,
    ) -> f32 {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        let out = rustybuzz::shape(face, shaping_features(), buffer);
        let units = out
            .glyph_positions()
            .iter()
            .map(|position| position.x_advance as f32)
            .sum::<f32>()
            / GLYPH_UNITS_SCALE as f32;
        units * (font_size / units_per_em.max(1.0_f32))
    }

    fn shaping_features() -> &'static [rustybuzz::Feature] {
        static FEATURES: OnceLock<Vec<rustybuzz::Feature>> = OnceLock::new();
        FEATURES
            .get_or_init(|| {
                vec![
                    rustybuzz::Feature::new(rustybuzz::Tag::from_bytes(b"kern"), 1, ..),
                    rustybuzz::Feature::new(rustybuzz::Tag::from_bytes(b"liga"), 1, ..),
                    rustybuzz::Feature::new(rustybuzz::Tag::from_bytes(b"clig"), 1, ..),
                ]
            })
            .as_slice()
    }

    fn measured_width_from_points(width_pt: f64, _dpi: f32, source: WidthSource) -> MeasuredWidth {
        MeasuredWidth {
            pt: width_pt,
            source,
        }
    }

    fn times_roman_width(ch: char) -> i32 {
        match ch {
            ' ' => 250,
            '!' => 333,
            '"' => 408,
            '#' => 500,
            '$' => 500,
            '%' => 833,
            '&' => 778,
            '\'' => 180,
            '(' => 333,
            ')' => 333,
            '*' => 500,
            '+' => 564,
            ',' => 250,
            '-' => 333,
            '.' => 250,
            '/' => 278,
            '0' => 500,
            '1' => 500,
            '2' => 500,
            '3' => 500,
            '4' => 500,
            '5' => 500,
            '6' => 500,
            '7' => 500,
            '8' => 500,
            '9' => 500,
            ':' => 278,
            ';' => 278,
            '<' => 564,
            '=' => 564,
            '>' => 564,
            '?' => 444,
            '@' => 921,
            'A' => 722,
            'B' => 667,
            'C' => 667,
            'D' => 722,
            'E' => 611,
            'F' => 556,
            'G' => 722,
            'H' => 722,
            'I' => 333,
            'J' => 389,
            'K' => 722,
            'L' => 611,
            'M' => 889,
            'N' => 722,
            'O' => 722,
            'P' => 556,
            'Q' => 722,
            'R' => 667,
            'S' => 556,
            'T' => 611,
            'U' => 722,
            'V' => 722,
            'W' => 944,
            'X' => 722,
            'Y' => 722,
            'Z' => 611,
            '[' => 333,
            '\\' => 278,
            ']' => 333,
            '^' => 469,
            '_' => 500,
            '`' => 333,
            'a' => 444,
            'b' => 500,
            'c' => 444,
            'd' => 500,
            'e' => 444,
            'f' => 333,
            'g' => 500,
            'h' => 500,
            'i' => 278,
            'j' => 278,
            'k' => 500,
            'l' => 278,
            'm' => 778,
            'n' => 500,
            'o' => 500,
            'p' => 500,
            'q' => 500,
            'r' => 333,
            's' => 389,
            't' => 278,
            'u' => 500,
            'v' => 500,
            'w' => 722,
            'x' => 500,
            'y' => 500,
            'z' => 444,
            '{' => 480,
            '|' => 200,
            '}' => 480,
            '~' => 541,
            _ => 500,
        }
    }

    fn helvetica_width(ch: char) -> i32 {
        match ch {
            ' ' => 278,
            '!' => 278,
            '"' => 355,
            '#' => 556,
            '$' => 556,
            '%' => 889,
            '&' => 667,
            '\'' => 191,
            '(' => 333,
            ')' => 333,
            '*' => 389,
            '+' => 584,
            ',' => 278,
            '-' => 333,
            '.' => 278,
            '/' => 278,
            '0' => 556,
            '1' => 556,
            '2' => 556,
            '3' => 556,
            '4' => 556,
            '5' => 556,
            '6' => 556,
            '7' => 556,
            '8' => 556,
            '9' => 556,
            ':' => 278,
            ';' => 278,
            '<' => 584,
            '=' => 584,
            '>' => 584,
            '?' => 556,
            '@' => 1015,
            'A' => 667,
            'B' => 667,
            'C' => 722,
            'D' => 722,
            'E' => 667,
            'F' => 611,
            'G' => 778,
            'H' => 722,
            'I' => 278,
            'J' => 500,
            'K' => 667,
            'L' => 556,
            'M' => 833,
            'N' => 722,
            'O' => 778,
            'P' => 667,
            'Q' => 778,
            'R' => 722,
            'S' => 667,
            'T' => 611,
            'U' => 722,
            'V' => 667,
            'W' => 944,
            'X' => 667,
            'Y' => 667,
            'Z' => 611,
            '[' => 278,
            '\\' => 278,
            ']' => 278,
            '^' => 469,
            '_' => 556,
            '`' => 222,
            'a' => 556,
            'b' => 556,
            'c' => 500,
            'd' => 556,
            'e' => 556,
            'f' => 278,
            'g' => 556,
            'h' => 556,
            'i' => 222,
            'j' => 222,
            'k' => 500,
            'l' => 222,
            'm' => 833,
            'n' => 556,
            'o' => 556,
            'p' => 556,
            'q' => 556,
            'r' => 333,
            's' => 500,
            't' => 278,
            'u' => 556,
            'v' => 500,
            'w' => 722,
            'x' => 500,
            'y' => 500,
            'z' => 500,
            '{' => 334,
            '|' => 260,
            '}' => 334,
            '~' => 584,
            _ => 500,
        }
    }

    fn extract_context(
        redaction: &RedactionOccurrence,
    ) -> (String, String, Option<Rect>, Option<Rect>) {
        let left = redaction.underlying_text.first();
        let right = redaction.underlying_text.get(1);
        let left_text = left.map(|h| h.text.clone()).unwrap_or_default();
        let right_text = right.map(|h| h.text.clone()).unwrap_or_default();
        let left_bbox = left.map(|h| h.bbox);
        let right_bbox = right.map(|h| h.bbox);
        (left_text, right_text, left_bbox, right_bbox)
    }

    fn compute_gap_pt(
        red_bbox: Rect,
        left_bbox: Option<Rect>,
        right_bbox: Option<Rect>,
        left_anchor_text: &str,
        right_anchor_text: &str,
    ) -> f64 {
        if !left_anchor_text.trim().is_empty() && !right_anchor_text.trim().is_empty() {
            if let (Some(l), Some(r)) = (left_bbox, right_bbox) {
                return (r.x0 - l.x1).max(0.0) as f64;
            }
        }
        let w = red_bbox.width().abs();
        if w > 0.0 {
            return w as f64;
        }
        0.0
    }

    fn estimate_char_width_pt(
        left_anchor_text: &str,
        right_anchor_text: &str,
        left_bbox: Option<Rect>,
        right_bbox: Option<Rect>,
        red_bbox: Rect,
    ) -> f64 {
        let mut samples = Vec::new();
        if let (Some(b), count) = (left_bbox, left_anchor_text.chars().count()) {
            if count > 0 {
                let w = b.width().abs() as f64;
                if w > 0.0_f64 {
                    samples.push(w / count as f64);
                }
            }
        }
        if let (Some(b), count) = (right_bbox, right_anchor_text.chars().count()) {
            if count > 0 {
                let w = b.width().abs() as f64;
                if w > 0.0_f64 {
                    samples.push(w / count as f64);
                }
            }
        }
        if !samples.is_empty() {
            let sum = samples.iter().sum::<f64>();
            return sum / samples.len() as f64;
        }
        let fallback = red_bbox.height().abs() as f64 * 0.5_f64;
        if fallback > 0.0 {
            fallback
        } else {
            6.0
        }
    }

    fn fallback_measured_width(
        text: &str,
        fallback_char_width: f64,
        fallback_space_width: f64,
        dpi: f32,
    ) -> MeasuredWidth {
        let width_pt = text
            .chars()
            .map(|ch| {
                if ch.is_whitespace() {
                    fallback_space_width
                } else {
                    fallback_char_width.max(0.1_f64)
                }
            })
            .sum::<f64>();
        measured_width_from_points(width_pt, dpi, WidthSource::Fallback)
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct WidthKey {
        page_index: u32,
        font_key: String,
        font_size_bits: u32,
        h_scale_bits: u32,
        metrics_dpi_bits: u32,
    }

    #[derive(Debug, Clone)]
    struct CandidateWidthEntry {
        text: String,
        width_pt: f64,
        source: WidthSource,
    }

    struct WidthCache {
        measured:
            std::collections::BTreeMap<WidthKey, std::collections::BTreeMap<String, MeasuredWidth>>,
        variants: std::collections::BTreeMap<String, Vec<String>>,
    }

    impl WidthCache {
        fn new() -> Self {
            Self {
                measured: std::collections::BTreeMap::new(),
                variants: std::collections::BTreeMap::new(),
            }
        }
    }

    fn build_row_candidate_width_index(
        dictionary: &[String],
        key: &WidthKey,
        cache: &mut WidthCache,
        min_char_units: f64,
        max_char_units: f64,
        measure_width: &dyn Fn(&str) -> Option<MeasuredWidth>,
    ) -> Vec<CandidateWidthEntry> {
        let mut seen = std::collections::BTreeSet::<String>::new();
        let mut out = Vec::<CandidateWidthEntry>::new();
        for entry in dictionary {
            for variant in entry_variants(entry, cache) {
                let trimmed = variant.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let char_units = candidate_char_units(trimmed);
                if char_units < min_char_units || char_units > max_char_units {
                    continue;
                }
                if !seen.insert(trimmed.to_owned()) {
                    continue;
                }
                let measured = measured_candidate_width(key, trimmed, cache, measure_width);
                out.push(CandidateWidthEntry {
                    text: trimmed.to_owned(),
                    width_pt: measured.pt,
                    source: measured.source,
                });
            }
        }
        out.sort_by(|left, right| {
            left.width_pt
                .partial_cmp(&right.width_pt)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.text.cmp(&right.text))
        });
        out
    }

    fn measured_candidate_width(
        key: &WidthKey,
        text: &str,
        cache: &mut WidthCache,
        measure_width: &dyn Fn(&str) -> Option<MeasuredWidth>,
    ) -> MeasuredWidth {
        if let Some(existing) = cache
            .measured
            .get(key)
            .and_then(|rows| rows.get(text))
            .copied()
        {
            return existing;
        }
        let measured = measure_width(text).unwrap_or_else(|| {
            measured_width_from_points(0.0_f64, DEFAULT_METRICS_DPI, WidthSource::Fallback)
        });
        cache
            .measured
            .entry(key.clone())
            .or_default()
            .insert(text.to_owned(), measured);
        measured
    }

    fn entry_variants(entry: &str, cache: &mut WidthCache) -> Vec<String> {
        let canonical = normalize_dictionary_entry(entry);
        if canonical.is_empty() {
            return Vec::new();
        }
        if let Some(variants) = cache.variants.get(&canonical) {
            return variants.clone();
        }
        let variants = build_name_variants(&canonical);
        cache.variants.insert(canonical, variants.clone());
        variants
    }

    fn normalize_dictionary_entry(value: &str) -> String {
        let mut out = String::with_capacity(value.len());
        let mut in_space = false;
        for ch in value.chars() {
            if ch.is_whitespace() {
                if !in_space && !out.is_empty() {
                    out.push(' ');
                }
                in_space = true;
            } else {
                out.push(ch);
                in_space = false;
            }
        }
        out.trim().to_owned()
    }

    fn build_name_variants(canonical: &str) -> Vec<String> {
        let mut template_seen = std::collections::BTreeSet::<String>::new();
        let mut templates = Vec::<String>::new();
        push_unique_variant(&mut template_seen, &mut templates, canonical.to_owned());

        let tokens = canonical
            .split_whitespace()
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        if !tokens.is_empty() && has_special_name_structure(canonical, &tokens) {
            let parts = parse_name_parts(canonical, &tokens);
            let core = join_name_tokens(&parts.core_tokens);
            let given = join_name_tokens(&parts.given_tokens);
            let surname = join_name_tokens(&parts.surname_tokens);
            let given_first = parts.given_tokens.first().cloned().unwrap_or_default();
            let surname_last = parts.surname_tokens.last().cloned().unwrap_or_default();
            let prefix = join_name_tokens(&parts.prefix_tokens);
            let suffix = join_name_tokens(&parts.suffix_tokens);

            if !core.is_empty() {
                push_unique_variant(&mut template_seen, &mut templates, core.clone());
            }
            if !given_first.is_empty() && !surname.is_empty() {
                push_unique_variant(
                    &mut template_seen,
                    &mut templates,
                    format!("{given_first} {surname}"),
                );
            }
            if !surname.is_empty() && !given_first.is_empty() {
                push_unique_variant(
                    &mut template_seen,
                    &mut templates,
                    format!("{surname}, {given_first}"),
                );
            }
            if !prefix.is_empty() && !given_first.is_empty() && !surname.is_empty() {
                push_unique_variant(
                    &mut template_seen,
                    &mut templates,
                    format!("{prefix} {given_first} {surname}"),
                );
            }
            if !suffix.is_empty() && !given_first.is_empty() && !surname.is_empty() {
                push_unique_variant(
                    &mut template_seen,
                    &mut templates,
                    format!("{given_first} {surname} {suffix}"),
                );
            }
            if !prefix.is_empty()
                && !suffix.is_empty()
                && !given_first.is_empty()
                && !surname.is_empty()
            {
                push_unique_variant(
                    &mut template_seen,
                    &mut templates,
                    format!("{prefix} {given_first} {surname} {suffix}"),
                );
            }
            if !given.is_empty() && !surname.is_empty() {
                push_unique_variant(
                    &mut template_seen,
                    &mut templates,
                    format!("{given} {surname}"),
                );
            }
            if !given.is_empty() && !surname.is_empty() && !suffix.is_empty() {
                push_unique_variant(
                    &mut template_seen,
                    &mut templates,
                    format!("{given} {surname} {suffix}"),
                );
            }
            if !prefix.is_empty() && !given.is_empty() && !surname.is_empty() {
                push_unique_variant(
                    &mut template_seen,
                    &mut templates,
                    format!("{prefix} {given} {surname}"),
                );
            }
            if !prefix.is_empty() && !given.is_empty() && !surname.is_empty() && !suffix.is_empty()
            {
                push_unique_variant(
                    &mut template_seen,
                    &mut templates,
                    format!("{prefix} {given} {surname} {suffix}"),
                );
            }
            if !given_first.is_empty() {
                push_unique_variant(&mut template_seen, &mut templates, given_first.clone());
            }
            if !surname.is_empty() {
                push_unique_variant(&mut template_seen, &mut templates, surname.clone());
            }
            if !surname_last.is_empty() {
                push_unique_variant(&mut template_seen, &mut templates, surname_last);
            }
            if !prefix.is_empty() && !surname.is_empty() {
                push_unique_variant(
                    &mut template_seen,
                    &mut templates,
                    format!("{prefix} {surname}"),
                );
            }
            if !suffix.is_empty() && !surname.is_empty() {
                push_unique_variant(
                    &mut template_seen,
                    &mut templates,
                    format!("{surname} {suffix}"),
                );
            }
            if !core.is_empty() && !canonical.contains(',') {
                let mut split = core.split_whitespace();
                if let (Some(first), Some(last)) = (split.next(), core.split_whitespace().last()) {
                    if first != last {
                        push_unique_variant(
                            &mut template_seen,
                            &mut templates,
                            format!("{last}, {first}"),
                        );
                    }
                }
            }
            if parts.given_tokens.len() >= 2 && !surname.is_empty() {
                let middle_initials = parts.given_tokens[1..]
                    .iter()
                    .filter_map(|value| value.chars().next())
                    .map(|ch| format!("{ch}."))
                    .collect::<Vec<_>>()
                    .join(" ");
                if !middle_initials.is_empty() && !given_first.is_empty() {
                    push_unique_variant(
                        &mut template_seen,
                        &mut templates,
                        format!("{given_first} {middle_initials} {surname}"),
                    );
                }
            }
        } else if tokens.len() >= 2 {
            let first = tokens[0];
            let last = tokens[tokens.len() - 1];
            push_unique_variant(
                &mut template_seen,
                &mut templates,
                format!("{first} {last}"),
            );
            if !canonical.contains(',') {
                push_unique_variant(
                    &mut template_seen,
                    &mut templates,
                    format!("{last}, {first}"),
                );
            }
            push_unique_variant(&mut template_seen, &mut templates, first.to_owned());
            push_unique_variant(&mut template_seen, &mut templates, last.to_owned());
        }

        finalize_name_variants(&templates)
    }

    fn has_special_name_structure(canonical: &str, tokens: &[&str]) -> bool {
        canonical.contains(',')
            || tokens
                .iter()
                .any(|token| is_name_prefix_token(token) || is_name_suffix_token(token))
            || tokens
                .iter()
                .take(tokens.len().saturating_sub(1))
                .any(|token| is_surname_particle_token(token))
    }

    fn finalize_name_variants(templates: &[String]) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::<String>::new();
        let mut out = Vec::<String>::new();
        for template in templates {
            push_unique_variant(&mut seen, &mut out, template.clone());
            push_unique_variant(&mut seen, &mut out, template.to_uppercase());
            push_unique_variant(&mut seen, &mut out, template.to_lowercase());
            push_unique_variant(&mut seen, &mut out, title_case_text(template));
            if out.len() >= MAX_NAME_VARIANTS_PER_ENTRY {
                break;
            }
        }
        if out.len() > MAX_NAME_VARIANTS_PER_ENTRY {
            out.truncate(MAX_NAME_VARIANTS_PER_ENTRY);
        }
        out
    }

    #[derive(Debug, Clone, Default)]
    struct NameParts {
        prefix_tokens: Vec<String>,
        given_tokens: Vec<String>,
        surname_tokens: Vec<String>,
        suffix_tokens: Vec<String>,
        core_tokens: Vec<String>,
    }

    fn parse_name_parts(canonical: &str, tokens: &[&str]) -> NameParts {
        parse_comma_name_parts(canonical).unwrap_or_else(|| parse_space_name_parts(tokens))
    }

    fn parse_comma_name_parts(canonical: &str) -> Option<NameParts> {
        let (left, right) = canonical.split_once(',')?;
        let left_tokens = split_name_tokens(left);
        let right_tokens = split_name_tokens(right);
        if left_tokens.is_empty() || right_tokens.is_empty() {
            return None;
        }
        let (prefix_tokens, mut right_core_tokens, suffix_tokens) =
            split_prefix_suffix(&right_tokens);
        if right_core_tokens.is_empty() {
            right_core_tokens = right_tokens;
        }
        let given_tokens = right_core_tokens;
        let surname_tokens = left_tokens;
        let mut core_tokens = Vec::<String>::new();
        core_tokens.extend(given_tokens.iter().cloned());
        core_tokens.extend(surname_tokens.iter().cloned());
        if core_tokens.is_empty() {
            core_tokens.extend(surname_tokens.iter().cloned());
        }
        Some(NameParts {
            prefix_tokens,
            given_tokens,
            surname_tokens,
            suffix_tokens,
            core_tokens,
        })
    }

    fn parse_space_name_parts(tokens: &[&str]) -> NameParts {
        let all_tokens = tokens
            .iter()
            .map(|token| normalize_dictionary_entry(token))
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>();
        let (prefix_tokens, mut core_tokens, suffix_tokens) = split_prefix_suffix(&all_tokens);
        if core_tokens.is_empty() {
            core_tokens = all_tokens;
        }
        let (mut given_tokens, mut surname_tokens) = split_given_surname(&core_tokens);
        if given_tokens.is_empty() && !core_tokens.is_empty() {
            given_tokens.push(core_tokens[0].clone());
        }
        if surname_tokens.is_empty() && !core_tokens.is_empty() {
            surname_tokens.push(core_tokens[core_tokens.len() - 1].clone());
        }
        NameParts {
            prefix_tokens,
            given_tokens,
            surname_tokens,
            suffix_tokens,
            core_tokens,
        }
    }

    fn split_name_tokens(value: &str) -> Vec<String> {
        value
            .split_whitespace()
            .map(normalize_dictionary_entry)
            .filter(|token| !token.is_empty())
            .collect::<Vec<_>>()
    }

    fn split_prefix_suffix(tokens: &[String]) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut prefix_end = 0_usize;
        while prefix_end < tokens.len() && is_name_prefix_token(&tokens[prefix_end]) {
            prefix_end += 1;
        }
        let mut suffix_start = tokens.len();
        while suffix_start > prefix_end && is_name_suffix_token(&tokens[suffix_start - 1]) {
            suffix_start -= 1;
        }
        (
            tokens[..prefix_end].to_vec(),
            tokens[prefix_end..suffix_start].to_vec(),
            tokens[suffix_start..].to_vec(),
        )
    }

    fn split_given_surname(tokens: &[String]) -> (Vec<String>, Vec<String>) {
        if tokens.is_empty() {
            return (Vec::new(), Vec::new());
        }
        if tokens.len() == 1 {
            return (vec![tokens[0].clone()], Vec::new());
        }
        let mut surname_start = tokens.len() - 1;
        while surname_start > 0 && is_surname_particle_token(&tokens[surname_start - 1]) {
            surname_start -= 1;
        }
        if surname_start == 0 {
            return (Vec::new(), tokens.to_vec());
        }
        (
            tokens[..surname_start].to_vec(),
            tokens[surname_start..].to_vec(),
        )
    }

    fn join_name_tokens(tokens: &[String]) -> String {
        tokens.join(" ")
    }

    fn name_token_lookup_key(value: &str) -> String {
        value
            .trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '\'' && ch != '-')
            .trim_end_matches('.')
            .to_ascii_lowercase()
    }

    fn is_name_prefix_token(value: &str) -> bool {
        let key = name_token_lookup_key(value);
        !key.is_empty() && NAME_PREFIX_TOKENS.contains(&key.as_str())
    }

    fn is_name_suffix_token(value: &str) -> bool {
        let key = name_token_lookup_key(value);
        !key.is_empty() && NAME_SUFFIX_TOKENS.contains(&key.as_str())
    }

    fn is_surname_particle_token(value: &str) -> bool {
        let key = name_token_lookup_key(value);
        !key.is_empty() && NAME_SURNAME_PARTICLE_TOKENS.contains(&key.as_str())
    }

    fn push_unique_variant(
        seen: &mut std::collections::BTreeSet<String>,
        out: &mut Vec<String>,
        value: String,
    ) {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        let normalized = normalize_dictionary_entry(trimmed);
        if normalized.is_empty() {
            return;
        }
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }

    fn title_case_text(value: &str) -> String {
        let mut out = String::with_capacity(value.len());
        let mut new_word = true;
        for ch in value.chars() {
            if ch.is_alphabetic() {
                if new_word {
                    out.extend(ch.to_uppercase());
                    new_word = false;
                } else {
                    out.extend(ch.to_lowercase());
                }
            } else {
                new_word = ch == ' ' || ch == '-' || ch == '\'';
                out.push(ch);
            }
        }
        out
    }

    fn vertical_overlap_run(a: &FontRect, b: &Rect) -> f32 {
        (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0)
    }

    fn collect_row_runs_for_anchor<'a>(
        redaction: &RedactionOccurrence,
        runs: &[&'a FontTextRun],
        tight: bool,
    ) -> Vec<&'a FontTextRun> {
        let red_rect = Rect::new(
            redaction.bbox.x0,
            redaction.bbox.y0,
            redaction.bbox.x1,
            redaction.bbox.y1,
        );
        let red_center_y = rect_center_y(&redaction.bbox);
        let red_center_x = ((redaction.bbox.x0 + redaction.bbox.x1) * 0.5) as f64;
        let y_tolerance = if tight { 12.0_f32 } else { 20.0_f32 };
        let baseline_tolerance = if tight { 8.0_f32 } else { 16.0_f32 };
        let x_tolerance = if tight { 120.0_f64 } else { 220.0_f64 };

        runs.iter()
            .copied()
            .filter(|run| {
                let overlap = vertical_overlap_run(&run.bbox, &red_rect);
                let center_distance = (run_center_y(run) - red_center_y).abs();
                let baseline_distance = (run.bbox.y1 - redaction.bbox.y1).abs();
                let left_gap = (redaction.bbox.x0 as f64 - run.bbox.x1 as f64).abs();
                let right_gap = (run.bbox.x0 as f64 - redaction.bbox.x1 as f64).abs();
                let contains_center =
                    run.bbox.x0 as f64 <= red_center_x && run.bbox.x1 as f64 >= red_center_x;
                let near_x = left_gap <= x_tolerance || right_gap <= x_tolerance || contains_center;
                let near_y = overlap > 0.0
                    || center_distance <= y_tolerance
                    || baseline_distance <= baseline_tolerance;
                if tight {
                    near_x && near_y
                } else {
                    near_y
                }
            })
            .collect::<Vec<_>>()
    }

    fn text_matches(run_text: &str, target: &str) -> bool {
        if run_text == target {
            return true;
        }
        run_text.contains(target) || target.contains(run_text)
    }

    fn candidate_width_entries_in_range(
        entries: &[CandidateWidthEntry],
        min_width_pt: f64,
        max_width_pt: f64,
    ) -> &[CandidateWidthEntry] {
        if entries.is_empty() || !min_width_pt.is_finite() || !max_width_pt.is_finite() {
            return &[];
        }
        if min_width_pt > max_width_pt {
            return &[];
        }
        let start = entries.partition_point(|entry| entry.width_pt < min_width_pt);
        let end = entries.partition_point(|entry| entry.width_pt <= max_width_pt);
        if start >= end || start >= entries.len() {
            return &[];
        }
        &entries[start..end.min(entries.len())]
    }

    fn trim_width_band_around_target(
        entries: &[CandidateWidthEntry],
        target_width_pt: f64,
        limit: usize,
    ) -> &[CandidateWidthEntry] {
        if entries.len() <= limit || limit == 0 {
            return entries;
        }
        let mid = entries.partition_point(|entry| entry.width_pt < target_width_pt);
        let half = limit >> 1_usize;
        let mut start = mid.saturating_sub(half);
        let mut end = start.saturating_add(limit).min(entries.len());
        if end - start < limit {
            start = end.saturating_sub(limit);
        }
        end = end.max(start);
        &entries[start..end]
    }

    fn passes_context_filter(
        left_anchor_text: &str,
        right_anchor_text: &str,
        candidate: &str,
    ) -> bool {
        let left_lower = left_anchor_text.to_ascii_lowercase();
        let right_lower = right_anchor_text.to_ascii_lowercase();
        if right_lower.starts_with("and") && candidate.contains(',') {
            return false;
        }
        if left_lower.contains("including") && right_lower.starts_with("and") {
            let count = candidate.split_whitespace().count();
            if count > 3 {
                return false;
            }
        }
        true
    }

    fn is_list_like_context(left_anchor_text: &str, right_anchor_text: &str) -> bool {
        let left_lower = left_anchor_text.trim().to_ascii_lowercase();
        let right_lower = right_anchor_text.trim().to_ascii_lowercase();
        left_lower.contains("including")
            || left_lower.contains("included")
            || left_lower.contains("among")
            || left_lower.contains("served")
            || left_lower.ends_with(',')
            || right_lower.starts_with(',')
            || right_lower.ends_with(',')
            || right_lower.starts_with("and ")
    }

    fn looks_like_alpha_phrase_candidate(candidate: &str) -> bool {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            return false;
        }
        if trimmed.contains(',')
            || trimmed.contains('(')
            || trimmed.contains(')')
            || trimmed.contains('/')
            || trimmed.contains('&')
        {
            return false;
        }
        if trimmed.chars().any(|ch| ch.is_ascii_digit()) {
            return false;
        }
        let words = trimmed.split_whitespace().collect::<Vec<_>>();
        if words.is_empty() || words.len() > 4 {
            return false;
        }
        words.iter().all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|ch| ch.is_ascii_alphabetic() || ch == '-' || ch == '\'')
        })
    }

    fn punctuation_context_penalty(
        left_anchor_text: &str,
        right_anchor_text: &str,
        candidate: &str,
    ) -> f64 {
        let left_lower = left_anchor_text.trim().to_ascii_lowercase();
        let right_lower = right_anchor_text.trim().to_ascii_lowercase();
        let candidate_trim = candidate.trim();
        if candidate_trim.is_empty() {
            return 5.0;
        }

        let word_count = candidate_trim.split_whitespace().count();
        let mut penalty = 0.0_f64;
        let list_context = is_list_like_context(&left_lower, &right_lower);

        if list_context {
            if word_count == 1 || word_count >= 5 {
                penalty += 0.20_f64;
            }
            if candidate_trim.contains('-') {
                penalty += 0.30_f64;
            }
            if candidate_trim.contains(',') {
                penalty += 0.45_f64;
            }
            if candidate_trim.contains('(') || candidate_trim.contains(')') {
                penalty += 0.40_f64;
            }
            if candidate_trim.contains('/') || candidate_trim.contains('&') {
                penalty += 0.50_f64;
            }
        }

        if (right_lower.starts_with(',') || right_lower.starts_with("and "))
            && (candidate_trim.ends_with(',') || candidate_trim.ends_with(';'))
        {
            penalty += 0.25_f64;
        }

        if candidate_trim.chars().any(|ch| ch.is_ascii_digit()) {
            penalty += 0.20_f64;
        }

        penalty.max(0.0)
    }

    #[cfg(test)]
    mod tests {
        use super::{run_from_bytes, RunGuessFromBytesRequest};
        use crate::logic::redaction_guessing_component::{
            build_report_from_input_name, run_redaction_scan_from_bytes,
        };
        use crate::types::guess_types::GuessConfig;
        use crate::types::redaction_types::{RedactionFinderConfig, RedactionMode};

        fn sample_pdf_bytes() -> Vec<u8> {
            let input = std::path::Path::new("test_data/EFTA00101126.pdf");
            std::fs::read(input)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", input.display()))
        }

        fn sample_redaction_report(
            pdf_bytes: &[u8],
        ) -> crate::types::redaction_types::RedactionReport {
            let cfg = RedactionFinderConfig {
                include_details: false,
                mode: RedactionMode::All,
                include_full_page_rects: false,
                enable_image_analysis: false,
                raster_dpi: 96.0_f32,
            };
            let scan = run_redaction_scan_from_bytes(pdf_bytes, None, cfg)
                .expect("redaction scan should succeed");
            build_report_from_input_name("memory://EFTA00101126.pdf", scan)
        }

        fn diagnostics_without_timing(lines: &[String]) -> Vec<String> {
            lines
                .iter()
                .filter(|line| !line.starts_with("timing_ms stage="))
                .cloned()
                .collect::<Vec<_>>()
        }

        #[test]
        fn run_from_bytes_is_deterministic_for_same_inputs() {
            let pdf_bytes = sample_pdf_bytes();
            let redactions = sample_redaction_report(&pdf_bytes);
            let dictionary = vec![
                "SARAH KELLEN".to_owned(),
                "GHISLANE MAXWELL".to_owned(),
                "ALPHA".to_owned(),
            ];
            let diagnostics = vec![
                "dictionary_source=file".to_owned(),
                "dictionary_size=3".to_owned(),
            ];
            let cfg = GuessConfig {
                visual_score: false,
                visual_score_dpi: 200.0_f32,
            };

            let report_a = run_from_bytes(RunGuessFromBytesRequest {
                pdf_name: "EFTA00101126.pdf",
                pdf_bytes: &pdf_bytes,
                redactions: &redactions,
                dictionary: &dictionary,
                diagnostics: &diagnostics,
                cfg: &cfg,
            })
            .expect("guessing should succeed");
            let report_b = run_from_bytes(RunGuessFromBytesRequest {
                pdf_name: "EFTA00101126.pdf",
                pdf_bytes: &pdf_bytes,
                redactions: &redactions,
                dictionary: &dictionary,
                diagnostics: &diagnostics,
                cfg: &cfg,
            })
            .expect("guessing should succeed");

            assert_eq!(report_a.input_redactions, report_b.input_redactions);
            assert_eq!(report_a.input_fonts, report_b.input_fonts);
            assert_eq!(report_a.guesses, report_b.guesses);
            assert_eq!(
                diagnostics_without_timing(&report_a.diagnostics),
                diagnostics_without_timing(&report_b.diagnostics)
            );
        }

        #[test]
        fn run_from_bytes_propagates_dictionary_diagnostics() {
            let pdf_bytes = sample_pdf_bytes();
            let redactions = sample_redaction_report(&pdf_bytes);
            let dictionary = vec!["SARAH KELLEN".to_owned()];
            let diagnostics = vec![
                "dictionary_source=file".to_owned(),
                "dictionary_size=1".to_owned(),
            ];
            let cfg = GuessConfig {
                visual_score: false,
                visual_score_dpi: 200.0_f32,
            };

            let report = run_from_bytes(RunGuessFromBytesRequest {
                pdf_name: "EFTA00101126.pdf",
                pdf_bytes: &pdf_bytes,
                redactions: &redactions,
                dictionary: &dictionary,
                diagnostics: &diagnostics,
                cfg: &cfg,
            })
            .expect("guessing should succeed");

            for expected in diagnostics {
                assert!(report.diagnostics.iter().any(|line| line == &expected));
            }
        }

        #[test]
        fn run_from_bytes_emits_visual_score_disabled_marker_when_off() {
            let pdf_bytes = sample_pdf_bytes();
            let redactions = sample_redaction_report(&pdf_bytes);
            let dictionary = vec!["SARAH KELLEN".to_owned()];
            let cfg = GuessConfig {
                visual_score: false,
                visual_score_dpi: 200.0_f32,
            };

            let report = run_from_bytes(RunGuessFromBytesRequest {
                pdf_name: "EFTA00101126.pdf",
                pdf_bytes: &pdf_bytes,
                redactions: &redactions,
                dictionary: &dictionary,
                diagnostics: &[],
                cfg: &cfg,
            })
            .expect("guessing should succeed");

            assert!(report
                .diagnostics
                .iter()
                .any(|line| line == "visual_score=disabled"));
        }
    }
}

mod redaction_impl {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::data::redactions_data::{PdfFileRetriever, RedactionDataRetriever};
    use crate::logic::time::Instant;
    use crate::types::redaction_types::{
        PdfRenderer, Rect, RedactionFinderConfig, RedactionFinderOutput, RedactionMode,
        RedactionOccurrence, RedactionReport, UnderlyingTextHit,
    };

    const LINE_BUCKET_PT: f32 = 2.0;
    const Y_BAND_PADDING_PT: f32 = 2.0;
    const WORD_JOIN_GAP_PT: f32 = 30.0;
    const LINE_SEARCH_WINDOW_PT: f32 = 18.0;
    const MAX_CONTEXT_GAP_PT: f32 = 80.0;
    const LARGE_OVERLAP_PT: f32 = 20.0;
    const MAX_CONTEXT_WORDS_PER_SIDE: usize = 2;
    const RASTER_PREPASS_DPI: f32 = 18.0;
    const RASTER_HIGHPASS_DPI: f32 = 96.0;
    type LineMatchScore = (i32, i32, i32, i32);
    type LineMatch = (Vec<usize>, Option<usize>, Option<usize>, LineMatchScore);

    #[derive(Debug, Clone, Copy)]
    struct ScanPlan {
        include_annotations: bool,
        include_drawn: bool,
        include_raster: bool,
    }

    impl ScanPlan {
        #[inline]
        fn from_cfg(cfg: &RedactionFinderConfig) -> Self {
            let include_annotations =
                matches!(cfg.mode, RedactionMode::Annotations | RedactionMode::All);
            let include_drawn = matches!(cfg.mode, RedactionMode::Drawn | RedactionMode::All);
            Self {
                include_annotations,
                include_drawn,
                include_raster: cfg.enable_image_analysis,
            }
        }
    }

    #[derive(Debug, Default)]
    struct ScanAccumulator {
        occurrences: Vec<RedactionOccurrence>,
        diagnostics: Vec<String>,
    }

    #[inline]
    pub fn run_redaction_scan_from_bytes(
        bytes: &[u8],
        renderer: Option<&dyn PdfRenderer>,
        cfg: RedactionFinderConfig,
    ) -> Result<RedactionFinderOutput, String> {
        let retriever = PdfFileRetriever::new_from_bytes(bytes, renderer)?;
        Ok(run_redaction_scan(&retriever, cfg))
    }

    #[inline]
    pub fn run_redaction_scan(
        retriever: &dyn RedactionDataRetriever,
        cfg: RedactionFinderConfig,
    ) -> RedactionFinderOutput {
        let page_indices = retriever.page_indices();
        let plan = ScanPlan::from_cfg(&cfg);
        let mut acc = ScanAccumulator::default();

        collect_vector_redactions(retriever, &page_indices, &cfg, plan, &mut acc);

        if plan.include_raster {
            acc.occurrences.extend(collect_raster_redactions_two_pass(
                retriever,
                &page_indices,
                &cfg,
                &mut acc.diagnostics,
            ));
        }

        for page_index in page_indices {
            attach_underlying_text(
                retriever,
                page_index,
                &mut acc.occurrences,
                &mut acc.diagnostics,
            );
        }

        RedactionFinderOutput {
            redactions: dedup_occurrences(acc.occurrences),
            diagnostics: acc.diagnostics,
        }
    }

    fn collect_vector_redactions(
        retriever: &dyn RedactionDataRetriever,
        page_indices: &[u32],
        cfg: &RedactionFinderConfig,
        plan: ScanPlan,
        acc: &mut ScanAccumulator,
    ) {
        for page_index in page_indices {
            if plan.include_annotations {
                collect_page_annotations(retriever, *page_index, cfg.include_details, acc);
            }
            if plan.include_drawn {
                collect_page_drawn(retriever, *page_index, cfg.include_details, acc);
            }
        }
    }

    fn collect_page_annotations(
        retriever: &dyn RedactionDataRetriever,
        page_index: u32,
        include_details: bool,
        acc: &mut ScanAccumulator,
    ) {
        match retriever.annotation_redactions(page_index, include_details) {
            Ok(v) => acc.occurrences.extend(v),
            Err(m) => acc
                .diagnostics
                .push(format!("page_index={page_index} annotation_error={m}")),
        }
    }

    fn collect_page_drawn(
        retriever: &dyn RedactionDataRetriever,
        page_index: u32,
        include_details: bool,
        acc: &mut ScanAccumulator,
    ) {
        match retriever.drawn_redactions(page_index, include_details, false) {
            Ok(v) => acc.occurrences.extend(v),
            Err(m) => acc
                .diagnostics
                .push(format!("page_index={page_index} drawn_error={m}")),
        }
    }

    #[inline]
    pub fn build_report_from_input_name(
        input_name: &str,
        output: RedactionFinderOutput,
    ) -> RedactionReport {
        let mut occs = output.redactions;
        occs.sort_by(|a, b| {
            a.page_index
                .cmp(&b.page_index)
                .then_with(|| {
                    a.bbox
                        .x0
                        .partial_cmp(&b.bbox.x0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    a.bbox
                        .y0
                        .partial_cmp(&b.bbox.y0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        let mut page_counts: BTreeMap<u32, u32> = BTreeMap::new();
        for occurrence in &occs {
            *page_counts.entry(occurrence.page_index).or_insert(0) += 1;
        }

        RedactionReport {
            input: input_name.to_owned(),
            redactions: occs.clone(),
            count: occs.len() as u32,
            page_counts,
            diagnostics: output.diagnostics,
        }
    }

    fn dedup_occurrences(items: Vec<RedactionOccurrence>) -> Vec<RedactionOccurrence> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut out = Vec::new();

        for item in items {
            let key = format!(
                "{}:{:.2}:{:.2}:{:.2}:{:.2}:{:?}",
                item.page_index, item.bbox.x0, item.bbox.y0, item.bbox.x1, item.bbox.y1, item.kind
            );
            if seen.insert(key) {
                out.push(item);
            }
        }

        out
    }

    fn collect_raster_redactions_two_pass(
        retriever: &dyn RedactionDataRetriever,
        page_indices: &[u32],
        cfg: &RedactionFinderConfig,
        diagnostics: &mut Vec<String>,
    ) -> Vec<RedactionOccurrence> {
        if page_indices.is_empty() {
            diagnostics.push("raster_two_pass=pages=0 mode=skipped_empty".to_owned());
            return Vec::new();
        }

        let prepass_cfg = RedactionFinderConfig {
            raster_dpi: RASTER_PREPASS_DPI,
            ..*cfg
        };
        let prepass_started = Instant::now();
        let mut prepass_by_page = BTreeMap::<u32, Vec<RedactionOccurrence>>::new();
        let mut candidate_pages = Vec::<u32>::new();
        for page_index in page_indices {
            match retriever.raster_redactions(*page_index, &prepass_cfg) {
                Ok(v) => {
                    if !v.is_empty() {
                        candidate_pages.push(*page_index);
                        prepass_by_page.insert(*page_index, v);
                    }
                }
                Err(m) => {
                    diagnostics.push(format!("page_index={page_index} raster_prepass_error={m}"))
                }
            }
        }
        let prepass_ms = prepass_started.elapsed().as_millis();

        let highpass_cfg = RedactionFinderConfig {
            raster_dpi: RASTER_HIGHPASS_DPI,
            ..*cfg
        };
        let highpass_started = Instant::now();
        let mut out = Vec::<RedactionOccurrence>::new();
        let mut highpass_pages = 0_usize;
        let mut highpass_fallback_pages = 0_usize;
        for page_index in &candidate_pages {
            highpass_pages += 1;
            match retriever.raster_redactions(*page_index, &highpass_cfg) {
                Ok(v) if !v.is_empty() => out.extend(v),
                Ok(_) => {
                    highpass_fallback_pages += 1;
                    if let Some(prepass_hits) = prepass_by_page.remove(page_index) {
                        out.extend(prepass_hits);
                    }
                }
                Err(m) => {
                    diagnostics.push(format!("page_index={page_index} raster_highpass_error={m}"));
                    highpass_fallback_pages += 1;
                    if let Some(prepass_hits) = prepass_by_page.remove(page_index) {
                        out.extend(prepass_hits);
                    }
                }
            }
        }
        let highpass_ms = highpass_started.elapsed().as_millis();
        diagnostics.push(format!(
            "raster_two_pass=pages={} candidate_pages={} non_candidate_pages={} prepass_dpi={:.1} highpass_dpi={:.1} requested_dpi={:.1} prepass_ms={} highpass_pages={} highpass_ms={} highpass_fallback_pages={}",
            page_indices.len(),
            candidate_pages.len(),
            page_indices.len().saturating_sub(candidate_pages.len()),
            RASTER_PREPASS_DPI,
            RASTER_HIGHPASS_DPI,
            cfg.raster_dpi,
            prepass_ms,
            highpass_pages,
            highpass_ms,
            highpass_fallback_pages
        ));
        out
    }

    fn attach_underlying_text(
        retriever: &dyn RedactionDataRetriever,
        page_index: u32,
        occs: &mut [RedactionOccurrence],
        diagnostics: &mut Vec<String>,
    ) {
        let page_redactions = occs
            .iter_mut()
            .filter(|occurrence| occurrence.page_index == page_index)
            .collect::<Vec<_>>();

        if page_redactions.is_empty() {
            return;
        }

        let hits = match retriever.underlying_text_hits(page_index) {
            Ok(v) => v,
            Err(m) => {
                diagnostics.push(format!("page_index={page_index} underlying_text_error={m}"));
                return;
            }
        };

        if hits.is_empty() {
            return;
        }

        for redaction in page_redactions {
            redaction.underlying_text = collect_context_hits_for_redaction(&hits, &redaction.bbox);
        }
    }

    fn collect_context_hits_for_redaction(
        hits: &[UnderlyingTextHit],
        red_bbox: &Rect,
    ) -> Vec<UnderlyingTextHit> {
        let band = Rect::new(
            red_bbox.x0,
            red_bbox.y0 - Y_BAND_PADDING_PT,
            red_bbox.x1,
            red_bbox.y1 + Y_BAND_PADDING_PT,
        );

        let mut by_line: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
        let red_center_y = (red_bbox.y0 + red_bbox.y1) * 0.5;
        for (idx, hit) in hits.iter().enumerate() {
            let hit_center_y = (hit.bbox.y0 + hit.bbox.y1) * 0.5;
            let close_in_y = (hit_center_y - red_center_y).abs() <= LINE_SEARCH_WINDOW_PT;
            if vertical_overlap(&hit.bbox, &band) <= 0.0 && !close_in_y {
                continue;
            }
            by_line.entry(line_bucket(&hit.bbox)).or_default().push(idx);
        }

        let mut best_line: Option<LineMatch> = None;

        for mut line in by_line.into_values() {
            line.sort_by(|a, b| {
                let left = &hits[*a].bbox;
                let right = &hits[*b].bbox;
                left.x0
                    .partial_cmp(&right.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        left.x1
                            .partial_cmp(&right.x1)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });

            let before_anchor = line
                .iter()
                .enumerate()
                .filter(|(_, idx)| {
                    let bbox = &hits[**idx].bbox;
                    bbox.x0 < red_bbox.x0
                })
                .map(|(pos, _)| pos)
                .next_back()
                .filter(|pos| {
                    let idx = line[*pos];
                    let gap = (red_bbox.x0 - hits[idx].bbox.x1).max(0.0);
                    gap <= MAX_CONTEXT_GAP_PT || horizontal_overlap(&hits[idx].bbox, red_bbox) > 0.0
                })
                .or_else(|| {
                    line.iter()
                        .enumerate()
                        .filter(|(_, idx)| hits[**idx].bbox.x0 < red_bbox.x0)
                        .map(|(pos, _)| pos)
                        .next_back()
                });

            let after_anchor = line
                .iter()
                .enumerate()
                .filter(|(_, idx)| {
                    let bbox = &hits[**idx].bbox;
                    bbox.x1 > red_bbox.x1
                })
                .map(|(pos, _)| pos)
                .next()
                .filter(|pos| {
                    let idx = line[*pos];
                    let gap = (hits[idx].bbox.x0 - red_bbox.x1).max(0.0);
                    gap <= MAX_CONTEXT_GAP_PT || horizontal_overlap(&hits[idx].bbox, red_bbox) > 0.0
                })
                .or_else(|| {
                    line.iter()
                        .enumerate()
                        .filter(|(_, idx)| hits[**idx].bbox.x1 > red_bbox.x1)
                        .map(|(pos, _)| pos)
                        .next()
                });

            if before_anchor.is_none() && after_anchor.is_none() {
                continue;
            }

            let overlap_pt = line
                .iter()
                .map(|idx| horizontal_overlap(&hits[*idx].bbox, red_bbox))
                .sum::<f32>();
            if overlap_pt > LARGE_OVERLAP_PT && before_anchor.is_none() {
                continue;
            }

            let line_center_y = {
                let sum = line
                    .iter()
                    .map(|idx| (hits[*idx].bbox.y0 + hits[*idx].bbox.y1) * 0.5)
                    .sum::<f32>();
                sum / line.len() as f32
            };

            let mut context_rank = match (before_anchor.is_some(), after_anchor.is_some()) {
                (true, true) => 0_i32,
                (true, false) => 1_i32,
                (false, true) => 2_i32,
                (false, false) => 3_i32,
            };
            if overlap_pt > LARGE_OVERLAP_PT && before_anchor.is_some() && after_anchor.is_some() {
                context_rank += 2_i32;
            }
            let y_rank = ((line_center_y - red_center_y).abs() * 100.0).round() as i32;
            let before_gap_rank = if let Some(pos) = before_anchor {
                let idx = line[pos];
                ((red_bbox.x0 - hits[idx].bbox.x1).max(0.0) * 100.0).round() as i32
            } else {
                100_000_i32
            };
            let after_gap_rank = if let Some(pos) = after_anchor {
                let idx = line[pos];
                ((hits[idx].bbox.x0 - red_bbox.x1).max(0.0) * 100.0).round() as i32
            } else {
                100_000_i32
            };

            let score = (context_rank, y_rank, before_gap_rank, after_gap_rank);
            match &best_line {
                None => best_line = Some((line, before_anchor, after_anchor, score)),
                Some((_, _, _, best_score)) if score < *best_score => {
                    best_line = Some((line, before_anchor, after_anchor, score));
                }
                _ => {}
            }
        }

        let Some((line, before_anchor, after_anchor, _)) = best_line else {
            let page_index = hits.first().map(|h| h.page_index).unwrap_or_default();
            return vec![
                UnderlyingTextHit {
                    page_index,
                    bbox: *red_bbox,
                    text: String::new(),
                },
                UnderlyingTextHit {
                    page_index,
                    bbox: *red_bbox,
                    text: String::new(),
                },
            ];
        };

        let before_phrase = before_anchor
            .map(|pos| grow_phrase_left(&line, pos, hits))
            .unwrap_or_default();
        let after_phrase = after_anchor
            .map(|pos| grow_phrase_right(&line, pos, hits))
            .unwrap_or_default();

        let page_index = line
            .first()
            .map(|idx| hits[*idx].page_index)
            .unwrap_or_default();

        vec![
            build_phrase_hit(page_index, &before_phrase, hits, red_bbox),
            build_phrase_hit(page_index, &after_phrase, hits, red_bbox),
        ]
    }

    fn grow_phrase_left(
        line: &[usize],
        anchor_pos: usize,
        hits: &[UnderlyingTextHit],
    ) -> Vec<usize> {
        let mut start = anchor_pos;
        while start > 0 {
            let prev = line[start - 1];
            let cur = line[start];
            if word_gap(&hits[prev].bbox, &hits[cur].bbox) > WORD_JOIN_GAP_PT {
                break;
            }
            start -= 1;
        }
        let mut phrase = line[start..=anchor_pos].to_vec();
        if phrase.len() > MAX_CONTEXT_WORDS_PER_SIDE {
            phrase = phrase[phrase.len() - MAX_CONTEXT_WORDS_PER_SIDE..].to_vec();
        }
        phrase
    }

    fn grow_phrase_right(
        line: &[usize],
        anchor_pos: usize,
        hits: &[UnderlyingTextHit],
    ) -> Vec<usize> {
        let mut end = anchor_pos;
        while end + 1 < line.len() {
            let cur = line[end];
            let next = line[end + 1];
            if word_gap(&hits[cur].bbox, &hits[next].bbox) > WORD_JOIN_GAP_PT {
                break;
            }
            end += 1;
        }
        let mut phrase = line[anchor_pos..=end].to_vec();
        if phrase.len() > MAX_CONTEXT_WORDS_PER_SIDE {
            phrase.truncate(MAX_CONTEXT_WORDS_PER_SIDE);
        }
        phrase
    }

    fn word_gap(left: &Rect, right: &Rect) -> f32 {
        (right.x0 - left.x1).max(0.0)
    }

    fn horizontal_overlap(a: &Rect, b: &Rect) -> f32 {
        (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0)
    }

    fn line_bucket(rect: &Rect) -> i32 {
        let center = (rect.y0 + rect.y1) * 0.5;
        (center / LINE_BUCKET_PT).round() as i32
    }

    fn vertical_overlap(a: &Rect, b: &Rect) -> f32 {
        (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0)
    }

    fn build_phrase_hit(
        page_index: u32,
        phrase_indices: &[usize],
        hits: &[UnderlyingTextHit],
        red_bbox: &Rect,
    ) -> UnderlyingTextHit {
        if phrase_indices.is_empty() {
            return UnderlyingTextHit {
                page_index,
                bbox: *red_bbox,
                text: String::new(),
            };
        }

        let mut x0 = f32::INFINITY;
        let mut y0 = f32::INFINITY;
        let mut x1 = f32::NEG_INFINITY;
        let mut y1 = f32::NEG_INFINITY;
        let mut words = Vec::new();

        for idx in phrase_indices {
            let hit = &hits[*idx];
            x0 = x0.min(hit.bbox.x0);
            y0 = y0.min(hit.bbox.y0);
            x1 = x1.max(hit.bbox.x1);
            y1 = y1.max(hit.bbox.y1);

            let trimmed = hit.text.trim();
            if !trimmed.is_empty() {
                words.push(trimmed.to_owned());
            }
        }

        UnderlyingTextHit {
            page_index,
            bbox: Rect::new(x0, y0, x1, y1),
            text: words.join(" "),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{
            build_report_from_input_name, run_redaction_scan, run_redaction_scan_from_bytes,
        };
        use crate::data::redactions_data::RedactionDataRetriever;
        use crate::types::redaction_types::{
            PdfRenderer, Rect, RedactionFinderConfig, RedactionFinderOutput, RedactionKind,
            RedactionMode, RedactionOccurrence, RenderedPage, UnderlyingTextHit,
        };
        use std::cell::RefCell;
        use std::collections::BTreeMap;

        #[derive(Clone)]
        struct FakeRenderer {
            page_count: usize,
            page: RenderedPage,
        }

        impl PdfRenderer for FakeRenderer {
            fn page_count(&self) -> usize {
                self.page_count
            }

            fn render_page_to_rgba(
                &self,
                page_index: usize,
                _target_dpi: f32,
            ) -> Result<RenderedPage, String> {
                if page_index >= self.page_count {
                    return Err(format!(
                        "page_out_of_bounds:index={} page_count={}",
                        page_index, self.page_count
                    ));
                }
                Ok(self.page.clone())
            }
        }

        struct FakeRasterRetriever {
            pages: Vec<u32>,
            prepass: BTreeMap<u32, Vec<RedactionOccurrence>>,
            highpass: BTreeMap<u32, Vec<RedactionOccurrence>>,
            calls: RefCell<Vec<(u32, f32)>>,
        }

        impl FakeRasterRetriever {
            fn new(
                pages: Vec<u32>,
                prepass: BTreeMap<u32, Vec<RedactionOccurrence>>,
                highpass: BTreeMap<u32, Vec<RedactionOccurrence>>,
            ) -> Self {
                Self {
                    pages,
                    prepass,
                    highpass,
                    calls: RefCell::new(Vec::new()),
                }
            }
        }

        impl RedactionDataRetriever for FakeRasterRetriever {
            fn page_indices(&self) -> Vec<u32> {
                self.pages.clone()
            }

            fn annotation_redactions(
                &self,
                _page_index: u32,
                _include_details: bool,
            ) -> Result<Vec<RedactionOccurrence>, String> {
                Ok(Vec::new())
            }

            fn drawn_redactions(
                &self,
                _page_index: u32,
                _include_details: bool,
                _include_full_page_rects: bool,
            ) -> Result<Vec<RedactionOccurrence>, String> {
                Ok(Vec::new())
            }

            fn raster_redactions(
                &self,
                page_index: u32,
                cfg: &RedactionFinderConfig,
            ) -> Result<Vec<RedactionOccurrence>, String> {
                self.calls.borrow_mut().push((page_index, cfg.raster_dpi));
                if (cfg.raster_dpi - 18.0_f32).abs() < f32::EPSILON {
                    return Ok(self.prepass.get(&page_index).cloned().unwrap_or_default());
                }
                if (cfg.raster_dpi - 96.0_f32).abs() < f32::EPSILON {
                    return Ok(self.highpass.get(&page_index).cloned().unwrap_or_default());
                }
                Err(format!("unexpected_dpi={}", cfg.raster_dpi))
            }

            fn underlying_text_hits(
                &self,
                _page_index: u32,
            ) -> Result<Vec<UnderlyingTextHit>, String> {
                Ok(Vec::new())
            }
        }

        fn occ(page_index: u32, x0: f32, y0: f32, x1: f32, y1: f32) -> RedactionOccurrence {
            RedactionOccurrence {
                page_index,
                bbox: Rect::new(x0, y0, x1, y1),
                kind: RedactionKind::RasterDarkRegion,
                score: 1.0_f32,
                meta: BTreeMap::new(),
                underlying_text: Vec::new(),
            }
        }

        #[test]
        fn run_redaction_scan_uses_two_pass_raster_strategy() {
            let mut prepass = BTreeMap::<u32, Vec<RedactionOccurrence>>::new();
            prepass.insert(0, vec![occ(0, 10.0_f32, 10.0_f32, 30.0_f32, 20.0_f32)]);
            prepass.insert(2, vec![occ(2, 50.0_f32, 10.0_f32, 80.0_f32, 20.0_f32)]);
            let mut highpass = BTreeMap::<u32, Vec<RedactionOccurrence>>::new();
            highpass.insert(0, vec![occ(0, 12.0_f32, 10.0_f32, 32.0_f32, 20.0_f32)]);

            let retriever = FakeRasterRetriever::new(vec![0, 1, 2], prepass, highpass);
            let cfg = RedactionFinderConfig {
                include_details: false,
                mode: RedactionMode::All,
                include_full_page_rects: false,
                enable_image_analysis: true,
                raster_dpi: 200.0_f32,
            };

            let out = run_redaction_scan(&retriever, cfg);
            let calls = retriever
                .calls
                .borrow()
                .iter()
                .map(|(page_index, dpi)| format!("{page_index}:{dpi:.1}"))
                .collect::<Vec<_>>();
            assert_eq!(
                calls,
                vec![
                    "0:18.0".to_owned(),
                    "1:18.0".to_owned(),
                    "2:18.0".to_owned(),
                    "0:96.0".to_owned(),
                    "2:96.0".to_owned(),
                ]
            );
            assert_eq!(out.redactions.len(), 2);
            assert!(out
                .diagnostics
                .iter()
                .any(|line| line.starts_with("raster_two_pass=")));
        }

        #[test]
        fn run_redaction_scan_from_bytes_is_deterministic_for_same_input() {
            let input = std::path::Path::new("test_data/EFTA02238592.pdf");
            let bytes = std::fs::read(input)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", input.display()));
            let cfg = RedactionFinderConfig {
                include_details: false,
                mode: RedactionMode::All,
                include_full_page_rects: false,
                enable_image_analysis: false,
                raster_dpi: 96.0_f32,
            };

            let output_a =
                run_redaction_scan_from_bytes(&bytes, None, cfg).expect("scan should succeed");
            let output_b =
                run_redaction_scan_from_bytes(&bytes, None, cfg).expect("scan should succeed");
            assert_eq!(output_a, output_b);
        }

        #[test]
        fn build_report_from_input_name_sorts_and_counts_pages() {
            let output = RedactionFinderOutput {
                redactions: vec![
                    occ(1, 30.0_f32, 200.0_f32, 50.0_f32, 210.0_f32),
                    occ(0, 40.0_f32, 150.0_f32, 60.0_f32, 160.0_f32),
                    occ(0, 10.0_f32, 140.0_f32, 20.0_f32, 150.0_f32),
                ],
                diagnostics: vec!["d1".to_owned()],
            };

            let report = build_report_from_input_name("memory://sample.pdf", output);
            assert_eq!(report.count, 3_u32);
            assert_eq!(report.page_counts.get(&0).copied(), Some(2_u32));
            assert_eq!(report.page_counts.get(&1).copied(), Some(1_u32));
            assert_eq!(report.redactions[0].page_index, 0_u32);
            assert!((report.redactions[0].bbox.x0 - 10.0_f32).abs() < 0.01_f32);
        }

        #[test]
        fn run_redaction_scan_from_bytes_with_renderer_supports_image_analysis() {
            let input = std::path::Path::new("test_data/EFTA02238592.pdf");
            let bytes = std::fs::read(input)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", input.display()));
            let page_count = lopdf::Document::load_mem(&bytes)
                .expect("pdf should parse")
                .get_pages()
                .len();
            let mut pixels = vec![240_u8; 64_usize * 64_usize * 4_usize];
            for px in pixels.chunks_exact_mut(4_usize) {
                px[3] = 255_u8;
            }
            for y in 24_usize..40_usize {
                for x in 8_usize..56_usize {
                    let idx = (y * 64_usize + x) * 4_usize;
                    pixels[idx] = 5_u8;
                    pixels[idx + 1] = 5_u8;
                    pixels[idx + 2] = 5_u8;
                }
            }
            let renderer = FakeRenderer {
                page_count,
                page: RenderedPage {
                    width_px: 64_u32,
                    height_px: 64_u32,
                    dpi: 96.0_f32,
                    pixels,
                },
            };
            let cfg = RedactionFinderConfig {
                include_details: false,
                mode: RedactionMode::All,
                include_full_page_rects: false,
                enable_image_analysis: true,
                raster_dpi: 96.0_f32,
            };

            let output = run_redaction_scan_from_bytes(&bytes, Some(&renderer), cfg)
                .expect("scan should succeed");
            assert!(output
                .redactions
                .iter()
                .any(|value| matches!(value.kind, RedactionKind::RasterDarkRegion)));
        }
    }
}

pub use guess_impl::{run_from_bytes as run_guess_from_bytes, RunGuessFromBytesRequest};

pub use redaction_impl::{build_report_from_input_name, run_redaction_scan_from_bytes};

mod visual_guess_score_impl {
    use std::collections::{BTreeMap, BTreeSet};

    use lopdf::{Document, Object, ObjectId};

    use crate::data::visualization_data::{VisualizationData, VisualizationInputs};
    use crate::dependency::hayro_renderer::HayroRenderer;
    use crate::dependency::pdf_annotator::PdfAnnotator;
    use crate::logic::time::Instant;
    use crate::types::file_types::FontRunReport;
    use crate::types::guess_types::{GuessReport, RedactionGuess};
    use crate::types::redaction_types::{PdfRenderer as _, Rect, RedactionReport, RenderedPage};
    use crate::types::text_overlay::TextOverlay;

    const BACKGROUND_LUMA_THRESHOLD: u8 = 245_u8;
    const CHANGED_LUMA_DELTA: u8 = 24_u8;
    const WINDOW_PADDING_PT: f32 = 1.0_f32;
    const OVERLAY_TEXT_COLOR: [f32; 3] = [0.0_f32, 0.0_f32, 0.0_f32];
    const OVERLAY_BORDER_WIDTH: f32 = 1.0_f32;
    const CONTEXT_ALIGNMENT_MAX_DIFF: f32 = 0.22_f32;
    const MAX_VISUAL_SCORE_DPI: f32 = 72.0_f32;
    const VISUAL_TILE_PAGE_PADDING_PT: f32 = 8.0_f32;
    const VISUAL_TILE_MAX_COVERAGE_FOR_CROP: f32 = 0.92_f32;
    const VISUAL_RERANK_TOP_K: usize = 3;
    const VISUAL_RERANK_MAX_EVAL_CANDIDATES: usize = 8;
    const VISUAL_RERANK_BLEND_WEIGHT: f32 = 0.92_f32;
    const VISUAL_RERANK_MAX_BASE_GAP: f32 = 0.10_f32;
    const VISUAL_RERANK_MAX_TOP_SCORE: f32 = 0.90_f32;
    const VISUAL_RERANK_MAX_GEOMETRIC_GAP_FOR_EVAL: f32 = 0.12_f32;
    const VISUAL_RERANK_MAX_SCORE_GAP_FOR_EXPANSION: f32 = 0.18_f32;
    const VISUAL_RERANK_MAX_WIDTH_DELTA_RATIO_FOR_EXPANSION: f32 = 0.08_f32;
    const VISUAL_RERANK_MIN_SHIFT_PX: f32 = 0.5_f32;
    const VISUAL_RERANK_MIN_GAIN_TO_REORDER: f32 = 0.02_f32;
    const EDGE_BAND_PT: f32 = 1.5_f32;
    const EDGE_BAND_WEIGHT: f32 = 1.8_f32;
    const EDGE_INTERIOR_WEIGHT: f32 = 2.2_f32;
    const REDACTION_INTERIOR_IGNORE_DARK_LUMA: u8 = 8_u8;
    const EDGE_INK_LUMA_THRESHOLD: u8 = 104_u8;
    const EDGE_INK_MATCH_BASE_LUMA_THRESHOLD: u8 = 176_u8;
    const EDGE_INK_MISMATCH_BASE_LUMA_THRESHOLD: u8 = 236_u8;

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct VisualGuessScoreConfig {
        pub enabled: bool,
        pub dpi: f32,
        pub min_ink_pixels: u32,
        pub drop_threshold: Option<f32>,
    }

    impl Default for VisualGuessScoreConfig {
        #[inline]
        fn default() -> Self {
            Self {
                enabled: true,
                dpi: 200.0_f32,
                min_ink_pixels: 64_u32,
                drop_threshold: None,
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct RowPixelScore {
        compared_pixels: u32,
        mean_abs_diff: f32,
        changed_pixel_ratio: f32,
        edge_ink_overlap_ratio: f32,
        edge_ink_mismatch_ratio: f32,
    }

    #[derive(Debug, Clone)]
    struct CandidateVisualScore {
        text: String,
        score: RowPixelScore,
        blended_score: f32,
        combined_gain: f32,
    }

    #[inline]
    pub fn apply_visual_scores_from_bytes(
        pdf_bytes: &[u8],
        redactions: &RedactionReport,
        font_runs: &FontRunReport,
        guesses: &mut [RedactionGuess],
        cfg: VisualGuessScoreConfig,
    ) -> Result<Vec<String>, String> {
        if !cfg.enabled {
            return Ok(vec!["visual_score=disabled".to_owned()]);
        }
        if !cfg.dpi.is_finite() || cfg.dpi <= 0.0_f32 {
            return Err(format!("visual_score_invalid_dpi:{}", cfg.dpi));
        }
        if cfg.min_ink_pixels == 0 {
            return Err("visual_score_min_ink_pixels_must_be_positive".to_owned());
        }
        if let Some(threshold) = cfg.drop_threshold {
            if !threshold.is_finite() || threshold < 0.0_f32 {
                return Err(format!("visual_score_invalid_drop_threshold:{threshold}"));
            }
        }

        let max_items = redactions.redactions.len().min(guesses.len());
        if max_items == 0 {
            return Ok(vec!["visual_score=skipped_empty_input".to_owned()]);
        }

        let visualization = VisualizationData::new();
        let guess_report = GuessReport {
            input_redactions: String::new(),
            input_fonts: String::new(),
            guesses: guesses.to_vec(),
            diagnostics: Vec::new(),
        };
        let inputs = visualization.load_inputs_from_bytes(
            pdf_bytes,
            redactions,
            Some(&guess_report),
            Some(font_runs),
        )?;
        apply_visual_scores_with_inputs(inputs, redactions, font_runs, guesses, cfg, max_items)
    }

    fn apply_visual_scores_with_inputs(
        inputs: VisualizationInputs,
        redactions: &RedactionReport,
        _font_runs: &FontRunReport,
        guesses: &mut [RedactionGuess],
        cfg: VisualGuessScoreConfig,
        max_items: usize,
    ) -> Result<Vec<String>, String> {
        let effective_dpi = cfg.dpi.min(MAX_VISUAL_SCORE_DPI);
        let dpi_ratio = if cfg.dpi <= 0.0_f32 {
            1.0_f32
        } else {
            (effective_dpi / cfg.dpi).clamp(0.1_f32, 1.0_f32)
        };
        let effective_min_ink_pixels =
            ((cfg.min_ink_pixels as f32 * dpi_ratio * dpi_ratio).round() as u32).max(8_u32);
        let overlays_by_redaction = group_overlays_by_redaction(&inputs.overlays);
        let page_boxes = build_page_boxes(&inputs.pdf_bytes)?;
        let rerank_enabled = cfg.enabled;

        let mut diagnostics = Vec::<String>::new();
        if overlays_by_redaction.is_empty() {
            for guess in guesses.iter_mut().take(max_items) {
                guess.visual_compared_pixels = None;
                guess.visual_mean_abs_diff = None;
                guess.visual_changed_pixel_ratio = None;
                guess.visual_reason = Some("no_overlay_for_top_guess".to_owned());
                guess.visual_dropped = false;
            }
            diagnostics
                .push("visual_score=scored_rows=0 dropped_rows=0 reason=no_overlays".to_owned());
            return Ok(diagnostics);
        }

        let annotator = PdfAnnotator;
        let context_overlays_by_redaction =
            build_context_overlays_by_redaction(&overlays_by_redaction);
        let (annotated_bytes, annotate_overlay_ms) = {
            let annotate_overlay_started = Instant::now();
            let bytes = annotator.annotate(
                &inputs.pdf_bytes,
                &[],
                &inputs.overlays,
                OVERLAY_TEXT_COLOR,
                OVERLAY_TEXT_COLOR,
                OVERLAY_BORDER_WIDTH,
            )?;
            (bytes, annotate_overlay_started.elapsed().as_millis())
        };
        let annotate_context_ms = 0_u128;
        let page_crop_boxes = build_visual_page_crop_boxes(&overlays_by_redaction, &page_boxes);
        let (base_pdf_bytes_for_visual, overlay_pdf_bytes_for_visual, crop_apply_ms) =
            if page_crop_boxes.is_empty() {
                (inputs.pdf_bytes.clone(), annotated_bytes.clone(), 0_u128)
            } else {
                let crop_started = Instant::now();
                let base_cropped = apply_page_crop_boxes(&inputs.pdf_bytes, &page_crop_boxes)?;
                let overlay_cropped = apply_page_crop_boxes(&annotated_bytes, &page_crop_boxes)?;
                (
                    base_cropped,
                    overlay_cropped,
                    crop_started.elapsed().as_millis(),
                )
            };
        let mut scoring_page_boxes = page_boxes.clone();
        for (page_index, crop_box) in &page_crop_boxes {
            scoring_page_boxes.insert(*page_index, *crop_box);
        }

        let (base_renderer, overlay_renderer, renderer_init_ms) = {
            let renderer_init_started = Instant::now();
            let base_renderer = HayroRenderer::new_from_bytes(&base_pdf_bytes_for_visual)?;
            let overlay_renderer = HayroRenderer::new_from_bytes(&overlay_pdf_bytes_for_visual)?;
            (
                base_renderer,
                overlay_renderer,
                renderer_init_started.elapsed().as_millis(),
            )
        };
        let mut pages_to_render = BTreeSet::<u32>::new();
        for overlays in overlays_by_redaction.values() {
            if let Some(first) = overlays.first() {
                pages_to_render.insert(first.page_index);
            }
        }
        let mut base_pages = BTreeMap::<u32, RenderedPage>::new();
        let mut overlay_pages = BTreeMap::<u32, RenderedPage>::new();
        let pages_render_started = Instant::now();
        for page_index in pages_to_render {
            let base = base_renderer.render_page_to_rgba(page_index as usize, effective_dpi)?;
            let overlay =
                overlay_renderer.render_page_to_rgba(page_index as usize, effective_dpi)?;
            base_pages.insert(page_index, base);
            overlay_pages.insert(page_index, overlay);
        }
        let page_render_ms = pages_render_started.elapsed().as_millis();

        let mut rows_with_top_guess = 0_usize;
        let mut context_rows_scored = 0_usize;
        let mut context_rows_rejected = 0_usize;
        let mut rows_scored = 0_usize;
        let mut rows_dropped = 0_usize;
        let mut rerank_rows_considered = 0_usize;
        let mut rerank_rows_scored = 0_usize;
        let mut rerank_top1_changed = 0_usize;
        let mut rerank_gain_sum = 0.0_f64;
        let mut rerank_candidate_evals = 0_usize;
        let mut rerank_eval_ms = 0_u128;
        let row_scoring_started = Instant::now();
        for (index, (guess, redaction)) in guesses
            .iter_mut()
            .zip(redactions.redactions.iter())
            .enumerate()
            .take(max_items)
        {
            guess.visual_compared_pixels = None;
            guess.visual_mean_abs_diff = None;
            guess.visual_changed_pixel_ratio = None;
            guess.visual_reason = None;
            guess.visual_dropped = false;

            if top_guess_text(guess).is_none() {
                guess.visual_reason = Some("no_top_guess".to_owned());
                continue;
            }
            rows_with_top_guess += 1;

            let Some(overlays) = overlays_by_redaction.get(&index) else {
                guess.visual_reason = Some("no_overlay_for_top_guess".to_owned());
                continue;
            };
            if overlays.is_empty() {
                guess.visual_reason = Some("overlay_group_empty".to_owned());
                continue;
            }

            let Some(page_box) = scoring_page_boxes.get(&redaction.page_index).copied() else {
                guess.visual_reason = Some("page_box_missing".to_owned());
                continue;
            };
            let Some(base_page) = base_pages.get(&redaction.page_index) else {
                guess.visual_reason = Some("base_page_missing".to_owned());
                continue;
            };
            let Some(overlay_page) = overlay_pages.get(&redaction.page_index) else {
                guess.visual_reason = Some("overlay_page_missing".to_owned());
                continue;
            };

            let Some(window_bbox) =
                union_overlay_bbox(overlays).map(|bbox| pad_rect(bbox, page_box))
            else {
                guess.visual_reason = Some("overlay_bbox_missing".to_owned());
                continue;
            };

            if let Some(context_overlays) = context_overlays_by_redaction.get(&index) {
                if let Some(context_window_bbox) =
                    union_overlay_bbox(context_overlays).map(|bbox| pad_rect(bbox, page_box))
                {
                    if let Some(context_score) = score_row_overlay(
                        base_page,
                        overlay_page,
                        page_box,
                        context_window_bbox,
                        redaction.bbox,
                        effective_min_ink_pixels,
                    ) {
                        context_rows_scored += 1;
                        if context_score.mean_abs_diff > CONTEXT_ALIGNMENT_MAX_DIFF {
                            context_rows_rejected += 1;
                            guess.visual_reason = Some("context_alignment_failed".to_owned());
                            continue;
                        }
                    }
                }
            }

            let top_before = top_guess_text(guess).map(|value| value.to_owned());
            let mut row_scored = false;
            if rerank_enabled && should_visual_rerank_row(guess, overlays) {
                rerank_rows_considered += 1;
                let rerank_eval_started = Instant::now();
                match score_top_k_candidates_for_row(
                    &annotator,
                    &base_pdf_bytes_for_visual,
                    redaction.page_index,
                    base_page,
                    page_box,
                    redaction.bbox,
                    overlays,
                    guess,
                    effective_dpi,
                    effective_min_ink_pixels,
                ) {
                    Ok(mut candidate_scores) if !candidate_scores.is_empty() => {
                        rerank_eval_ms += rerank_eval_started.elapsed().as_millis();
                        rerank_rows_scored += 1;
                        rerank_candidate_evals += candidate_scores.len();
                        candidate_scores.sort_by(|left, right| {
                            right
                                .blended_score
                                .partial_cmp(&left.blended_score)
                                .unwrap_or(std::cmp::Ordering::Equal)
                                .then_with(|| {
                                    left.score
                                        .mean_abs_diff
                                        .partial_cmp(&right.score.mean_abs_diff)
                                        .unwrap_or(std::cmp::Ordering::Equal)
                                })
                        });
                        let mut chosen = candidate_scores.first().cloned();
                        let Some(mut chosen) = chosen.take() else {
                            guess.visual_reason = Some("visual_rerank_empty".to_owned());
                            continue;
                        };
                        if let Some(before) = top_before.as_deref() {
                            if !before.eq_ignore_ascii_case(&chosen.text)
                                && chosen.combined_gain < VISUAL_RERANK_MIN_GAIN_TO_REORDER
                            {
                                if let Some(original) = candidate_scores
                                    .iter()
                                    .find(|candidate| candidate.text.eq_ignore_ascii_case(before))
                                    .cloned()
                                {
                                    chosen = original;
                                }
                            }
                        }
                        rows_scored += 1;
                        row_scored = true;
                        guess.visual_compared_pixels = Some(chosen.score.compared_pixels);
                        guess.visual_mean_abs_diff = Some(chosen.score.mean_abs_diff);
                        guess.visual_changed_pixel_ratio = Some(chosen.score.changed_pixel_ratio);

                        if let Some(before) = top_before.as_deref() {
                            if !before.eq_ignore_ascii_case(&chosen.text) {
                                rerank_top1_changed += 1;
                                promote_guess_text_to_front(guess, &chosen.text);
                            }
                        }
                        rerank_gain_sum += chosen.combined_gain.max(0.0_f32) as f64;

                        if let Some(threshold) = cfg.drop_threshold {
                            if chosen.score.mean_abs_diff > threshold {
                                guess.candidates.clear();
                                guess.exact_matches.clear();
                                guess.visual_dropped = true;
                                rows_dropped += 1;
                            }
                        }
                    }
                    Ok(_) => {
                        rerank_eval_ms += rerank_eval_started.elapsed().as_millis();
                    }
                    Err(error) => {
                        rerank_eval_ms += rerank_eval_started.elapsed().as_millis();
                        guess.visual_reason = Some(format!("visual_rerank_failed:{error}"));
                    }
                }
            }

            if row_scored {
                continue;
            }

            let score = score_row_overlay(
                base_page,
                overlay_page,
                page_box,
                window_bbox,
                redaction.bbox,
                effective_min_ink_pixels,
            );
            let Some(score) = score else {
                if guess.visual_reason.is_none() {
                    guess.visual_reason = Some("insufficient_ink_pixels".to_owned());
                }
                continue;
            };

            rows_scored += 1;
            guess.visual_compared_pixels = Some(score.compared_pixels);
            guess.visual_mean_abs_diff = Some(score.mean_abs_diff);
            guess.visual_changed_pixel_ratio = Some(score.changed_pixel_ratio);

            if let Some(threshold) = cfg.drop_threshold {
                if score.mean_abs_diff > threshold {
                    guess.candidates.clear();
                    guess.exact_matches.clear();
                    guess.visual_dropped = true;
                    rows_dropped += 1;
                }
            }
        }

        diagnostics.push(format!(
        "visual_score=enabled rows_total={} rows_with_top_guess={} context_rows_scored={} context_rows_rejected={} rows_scored={} rows_dropped={} dpi_requested={} dpi_effective={} min_ink_pixels_requested={} min_ink_pixels_effective={} drop_threshold={} context_max_diff={}",
        max_items,
        rows_with_top_guess,
        context_rows_scored,
        context_rows_rejected,
        rows_scored,
        rows_dropped,
        cfg.dpi,
        effective_dpi,
        cfg.min_ink_pixels,
        effective_min_ink_pixels,
        cfg.drop_threshold
            .map(|value| format!("{value:.4}"))
            .unwrap_or_else(|| "none".to_owned()),
        CONTEXT_ALIGNMENT_MAX_DIFF
    ));
        let rerank_changed_ratio = if rerank_rows_scored == 0 {
            0.0_f64
        } else {
            rerank_top1_changed as f64 / rerank_rows_scored as f64
        };
        let rerank_mean_gain = if rerank_rows_scored == 0 {
            0.0_f64
        } else {
            rerank_gain_sum / rerank_rows_scored as f64
        };
        diagnostics.push(format!(
            "visual_rerank=enabled={} rows_considered={} rows_scored={} top1_changed={} top1_changed_ratio={:.4} mean_gain={:.4} top_k={} eval_cap={} blend_weight={:.3}",
            rerank_enabled,
            rerank_rows_considered,
            rerank_rows_scored,
            rerank_top1_changed,
            rerank_changed_ratio,
            rerank_mean_gain,
            VISUAL_RERANK_TOP_K,
            VISUAL_RERANK_MAX_EVAL_CANDIDATES,
            VISUAL_RERANK_BLEND_WEIGHT
        ));
        let rerank_mean_eval_ms_per_candidate = if rerank_candidate_evals == 0 {
            0.0_f64
        } else {
            rerank_eval_ms as f64 / rerank_candidate_evals as f64
        };
        let rerank_mean_eval_ms_per_row = if rerank_rows_scored == 0 {
            0.0_f64
        } else {
            rerank_eval_ms as f64 / rerank_rows_scored as f64
        };
        diagnostics.push(format!(
            "visual_rerank_timing=candidate_evals={} eval_ms_total={} eval_ms_per_candidate={:.3} eval_ms_per_scored_row={:.3}",
            rerank_candidate_evals,
            rerank_eval_ms,
            rerank_mean_eval_ms_per_candidate,
            rerank_mean_eval_ms_per_row
        ));
        diagnostics.push(format!(
            "visual_score_timing=annotate_overlay_ms={} annotate_context_ms={} renderer_init_ms={} page_render_ms={} row_scoring_ms={} pages_rendered={}",
            annotate_overlay_ms,
            annotate_context_ms,
            renderer_init_ms,
            page_render_ms,
            row_scoring_started.elapsed().as_millis(),
            base_pages.len()
        ));
        diagnostics.push(format!(
            "visual_tile_rendering=pages_cropped={} crop_apply_ms={} tile_padding_pt={} max_crop_coverage={}",
            page_crop_boxes.len(),
            crop_apply_ms,
            VISUAL_TILE_PAGE_PADDING_PT,
            VISUAL_TILE_MAX_COVERAGE_FOR_CROP
        ));
        Ok(diagnostics)
    }

    fn group_overlays_by_redaction(overlays: &[TextOverlay]) -> BTreeMap<usize, Vec<TextOverlay>> {
        let mut by_index = BTreeMap::<usize, Vec<TextOverlay>>::new();
        for overlay in overlays {
            let Some(index) = overlay.redaction_index else {
                continue;
            };
            by_index.entry(index).or_default().push(overlay.clone());
        }
        by_index
    }

    fn build_context_overlays_by_redaction(
        overlays_by_redaction: &BTreeMap<usize, Vec<TextOverlay>>,
    ) -> BTreeMap<usize, Vec<TextOverlay>> {
        let mut out = BTreeMap::<usize, Vec<TextOverlay>>::new();
        for (index, overlays) in overlays_by_redaction {
            if overlays.len() < 3 {
                continue;
            }
            let Some(first) = overlays.first().cloned() else {
                continue;
            };
            let Some(last) = overlays.last().cloned() else {
                continue;
            };
            out.insert(*index, vec![first, last]);
        }
        out
    }

    fn top_guess_text(guess: &RedactionGuess) -> Option<&str> {
        if let Some(exact) = guess.exact_matches.first() {
            return Some(exact.as_str());
        }
        guess
            .candidates
            .first()
            .map(|candidate| candidate.text.as_str())
    }

    fn promote_guess_text_to_front(guess: &mut RedactionGuess, selected_text: &str) {
        if let Some(pos) = guess
            .candidates
            .iter()
            .position(|candidate| candidate.text.eq_ignore_ascii_case(selected_text))
        {
            let chosen = guess.candidates.remove(pos);
            guess.candidates.insert(0, chosen);
        }
        if let Some(pos) = guess
            .exact_matches
            .iter()
            .position(|text| text.eq_ignore_ascii_case(selected_text))
        {
            let chosen = guess.exact_matches.remove(pos);
            guess.exact_matches.insert(0, chosen);
        }
    }

    fn ordered_candidate_texts_top_k(guess: &RedactionGuess, top_k: usize) -> Vec<String> {
        let mut out = Vec::<String>::new();
        let mut seen = BTreeSet::<String>::new();
        for text in &guess.exact_matches {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = trimmed.to_ascii_uppercase();
            if seen.insert(key) {
                out.push(trimmed.to_owned());
            }
            if out.len() >= top_k {
                return out;
            }
        }
        for candidate in &guess.candidates {
            let trimmed = candidate.text.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = trimmed.to_ascii_uppercase();
            if seen.insert(key) {
                out.push(trimmed.to_owned());
            }
            if out.len() >= top_k {
                return out;
            }
        }
        out
    }

    fn rerank_candidate_texts(guess: &RedactionGuess) -> Vec<String> {
        let mut out = Vec::<String>::new();
        let mut seen = BTreeSet::<String>::new();

        for text in ordered_candidate_texts_top_k(guess, VISUAL_RERANK_TOP_K) {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = trimmed.to_ascii_uppercase();
            if seen.insert(key) {
                out.push(trimmed.to_owned());
            }
            if out.len() >= VISUAL_RERANK_MAX_EVAL_CANDIDATES {
                return out;
            }
        }
        let Some(top_text) = out.first().cloned() else {
            return out;
        };

        let top_geometric = geometric_score_for_text(guess, &top_text);
        let top_width = candidate_width_pt_for_text(guess, &top_text).max(0.1_f32);
        let mut expansion = Vec::<(f32, f32, String)>::new();

        for candidate in &guess.candidates {
            let trimmed = candidate.text.trim();
            if trimmed.is_empty() {
                continue;
            }
            let key = trimmed.to_ascii_uppercase();
            if seen.contains(&key) {
                continue;
            }
            let width = candidate
                .width_pt
                .filter(|value| value.is_finite() && *value > 0.0_f32)
                .unwrap_or_else(|| candidate_width_pt_for_text(guess, trimmed));
            let width_delta_ratio =
                ((width - top_width).abs() / top_width.max(1.0_f32)).max(0.0_f32);
            if width_delta_ratio > VISUAL_RERANK_MAX_WIDTH_DELTA_RATIO_FOR_EXPANSION {
                continue;
            }
            let score_gap = (top_geometric - candidate.score).max(0.0_f32);
            if score_gap > VISUAL_RERANK_MAX_SCORE_GAP_FOR_EXPANSION {
                continue;
            }
            expansion.push((width_delta_ratio, score_gap, trimmed.to_owned()));
        }
        expansion.sort_by(|left, right| {
            left.0
                .partial_cmp(&right.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    left.1
                        .partial_cmp(&right.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.2.cmp(&right.2))
        });
        for (_, _, text) in expansion {
            let key = text.to_ascii_uppercase();
            if seen.insert(key) {
                out.push(text);
            }
            if out.len() >= VISUAL_RERANK_MAX_EVAL_CANDIDATES {
                break;
            }
        }
        out
    }

    fn should_visual_rerank_row(guess: &RedactionGuess, overlays: &[TextOverlay]) -> bool {
        if overlays.len() < 3 {
            return false;
        }
        if !guess.exact_matches.is_empty() {
            return false;
        }
        let mut ordered = overlays.to_vec();
        ordered.sort_by(|left, right| {
            left.x
                .partial_cmp(&right.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let Some(left) = ordered.first() else {
            return false;
        };
        let Some(right) = ordered.last() else {
            return false;
        };
        if left.text.trim().is_empty() || right.text.trim().is_empty() {
            return false;
        }

        let texts = rerank_candidate_texts(guess);
        if texts.len() < 2 {
            return false;
        }
        let top = geometric_score_for_text(guess, &texts[0]);
        let second = geometric_score_for_text(guess, &texts[1]);
        if !top.is_finite() || !second.is_finite() {
            return false;
        }
        if top > VISUAL_RERANK_MAX_TOP_SCORE {
            return false;
        }
        (top - second).abs() <= VISUAL_RERANK_MAX_BASE_GAP
    }

    fn visual_quality_from_score(score: &RowPixelScore) -> f32 {
        let diff_quality = (1.0_f32 - score.mean_abs_diff / 0.30_f32).clamp(0.0_f32, 1.0_f32);
        let changed_quality =
            (1.0_f32 - score.changed_pixel_ratio / 0.45_f32).clamp(0.0_f32, 1.0_f32);
        let edge_overlap_quality = if score.edge_ink_overlap_ratio.is_finite() {
            score.edge_ink_overlap_ratio.clamp(0.0_f32, 1.0_f32)
        } else {
            0.5_f32
        };
        let edge_clean_quality = if score.edge_ink_mismatch_ratio.is_finite() {
            (1.0_f32 - score.edge_ink_mismatch_ratio).clamp(0.0_f32, 1.0_f32)
        } else {
            0.5_f32
        };
        (diff_quality * 0.45_f32
            + changed_quality * 0.20_f32
            + edge_overlap_quality * 0.25_f32
            + edge_clean_quality * 0.10_f32)
            .clamp(0.0_f32, 1.0_f32)
    }

    fn geometric_score_for_text(guess: &RedactionGuess, text: &str) -> f32 {
        if let Some(candidate) = guess
            .candidates
            .iter()
            .find(|candidate| candidate.text.eq_ignore_ascii_case(text))
        {
            return candidate.score;
        }
        if guess
            .exact_matches
            .iter()
            .any(|value| value.eq_ignore_ascii_case(text))
        {
            return 1.0_f32;
        }
        0.0_f32
    }

    fn candidate_width_pt_for_text(guess: &RedactionGuess, text: &str) -> f32 {
        if let Some(width) = guess
            .candidates
            .iter()
            .find(|candidate| candidate.text.eq_ignore_ascii_case(text))
            .and_then(|candidate| candidate.width_pt)
            .filter(|value| value.is_finite() && *value > 0.0_f32)
        {
            return width;
        }
        let char_width = guess.context.char_width_pt.max(0.1_f32);
        approximate_candidate_width_pt(text, char_width)
    }

    fn approximate_candidate_width_pt(text: &str, char_width_pt: f32) -> f32 {
        let glyph_count = text
            .chars()
            .filter(|ch| !ch.is_whitespace() && *ch != ',')
            .count()
            .max(1) as f32;
        let spaces = text.chars().filter(|ch| ch.is_whitespace()).count() as f32;
        (glyph_count * char_width_pt + spaces * char_width_pt * 0.45_f32).max(0.1_f32)
    }

    fn build_candidate_overlays_from_template(
        template_overlays: &[TextOverlay],
        candidate_text: &str,
        candidate_width_pt: f32,
        top_width_pt: f32,
    ) -> Option<Vec<TextOverlay>> {
        if template_overlays.len() < 3 {
            return None;
        }
        let mut ordered = template_overlays.to_vec();
        ordered.sort_by(|left, right| {
            left.x
                .partial_cmp(&right.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut left = ordered.first()?.clone();
        let mut guess = ordered.get(1)?.clone();
        let mut right = ordered.get(2)?.clone();
        let template_bbox = union_overlay_bbox(&ordered)?;

        let top_width = top_width_pt.max(0.1_f32);
        let right_space = (right.x - (guess.x + top_width)).max(0.0_f32);
        let new_right_x = guess.x + candidate_width_pt.max(0.1_f32) + right_space;

        guess.text = candidate_text.to_owned();
        right.x = new_right_x;

        left.bbox = template_bbox;
        guess.bbox = template_bbox;
        right.bbox = template_bbox;
        Some(vec![left, guess, right])
    }

    #[allow(clippy::too_many_arguments)]
    fn score_top_k_candidates_for_row(
        annotator: &PdfAnnotator,
        pdf_bytes: &[u8],
        page_index: u32,
        base_page: &RenderedPage,
        page_box: Rect,
        redaction_bbox: Rect,
        template_overlays: &[TextOverlay],
        guess: &RedactionGuess,
        dpi: f32,
        min_ink_pixels: u32,
    ) -> Result<Vec<CandidateVisualScore>, String> {
        let texts = rerank_candidate_texts(guess);
        if texts.len() < 2 {
            return Ok(Vec::new());
        }
        let Some(template_bbox) = union_overlay_bbox(template_overlays) else {
            return Ok(Vec::new());
        };
        let top_text = texts.first().cloned().unwrap_or_default();
        let top_width = candidate_width_pt_for_text(guess, &top_text);
        let max_positive_shift_pt = texts
            .iter()
            .map(|text| (candidate_width_pt_for_text(guess, text) - top_width).max(0.0_f32))
            .fold(0.0_f32, f32::max);
        let fixed_window_bbox = pad_rect(
            Rect::new(
                template_bbox.x0,
                template_bbox.y0,
                template_bbox.x1 + max_positive_shift_pt,
                template_bbox.y1,
            ),
            page_box,
        );
        let top_geometric = geometric_score_for_text(guess, &top_text);
        let min_shift_pt =
            ((72.0_f32 / dpi.max(1.0_f32)) * VISUAL_RERANK_MIN_SHIFT_PX).max(0.25_f32);
        let mut out = Vec::<CandidateVisualScore>::new();
        for text in texts {
            let candidate_width = candidate_width_pt_for_text(guess, &text);
            let geometric = geometric_score_for_text(guess, &text);
            if !text.eq_ignore_ascii_case(&top_text) {
                let geometric_gap = (top_geometric - geometric).max(0.0_f32);
                if geometric_gap > VISUAL_RERANK_MAX_GEOMETRIC_GAP_FOR_EVAL {
                    continue;
                }
                if (candidate_width - top_width).abs() < min_shift_pt {
                    continue;
                }
            }
            let Some(overlays) = build_candidate_overlays_from_template(
                template_overlays,
                &text,
                candidate_width,
                top_width,
            ) else {
                continue;
            };
            let annotated = annotator.annotate(
                pdf_bytes,
                &[],
                &overlays,
                OVERLAY_TEXT_COLOR,
                OVERLAY_TEXT_COLOR,
                OVERLAY_BORDER_WIDTH,
            )?;
            let renderer = HayroRenderer::new_from_bytes(&annotated)?;
            let overlaid_page = renderer.render_page_to_rgba(page_index as usize, dpi)?;
            let Some(score) = score_row_overlay(
                base_page,
                &overlaid_page,
                page_box,
                fixed_window_bbox,
                redaction_bbox,
                min_ink_pixels,
            ) else {
                continue;
            };
            let visual = visual_quality_from_score(&score);
            let blended = geometric * (1.0_f32 - VISUAL_RERANK_BLEND_WEIGHT)
                + visual * VISUAL_RERANK_BLEND_WEIGHT;
            out.push(CandidateVisualScore {
                text,
                score,
                blended_score: blended,
                combined_gain: 0.0_f32,
            });
        }
        if out.is_empty() {
            return Ok(out);
        }
        let top_blended = out
            .iter()
            .find(|candidate| candidate.text.eq_ignore_ascii_case(&top_text))
            .map(|candidate| candidate.blended_score)
            .unwrap_or_else(|| out[0].blended_score);
        for candidate in &mut out {
            candidate.combined_gain = candidate.blended_score - top_blended;
        }
        Ok(out)
    }

    fn union_overlay_bbox(overlays: &[TextOverlay]) -> Option<Rect> {
        let first = overlays.first()?;
        let mut x0 = first.bbox.x0;
        let mut y0 = first.bbox.y0;
        let mut x1 = first.bbox.x1;
        let mut y1 = first.bbox.y1;
        for overlay in overlays.iter().skip(1) {
            x0 = x0.min(overlay.bbox.x0);
            y0 = y0.min(overlay.bbox.y0);
            x1 = x1.max(overlay.bbox.x1);
            y1 = y1.max(overlay.bbox.y1);
        }
        Some(Rect::new(x0, y0, x1, y1))
    }

    fn pad_rect(rect: Rect, page_box: Rect) -> Rect {
        Rect::new(
            (rect.x0 - WINDOW_PADDING_PT).max(page_box.x0),
            (rect.y0 - WINDOW_PADDING_PT).max(page_box.y0),
            (rect.x1 + WINDOW_PADDING_PT).min(page_box.x1),
            (rect.y1 + WINDOW_PADDING_PT).min(page_box.y1),
        )
    }

    fn pad_rect_with_padding(rect: Rect, page_box: Rect, padding_pt: f32) -> Rect {
        Rect::new(
            (rect.x0 - padding_pt).max(page_box.x0),
            (rect.y0 - padding_pt).max(page_box.y0),
            (rect.x1 + padding_pt).min(page_box.x1),
            (rect.y1 + padding_pt).min(page_box.y1),
        )
    }

    fn merge_rect(left: Rect, right: Rect) -> Rect {
        Rect::new(
            left.x0.min(right.x0),
            left.y0.min(right.y0),
            left.x1.max(right.x1),
            left.y1.max(right.y1),
        )
    }

    fn build_visual_page_crop_boxes(
        overlays_by_redaction: &BTreeMap<usize, Vec<TextOverlay>>,
        page_boxes: &BTreeMap<u32, Rect>,
    ) -> BTreeMap<u32, Rect> {
        let mut out = BTreeMap::<u32, Rect>::new();
        for overlays in overlays_by_redaction.values() {
            let Some(first) = overlays.first() else {
                continue;
            };
            let page_index = first.page_index;
            let Some(page_box) = page_boxes.get(&page_index).copied() else {
                continue;
            };
            let Some(row_bbox) = union_overlay_bbox(overlays) else {
                continue;
            };
            let padded = pad_rect_with_padding(row_bbox, page_box, VISUAL_TILE_PAGE_PADDING_PT);
            out.entry(page_index)
                .and_modify(|existing| *existing = merge_rect(*existing, padded))
                .or_insert(padded);
        }
        for (page_index, crop_box) in out.iter_mut() {
            if let Some(page_box) = page_boxes.get(page_index).copied() {
                let coverage = crop_box.area() / page_box.area().max(0.0001_f32);
                if coverage >= VISUAL_TILE_MAX_COVERAGE_FOR_CROP {
                    *crop_box = page_box;
                }
            }
        }
        out
    }

    fn apply_page_crop_boxes(
        pdf_bytes: &[u8],
        page_crop_boxes: &BTreeMap<u32, Rect>,
    ) -> Result<Vec<u8>, String> {
        if page_crop_boxes.is_empty() {
            return Ok(pdf_bytes.to_vec());
        }
        let mut doc = Document::load_mem(pdf_bytes).map_err(|error| error.to_string())?;
        for (page_no, page_id) in doc.get_pages() {
            let page_index = page_no.saturating_sub(1);
            let Some(crop) = page_crop_boxes.get(&page_index).copied() else {
                continue;
            };
            let page_object = doc
                .get_object_mut(page_id)
                .map_err(|error| error.to_string())?;
            let page_dict = match page_object {
                Object::Dictionary(value) => value,
                _ => continue,
            };
            let box_array = Object::Array(vec![
                Object::Real(crop.x0),
                Object::Real(crop.y0),
                Object::Real(crop.x1),
                Object::Real(crop.y1),
            ]);
            page_dict.set("CropBox", box_array.clone());
            page_dict.set("MediaBox", box_array);
        }
        let mut out = Vec::<u8>::new();
        doc.save_to(&mut out).map_err(|error| error.to_string())?;
        Ok(out)
    }

    fn score_row_overlay(
        base: &RenderedPage,
        overlaid: &RenderedPage,
        page_box: Rect,
        window_bbox: Rect,
        redaction_bbox: Rect,
        min_ink_pixels: u32,
    ) -> Option<RowPixelScore> {
        if base.width_px != overlaid.width_px || base.height_px != overlaid.height_px {
            return None;
        }
        if base.pixels.len() != overlaid.pixels.len() || base.pixels.is_empty() {
            return None;
        }

        let window = rect_pdf_to_pixels(
            &window_bbox,
            page_box,
            base.dpi,
            base.width_px,
            base.height_px,
        )?;
        let redaction = rect_pdf_to_pixels(
            &redaction_bbox,
            page_box,
            base.dpi,
            base.width_px,
            base.height_px,
        );

        let width = base.width_px as usize;
        let mut compared_pixels = 0_u32;
        let mut changed_weight = 0.0_f32;
        let mut compared_weight = 0.0_f32;
        let mut diff_sum = 0.0_f32;
        let mut edge_ink_weight = 0.0_f32;
        let mut edge_ink_overlap_weight = 0.0_f32;
        let mut edge_ink_mismatch_weight = 0.0_f32;
        let edge_band_px = ((EDGE_BAND_PT / 72.0_f32) * base.dpi).ceil().max(0.0_f32) as u32;

        for y in window.1..window.3 {
            for x in window.0..window.2 {
                let mut inside_redaction_edge_band = false;
                let mut outside_redaction_edge_band = false;
                if let Some(red_box) = redaction {
                    if point_in_rect_px(x, y, red_box) {
                        if !point_in_inner_edge_band_px(x, y, red_box, edge_band_px) {
                            continue;
                        }
                        inside_redaction_edge_band = true;
                    } else if point_in_edge_band_px(x, y, red_box, edge_band_px) {
                        outside_redaction_edge_band = true;
                    }
                }
                let index = ((y as usize * width) + x as usize) * 4;
                if index + 2 >= base.pixels.len() {
                    continue;
                }
                let base_luma = luma_u8(&base.pixels[index..index + 4]);
                let over_luma = luma_u8(&overlaid.pixels[index..index + 4]);
                if base_luma >= BACKGROUND_LUMA_THRESHOLD && over_luma >= BACKGROUND_LUMA_THRESHOLD
                {
                    continue;
                }
                if inside_redaction_edge_band
                    && base_luma <= REDACTION_INTERIOR_IGNORE_DARK_LUMA
                    && over_luma <= REDACTION_INTERIOR_IGNORE_DARK_LUMA
                {
                    continue;
                }

                compared_pixels = compared_pixels.saturating_add(1);
                let edge_weight = if inside_redaction_edge_band {
                    EDGE_INTERIOR_WEIGHT
                } else if outside_redaction_edge_band {
                    EDGE_BAND_WEIGHT
                } else {
                    1.0_f32
                };
                if (inside_redaction_edge_band || outside_redaction_edge_band)
                    && over_luma <= EDGE_INK_LUMA_THRESHOLD
                {
                    edge_ink_weight += edge_weight;
                    if base_luma <= EDGE_INK_MATCH_BASE_LUMA_THRESHOLD {
                        edge_ink_overlap_weight += edge_weight;
                    } else if base_luma >= EDGE_INK_MISMATCH_BASE_LUMA_THRESHOLD {
                        edge_ink_mismatch_weight += edge_weight;
                    }
                }
                compared_weight += edge_weight;
                let delta = base_luma.abs_diff(over_luma);
                diff_sum += (delta as f32 / 255.0_f32) * edge_weight;
                if delta >= CHANGED_LUMA_DELTA {
                    changed_weight += edge_weight;
                }
            }
        }

        if compared_pixels < min_ink_pixels {
            return None;
        }
        let denom = compared_weight.max(0.0001_f32);
        let (edge_ink_overlap_ratio, edge_ink_mismatch_ratio) = if edge_ink_weight <= 0.0_f32 {
            (0.5_f32, 0.5_f32)
        } else {
            let edge_denom = edge_ink_weight.max(0.0001_f32);
            (
                edge_ink_overlap_weight / edge_denom,
                edge_ink_mismatch_weight / edge_denom,
            )
        };
        Some(RowPixelScore {
            compared_pixels,
            mean_abs_diff: diff_sum / denom,
            changed_pixel_ratio: changed_weight / denom,
            edge_ink_overlap_ratio,
            edge_ink_mismatch_ratio,
        })
    }

    fn luma_u8(rgba: &[u8]) -> u8 {
        if rgba.len() < 3 {
            return 255;
        }
        let r = rgba[0] as f32;
        let g = rgba[1] as f32;
        let b = rgba[2] as f32;
        (0.299_f32 * r + 0.587_f32 * g + 0.114_f32 * b)
            .round()
            .clamp(0.0_f32, 255.0_f32) as u8
    }

    fn point_in_rect_px(x: u32, y: u32, rect: (u32, u32, u32, u32)) -> bool {
        x >= rect.0 && x < rect.2 && y >= rect.1 && y < rect.3
    }

    fn point_in_edge_band_px(x: u32, y: u32, rect: (u32, u32, u32, u32), band_px: u32) -> bool {
        if band_px == 0 {
            return false;
        }
        let y_min = rect.1.saturating_sub(band_px);
        let y_max = rect.3.saturating_add(band_px);
        if y < y_min || y >= y_max {
            return false;
        }
        let left_min = rect.0.saturating_sub(band_px);
        let left_band = x >= left_min && x < rect.0;
        let right_max = rect.2.saturating_add(band_px);
        let right_band = x >= rect.2 && x < right_max;
        left_band || right_band
    }

    fn point_in_inner_edge_band_px(
        x: u32,
        y: u32,
        rect: (u32, u32, u32, u32),
        band_px: u32,
    ) -> bool {
        if band_px == 0 {
            return false;
        }
        if !point_in_rect_px(x, y, rect) {
            return false;
        }
        let near_left = x < rect.0.saturating_add(band_px);
        let near_right = x >= rect.2.saturating_sub(band_px);
        let near_top = y < rect.1.saturating_add(band_px);
        let near_bottom = y >= rect.3.saturating_sub(band_px);
        near_left || near_right || near_top || near_bottom
    }

    fn rect_pdf_to_pixels(
        rect: &Rect,
        page_box: Rect,
        dpi: f32,
        width_px: u32,
        height_px: u32,
    ) -> Option<(u32, u32, u32, u32)> {
        if dpi <= 0.0_f32 || width_px == 0 || height_px == 0 {
            return None;
        }

        let x0 = (((rect.x0 - page_box.x0) / 72.0_f32) * dpi).floor();
        let x1 = (((rect.x1 - page_box.x0) / 72.0_f32) * dpi).ceil();
        let y0 = (((page_box.y1 - rect.y1) / 72.0_f32) * dpi).floor();
        let y1 = (((page_box.y1 - rect.y0) / 72.0_f32) * dpi).ceil();

        let x0_px = x0.clamp(0.0_f32, width_px as f32) as u32;
        let x1_px = x1.clamp(0.0_f32, width_px as f32) as u32;
        let y0_px = y0.clamp(0.0_f32, height_px as f32) as u32;
        let y1_px = y1.clamp(0.0_f32, height_px as f32) as u32;

        if x1_px <= x0_px || y1_px <= y0_px {
            return None;
        }
        Some((x0_px, y0_px, x1_px, y1_px))
    }

    fn build_page_boxes(pdf_bytes: &[u8]) -> Result<BTreeMap<u32, Rect>, String> {
        let doc = Document::load_mem(pdf_bytes).map_err(|error| error.to_string())?;
        let mut boxes = BTreeMap::<u32, Rect>::new();
        for (page_no, page_id) in doc.get_pages() {
            let page_index = page_no.saturating_sub(1);
            let page_box = page_render_box_from_page(&doc, page_id)
                .unwrap_or(Rect::new(0.0_f32, 0.0_f32, 612.0_f32, 792.0_f32));
            boxes.insert(page_index, page_box);
        }
        Ok(boxes)
    }

    fn page_render_box_from_page(doc: &Document, page_id: ObjectId) -> Option<Rect> {
        inherited_page_rect(doc, page_id, b"CropBox")
            .or_else(|| inherited_page_rect(doc, page_id, b"MediaBox"))
    }

    fn inherited_page_rect(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<Rect> {
        let mut current_id = page_id;
        let mut depth = 0_usize;
        loop {
            if depth > 32 {
                return None;
            }
            depth += 1;
            let object = doc.get_object(current_id).ok()?;
            let dict = match object {
                Object::Dictionary(value) => value,
                _ => return None,
            };

            if let Ok(value) = dict.get(key) {
                if let Some(rect) = object_to_rect_resolved(doc, value) {
                    return Some(rect);
                }
            }

            let parent = match dict.get(b"Parent").ok()? {
                Object::Reference(parent_id) => *parent_id,
                _ => return None,
            };
            current_id = parent;
        }
    }

    fn object_to_rect_resolved(doc: &Document, object: &Object) -> Option<Rect> {
        match object {
            Object::Reference(object_id) => {
                doc.get_object(*object_id).ok().and_then(object_to_rect)
            }
            _ => object_to_rect(object),
        }
    }

    fn object_to_rect(object: &Object) -> Option<Rect> {
        let values = match object {
            Object::Array(items) => items,
            _ => return None,
        };
        if values.len() < 4 {
            return None;
        }
        let x0 = object_to_f32(values.first()?)?;
        let y0 = object_to_f32(values.get(1)?)?;
        let x1 = object_to_f32(values.get(2)?)?;
        let y1 = object_to_f32(values.get(3)?)?;
        Some(Rect::new(x0, y0, x1, y1))
    }

    fn object_to_f32(object: &Object) -> Option<f32> {
        match object {
            Object::Integer(value) => Some(*value as f32),
            Object::Real(value) => Some(*value),
            _ => None,
        }
    }

    #[cfg(test)]
    mod tests {
        use crate::logic::redaction_guessing_component::{
            build_report_from_input_name, run_guess_from_bytes, run_redaction_scan_from_bytes,
            RunGuessFromBytesRequest,
        };
        use crate::types::guess_types::GuessConfig;
        use crate::types::redaction_types::{RedactionFinderConfig, RedactionMode};

        #[test]
        fn visual_scoring_path_executes_through_public_api() {
            let input = std::path::Path::new("test_data/EFTA00101126.pdf");
            let pdf_bytes = std::fs::read(input)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", input.display()));

            let scan_cfg = RedactionFinderConfig {
                include_details: false,
                mode: RedactionMode::All,
                include_full_page_rects: false,
                enable_image_analysis: false,
                raster_dpi: 96.0_f32,
            };
            let scan = run_redaction_scan_from_bytes(&pdf_bytes, None, scan_cfg)
                .expect("redaction scan should succeed");
            let redactions = build_report_from_input_name("memory://EFTA00101126.pdf", scan);

            let dictionary = vec![
                "SARAH KELLEN".to_owned(),
                "GHISLANE MAXWELL".to_owned(),
                "ALPHA".to_owned(),
            ];
            let cfg = GuessConfig {
                visual_score: true,
                visual_score_dpi: 200.0_f32,
            };
            let report = run_guess_from_bytes(RunGuessFromBytesRequest {
                pdf_name: "EFTA00101126.pdf",
                pdf_bytes: &pdf_bytes,
                redactions: &redactions,
                dictionary: &dictionary,
                diagnostics: &[],
                cfg: &cfg,
            })
            .expect("guessing should succeed");

            assert!(
                !report
                    .diagnostics
                    .iter()
                    .any(|line| line == "visual_score=disabled"),
                "visual scoring path should not emit disabled marker"
            );
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|line| line.starts_with("visual_score=")),
                "expected visual score diagnostics in public-path execution"
            );
        }
    }
}
