use crate::types::redaction_types::{
    PdfRenderer, Rect, RedactionFinderConfig, RedactionKind, RedactionOccurrence,
};
use lopdf::{Document, Object, ObjectId};
use std::collections::VecDeque;

use crate::dependency::pdf_redaction::{
    normalized_rect_from_pixels, object_to_rect, rect_is_near_full_page_with_size,
    rect_pixels_to_pdf, score_rect_as_redaction, DetailPolicy, MAX_RASTER_ANALYSIS_DPI,
};

#[derive(Debug, Clone)]
struct ImageDetectionResult {
    detections: Vec<ImageRegionDetection>,
}

#[derive(Debug, Clone)]
struct ImageRegionDetection {
    normalized_rect: Rect,
    avg_luminance: f32,
    area_fraction: f32,
    score: f32,
}

#[derive(Debug, Clone)]
struct DarkRegion {
    x0_px: u32,
    y0_px: u32,
    x1_px: u32,
    y1_px: u32,
    avg_luminance: f32,
    area_fraction: f32,
    score: f32,
}

#[derive(Debug, Clone)]
struct DarkRegionDetections {
    regions: Vec<DarkRegion>,
}

#[derive(Debug, Clone)]
struct DarkRunProfile {
    dark_runs: Vec<(u32, u32)>,
    max_gap_px: u32,
    split_confidence: f32,
    dark_ratio: f32,
}

#[expect(
    clippy::cognitive_complexity,
    reason = "Raster region detection is a single-pass algorithm with connected-component expansion."
)]
fn detect_dark_regions_in_image(gray: &[u8], width: usize, height: usize) -> ImageDetectionResult {
    if width == 0 || height == 0 {
        return ImageDetectionResult {
            detections: Vec::new(),
        };
    }
    let total_pixels = match width.checked_mul(height) {
        Some(v) => v,
        None => {
            return ImageDetectionResult {
                detections: Vec::new(),
            };
        }
    };
    if gray.len() < total_pixels {
        return ImageDetectionResult {
            detections: Vec::new(),
        };
    }

    let mut sum = 0_u64;
    let mut min_v = 255_u8;
    for &px in gray.iter().take(total_pixels) {
        sum += px as u64;
        if px < min_v {
            min_v = px;
        }
    }
    let global_avg = sum as f32 / total_pixels as f32;

    let grid_cols = target_bin_count(width);
    let grid_rows = target_bin_count(height);
    let col_bins = build_bins(width, grid_cols);
    let row_bins = build_bins(height, grid_rows);
    let cols = col_bins.len();
    let rows = row_bins.len();

    let mut cell_sums = vec![0_u64; rows * cols];
    let mut cell_area = vec![1_u32; rows * cols];

    for (row_idx, (y0, y1)) in row_bins.iter().enumerate() {
        let y_span = y1.saturating_sub(*y0).max(1);
        for (col_idx, (x0, x1)) in col_bins.iter().enumerate() {
            let idx = row_idx * cols + col_idx;
            let x_span = x1.saturating_sub(*x0).max(1);
            cell_area[idx] = (x_span * y_span) as u32;
        }
        for y in *y0..*y1 {
            let row_offset = y * width;
            for (col_idx, (x0, x1)) in col_bins.iter().enumerate() {
                let idx = row_idx * cols + col_idx;
                let mut acc = 0_u64;
                for x in *x0..*x1 {
                    acc += gray[row_offset + x] as u64;
                }
                cell_sums[idx] += acc;
            }
        }
    }

    let mut cell_avg = vec![0_f32; rows * cols];
    for idx in 0..cell_sums.len() {
        let area = cell_area[idx].max(1) as f32;
        cell_avg[idx] = cell_sums[idx] as f32 / area;
    }

    let threshold = {
        let base = (global_avg * 0.65).min(120.0);
        base.max(min_v as f32 + 5.0).max(32.0)
    };

    let mut visited = vec![false; rows * cols];
    let mut detections = Vec::new();
    let mut queue = VecDeque::new();

    for row_index in 0..rows {
        for col_index in 0..cols {
            let idx = row_index * cols + col_index;
            if visited[idx] || cell_avg[idx] > threshold {
                continue;
            }
            visited[idx] = true;
            queue.clear();
            queue.push_back((row_index, col_index));

            let mut sum_lum = 0_f32;
            let mut pixel_area = 0_u64;
            let mut min_col = cols;
            let mut max_col = 0;
            let mut min_row = rows;
            let mut max_row = 0;

            while let Some((row, col)) = queue.pop_front() {
                let current = row * cols + col;
                let area = cell_area[current] as u64;
                sum_lum += cell_avg[current] * area as f32;
                pixel_area += area;
                if row < min_row {
                    min_row = row;
                }
                if row > max_row {
                    max_row = row;
                }
                if col < min_col {
                    min_col = col;
                }
                if col > max_col {
                    max_col = col;
                }

                let neighbors = [
                    row.checked_sub(1).map(|prev_row| (prev_row, col)),
                    (row + 1 < rows).then(|| (row + 1, col)),
                    col.checked_sub(1).map(|prev_col| (row, prev_col)),
                    (col + 1 < cols).then(|| (row, col + 1)),
                ];
                for (next_row, next_col) in neighbors.into_iter().flatten() {
                    let neighbor_index = next_row * cols + next_col;
                    if visited[neighbor_index] {
                        continue;
                    }
                    if cell_avg[neighbor_index] > threshold {
                        continue;
                    }
                    visited[neighbor_index] = true;
                    queue.push_back((next_row, next_col));
                }
            }

            if pixel_area == 0 {
                continue;
            }
            let area_fraction = pixel_area as f32 / total_pixels as f32;
            if !(0.0005..=0.9).contains(&area_fraction) {
                continue;
            }
            if min_col >= cols || min_row >= rows {
                continue;
            }
            let x0 = col_bins[min_col].0;
            let x1 = col_bins[max_col].1;
            let y0 = row_bins[min_row].0;
            let y1 = row_bins[max_row].1;
            if x1 <= x0 || y1 <= y0 {
                continue;
            }

            let avg_lum = (sum_lum / pixel_area as f32).clamp(0.0, 255.0);
            let normalized = normalized_rect_from_pixels(x0, y0, x1, y1, width, height);
            let short_edge = ((x1 - x0) as f32).min((y1 - y0) as f32);
            if short_edge < 4.0 {
                continue;
            }

            let darkness = (1.0 - avg_lum / 255.0).clamp(0.0, 1.0);
            let coverage = (area_fraction / 0.12).min(1.0);
            let aspect = {
                let w = (x1 - x0) as f32;
                let h = (y1 - y0) as f32;
                if h > 0.0 && w > 0.0 {
                    (w.max(h) / w.min(h)).min(12.0)
                } else {
                    1.0
                }
            };

            let score = (0.55 * darkness) + (0.35 * coverage) + (0.10 * (aspect / 4.0).min(1.0));

            detections.push(ImageRegionDetection {
                normalized_rect: normalized,
                avg_luminance: avg_lum,
                area_fraction,
                score,
            });
        }
    }

    ImageDetectionResult { detections }
}

