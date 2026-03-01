use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::types::redaction_types::Rect;
use crate::types::text_overlay::TextOverlay;

#[derive(Debug, Clone, Copy)]
pub struct PdfAnnotator;

const OVERLAY_MULTILINE_LEADING_RATIO: f32 = 1.15_f32;

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
        Ok(Object::Reference(oid)) => {
            Object::Array(vec![Object::Reference(*oid), Object::Reference(stream_id)])
        }
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
    let normalized = normalize_overlay_line_breaks(&overlay.text);
    let mut lines = normalized.split('\n');
    let first_line = lines.next().unwrap_or_default();
    let mut text_ops = format!("({}) Tj", escape_pdf_string(first_line));
    let line_step_pt = overlay.font_size_pt.max(1.0_f32) * OVERLAY_MULTILINE_LEADING_RATIO;
    for line in lines {
        text_ops.push_str(&format!(
            " 0 -{} Td ({}) Tj",
            line_step_pt,
            escape_pdf_string(line),
        ));
    }
    let font = format!("/{}", overlay.font_key.trim_start_matches('/'));
    let clip_x = overlay.bbox.x0.min(overlay.bbox.x1);
    let clip_y = overlay.bbox.y0.min(overlay.bbox.y1);
    let clip_w = (overlay.bbox.x1 - overlay.bbox.x0).abs().max(0.01_f32);
    let clip_h = (overlay.bbox.y1 - overlay.bbox.y0).abs().max(0.01_f32);
    format!(
        "q {} {} {} rg {} {} {} {} re W n BT {} Tz {} {} Tf 1 0 0 1 {} {} Tm {} ET Q",
        color[0],
        color[1],
        color[2],
        clip_x,
        clip_y,
        clip_w,
        clip_h,
        overlay.h_scale_pct.max(1.0),
        font,
        overlay.font_size_pt.max(1.0),
        overlay.x,
        overlay.y,
        text_ops
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

fn normalize_overlay_line_breaks(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::build_text_ops;
    use crate::types::redaction_types::Rect;
    use crate::types::text_overlay::TextOverlay;

    #[test]
    fn build_text_ops_emits_line_steps_for_multiline_text() {
        let overlay = TextOverlay {
            redaction_index: Some(0),
            page_index: 0,
            text: "NADIA\r\nMARCINKOVA".to_owned(),
            font_key: "F1".to_owned(),
            font_size_pt: 11.0_f32,
            h_scale_pct: 100.0_f32,
            x: 100.0_f32,
            y: 200.0_f32,
            bbox: Rect::new(100.0_f32, 190.0_f32, 200.0_f32, 210.0_f32),
        };

        let ops = build_text_ops(&overlay, [0.0_f32, 0.4_f32, 1.0_f32]);
        assert!(ops.contains("(NADIA) Tj"));
        assert!(ops.contains("(MARCINKOVA) Tj"));
        assert!(
            ops.contains(" Td "),
            "multiline overlay should emit baseline step operations"
        );
        assert!(
            !ops.contains("\\n"),
            "multiline overlay should not render escaped newline characters"
        );
        assert!(
            ops.contains(" re W n "),
            "overlay text operations should clip drawing to overlay bbox"
        );
    }
}
