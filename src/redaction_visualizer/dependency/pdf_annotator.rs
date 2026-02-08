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

        for (page_index, rect) in rects {
            let page_no = page_index.saturating_add(1);
            let page_id = match page_map.get(&page_no) {
                Some(id) => *id,
                None => continue,
            };
            let annot_id = doc.new_object_id();
            let annot = annotation_dict(*rect, color, border_width);
            doc.objects
                .insert(annot_id, Object::Dictionary(annot));
            attach_annotation(&mut doc, page_id, annot_id)?;
        }

        let mut by_page: std::collections::BTreeMap<u32, Vec<&TextOverlay>> =
            std::collections::BTreeMap::new();
        for overlay in overlays {
            by_page.entry(overlay.page_index).or_default().push(overlay);
        }

        for (page_index, items) in by_page {
            let page_no = page_index.saturating_add(1);
            let page_id = match page_map.get(&page_no) {
                Some(id) => *id,
                None => continue,
            };
            let mut content = Vec::new();
            for overlay in items {
                let line = build_text_ops(
                    overlay,
                    text_color,
                );
                content.extend_from_slice(line.as_bytes());
                content.push(b'\n');
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

fn annotation_dict(rect: Rect, color: [f32; 3], border_width: f32) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"Annot".to_vec()));
    dict.set("Subtype", Object::Name(b"Square".to_vec()));
    dict.set(
        "Rect",
        Object::Array(vec![
            Object::Real(rect.x0),
            Object::Real(rect.y0),
            Object::Real(rect.x1),
            Object::Real(rect.y1),
        ]),
    );
    dict.set(
        "C",
        Object::Array(vec![
            Object::Real(color[0]),
            Object::Real(color[1]),
            Object::Real(color[2]),
        ]),
    );
    dict.set(
        "Border",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(border_width),
        ]),
    );
    dict.set("F", Object::Integer(4));
    dict
}

fn attach_annotation(doc: &mut Document, page_id: ObjectId, annot_id: ObjectId) -> Result<(), String> {
    let new_annots = build_annots_object(doc, page_id, annot_id)?;

    let page_obj = doc.get_object_mut(page_id).map_err(|e| e.to_string())?;
    let dict = match page_obj {
        Object::Dictionary(d) => d,
        _ => return Ok(()),
    };
    dict.set("Annots", new_annots);
    Ok(())
}

fn build_annots_object(
    doc: &Document,
    page_id: ObjectId,
    annot_id: ObjectId,
) -> Result<Object, String> {
    let page_obj = doc.get_object(page_id).map_err(|e| e.to_string())?;
    let dict = match page_obj {
        Object::Dictionary(d) => d,
        _ => return Ok(Object::Array(vec![Object::Reference(annot_id)])),
    };

    let mut out = Vec::new();
    match dict.get(b"Annots") {
        Ok(Object::Array(a)) => out.extend(a.iter().cloned()),
        Ok(Object::Reference(oid)) => {
            if let Ok(Object::Array(a)) = doc.get_object(*oid) {
                out.extend(a.iter().cloned());
            }
        }
        _ => {}
    }
    out.push(Object::Reference(annot_id));
    Ok(Object::Array(out))
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
