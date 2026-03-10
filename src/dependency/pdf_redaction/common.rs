use crate::types::redaction_types::Rect;
use lopdf::Object;
use std::collections::BTreeMap;

pub const MAX_RASTER_ANALYSIS_DPI: f32 = 120.0;

#[derive(Debug, Clone, Copy)]
pub struct DetailPolicy {
    pub include_details: bool,
}

impl DetailPolicy {
    #[inline]
    pub fn new(include_details: bool) -> Self {
        Self { include_details }
    }

    #[inline]
    pub fn new_meta(self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    #[inline]
    pub fn insert_owned(self, meta: &mut BTreeMap<String, String>, key: &str, value: String) {
        if self.include_details && !value.is_empty() {
            meta.insert(key.to_owned(), value);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DrawScanOptions {
    pub detail: DetailPolicy,
    pub include_full_page_rects: bool,
    pub is_page_level: bool,
}

impl DrawScanOptions {
    #[inline]
    pub fn page_level(include_details: bool, include_full_page_rects: bool) -> Self {
        Self {
            detail: DetailPolicy::new(include_details),
            include_full_page_rects,
            is_page_level: true,
        }
    }

    #[inline]
    pub fn nested(self) -> Self {
        Self {
            is_page_level: false,
            ..self
        }
    }
}

pub fn rect_from_re(operands: &[Object]) -> Option<Rect> {
    let x = operands.first().and_then(object_to_f32)?;
    let y = operands.get(1).and_then(object_to_f32)?;
    let w = operands.get(2).and_then(object_to_f32)?;
    let h = operands.get(3).and_then(object_to_f32)?;

    if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
        return None;
    }

    Some(Rect::new(x, y, x + w, y + h))
}

pub fn rect_is_near_full_page(r: &Rect) -> bool {
    let w = r.width().abs();
    let h = r.height().abs();

    if w <= 0.0 || h <= 0.0 {
        return false;
    }

    w >= 500.0 && h >= 650.0
}

pub fn rect_is_near_full_page_with_size(r: &Rect, page_width_pt: f32, page_height_pt: f32) -> bool {
    let w = r.width().abs();
    let h = r.height().abs();
    if w <= 0.0 || h <= 0.0 || page_width_pt <= 0.0 || page_height_pt <= 0.0 {
        return false;
    }
    let frac_w = w / page_width_pt;
    let frac_h = h / page_height_pt;
    frac_w >= 0.9 && frac_h >= 0.9
}

pub fn score_rect_as_redaction(r: &Rect) -> f32 {
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

pub fn normalized_rect_from_pixels(
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
    let fy0 = y0 as f32 / height as f32;
    let fy1 = y1 as f32 / height as f32;

    Rect::new(fx0, fy0, fx1, fy1)
}

pub fn rect_pixels_to_pdf(
    x0_px: u32,
    y0_px: u32,
    x1_px: u32,
    y1_px: u32,
    page_box: Rect,
    dpi: f32,
) -> Rect {
    let x0_in = x0_px as f32 / dpi;
    let x1_in = x1_px as f32 / dpi;
    let y0_in_from_top = y0_px as f32 / dpi;
    let y1_in_from_top = y1_px as f32 / dpi;

    let x0_pt = (page_box.x0 + (x0_in * 72.0)).clamp(page_box.x0, page_box.x1);
    let x1_pt = (page_box.x0 + (x1_in * 72.0)).clamp(page_box.x0, page_box.x1);

    let y1_pt = (page_box.y1 - (y0_in_from_top * 72.0)).clamp(page_box.y0, page_box.y1);
    let y0_pt = (page_box.y1 - (y1_in_from_top * 72.0)).clamp(page_box.y0, page_box.y1);

    Rect::new(x0_pt, y0_pt, x1_pt, y1_pt)
}

fn object_to_f32(o: &Object) -> Option<f32> {
    match o {
        Object::Real(r) => Some(*r),
        Object::Integer(i) => Some(*i as f32),
        _ => None,
    }
}