fn image_detections_to_dark_regions(
    detections: &ImageDetectionResult,
    width_px: u32,
    height_px: u32,
) -> DarkRegionDetections {
    let mut regions = Vec::new();
    for det in &detections.detections {
        let x0_px = (det.normalized_rect.x0 * width_px as f32)
            .floor()
            .clamp(0.0, width_px as f32) as u32;
        let x1_px = (det.normalized_rect.x1 * width_px as f32)
            .ceil()
            .clamp(0.0, width_px as f32) as u32;
        let y0_px = (det.normalized_rect.y0 * height_px as f32)
            .floor()
            .clamp(0.0, height_px as f32) as u32;
        let y1_px = (det.normalized_rect.y1 * height_px as f32)
            .ceil()
            .clamp(0.0, height_px as f32) as u32;

        if x1_px <= x0_px || y1_px <= y0_px {
            continue;
        }

        regions.push(DarkRegion {
            x0_px,
            y0_px,
            x1_px,
            y1_px,
            avg_luminance: det.avg_luminance,
            area_fraction: det.area_fraction,
            score: det.score,
        });
    }
    DarkRegionDetections { regions }
}

fn target_bin_count(size: usize) -> usize {
    if size <= 16 {
        return size.max(1);
    }
    let approx = ((size as f32) / 4.0).ceil() as usize;
    approx.clamp(16, 512).min(size.max(1))
}

