use std::path::Path;

use crate::font_detection::logic::types::file_types::{
    FontAsset, FontRunReport, FontTextRun, Rect as FontRect,
};
use crate::redaction_finder::types::{Rect, RedactionOccurrence, RedactionReport};
use crate::redaction_guess::data::{DictionaryDataSource, FontRunDataSource, ReportDataSource};
use crate::redaction_guess::types::{GuessCandidate, GuessConfig, GuessContext, GuessReport, RedactionGuess};

#[derive(Debug, Clone)]
struct WordMetric {
    word: String,
    width_pt: f64,
}

#[derive(Debug, Clone)]
struct CandidateInternal {
    text: String,
    error_pt: f64,
    word_count: usize,
}

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
pub fn guess_for_redaction(
    redaction: &RedactionOccurrence,
    dictionary: &[String],
    cfg: &GuessConfig,
) -> RedactionGuess {
    let (left_text, right_text, left_bbox, right_bbox) = extract_context(redaction);
    let gap_pt = compute_gap_pt(redaction.bbox, left_bbox, right_bbox);
    let char_width_pt = estimate_char_width_pt(
        &left_text,
        &right_text,
        left_bbox,
        right_bbox,
        redaction.bbox,
    );
    let tol_pt = cfg.tol_pt.max(0.0);
    let candidates = if dictionary.is_empty() || gap_pt <= 0.0_f64 || char_width_pt <= 0.0_f64 {
        Vec::new()
    } else {
        let space_width = char_width_pt * 0.6_f64;
        let words = build_word_metrics(dictionary, char_width_pt);
        let mut found = search_candidates(&words, gap_pt, tol_pt, space_width, cfg);
        found.sort_by(|a, b| {
            a.error_pt
                .partial_cmp(&b.error_pt)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.word_count.cmp(&b.word_count))
                .then_with(|| a.text.cmp(&b.text))
        });
        found.truncate(cfg.max_candidates);
        found
            .into_iter()
            .map(|c| {
                let score = if tol_pt > 0.0_f64 {
                    (1.0 - (c.error_pt / tol_pt)).clamp(0.0, 1.0)
                } else {
                    0.0_f64
                };
                GuessCandidate {
                    text: c.text,
                    score: score as f32,
                    error_pt: c.error_pt as f32,
                    word_count: c.word_count as u32,
                }
            })
            .collect::<Vec<_>>()
    };

    RedactionGuess {
        page_index: redaction.page_index,
        bbox: redaction.bbox,
        candidates,
        exact_matches: Vec::new(),
        context: GuessContext {
            left_text,
            right_text,
            gap_pt: gap_pt as f32,
            char_width_pt: char_width_pt as f32,
            tol_pt: tol_pt as f32,
        },
    }
}

#[inline]
pub fn build_guesses(
    redactions: &[RedactionOccurrence],
    dictionary: &[String],
    cfg: &GuessConfig,
) -> Vec<RedactionGuess> {
    redactions
        .iter()
        .map(|r| guess_for_redaction(r, dictionary, cfg))
        .collect::<Vec<_>>()
}

#[inline]
pub fn run_from_paths(
    req: RunGuessRequest<'_>,
) -> Result<GuessReport, String> {
    let reports = req
        .report_data
        .load_reports(req.redactions_path, req.fonts_path)?;
    let dictionary = req.dictionary_data.load_dictionary(
        req.dictionary_path,
        req.cfg.max_dictionary,
    )?;
    let font_runs = req.font_run_data.load_font_runs(req.pdf_path)?;
    let mut diagnostics = reports.diagnostics;
    diagnostics.extend(dictionary.diagnostics);
    Ok(build_report_from_parts_with_fonts(
        req.redactions_path,
        req.fonts_path,
        reports.redactions,
        dictionary.dictionary,
        diagnostics,
        font_runs.report,
        req.cfg,
    ))
}

#[inline]
pub fn build_report_from_parts(
    redactions_path: &Path,
    fonts_path: &Path,
    redactions: RedactionReport,
    dictionary: Vec<String>,
    diagnostics: Vec<String>,
    cfg: &GuessConfig,
) -> GuessReport {
    let guesses = build_guesses(&redactions.redactions, &dictionary, cfg);
    GuessReport {
        input_redactions: redactions_path.to_string_lossy().to_string(),
        input_fonts: fonts_path.to_string_lossy().to_string(),
        guesses,
        diagnostics,
    }
}

