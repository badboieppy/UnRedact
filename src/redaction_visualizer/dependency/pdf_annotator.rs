use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::redaction_finder::types::Rect;
use crate::redaction_visualizer::types::TextOverlay;

#[derive(Debug, Clone, Copy)]
pub struct PdfAnnotator;

impl PdfAnnotator {
    #[inline]
    pub fn annotate(
        &self,
        bytes: &[u8],
        rects: &[(u32, Rect)],
        overlays: &[TextOverlay],
        color: [f32; 3],
        text_color: [f32; 3],
        border_width: f32,
    ) -> Result<Vec<u8>, String> {
        let mut doc = Document::load_mem(bytes).map_err(|e| e.to_string())?;
        let page_map = doc.get_pages();

        let mut rects_by_page: std::collections::BTreeMap<u32, Vec<Rect>> =
            std::collections::BTreeMap::new();
        for (page_index, rect) in rects {
            rects_by_page.entry(*page_index).or_default().push(*rect);
        }

        let mut overlays_by_page: std::collections::BTreeMap<u32, Vec<&TextOverlay>> =
            std::collections::BTreeMap::new();
        for overlay in overlays {
            overlays_by_page
                .entry(overlay.page_index)
                .or_default()
                .push(overlay);
        }

        let mut pages: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        pages.extend(rects_by_page.keys().copied());
        pages.extend(overlays_by_page.keys().copied());

        for page_index in pages {
            let page_no = page_index.saturating_add(1);
            let page_id = match page_map.get(&page_no) {
                Some(id) => *id,
                None => continue,
            };
            let mut content = Vec::new();
            if let Some(rect_items) = rects_by_page.get(&page_index) {
                for rect in rect_items {
                    let line = build_rect_ops(*rect, color, border_width);
                    content.extend_from_slice(line.as_bytes());
                    content.push(b'\n');
                }
            }
            if let Some(overlay_items) = overlays_by_page.get(&page_index) {
                for overlay in overlay_items {
                    let line = build_text_ops(overlay, text_color);
                    content.extend_from_slice(line.as_bytes());
                    content.push(b'\n');
                }
            }
            if !content.is_empty() {
                add_page_content(&mut doc, page_id, &content)?;
            }
        }

        let mut out = Vec::new();
        doc.save_to(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }
}

fn add_page_content(doc: &mut Document, page_id: ObjectId, content: &[u8]) -> Result<(), String> {
    let stream = Stream::new(Dictionary::new(), content.to_vec());
    let stream_id = doc.new_object_id();
    doc.objects.insert(stream_id, Object::Stream(stream));

    let page_obj = doc.get_object_mut(page_id).map_err(|e| e.to_string())?;
    let dict = match page_obj {
        Object::Dictionary(d) => d,
        _ => return Ok(()),
    };
    let new_contents = match dict.get(b"Contents") {
        Ok(Object::Reference(oid)) => Object::Array(vec![Object::Reference(*oid), Object::Reference(stream_id)]),
        Ok(Object::Array(existing)) => {
            let mut arr = existing.clone();
            arr.push(Object::Reference(stream_id));
            Object::Array(arr)
        }
        Ok(Object::Stream(_)) => Object::Array(vec![Object::Reference(stream_id)]),
        _ => Object::Array(vec![Object::Reference(stream_id)]),
    };
    dict.set("Contents", new_contents);
    Ok(())
}

fn build_text_ops(overlay: &TextOverlay, color: [f32; 3]) -> String {
    let escaped = escape_pdf_string(&overlay.text);
    let font = format!("/{}", overlay.font_key.trim_start_matches('/'));
    format!(
        "q {} {} {} rg BT {} {} Tf 1 0 0 1 {} {} Tm ({}) Tj ET Q",
        color[0],
        color[1],
        color[2],
        font,
        overlay.font_size_pt.max(1.0),
        overlay.x,
        overlay.y,
        escaped
    )
}

fn build_rect_ops(rect: Rect, color: [f32; 3], border_width: f32) -> String {
    let w = (rect.x1 - rect.x0).abs();
    let h = (rect.y1 - rect.y0).abs();
    format!(
        "q {} {} {} RG {} w {} {} {} {} re S Q",
        color[0],
        color[1],
        color[2],
        border_width.max(0.1),
        rect.x0,
        rect.y0,
        w,
        h
    )
}

fn escape_pdf_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}
