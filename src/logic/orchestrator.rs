use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::data::dictionary_data::DictionaryData;
use crate::data::fonts_data::FontRunDataSource as _;
use crate::data::fonts_data::FontsData;
use crate::data::guess_validation_data::GuessValidationData;
use crate::data::redactions_data::RedactionsData;
use crate::data::visualization_data::VisualizationData;
use crate::types::guess_types::GuessConfig;
use crate::types::redaction_types::RedactionFinderConfig;
use crate::types::visualizer_config::VisualizerConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestratorConfig {
    pub include_details: bool,
    pub include_full_page_rects: bool,
    pub enable_image_analysis: bool,
    pub raster_dpi: f32,
    pub guess: GuessConfig,
    pub visualize: bool,
    pub visualizer: VisualizerConfig,
}

impl Default for OrchestratorConfig {
    #[inline]
    fn default() -> Self {
        Self {
            include_details: false,
            include_full_page_rects: false,
            enable_image_analysis: true,
            raster_dpi: 200.0_f32,
            guess: GuessConfig::default(),
            visualize: false,
            visualizer: VisualizerConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestratorRequest {
    pub input: PathBuf,
    pub output_dir: PathBuf,
    pub dictionary_path: Option<PathBuf>,
    pub cfg: OrchestratorConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestratorOutputs {
    pub redactions_path: PathBuf,
    pub fonts_path: PathBuf,
    pub guesses_path: PathBuf,
    pub visualized_pdf_path: Option<PathBuf>,
}

#[inline]
pub fn run_orchestrator(req: OrchestratorRequest) -> Result<OrchestratorOutputs, String> {
    let orchestrator_started = Instant::now();
    let outputs = build_output_paths(&req.input, &req.output_dir)?;

    let redactions_data = RedactionsData::new();
    let fonts_data = FontsData::new();
    let guess_validation_data = GuessValidationData::new();
    let dictionary_data = DictionaryData::new();
    let visualization_data = VisualizationData::new();

    let redactions_started = Instant::now();
    let bytes = redactions_data.read_input_bytes(&req.input)?;
    let redaction_cfg = RedactionFinderConfig {
        include_details: req.cfg.include_details,
        mode: crate::types::redaction_types::RedactionMode::All,
        include_full_page_rects: req.cfg.include_full_page_rects,
        enable_image_analysis: req.cfg.enable_image_analysis,
        raster_dpi: req.cfg.raster_dpi,
    };
    let output = if req.cfg.enable_image_analysis {
        let renderer = redactions_data.build_renderer(&bytes)?;
        run_redaction_scan_from_bytes(&bytes, Some(&renderer), redaction_cfg)?
    } else {
        run_redaction_scan_from_bytes(&bytes, None, redaction_cfg)?
    };
    let redactions = build_report(&req.input, output);
    redactions_data.write_redactions(&outputs.redactions_path, &redactions)?;
    let redactions_ms = redactions_started.elapsed().as_millis();

    let fonts_started = Instant::now();
    let fonts = fonts_data.detect_fonts(&req.input, req.cfg.include_details)?;
    fonts_data.write_fonts(&outputs.fonts_path, &fonts)?;
    let fonts_ms = fonts_started.elapsed().as_millis();

    let guess_started = Instant::now();
    let mut guess_report = run_guess_from_paths(RunGuessRequest {
        report_data: &guess_validation_data,
        dictionary_data: &dictionary_data,
        font_run_data: &fonts_data,
        redactions_path: &outputs.redactions_path,
        fonts_path: &outputs.fonts_path,
        pdf_path: &req.input,
        dictionary_path: req.dictionary_path.as_deref(),
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
    if req.cfg.visualize {
        let visualize_started = Instant::now();
        let output_path = outputs
            .visualized_pdf_path
            .clone()
            .ok_or_else(|| "visualized pdf path missing".to_owned())?;
        let font_runs = fonts_data.load_font_runs(&req.input)?.report;
        visualization_data.render_and_write(
            &req.input,
            &redactions,
            Some(&guess_report),
            Some(&font_runs),
            &output_path,
            req.cfg.visualizer,
        )?;
        visualize_ms = visualize_started.elapsed().as_millis();
    }
    guess_report
        .diagnostics
        .push(format!("timing_ms stage=visualize value={visualize_ms}"));
    guess_report.diagnostics.push(format!(
        "timing_ms stage=orchestrator_total value={}",
        orchestrator_started.elapsed().as_millis()
    ));
    guess_validation_data.write_guesses(&outputs.guesses_path, &guess_report)?;

    Ok(outputs)
}

#[inline]
pub fn build_output_paths(input: &Path, output_dir: &Path) -> Result<OrchestratorOutputs, String> {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "input file has no stem".to_owned())?;
    let redactions_path = output_dir.join(format!("{stem}.redactions.json"));
    let fonts_path = output_dir.join(format!("{stem}.fonts.json"));
    let guesses_path = output_dir.join(format!("{stem}.guesses.json"));
    let visualized_pdf_path = Some(output_dir.join(format!("{stem}.visualized.pdf")));
    Ok(OrchestratorOutputs {
        redactions_path,
        fonts_path,
        guesses_path,
        visualized_pdf_path,
    })
}

mod guess_impl {
    use lopdf::{Dictionary, Document, Object};
    use std::path::Path;
    use std::sync::OnceLock;

    use crate::data::{DictionaryDataSource, FontRunDataSource, ReportDataSource};
    use crate::logic::visual_guess_score::{apply_visual_scores, VisualGuessScoreConfig};
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
        let mut diagnostics = reports.diagnostics;
        diagnostics.extend(dictionary.diagnostics);
        let inputs = BuildReportWithFontsInputs {
            pdf_path: req.pdf_path,
            redactions_path: req.redactions_path,
            fonts_path: req.fonts_path,
            redactions: reports.redactions,
            dictionary: dictionary.dictionary,
            diagnostics,
            font_runs: font_runs.report,
            width_tables,
        };
        Ok(build_report_from_parts_with_fonts_inputs(inputs, req.cfg))
    }

    struct BuildReportWithFontsInputs<'a> {
        pdf_path: &'a Path,
        redactions_path: &'a Path,
        fonts_path: &'a Path,
        redactions: RedactionReport,
        dictionary: Vec<String>,
        diagnostics: Vec<String>,
        font_runs: FontRunReport,
        width_tables: std::collections::BTreeMap<WidthTableKey, WidthTable>,
    }

    fn build_report_from_parts_with_fonts_inputs(
        inputs: BuildReportWithFontsInputs<'_>,
        cfg: &GuessConfig,
    ) -> GuessReport {
        let (mut guesses, guess_diagnostics) = build_anchor_validated_guesses(
            &inputs.redactions.redactions,
            &inputs.dictionary,
            &inputs.font_runs,
            &inputs.width_tables,
            cfg,
        );
        let mut all_diagnostics = inputs.diagnostics;
        all_diagnostics.extend(guess_diagnostics);
        if cfg.visual_score {
            let visual_cfg = VisualGuessScoreConfig {
                enabled: cfg.visual_score,
                dpi: cfg.visual_score_dpi,
                min_ink_pixels: cfg.visual_min_ink_pixels,
                drop_threshold: cfg.visual_drop_threshold,
            };
            match apply_visual_scores(
                inputs.pdf_path,
                &inputs.redactions,
                &inputs.font_runs,
                &mut guesses,
                visual_cfg,
            ) {
                Ok(visual_diagnostics) => all_diagnostics.extend(visual_diagnostics),
                Err(error) => all_diagnostics.push(format!("visual_score_failed:{error}")),
            }
        } else {
            all_diagnostics.push("visual_score=disabled".to_owned());
        }
        GuessReport {
            input_redactions: inputs.redactions_path.to_string_lossy().to_string(),
            input_fonts: inputs.fonts_path.to_string_lossy().to_string(),
            guesses,
            diagnostics: all_diagnostics,
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
    }

    #[derive(Debug, Clone)]
    struct ScoredDictionaryCandidate {
        text: String,
        raw_error_pt: f64,
        effective_error_pt: f64,
        word_count: u32,
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

    #[derive(Debug, Clone, Copy)]
    struct MeasuredWidth {
        pt: f64,
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

            let guess = build_guess_for_anchor(
                redaction,
                dictionary,
                cfg,
                &anchor,
                &assets,
                width_tables,
                &mut cache,
            );
            guesses.push(guess);
        }

        apply_cluster_consensus(&mut guesses);
        apply_row_sequence_consensus(&mut guesses);
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
                "redaction_index={index} page_index={} anchored_row=true exact_count={} candidate_count={} top_guess={} left_anchor=[{}] right_anchor=[{}] tol_pt={}",
                guess.page_index,
                guess.exact_matches.len(),
                guess.candidates.len(),
                top,
                guess.context.left_anchor_text,
                guess.context.right_anchor_text,
                guess.context.tol_pt
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

    fn apply_row_sequence_consensus(guesses: &mut [RedactionGuess]) {
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
        let glyph_count = text
            .chars()
            .filter(|ch| !ch.is_whitespace() && *ch != ',')
            .count()
            .max(1) as f64;
        let spaces = text.chars().filter(|ch| ch.is_whitespace()).count() as f64;
        let estimated = glyph_count * char_width + spaces * char_width * 0.45_f64;
        ((estimated - target).abs() / target.max(1.0_f64)).min(3.0_f64)
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
    ) -> RedactionGuess {
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
            cache.candidates.insert(key.clone(), widths);
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
        let candidate_widths = cache.candidates.get(&key);
        let Some(candidate_widths) = candidate_widths else {
            return RedactionGuess {
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
                    has_anchor_pair: true,
                },
                visual_compared_pixels: None,
                visual_mean_abs_diff: None,
                visual_changed_pixel_ratio: None,
                visual_reason: None,
                visual_dropped: false,
            };
        };

        let mut scored = Vec::new();
        let redaction_width_pt = (redaction.bbox.width().abs() as f64).max(1.0_f64);
        let anchor_gap_pt = (anchor.right_x - anchor.left_x).abs().max(1.0_f64);
        let gap_ratio = anchor_gap_pt / redaction_width_pt;
        let multi_span_mode = gap_ratio >= MULTI_SPAN_GAP_RATIO_THRESHOLD;
        let anchor_filter_limit_pt =
            (anchor.epsilon_pt.max(1.0_f64) * 4.0_f64).max(cfg.tol_pt.max(4.0_f64));
        let box_filter_limit_pt = (redaction_width_pt * MULTI_SPAN_BOX_ERROR_RATIO
            + MULTI_SPAN_BOX_ERROR_PAD_PT)
            .max(anchor.epsilon_pt.max(2.5_f64));
        for word in dictionary {
            let trimmed = word.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !passes_context_filter(&anchor.left_anchor_text, &anchor.right_anchor_text, trimmed)
            {
                continue;
            }
            let Some(measured_width) = candidate_widths.get(trimmed).copied() else {
                continue;
            };
            let predicted_right = anchor.left_x
                + left_width.pt
                + space_width.pt
                + measured_width.pt
                + space_width.pt
                + anchor.row_bias_pt;
            let anchor_err = (predicted_right - anchor.right_x).abs();
            let box_err = (measured_width.pt - redaction_width_pt).abs();
            if multi_span_mode {
                if box_err > box_filter_limit_pt {
                    continue;
                }
            } else if anchor_err > anchor_filter_limit_pt {
                continue;
            }
            let raw_err = if multi_span_mode {
                box_err + (anchor_err * MULTI_SPAN_ANCHOR_PRIOR_WEIGHT)
            } else {
                anchor_err + (box_err * SINGLE_SPAN_BOX_PRIOR_WEIGHT)
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
            });
        }
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
            })
            .collect::<Vec<_>>();

        let char_width = if !anchor.left_anchor_text.trim().is_empty() {
            let chars = anchor.left_anchor_text.trim().chars().count().max(1) as f64;
            (left_width.pt / chars).max(0.0)
        } else {
            fallback_char_width
        };

        RedactionGuess {
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
                has_anchor_pair: true,
            },
            visual_compared_pixels: None,
            visual_mean_abs_diff: None,
            visual_changed_pixel_ratio: None,
            visual_reason: None,
            visual_dropped: false,
        }
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
            return None;
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
            return None;
        }
        if right_x <= left_x {
            return None;
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
        })
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
        Some(measured_width_from_points(width_pt, metrics_dpi))
    }

    fn measure_text_width_from_sources(
        input: &TextMeasureInput<'_>,
        asset: Option<&FontAsset>,
        width_tables: &std::collections::BTreeMap<WidthTableKey, WidthTable>,
    ) -> Option<MeasuredWidth> {
        if input.text.is_empty() {
            return Some(measured_width_from_points(0.0_f64, input.metrics_dpi));
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
                    return Some(measured_width_from_points(width * scale, input.metrics_dpi));
                }
            }
        }
        width_from_core_font(input.font_name, input.text, input.font_size_pt).and_then(|width| {
            let scale = (input.h_scale_pct as f64 / 100.0_f64).max(0.01_f64);
            let width_pt = width * scale;
            (width_pt.is_finite() && width_pt > 0.0_f64)
                .then_some(measured_width_from_points(width_pt, input.metrics_dpi))
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
        let doc = Document::load_mem(&bytes).map_err(|error| error.to_string())?;
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

    fn measured_width_from_points(width_pt: f64, _dpi: f32) -> MeasuredWidth {
        MeasuredWidth { pt: width_pt }
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
        measured_width_from_points(width_pt, dpi)
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct WidthKey {
        page_index: u32,
        font_key: String,
        font_size_bits: u32,
        h_scale_bits: u32,
        metrics_dpi_bits: u32,
    }

    struct WidthCache {
        candidates:
            std::collections::BTreeMap<WidthKey, std::collections::BTreeMap<String, MeasuredWidth>>,
    }

    impl WidthCache {
        fn new() -> Self {
            Self {
                candidates: std::collections::BTreeMap::new(),
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
        let list_context = left_lower.contains("including")
            || left_lower.contains("included")
            || left_lower.contains("among")
            || left_lower.contains("served")
            || right_lower.starts_with(',')
            || right_lower.starts_with("and ");

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
            input: input.to_string_lossy().to_string(),
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
        line[start..=anchor_pos].to_vec()
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
        line[anchor_pos..=end].to_vec()
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

pub use guess_impl::{run_from_paths as run_guess_from_paths, RunGuessRequest};

pub use redaction_impl::{build_report, run_redaction_scan, run_redaction_scan_from_bytes};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_output_paths_uses_stem_and_dir() {
        let input = Path::new("C:/data/report.pdf");
        let out = build_output_paths(input, Path::new("C:/out")).expect("expected value in test");
        assert_eq!(
            out.redactions_path,
            PathBuf::from("C:/out/report.redactions.json")
        );
        assert_eq!(out.fonts_path, PathBuf::from("C:/out/report.fonts.json"));
        assert_eq!(
            out.guesses_path,
            PathBuf::from("C:/out/report.guesses.json")
        );
        assert_eq!(
            out.visualized_pdf_path,
            Some(PathBuf::from("C:/out/report.visualized.pdf"))
        );
    }
}
