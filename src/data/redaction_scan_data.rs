use std::collections::{BTreeMap, BTreeSet};

use crate::data::redactions_data::{PdfFileRetriever, RedactionDataRetriever};
use crate::types::redaction_types::{
    PdfRenderer, Rect, RedactionFinderConfig, RedactionFinderOutput, RedactionMode,
    RedactionOccurrence, RedactionReport, UnderlyingTextHit,
};
use crate::types::runtime_defaults::{RASTER_HIGHPASS_DPI, RASTER_PREPASS_DPI};
use crate::types::time::Instant;
use serde::Serialize;

const LINE_BUCKET_PT: f32 = 2.0;
const Y_BAND_PADDING_PT: f32 = 2.0;
const WORD_JOIN_GAP_PT: f32 = 30.0;
const LINE_SEARCH_WINDOW_PT: f32 = 18.0;
const MAX_CONTEXT_GAP_PT: f32 = 80.0;
const LARGE_OVERLAP_PT: f32 = 20.0;
const MAX_CONTEXT_WORDS_PER_SIDE: usize = 2;
const CONTEXT_CLUSTER_MAX_Y_GAP_PT: f32 = 18.0;
const CONTEXT_CLUSTER_WRAP_RESET_X_DELTA_PT: f32 = 16.0;
const CONTEXT_CLUSTER_WRAP_FORWARD_X_ALLOWANCE_PT: f32 = 28.0;
const MAX_CONTEXT_SPANS_PER_REDACTION: usize = 48;
pub(crate) const CONTEXT_SPANS_META_KEY: &str = "context_spans_json_v1";
type LineMatchScore = (i32, i32, i32, i32);
type LineMatch = (Vec<usize>, Option<usize>, Option<usize>, LineMatchScore);

#[derive(Debug, Clone, Serialize)]
struct ContextSpanRecord {
    text: String,
    bbox: Rect,
    line_bucket: i32,
    role_hint: String,
    source: String,
}

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
            collect_page_annotations(retriever, *page_index, cfg, acc);
        }
        if plan.include_drawn {
            collect_page_drawn(retriever, *page_index, cfg, acc);
        }
    }
}

fn collect_page_annotations(
    retriever: &dyn RedactionDataRetriever,
    page_index: u32,
    cfg: &RedactionFinderConfig,
    acc: &mut ScanAccumulator,
) {
    match retriever.annotation_redactions(page_index, *cfg) {
        Ok(v) => acc.occurrences.extend(v),
        Err(m) => acc
            .diagnostics
            .push(format!("page_index={page_index} annotation_error={m}")),
    }
}

