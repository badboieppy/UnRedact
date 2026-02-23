use crate::types::redaction_types::{RedactionKind, RedactionOccurrence};
use lopdf::{Document, Object, ObjectId};

use crate::dependency::pdf_redaction::{
    deref_to_array, deref_to_dict, object_to_name_string, object_to_rect, object_to_string_lossy,
    score_rect_as_redaction, DetailPolicy,
};

pub fn extract_annotation_redactions(
    doc: &Document,
    page_id: ObjectId,
    page_index: u32,
    detail: DetailPolicy,
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

        let mut meta = detail.new_meta();
        detail.insert_owned(&mut meta, "Subtype", subtype);
        detail.insert_owned(&mut meta, "RT", rt);
        detail.insert_owned(&mut meta, "IT", it);
        detail.insert_owned(&mut meta, "FT", ft);
        detail.insert_owned(&mut meta, "NM", nm);
        detail.insert_owned(&mut meta, "Contents", contents);

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
