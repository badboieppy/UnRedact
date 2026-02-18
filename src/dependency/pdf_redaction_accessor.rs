use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use crate::types::redaction_types::{
    PdfRenderer, Rect, RedactionFinderConfig, RedactionKind, RedactionOccurrence, UnderlyingTextHit,
};

const OCR_WINDOW_MIN_WIDTH_PT: f32 = 64.0;
const OCR_WINDOW_MAX_WIDTH_PT: f32 = 220.0;
const OCR_WINDOW_PADDING_PT: f32 = 2.0;
const MAX_RASTER_ANALYSIS_DPI: f32 = 120.0;
const OCR_ENABLE_ENV: &str = "UNREDACT_ENABLE_LOCAL_OCR";
const OCR_CMD_ENV: &str = "UNREDACT_TESSERACT_CMD";
static OCR_TEMP_SEQ: AtomicU64 = AtomicU64::new(1);

pub trait RedactionDataRetriever {
    fn page_indices(&self) -> Vec<u32>;
    fn annotation_redactions(
        &self,
        page_index: u32,
        include_details: bool,
    ) -> Result<Vec<RedactionOccurrence>, String>;
    fn drawn_redactions(
        &self,
        page_index: u32,
        include_details: bool,
        include_full_page_rects: bool,
    ) -> Result<Vec<RedactionOccurrence>, String>;
    fn raster_redactions(
        &self,
        page_index: u32,
        cfg: &RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String>;
    fn underlying_text_hits(&self, page_index: u32) -> Result<Vec<UnderlyingTextHit>, String>;
    #[inline]
    fn ocr_context_hits(
        &self,
        _page_index: u32,
        _red_bbox: &Rect,
        _cfg: &RedactionFinderConfig,
    ) -> Result<Vec<UnderlyingTextHit>, String> {
        Ok(Vec::new())
    }
}

pub struct PdfFileRetriever<'renderer> {
    doc: Document,
    page_map: BTreeMap<u32, ObjectId>,
    renderer: Option<&'renderer dyn PdfRenderer>,
}

impl<'renderer> PdfFileRetriever<'renderer> {
    #[inline]
    pub fn new_from_bytes(
        bytes: &[u8],
        renderer: Option<&'renderer dyn PdfRenderer>,
    ) -> Result<Self, String> {
        let doc = Document::load_mem(bytes).map_err(|e| e.to_string())?;
        Ok(Self::new(doc, renderer))
    }

    #[inline]
    pub fn new(doc: Document, renderer: Option<&'renderer dyn PdfRenderer>) -> Self {
        let page_map = doc
            .get_pages()
            .into_iter()
            .map(|(page_no, page_id)| (page_no.saturating_sub(1), page_id))
            .collect::<BTreeMap<u32, ObjectId>>();
        Self {
            doc,
            page_map,
            renderer,
        }
    }

    fn page_id(&self, page_index: u32) -> Option<ObjectId> {
        self.page_map.get(&page_index).copied()
    }
}

impl<'renderer> RedactionDataRetriever for PdfFileRetriever<'renderer> {
    #[inline]
    fn page_indices(&self) -> Vec<u32> {
        self.page_map.keys().copied().collect::<Vec<u32>>()
    }

    #[inline]
    fn annotation_redactions(
        &self,
        page_index: u32,
        include_details: bool,
    ) -> Result<Vec<RedactionOccurrence>, String> {
        let page_id = self
            .page_id(page_index)
            .ok_or_else(|| format!("page_missing:index={page_index}"))?;
        extract_annotation_redactions(&self.doc, page_id, page_index, include_details)
    }

    #[inline]
    fn drawn_redactions(
        &self,
        page_index: u32,
        include_details: bool,
        include_full_page_rects: bool,
    ) -> Result<Vec<RedactionOccurrence>, String> {
        let page_id = self
            .page_id(page_index)
            .ok_or_else(|| format!("page_missing:index={page_index}"))?;
        extract_page_drawn_redactions(
            &self.doc,
            page_id,
            page_index,
            include_details,
            include_full_page_rects,
        )
    }