#[inline]
pub fn build_report_from_parts_with_fonts(
    redactions_path: &Path,
    fonts_path: &Path,
    redactions: RedactionReport,
    dictionary: Vec<String>,
    diagnostics: Vec<String>,
    font_runs: FontRunReport,
    cfg: &GuessConfig,
) -> GuessReport {
    let exact = find_exact_matches(&redactions.redactions, &font_runs, &dictionary, cfg);
    let mut guesses = build_guesses(&redactions.redactions, &dictionary, cfg);
    for (guess, matches) in guesses.iter_mut().zip(exact.into_iter()) {
        guess.exact_matches = matches;
    }
    GuessReport {
        input_redactions: redactions_path.to_string_lossy().to_string(),
        input_fonts: fonts_path.to_string_lossy().to_string(),
        guesses,
        diagnostics,
    }
}

fn find_exact_matches(
    redactions: &[RedactionOccurrence],
    font_runs: &FontRunReport,
    dictionary: &[String],
    cfg: &GuessConfig,
) -> Vec<Vec<String>> {
    let assets = font_runs
        .assets
        .iter()
        .map(|a| (a.font_key.clone(), a.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut by_page: std::collections::BTreeMap<u32, Vec<&FontTextRun>> =
        std::collections::BTreeMap::new();
    for run in &font_runs.runs {
        by_page.entry(run.page_index).or_default().push(run);
    }

    let mut out = vec![Vec::new(); redactions.len()];
    for (idx, redaction) in redactions.iter().enumerate() {
        let left = redaction.underlying_text.first();
        let right = redaction.underlying_text.get(1);
        let left_text = left.map(|h| h.text.trim()).unwrap_or("");
        let right_text = right.map(|h| h.text.trim()).unwrap_or("");
        if left_text.is_empty() || right_text.is_empty() {
            continue;
        }
        let runs = match by_page.get(&redaction.page_index) {
            Some(r) => r,
            None => continue,
        };
        let left_run = match select_run(runs, left_text, left.map(|h| h.bbox), redaction.bbox) {
            Some(r) => r,
            None => continue,
        };
        let right_run = match select_run(runs, right_text, right.map(|h| h.bbox), redaction.bbox) {
            Some(r) => r,
            None => continue,
        };
        if left_run.font_key != right_run.font_key {
            continue;
        }
        if (left_run.font_size_pt - right_run.font_size_pt).abs() > 0.01 {
            continue;
        }
        let asset = match assets.get(&left_run.font_key) {
            Some(a) => a,
            None => continue,
        };
        let matches = exact_matches_for_row(
            left_run,
            right_run,
            dictionary,
            asset,
            cfg,
        );
        if !matches.is_empty() {
            out[idx] = matches;
        }
    }
    out
}

fn select_run<'a>(
    runs: &'a [&FontTextRun],
    text: &str,
    bbox: Option<Rect>,
    red_bbox: Rect,
) -> Option<&'a FontTextRun> {
    let mut best: Option<(&FontTextRun, f32)> = None;
    for run in runs {
        if run.text.trim() != text {
            continue;
        }
        if let Some(b) = bbox {
            if vertical_overlap_run(&run.bbox, &b) <= 0.0 {
                continue;
            }
        }
        let dist = if run.bbox.x1 < red_bbox.x0 {
            red_bbox.x0 - run.bbox.x1
        } else if run.bbox.x0 > red_bbox.x1 {
            run.bbox.x0 - red_bbox.x1
        } else {
            0.0
        };
        let score = dist.abs();
        match best {
            None => best = Some((run, score)),
            Some((_, best_score)) if score < best_score => best = Some((run, score)),
            _ => {}
        }
    }
    best.map(|(r, _)| r)
}

fn exact_matches_for_row(
    left: &FontTextRun,
    right: &FontTextRun,
    dictionary: &[String],
    asset: &FontAsset,
    cfg: &GuessConfig,
) -> Vec<String> {
    let face = match rustybuzz::Face::from_slice(&asset.bytes, 0) {
        Some(f) => f,
        None => return Vec::new(),
    };
    let units_per_em = asset.units_per_em.max(1) as f32;
    let font_size = left.font_size_pt;
    let left_x = left.bbox.x0;
    let right_x = right.bbox.x0;
    let tol = 0.1_f32;

    let mut out = Vec::new();
    for word in dictionary {
        let candidate = format!("{} {} ", left.text.trim(), word.trim());
        let advance = advance_pt(&face, &candidate, font_size, units_per_em);
        let expected = left_x + advance;
        let err = (expected - right_x).abs();
        if err <= tol {
            out.push(word.clone());
        }
        if out.len() >= cfg.max_candidates {
            break;
        }
    }
    out
}

