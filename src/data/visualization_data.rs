use std::path::Path;
use std::sync::OnceLock;

use lopdf::{Dictionary, Document, Object};

use crate::dependency::file_store::FileStore;
use crate::dependency::pdf_annotator::PdfAnnotator;
use crate::types::file_types::{FontRunReport, FontTextRun, Rect as FontRect};
use crate::types::guess_types::GuessReport;
use crate::types::redaction_types::{Rect, RedactionKind, RedactionReport};
use crate::types::text_overlay::TextOverlay;
use crate::types::visualizer_config::VisualizerConfig;

const RASTER_TEXT_PADDING_PT: f32 = 1.0_f32;
const RASTER_MAX_FONT_TO_BOX_HEIGHT: f32 = 0.80_f32;
const RASTER_MIN_FONT_SIZE_PT: f32 = 4.5_f32;
const RASTER_BASELINE_ASCENT_RATIO: f32 = 0.75_f32;

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

#[derive(Debug, Clone, Copy)]
struct RasterOverlayLayout {
    x: f32,
    y: f32,
    font_size_pt: f32,
}

impl VisualizationData {
    #[inline]
    pub fn new() -> Self {
        Self {
            file_store: FileStore,
        }
    }

    #[inline]
    pub fn render_visualized_pdf(
        &self,
        pdf_path: &Path,
        report: &RedactionReport,
        guesses: Option<&GuessReport>,
        font_runs: Option<&FontRunReport>,
        cfg: VisualizerConfig,
    ) -> Result<Vec<u8>, String> {
        let inputs = self.load_inputs(pdf_path, report, guesses, font_runs)?;
        let annotator = PdfAnnotator;
        annotator.annotate(
            &inputs.pdf_bytes,
            &inputs.rects,
            &inputs.overlays,
            cfg.color,
            cfg.text_color,
            cfg.border_width,
        )
    }

