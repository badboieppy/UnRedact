use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::redaction_finder::data::file_retriever::{PdfFileRetriever, RedactionDataRetriever};
use crate::redaction_finder::types::{
    PdfRenderer, Rect, RedactionFinderConfig, RedactionFinderOutput, RedactionKind, RedactionMode,
    RedactionOccurrence, RedactionReport, UnderlyingTextHit,
};

const LINE_BUCKET_PT: f32 = 2.0;
const Y_BAND_PADDING_PT: f32 = 2.0;
const WORD_JOIN_GAP_PT: f32 = 30.0;
const LINE_SEARCH_WINDOW_PT: f32 = 18.0;
const MAX_CONTEXT_GAP_PT: f32 = 18.0;
const EDGE_OVERLAP_TOL_PT: f32 = 12.0;
const LARGE_OVERLAP_PT: f32 = 20.0;
type LineMatchScore = (i32, i32, i32, i32);
type LineMatch = (Vec<usize>, Option<usize>, Option<usize>, LineMatchScore);

#[inline]
pub fn run_redaction_finder_from_bytes(
    bytes: &[u8],
    renderer: Option<&dyn PdfRenderer>,
    cfg: RedactionFinderConfig,
) -> Result<RedactionFinderOutput, String> {
    let retriever = PdfFileRetriever::new_from_bytes(bytes, renderer)?;
    Ok(run_redaction_finder(&retriever, cfg))
}

#[inline]
pub fn run_redaction_finder(
    retriever: &dyn RedactionDataRetriever,
    cfg: RedactionFinderConfig,
) -> RedactionFinderOutput {
    let mut all: Vec<RedactionOccurrence> = Vec::new();
    let mut diagnostics: Vec<String> = Vec::new();

    for page_index in retriever.page_indices() {
        match cfg.mode {
            RedactionMode::Annotations | RedactionMode::All => match retriever
                .annotation_redactions(page_index, cfg.include_details)
            {
                Ok(v) => all.extend(v),
                Err(m) => diagnostics.push(format!("page_index={page_index} annotation_error={m}")),
            },
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

        attach_underlying_text(retriever, page_index, &mut all, &mut diagnostics);
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
                bbox.x0 < red_bbox.x0 && bbox.x1 <= red_bbox.x0 + EDGE_OVERLAP_TOL_PT
            })
            .map(|(pos, _)| pos)
            .next_back()
            .filter(|pos| {
                let idx = line[*pos];
                (red_bbox.x0 - hits[idx].bbox.x1).max(0.0) <= MAX_CONTEXT_GAP_PT
            });

        let after_anchor = line
            .iter()
            .enumerate()
            .filter(|(_, idx)| {
                let bbox = &hits[**idx].bbox;
                bbox.x1 > red_bbox.x1 && bbox.x0 >= red_bbox.x1 - EDGE_OVERLAP_TOL_PT
            })
            .map(|(pos, _)| pos)
            .next()
            .filter(|pos| {
                let idx = line[*pos];
                (hits[idx].bbox.x0 - red_bbox.x1).max(0.0) <= MAX_CONTEXT_GAP_PT
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

fn grow_phrase_left(line: &[usize], anchor_pos: usize, hits: &[UnderlyingTextHit]) -> Vec<usize> {
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

fn grow_phrase_right(line: &[usize], anchor_pos: usize, hits: &[UnderlyingTextHit]) -> Vec<usize> {
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

#[inline]
pub fn is_unknown_kind(kind: &RedactionKind) -> bool {
    matches!(kind, RedactionKind::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(page_index: u32, x0: f32, y0: f32, x1: f32, y1: f32, text: &str) -> UnderlyingTextHit {
        UnderlyingTextHit {
            page_index,
            bbox: Rect::new(x0, y0, x1, y1),
            text: text.to_owned(),
        }
    }

    #[test]
    fn unknown_kind_helper_is_deterministic() {
        assert!(is_unknown_kind(&RedactionKind::Unknown));
        assert!(!is_unknown_kind(&RedactionKind::Annotation));
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
            hit(0, 0.0, 90.0, 10.0, 100.0, "left_far"),
            hit(0, 120.0, 90.0, 140.0, 100.0, "right_far"),
        ];
        let red = Rect::new(30.0, 88.0, 40.0, 102.0);
        let context = collect_context_hits_for_redaction(&hits, &red);
        let words = context.iter().map(|h| h.text.as_str()).collect::<Vec<_>>();
        assert_eq!(words, vec!["", ""]);
    }
}