fn advance_pt(face: &rustybuzz::Face<'_>, text: &str, font_size: f32, units_per_em: f32) -> f32 {
    let mut buf = rustybuzz::UnicodeBuffer::new();
    buf.push_str(text);
    let out = rustybuzz::shape(face, &[], buf);
    let units = out
        .glyph_positions()
        .iter()
        .map(|p| p.x_advance as f32)
        .sum::<f32>()
        / 64.0;
    units * (font_size / units_per_em)
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

fn compute_gap_pt(red_bbox: Rect, left_bbox: Option<Rect>, right_bbox: Option<Rect>) -> f64 {
    let w = red_bbox.width().abs();
    if w > 0.0 {
        return w as f64;
    }
    if let (Some(l), Some(r)) = (left_bbox, right_bbox) {
        return (r.x0 - l.x1).max(0.0) as f64;
    }
    0.0
}

fn estimate_char_width_pt(
    left_text: &str,
    right_text: &str,
    left_bbox: Option<Rect>,
    right_bbox: Option<Rect>,
    red_bbox: Rect,
) -> f64 {
    let mut samples = Vec::new();
    if let (Some(b), count) = (left_bbox, left_text.chars().count()) {
        if count > 0 {
            let w = b.width().abs() as f64;
            if w > 0.0_f64 {
                samples.push(w / count as f64);
            }
        }
    }
    if let (Some(b), count) = (right_bbox, right_text.chars().count()) {
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

fn build_word_metrics(dictionary: &[String], char_width_pt: f64) -> Vec<WordMetric> {
    dictionary
        .iter()
        .map(|w| WordMetric {
            word: w.clone(),
            width_pt: w.chars().count() as f64 * char_width_pt,
        })
        .filter(|w| w.width_pt > 0.0_f64)
        .collect::<Vec<_>>()
}

fn search_candidates(
    words: &[WordMetric],
    target_pt: f64,
    tol_pt: f64,
    space_width: f64,
    cfg: &GuessConfig,
) -> Vec<CandidateInternal> {
    let mut out = Vec::new();
    if words.is_empty() {
        return out;
    }
    let mut stack: Vec<(Vec<usize>, f64)> = vec![(Vec::new(), 0.0)];
    let mut nodes = 0_usize;

    while let Some((seq, width)) = stack.pop() {
        nodes += 1;
        if nodes > cfg.max_nodes {
            break;
        }
        if !seq.is_empty() {
            let err = (width - target_pt).abs();
            if err <= tol_pt {
                let text = seq
                    .iter()
                    .map(|idx| words[*idx].word.clone())
                    .collect::<Vec<_>>()
                    .join(" ");
                out.push(CandidateInternal {
                    text,
                    error_pt: err,
                    word_count: seq.len(),
                });
            }
        }
        if seq.len() >= cfg.max_words {
            continue;
        }
        if width > target_pt + tol_pt {
            continue;
        }
        for (idx, word) in words.iter().enumerate().rev() {
            let add = if seq.is_empty() {
                word.width_pt
            } else {
                word.width_pt + space_width
            };
            let mut next = seq.clone();
            next.push(idx);
            stack.push((next, width + add));
        }
    }
    out
}

fn vertical_overlap_run(a: &FontRect, b: &Rect) -> f32 {
    (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redaction_finder::types::{RedactionKind, UnderlyingTextHit};

    fn redaction(bbox: Rect, left: &str, right: &str) -> RedactionOccurrence {
        RedactionOccurrence {
            page_index: 0,
            bbox,
            kind: RedactionKind::DrawnRect,
            score: 0.5,
            meta: std::collections::BTreeMap::new(),
            underlying_text: vec![
                UnderlyingTextHit {
                    page_index: 0,
                    bbox: Rect::new(0.0, 0.0, 40.0, 10.0),
                    text: left.to_owned(),
                },
                UnderlyingTextHit {
                    page_index: 0,
                    bbox: Rect::new(60.0, 0.0, 120.0, 10.0),
                    text: right.to_owned(),
                },
            ],
        }
    }

    #[test]
    fn guess_for_redaction_is_deterministic() {
        let red = redaction(Rect::new(40.0, 0.0, 80.0, 10.0), "left", "right");
        let dict = vec!["alpha".to_owned(), "beta".to_owned(), "gamma".to_owned()];
        let cfg = GuessConfig {
            max_words: 2,
            max_candidates: 5,
            max_dictionary: 100,
            tol_pt: 10.0,
            max_nodes: 1000,
        };
        let out1 = guess_for_redaction(&red, &dict, &cfg);
        let out2 = guess_for_redaction(&red, &dict, &cfg);
        assert_eq!(out1, out2);
    }
}
