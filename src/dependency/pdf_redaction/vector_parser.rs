use crate::types::redaction_types::{Rect, RedactionKind, RedactionOccurrence};
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::BTreeSet;

use crate::dependency::pdf_redaction::{
    deref_to_dict, deref_to_stream, object_to_f32, object_to_name_string, rect_from_re,
    rect_is_near_full_page, score_rect_as_redaction, DrawScanOptions,
};

pub fn extract_page_drawn_redactions(
    doc: &Document,
    page_id: ObjectId,
    page_index: u32,
    options: DrawScanOptions,
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
        options,
        &decoded.operations,
        &xobject,
        &mut diagnostics,
        &mut visited,
    );

    let mut xo = extract_from_xobjects(
        doc,
        page_index,
        options,
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
    options: DrawScanOptions,
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
            options.nested(),
            &decoded.operations,
            xobject_dict,
            diagnostics,
            visited,
        );
        out.append(&mut sub);

        let mut nested = extract_from_xobjects(
            doc,
            page_index,
            options.nested(),
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
    options: DrawScanOptions,
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
                        && (options.include_full_page_rects
                            || !options.is_page_level
                            || !rect_is_near_full_page(&rect));

                    if keep {
                        let mut meta = options.detail.new_meta();
                        options.detail.insert_owned(
                            &mut meta,
                            "fill_rgb",
                            format!("{:.3},{:.3},{:.3}", state.fill_r, state.fill_g, state.fill_b),
                        );
                        options
                            .detail
                            .insert_owned(&mut meta, "path_kind", op.operator.clone());
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
                        && (options.include_full_page_rects
                            || !options.is_page_level
                            || !rect_is_near_full_page(&rect));

                    if keep {
                        let mut meta = options.detail.new_meta();
                        options.detail.insert_owned(
                            &mut meta,
                            "fill_rgb",
                            format!("{:.3},{:.3},{:.3}", state.fill_r, state.fill_g, state.fill_b),
                        );
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
        let r = operands.first().and_then(object_to_f32).unwrap_or(self.fill_r);
        let g = operands.get(1).and_then(object_to_f32).unwrap_or(self.fill_g);
        let b = operands.get(2).and_then(object_to_f32).unwrap_or(self.fill_b);
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
        let c = operands.first().and_then(object_to_f32).unwrap_or(self.fill_c);
        let m = operands.get(1).and_then(object_to_f32).unwrap_or(self.fill_m);
        let y = operands.get(2).and_then(object_to_f32).unwrap_or(self.fill_y);
        let k = operands.get(3).and_then(object_to_f32).unwrap_or(self.fill_k);
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