fn build_bins(size: usize, target: usize) -> Vec<(usize, usize)> {
    if size == 0 {
        return vec![(0, 0)];
    }

    let bins = target.max(1).min(size);
    let mut result = Vec::with_capacity(bins);
    let mut start = 0_usize;
    let mut remaining_bins = bins;
    let mut remaining = size;

    while remaining_bins > 0 {
        let chunk = remaining.div_ceil(remaining_bins);
        let end = (start + chunk).min(size);
        result.push((start, end));
        start = end;
        remaining = size - start;
        remaining_bins -= 1;
    }

    result.retain(|(s, e)| e > s);
    if result.is_empty() {
        result.push((0, size));
    }

    result
}

fn dark_run_profile_for_region(
    gray: &[u8],
    width: usize,
    height: usize,
    region: &DarkRegion,
) -> DarkRunProfile {
    if width == 0 || height == 0 || gray.len() < width.saturating_mul(height) {
        return DarkRunProfile {
            dark_runs: Vec::new(),
            max_gap_px: 0,
            split_confidence: 0.0,
            dark_ratio: 0.0,
        };
    }

    let x0 = region.x0_px.min(width as u32) as usize;
    let y0 = region.y0_px.min(height as u32) as usize;
    let x1 = region.x1_px.min(width as u32) as usize;
    let y1 = region.y1_px.min(height as u32) as usize;
    if x1 <= x0 || y1 <= y0 {
        return DarkRunProfile {
            dark_runs: Vec::new(),
            max_gap_px: 0,
            split_confidence: 0.0,
            dark_ratio: 0.0,
        };
    }

    let region_w = x1 - x0;
    let region_h = y1 - y0;
    let region_area = region_w.saturating_mul(region_h).max(1);
    let dark_threshold = (region.avg_luminance + 22.0).clamp(28.0, 145.0) as u8;

    let mut dark_cols = vec![false; region_w];
    let mut dark_pixels = 0_u64;

    for (offset, x) in (x0..x1).enumerate() {
        let mut col_dark = 0_u32;
        let mut col_sum = 0_u32;
        for y in y0..y1 {
            let px = gray[y * width + x];
            col_sum += px as u32;
            if px <= dark_threshold {
                col_dark += 1;
            }
        }
        dark_pixels += col_dark as u64;
        let col_ratio = col_dark as f32 / region_h as f32;
        let col_avg = col_sum as f32 / region_h as f32;
        dark_cols[offset] =
            col_ratio >= 0.55 || (col_ratio >= 0.38 && col_avg <= dark_threshold as f32);
    }

    fill_small_bright_gaps(&mut dark_cols, 1);

    let min_run_px = ((region_w as f32) * 0.035).ceil() as usize;
    let min_run_px = min_run_px.max(2);

    let mut runs = Vec::<(u32, u32)>::new();
    let mut idx = 0_usize;
    while idx < dark_cols.len() {
        if !dark_cols[idx] {
            idx += 1;
            continue;
        }
        let start = idx;
        while idx < dark_cols.len() && dark_cols[idx] {
            idx += 1;
        }
        let end = idx;
        if end.saturating_sub(start) >= min_run_px {
            runs.push((start as u32, end as u32));
        }
    }

    let mut max_gap_px = 0_u32;
    for pair in runs.windows(2) {
        let gap = pair[1].0.saturating_sub(pair[0].1);
        max_gap_px = max_gap_px.max(gap);
    }

    let dark_ratio = (dark_pixels as f32 / region_area as f32).clamp(0.0, 1.0);
    let split_confidence = if runs.len() <= 1 {
        0.0
    } else {
        let run_count_factor = (((runs.len() - 1) as f32) / 3.0).clamp(0.0, 1.0);
        let gap_factor = (max_gap_px as f32 / ((region_w as f32) * 0.18).max(1.0)).clamp(0.0, 1.0);
        let run_coverage = runs
            .iter()
            .map(|(run_x0, run_x1)| run_x1.saturating_sub(*run_x0))
            .sum::<u32>() as f32
            / (region_w as f32).max(1.0);
        let coverage_factor = (1.0 - ((run_coverage - 0.55).abs() / 0.55)).clamp(0.0, 1.0);
        let mut confidence =
            (0.45 * run_count_factor) + (0.35 * gap_factor) + (0.20 * coverage_factor);
        if max_gap_px == 0 {
            confidence *= 0.5;
        }
        if dark_ratio < 0.08 {
            confidence *= 0.6;
        }
        confidence.clamp(0.0, 1.0)
    };

    DarkRunProfile {
        dark_runs: runs,
        max_gap_px,
        split_confidence,
        dark_ratio,
    }
}

