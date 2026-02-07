use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::redaction_finder::types::{
    PdfRenderer, Rect, RedactionFinderConfig, RedactionKind, RedactionOccurrence, UnderlyingTextHit,
};

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
}

pub struct PdfFileRetriever<'a> {
    doc: Document,
    page_map: BTreeMap<u32, ObjectId>,
    renderer: Option<&'a dyn PdfRenderer>,
}

impl<'a> PdfFileRetriever<'a> {
    pub fn new_from_bytes(
        bytes: &[u8],
        renderer: Option<&'a dyn PdfRenderer>,
    ) -> Result<Self, String> {
        let doc = Document::load_mem(bytes).map_err(|e| e.to_string())?;
        Ok(Self::new(doc, renderer))
    }

    pub fn new(doc: Document, renderer: Option<&'a dyn PdfRenderer>) -> Self {
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

impl<'a> RedactionDataRetriever for PdfFileRetriever<'a> {
    fn page_indices(&self) -> Vec<u32> {
        self.page_map.keys().copied().collect::<Vec<u32>>()
    }

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
        let (page_width_pt, page_height_pt) =
            page_size_from_page(&self.doc, page_id).unwrap_or((612.0, 792.0));

        extract_raster_page_redactions(renderer, page_index, page_width_pt, page_height_pt, cfg)
    }

    fn underlying_text_hits(&self, page_index: u32) -> Result<Vec<UnderlyingTextHit>, String> {
        let page_id = self
            .page_id(page_index)
            .ok_or_else(|| format!("page_missing:index={page_index}"))?;

        let content = self
            .doc
            .get_page_content(page_id)
            .map_err(|e| format!("page_content_error={e}"))?;
        let decoded = lopdf::content::Content::decode(&content)
            .map_err(|e| format!("page_decode_error={e}"))?;

        Ok(extract_text_runs(page_index, &decoded.operations))
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

        let rect = dict.get(b"Rect").ok().and_then(object_to_rect);
        let rect = match rect {
            None => continue,
            Some(r) => r,
        };

        let mut meta: BTreeMap<String, String> = BTreeMap::new();
        if include_details {
            if !subtype.is_empty() {
                meta.insert("Subtype".to_string(), subtype);
            }
            if !rt.is_empty() {
                meta.insert("RT".to_string(), rt);
            }
            if !it.is_empty() {
                meta.insert("IT".to_string(), it);
            }
            if !ft.is_empty() {
                meta.insert("FT".to_string(), ft);
            }
            if !nm.is_empty() {
                meta.insert("NM".to_string(), nm);
            }
            if !contents.is_empty() {
                meta.insert("Contents".to_string(), contents);
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
            .get(0)
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

fn extract_drawn_from_ops(
    _doc: &Document,
    page_index: u32,
    include_details: bool,
    include_full_page_rects: bool,
    is_page_level: bool,
    ops: &[lopdf::content::Operation],
    _xobject_dict: &Dictionary,
    diagnostics: &mut Vec<String>,
    _visited: &mut BTreeSet<String>,
) -> Vec<RedactionOccurrence> {
    let mut out = Vec::new();
    let mut state = DrawState::default();
    let mut path = PathState::default();
    let mut pending_fill_rgb = None::<(f32, f32, f32)>;

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
                pending_fill_rgb = Some((state.fill_r, state.fill_g, state.fill_b));

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
                                "fill_rgb".to_string(),
                                format!(
                                    "{:.3},{:.3},{:.3}",
                                    state.fill_r, state.fill_g, state.fill_b
                                ),
                            );
                            meta.insert("path_kind".to_string(), op.operator.clone());
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
                                "fill_rgb".to_string(),
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

    let _ = pending_fill_rgb;
    let _ = diagnostics;
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
            .get(0)
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
            .get(0)
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
            .get(0)
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
    let x = operands.get(0).and_then(object_to_f32)?;
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
        let x = operands.get(0).and_then(object_to_f32);
        let y = operands.get(1).and_then(object_to_f32);
        match (x, y) {
            (Some(x), Some(y)) => {
                self.current = Some((x, y));
                self.start = Some((x, y));
                self.points.clear();
                self.points.push((x, y));
                self.closed = false;
                self
            }
            _ => self,
        }
    }

    fn line_to(mut self, operands: &[Object]) -> Self {
        let x = operands.get(0).and_then(object_to_f32);
        let y = operands.get(1).and_then(object_to_f32);
        match (x, y) {
            (Some(x), Some(y)) => {
                self.current = Some((x, y));
                self.points.push((x, y));
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

    let fy0 = (height.saturating_sub(y1) as f32) / height as f32;

    let fy1 = (height.saturating_sub(y0) as f32) / height as f32;

    Rect::new(fx0, fy0, fx1, fy1)
}

/// Map a rectangle in rendered-page pixel coordinates (top-left origin) to
/// PDF user-space coordinates, given the page size and DPI.
fn rect_pixels_to_pdf(
    x0_px: u32,
    y0_px: u32,
    x1_px: u32,
    y1_px: u32,
    page_width_pt: f32,
    page_height_pt: f32,
    dpi: f32,
) -> Rect {
    // Convert pixel positions to inches.
    let x0_in = x0_px as f32 / dpi;
    let x1_in = x1_px as f32 / dpi;
    let y0_in_from_top = y0_px as f32 / dpi;
    let y1_in_from_top = y1_px as f32 / dpi;

    // Convert inches to points.
    let x0_pt = (x0_in * 72.0).clamp(0.0, page_width_pt);
    let x1_pt = (x1_in * 72.0).clamp(0.0, page_width_pt);

    // Flip Y from top-origin image to bottom-origin PDF.
    let page_height_in = page_height_pt / 72.0;
    let y1_pt = ((page_height_in - y0_in_from_top) * 72.0).clamp(0.0, page_height_pt);
    let y0_pt = ((page_height_in - y1_in_from_top) * 72.0).clamp(0.0, page_height_pt);

    Rect::new(x0_pt, y0_pt, x1_pt, y1_pt)
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

    let mut sum = 0u64;
    let mut min_v = 255u8;
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

    let mut cell_sums = vec![0u64; rows * cols];
    let mut cell_area = vec![1u32; rows * cols];

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
                let mut acc = 0u64;
                for x in *x0..*x1 {
                    acc += gray[row_offset + x] as u64;
                }
                cell_sums[idx] += acc;
            }
        }
    }

    let mut cell_avg = vec![0f32; rows * cols];
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

    for idx in 0..cell_avg.len() {
        if visited[idx] || cell_avg[idx] > threshold {
            continue;
        }
        visited[idx] = true;
        queue.clear();
        queue.push_back(idx);

        let mut sum_lum = 0f32;
        let mut pixel_area = 0u64;
        let mut min_col = cols;
        let mut max_col = 0;
        let mut min_row = rows;
        let mut max_row = 0;

        while let Some(current) = queue.pop_front() {
            let row = current / cols;
            let col = current % cols;
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
                row.checked_sub(1).map(|r| r * cols + col),
                if row + 1 < rows {
                    Some((row + 1) * cols + col)
                } else {
                    None
                },
                col.checked_sub(1).map(|c| row * cols + c),
                if col + 1 < cols {
                    Some(row * cols + (col + 1))
                } else {
                    None
                },
            ];
            for n in neighbors.into_iter().flatten() {
                if visited[n] {
                    continue;
                }
                if cell_avg[n] > threshold {
                    continue;
                }
                visited[n] = true;
                queue.push_back(n);
            }
        }

        if pixel_area == 0 {
            continue;
        }
        let area_fraction = pixel_area as f32 / total_pixels as f32;
        if area_fraction < 0.0005 || area_fraction > 0.9 {
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
    let approx = ((size as f32) / 24.0).ceil() as usize;
    approx.clamp(4, 96).min(size.max(1))
}

fn build_bins(size: usize, target: usize) -> Vec<(usize, usize)> {
    if size == 0 {
        return vec![(0, 0)];
    }

    let bins = target.max(1).min(size);

    let mut result = Vec::with_capacity(bins);

    let mut start = 0usize;

    let mut remaining_bins = bins;

    let mut remaining = size;

    while remaining_bins > 0 {
        let chunk = (remaining + remaining_bins - 1) / remaining_bins;

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

fn page_size_from_page(doc: &Document, page_id: ObjectId) -> Option<(f32, f32)> {
    let page_obj = doc.get_object(page_id).ok()?;
    let dict = match page_obj {
        Object::Dictionary(d) => d,
        _ => return None,
    };
    let media_box = dict.get(b"MediaBox").ok()?;
    let rect = object_to_rect(media_box)?;
    Some((rect.width().abs(), rect.height().abs()))
}

/// Perform raster-based dark region detection on a fully rendered page bitmap
/// using the same detector as image XObjects, then map the result back into
/// PDF coordinates and produce `RedactionOccurrence`s.
fn extract_raster_page_redactions(
    renderer: &dyn PdfRenderer,
    page_index: u32,
    page_width_pt: f32,
    page_height_pt: f32,
    cfg: &RedactionFinderConfig,
) -> Result<Vec<RedactionOccurrence>, String> {
    let rendered = renderer
        .render_page_to_rgba(page_index as usize, cfg.raster_dpi)
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
        // Map pixel rects back into PDF user space.
        let page_rect = rect_pixels_to_pdf(
            det.x0_px,
            det.y0_px,
            det.x1_px,
            det.y1_px,
            page_width_pt,
            page_height_pt,
            rendered.dpi,
        );

        if rect_is_near_full_page_with_size(&page_rect, page_width_pt, page_height_pt) {
            continue;
        }
        if page_rect.width().abs() < 2.0 || page_rect.height().abs() < 2.0 {
            continue;
        }

        let darkness = (1.0 - det.avg_luminance / 255.0).clamp(0.0, 1.0);
        let mut score = (det.score * 0.7) + (darkness * 0.3);
        score = score.min(1.0);

        let mut meta: BTreeMap<String, String> = BTreeMap::new();
        if cfg.include_details {
            meta.insert("raster_dpi".to_string(), format!("{:.1}", rendered.dpi));
            meta.insert(
                "image_dims_px".to_string(),
                format!("{}x{}", rendered.width_px, rendered.height_px),
            );
            meta.insert(
                "region_area_fraction".to_string(),
                format!("{:.4}", det.area_fraction),
            );
            meta.insert(
                "region_avg_luminance".to_string(),
                format!("{:.1}", det.avg_luminance),
            );
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
        let r = px[0] as f32;
        let g = px[1] as f32;
        let b = px[2] as f32;
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        gray.push(y.clamp(0.0, 255.0) as u8);
    }
    gray
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn detects_dark_bar_in_synthetic_image() {
        let width = 60;
        let height = 32;
        let mut buf = vec![235u8; width * height];
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
        let mut buf = vec![210u8; width * height];
        for i in (0..width * height).step_by(1500) {
            buf[i] = 20;
        }
        let result = detect_dark_regions_in_image(&buf, width, height);
        assert!(result.detections.is_empty());
    }

    #[test]
    fn rect_pixels_to_pdf_maps_coords() {
        let mapped = rect_pixels_to_pdf(72, 72, 648, 648, 720.0, 720.0, 72.0);
        assert!((mapped.x0 - 72.0).abs() < 0.01);
        assert!((mapped.x1 - 648.0).abs() < 0.01);
        assert!((mapped.y0 - 72.0).abs() < 0.01);
        assert!((mapped.y1 - 648.0).abs() < 0.01);
    }
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
                let dx = op.operands.get(0).and_then(object_to_f32).unwrap_or(0.0);
                let dy = op.operands.get(1).and_then(object_to_f32).unwrap_or(0.0);
                st.tm_e += dx;
                st.tm_f += dy;
            }
            "Tj" | "TJ" | "'" | "\"" => {
                if !st.in_text {
                    continue;
                }
                let text = text_from_show_op(op);
                let text = text.trim().to_string();
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
        if let Some(Object::Array(a)) = op.operands.get(0) {
            return a
                .iter()
                .filter_map(|o| match o {
                    Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
        }
    }

    if let Some(Object::String(s, _)) = op.operands.last() {
        return String::from_utf8_lossy(s).to_string();
    }

    String::new()
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

fn deref_to_dict<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Dictionary> {
    match obj {
        Object::Reference(oid) => match doc.get_object(*oid).ok()? {
            Object::Dictionary(d) => Some(d),
            _ => None,
        },
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

fn deref_to_stream<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Stream> {
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
        Object::Real(r) => Some(*r as f32),
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

    let x0 = a.get(0).and_then(object_to_f32)?;

    let y0 = a.get(1).and_then(object_to_f32)?;

    let x1 = a.get(2).and_then(object_to_f32)?;

    let y1 = a.get(3).and_then(object_to_f32)?;

    Some(Rect::new(x0, y0, x1, y1))
}
