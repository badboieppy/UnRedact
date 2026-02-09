use std::path::Path;

use crate::font_detection::logic::types::file_types::{FontRunReport, FontTextRun, Rect as FontRect};
use crate::redaction_finder::types::{Rect, RedactionReport};
use crate::redaction_guess::types::GuessReport;
use crate::redaction_visualizer::dependency::FileStore;
use crate::redaction_visualizer::types::TextOverlay;

#[derive(Debug, Clone)]
pub struct VisualizationInputs {
    pub pdf_bytes: Vec<u8>,
    pub rects: Vec<(u32, Rect)>,
    pub overlays: Vec<TextOverlay>,
}

pub trait VisualizationDataSource {
    fn load_inputs(
        &self,
        pdf_path: &Path,
        report: &RedactionReport,
        guesses: Option<&GuessReport>,
        font_runs: Option<&FontRunReport>,
    ) -> Result<VisualizationInputs, String>;
    fn write_output(&self, output_path: &Path, bytes: &[u8]) -> Result<(), String>;
}

#[derive(Debug, Clone, Copy)]
pub struct VisualizationData {
    file_store: FileStore,
}

impl VisualizationData {
    #[inline]
    pub fn new() -> Self {
        Self {
            file_store: FileStore,
        }
    }
}

impl Default for VisualizationData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl VisualizationDataSource for VisualizationData {
    #[inline]
    fn load_inputs(
        &self,
        pdf_path: &Path,
        report: &RedactionReport,
        guesses: Option<&GuessReport>,
        font_runs: Option<&FontRunReport>,
    ) -> Result<VisualizationInputs, String> {
        let pdf_bytes = self.file_store.read(pdf_path)?;
        let mut rects = Vec::with_capacity(report.redactions.len());
        for r in &report.redactions {
            rects.push((r.page_index, r.bbox));
        }
        let overlays = build_overlays(report, guesses, font_runs);
        Ok(VisualizationInputs {
            pdf_bytes,
            rects,
            overlays,
        })
    }

    #[inline]
    fn write_output(&self, output_path: &Path, bytes: &[u8]) -> Result<(), String> {
        self.file_store.write(output_path, bytes)
    }
}

fn build_overlays(
    report: &RedactionReport,
    guesses: Option<&GuessReport>,
    font_runs: Option<&FontRunReport>,
) -> Vec<TextOverlay> {
    let mut out = Vec::new();
    let guesses = match guesses {
        Some(g) => g,
        None => return out,
    };
    let font_runs = match font_runs {
        Some(runs) => runs,
        None => return out,
    };

    let max = report.redactions.len().min(guesses.guesses.len());
    for idx in 0..max {
        let redaction = &report.redactions[idx];
        let guess = &guesses.guesses[idx];
        let left_hit = redaction.underlying_text.first();
        let right_hit = redaction.underlying_text.get(1);
        let left_text = left_hit.map(|h| h.text.trim());
        let right_text = right_hit.map(|h| h.text.trim());
        let left_text = match left_text {
            Some(text) if !text.is_empty() => text,
            _ => continue,
        };
        let right_text = match right_text {
            Some(text) if !text.is_empty() => text,
            _ => continue,
        };

        let selected = pick_best_guess(guess);
        let selected = match selected {
            Some(text) => text,
            None => continue,
        };

        let left_bbox = left_hit.map(|h| h.bbox);
        let right_bbox = right_hit.map(|h| h.bbox);
        let left_run = select_run_by_text(
            &font_runs.runs,
            redaction.page_index,
            left_text,
            left_bbox,
        )
        .or_else(|| select_run_by_bbox(&font_runs.runs, redaction.page_index, left_bbox));
        let right_run = select_run_by_text(
            &font_runs.runs,
            redaction.page_index,
            right_text,
            right_bbox,
        )
        .or_else(|| select_run_by_bbox(&font_runs.runs, redaction.page_index, right_bbox));
        let (left_run, _right_run) = match (left_run, right_run) {
            (Some(l), Some(r)) => (l, r),
            _ => continue,
        };

        let full = format!("{} {} {}", left_text, selected.trim(), right_text)
            .trim()
            .to_owned();
        if full.is_empty() {
            continue;
        }

        let left_bbox = left_bbox.unwrap_or(redaction.bbox);
        let right_bbox = right_bbox.unwrap_or(redaction.bbox);
        let x0 = left_bbox.x0.min(redaction.bbox.x0);
        let x1 = right_bbox.x1.max(redaction.bbox.x1);
        let y0 = left_bbox
            .y0
            .min(right_bbox.y0)
            .min(redaction.bbox.y0);
        let y1 = left_bbox
            .y1
            .max(right_bbox.y1)
            .max(redaction.bbox.y1);
        let overlay_bbox = Rect::new(x0, y0, x1, y1);

        out.push(TextOverlay {
            page_index: redaction.page_index,
            text: full,
            font_key: left_run.font_key.clone(),
            font_size_pt: left_run.font_size_pt,
            x: left_run.bbox.x0,
            y: left_run.bbox.y1,
            bbox: overlay_bbox,
        });
    }
    out
}

fn pick_best_guess(guess: &crate::redaction_guess::types::RedactionGuess) -> Option<&str> {
    if let Some(first) = guess.exact_matches.first() {
        return Some(first);
    }
    guess.candidates.first().map(|c| c.text.as_str())
}

fn select_run_by_text<'a>(
    runs: &'a [FontTextRun],
    page_index: u32,
    text: &str,
    bbox: Option<Rect>,
) -> Option<&'a FontTextRun> {
    let mut best: Option<(&FontTextRun, f32)> = None;
    for run in runs {
        if run.page_index != page_index {
            continue;
        }
        if run.text.trim() != text {
            continue;
        }
        if let Some(b) = bbox {
            if vertical_overlap_run(&run.bbox, &b) <= 0.0 {
                continue;
            }
        }
        let dist = bbox.map(|b| (run.bbox.x0 - b.x0).abs()).unwrap_or(0.0);
        match best {
            None => best = Some((run, dist)),
            Some((_, best_score)) if dist < best_score => best = Some((run, dist)),
            _ => {}
        }
    }
    best.map(|(r, _)| r)
}

fn select_run_by_bbox(
    runs: &[FontTextRun],
    page_index: u32,
    bbox: Option<Rect>,
) -> Option<&FontTextRun> {
    let bbox = bbox?;
    let mut best: Option<(&FontTextRun, f32, f32)> = None;
    for run in runs {
        if run.page_index != page_index {
            continue;
        }
        let overlap = vertical_overlap_run(&run.bbox, &bbox);
        if overlap <= 0.0 {
            continue;
        }
        let dist = (run.bbox.x0 - bbox.x0).abs();
        match best {
            None => best = Some((run, overlap, dist)),
            Some((_, best_overlap, best_dist)) => {
                if overlap > best_overlap || (overlap == best_overlap && dist < best_dist) {
                    best = Some((run, overlap, dist));
                }
            }
        }
    }
    best.map(|(r, _, _)| r)
}

fn vertical_overlap_run(a: &FontRect, b: &Rect) -> f32 {
    (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0)
}