fn collect_page_drawn(
    retriever: &dyn RedactionDataRetriever,
    page_index: u32,
    cfg: &RedactionFinderConfig,
    acc: &mut ScanAccumulator,
) {
    match retriever.drawn_redactions(page_index, *cfg) {
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
        match retriever.raster_redactions(*page_index, prepass_cfg) {
            Ok(v) => {
                if !v.is_empty() {
                    candidate_pages.push(*page_index);
                    prepass_by_page.insert(*page_index, v);
                }
            }
            Err(m) => diagnostics.push(format!("page_index={page_index} raster_prepass_error={m}")),
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
        match retriever.raster_redactions(*page_index, highpass_cfg) {
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
        let (context_hits, context_spans) =
            collect_context_hits_for_redaction(&hits, &redaction.bbox);
        redaction.underlying_text = context_hits;
        if !context_spans.is_empty() {
            match serde_json::to_string(&context_spans) {
                Ok(encoded) => {
                    redaction
                        .meta
                        .insert(CONTEXT_SPANS_META_KEY.to_owned(), encoded);
                }
                Err(error) => diagnostics.push(format!(
                    "page_index={} context_span_encode_error={error}",
                    redaction.page_index
                )),
            }
        }
    }
}

fn collect_context_hits_for_redaction(
    hits: &[UnderlyingTextHit],
    red_bbox: &Rect,
) -> (Vec<UnderlyingTextHit>, Vec<ContextSpanRecord>) {
    let red_center_y = (red_bbox.y0 + red_bbox.y1) * 0.5;
    let by_line = collect_line_candidates(hits, red_bbox, red_center_y);
    let by_line_for_cluster = by_line.clone();
    let Some((line, before_anchor, after_anchor, _)) =
        select_best_context_line(hits, red_bbox, red_center_y, by_line)
    else {
        return (empty_context_hits(hits, red_bbox), Vec::new());
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

    let context_hits = vec![
        build_phrase_hit(page_index, &before_phrase, hits, red_bbox),
        build_phrase_hit(page_index, &after_phrase, hits, red_bbox),
    ];
    let line_cluster =
        collect_context_line_cluster(hits, red_bbox, red_center_y, &line, &by_line_for_cluster);
    let context_spans = build_context_span_records(hits, red_bbox, &line_cluster);
    (context_hits, context_spans)
}

fn collect_line_candidates(
    hits: &[UnderlyingTextHit],
    red_bbox: &Rect,
    red_center_y: f32,
) -> BTreeMap<i32, Vec<usize>> {
    let band = Rect::new(
        red_bbox.x0,
        red_bbox.y0 - Y_BAND_PADDING_PT,
        red_bbox.x1,
        red_bbox.y1 + Y_BAND_PADDING_PT,
    );
    let mut by_line = BTreeMap::<i32, Vec<usize>>::new();
    for (idx, hit) in hits.iter().enumerate() {
        let hit_center_y = (hit.bbox.y0 + hit.bbox.y1) * 0.5_f32;
        let close_in_y = (hit_center_y - red_center_y).abs() <= LINE_SEARCH_WINDOW_PT;
        if vertical_overlap(&hit.bbox, &band) <= 0.0_f32 && !close_in_y {
            continue;
        }
        by_line.entry(line_bucket(&hit.bbox)).or_default().push(idx);
    }
    by_line
}

fn select_best_context_line(
    hits: &[UnderlyingTextHit],
    red_bbox: &Rect,
    red_center_y: f32,
    by_line: BTreeMap<i32, Vec<usize>>,
) -> Option<LineMatch> {
    let mut best_line = None::<LineMatch>;
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
        let before_anchor = select_before_anchor(&line, hits, red_bbox);
        let after_anchor = select_after_anchor(&line, hits, red_bbox);
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

        let line_center_y = line
            .iter()
            .map(|idx| (hits[*idx].bbox.y0 + hits[*idx].bbox.y1) * 0.5_f32)
            .sum::<f32>()
            / line.len() as f32;
        let mut context_rank = match (before_anchor.is_some(), after_anchor.is_some()) {
            (true, true) => 0_i32,
            (true, false) => 1_i32,
            (false, true) => 2_i32,
            (false, false) => 3_i32,
        };
        if overlap_pt > LARGE_OVERLAP_PT && before_anchor.is_some() && after_anchor.is_some() {
            context_rank += 2_i32;
        }
        let y_rank = ((line_center_y - red_center_y).abs() * 100.0_f32).round() as i32;
        let before_gap_rank = context_gap_rank_before(&line, before_anchor, hits, red_bbox);
        let after_gap_rank = context_gap_rank_after(&line, after_anchor, hits, red_bbox);
        let score = (context_rank, y_rank, before_gap_rank, after_gap_rank);
        match &best_line {
            None => best_line = Some((line, before_anchor, after_anchor, score)),
            Some((_, _, _, best_score)) if score < *best_score => {
                best_line = Some((line, before_anchor, after_anchor, score));
            }
            _ => {}
        }
    }
    best_line
}

fn select_before_anchor(
    line: &[usize],
    hits: &[UnderlyingTextHit],
    red_bbox: &Rect,
) -> Option<usize> {
    line.iter()
        .enumerate()
        .filter(|(_, idx)| hits[**idx].bbox.x0 < red_bbox.x0)
        .map(|(pos, _)| pos)
        .next_back()
        .filter(|pos| {
            let idx = line[*pos];
            let gap = (red_bbox.x0 - hits[idx].bbox.x1).max(0.0_f32);
            gap <= MAX_CONTEXT_GAP_PT || horizontal_overlap(&hits[idx].bbox, red_bbox) > 0.0_f32
        })
        .or_else(|| {
            line.iter()
                .enumerate()
                .filter(|(_, idx)| hits[**idx].bbox.x0 < red_bbox.x0)
                .map(|(pos, _)| pos)
                .next_back()
        })
}

fn select_after_anchor(
    line: &[usize],
    hits: &[UnderlyingTextHit],
    red_bbox: &Rect,
) -> Option<usize> {
    line.iter()
        .enumerate()
        .filter(|(_, idx)| hits[**idx].bbox.x1 > red_bbox.x1)
        .map(|(pos, _)| pos)
        .next()
        .filter(|pos| {
            let idx = line[*pos];
            let gap = (hits[idx].bbox.x0 - red_bbox.x1).max(0.0_f32);
            gap <= MAX_CONTEXT_GAP_PT || horizontal_overlap(&hits[idx].bbox, red_bbox) > 0.0_f32
        })
        .or_else(|| {
            line.iter()
                .enumerate()
                .filter(|(_, idx)| hits[**idx].bbox.x1 > red_bbox.x1)
                .map(|(pos, _)| pos)
                .next()
        })
}

fn context_gap_rank_before(
    line: &[usize],
    before_anchor: Option<usize>,
    hits: &[UnderlyingTextHit],
    red_bbox: &Rect,
) -> i32 {
    if let Some(pos) = before_anchor {
        let idx = line[pos];
        ((red_bbox.x0 - hits[idx].bbox.x1).max(0.0_f32) * 100.0_f32).round() as i32
    } else {
        100_000_i32
    }
}

fn context_gap_rank_after(
    line: &[usize],
    after_anchor: Option<usize>,
    hits: &[UnderlyingTextHit],
    red_bbox: &Rect,
) -> i32 {
    if let Some(pos) = after_anchor {
        let idx = line[pos];
        ((hits[idx].bbox.x0 - red_bbox.x1).max(0.0_f32) * 100.0_f32).round() as i32
    } else {
        100_000_i32
    }
}

fn collect_context_line_cluster(
    hits: &[UnderlyingTextHit],
    red_bbox: &Rect,
    red_center_y: f32,
    primary_line: &[usize],
    by_line: &BTreeMap<i32, Vec<usize>>,
) -> Vec<(i32, Vec<usize>)> {
    if primary_line.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::<(i32, Vec<usize>)>::new();
    let mut primary_sorted = primary_line.to_vec();
    primary_sorted.sort_by(|left_idx, right_idx| {
        hits[*left_idx]
            .bbox
            .x0
            .partial_cmp(&hits[*right_idx].bbox.x0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                hits[*left_idx]
                    .bbox
                    .x1
                    .partial_cmp(&hits[*right_idx].bbox.x1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let primary_bucket = primary_sorted
        .first()
        .map(|index| line_bucket(&hits[*index].bbox))
        .unwrap_or(0_i32);
    let primary_first_x = primary_sorted
        .first()
        .map(|index| hits[*index].bbox.x0)
        .unwrap_or(red_bbox.x0);
    let primary_last_x = primary_sorted
        .last()
        .map(|index| hits[*index].bbox.x1)
        .unwrap_or(red_bbox.x1);
    let primary_center_y = primary_sorted
        .iter()
        .map(|idx| (hits[*idx].bbox.y0 + hits[*idx].bbox.y1) * 0.5_f32)
        .sum::<f32>()
        / primary_sorted.len() as f32;
    out.push((primary_bucket, primary_sorted));

    for (bucket, line_indices) in by_line {
        if *bucket == primary_bucket {
            continue;
        }
        let mut sorted = line_indices.clone();
        if sorted.is_empty() {
            continue;
        }
        sorted.sort_by(|left_idx, right_idx| {
            hits[*left_idx]
                .bbox
                .x0
                .partial_cmp(&hits[*right_idx].bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    hits[*left_idx]
                        .bbox
                        .x1
                        .partial_cmp(&hits[*right_idx].bbox.x1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        let line_center_y = sorted
            .iter()
            .map(|idx| (hits[*idx].bbox.y0 + hits[*idx].bbox.y1) * 0.5_f32)
            .sum::<f32>()
            / sorted.len() as f32;
        let y_delta = (line_center_y - primary_center_y).abs();
        if y_delta > CONTEXT_CLUSTER_MAX_Y_GAP_PT
            || (line_center_y - red_center_y).abs() > CONTEXT_CLUSTER_MAX_Y_GAP_PT
        {
            continue;
        }
        let line_first_x = sorted
            .first()
            .map(|idx| hits[*idx].bbox.x0)
            .unwrap_or(red_bbox.x0);
        let wraps_to_new_line =
            line_first_x + CONTEXT_CLUSTER_WRAP_RESET_X_DELTA_PT < primary_first_x;
        let continues_forward =
            line_first_x <= primary_last_x + CONTEXT_CLUSTER_WRAP_FORWARD_X_ALLOWANCE_PT;
        if wraps_to_new_line || continues_forward {
            out.push((*bucket, sorted));
        }
    }

    out.sort_by(|left, right| {
        let left_center = left
            .1
            .iter()
            .map(|idx| (hits[*idx].bbox.y0 + hits[*idx].bbox.y1) * 0.5_f32)
            .sum::<f32>()
            / left.1.len().max(1) as f32;
        let right_center = right
            .1
            .iter()
            .map(|idx| (hits[*idx].bbox.y0 + hits[*idx].bbox.y1) * 0.5_f32)
            .sum::<f32>()
            / right.1.len().max(1) as f32;
        left_center
            .partial_cmp(&right_center)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    out
}

fn build_context_span_records(
    hits: &[UnderlyingTextHit],
    red_bbox: &Rect,
    lines: &[(i32, Vec<usize>)],
) -> Vec<ContextSpanRecord> {
    let mut out = Vec::<ContextSpanRecord>::new();
    let mut seen = BTreeSet::<String>::new();
    for (line_bucket, line_indices) in lines {
        for idx in line_indices {
            let Some(hit) = hits.get(*idx) else {
                continue;
            };
            let text = hit.text.trim();
            if text.is_empty() {
                continue;
            }
            let key = context_span_dedup_key(hit, *line_bucket);
            if !seen.insert(key) {
                continue;
            }
            out.push(ContextSpanRecord {
                text: text.to_owned(),
                bbox: hit.bbox,
                line_bucket: *line_bucket,
                role_hint: context_span_role_hint(hit, red_bbox).to_owned(),
                source: "line_cluster".to_owned(),
            });
        }
    }
    out.sort_by(|left, right| {
        let left_center_y = (left.bbox.y0 + left.bbox.y1) * 0.5_f32;
        let right_center_y = (right.bbox.y0 + right.bbox.y1) * 0.5_f32;
        left_center_y
            .partial_cmp(&right_center_y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.bbox
                    .x0
                    .partial_cmp(&right.bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.text.cmp(&right.text))
    });
    if out.len() > MAX_CONTEXT_SPANS_PER_REDACTION {
        out.truncate(MAX_CONTEXT_SPANS_PER_REDACTION);
    }
    out
}

fn context_span_role_hint(hit: &UnderlyingTextHit, red_bbox: &Rect) -> &'static str {
    if hit.bbox.x1 <= red_bbox.x0 + 0.5_f32 {
        "left"
    } else if hit.bbox.x0 >= red_bbox.x1 - 0.5_f32 {
        "right"
    } else {
        "center"
    }
}

fn context_span_dedup_key(hit: &UnderlyingTextHit, line_bucket: i32) -> String {
    format!(
        "{line_bucket}:{:.1}:{:.1}:{:.1}:{:.1}:{}",
        hit.bbox.x0,
        hit.bbox.y0,
        hit.bbox.x1,
        hit.bbox.y1,
        hit.text.trim()
    )
}

fn empty_context_hits(hits: &[UnderlyingTextHit], red_bbox: &Rect) -> Vec<UnderlyingTextHit> {
    let page_index = hits.first().map(|hit| hit.page_index).unwrap_or_default();
    vec![
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
    let mut phrase = line[start..=anchor_pos].to_vec();
    if phrase.len() > MAX_CONTEXT_WORDS_PER_SIDE {
        phrase = phrase[phrase.len() - MAX_CONTEXT_WORDS_PER_SIDE..].to_vec();
    }
    phrase
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
