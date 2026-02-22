use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Document, Object, ObjectId};

use crate::dependency::hayro_renderer::HayroRenderer;
use crate::dependency::pdf_annotator::PdfAnnotator;
use crate::types::redaction_types::{PdfRenderer as _, Rect, RenderedPage};
use crate::types::text_overlay::TextOverlay;

pub const DEFAULT_PAGE_BOX: Rect = Rect {
    x0: 0.0_f32,
    y0: 0.0_f32,
    x1: 612.0_f32,
    y1: 792.0_f32,
};

#[inline]
pub fn annotate_overlays(
    pdf_bytes: &[u8],
    overlays: &[TextOverlay],
    color: [f32; 3],
    border_width: f32,
) -> Result<Vec<u8>, String> {
    PdfAnnotator.annotate(pdf_bytes, &[], overlays, color, color, border_width)
}

#[inline]
pub fn render_pages_to_rgba(
    pdf_bytes: &[u8],
    page_indices: &BTreeSet<u32>,
    dpi: f32,
) -> Result<BTreeMap<u32, RenderedPage>, String> {
    let renderer = HayroRenderer::new_from_bytes(pdf_bytes)?;
    let mut pages = BTreeMap::<u32, RenderedPage>::new();
    for page_index in page_indices {
        let page = renderer.render_page_to_rgba(*page_index as usize, dpi)?;
        pages.insert(*page_index, page);
    }
    Ok(pages)
}

#[inline]
pub fn build_page_boxes(pdf_bytes: &[u8]) -> Result<BTreeMap<u32, Rect>, String> {
    let doc = Document::load_mem(pdf_bytes).map_err(|error| error.to_string())?;
    let mut boxes = BTreeMap::<u32, Rect>::new();
    for (page_no, page_id) in doc.get_pages() {
        let page_index = page_no.saturating_sub(1);
        let page_box = page_render_box_from_page(&doc, page_id).unwrap_or(DEFAULT_PAGE_BOX);
        boxes.insert(page_index, page_box);
    }
    Ok(boxes)
}

pub fn apply_page_crop_boxes(
    pdf_bytes: &[u8],
    page_crop_boxes: &BTreeMap<u32, Rect>,
) -> Result<Vec<u8>, String> {
    if page_crop_boxes.is_empty() {
        return Ok(pdf_bytes.to_vec());
    }
    let mut doc = Document::load_mem(pdf_bytes).map_err(|error| error.to_string())?;
    for (page_no, page_id) in doc.get_pages() {
        let page_index = page_no.saturating_sub(1);
        let Some(crop) = page_crop_boxes.get(&page_index).copied() else {
            continue;
        };
        let page_object = doc
            .get_object_mut(page_id)
            .map_err(|error| error.to_string())?;
        let page_dict = match page_object {
            Object::Dictionary(value) => value,
            _ => continue,
        };
        let box_array = Object::Array(vec![
            Object::Real(crop.x0),
            Object::Real(crop.y0),
            Object::Real(crop.x1),
            Object::Real(crop.y1),
        ]);
        page_dict.set("CropBox", box_array.clone());
        page_dict.set("MediaBox", box_array);
    }
    let mut out = Vec::<u8>::new();
    doc.save_to(&mut out).map_err(|error| error.to_string())?;
    Ok(out)
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
        let object = doc.get_object(current_id).ok()?;
        let dict = match object {
            Object::Dictionary(value) => value,
            _ => return None,
        };

        if let Ok(value) = dict.get(key) {
            if let Some(rect) = object_to_rect_resolved(doc, value) {
                return Some(rect);
            }
        }

        let parent = match dict.get(b"Parent").ok()? {
            Object::Reference(parent_id) => *parent_id,
            _ => return None,
        };
        current_id = parent;
    }
}

fn object_to_rect_resolved(doc: &Document, object: &Object) -> Option<Rect> {
    match object {
        Object::Reference(object_id) => doc.get_object(*object_id).ok().and_then(object_to_rect),
        _ => object_to_rect(object),
    }
}

fn object_to_rect(object: &Object) -> Option<Rect> {
    let values = match object {
        Object::Array(items) => items,
        _ => return None,
    };
    if values.len() < 4 {
        return None;
    }
    let x0 = object_to_f32(values.first()?)?;
    let y0 = object_to_f32(values.get(1)?)?;
    let x1 = object_to_f32(values.get(2)?)?;
    let y1 = object_to_f32(values.get(3)?)?;
    Some(Rect::new(x0, y0, x1, y1))
}

fn object_to_f32(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => None,
    }
}
