use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::redaction_finder::types::Rect;

#[derive(Debug, Clone, Copy)]
pub struct PdfAnnotator;

impl PdfAnnotator {
    #[inline]
    pub fn annotate(
        &self,
        bytes: &[u8],
        rects: &[(u32, Rect)],
        color: [f32; 3],
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