fn fill_small_bright_gaps(columns: &mut [bool], max_gap: usize) {
    if columns.is_empty() || max_gap == 0 {
        return;
    }
    let mut idx = 0_usize;
    while idx < columns.len() {
        if columns[idx] {
            idx += 1;
            continue;
        }
        let gap_start = idx;
        while idx < columns.len() && !columns[idx] {
            idx += 1;
        }
        let gap_end = idx;
        let gap_len = gap_end.saturating_sub(gap_start);
        let bounded =
            gap_start > 0 && gap_end < columns.len() && columns[gap_start - 1] && columns[gap_end];
        if bounded && gap_len <= max_gap {
            for col in &mut columns[gap_start..gap_end] {
                *col = true;
            }
        }
    }
}

fn split_dark_region_by_profile(region: &DarkRegion, profile: &DarkRunProfile) -> Vec<DarkRegion> {
    if profile.dark_runs.len() <= 1 || profile.split_confidence < 0.25 {
        return vec![region.clone()];
    }

    let region_width = region.x1_px.saturating_sub(region.x0_px).max(1);
    let mut split = profile
        .dark_runs
        .iter()
        .filter_map(|(run_x0, run_x1)| {
            let abs_x0 = region.x0_px.saturating_add(*run_x0).min(region.x1_px);
            let abs_x1 = region.x0_px.saturating_add(*run_x1).min(region.x1_px);
            if abs_x1 <= abs_x0 {
                return None;
            }
            let span = abs_x1.saturating_sub(abs_x0);
            if span < 2 {
                return None;
            }
            let span_fraction = span as f32 / region_width as f32;
            Some(DarkRegion {
                x0_px: abs_x0,
                y0_px: region.y0_px,
                x1_px: abs_x1,
                y1_px: region.y1_px,
                avg_luminance: region.avg_luminance,
                area_fraction: (region.area_fraction * span_fraction).clamp(0.0, 1.0),
                score: (region.score * (0.85 + 0.15 * profile.split_confidence)).clamp(0.0, 1.0),
            })
        })
        .collect::<Vec<_>>();

    if split.len() <= 1 {
        return vec![region.clone()];
    }
    split.sort_by(|left, right| left.x0_px.cmp(&right.x0_px));
    split
}

pub fn page_render_box_from_page(doc: &Document, page_id: ObjectId) -> Option<Rect> {
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

        let current_obj = doc.get_object(current_id).ok()?;
        let current_dict = match current_obj {
            Object::Dictionary(d) => d,
            _ => return None,
        };

        if let Ok(obj) = current_dict.get(key) {
            if let Some(rect) = object_to_rect_resolved(doc, obj) {
                return Some(rect);
            }
        }

        let parent_ref = match current_dict.get(b"Parent").ok()? {
            Object::Reference(id) => *id,
            _ => return None,
        };
        current_id = parent_ref;
    }
}

fn object_to_rect_resolved(doc: &Document, obj: &Object) -> Option<Rect> {
    match obj {
        Object::Reference(oid) => doc.get_object(*oid).ok().and_then(object_to_rect),
        _ => object_to_rect(obj),
    }
}

struct RasterRenderCapture {
    effective_dpi: f32,
    width_px: u32,
    height_px: u32,
    gray: Vec<u8>,
}

fn capture_raster_render(
    renderer: &dyn PdfRenderer,
    page_index: u32,
    cfg: &RedactionFinderConfig,
) -> Result<Option<RasterRenderCapture>, String> {
    let effective_dpi = cfg.raster_dpi.min(MAX_RASTER_ANALYSIS_DPI);
    let rendered = renderer
        .render_page_to_rgba(page_index as usize, effective_dpi)
        .map_err(|e| format!("render_failed:{e}"))?;
    if rendered.width_px == 0 || rendered.height_px == 0 {
        return Ok(None);
    }
    let gray = rgba_to_grayscale(&rendered.pixels, rendered.width_px, rendered.height_px);
    Ok(Some(RasterRenderCapture {
        effective_dpi,
        width_px: rendered.width_px,
        height_px: rendered.height_px,
        gray,
    }))
}

