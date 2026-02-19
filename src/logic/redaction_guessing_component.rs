use std::time::Instant;

use crate::data::dictionary_data::DictionaryData;
use crate::data::fonts_data::FontsData;
use crate::data::redactions_data::RedactionsData;
use crate::data::visualization_data::VisualizationData;
use crate::logic::types::{BytesPipelineOutputs, BytesPipelineRequest};
use crate::types::redaction_types::{RedactionFinderConfig, RedactionMode};

#[inline]
pub fn run_redaction_guessing_component(
    req: BytesPipelineRequest,
) -> Result<BytesPipelineOutputs, String> {
    let component_started = Instant::now();
    let redactions_data = RedactionsData::new();
    let fonts_data = FontsData::new();
    let dictionary_data = DictionaryData::new();
    let visualization_data = VisualizationData::new();

    let redactions_started = Instant::now();
    let redaction_cfg = RedactionFinderConfig {
        include_details: req.cfg.include_details,
        mode: RedactionMode::All,
        include_full_page_rects: req.cfg.include_full_page_rects,
        enable_image_analysis: req.cfg.enable_image_analysis,
        raster_dpi: req.cfg.raster_dpi,
    };
    let output = if req.cfg.enable_image_analysis {
        let renderer = redactions_data.build_renderer(&req.pdf_bytes)?;
        run_redaction_scan_from_bytes(&req.pdf_bytes, Some(&renderer), redaction_cfg)?
    } else {
        run_redaction_scan_from_bytes(&req.pdf_bytes, None, redaction_cfg)?
    };
    let redactions = build_report_from_input_name(&req.input_name, output);
    let redactions_ms = redactions_started.elapsed().as_millis();

    let fonts_started = Instant::now();
    let fonts = fonts_data.detect_fonts_from_bytes(
        &req.input_name,
        &req.pdf_bytes,
        req.cfg.include_details,
    )?;
    let fonts_ms = fonts_started.elapsed().as_millis();

    let guess_started = Instant::now();
    let dictionary_inputs = dictionary_data.load_dictionary_from_bytes(
        req.dictionary_bytes.as_deref(),
        req.cfg.guess.max_dictionary,
    )?;
    let mut guess_report = run_guess_from_bytes(RunGuessFromBytesRequest {
        pdf_name: &req.input_name,
        pdf_bytes: &req.pdf_bytes,
        redactions: &redactions,
        dictionary: &dictionary_inputs.dictionary,
        diagnostics: &dictionary_inputs.diagnostics,
        cfg: &req.cfg.guess,
    })?;
    let guess_ms = guess_started.elapsed().as_millis();
    guess_report
        .diagnostics
        .push(format!("timing_ms stage=redactions value={redactions_ms}"));
    guess_report
        .diagnostics
        .push(format!("timing_ms stage=fonts value={fonts_ms}"));
    guess_report
        .diagnostics
        .push(format!("timing_ms stage=guess value={guess_ms}"));

    let mut visualize_ms = 0_u128;
    let mut visualized_pdf_bytes = None::<Vec<u8>>;
    if req.cfg.visualize {
        let visualize_started = Instant::now();
        let font_runs = fonts_data
            .load_font_runs_from_bytes(&req.input_name, &req.pdf_bytes)?
            .report;
        visualized_pdf_bytes = Some(visualization_data.render_visualized_pdf_from_bytes(
            &req.pdf_bytes,
            &redactions,
            Some(&guess_report),
            Some(&font_runs),
            req.cfg.visualizer,
        )?);
        visualize_ms = visualize_started.elapsed().as_millis();
    }
    guess_report
        .diagnostics
        .push(format!("timing_ms stage=visualize value={visualize_ms}"));
    guess_report.diagnostics.push(format!(
        "timing_ms stage=orchestrator_total value={}",
        component_started.elapsed().as_millis()
    ));

    Ok(BytesPipelineOutputs {
        redactions,
        fonts,
        guesses: guess_report,
        visualized_pdf_bytes,
    })
}

mod guess_impl {
    use lopdf::{Dictionary, Document, Object};
    use std::path::Path;
    use std::sync::OnceLock;
    use std::time::Instant;