    #[inline]
    pub fn render_and_write(
        &self,
        pdf_path: &Path,
        report: &RedactionReport,
        guesses: Option<&GuessReport>,
        font_runs: Option<&FontRunReport>,
        output_path: &Path,
        cfg: VisualizerConfig,
    ) -> Result<(), String> {
        let bytes = self.render_visualized_pdf(pdf_path, report, guesses, font_runs, cfg)?;
        self.write_output(output_path, &bytes)
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
        let width_map = build_font_width_map(&pdf_bytes)?;
        let mut rects = Vec::with_capacity(report.redactions.len());
        for r in &report.redactions {
            rects.push((r.page_index, r.bbox));
        }
        let overlays = build_overlays(report, guesses, font_runs, &width_map);
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
    width_map: &std::collections::BTreeMap<FontWidthKey, FontWidthTable>,
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
    let assets = font_runs
        .assets
        .iter()
        .map(|a| (a.font_key.clone(), a.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for idx in 0..max {
        let redaction = &report.redactions[idx];
        let guess = &guesses.guesses[idx];
        let selected = pick_best_guess(guess);
        let selected = match selected {
            Some(text) => text,
            None => continue,
        };
        if matches!(redaction.kind, RedactionKind::RasterDarkRegion) {
            let anchor_font = guess.context.anchor_font_key.as_ref().and_then(|font_key| {
                guess.context.anchor_font_size_pt.map(|font_size_pt| {
                    (
                        font_key.clone(),
                        font_size_pt,
                        guess.context.anchor_h_scale_pct.unwrap_or(100.0_f32),
                    )
                })
            });
            let nearby_run =
                select_run_by_bbox(&font_runs.runs, redaction.page_index, Some(redaction.bbox));
            let (font_key, requested_font_size_pt, h_scale_pct) = if let Some(font) = anchor_font {
                (font.0, font.1, font.2)
            } else if let Some(run) = nearby_run {
                (run.font_key.clone(), run.font_size_pt, run.h_scale_pct)
            } else {
                ("F1".to_owned(), 11.0_f32, 100.0_f32)
            };
            let box_height = redaction.bbox.height().abs().max(1.0_f32);
            let fitted_font_size_pt = requested_font_size_pt
                .min(box_height * RASTER_MAX_FONT_TO_BOX_HEIGHT)
                .max(RASTER_MIN_FONT_SIZE_PT);
            let guess_width = text_width_pt(
                redaction.page_index,
                &font_key,
                fitted_font_size_pt,
                h_scale_pct,
                selected.trim(),
                &assets,
                width_map,
            );
            let layout = raster_overlay_layout(redaction.bbox, fitted_font_size_pt, guess_width);
            out.push(TextOverlay {
                page_index: redaction.page_index,
                text: selected.trim().to_owned(),
                font_key,
                font_size_pt: layout.font_size_pt,
                h_scale_pct,
                x: layout.x,
                y: layout.y,
                bbox: redaction.bbox,
            });
            continue;
        }

        let left_hit = redaction.underlying_text.first();
        let right_hit = redaction.underlying_text.get(1);
        let context_left = guess.context.left_anchor_text.trim();
        let context_right = guess.context.right_anchor_text.trim();
        if context_left.is_empty() || context_right.is_empty() {
            continue;
        }

        if guess.context.has_anchor_pair {
            let anchor_left_x = guess.context.anchor_left_x;
            let anchor_right_x = guess.context.anchor_right_x;
            let anchor_font_key = guess.context.anchor_font_key.as_deref();
            let anchor_font_size = guess.context.anchor_font_size_pt;
            let anchor_h_scale = guess.context.anchor_h_scale_pct.unwrap_or(100.0_f32);
            let anchor_row_bias = guess.context.anchor_row_bias_pt.unwrap_or(0.0_f32);
            if let (Some(left_x), Some(font_key), Some(font_size_pt)) =
                (anchor_left_x, anchor_font_key, anchor_font_size)
            {
                if font_size_pt.is_finite() && font_size_pt > 0.0_f32 {
                    let left_width = text_width_pt(
                        redaction.page_index,
                        font_key,
                        font_size_pt,
                        anchor_h_scale,
                        context_left,
                        &assets,
                        width_map,
                    );
                    let space_width = text_width_pt(
                        redaction.page_index,
                        font_key,
                        font_size_pt,
                        anchor_h_scale,
                        " ",
                        &assets,
                        width_map,
                    );
                    let guess_width = text_width_pt(
                        redaction.page_index,
                        font_key,
                        font_size_pt,
                        anchor_h_scale,
                        selected.trim(),
                        &assets,
                        width_map,
                    );
                    let nominal_guess_x = left_x + left_width + space_width + anchor_row_bias;
                    let nominal_right_x = nominal_guess_x + guess_width + space_width;
                    let (guess_x, right_x) = match anchor_right_x {
                        Some(x) if x.is_finite() => {
                            let delta = x - nominal_right_x;
                            (nominal_guess_x + delta, x)
                        }
                        _ => (nominal_guess_x, nominal_right_x),
                    };

                    let left_bbox = left_hit.map(|hit| hit.bbox);
                    let right_bbox = right_hit.map(|hit| hit.bbox);
                    let anchor_run = select_run_by_text(
                        &font_runs.runs,
                        redaction.page_index,
                        context_left,
                        left_bbox,
                    )
                    .or_else(|| {
                        select_run_by_text(
                            &font_runs.runs,
                            redaction.page_index,
                            context_right,
                            right_bbox,
                        )
                    })
                    .or_else(|| {
                        select_run_by_bbox(&font_runs.runs, redaction.page_index, left_bbox)
                    })
                    .or_else(|| {
                        select_run_by_bbox(&font_runs.runs, redaction.page_index, right_bbox)
                    });
                    let y0 = anchor_run
                        .map(|run| run.bbox.y0)
                        .unwrap_or(redaction.bbox.y0);
                    let y1 = anchor_run
                        .map(|run| run.bbox.y1)
                        .unwrap_or(redaction.bbox.y1);

                    let overlay_bbox = Rect::new(
                        left_x.min(redaction.bbox.x0),
                        y0.min(redaction.bbox.y0),
                        right_x.max(redaction.bbox.x1),
                        y1.max(redaction.bbox.y1),
                    );
                    out.push(TextOverlay {
                        page_index: redaction.page_index,
                        text: context_left.to_owned(),
                        font_key: font_key.to_owned(),
                        font_size_pt,
                        h_scale_pct: anchor_h_scale,
                        x: left_x,
                        y: y1,
                        bbox: overlay_bbox,
                    });
                    out.push(TextOverlay {
                        page_index: redaction.page_index,
                        text: selected.trim().to_owned(),
                        font_key: font_key.to_owned(),
                        font_size_pt,
                        h_scale_pct: anchor_h_scale,
                        x: guess_x,
                        y: y1,
                        bbox: overlay_bbox,
                    });
                    out.push(TextOverlay {
                        page_index: redaction.page_index,
                        text: context_right.to_owned(),
                        font_key: font_key.to_owned(),
                        font_size_pt,
                        h_scale_pct: anchor_h_scale,
                        x: right_x,
                        y: y1,
                        bbox: overlay_bbox,
                    });
                    continue;
                }
            }
        }

        let left_text = left_hit.map(|h| h.text.trim());
        let right_text = right_hit.map(|h| h.text.trim());
        let left_text = match left_text {
            Some(text) if !text.is_empty() => text,
            _ => context_left,
        };
        let right_text = match right_text {
            Some(text) if !text.is_empty() => text,
            _ => context_right,
        };

        let left_bbox = left_hit.map(|h| h.bbox);
        let right_bbox = right_hit.map(|h| h.bbox);
        let left_run =
            select_run_by_text(&font_runs.runs, redaction.page_index, left_text, left_bbox)
                .or_else(|| select_run_by_bbox(&font_runs.runs, redaction.page_index, left_bbox));
        let right_run = select_run_by_text(
            &font_runs.runs,
            redaction.page_index,
            right_text,
            right_bbox,
        )
        .or_else(|| select_run_by_bbox(&font_runs.runs, redaction.page_index, right_bbox));
        let (left_run, right_run) = match (left_run, right_run) {
            (Some(l), Some(r)) => (l, r),
            _ => continue,
        };

        let left_font = &left_run.font_key;
        let right_font = &right_run.font_key;
        let left_size = left_run.font_size_pt;
        let right_size = right_run.font_size_pt;

        let left_width = text_width_pt(
            redaction.page_index,
            left_font,
            left_size,
            left_run.h_scale_pct,
            left_text,
            &assets,
            width_map,
        );
        let left_space = text_width_pt(
            redaction.page_index,
            left_font,
            left_size,
            left_run.h_scale_pct,
            " ",
            &assets,
            width_map,
        );
        let right_space = text_width_pt(
            redaction.page_index,
            right_font,
            right_size,
            right_run.h_scale_pct,
            " ",
            &assets,
            width_map,
        );

        let guess_font = if left_font == right_font && (left_size - right_size).abs() <= 0.01 {
            left_run
        } else {
            right_run
        };
        let guess_width = text_width_pt(
            redaction.page_index,
            &guess_font.font_key,
            guess_font.font_size_pt,
            guess_font.h_scale_pct,
            selected.trim(),
            &assets,
            width_map,
        );

        let left_x = left_run.bbox.x0;
        let guess_x = left_x + left_width + left_space;
        let right_x = guess_x + guess_width + right_space;

        let overlay_bbox = Rect::new(
            left_x.min(redaction.bbox.x0),
            left_run
                .bbox
                .y0
                .min(right_run.bbox.y0)
                .min(redaction.bbox.y0),
            right_x.max(redaction.bbox.x1),
            left_run
                .bbox
                .y1
                .max(right_run.bbox.y1)
                .max(redaction.bbox.y1),
        );

        out.push(TextOverlay {
            page_index: redaction.page_index,
            text: left_text.to_owned(),
            font_key: left_run.font_key.clone(),
            font_size_pt: left_size,
            h_scale_pct: left_run.h_scale_pct,
            x: left_x,
            y: left_run.bbox.y1,
            bbox: overlay_bbox,
        });
        out.push(TextOverlay {
            page_index: redaction.page_index,
            text: selected.trim().to_owned(),
            font_key: guess_font.font_key.clone(),
            font_size_pt: guess_font.font_size_pt,
            h_scale_pct: guess_font.h_scale_pct,
            x: guess_x,
            y: guess_font.bbox.y1,
            bbox: overlay_bbox,
        });
        out.push(TextOverlay {
            page_index: redaction.page_index,
            text: right_text.to_owned(),
            font_key: right_run.font_key.clone(),
            font_size_pt: right_size,
            h_scale_pct: right_run.h_scale_pct,
            x: right_x,
            y: right_run.bbox.y1,
            bbox: overlay_bbox,
        });
    }
    out
}

fn raster_overlay_layout(
    rect: Rect,
    requested_font_size_pt: f32,
    text_width_pt: f32,
) -> RasterOverlayLayout {
    let box_width = rect.width().abs().max(1.0_f32);
    let box_height = rect.height().abs().max(1.0_f32);
    let font_size_pt = requested_font_size_pt
        .min(box_height * RASTER_MAX_FONT_TO_BOX_HEIGHT)
        .max(RASTER_MIN_FONT_SIZE_PT);

    let x_min = rect.x0 + RASTER_TEXT_PADDING_PT;
    let x_max = (rect.x1 - RASTER_TEXT_PADDING_PT - text_width_pt).max(x_min);
    let centered_x = rect.x0 + ((box_width - text_width_pt) * 0.5_f32);
    let x = centered_x.clamp(x_min, x_max);

    let text_bottom = rect.y0 + ((box_height - font_size_pt) * 0.5_f32);
    let baseline = text_bottom + (font_size_pt * RASTER_BASELINE_ASCENT_RATIO);
    let y_min = rect.y0 + RASTER_TEXT_PADDING_PT;
    let y_max = rect.y1 - RASTER_TEXT_PADDING_PT;
    let y = baseline.clamp(y_min, y_max);

    RasterOverlayLayout { x, y, font_size_pt }
}

fn pick_best_guess(guess: &crate::types::guess_types::RedactionGuess) -> Option<&str> {
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct FontWidthKey {
    page_index: u32,
    font_key: String,
}

#[derive(Debug, Clone)]
struct FontWidthTable {
    first_char: u16,
    widths: Vec<f32>,
}

fn text_width_pt(
    page_index: u32,
    font_key: &str,
    font_size_pt: f32,
    h_scale_pct: f32,
    text: &str,
    assets: &std::collections::BTreeMap<String, crate::types::file_types::FontAsset>,
    width_map: &std::collections::BTreeMap<FontWidthKey, FontWidthTable>,
) -> f32 {
    let scale = (h_scale_pct / 100.0_f32).max(0.01_f32);
    if let Some(asset) = assets.get(font_key) {
        if let Some(width) = advance_pt(asset, text, font_size_pt) {
            if width.is_finite() && width > 0.0 {
                return width * scale;
            }
        }
    }
    let key = FontWidthKey {
        page_index,
        font_key: font_key.to_owned(),
    };
    if let Some(table) = width_map.get(&key) {
        if let Some(width) = width_from_table(table, text, font_size_pt) {
            if width.is_finite() && width > 0.0 {
                return width * scale;
            }
        }
    }
    fallback_width_pt(text, font_size_pt) * scale
}

fn advance_pt(
    asset: &crate::types::file_types::FontAsset,
    text: &str,
    font_size_pt: f32,
) -> Option<f32> {
    let face = rustybuzz::Face::from_slice(&asset.bytes, 0)?;
    let units_per_em = asset.units_per_em.max(1) as f32;
    let mut buf = rustybuzz::UnicodeBuffer::new();
    buf.push_str(text);
    let out = rustybuzz::shape(&face, shaping_features(), buf);
    let units = out
        .glyph_positions()
        .iter()
        .map(|p| p.x_advance as f32)
        .sum::<f32>()
        / 64.0;
    Some(units * (font_size_pt / units_per_em))
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

fn width_from_table(table: &FontWidthTable, text: &str, font_size_pt: f32) -> Option<f32> {
    let mut sum = 0.0_f32;
    let mut any = false;
    for ch in text.chars() {
        let code = ch as u32;
        if code > u16::MAX as u32 {
            continue;
        }
        let code = code as u16;
        if code < table.first_char {
            continue;
        }
        let idx = (code - table.first_char) as usize;
        if idx >= table.widths.len() {
            continue;
        }
        sum += table.widths[idx] * (font_size_pt / 1000.0);
        any = true;
    }
    any.then_some(sum)
}

fn fallback_width_pt(text: &str, font_size_pt: f32) -> f32 {
    let count = text.chars().count().max(1) as f32;
    font_size_pt.abs().max(1.0) * 0.6 * count
}

fn build_font_width_map(
    pdf_bytes: &[u8],
) -> Result<std::collections::BTreeMap<FontWidthKey, FontWidthTable>, String> {
    let doc = Document::load_mem(pdf_bytes).map_err(|e| e.to_string())?;
    let pages = doc.get_pages();
    let mut map = std::collections::BTreeMap::new();

    for (page_no, page_id) in pages {
        let page_index = page_no.saturating_sub(1);
        let (res_opt, _unused_pages) =
            doc.get_page_resources(page_id).map_err(|e| e.to_string())?;
        let resources = match res_opt {
            Some(r) => r,
            None => continue,
        };
        let font_obj = match resources.get(b"Font").ok() {
            Some(o) => o,
            None => continue,
        };
        let font_dict = match deref_to_dict(&doc, font_obj).or_else(|| object_to_dict(font_obj)) {
            Some(d) => d,
            None => continue,
        };
        for (key_bytes, value_object) in font_dict.iter() {
            let key = String::from_utf8_lossy(key_bytes).to_string();
            let dict =
                match deref_to_dict(&doc, value_object).or_else(|| object_to_dict(value_object)) {
                    Some(d) => d,
                    None => continue,
                };
            let target = resolve_width_dict(&doc, dict);
            let target = match target {
                Some(d) => d,
                None => continue,
            };
            let first_char = target.get(b"FirstChar").ok().and_then(object_to_u16);
            let widths = target.get(b"Widths").ok().and_then(object_to_f32_array);
            let (first_char, widths) = match (first_char, widths) {
                (Some(f), Some(w)) if !w.is_empty() => (f, w),
                _ => continue,
            };
            let table = FontWidthTable { first_char, widths };
            map.insert(
                FontWidthKey {
                    page_index,
                    font_key: key,
                },
                table,
            );
        }
    }

    Ok(map)
}

fn resolve_width_dict<'a>(doc: &'a Document, dict: &'a Dictionary) -> Option<&'a Dictionary> {
    if dict.has(b"Widths") {
        return Some(dict);
    }
    let subtype = dict.get(b"Subtype").ok().and_then(object_to_name_string);
    if subtype.as_deref() == Some("Type0") {
        let descendants = dict.get(b"DescendantFonts").ok().and_then(object_to_array);
        let first = descendants
            .and_then(|arr| arr.first())
            .and_then(|obj| deref_to_dict(doc, obj));
        if let Some(desc) = first {
            if desc.has(b"Widths") {
                return Some(desc);
            }
        }
    }
    None
}

fn object_to_u16(object: &Object) -> Option<u16> {
    match object {
        Object::Integer(v) => (*v).try_into().ok(),
        Object::Real(v) => (*v as i64).try_into().ok(),
        _ => None,
    }
}

fn object_to_f32(object: &Object) -> Option<f32> {
    match object {
        Object::Real(real_value) => Some(*real_value),
        Object::Integer(integer_value) => Some(*integer_value as f32),
        _ => None,
    }
}

fn object_to_f32_array(object: &Object) -> Option<Vec<f32>> {
    match object {
        Object::Array(values) => {
            let mut out = Vec::with_capacity(values.len());
            for item in values {
                if let Some(v) = object_to_f32(item) {
                    out.push(v);
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn object_to_array(object: &Object) -> Option<&Vec<Object>> {
    match object {
        Object::Array(values) => Some(values),
        _ => None,
    }
}

fn object_to_name_string(object: &Object) -> Option<String> {
    match object {
        Object::Name(name_bytes) => Some(String::from_utf8_lossy(name_bytes).to_string()),
        _ => None,
    }
}

fn object_to_dict(object: &Object) -> Option<&Dictionary> {
    match object {
        Object::Dictionary(dictionary) => Some(dictionary),
        _ => None,
    }
}

fn deref_to_dict<'doc>(doc: &'doc Document, object: &'doc Object) -> Option<&'doc Dictionary> {
    match object {
        Object::Reference(object_id) => match doc.get_object(*object_id).ok()? {
            Object::Dictionary(d) => Some(d),
            _ => None,
        },
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_overlay_layout_keeps_baseline_inside_box() {
        let bbox = Rect::new(120.0, 300.0, 190.0, 314.0);
        let layout = raster_overlay_layout(bbox, 12.0, 40.0);
        assert!(layout.y >= bbox.y0);
        assert!(layout.y <= bbox.y1);
        assert!(layout.font_size_pt <= bbox.height().abs() * RASTER_MAX_FONT_TO_BOX_HEIGHT + 0.01);
    }

    #[test]
    fn raster_overlay_layout_clamps_x_when_text_is_wide() {
        let bbox = Rect::new(50.0, 100.0, 90.0, 112.0);
        let layout = raster_overlay_layout(bbox, 11.0, 120.0);
        assert!(layout.x >= bbox.x0 + RASTER_TEXT_PADDING_PT);
        assert!(layout.x <= bbox.x1);
    }
}
