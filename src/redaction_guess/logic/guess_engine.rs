use std::path::Path;

use crate::redaction_finder::types::{Rect, RedactionOccurrence};
use crate::redaction_guess::data::{DictionaryDataSource, ReportDataSource};
use crate::redaction_guess::types::GuessReport;
use crate::redaction_guess::types::{GuessCandidate, GuessConfig, GuessContext, RedactionGuess};

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
    report_data: &dyn ReportDataSource,
    dictionary_data: &dyn DictionaryDataSource,
    redactions_path: &Path,
    fonts_path: &Path,
    dictionary_path: Option<&Path>,
    cfg: &GuessConfig,
) -> Result<crate::redaction_guess::types::GuessReport, String> {
    let reports = report_data.load_reports(redactions_path, fonts_path)?;
    let dictionary =
        dictionary_data.load_dictionary(dictionary_path, &reports.fonts, cfg.max_dictionary)?;
    let mut diagnostics = reports.diagnostics;
    diagnostics.extend(dictionary.diagnostics);
    Ok(build_report_from_parts(
        redactions_path,
        fonts_path,
        reports.redactions,
        dictionary.dictionary,
        diagnostics,
        cfg,
    ))
}

#[inline]
pub fn build_report_from_parts(
    redactions_path: &Path,
    fonts_path: &Path,
    redactions: crate::redaction_finder::types::RedactionReport,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::redaction_finder::types::{RedactionKind, RedactionOccurrence, UnderlyingTextHit};

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