    use super::visual_guess_score_impl::{apply_visual_scores_from_bytes, VisualGuessScoreConfig};
    use crate::data::{DictionaryDataSource, FontRunDataSource, ReportDataSource};
    use crate::dependency::pdf_font_run_accessor::build_font_run_report;
    use crate::types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect as FontRect};
    use crate::types::guess_types::{
        GuessCandidate, GuessConfig, GuessContext, GuessReport, RedactionGuess,
    };
    use crate::types::redaction_types::{Rect, RedactionOccurrence, RedactionReport};

    pub struct RunGuessRequest<'a> {
        pub report_data: &'a dyn ReportDataSource,
        pub dictionary_data: &'a dyn DictionaryDataSource,
        pub font_run_data: &'a dyn FontRunDataSource,
        pub redactions_path: &'a Path,
        pub fonts_path: &'a Path,
        pub pdf_path: &'a Path,
        pub dictionary_path: Option<&'a Path>,
        pub cfg: &'a GuessConfig,
    }

    pub struct RunGuessFromBytesRequest<'a> {
        pub pdf_name: &'a str,
        pub pdf_bytes: &'a [u8],
        pub redactions: &'a RedactionReport,
        pub dictionary: &'a [String],
        pub diagnostics: &'a [String],
        pub cfg: &'a GuessConfig,
    }

    #[inline]
    pub fn run_from_paths(req: RunGuessRequest<'_>) -> Result<GuessReport, String> {
        let reports = req
            .report_data
            .load_reports(req.redactions_path, req.fonts_path)?;
        let dictionary = req
            .dictionary_data
            .load_dictionary(req.dictionary_path, req.cfg.max_dictionary)?;
        let font_runs = req.font_run_data.load_font_runs(req.pdf_path)?;
        let width_tables = build_pdf_width_table_map(req.pdf_path).unwrap_or_default();
        let pdf_bytes = std::fs::read(req.pdf_path).ok();
        let mut diagnostics = reports.diagnostics;
        diagnostics.extend(dictionary.diagnostics);
        let inputs = BuildReportWithFontsInputs {
            input_redactions_label: req.redactions_path.to_string_lossy().to_string(),
            input_fonts_label: req.fonts_path.to_string_lossy().to_string(),
            redactions: reports.redactions,
            dictionary: dictionary.dictionary,
            diagnostics,
            font_runs: font_runs.report,
            width_tables,
            pdf_bytes: pdf_bytes.as_deref(),
        };
        Ok(build_report_from_parts_with_fonts_inputs(inputs, req.cfg))
    }

    #[inline]
    pub fn run_from_bytes(req: RunGuessFromBytesRequest<'_>) -> Result<GuessReport, String> {
        let started = Instant::now();
        let font_runs_started = Instant::now();
        let font_runs = build_font_run_report(Path::new(req.pdf_name), req.pdf_bytes)?;
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
                min_ink_pixels: cfg.visual_min_ink_pixels,
                drop_threshold: cfg.visual_drop_threshold,
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
    const GLYPH_UNITS_SCALE: f64 = 64.0_f64;
    const MULTI_SPAN_GAP_RATIO_THRESHOLD: f64 = 2.0_f64;
    const MULTI_SPAN_ANCHOR_PRIOR_WEIGHT: f64 = 0.15_f64;
    const SINGLE_SPAN_BOX_PRIOR_WEIGHT: f64 = 0.12_f64;
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
    const JOINT_ASSIGNMENT_DUPLICATE_PENALTY: f64 = 8.0_f64;
    const JOINT_ASSIGNMENT_OVERLAP_MARGIN_PT: f64 = 0.75_f64;
    const JOINT_ASSIGNMENT_OVERLAP_PENALTY: f64 = 2.8_f64;
    const JOINT_ASSIGNMENT_MAX_GROUP_GAP_PT: f64 = 140.0_f64;
    const JOINT_ASSIGNMENT_NAME_SHAPE_PENALTY: f64 = 1.25_f64;
    const JOINT_ASSIGNMENT_NULL_DELTA: f64 = 0.75_f64;
    const JOINT_ASSIGNMENT_NULL_MIN_BEST_COST: f64 = 1.4_f64;

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
                        tol_pt: cfg.tol_pt as f32,
                        anchor_left_x: None,
                        anchor_right_x: None,
                        anchor_font_key: None,
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

        let prefer_name_shape = group.iter().copied().any(|guess_index| {
            let guess = &guesses[guess_index];
            is_multi_span_row_guess(guess)
                || is_list_like_context(
                    &guess.context.left_anchor_text,
                    &guess.context.right_anchor_text,
                )
        });

        let mut options_by_row = Vec::<Vec<JointAssignmentOption>>::with_capacity(group.len());
        let mut null_costs = Vec::<f64>::with_capacity(group.len());
        let mut allow_null_by_row = Vec::<bool>::with_capacity(group.len());
        for guess_index in group.iter().copied() {
            let guess = guesses.get(guess_index)?;
            let options = build_joint_assignment_options(
                guess,
                JOINT_ASSIGNMENT_OPTION_SCAN_LIMIT,
                JOINT_ASSIGNMENT_MAX_OPTIONS_PER_ROW,
                prefer_name_shape,
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
                    if state.used_keys.iter().any(|key| key == &option.key) {
                        cost += JOINT_ASSIGNMENT_DUPLICATE_PENALTY;
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
        prefer_name_shape: bool,
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
            let name_shape_penalty =
                if prefer_name_shape && !looks_like_multi_span_name_candidate(text) {
                    JOINT_ASSIGNMENT_NAME_SHAPE_PENALTY
                } else {
                    0.0_f64
                };
            let base_cost = (candidate.error_pt as f64)
                + context_penalty
                + width_penalty
                + rank_penalty
                + exact_bonus
                + anchor_overlap_penalty
                + name_shape_penalty;
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
            for guess_index in indices.iter().copied() {
                let guess = &mut guesses[guess_index];
                if guess.candidates.is_empty() {
                    continue;
                }
                let mut best: Option<(String, f64)> = None;
                let max_scan = guess.candidates.len().min(80);
                for (rank, candidate) in guess.candidates.iter().take(max_scan).enumerate() {
                    let key = normalize_candidate_key(&candidate.text);
                    let duplicate_penalty = if used.contains(&key) {
                        6.0_f64
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

    fn build_guess_for_anchor(
        redaction: &RedactionOccurrence,
        dictionary: &[String],
        cfg: &GuessConfig,
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
        if !cache.candidates.contains_key(&key) {
            let mut widths = std::collections::BTreeMap::new();
            for word in dictionary {
                let trimmed = word.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let width = measure_width(trimmed).unwrap_or_else(|| {
                    fallback_measured_width(
                        trimmed,
                        fallback_char_width,
                        fallback_space_width,
                        DEFAULT_METRICS_DPI,
                    )
                });
                widths.insert(trimmed.to_owned(), width);
            }
            let mut sorted = widths
                .iter()
                .map(|(text, measured)| CandidateWidthEntry {
                    text: text.clone(),
                    width_pt: measured.pt,
                    source: measured.source,
                })
                .collect::<Vec<_>>();
            sorted.sort_by(|left, right| {
                left.width_pt
                    .partial_cmp(&right.width_pt)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.text.cmp(&right.text))
            });
            cache.candidates.insert(key.clone(), widths);
            cache.sorted_by_width.insert(key.clone(), sorted);
        }

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
        let candidate_width_index = cache.sorted_by_width.get(&key);
        let Some(candidate_width_index) = candidate_width_index else {
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
        };

        let mut scored = Vec::new();
        let redaction_width_pt = (redaction.bbox.width().abs() as f64).max(1.0_f64);
        let anchor_gap_pt = (anchor.right_x - anchor.left_x).abs().max(1.0_f64);
        let gap_ratio = anchor_gap_pt / redaction_width_pt;
        let multi_span_mode = matches!(anchor.mode, AnchorMode::TwoSided)
            && gap_ratio >= MULTI_SPAN_GAP_RATIO_THRESHOLD;
        let (min_char_units, max_char_units) = char_unit_band(
            redaction_width_pt,
            fallback_char_width.max(0.1_f64),
            anchor.epsilon_pt.max(cfg.tol_pt),
        );
        let anchor_filter_limit_pt =
            (anchor.epsilon_pt.max(1.0_f64) * 4.0_f64).max(cfg.tol_pt.max(4.0_f64));
        let box_filter_limit_pt = (redaction_width_pt * MULTI_SPAN_BOX_ERROR_RATIO
            + MULTI_SPAN_BOX_ERROR_PAD_PT)
            .max(anchor.epsilon_pt.max(2.5_f64));
        let list_like_context =
            is_list_like_context(&anchor.left_anchor_text, &anchor.right_anchor_text);
        if multi_span_mode {
            let lower_width = (redaction_width_pt - box_filter_limit_pt).max(0.0_f64);
            let upper_width = redaction_width_pt + box_filter_limit_pt;
            let ranged =
                candidate_width_entries_in_range(candidate_width_index, lower_width, upper_width);
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
                if list_like_context && !looks_like_multi_span_name_candidate(trimmed) {
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
                let box_err = (entry.width_pt - redaction_width_pt).abs();
                if box_err > box_filter_limit_pt {
                    continue;
                }
                funnel.after_box += 1;
                let raw_err = box_err + (anchor_err * MULTI_SPAN_ANCHOR_PRIOR_WEIGHT);
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
            let single_span_width_slack_pt = (anchor.epsilon_pt.max(cfg.tol_pt) * 1.75_f64)
                .max(redaction_width_pt * 0.45_f64)
                .max(12.0_f64);
            let lower_width = (redaction_width_pt - single_span_width_slack_pt).max(0.0_f64);
            let upper_width = redaction_width_pt + single_span_width_slack_pt;
            let ranged =
                candidate_width_entries_in_range(candidate_width_index, lower_width, upper_width);
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
                if list_like_context && !looks_like_multi_span_name_candidate(trimmed) {
                    continue;
                }
                funnel.after_shape += 1;
                let box_err = (entry.width_pt - redaction_width_pt).abs();
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
                .then_with(|| {
                    let left_is_base = is_base_name(&left_candidate.text);
                    let right_is_base = is_base_name(&right_candidate.text);
                    right_is_base.cmp(&left_is_base)
                })
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
                .take(cfg.max_candidates)
                .cloned()
                .collect::<Vec<_>>()
        } else {
            exact_scored
                .iter()
                .take(cfg.max_candidates)
                .cloned()
                .collect::<Vec<_>>()
        };

        let denom = if exact_scored.is_empty() {
            cfg.tol_pt.max(0.0001)
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

    fn build_pdf_width_table_map(
        pdf_path: &Path,
    ) -> Result<std::collections::BTreeMap<WidthTableKey, WidthTable>, String> {
        let bytes = std::fs::read(pdf_path).map_err(|error| {
            format!(
                "failed to read pdf bytes from {}: {error}",
                pdf_path.display()
            )
        })?;
        build_pdf_width_table_map_from_bytes(&bytes)
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
        candidates:
            std::collections::BTreeMap<WidthKey, std::collections::BTreeMap<String, MeasuredWidth>>,
        sorted_by_width: std::collections::BTreeMap<WidthKey, Vec<CandidateWidthEntry>>,
    }

    impl WidthCache {
        fn new() -> Self {
            Self {
                candidates: std::collections::BTreeMap::new(),
                sorted_by_width: std::collections::BTreeMap::new(),
            }
        }
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
            || right_lower.starts_with(',')
            || right_lower.starts_with("and ")
    }

    fn looks_like_multi_span_name_candidate(candidate: &str) -> bool {
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
        if words.len() < 2 || words.len() > 4 {
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
            if word_count <= 1 {
                penalty += 0.85_f64;
            }
            if word_count >= 5 {
                penalty += 0.40_f64;
            }
            if candidate_trim.contains(',') {
                penalty += 0.55_f64;
            }
            if candidate_trim.contains('(') || candidate_trim.contains(')') {
                penalty += 0.45_f64;
            }
            if candidate_trim.contains('/') || candidate_trim.contains('&') {
                penalty += 0.35_f64;
            }
            if word_count == 2 && !candidate_trim.contains(',') {
                penalty -= 0.15_f64;
            }
        }

        if (right_lower.starts_with(',') || right_lower.starts_with("and "))
            && (candidate_trim.ends_with(',') || candidate_trim.ends_with(';'))
        {
            penalty += 0.35_f64;
        }

        if candidate_trim.chars().any(|ch| ch.is_ascii_digit()) {
            penalty += 0.35_f64;
        }

        penalty.max(0.0)
    }

    fn is_base_name(value: &str) -> bool {
        let set = base_name_set();
        set.contains(&value.to_lowercase())
    }

    fn base_name_set() -> &'static std::collections::BTreeSet<String> {
        static SET: OnceLock<std::collections::BTreeSet<String>> = OnceLock::new();
        SET.get_or_init(|| {
            let raw = include_str!("../../assets/names.txt");
            raw.lines()
                .map(|line| line.trim().to_lowercase())
                .filter(|line| !line.is_empty())
                .collect::<std::collections::BTreeSet<String>>()
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::collections::BTreeMap;

        fn synthetic_guess(
            x0: f32,
            x1: f32,
            y0: f32,
            y1: f32,
            candidates: &[(&str, f32)],
        ) -> RedactionGuess {
            RedactionGuess {
                page_index: 1,
                bbox: Rect::new(x0, y0, x1, y1),
                candidates: candidates
                    .iter()
                    .map(|(text, error)| GuessCandidate {
                        text: (*text).to_owned(),
                        score: 1.0_f32,
                        error_pt: *error,
                        word_count: text.split_whitespace().count() as u32,
                        width_pt: None,
                    })
                    .collect::<Vec<_>>(),
                exact_matches: candidates
                    .first()
                    .map(|(text, _)| vec![(*text).to_owned()])
                    .unwrap_or_default(),
                context: GuessContext {
                    left_anchor_text: "including".to_owned(),
                    right_anchor_text: "and".to_owned(),
                    gap_pt: 120.0_f32,
                    char_width_pt: 5.0_f32,
                    tol_pt: 4.0_f32,
                    anchor_left_x: Some(80.0_f32),
                    anchor_right_x: Some(200.0_f32),
                    anchor_font_key: Some("F1".to_owned()),
                    anchor_font_size_pt: Some(11.0_f32),
                    anchor_h_scale_pct: Some(100.0_f32),
                    anchor_row_bias_pt: Some(0.0_f32),
                    anchor_mode: Some("two_sided".to_owned()),
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
            }
        }

        fn run(page_index: u32, text: &str, x0: f32, y0: f32, x1: f32, y1: f32) -> FontTextRun {
            FontTextRun {
                page_index,
                text: text.to_owned(),
                bbox: FontRect::new(x0, y0, x1, y1),
                font_key: "F1".to_owned(),
                font_name: "Helvetica".to_owned(),
                font_size_pt: 11.0,
                h_scale_pct: 100.0,
                measured_width_pt: None,
                measured_width_px: None,
                measured_dpi: None,
                char_advances_pt: Vec::new(),
                char_advances_px: Vec::new(),
            }
        }

        #[test]
        fn punctuation_penalty_prefers_full_name_for_list_context() {
            let short = punctuation_context_penalty("those served included", ",", "MAXWELL");
            let full =
                punctuation_context_penalty("those served included", ",", "GHISLAINE MAXWELL");
            assert!(short > full, "expected short token to be penalized more");
        }

        #[test]
        fn select_anchor_pair_uses_tight_row_before_broad_fallback() {
            let redaction = RedactionOccurrence {
                page_index: 1,
                bbox: Rect::new(50.0, 100.0, 80.0, 112.0),
                kind: crate::types::redaction_types::RedactionKind::RasterDarkRegion,
                score: 1.0,
                meta: BTreeMap::new(),
                underlying_text: Vec::new(),
            };

            let runs = [
                run(1, "included", 10.0, 100.0, 45.0, 112.0),
                run(1, ",", 82.0, 100.0, 90.0, 112.0),
                run(1, "noise_left", 12.0, 118.0, 46.0, 130.0),
                run(1, "noise_right", 82.0, 118.0, 95.0, 130.0),
            ];
            let run_refs = runs.iter().collect::<Vec<_>>();
            let assets = BTreeMap::new();
            let width_tables = BTreeMap::new();

            let selected = select_anchor_pair(&redaction, &run_refs, &assets, &width_tables)
                .expect("expected an anchor pair");
            assert_eq!(selected.left_anchor_text, "included");
            assert_eq!(selected.right_anchor_text, ",");
            assert!((selected.left_bbox.y1 - 112.0).abs() < 0.01);
        }

        #[test]
        fn list_context_name_filter_accepts_expected_name_shapes() {
            assert!(looks_like_multi_span_name_candidate("SARAH KELLEN"));
            assert!(looks_like_multi_span_name_candidate("JEAN LUC BRUNEL"));
            assert!(looks_like_multi_span_name_candidate("ANNE-MARIE O'NEIL"));
            assert!(!looks_like_multi_span_name_candidate("MAXWELL"));
            assert!(!looks_like_multi_span_name_candidate("BARNETT, RICHARD"));
            assert!(!looks_like_multi_span_name_candidate("(pilot)"));
            assert!(!looks_like_multi_span_name_candidate("A/B TEST"));
            assert!(!looks_like_multi_span_name_candidate("TOKEN123"));
        }

        #[test]
        fn select_anchor_pair_recovers_left_only_when_right_neighbor_missing() {
            let redaction = RedactionOccurrence {
                page_index: 1,
                bbox: Rect::new(50.0, 100.0, 88.0, 112.0),
                kind: crate::types::redaction_types::RedactionKind::RasterDarkRegion,
                score: 1.0,
                meta: BTreeMap::new(),
                underlying_text: vec![crate::types::redaction_types::UnderlyingTextHit {
                    page_index: 1,
                    bbox: Rect::new(10.0, 100.0, 45.0, 112.0),
                    text: "those served included".to_owned(),
                }],
            };
            let runs = [run(1, "included", 10.0, 100.0, 45.0, 112.0)];
            let run_refs = runs.iter().collect::<Vec<_>>();
            let assets = BTreeMap::new();
            let width_tables = BTreeMap::new();

            let selected = select_anchor_pair(&redaction, &run_refs, &assets, &width_tables)
                .expect("expected one-sided anchor recovery");
            assert_eq!(selected.mode.as_str(), "left_only");
            assert_eq!(selected.left_anchor_text, "included");
            assert!(selected.right_x > selected.left_x);
        }

        #[test]
        fn single_span_list_rows_filter_non_name_candidates() {
            let redaction = RedactionOccurrence {
                page_index: 1,
                bbox: Rect::new(80.0, 200.0, 145.0, 212.0),
                kind: crate::types::redaction_types::RedactionKind::RasterDarkRegion,
                score: 1.0,
                meta: BTreeMap::new(),
                underlying_text: Vec::new(),
            };
            let anchor = AnchorPairData {
                left_anchor_text: "those served included".to_owned(),
                right_anchor_text: ",".to_owned(),
                left_x: 30.0_f64,
                right_x: 142.0_f64,
                font_key: "F1".to_owned(),
                font_name: "Helvetica".to_owned(),
                font_size_pt: 11.0_f32,
                h_scale_pct: 100.0_f32,
                left_bbox: FontRect::new(10.0, 198.0, 75.0, 212.0),
                right_bbox: FontRect::new(146.0, 198.0, 148.0, 212.0),
                epsilon_pt: 8.0_f64,
                row_bias_pt: 0.0_f64,
                mode: AnchorMode::TwoSided,
            };
            let dictionary = vec![
                "SARAH KELLEN".to_owned(),
                "MAXWELL".to_owned(),
                "A/B TEST".to_owned(),
                "TOKEN123".to_owned(),
                "WILLIAM HAMMOND".to_owned(),
            ];
            let cfg = GuessConfig {
                max_words: 4,
                max_candidates: 10,
                max_dictionary: 100,
                tol_pt: 100.0,
                max_nodes: 1_000,
                visual_score: false,
                visual_score_dpi: 200.0_f32,
                visual_min_ink_pixels: 64_u32,
                visual_drop_threshold: None,
            };
            let assets = BTreeMap::new();
            let width_tables = BTreeMap::new();
            let mut cache = WidthCache::new();

            let (guess, _funnel) = build_guess_for_anchor(
                &redaction,
                &dictionary,
                &cfg,
                &anchor,
                &assets,
                &width_tables,
                &mut cache,
            );
            let names_only = guess
                .candidates
                .iter()
                .map(|candidate| candidate.text.as_str())
                .all(looks_like_multi_span_name_candidate);
            assert!(
                names_only,
                "expected single-span list context to keep only name-like candidates, got {:?}",
                guess
                    .candidates
                    .iter()
                    .map(|candidate| candidate.text.clone())
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn width_band_range_and_trim_stay_near_target() {
            let entries = (0_i32..30_i32)
                .map(|idx| CandidateWidthEntry {
                    text: format!("C{idx}"),
                    width_pt: 10.0 + idx as f64,
                    source: WidthSource::CoreFont,
                })
                .collect::<Vec<_>>();
            let ranged = candidate_width_entries_in_range(&entries, 18.0_f64, 24.0_f64);
            assert_eq!(ranged.len(), 7);
            assert!(ranged
                .first()
                .is_some_and(|entry| entry.width_pt >= 18.0_f64));
            assert!(ranged
                .last()
                .is_some_and(|entry| entry.width_pt <= 24.0_f64));

            let trimmed = trim_width_band_around_target(&entries, 20.0_f64, 8_usize);
            assert_eq!(trimmed.len(), 8);
            assert!(trimmed
                .iter()
                .any(|entry| (entry.width_pt - 20.0_f64).abs() < 0.001_f64));
        }

        #[test]
        fn row_joint_assignment_promotes_unique_names_for_multi_span_groups() {
            let mut guesses = vec![
                synthetic_guess(
                    100.0_f32,
                    145.0_f32,
                    460.0_f32,
                    472.0_f32,
                    &[
                        ("SARAH KELLEN", 0.45_f32),
                        ("ADRIANA MUCINSKA", 0.85_f32),
                        ("NADIA MARCINKOVA", 1.10_f32),
                    ],
                ),
                synthetic_guess(
                    260.0_f32,
                    308.0_f32,
                    460.0_f32,
                    472.0_f32,
                    &[
                        ("SARAH KELLEN", 0.40_f32),
                        ("ADRIANA MUCINSKA", 0.55_f32),
                        ("NADIA MARCINKOVA", 0.95_f32),
                    ],
                ),
                synthetic_guess(
                    420.0_f32,
                    469.0_f32,
                    460.0_f32,
                    472.0_f32,
                    &[
                        ("SARAH KELLEN", 0.42_f32),
                        ("NADIA MARCINKOVA", 0.58_f32),
                        ("ADRIANA MUCINSKA", 0.90_f32),
                    ],
                ),
            ];

            let assigned = apply_row_joint_assignment(&mut guesses);
            assert_eq!(assigned.len(), 3);
            let top = guesses
                .iter()
                .map(|guess| {
                    guess
                        .candidates
                        .first()
                        .map(|candidate| candidate.text.clone())
                })
                .collect::<Vec<_>>();
            assert_eq!(top[0].as_deref(), Some("SARAH KELLEN"));
            assert_eq!(top[1].as_deref(), Some("ADRIANA MUCINSKA"));
            assert_eq!(top[2].as_deref(), Some("NADIA MARCINKOVA"));
        }

        #[test]
        fn row_joint_assignment_can_skip_weak_rows_with_null_option() {
            let mut guesses = vec![
                synthetic_guess(
                    100.0_f32,
                    145.0_f32,
                    460.0_f32,
                    472.0_f32,
                    &[("SARAH KELLEN", 0.35_f32), ("ADRIANA MUCINSKA", 0.95_f32)],
                ),
                synthetic_guess(
                    152.0_f32,
                    196.0_f32,
                    460.0_f32,
                    472.0_f32,
                    &[("SARAH KELLEN", 2.50_f32), ("ADRIANA MUCINSKA", 2.90_f32)],
                ),
            ];

            let assigned = apply_row_joint_assignment(&mut guesses);
            assert_eq!(assigned.len(), 1);
            assert_eq!(
                guesses[0]
                    .candidates
                    .first()
                    .map(|candidate| candidate.text.as_str()),
                Some("SARAH KELLEN")
            );
            assert_eq!(
                guesses[1]
                    .candidates
                    .first()
                    .map(|candidate| candidate.text.as_str()),
                Some("SARAH KELLEN")
            );
        }
    }
}

mod redaction_impl {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    use crate::data::redactions_data::{PdfFileRetriever, RedactionDataRetriever};
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
    type LineMatchScore = (i32, i32, i32, i32);
    type LineMatch = (Vec<usize>, Option<usize>, Option<usize>, LineMatchScore);

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
        let mut all: Vec<RedactionOccurrence> = Vec::new();
        let mut diagnostics: Vec<String> = Vec::new();

        for page_index in retriever.page_indices() {
            match cfg.mode {
                RedactionMode::Annotations | RedactionMode::All => {
                    match retriever.annotation_redactions(page_index, cfg.include_details) {
                        Ok(v) => all.extend(v),
                        Err(m) => diagnostics
                            .push(format!("page_index={page_index} annotation_error={m}")),
                    }
                }
                RedactionMode::Drawn => {}
            }

            match cfg.mode {
                RedactionMode::Drawn | RedactionMode::All => match retriever.drawn_redactions(
                    page_index,
                    cfg.include_details,
                    cfg.include_full_page_rects,
                ) {
                    Ok(v) => all.extend(v),
                    Err(m) => diagnostics.push(format!("page_index={page_index} drawn_error={m}")),
                },
                RedactionMode::Annotations => {}
            }

            if cfg.enable_image_analysis {
                match retriever.raster_redactions(page_index, &cfg) {
                    Ok(v) => all.extend(v),
                    Err(m) => {
                        diagnostics.push(format!("page_index={page_index} raster_page_error={m}"))
                    }
                }
            }

            attach_underlying_text(retriever, page_index, &cfg, &mut all, &mut diagnostics);
        }

        RedactionFinderOutput {
            redactions: dedup_occurrences(all),
            diagnostics,
        }
    }

    #[inline]
    pub fn build_report(input: &Path, output: RedactionFinderOutput) -> RedactionReport {
        build_report_from_input_name(&input.to_string_lossy(), output)
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

    fn attach_underlying_text(
        retriever: &dyn RedactionDataRetriever,
        page_index: u32,
        cfg: &RedactionFinderConfig,
        occs: &mut [RedactionOccurrence],
        diagnostics: &mut Vec<String>,
    ) {
        let mut ocr_diagnostics_seen = BTreeSet::<String>::new();
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
            let mut context = collect_context_hits_for_redaction(&hits, &redaction.bbox);
            if should_try_ocr(redaction, &context) {
                match retriever.ocr_context_hits(page_index, &redaction.bbox, cfg) {
                    Ok(ocr_hits) => merge_ocr_context_hits(&mut context, &ocr_hits),
                    Err(message) => {
                        if ocr_diagnostics_seen.insert(message.clone()) {
                            diagnostics.push(format!(
                                "page_index={page_index} ocr_context_error={message}"
                            ));
                        }
                    }
                }
            }
            redaction.underlying_text = context;
        }
    }

    fn should_try_ocr(redaction: &RedactionOccurrence, context: &[UnderlyingTextHit]) -> bool {
        if redaction.kind != crate::types::redaction_types::RedactionKind::RasterDarkRegion {
            return false;
        }
        let left = context
            .first()
            .map(|hit| hit.text.as_str())
            .unwrap_or_default();
        let right = context
            .get(1)
            .map(|hit| hit.text.as_str())
            .unwrap_or_default();
        is_weak_anchor_text(left) || is_weak_anchor_text(right)
    }

    fn is_weak_anchor_text(text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return true;
        }
        let alpha_count = trimmed
            .chars()
            .filter(|ch| ch.is_ascii_alphabetic())
            .count();
        alpha_count < 2
    }

    fn merge_ocr_context_hits(
        context: &mut Vec<UnderlyingTextHit>,
        ocr_hits: &[UnderlyingTextHit],
    ) {
        for (idx, ocr_hit) in ocr_hits.iter().enumerate() {
            if ocr_hit.text.trim().is_empty() {
                continue;
            }
            if idx < context.len() {
                if is_weak_anchor_text(&context[idx].text) {
                    context[idx] = ocr_hit.clone();
                }
            } else {
                context.push(ocr_hit.clone());
            }
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
        use super::*;

        fn hit(
            page_index: u32,
            x0: f32,
            y0: f32,
            x1: f32,
            y1: f32,
            text: &str,
        ) -> UnderlyingTextHit {
            UnderlyingTextHit {
                page_index,
                bbox: Rect::new(x0, y0, x1, y1),
                text: text.to_owned(),
            }
        }

        #[test]
        fn context_hits_use_left_and_right_neighbors() {
            let hits = vec![
                hit(0, 10.0, 90.0, 22.0, 100.0, "before1"),
                hit(0, 24.0, 90.0, 35.0, 100.0, "inside"),
                hit(0, 42.0, 90.0, 53.0, 100.0, "after1"),
                hit(0, 55.0, 90.0, 66.0, 100.0, "after2"),
            ];
            let red = Rect::new(24.0, 88.0, 40.0, 102.0);

            let context = collect_context_hits_for_redaction(&hits, &red);
            let words = context.iter().map(|h| h.text.as_str()).collect::<Vec<_>>();

            assert_eq!(words, vec!["before1", "after1 after2"]);
        }

        #[test]
        fn context_hits_support_multi_line_redactions() {
            let hits = vec![
                hit(0, 5.0, 90.0, 14.0, 100.0, "top_l"),
                hit(0, 42.0, 90.0, 50.0, 100.0, "top_r"),
                hit(0, 6.0, 74.0, 16.0, 84.0, "bot_l"),
                hit(0, 41.0, 74.0, 52.0, 84.0, "bot_r"),
            ];
            let red = Rect::new(20.0, 72.0, 38.0, 102.0);

            let context = collect_context_hits_for_redaction(&hits, &red);
            let words = context.iter().map(|h| h.text.as_str()).collect::<Vec<_>>();

            assert_eq!(words, vec!["bot_l", "bot_r"]);
        }

        #[test]
        fn context_hits_return_empty_after_when_missing() {
            let hits = vec![hit(0, 10.0, 90.0, 22.0, 100.0, "left_only")];
            let red = Rect::new(24.0, 88.0, 40.0, 102.0);
            let context = collect_context_hits_for_redaction(&hits, &red);
            let words = context.iter().map(|h| h.text.as_str()).collect::<Vec<_>>();
            assert_eq!(words, vec!["left_only", ""]);
        }

        #[test]
        fn context_hits_ignore_far_neighbors() {
            let hits = vec![
                hit(0, -100.0, 90.0, -80.0, 100.0, "left_far"),
                hit(0, 200.0, 90.0, 220.0, 100.0, "right_far"),
            ];
            let red = Rect::new(30.0, 88.0, 40.0, 102.0);
            let context = collect_context_hits_for_redaction(&hits, &red);
            let words = context.iter().map(|h| h.text.as_str()).collect::<Vec<_>>();
            assert_eq!(words, vec!["left_far", "right_far"]);
        }
    }
}

pub use guess_impl::{
    run_from_bytes as run_guess_from_bytes, run_from_paths as run_guess_from_paths,
    RunGuessFromBytesRequest, RunGuessRequest,
};

pub use redaction_impl::{
    build_report, build_report_from_input_name, run_redaction_scan, run_redaction_scan_from_bytes,
};

mod visual_guess_score_impl {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;
    use std::time::Instant;

    use lopdf::{Document, Object, ObjectId};

    use crate::data::visualization_data::{
        VisualizationData, VisualizationDataSource as _, VisualizationInputs,
    };
    use crate::dependency::hayro_renderer::HayroRenderer;
    use crate::dependency::pdf_annotator::PdfAnnotator;
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
    const ENABLE_VISUAL_RERANK: bool = false;
    const VISUAL_RERANK_TOP_K: usize = 3;
    const VISUAL_RERANK_BLEND_WEIGHT: f32 = 0.35_f32;
    const VISUAL_RERANK_MAX_BASE_GAP: f32 = 0.08_f32;
    const VISUAL_RERANK_MAX_TOP_SCORE: f32 = 0.80_f32;
    const VISUAL_RERANK_MIN_GAIN_TO_REORDER: f32 = 0.04_f32;
    const EDGE_BAND_PT: f32 = 1.5_f32;
    const EDGE_BAND_WEIGHT: f32 = 1.8_f32;

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
    }

    #[derive(Debug, Clone)]
    struct CandidateVisualScore {
        text: String,
        score: RowPixelScore,
        blended_score: f32,
        combined_gain: f32,
    }

    #[inline]
    #[allow(dead_code)]
    pub fn apply_visual_scores(
        pdf_path: &Path,
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
        let inputs = visualization.load_inputs(
            pdf_path,
            redactions,
            Some(&guess_report),
            Some(font_runs),
        )?;
        apply_visual_scores_with_inputs(inputs, redactions, font_runs, guesses, cfg, max_items)
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

        let (base_renderer, overlay_renderer, renderer_init_ms) = {
            let renderer_init_started = Instant::now();
            let base_renderer = HayroRenderer::new_from_bytes(&inputs.pdf_bytes)?;
            let overlay_renderer = HayroRenderer::new_from_bytes(&annotated_bytes)?;
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

            let Some(page_box) = page_boxes.get(&redaction.page_index).copied() else {
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
            if should_visual_rerank_row(guess, overlays) {
                rerank_rows_considered += 1;
                match score_top_k_candidates_for_row(
                    &annotator,
                    &inputs.pdf_bytes,
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
                        rerank_rows_scored += 1;
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
                    Ok(_) => {}
                    Err(error) => {
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
            "visual_rerank=rows_considered={} rows_scored={} top1_changed={} top1_changed_ratio={:.4} mean_gain={:.4} top_k={} blend_weight={:.3}",
            rerank_rows_considered,
            rerank_rows_scored,
            rerank_top1_changed,
            rerank_changed_ratio,
            rerank_mean_gain,
            VISUAL_RERANK_TOP_K,
            VISUAL_RERANK_BLEND_WEIGHT
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

    fn should_visual_rerank_row(guess: &RedactionGuess, overlays: &[TextOverlay]) -> bool {
        if !ENABLE_VISUAL_RERANK {
            return false;
        }
        if overlays.len() < 3 {
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

        let texts = ordered_candidate_texts_top_k(guess, VISUAL_RERANK_TOP_K);
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

    fn visual_quality_from_diff(mean_abs_diff: f32) -> f32 {
        (1.0_f32 - mean_abs_diff / 0.30_f32).clamp(0.0_f32, 1.0_f32)
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

        let top_width = top_width_pt.max(0.1_f32);
        let right_space = (right.x - (guess.x + top_width)).max(0.0_f32);
        let new_right_x = guess.x + candidate_width_pt.max(0.1_f32) + right_space;
        let delta = new_right_x - right.x;

        guess.text = candidate_text.to_owned();
        right.x = new_right_x;

        let mut bbox = left.bbox;
        bbox.x1 = (bbox.x1 + delta).max(bbox.x0 + 1.0_f32);
        left.bbox = bbox;
        guess.bbox = bbox;
        right.bbox = bbox;
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
        let texts = ordered_candidate_texts_top_k(guess, VISUAL_RERANK_TOP_K);
        if texts.len() < 2 {
            return Ok(Vec::new());
        }
        let top_text = texts.first().cloned().unwrap_or_default();
        let top_width = candidate_width_pt_for_text(guess, &top_text);
        let mut out = Vec::<CandidateVisualScore>::new();
        for text in texts {
            let candidate_width = candidate_width_pt_for_text(guess, &text);
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
            let Some(window_bbox) =
                union_overlay_bbox(&overlays).map(|bbox| pad_rect(bbox, page_box))
            else {
                continue;
            };
            let Some(score) = score_row_overlay(
                base_page,
                &overlaid_page,
                page_box,
                window_bbox,
                redaction_bbox,
                min_ink_pixels,
            ) else {
                continue;
            };
            let geometric = geometric_score_for_text(guess, &text);
            let visual = visual_quality_from_diff(score.mean_abs_diff);
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
        let edge_band_px = ((EDGE_BAND_PT / 72.0_f32) * base.dpi).ceil().max(0.0_f32) as u32;

        for y in window.1..window.3 {
            for x in window.0..window.2 {
                if let Some(red_box) = redaction {
                    if point_in_rect_px(x, y, red_box) {
                        continue;
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

                compared_pixels = compared_pixels.saturating_add(1);
                let edge_weight = if let Some(red_box) = redaction {
                    if point_in_edge_band_px(x, y, red_box, edge_band_px) {
                        EDGE_BAND_WEIGHT
                    } else {
                        1.0_f32
                    }
                } else {
                    1.0_f32
                };
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
        Some(RowPixelScore {
            compared_pixels,
            mean_abs_diff: diff_sum / denom,
            changed_pixel_ratio: changed_weight / denom,
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
        use super::*;

        #[test]
        fn score_row_overlay_detects_difference() {
            let base = RenderedPage {
                width_px: 4,
                height_px: 4,
                dpi: 200.0_f32,
                pixels: vec![255_u8; 4 * 4 * 4],
            };
            let mut overlaid = base.clone();
            for i in 0..4_usize {
                let idx = (4_usize + i) * 4_usize;
                overlaid.pixels[idx] = 0_u8;
                overlaid.pixels[idx + 1] = 0_u8;
                overlaid.pixels[idx + 2] = 0_u8;
            }

            let page_box = Rect::new(0.0_f32, 0.0_f32, 72.0_f32, 72.0_f32);
            let window = Rect::new(0.0_f32, 0.0_f32, 72.0_f32, 72.0_f32);
            let redaction = Rect::new(1000.0_f32, 1000.0_f32, 1001.0_f32, 1001.0_f32);
            let score = score_row_overlay(&base, &overlaid, page_box, window, redaction, 1_u32)
                .expect("score should be present");
            assert!(score.compared_pixels > 0);
            assert!(score.mean_abs_diff > 0.0_f32);
            assert!(score.changed_pixel_ratio > 0.0_f32);
        }
    }
}