    #[inline]
    fn raster_redactions(
        &self,
        page_index: u32,
        cfg: &RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String> {
        let renderer = match self.renderer {
            Some(r) => r,
            None => return Ok(Vec::new()),
        };

        let page_id = self
            .page_id(page_index)
            .ok_or_else(|| format!("page_missing:index={page_index}"))?;
        let page_box = page_render_box_from_page(&self.doc, page_id)
            .unwrap_or(Rect::new(0.0, 0.0, 612.0, 792.0));

        extract_raster_page_redactions(renderer, page_index, page_box, cfg)
    }

    #[inline]
    fn underlying_text_hits(&self, page_index: u32) -> Result<Vec<UnderlyingTextHit>, String> {
        let page_id = self
            .page_id(page_index)
            .ok_or_else(|| format!("page_missing:index={page_index}"))?;

        extract_page_text_runs(&self.doc, page_id, page_index)
    }

    #[inline]
    fn ocr_context_hits(
        &self,
        page_index: u32,
        red_bbox: &Rect,
        cfg: &RedactionFinderConfig,
    ) -> Result<Vec<UnderlyingTextHit>, String> {
        extract_ocr_context_hits(self, page_index, red_bbox, cfg)
    }
}
fn extract_annotation_redactions(
    doc: &Document,
    page_id: ObjectId,
    page_index: u32,
    include_details: bool,
) -> Result<Vec<RedactionOccurrence>, String> {
    let page_obj = doc.get_object(page_id).map_err(|e| e.to_string())?;
    let page_dict = match page_obj {
        Object::Dictionary(d) => d,
        _ => return Ok(vec![]),
    };

    let annots_obj = match page_dict.get(b"Annots") {
        Ok(o) => o,
        Err(_) => return Ok(vec![]),
    };

    let annots_array = deref_to_array(doc, annots_obj).unwrap_or_default();
    let mut out = Vec::new();

    for a in annots_array {
        let dict = match deref_to_dict(doc, &a) {
            None => continue,
            Some(d) => d,
        };

        let subtype = dict
            .get(b"Subtype")
            .ok()
            .and_then(object_to_name_string)
            .unwrap_or_default();
        let rt = dict
            .get(b"RT")
            .ok()
            .and_then(object_to_name_string)
            .unwrap_or_default();
        let it = dict
            .get(b"IT")
            .ok()
            .and_then(object_to_name_string)
            .unwrap_or_default();
        let ft = dict
            .get(b"FT")
            .ok()
            .and_then(object_to_name_string)
            .unwrap_or_default();
        let nm = dict
            .get(b"NM")
            .ok()
            .and_then(object_to_string_lossy)
            .unwrap_or_default();
        let contents = dict
            .get(b"Contents")
            .ok()
            .and_then(object_to_string_lossy)
            .unwrap_or_default();

        let mut hay = String::new();
        for s in [&subtype, &rt, &it, &ft, &nm, &contents] {
            if !s.is_empty() {
                hay.push_str(s);
                hay.push(' ');
            }
        }
        let hay_lc = hay.to_ascii_lowercase();

        let is_redact_like = hay_lc.contains("redact")
            || hay_lc.contains("redaction")
            || hay_lc.contains("blackout")
            || matches!(subtype.as_str(), "Redact" | "Redaction");

        if !is_redact_like {
            continue;
        }

        let rect_opt = dict.get(b"Rect").ok().and_then(object_to_rect);
        let rect = match rect_opt {
            None => continue,
            Some(r) => r,
        };

        let mut meta: BTreeMap<String, String> = BTreeMap::new();
        if include_details {
            if !subtype.is_empty() {
                meta.insert("Subtype".to_owned(), subtype);
            }
            if !rt.is_empty() {
                meta.insert("RT".to_owned(), rt);
            }
            if !it.is_empty() {
                meta.insert("IT".to_owned(), it);
            }
            if !ft.is_empty() {
                meta.insert("FT".to_owned(), ft);
            }
            if !nm.is_empty() {
                meta.insert("NM".to_owned(), nm);
            }
            if !contents.is_empty() {
                meta.insert("Contents".to_owned(), contents);
            }
        }

        let score = score_rect_as_redaction(&rect);
        out.push(RedactionOccurrence {
            page_index,
            bbox: rect,
            kind: RedactionKind::Annotation,
            score,
            meta,
            underlying_text: vec![],
        });
    }

    Ok(out)
}

fn extract_page_drawn_redactions(
    doc: &Document,
    page_id: ObjectId,
    page_index: u32,
    include_details: bool,
    include_full_page_rects: bool,
) -> Result<Vec<RedactionOccurrence>, String> {
    let page_obj = doc.get_object(page_id).map_err(|e| e.to_string())?;
    let page_dict = match page_obj {
        Object::Dictionary(d) => d.clone(),
        _ => return Ok(vec![]),
    };

    let resources_obj = page_dict.get(b"Resources").ok();
    let resources = resources_obj
        .and_then(|o| deref_to_dict(doc, o))
        .cloned()
        .unwrap_or_else(Dictionary::new);

    let xobject = resources
        .get(b"XObject")
        .ok()
        .and_then(|o| deref_to_dict(doc, o))
        .cloned()
        .unwrap_or_else(Dictionary::new);

    let content = match doc.get_page_content(page_id) {
        Ok(c) => c,
        Err(_) => return Ok(vec![]),
    };

    let decoded = match lopdf::content::Content::decode(&content) {
        Ok(c) => c,
        Err(_) => return Ok(vec![]),
    };

    let mut diagnostics = Vec::new();
    let mut visited = BTreeSet::<String>::new();

    let mut out = extract_drawn_from_ops(
        doc,
        page_index,
        include_details,
        include_full_page_rects,
        true,
        &decoded.operations,
        &xobject,
        &mut diagnostics,
        &mut visited,
    );

    let mut xo = extract_from_xobjects(
        doc,
        page_index,
        include_details,
        include_full_page_rects,
        &decoded.operations,
        &xobject,
        &mut diagnostics,
        &mut visited,
    );

    out.append(&mut xo);
    Ok(out)
}

#[expect(
    clippy::too_many_arguments,
    reason = "PDF content graph traversal requires this contextual parameter set."
)]
fn extract_from_xobjects(
    doc: &Document,
    page_index: u32,
    include_details: bool,
    include_full_page_rects: bool,
    ops: &[lopdf::content::Operation],
    xobject_dict: &Dictionary,
    diagnostics: &mut Vec<String>,
    visited: &mut BTreeSet<String>,
) -> Vec<RedactionOccurrence> {
    let mut out = Vec::new();

    for op in ops {
        if op.operator.as_str() != "Do" {
            continue;
        }

        let name = op
            .operands
            .first()
            .and_then(object_to_name_string)
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }

        let key = format!("page_index={page_index}:xobject={name}");
        if !visited.insert(key) {
            continue;
        }

        let xo_obj = match xobject_dict.get(name.as_bytes()) {
            Ok(o) => o,
            Err(_) => continue,
        };

        let stream = match deref_to_stream(doc, xo_obj) {
            Some(s) => s,
            None => continue,
        };

        let content = match stream.decompressed_content() {
            Ok(c) => c,
            Err(e) => {
                diagnostics.push(format!(
                    "page_index={page_index} xobject={name} decompress_error={e}"
                ));
                continue;
            }
        };

        let decoded = match lopdf::content::Content::decode(&content) {
            Ok(c) => c,
            Err(e) => {
                diagnostics.push(format!(
                    "page_index={page_index} xobject={name} decode_error={e}"
                ));
                continue;
            }
        };

        let mut sub = extract_drawn_from_ops(
            doc,
            page_index,
            include_details,
            include_full_page_rects,
            false,
            &decoded.operations,
            xobject_dict,
            diagnostics,
            visited,
        );
        out.append(&mut sub);

        let mut nested = extract_from_xobjects(
            doc,
            page_index,
            include_details,
            include_full_page_rects,
            &decoded.operations,
            xobject_dict,
            diagnostics,
            visited,
        );
        out.append(&mut nested);
    }

    out
}

