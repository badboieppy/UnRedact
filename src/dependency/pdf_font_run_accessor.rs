use crate::types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect};
use crate::types::runtime_defaults::{DEFAULT_FONT_METRICS_DPI, GLYPH_UNITS_SCALE};
use crate::types::typography_shaping::shaping_features;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
struct FontInfo {
    font_key: String,
    font_name: String,
    bytes: Option<Vec<u8>>,
    units_per_em: Option<u16>,
}

#[derive(Debug, Clone)]
struct TextState {
    in_text: bool,
    font_key: String,
    font_name: String,
    font_size_pt: f32,
    h_scale_pct: f32,
    char_spacing: f32,
    word_spacing: f32,
    leading: f32,
    text_matrix: Matrix,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            in_text: false,
            font_key: String::new(),
            font_name: String::new(),
            font_size_pt: 0.0,
            h_scale_pct: 100.0_f32,
            char_spacing: 0.0_f32,
            word_spacing: 0.0_f32,
            leading: 0.0_f32,
            text_matrix: Matrix::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Matrix {
    tx: f32,
    ty: f32,
}

impl Default for Matrix {
    fn default() -> Self {
        Self { tx: 0.0, ty: 0.0 }
    }
}

#[derive(Debug, Clone)]
struct TextMetrics {
    width_pt: f32,
    width_px: f32,
    char_advances_pt: Vec<f32>,
    char_advances_px: Vec<f32>,
}

#[derive(Debug, Clone)]
struct ShowTextOp {
    text: String,
    per_char_adjustments_pt: Vec<(usize, f32)>,
}

#[inline]
pub fn build_font_run_report_from_input_name(
    input_name: &str,
    bytes: &[u8],
) -> Result<FontRunReport, String> {
    let doc = Document::load_mem(bytes).map_err(|e| e.to_string())?;
    let pages = doc.get_pages();

    let mut runs = Vec::new();
    let mut assets_map: BTreeMap<String, FontAsset> = BTreeMap::new();

    for (page_no, page_id) in pages {
        let page_index = page_no.saturating_sub(1);
        let font_map = extract_page_font_info(&doc, page_id)?;
        for info in font_map.values() {
            if let Some(bytes) = &info.bytes {
                let units_per_em = info.units_per_em.unwrap_or(1000);
                assets_map
                    .entry(info.font_key.clone())
                    .or_insert(FontAsset {
                        font_key: info.font_key.clone(),
                        font_name: info.font_name.clone(),
                        units_per_em,
                        bytes: bytes.clone(),
                    });
            }
        }
        let content = doc
            .get_page_content(page_id)
            .map_err(|e| format!("page_content_error={e}"))?;
        let decoded = lopdf::content::Content::decode(&content)
            .map_err(|e| format!("page_decode_error={e}"))?;
        runs.extend(extract_text_runs(
            page_index,
            &decoded.operations,
            &font_map,
        ));
    }

    Ok(FontRunReport {
        input: input_name.to_owned(),
        runs,
        assets: assets_map.into_values().collect(),
    })
}

fn extract_text_runs(
    page_index: u32,
    ops: &[lopdf::content::Operation],
    font_map: &BTreeMap<String, FontInfo>,
) -> Vec<FontTextRun> {
    let mut out = Vec::new();
    let mut st = TextState::default();

    for op in ops {
        match op.operator.as_str() {
            "BT" => st.in_text = true,
            "ET" => st.in_text = false,
            "Tf" => {
                let font_key = op
                    .operands
                    .first()
                    .and_then(object_to_name_string)
                    .unwrap_or_else(|| st.font_key.clone());
                let size = op
                    .operands
                    .get(1)
                    .and_then(object_to_f32)
                    .unwrap_or(st.font_size_pt);
                let info = font_map.get(&font_key);
                st.font_key = font_key.clone();
                st.font_name = info
                    .map(|i| i.font_name.clone())
                    .unwrap_or_else(|| font_key);
                st.font_size_pt = size;
            }
            "Tz" => {
                st.h_scale_pct = op
                    .operands
                    .first()
                    .and_then(object_to_f32)
                    .unwrap_or(st.h_scale_pct);
            }
            "Tc" => {
                st.char_spacing = op
                    .operands
                    .first()
                    .and_then(object_to_f32)
                    .unwrap_or(st.char_spacing);
            }
            "Tw" => {
                st.word_spacing = op
                    .operands
                    .first()
                    .and_then(object_to_f32)
                    .unwrap_or(st.word_spacing);
            }
            "TL" => {
                st.leading = op
                    .operands
                    .first()
                    .and_then(object_to_f32)
                    .unwrap_or(st.leading);
            }
            "Tm" => {
                if let Some(tx) = op.operands.get(4).and_then(object_to_f32) {
                    st.text_matrix.tx = tx;
                }
                if let Some(ty) = op.operands.get(5).and_then(object_to_f32) {
                    st.text_matrix.ty = ty;
                }
            }
            "Td" => {
                let dx = op.operands.first().and_then(object_to_f32).unwrap_or(0.0);
                let dy = op.operands.get(1).and_then(object_to_f32).unwrap_or(0.0);
                st.text_matrix.tx += dx;
                st.text_matrix.ty += dy;
            }
            "TD" => {
                let dx = op.operands.first().and_then(object_to_f32).unwrap_or(0.0);
                let dy = op.operands.get(1).and_then(object_to_f32).unwrap_or(0.0);
                st.text_matrix.tx += dx;
                st.text_matrix.ty += dy;
                // PDF spec: TD also sets leading to -dy.
                st.leading = -dy;
            }
            "T*" => {
                st.text_matrix.ty += next_line_delta_y(st.leading, st.font_size_pt);
            }
            "'" => {
                if !st.in_text {
                    continue;
                }
                st.text_matrix.ty += next_line_delta_y(st.leading, st.font_size_pt);
                let Some(show) = parse_show_text_op(op, &st) else {
                    continue;
                };
                if show.text.trim().is_empty() {
                    continue;
                }

                let font_info = font_map.get(&st.font_key);
                let mut metrics = measure_run_metrics(
                    font_info,
                    &st.font_name,
                    st.font_size_pt,
                    st.h_scale_pct,
                    &show.text,
                )
                .unwrap_or_else(|| {
                    fallback_text_metrics(
                        &show.text,
                        st.font_size_pt.abs().max(1.0_f32),
                        st.h_scale_pct,
                        DEFAULT_FONT_METRICS_DPI,
                    )
                });
                apply_pdf_spacing_adjustments(&mut metrics, &show, &st);
                let width = metrics.width_pt.max(0.0_f32);
                let h = st.font_size_pt.abs().max(1.0);
                let x0 = st.text_matrix.tx;
                let y1 = st.text_matrix.ty;
                let bbox = Rect::new(x0, y1 - h, x0 + width, y1);
                out.push(FontTextRun {
                    page_index,
                    text: show.text,
                    bbox,
                    font_key: st.font_key.clone(),
                    font_name: st.font_name.clone(),
                    font_size_pt: st.font_size_pt,
                    h_scale_pct: st.h_scale_pct.max(1.0_f32),
                    measured_width_pt: Some(metrics.width_pt),
                    measured_width_px: Some(metrics.width_px),
                    measured_dpi: Some(DEFAULT_FONT_METRICS_DPI),
                    char_advances_pt: metrics.char_advances_pt,
                    char_advances_px: metrics.char_advances_px,
                });
                st.text_matrix.tx += width;
            }
            "\"" => {
                if !st.in_text {
                    continue;
                }
                if let Some(value) = op.operands.first().and_then(object_to_f32) {
                    st.word_spacing = value;
                }
                if let Some(value) = op.operands.get(1).and_then(object_to_f32) {
                    st.char_spacing = value;
                }
                st.text_matrix.ty += next_line_delta_y(st.leading, st.font_size_pt);
                let Some(show) = parse_show_text_op(op, &st) else {
                    continue;
                };
                if show.text.trim().is_empty() {
                    continue;
                }

                let font_info = font_map.get(&st.font_key);
                let mut metrics = measure_run_metrics(
                    font_info,
                    &st.font_name,
                    st.font_size_pt,
                    st.h_scale_pct,
                    &show.text,
                )
                .unwrap_or_else(|| {
                    fallback_text_metrics(
                        &show.text,
                        st.font_size_pt.abs().max(1.0_f32),
                        st.h_scale_pct,
                        DEFAULT_FONT_METRICS_DPI,
                    )
                });
                apply_pdf_spacing_adjustments(&mut metrics, &show, &st);
                let width = metrics.width_pt.max(0.0_f32);
                let h = st.font_size_pt.abs().max(1.0);
                let x0 = st.text_matrix.tx;
                let y1 = st.text_matrix.ty;
                let bbox = Rect::new(x0, y1 - h, x0 + width, y1);
                out.push(FontTextRun {
                    page_index,
                    text: show.text,
                    bbox,
                    font_key: st.font_key.clone(),
                    font_name: st.font_name.clone(),
                    font_size_pt: st.font_size_pt,
                    h_scale_pct: st.h_scale_pct.max(1.0_f32),
                    measured_width_pt: Some(metrics.width_pt),
                    measured_width_px: Some(metrics.width_px),
                    measured_dpi: Some(DEFAULT_FONT_METRICS_DPI),
                    char_advances_pt: metrics.char_advances_pt,
                    char_advances_px: metrics.char_advances_px,
                });
                st.text_matrix.tx += width;
            }
            "Tj" | "TJ" => {
                if !st.in_text {
                    continue;
                }
                let Some(show) = parse_show_text_op(op, &st) else {
                    continue;
                };
                if show.text.trim().is_empty() {
                    continue;
                }

                let font_info = font_map.get(&st.font_key);
                let mut metrics = measure_run_metrics(
                    font_info,
                    &st.font_name,
                    st.font_size_pt,
                    st.h_scale_pct,
                    &show.text,
                )
                .unwrap_or_else(|| {
                    fallback_text_metrics(
                        &show.text,
                        st.font_size_pt.abs().max(1.0_f32),
                        st.h_scale_pct,
                        DEFAULT_FONT_METRICS_DPI,
                    )
                });
                apply_pdf_spacing_adjustments(&mut metrics, &show, &st);
                let width = metrics.width_pt.max(0.0_f32);
                let h = st.font_size_pt.abs().max(1.0);
                let x0 = st.text_matrix.tx;
                let y1 = st.text_matrix.ty;
                let bbox = Rect::new(x0, y1 - h, x0 + width, y1);
                out.push(FontTextRun {
                    page_index,
                    text: show.text,
                    bbox,
                    font_key: st.font_key.clone(),
                    font_name: st.font_name.clone(),
                    font_size_pt: st.font_size_pt,
                    h_scale_pct: st.h_scale_pct.max(1.0_f32),
                    measured_width_pt: Some(metrics.width_pt),
                    measured_width_px: Some(metrics.width_px),
                    measured_dpi: Some(DEFAULT_FONT_METRICS_DPI),
                    char_advances_pt: metrics.char_advances_pt,
                    char_advances_px: metrics.char_advances_px,
                });
                st.text_matrix.tx += width;
            }
            _ => {}
        }
    }

    out
}

fn extract_page_font_info(
    doc: &Document,
    page_id: ObjectId,
) -> Result<BTreeMap<String, FontInfo>, String> {
    let (page_resources_opt, _unused_pages) = doc
        .get_page_resources(page_id)
        .map_err(|error| error.to_string())?;
    let resources = match page_resources_opt {
        None => return Ok(BTreeMap::new()),
        Some(resources_dict) => resources_dict,
    };

    let font_obj_opt = resources.get(b"Font").ok();
    let font_obj = match font_obj_opt {
        None => return Ok(BTreeMap::new()),
        Some(font_object) => font_object,
    };

    let font_dict_opt = deref_to_dict(doc, font_obj).or_else(|| object_to_dict(font_obj));
    let font_dict = match font_dict_opt {
        None => return Ok(BTreeMap::new()),
        Some(font_dictionary) => font_dictionary,
    };

    let map = font_dict
        .iter()
        .filter_map(|(key_bytes, value_object)| {
            let key = String::from_utf8_lossy(key_bytes).to_string();
            let dict = deref_to_dict(doc, value_object)?;
            let font_name = resolve_pdf_font_name(doc, dict).unwrap_or_else(|| key.clone());
            let (bytes, units_per_em) = extract_font_bytes(doc, dict);
            Some((
                key.clone(),
                FontInfo {
                    font_key: key,
                    font_name,
                    bytes,
                    units_per_em,
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();

    Ok(map)
}

fn resolve_pdf_font_name(doc: &Document, dict: &Dictionary) -> Option<String> {
    let base = dict.get(b"BaseFont").ok().and_then(object_to_name_string);
    if let Some(base_font_name) = base {
        return Some(normalize_subset_font_name(&base_font_name));
    }

    let desc_obj = dict.get(b"FontDescriptor").ok();
    let desc_dict_opt =
        desc_obj.and_then(|descriptor_object| deref_to_dict(doc, descriptor_object));
    let desc_dict = desc_dict_opt?;
    let name = desc_dict
        .get(b"FontName")
        .ok()
        .and_then(object_to_name_string)?;
    Some(normalize_subset_font_name(&name))
}

fn extract_font_bytes(doc: &Document, dict: &Dictionary) -> (Option<Vec<u8>>, Option<u16>) {
    let desc_obj = match dict.get(b"FontDescriptor").ok() {
        Some(o) => o,
        None => return (None, None),
    };
    let desc = match deref_to_dict(doc, desc_obj) {
        Some(d) => d,
        None => return (None, None),
    };

    for key in [&b"FontFile"[..], &b"FontFile2"[..], &b"FontFile3"[..]] {
        if let Some(stream) = desc.get(key).ok().and_then(|o| deref_to_stream(doc, o)) {
            let bytes = stream.decompressed_content().ok();
            if let Some(b) = bytes {
                let units = ttf_parser::Face::parse(&b, 0)
                    .ok()
                    .map(|f| f.units_per_em());
                return (Some(b), units);
            }
        }
    }

    (None, None)
}

fn parse_show_text_op(op: &lopdf::content::Operation, st: &TextState) -> Option<ShowTextOp> {
    if op.operator.as_str() == "TJ" {
        let items = match op.operands.first() {
            Some(Object::Array(values)) => values,
            _ => return None,
        };
        let mut text = String::new();
        let mut per_char_adjustments_pt = Vec::<(usize, f32)>::new();
        let mut total_chars = 0_usize;
        for item in items {
            if let Some(value) = decode_pdf_text(item) {
                if value.is_empty() {
                    continue;
                }
                total_chars += value.chars().count();
                text.push_str(&value);
                continue;
            }
            if let Some(value) = object_to_f32(item) {
                if total_chars == 0 {
                    continue;
                }
                let adj = tj_adjustment_pt(value, st.font_size_pt, st.h_scale_pct);
                if adj.is_finite() && adj.abs() > f32::EPSILON {
                    per_char_adjustments_pt.push((total_chars - 1, adj));
                }
            }
        }
        return Some(ShowTextOp {
            text,
            per_char_adjustments_pt,
        });
    }

    op.operands
        .last()
        .and_then(decode_pdf_text)
        .map(|text| ShowTextOp {
            text,
            per_char_adjustments_pt: Vec::new(),
        })
}

fn next_line_delta_y(leading: f32, font_size_pt: f32) -> f32 {
    if leading.is_finite() && leading.abs() > 0.01_f32 {
        -leading
    } else {
        -font_size_pt.abs().max(1.0_f32) * 1.2_f32
    }
}

fn pdf_spacing_pt(value: f32, font_size_pt: f32, h_scale_pct: f32) -> f32 {
    let font_size = font_size_pt.abs().max(1.0_f32);
    let scale = (h_scale_pct / 100.0_f32).max(0.01_f32);
    value * (font_size / 1000.0_f32) * scale
}

fn tj_adjustment_pt(value: f32, font_size_pt: f32, h_scale_pct: f32) -> f32 {
    -pdf_spacing_pt(value, font_size_pt, h_scale_pct)
}

fn apply_pdf_spacing_adjustments(metrics: &mut TextMetrics, show: &ShowTextOp, st: &TextState) {
    if show.text.is_empty() || metrics.char_advances_pt.is_empty() {
        return;
    }
    let chars = show.text.chars().collect::<Vec<_>>();
    let inter_char_spacing = pdf_spacing_pt(st.char_spacing, st.font_size_pt, st.h_scale_pct);
    let inter_word_spacing = pdf_spacing_pt(st.word_spacing, st.font_size_pt, st.h_scale_pct);
    let mut width_delta = 0.0_f32;

    let last_gap_index = chars.len().saturating_sub(1);
    for (idx, ch) in chars.iter().enumerate().take(last_gap_index) {
        let mut delta = inter_char_spacing;
        if ch.is_whitespace() {
            delta += inter_word_spacing;
        }
        if delta.abs() <= f32::EPSILON {
            continue;
        }
        if idx < metrics.char_advances_pt.len() {
            metrics.char_advances_pt[idx] += delta;
        }
        width_delta += delta;
    }

    for (char_idx, delta) in &show.per_char_adjustments_pt {
        if !delta.is_finite() || delta.abs() <= f32::EPSILON {
            continue;
        }
        if *char_idx < metrics.char_advances_pt.len() {
            metrics.char_advances_pt[*char_idx] += *delta;
            width_delta += *delta;
        }
    }

    metrics.width_pt += width_delta;
    if !metrics.width_pt.is_finite() || metrics.width_pt <= 0.0_f32 {
        metrics.width_pt = 0.001_f32;
    }
    metrics.width_px = points_to_pixels(metrics.width_pt, DEFAULT_FONT_METRICS_DPI);
    metrics.char_advances_pt = normalize_char_advances(
        &show.text,
        metrics.char_advances_pt.clone(),
        metrics.width_pt,
    );
    metrics.char_advances_px = metrics
        .char_advances_pt
        .iter()
        .map(|value| points_to_pixels(*value, DEFAULT_FONT_METRICS_DPI))
        .collect::<Vec<_>>();
}

fn measure_run_metrics(
    font_info: Option<&FontInfo>,
    font_name: &str,
    font_size_pt: f32,
    h_scale_pct: f32,
    text: &str,
) -> Option<TextMetrics> {
    if text.is_empty() {
        return Some(TextMetrics {
            width_pt: 0.0_f32,
            width_px: 0.0_f32,
            char_advances_pt: Vec::new(),
            char_advances_px: Vec::new(),
        });
    }
    let font_size = font_size_pt.abs().max(1.0_f32);
    let scale = (h_scale_pct / 100.0_f32).max(0.01_f32);

    if let Some(info) = font_info {
        if let Some(bytes) = info.bytes.as_ref() {
            if let Some(face) = rustybuzz::Face::from_slice(bytes, 0) {
                let units_per_em = info.units_per_em.unwrap_or(1000).max(1) as f32;
                if let Some(metrics) = shape_text_metrics(
                    &face,
                    text,
                    font_size,
                    units_per_em,
                    scale,
                    DEFAULT_FONT_METRICS_DPI,
                ) {
                    return Some(metrics);
                }
            }
        }
    }

    core_font_metrics(font_name, text, font_size, scale, DEFAULT_FONT_METRICS_DPI)
}

fn shape_text_metrics(
    face: &rustybuzz::Face<'_>,
    text: &str,
    font_size_pt: f32,
    units_per_em: f32,
    scale: f32,
    dpi: f32,
) -> Option<TextMetrics> {
    let shaped = shape_text(face, text);
    let glyph_positions = shaped.glyph_positions();
    if glyph_positions.is_empty() {
        return None;
    }
    let units = glyph_positions
        .iter()
        .map(|position| position.x_advance as f32)
        .sum::<f32>()
        / GLYPH_UNITS_SCALE as f32;
    let width_pt = units * (font_size_pt / units_per_em.max(1.0_f32)) * scale;
    if !width_pt.is_finite() || width_pt <= 0.0_f32 {
        return None;
    }

    let char_units = contextual_char_advances_units(text, &shaped);
    let mut char_advances_pt = char_units
        .iter()
        .map(|units_value| units_value * (font_size_pt / units_per_em.max(1.0_f32)) * scale)
        .collect::<Vec<_>>();
    char_advances_pt = normalize_char_advances(text, char_advances_pt, width_pt);
    let char_advances_px = char_advances_pt
        .iter()
        .map(|value| points_to_pixels(*value, dpi))
        .collect::<Vec<_>>();
    Some(TextMetrics {
        width_pt,
        width_px: points_to_pixels(width_pt, dpi),
        char_advances_pt,
        char_advances_px,
    })
}

fn shape_text(face: &rustybuzz::Face<'_>, text: &str) -> rustybuzz::GlyphBuffer {
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    rustybuzz::shape(face, shaping_features(), buffer)
}

fn core_font_metrics(
    font_name: &str,
    text: &str,
    font_size_pt: f32,
    scale: f32,
    dpi: f32,
) -> Option<TextMetrics> {
    let normalized = font_name.to_ascii_lowercase();
    let table: fn(char) -> i32 = if normalized.contains("times") && normalized.contains("roman") {
        times_roman_width as fn(char) -> i32
    } else if normalized.contains("helvetica") {
        helvetica_width as fn(char) -> i32
    } else {
        return None;
    };

    let mut char_advances_pt = Vec::with_capacity(text.chars().count());
    for ch in text.chars() {
        let unit_width = table(ch) as f32;
        char_advances_pt.push(unit_width * (font_size_pt / 1000.0_f32) * scale);
    }
    let width_pt = char_advances_pt.iter().sum::<f32>();
    if !width_pt.is_finite() || width_pt <= 0.0_f32 {
        return None;
    }
    let char_advances_px = char_advances_pt
        .iter()
        .map(|value| points_to_pixels(*value, dpi))
        .collect::<Vec<_>>();
    Some(TextMetrics {
        width_pt,
        width_px: points_to_pixels(width_pt, dpi),
        char_advances_pt,
        char_advances_px,
    })
}

fn contextual_char_advances_units(text: &str, shaped: &rustybuzz::GlyphBuffer) -> Vec<f32> {
    let char_starts = text.char_indices().map(|(idx, _)| idx).collect::<Vec<_>>();
    if char_starts.is_empty() {
        return Vec::new();
    }
    let mut cluster_advances = BTreeMap::<usize, f32>::new();
    for (info, position) in shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions().iter())
    {
        let cluster = (info.cluster as usize).min(text.len());
        let entry = cluster_advances.entry(cluster).or_insert(0.0_f32);
        *entry += (position.x_advance as f32) / GLYPH_UNITS_SCALE as f32;
    }
    if cluster_advances.is_empty() {
        return vec![0.0_f32; char_starts.len()];
    }
    let mut clusters = cluster_advances.into_iter().collect::<Vec<_>>();
    clusters.sort_by_key(|(cluster, _)| *cluster);
    let mut per_char = vec![0.0_f32; char_starts.len()];
    for (index, (start_byte, advance_units)) in clusters.iter().enumerate() {
        let end_byte = clusters
            .get(index + 1)
            .map(|(cluster, _)| *cluster)
            .unwrap_or(text.len())
            .min(text.len());
        let start_char = start_char_index(&char_starts, *start_byte);
        let mut end_char = end_char_index(&char_starts, end_byte);
        if end_char <= start_char {
            end_char = (start_char + 1).min(char_starts.len());
        }
        let count = end_char.saturating_sub(start_char).max(1);
        let per_value = *advance_units / count as f32;
        for idx in start_char..(start_char + count).min(per_char.len()) {
            per_char[idx] += per_value;
        }
    }
    per_char
}

fn start_char_index(char_starts: &[usize], byte_offset: usize) -> usize {
    match char_starts.binary_search(&byte_offset) {
        Ok(idx) => idx,
        Err(idx) => idx
            .saturating_sub(1)
            .min(char_starts.len().saturating_sub(1)),
    }
}

fn end_char_index(char_starts: &[usize], byte_offset: usize) -> usize {
    match char_starts.binary_search(&byte_offset) {
        Ok(idx) => idx,
        Err(idx) => idx.min(char_starts.len()),
    }
}

fn normalize_char_advances(text: &str, advances: Vec<f32>, width_pt: f32) -> Vec<f32> {
    let char_count = text.chars().count();
    if char_count == 0 {
        return Vec::new();
    }
    if advances.len() != char_count
        || advances
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0_f32)
    {
        let per_char = width_pt / char_count as f32;
        return vec![per_char; char_count];
    }
    let sum = advances.iter().sum::<f32>();
    if !sum.is_finite() || sum <= 0.0_f32 || !width_pt.is_finite() {
        let per_char = width_pt / char_count as f32;
        return vec![per_char; char_count];
    }
    let factor = width_pt / sum;
    advances
        .into_iter()
        .map(|value| value * factor)
        .collect::<Vec<_>>()
}

fn fallback_text_metrics(text: &str, font_size_pt: f32, h_scale_pct: f32, dpi: f32) -> TextMetrics {
    let scale = (h_scale_pct / 100.0_f32).max(0.01_f32);
    let chars = text.chars().count().max(1);
    let width_pt = (font_size_pt.abs().max(1.0_f32) * 0.6_f32 * chars as f32 * scale).max(1.0_f32);
    let per_char = width_pt / chars as f32;
    let char_advances_pt = vec![per_char; chars];
    let char_advances_px = char_advances_pt
        .iter()
        .map(|value| points_to_pixels(*value, dpi))
        .collect::<Vec<_>>();
    TextMetrics {
        width_pt,
        width_px: points_to_pixels(width_pt, dpi),
        char_advances_pt,
        char_advances_px,
    }
}

fn points_to_pixels(points: f32, dpi: f32) -> f32 {
    points * (dpi / 72.0_f32)
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

fn decode_pdf_text(obj: &Object) -> Option<String> {
    match obj {
        Object::String(raw_bytes, _) => {
            let decoded = lopdf::decode_text_string(obj)
                .ok()
                .filter(|text| !text.contains('\u{FFFD}'));
            match decoded {
                Some(text) => Some(text),
                None => Some(String::from_utf8_lossy(raw_bytes).to_string()),
            }
        }
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

fn deref_to_stream<'doc>(doc: &'doc Document, object: &'doc Object) -> Option<&'doc Stream> {
    match object {
        Object::Reference(object_id) => match doc.get_object(*object_id).ok()? {
            Object::Stream(s) => Some(s),
            _ => None,
        },
        Object::Stream(s) => Some(s),
        _ => None,
    }
}

#[inline]
fn normalize_subset_font_name(raw: &str) -> String {
    let parts = raw.split('+').collect::<Vec<_>>();
    normalize_from_parts(&parts)
}

fn normalize_from_parts(parts: &[&str]) -> String {
    let is_subset = is_subset_prefix(parts);
    if !is_subset {
        return parts.join("+");
    }

    let second = parts.get(1).copied().unwrap_or("");
    if second.is_empty() {
        return parts.join("+");
    }

    second.to_owned()
}

fn is_subset_prefix(parts: &[&str]) -> bool {
    let has_two = parts.len() == 2;
    if !has_two {
        return false;
    }

    let prefix = parts[0];
    let len_ok = prefix.len() == 6;
    if !len_ok {
        return false;
    }

    prefix.chars().all(|c| c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::build_font_run_report_from_input_name;
    use std::path::Path;

    #[test]
    fn build_font_run_report_is_deterministic_for_same_input() {
        let input = Path::new("test_data/EFTA00101126.pdf");
        let bytes = std::fs::read(input)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", input.display()));

        let input_name = input.to_string_lossy().to_string();
        let report_a = build_font_run_report_from_input_name(&input_name, &bytes)
            .expect("font run report should parse input");
        let report_b = build_font_run_report_from_input_name(&input_name, &bytes)
            .expect("font run report should parse input");

        assert_eq!(report_a, report_b);
    }

    #[test]
    fn build_font_run_report_extracts_runs_with_metrics() {
        let input = Path::new("test_data/EFTA00101126.pdf");
        let bytes = std::fs::read(input)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", input.display()));

        let input_name = input.to_string_lossy().to_string();
        let report = build_font_run_report_from_input_name(&input_name, &bytes)
            .expect("font run report should parse input");
        assert!(!report.runs.is_empty(), "expected non-empty text runs");

        let measured = report
            .runs
            .iter()
            .filter(|run| run.measured_width_pt.is_some() && run.measured_width_px.is_some())
            .count();
        assert!(measured > 0, "expected measured widths on at least one run");
    }

    #[test]
    fn build_font_run_report_fails_on_invalid_pdf_bytes() {
        let err = build_font_run_report_from_input_name("invalid.pdf", b"not a pdf")
            .expect_err("invalid bytes should fail");
        assert!(!err.is_empty());
    }
}
