use crate::types::redaction_types::{Rect, UnderlyingTextHit};
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::BTreeSet;

use crate::dependency::pdf_redaction::{
    decode_pdf_text, deref_to_dict, deref_to_stream, object_to_f32, object_to_name_string,
};

pub fn extract_page_text_runs(
    doc: &Document,
    page_id: ObjectId,
    page_index: u32,
) -> Result<Vec<UnderlyingTextHit>, String> {
    let page_obj = doc.get_object(page_id).map_err(|e| e.to_string())?;
    let page_dict = match page_obj {
        Object::Dictionary(d) => d.clone(),
        _ => return Ok(Vec::new()),
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

    let content = doc
        .get_page_content(page_id)
        .map_err(|e| format!("page_content_error={e}"))?;
    let decoded =
        lopdf::content::Content::decode(&content).map_err(|e| format!("page_decode_error={e}"))?;

    let mut out = extract_text_runs(page_index, &decoded.operations);
    let mut visited = BTreeSet::<String>::new();
    let mut nested = extract_text_runs_from_xobjects(
        doc,
        page_index,
        &decoded.operations,
        &xobject,
        &mut visited,
    );
    out.append(&mut nested);
    Ok(out)
}

fn extract_text_runs_from_xobjects(
    doc: &Document,
    page_index: u32,
    ops: &[lopdf::content::Operation],
    xobject_dict: &Dictionary,
    visited: &mut BTreeSet<String>,
) -> Vec<UnderlyingTextHit> {
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

        let key = format!("page_index={page_index}:text_xobject={name}");
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
            Err(_) => continue,
        };

        let decoded = match lopdf::content::Content::decode(&content) {
            Ok(c) => c,
            Err(_) => continue,
        };

        out.extend(extract_text_runs(page_index, &decoded.operations));

        let nested_xobject = stream
            .dict
            .get(b"Resources")
            .ok()
            .and_then(|o| deref_to_dict(doc, o))
            .and_then(|res| res.get(b"XObject").ok())
            .and_then(|o| deref_to_dict(doc, o))
            .cloned()
            .unwrap_or_else(|| xobject_dict.clone());

        let mut nested = extract_text_runs_from_xobjects(
            doc,
            page_index,
            &decoded.operations,
            &nested_xobject,
            visited,
        );
        out.append(&mut nested);
    }

    out
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
                let dx = op.operands.first().and_then(object_to_f32).unwrap_or(0.0);
                let dy = op.operands.get(1).and_then(object_to_f32).unwrap_or(0.0);
                st.tm_e += dx;
                st.tm_f += dy;
            }
            "Tj" | "TJ" | "'" | "\"" => {
                if !st.in_text {
                    continue;
                }
                let raw_text = text_from_show_op(op);
                let text = raw_text.trim().to_owned();
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
        if let Some(Object::Array(a)) = op.operands.first() {
            return a
                .iter()
                .filter_map(decode_pdf_text)
                .collect::<Vec<_>>()
                .join("");
        }
    }

    if let Some(text) = op.operands.last().and_then(decode_pdf_text) {
        return text;
    }

    String::new()
}