#[expect(
    clippy::too_many_arguments,
    reason = "Operation scanning needs page/state/options to avoid global mutable state."
)]
fn extract_drawn_from_ops(
    _doc: &Document,
    page_index: u32,
    include_details: bool,
    include_full_page_rects: bool,
    is_page_level: bool,
    ops: &[lopdf::content::Operation],
    _xobject_dict: &Dictionary,
    _diagnostics: &mut Vec<String>,
    _visited: &mut BTreeSet<String>,
) -> Vec<RedactionOccurrence> {
    let mut out = Vec::new();
    let mut state = DrawState::default();
    let mut path = PathState::default();

    for op in ops {
        let name = op.operator.as_str();
        match name {
            "q" => state = state.push(),
            "Q" => state = state.pop(),
            "rg" => state = state.set_fill_rgb(op.operands.as_slice()),
            "g" => state = state.set_fill_gray(op.operands.as_slice()),
            "k" => state = state.set_fill_cmyk(op.operands.as_slice()),
            "sc" | "scn" => state = state.set_fill_generic(op.operands.as_slice()),
            "m" => path = path.move_to(op.operands.as_slice()),
            "l" => path = path.line_to(op.operands.as_slice()),
            "h" => path = path.close(),
            "n" => path = path.clear(),
            "f" | "f*" => {
                if let Some(rect) = rect_from_path_if_axis_aligned_rect(&path) {
                    let is_black = state.fill_is_black();
                    let score = if is_black {
                        score_rect_as_redaction(&rect)
                    } else {
                        0.0
                    };
                    let keep = is_black
                        && score > 0.2
                        && (include_full_page_rects
                            || !is_page_level
                            || !rect_is_near_full_page(&rect));

                    if keep {
                        let mut meta: BTreeMap<String, String> = BTreeMap::new();
                        if include_details {
                            meta.insert(
                                "fill_rgb".to_owned(),
                                format!(
                                    "{:.3},{:.3},{:.3}",
                                    state.fill_r, state.fill_g, state.fill_b
                                ),
                            );
                            meta.insert("path_kind".to_owned(), op.operator.clone());
                        }
                        out.push(RedactionOccurrence {
                            page_index,
                            bbox: rect,
                            kind: RedactionKind::DrawnPathRect,
                            score,
                            meta,
                            underlying_text: vec![],
                        });
                    }
                }

                path = path.clear();
            }
            "re" => {
                if let Some(rect) = rect_from_re(op.operands.as_slice()) {
                    let is_black = state.fill_is_black();
                    let score = if is_black {
                        score_rect_as_redaction(&rect)
                    } else {
                        0.0
                    };

                    let keep = is_black
                        && score > 0.2
                        && (include_full_page_rects
                            || !is_page_level
                            || !rect_is_near_full_page(&rect));

                    if keep {
                        let mut meta: BTreeMap<String, String> = BTreeMap::new();
                        if include_details {
                            meta.insert(
                                "fill_rgb".to_owned(),
                                format!(
                                    "{:.3},{:.3},{:.3}",
                                    state.fill_r, state.fill_g, state.fill_b
                                ),
                            );
                        }
                        out.push(RedactionOccurrence {
                            page_index,
                            bbox: rect,
                            kind: RedactionKind::DrawnRect,
                            score,
                            meta,
                            underlying_text: vec![],
                        });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[derive(Debug, Clone)]
struct DrawState {
    stack: Vec<DrawStateFrame>,
    fill_r: f32,
    fill_g: f32,
    fill_b: f32,
    fill_gray: f32,
    fill_c: f32,
    fill_m: f32,
    fill_y: f32,
    fill_k: f32,
}

#[derive(Debug, Clone)]
struct DrawStateFrame {
    fill_r: f32,
    fill_g: f32,
    fill_b: f32,
    fill_gray: f32,
    fill_c: f32,
    fill_m: f32,
    fill_y: f32,
    fill_k: f32,
}

impl Default for DrawState {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            fill_r: 0.0,
            fill_g: 0.0,
            fill_b: 0.0,
            fill_gray: 0.0,
            fill_c: 0.0,
            fill_m: 0.0,
            fill_y: 0.0,
            fill_k: 0.0,
        }
    }
}

impl DrawState {
    fn push(mut self) -> Self {
        self.stack.push(DrawStateFrame {
            fill_r: self.fill_r,
            fill_g: self.fill_g,
            fill_b: self.fill_b,
            fill_gray: self.fill_gray,
            fill_c: self.fill_c,
            fill_m: self.fill_m,
            fill_y: self.fill_y,
            fill_k: self.fill_k,
        });
        self
    }

    fn pop(mut self) -> Self {
        if let Some(f) = self.stack.pop() {
            self.fill_r = f.fill_r;
            self.fill_g = f.fill_g;
            self.fill_b = f.fill_b;
            self.fill_gray = f.fill_gray;
            self.fill_c = f.fill_c;
            self.fill_m = f.fill_m;
            self.fill_y = f.fill_y;
            self.fill_k = f.fill_k;
        }
        self
    }

    fn set_fill_rgb(mut self, operands: &[Object]) -> Self {
        let r = operands
            .first()
            .and_then(object_to_f32)
            .unwrap_or(self.fill_r);
        let g = operands
            .get(1)
            .and_then(object_to_f32)
            .unwrap_or(self.fill_g);
        let b = operands
            .get(2)
            .and_then(object_to_f32)
            .unwrap_or(self.fill_b);
        self.fill_r = r;
        self.fill_g = g;
        self.fill_b = b;
        self
    }

    fn set_fill_gray(mut self, operands: &[Object]) -> Self {
        let g = operands
            .first()
            .and_then(object_to_f32)
            .unwrap_or(self.fill_gray);
        self.fill_gray = g;
        self.fill_r = g;
        self.fill_g = g;
        self.fill_b = g;
        self
    }

    fn set_fill_cmyk(mut self, operands: &[Object]) -> Self {
        let c = operands
            .first()
            .and_then(object_to_f32)
            .unwrap_or(self.fill_c);
        let m = operands
            .get(1)
            .and_then(object_to_f32)
            .unwrap_or(self.fill_m);
        let y = operands
            .get(2)
            .and_then(object_to_f32)
            .unwrap_or(self.fill_y);
        let k = operands
            .get(3)
            .and_then(object_to_f32)
            .unwrap_or(self.fill_k);
        self.fill_c = c;
        self.fill_m = m;
        self.fill_y = y;
        self.fill_k = k;
        let r = (1.0 - c).max(0.0) * (1.0 - k).max(0.0);
        let g = (1.0 - m).max(0.0) * (1.0 - k).max(0.0);
        let b = (1.0 - y).max(0.0) * (1.0 - k).max(0.0);
        self.fill_r = r;
        self.fill_g = g;
        self.fill_b = b;
        self
    }

    fn set_fill_generic(self, operands: &[Object]) -> Self {
        if operands.len() == 1 {
            return self.set_fill_gray(operands);
        }
        if operands.len() == 3 {
            return self.set_fill_rgb(operands);
        }
        if operands.len() == 4 {
            return self.set_fill_cmyk(operands);
        }
        self
    }

    fn fill_is_black(&self) -> bool {
        self.fill_r <= 0.01 && self.fill_g <= 0.01 && self.fill_b <= 0.01
    }
}

fn rect_from_re(operands: &[Object]) -> Option<Rect> {
    let x = operands.first().and_then(object_to_f32)?;
    let y = operands.get(1).and_then(object_to_f32)?;
    let w = operands.get(2).and_then(object_to_f32)?;
    let h = operands.get(3).and_then(object_to_f32)?;

    if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
        return None;
    }

    Some(Rect::new(x, y, x + w, y + h))
}

fn rect_is_near_full_page(r: &Rect) -> bool {
    let w = r.width().abs();

    let h = r.height().abs();

    if w <= 0.0 || h <= 0.0 {
        return false;
    }

    w >= 500.0 && h >= 650.0
}

/// Check if a rect is near full-page, given an explicit page size in PDF
/// units. This is used by the page-rendering-based raster detector.
fn rect_is_near_full_page_with_size(r: &Rect, page_width_pt: f32, page_height_pt: f32) -> bool {
    let w = r.width().abs();
    let h = r.height().abs();
    if w <= 0.0 || h <= 0.0 || page_width_pt <= 0.0 || page_height_pt <= 0.0 {
        return false;
    }
    let frac_w = w / page_width_pt;
    let frac_h = h / page_height_pt;
    frac_w >= 0.9 && frac_h >= 0.9
}

fn score_rect_as_redaction(r: &Rect) -> f32 {
    let w = r.width().abs();
    let h = r.height().abs();

    if w <= 0.0 || h <= 0.0 {
        return 0.0;
    }

    let aspect = if h > 0.0 { w / h } else { 0.0 };
    let area = r.area();

    let mut score: f32 = 0.0;

    if area >= 25.0 {
        score += 0.2;
    }
    if area >= 200.0 {
        score += 0.2;
    }
    if aspect >= 2.0 {
        score += 0.2;
    }
    if aspect >= 6.0 {
        score += 0.2;
    }
    if w >= 20.0 && h >= 6.0 {
        score += 0.2;
    }

    score.min(1.0)
}

#[derive(Debug, Clone, Default)]
struct PathState {
    current: Option<(f32, f32)>,
    start: Option<(f32, f32)>,
    points: Vec<(f32, f32)>,
    closed: bool,
}

impl PathState {
    fn clear(mut self) -> Self {
        self.current = None;
        self.start = None;
        self.points.clear();
        self.closed = false;
        self
    }

    fn move_to(mut self, operands: &[Object]) -> Self {
        let x_opt = operands.first().and_then(object_to_f32);
        let y_opt = operands.get(1).and_then(object_to_f32);
        match (x_opt, y_opt) {
            (Some(x_value), Some(y_value)) => {
                self.current = Some((x_value, y_value));
                self.start = Some((x_value, y_value));
                self.points.clear();
                self.points.push((x_value, y_value));
                self.closed = false;
                self
            }
            _ => self,
        }
    }

    fn line_to(mut self, operands: &[Object]) -> Self {
        let x_opt = operands.first().and_then(object_to_f32);
        let y_opt = operands.get(1).and_then(object_to_f32);
        match (x_opt, y_opt) {
            (Some(x_value), Some(y_value)) => {
                self.current = Some((x_value, y_value));
                self.points.push((x_value, y_value));
                self
            }
            _ => self,
        }
    }

    fn close(mut self) -> Self {
        self.closed = true;
        self
    }
}

fn rect_from_path_if_axis_aligned_rect(path: &PathState) -> Option<Rect> {
    if !path.closed {
        return None;
    }
    if path.points.len() < 4 {
        return None;
    }

    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for (x, y) in &path.points {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        xs.push(*x);
        ys.push(*y);
    }

    let (min_x, max_x) = min_max_f32(&xs)?;
    let (min_y, max_y) = min_max_f32(&ys)?;

    let w = (max_x - min_x).abs();
    let h = (max_y - min_y).abs();
    if w <= 0.0 || h <= 0.0 {
        return None;
    }

    let corners = [
        (min_x, min_y),
        (min_x, max_y),
        (max_x, min_y),
        (max_x, max_y),
    ];

    let unique = path
        .points
        .iter()
        .map(|p| (round2(p.0), round2(p.1)))
        .collect::<BTreeSet<_>>();

    let corners_hit = corners
        .iter()
        .filter(|(x, y)| unique.contains(&(round2(*x), round2(*y))))
        .count();

    if corners_hit < 3 {
        return None;
    }

    Some(Rect::new(min_x, min_y, max_x, max_y))
}

fn min_max_f32(vals: &[f32]) -> Option<(f32, f32)> {
    let mut it = vals.iter().copied();
    let first = it.next()?;
    let mut min_v = first;
    let mut max_v = first;
    for v in it {
        if v < min_v {
            min_v = v;
        }
        if v > max_v {
            max_v = v;
        }
    }
    Some((min_v, max_v))
}

fn round2(v: f32) -> i32 {
    (v * 100.0).round() as i32
}

fn normalized_rect_from_pixels(
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    width: usize,
    height: usize,
) -> Rect {
    if width == 0 || height == 0 {
        return Rect::new(0.0, 0.0, 0.0, 0.0);
    }

    let fx0 = x0 as f32 / width as f32;
    let fx1 = x1 as f32 / width as f32;
    // Keep image-space Y in top-origin coordinates here. The conversion to
    // bottom-origin PDF coordinates is handled once in `rect_pixels_to_pdf`.
    let fy0 = y0 as f32 / height as f32;
    let fy1 = y1 as f32 / height as f32;

    Rect::new(fx0, fy0, fx1, fy1)
}

/// Map a rectangle in rendered-page pixel coordinates (top-left origin) to
/// PDF user-space coordinates, given the page size and DPI.
fn rect_pixels_to_pdf(
    x0_px: u32,
    y0_px: u32,
    x1_px: u32,
    y1_px: u32,
    page_box: Rect,
    dpi: f32,
) -> Rect {
    // Convert pixel positions to inches.
    let x0_in = x0_px as f32 / dpi;
    let x1_in = x1_px as f32 / dpi;
    let y0_in_from_top = y0_px as f32 / dpi;
    let y1_in_from_top = y1_px as f32 / dpi;

    // Convert inches to points.
    let x0_pt = (page_box.x0 + (x0_in * 72.0)).clamp(page_box.x0, page_box.x1);
    let x1_pt = (page_box.x0 + (x1_in * 72.0)).clamp(page_box.x0, page_box.x1);

    // Flip Y from top-origin image to bottom-origin PDF and retain page-box origin.
    let y1_pt = (page_box.y1 - (y0_in_from_top * 72.0)).clamp(page_box.y0, page_box.y1);
    let y0_pt = (page_box.y1 - (y1_in_from_top * 72.0)).clamp(page_box.y0, page_box.y1);

    Rect::new(x0_pt, y0_pt, x1_pt, y1_pt)
}

/// Map a rectangle in PDF user-space coordinates to rendered-page pixel
/// coordinates (top-left origin). Returns `None` when the input cannot be
/// represented as a non-empty pixel rect.
fn rect_pdf_to_pixels(
    rect: &Rect,
    page_box: Rect,
    dpi: f32,
    width_px: u32,
    height_px: u32,
) -> Option<(u32, u32, u32, u32)> {
    if dpi <= 0.0 || width_px == 0 || height_px == 0 {
        return None;
    }

    let x0 = (((rect.x0 - page_box.x0) / 72.0) * dpi).floor();
    let x1 = (((rect.x1 - page_box.x0) / 72.0) * dpi).ceil();
    let y0 = (((page_box.y1 - rect.y1) / 72.0) * dpi).floor();
    let y1 = (((page_box.y1 - rect.y0) / 72.0) * dpi).ceil();

    let x0_px = x0.clamp(0.0, width_px as f32) as u32;
    let x1_px = x1.clamp(0.0, width_px as f32) as u32;
    let y0_px = y0.clamp(0.0, height_px as f32) as u32;
    let y1_px = y1.clamp(0.0, height_px as f32) as u32;

    if x1_px <= x0_px || y1_px <= y0_px {
        return None;
    }
    Some((x0_px, y0_px, x1_px, y1_px))
}

fn build_ocr_context_windows(red_bbox: &Rect, page_box: Rect) -> Vec<Rect> {
    let mut windows = Vec::with_capacity(2);
    let red_width = red_bbox.width().abs().max(1.0);
    let window_width_pt = (red_width * 1.4).clamp(OCR_WINDOW_MIN_WIDTH_PT, OCR_WINDOW_MAX_WIDTH_PT);
    let y0 = (red_bbox.y0 - OCR_WINDOW_PADDING_PT).max(page_box.y0);
    let y1 = (red_bbox.y1 + OCR_WINDOW_PADDING_PT).min(page_box.y1);
    if y1 <= y0 {
        return windows;
    }

    let left_x0 = (red_bbox.x0 - window_width_pt).max(page_box.x0);
    let left_x1 = red_bbox.x0.min(page_box.x1);
    if left_x1 - left_x0 >= 2.0 {
        windows.push(Rect::new(left_x0, y0, left_x1, y1));
    }

    let right_x0 = red_bbox.x1.max(page_box.x0);
    let right_x1 = (red_bbox.x1 + window_width_pt).min(page_box.x1);
    if right_x1 - right_x0 >= 2.0 {
        windows.push(Rect::new(right_x0, y0, right_x1, y1));
    }

    windows
}

fn ocr_enabled_from_env() -> bool {
    static OCR_ENABLED: OnceLock<bool> = OnceLock::new();
    *OCR_ENABLED.get_or_init(|| {
        std::env::var(OCR_ENABLE_ENV)
            .map(|raw| {
                let value = raw.trim().to_ascii_lowercase();
                matches!(value.as_str(), "1" | "true" | "yes" | "on")
            })
            .unwrap_or(false)
    })
}

fn ocr_command_from_env() -> OsString {
    static OCR_COMMAND: OnceLock<OsString> = OnceLock::new();
    OCR_COMMAND
        .get_or_init(|| {
            std::env::var_os(OCR_CMD_ENV).unwrap_or_else(|| OsString::from("tesseract"))
        })
        .clone()
}

fn write_temp_pgm(
    gray: &[u8],
    width: u32,
    height: u32,
    page_index: u32,
) -> Result<PathBuf, String> {
    let seq = OCR_TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let name = format!(
        "unredact_ocr_p{page_index}_{}_{}.pgm",
        std::process::id(),
        seq
    );
    let path = std::env::temp_dir().join(name);

    let mut bytes = format!("P5\n{} {}\n255\n", width, height).into_bytes();
    bytes.extend_from_slice(gray);
    fs::write(&path, bytes).map_err(|e| format!("ocr_temp_write_failed:{e}"))?;
    Ok(path)
}

fn normalize_ocr_text(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn run_local_ocr(image_path: &Path, dpi: f32) -> Result<String, String> {
    let command = ocr_command_from_env();
    let output = Command::new(command)
        .arg(image_path)
        .arg("stdout")
        .arg("--psm")
        .arg("7")
        .arg("--oem")
        .arg("1")
        .arg("-l")
        .arg("eng")
        .arg("--dpi")
        .arg(format!("{:.0}", dpi.max(72.0)))
        .output()
        .map_err(|e| format!("ocr_exec_failed:{e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(format!(
            "ocr_failed:status={} stderr={stderr}",
            output.status
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn crop_gray_region(
    gray: &[u8],
    image_width: usize,
    image_height: usize,
    x0_px: u32,
    y0_px: u32,
    x1_px: u32,
    y1_px: u32,
) -> Option<Vec<u8>> {
    if gray.len() < image_width.saturating_mul(image_height) {
        return None;
    }
    let x0 = x0_px.min(image_width as u32) as usize;
    let y0 = y0_px.min(image_height as u32) as usize;
    let x1 = x1_px.min(image_width as u32) as usize;
    let y1 = y1_px.min(image_height as u32) as usize;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let crop_w = x1 - x0;
    let crop_h = y1 - y0;
    let mut out = Vec::with_capacity(crop_w.saturating_mul(crop_h));
    for y in y0..y1 {
        let row_start = y.saturating_mul(image_width);
        out.extend_from_slice(&gray[row_start + x0..row_start + x1]);
    }
    Some(out)
}

fn extract_ocr_context_hits(
    retriever: &PdfFileRetriever<'_>,
    page_index: u32,
    red_bbox: &Rect,
    cfg: &RedactionFinderConfig,
) -> Result<Vec<UnderlyingTextHit>, String> {
    if !ocr_enabled_from_env() {
        return Err(format!(
            "ocr_disabled:set {OCR_ENABLE_ENV}=1 to enable local OCR context"
        ));
    }

    let Some(renderer) = retriever.renderer else {
        return Err("ocr_renderer_unavailable".to_owned());
    };
    let Some(page_id) = retriever.page_id(page_index) else {
        return Err("ocr_page_missing".to_owned());
    };

    let page_box = page_render_box_from_page(&retriever.doc, page_id)
        .unwrap_or(Rect::new(0.0, 0.0, 612.0, 792.0));

    let effective_dpi = cfg.raster_dpi.min(MAX_RASTER_ANALYSIS_DPI);
    let rendered = renderer
        .render_page_to_rgba(page_index as usize, effective_dpi)
        .map_err(|e| format!("ocr_render_failed:{e}"))?;
    if rendered.width_px == 0 || rendered.height_px == 0 {
        return Err("ocr_empty_render".to_owned());
    }

    let gray = rgba_to_grayscale(&rendered.pixels, rendered.width_px, rendered.height_px);
    if gray.is_empty() {
        return Err("ocr_gray_conversion_empty".to_owned());
    }

    let windows = build_ocr_context_windows(red_bbox, page_box);
    if windows.is_empty() {
        return Err("ocr_windows_empty".to_owned());
    }

    let mut hits = Vec::new();
    for window in windows {
        let Some((x0_px, y0_px, x1_px, y1_px)) = rect_pdf_to_pixels(
            &window,
            page_box,
            rendered.dpi,
            rendered.width_px,
            rendered.height_px,
        ) else {
            continue;
        };
        let crop = crop_gray_region(
            &gray,
            rendered.width_px as usize,
            rendered.height_px as usize,
            x0_px,
            y0_px,
            x1_px,
            y1_px,
        );
        let Some(crop) = crop else {
            continue;
        };
        let crop_w = x1_px.saturating_sub(x0_px);
        let crop_h = y1_px.saturating_sub(y0_px);
        if crop_w < 8 || crop_h < 8 {
            continue;
        }

        let path = write_temp_pgm(&crop, crop_w, crop_h, page_index)?;
        let ocr_text = run_local_ocr(&path, rendered.dpi);
        drop(fs::remove_file(&path));
        let text = normalize_ocr_text(&ocr_text?);
        if text.is_empty() {
            continue;
        }
        hits.push(UnderlyingTextHit {
            page_index,
            bbox: window,
            text,
        });
    }

    Ok(hits)
}

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

/// A dark region detected in a rendered page image, expressed directly in
/// pixel coordinates. This is the page-level counterpart to
/// `ImageRegionDetection`.
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
    /// Contiguous dark-column runs relative to the region origin.
    /// Each tuple is `[x0, x1)` in pixels.
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

/// Convert the grid-based `ImageDetectionResult` for a full rendered page into
/// `DarkRegionDetections` that encode pixel-space rectangles directly. This is
/// purely geometric and deterministic.
fn image_detections_to_dark_regions(
    detections: &ImageDetectionResult,
    width_px: u32,
    height_px: u32,
) -> DarkRegionDetections {
    let mut regions = Vec::new();
    for det in &detections.detections {
        let x0_px = (det.normalized_rect.x0 * width_px as f32)
            .round()
            .clamp(0.0, width_px as f32) as u32;
        let x1_px = (det.normalized_rect.x1 * width_px as f32)
            .round()
            .clamp(0.0, width_px as f32) as u32;
        let y0_px = (det.normalized_rect.y0 * height_px as f32)
            .round()
            .clamp(0.0, height_px as f32) as u32;
        let y1_px = (det.normalized_rect.y1 * height_px as f32)
            .round()
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

    fill_small_bright_gaps(&mut dark_cols, 2);

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
        if max_gap_px < 2 {
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

/// Perform raster-based dark region detection on a fully rendered page bitmap
/// using the same detector as image XObjects, then map the result back into
/// PDF coordinates and produce `RedactionOccurrence`s.
fn extract_raster_page_redactions(
    renderer: &dyn PdfRenderer,
    page_index: u32,
    page_box: Rect,
    cfg: &RedactionFinderConfig,
) -> Result<Vec<RedactionOccurrence>, String> {
    let effective_dpi = cfg.raster_dpi.min(MAX_RASTER_ANALYSIS_DPI);
    let rendered = renderer
        .render_page_to_rgba(page_index as usize, effective_dpi)
        .map_err(|e| format!("render_failed:{e}"))?;

    if rendered.width_px == 0 || rendered.height_px == 0 {
        return Ok(Vec::new());
    }

    // Convert to grayscale using standard luma; ignore alpha.
    let gray = rgba_to_grayscale(&rendered.pixels, rendered.width_px, rendered.height_px);

    let detection = detect_dark_regions_in_image(
        &gray,
        rendered.width_px as usize,
        rendered.height_px as usize,
    );
    if detection.detections.is_empty() {
        return Ok(Vec::new());
    }

    let regions =
        image_detections_to_dark_regions(&detection, rendered.width_px, rendered.height_px);

    let mut out = Vec::new();
    for det in regions.regions {
        let profile = dark_run_profile_for_region(
            &gray,
            rendered.width_px as usize,
            rendered.height_px as usize,
            &det,
        );
        let split_regions = split_dark_region_by_profile(&det, &profile);
        let split_count = split_regions.len().max(1);

        for (split_index, split_region) in split_regions.into_iter().enumerate() {
            // Map pixel rects back into PDF user space.
            let page_rect = rect_pixels_to_pdf(
                split_region.x0_px,
                split_region.y0_px,
                split_region.x1_px,
                split_region.y1_px,
                page_box,
                rendered.dpi,
            );

            if rect_is_near_full_page_with_size(
                &page_rect,
                page_box.width().abs(),
                page_box.height().abs(),
            ) {
                continue;
            }
            if page_rect.width().abs() < 2.0 || page_rect.height().abs() < 2.0 {
                continue;
            }

            let darkness =
                (1.0_f32 - split_region.avg_luminance / 255.0_f32).clamp(0.0_f32, 1.0_f32);
            let mut score = (split_region.score * 0.7) + (darkness * 0.3);
            score = score.min(1.0);

            let split_profile = dark_run_profile_for_region(
                &gray,
                rendered.width_px as usize,
                rendered.height_px as usize,
                &split_region,
            );

            let mut meta: BTreeMap<String, String> = BTreeMap::new();
            if cfg.include_details {
                meta.insert("raster_dpi".to_owned(), format!("{:.1}", rendered.dpi));
                meta.insert(
                    "raster_dpi_requested".to_owned(),
                    format!("{:.1}", cfg.raster_dpi),
                );
                meta.insert(
                    "raster_dpi_effective".to_owned(),
                    format!("{:.1}", effective_dpi),
                );
                meta.insert(
                    "image_dims_px".to_owned(),
                    format!("{}x{}", rendered.width_px, rendered.height_px),
                );
                meta.insert(
                    "region_area_fraction".to_owned(),
                    format!("{:.4}", split_region.area_fraction),
                );
                meta.insert(
                    "region_avg_luminance".to_owned(),
                    format!("{:.1}", split_region.avg_luminance),
                );
            }
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

            out.push(RedactionOccurrence {
                page_index,
                bbox: page_rect,
                kind: RedactionKind::RasterDarkRegion,
                score,
                meta,
                underlying_text: vec![],
            });
        }
    }

    Ok(out)
}

/// Convert RGBA (8-bit per channel, row-major, top-left origin) into 8-bit
/// grayscale using a standard luma transform.
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

fn extract_page_text_runs(
    doc: &Document,
    page_id: ObjectId,
    page_index: u32,
) -> Result<Vec<UnderlyingTextHit>, String> {
    let page_obj = doc.get_object(page_id).map_err(|e| e.to_string())?;
    let page_dict = match page_obj {
        Object::Dictionary(d) => d.clone(),
        _ => return Ok(Vec::new()),
    };

    let resources_obj = page_dict.get(b"Resources").ok();
    let resources = resources_obj
        .and_then(|o| deref_to_dict(doc, o))
        .cloned()
        .unwrap_or_else(Dictionary::new);

    let xobject = resources
        .get(b"XObject")
        .ok()
        .and_then(|o| deref_to_dict(doc, o))
        .cloned()
        .unwrap_or_else(Dictionary::new);

    let content = doc
        .get_page_content(page_id)
        .map_err(|e| format!("page_content_error={e}"))?;
    let decoded =
        lopdf::content::Content::decode(&content).map_err(|e| format!("page_decode_error={e}"))?;

    let mut out = extract_text_runs(page_index, &decoded.operations);
    let mut visited = BTreeSet::<String>::new();
    let mut nested = extract_text_runs_from_xobjects(
        doc,
        page_index,
        &decoded.operations,
        &xobject,
        &mut visited,
    );
    out.append(&mut nested);
    Ok(out)
}

fn extract_text_runs_from_xobjects(
    doc: &Document,
    page_index: u32,
    ops: &[lopdf::content::Operation],
    xobject_dict: &Dictionary,
    visited: &mut BTreeSet<String>,
) -> Vec<UnderlyingTextHit> {
    let mut out = Vec::new();

    for op in ops {
        if op.operator.as_str() != "Do" {
            continue;
        }

        let name = op
            .operands
            .first()
            .and_then(object_to_name_string)
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }

        let key = format!("page_index={page_index}:text_xobject={name}");
        if !visited.insert(key) {
            continue;
        }

        let xo_obj = match xobject_dict.get(name.as_bytes()) {
            Ok(o) => o,
            Err(_) => continue,
        };

        let stream = match deref_to_stream(doc, xo_obj) {
            Some(s) => s,
            None => continue,
        };

        let content = match stream.decompressed_content() {
            Ok(c) => c,
            Err(_) => continue,
        };

        let decoded = match lopdf::content::Content::decode(&content) {
            Ok(c) => c,
            Err(_) => continue,
        };

        out.extend(extract_text_runs(page_index, &decoded.operations));

        let nested_xobject = stream
            .dict
            .get(b"Resources")
            .ok()
            .and_then(|o| deref_to_dict(doc, o))
            .and_then(|res| res.get(b"XObject").ok())
            .and_then(|o| deref_to_dict(doc, o))
            .cloned()
            .unwrap_or_else(|| xobject_dict.clone());

        let mut nested = extract_text_runs_from_xobjects(
            doc,
            page_index,
            &decoded.operations,
            &nested_xobject,
            visited,
        );
        out.append(&mut nested);
    }

    out
}

#[derive(Debug, Clone, Default)]
struct TextState {
    in_text: bool,
    font_size: f32,
    tm_e: f32,
    tm_f: f32,
}

fn extract_text_runs(page_index: u32, ops: &[lopdf::content::Operation]) -> Vec<UnderlyingTextHit> {
    let mut out = Vec::new();
    let mut st = TextState::default();

    for op in ops {
        match op.operator.as_str() {
            "BT" => st.in_text = true,
            "ET" => st.in_text = false,
            "Tf" => {
                st.font_size = op
                    .operands
                    .get(1)
                    .and_then(object_to_f32)
                    .unwrap_or(st.font_size);
            }
            "Tm" => {
                st.tm_e = op
                    .operands
                    .get(4)
                    .and_then(object_to_f32)
                    .unwrap_or(st.tm_e);
                st.tm_f = op
                    .operands
                    .get(5)
                    .and_then(object_to_f32)
                    .unwrap_or(st.tm_f);
            }
            "Td" | "TD" => {
                let dx = op.operands.first().and_then(object_to_f32).unwrap_or(0.0);
                let dy = op.operands.get(1).and_then(object_to_f32).unwrap_or(0.0);
                st.tm_e += dx;
                st.tm_f += dy;
            }
            "Tj" | "TJ" | "'" | "\"" => {
                if !st.in_text {
                    continue;
                }
                let raw_text = text_from_show_op(op);
                let text = raw_text.trim().to_owned();
                if text.is_empty() {
                    continue;
                }

                let h = st.font_size.abs().max(1.0);
                let w = (st.font_size.abs() * 0.6 * (text.chars().count().max(1) as f32)).max(1.0);

                let x0 = st.tm_e;
                let y1 = st.tm_f;
                let bbox = Rect::new(x0, y1 - h, x0 + w, y1);

                out.push(UnderlyingTextHit {
                    page_index,
                    bbox,
                    text,
                });
            }
            _ => {}
        }
    }

    out
}

fn text_from_show_op(op: &lopdf::content::Operation) -> String {
    if op.operator.as_str() == "TJ" {
        if let Some(Object::Array(a)) = op.operands.first() {
            return a
                .iter()
                .filter_map(decode_pdf_text)
                .collect::<Vec<_>>()
                .join("");
        }
    }

    if let Some(text) = op.operands.last().and_then(decode_pdf_text) {
        return text;
    }

    String::new()
}

fn decode_pdf_text(obj: &Object) -> Option<String> {
    match obj {
        Object::String(raw_bytes, _) => {
            let decoded = lopdf::decode_text_string(obj)
                .ok()
                .filter(|text| !text.contains('\u{FFFD}'));
            match decoded {
                Some(text) => Some(normalize_decoded_text(&text)),
                None => Some(normalize_decoded_text(&String::from_utf8_lossy(raw_bytes))),
            }
        }
        _ => None,
    }
}

fn normalize_decoded_text(text: &str) -> String {
    text.chars()
        .map(|char_value| match char_value {
            '‰' | '‹' | '«' => '<',
            '›' | '»' => '>',
            _ => char_value,
        })
        .collect::<String>()
}

fn deref_to_array(doc: &Document, obj: &Object) -> Option<Vec<Object>> {
    match obj {
        Object::Reference(oid) => match doc.get_object(*oid).ok()? {
            Object::Array(a) => Some(a.clone()),
            _ => None,
        },
        Object::Array(a) => Some(a.clone()),
        _ => None,
    }
}

fn deref_to_dict<'doc>(doc: &'doc Document, obj: &'doc Object) -> Option<&'doc Dictionary> {
    match obj {
        Object::Reference(oid) => match doc.get_object(*oid).ok()? {
            Object::Dictionary(d) => Some(d),
            _ => None,
        },
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

fn deref_to_stream<'doc>(doc: &'doc Document, obj: &'doc Object) -> Option<&'doc Stream> {
    match obj {
        Object::Reference(oid) => match doc.get_object(*oid).ok()? {
            Object::Stream(s) => Some(s),

            _ => None,
        },

        Object::Stream(s) => Some(s),

        _ => None,
    }
}

fn object_to_f32(o: &Object) -> Option<f32> {
    match o {
        Object::Real(r) => Some(*r),
        Object::Integer(i) => Some(*i as f32),
        _ => None,
    }
}

fn object_to_name_string(o: &Object) -> Option<String> {
    match o {
        Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        _ => None,
    }
}

fn object_to_string_lossy(o: &Object) -> Option<String> {
    match o {
        Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
        _ => None,
    }
}

fn object_to_rect(o: &Object) -> Option<Rect> {
    let a = match o {
        Object::Array(a) => a,

        _ => return None,
    };

    if a.len() != 4 {
        return None;
    }

    let x0 = a.first().and_then(object_to_f32)?;

    let y0 = a.get(1).and_then(object_to_f32)?;

    let x1 = a.get(2).and_then(object_to_f32)?;

    let y1 = a.get(3).and_then(object_to_f32)?;

    Some(Rect::new(x0, y0, x1, y1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_dark_bar_in_synthetic_image() {
        let width = 60;
        let height = 32;
        let mut buf = vec![235_u8; width * height];
        for y in 10..22 {
            for x in 5..55 {
                buf[y * width + x] = 5;
            }
        }
        let result = detect_dark_regions_in_image(&buf, width, height);
        assert!(!result.detections.is_empty());
        let region = &result.detections[0];
        assert!(region.area_fraction > 0.1);
        assert!(region.avg_luminance < 80.0);
    }

    #[test]
    fn ignores_sparse_noise() {
        let width = 64;
        let height = 64;
        let mut buf = vec![210_u8; width * height];
        for i in (0..width * height).step_by(1500) {
            buf[i] = 20;
        }
        let result = detect_dark_regions_in_image(&buf, width, height);
        assert!(result.detections.is_empty());
    }

    #[test]
    fn rect_pixels_to_pdf_maps_coords() {
        let mapped = rect_pixels_to_pdf(72, 72, 648, 648, Rect::new(0.0, 0.0, 720.0, 720.0), 72.0);
        assert!((mapped.x0 - 72.0).abs() < 0.01);
        assert!((mapped.x1 - 648.0).abs() < 0.01);
        assert!((mapped.y0 - 72.0).abs() < 0.01);
        assert!((mapped.y1 - 648.0).abs() < 0.01);
    }

    #[test]
    fn rect_pixels_to_pdf_respects_page_box_origin() {
        let mapped =
            rect_pixels_to_pdf(0, 0, 100, 100, Rect::new(0.0, -18.0, 612.0, 804.75), 200.0);
        assert!((mapped.x0 - 0.0).abs() < 0.01);
        assert!((mapped.y1 - 804.75).abs() < 0.01);
        assert!((mapped.y0 - 768.75).abs() < 0.01);
    }

    #[test]
    fn dark_profile_splits_two_bars() {
        let width = 120_usize;
        let height = 24_usize;
        let mut buf = vec![230_u8; width * height];
        for y in 8..17 {
            for x in 10..46 {
                buf[y * width + x] = 8;
            }
            for x in 60..101 {
                buf[y * width + x] = 10;
            }
        }

        let region = DarkRegion {
            x0_px: 10,
            y0_px: 8,
            x1_px: 101,
            y1_px: 17,
            avg_luminance: 35.0,
            area_fraction: 0.10,
            score: 0.9,
        };
        let profile = dark_run_profile_for_region(&buf, width, height, &region);
        assert!(profile.dark_runs.len() >= 2);
        assert!(profile.max_gap_px >= 4);
        assert!(profile.split_confidence > 0.2);

        let split = split_dark_region_by_profile(&region, &profile);
        assert_eq!(split.len(), 2);
        assert!(split[0].x1_px <= split[1].x0_px);
    }

    #[test]
    fn dark_profile_does_not_split_single_bar() {
        let width = 80_usize;
        let height = 20_usize;
        let mut buf = vec![220_u8; width * height];
        for y in 6..14 {
            for x in 14..66 {
                buf[y * width + x] = 6;
            }
        }

        let region = DarkRegion {
            x0_px: 14,
            y0_px: 6,
            x1_px: 66,
            y1_px: 14,
            avg_luminance: 28.0,
            area_fraction: 0.09,
            score: 0.85,
        };
        let profile = dark_run_profile_for_region(&buf, width, height, &region);
        let split = split_dark_region_by_profile(&region, &profile);
        assert_eq!(split.len(), 1);
    }
}