pub fn extract_raster_page_redactions(
    renderer: &dyn PdfRenderer,
    page_index: u32,
    page_box: Rect,
    cfg: &RedactionFinderConfig,
) -> Result<Vec<RedactionOccurrence>, String> {
    let Some(capture) = capture_raster_render(renderer, page_index, cfg)? else {
        return Ok(Vec::new());
    };

    let detection = detect_dark_regions_in_image(
        &capture.gray,
        capture.width_px as usize,
        capture.height_px as usize,
    );
    if detection.detections.is_empty() {
        return Ok(Vec::new());
    }

    let regions = image_detections_to_dark_regions(&detection, capture.width_px, capture.height_px);

    let mut out = Vec::new();
    for det in regions.regions {
        let profile = dark_run_profile_for_region(
            &capture.gray,
            capture.width_px as usize,
            capture.height_px as usize,
            &det,
        );
        let split_regions = split_dark_region_by_profile(&det, &profile);
        let split_count = split_regions.len().max(1);

        for (split_index, split_region) in split_regions.into_iter().enumerate() {
            let page_rect = rect_pixels_to_pdf(
                split_region.x0_px,
                split_region.y0_px,
                split_region.x1_px,
                split_region.y1_px,
                page_box,
                capture.effective_dpi,
            );

            if rect_is_near_full_page_with_size(
                &page_rect,
                page_box.width().abs(),
                page_box.height().abs(),
            ) {
                continue;
            }

            let score = score_rect_as_redaction(&page_rect);
            if score <= 0.15 {
                continue;
            }

            let split_profile = dark_run_profile_for_region(
                &capture.gray,
                capture.width_px as usize,
                capture.height_px as usize,
                &split_region,
            );

            let meta = build_raster_redaction_meta(
                DetailPolicy::new(cfg.include_details),
                cfg,
                &capture,
                &split_region,
                &split_profile,
                split_index,
                split_count,
            );

            out.push(RedactionOccurrence {
                page_index,
                bbox: page_rect,
                kind: RedactionKind::RasterDarkRegion,
                score,
                meta,
                underlying_text: Vec::new(),
            });
        }
    }

    Ok(out)
}

fn build_raster_redaction_meta(
    detail: DetailPolicy,
    cfg: &RedactionFinderConfig,
    capture: &RasterRenderCapture,
    split_region: &DarkRegion,
    split_profile: &DarkRunProfile,
    split_index: usize,
    split_count: usize,
) -> std::collections::BTreeMap<String, String> {
    let mut meta = detail.new_meta();
    detail.insert_owned(
        &mut meta,
        "raster_dpi",
        format!("{:.1}", capture.effective_dpi),
    );
    detail.insert_owned(
        &mut meta,
        "raster_dpi_requested",
        format!("{:.1}", cfg.raster_dpi),
    );
    detail.insert_owned(
        &mut meta,
        "raster_dpi_effective",
        format!("{:.1}", capture.effective_dpi),
    );
    detail.insert_owned(
        &mut meta,
        "image_dims_px",
        format!("{}x{}", capture.width_px, capture.height_px),
    );
    detail.insert_owned(
        &mut meta,
        "region_area_fraction",
        format!("{:.4}", split_region.area_fraction),
    );
    detail.insert_owned(
        &mut meta,
        "region_avg_luminance",
        format!("{:.1}", split_region.avg_luminance),
    );

    meta.insert(
        "profile_split_confidence".to_owned(),
        format!("{:.3}", split_profile.split_confidence),
    );
    meta.insert(
        "profile_max_gap_px".to_owned(),
        split_profile.max_gap_px.to_string(),
    );
    meta.insert(
        "profile_dark_ratio".to_owned(),
        format!("{:.3}", split_profile.dark_ratio),
    );
    let run_labels = split_profile
        .dark_runs
        .iter()
        .map(|(x0, x1)| format!("{x0}-{x1}"))
        .collect::<Vec<_>>()
        .join(",");
    meta.insert("profile_dark_runs".to_owned(), run_labels);
    if split_count > 1 {
        meta.insert("profile_split_index".to_owned(), split_index.to_string());
        meta.insert("profile_split_count".to_owned(), split_count.to_string());
    }
    meta
}

fn rgba_to_grayscale(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if rgba.len() < expected {
        return Vec::new();
    }

    let mut gray = Vec::with_capacity((width * height) as usize);
    for px in rgba.chunks_exact(4) {
        let (r_u8, g_u8, b_u8) = match px {
            [r_u8, g_u8, b_u8, _alpha_u8] => (*r_u8, *g_u8, *b_u8),
            _ => continue,
        };
        let r = r_u8 as f32;
        let g = g_u8 as f32;
        let b = b_u8 as f32;
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        gray.push(y.clamp(0.0, 255.0) as u8);
    }
    gray
}
