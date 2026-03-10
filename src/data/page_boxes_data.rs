use std::collections::BTreeMap;

use lopdf::{Document, Object, ObjectId};

use crate::types::redaction_types::Rect;

pub const DEFAULT_PAGE_BOX: Rect = Rect {
    x0: 0.0_f32,
    y0: 0.0_f32,
    x1: 612.0_f32,
    y1: 792.0_f32,
};

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
