use crate::font_detection::logic::file_font_process::normalize_subset_font_name;
use crate::font_detection::logic::types::file_types::{FontAsset, FontRunReport, FontTextRun, Rect};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone)]
struct FontInfo {
    font_key: String,
    font_name: String,
    bytes: Option<Vec<u8>>,
    units_per_em: Option<u16>,
}

#[derive(Debug, Clone, Default)]
struct TextState {
    in_text: bool,
    font_key: String,
    font_name: String,
    font_size_pt: f32,
    text_matrix: Matrix,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Matrix {
    tx: f32,
    ty: f32,
}

impl Default for Matrix {
    fn default() -> Self {
        Self { tx: 0.0, ty: 0.0 }
    }
}

#[inline]
pub fn build_font_run_report(path: &Path, bytes: &[u8]) -> Result<FontRunReport, String> {
    let doc = Document::load_mem(bytes).map_err(|e| e.to_string())?;
    let pages = doc.get_pages();

    let mut runs = Vec::new();
    let mut assets_map: BTreeMap<String, FontAsset> = BTreeMap::new();

    for (page_no, page_id) in pages {
        let page_index = page_no.saturating_sub(1);
        let font_map = extract_page_font_info(&doc, page_id)?;
        for info in font_map.values() {
            if let Some(bytes) = &info.bytes {
                let units_per_em = info.units_per_em.unwrap_or(1000);
                assets_map.entry(info.font_key.clone()).or_insert(FontAsset {
                    font_key: info.font_key.clone(),
                    font_name: info.font_name.clone(),
                    units_per_em,
                    bytes: bytes.clone(),
                });
            }
        }
        let content = doc
            .get_page_content(page_id)
            .map_err(|e| format!("page_content_error={e}"))?;
        let decoded =
            lopdf::content::Content::decode(&content).map_err(|e| format!("page_decode_error={e}"))?;
        runs.extend(extract_text_runs(page_index, &decoded.operations, &font_map));
    }

    Ok(FontRunReport {
        input: path.to_string_lossy().to_string(),
        runs,
        assets: assets_map.into_values().collect(),
    })
}

fn extract_text_runs(
    page_index: u32,
    ops: &[lopdf::content::Operation],
    font_map: &BTreeMap<String, FontInfo>,
) -> Vec<FontTextRun> {
    let mut out = Vec::new();
    let mut st = TextState::default();

    for op in ops {
        match op.operator.as_str() {
            "BT" => st.in_text = true,
            "ET" => st.in_text = false,
            "Tf" => {
                let font_key = op
                    .operands
                    .first()
                    .and_then(object_to_name_string)
                    .unwrap_or_else(|| st.font_key.clone());
                let size = op
                    .operands
                    .get(1)
                    .and_then(object_to_f32)
                    .unwrap_or(st.font_size_pt);
                let info = font_map.get(&font_key);
                st.font_key = font_key.clone();
                st.font_name = info
                    .map(|i| i.font_name.clone())
                    .unwrap_or_else(|| font_key);
                st.font_size_pt = size;
            }
            "Tm" => {
                if let Some(tx) = op.operands.get(4).and_then(object_to_f32) {
                    st.text_matrix.tx = tx;
                }
                if let Some(ty) = op.operands.get(5).and_then(object_to_f32) {
                    st.text_matrix.ty = ty;
                }
            }
            "Td" | "TD" => {
                let dx = op.operands.first().and_then(object_to_f32).unwrap_or(0.0);
                let dy = op.operands.get(1).and_then(object_to_f32).unwrap_or(0.0);
                st.text_matrix.tx += dx;
                st.text_matrix.ty += dy;
            }
            "Tj" | "TJ" | "'" | "\"" => {
                if !st.in_text {
                    continue;
                }
                let text = text_from_show_op(op);
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let h = st.font_size_pt.abs().max(1.0);
                let w = (st.font_size_pt.abs() * 0.6 * (trimmed.chars().count().max(1) as f32))
                    .max(1.0);
                let x0 = st.text_matrix.tx;
                let y1 = st.text_matrix.ty;
                let bbox = Rect::new(x0, y1 - h, x0 + w, y1);
                out.push(FontTextRun {
                    page_index,
                    text: trimmed.to_owned(),
                    bbox,
                    font_key: st.font_key.clone(),
                    font_name: st.font_name.clone(),
                    font_size_pt: st.font_size_pt,
                });
            }
            _ => {}
        }
    }

    out
}

fn extract_page_font_info(
    doc: &Document,
    page_id: ObjectId,
) -> Result<BTreeMap<String, FontInfo>, String> {
    let (page_resources_opt, _unused_pages) = doc
        .get_page_resources(page_id)
        .map_err(|error| error.to_string())?;
    let resources = match page_resources_opt {
        None => return Ok(BTreeMap::new()),
        Some(resources_dict) => resources_dict,
    };

    let font_obj_opt = resources.get(b"Font").ok();
    let font_obj = match font_obj_opt {
        None => return Ok(BTreeMap::new()),
        Some(font_object) => font_object,
    };

    let font_dict_opt = deref_to_dict(doc, font_obj).or_else(|| object_to_dict(font_obj));
    let font_dict = match font_dict_opt {
        None => return Ok(BTreeMap::new()),
        Some(font_dictionary) => font_dictionary,
    };

    let map = font_dict
        .iter()
        .filter_map(|(key_bytes, value_object)| {
            let key = String::from_utf8_lossy(key_bytes).to_string();
            let dict = deref_to_dict(doc, value_object)?;
            let font_name = resolve_pdf_font_name(doc, dict).unwrap_or_else(|| key.clone());
            let (bytes, units_per_em) = extract_font_bytes(doc, dict);
            Some((
                key.clone(),
                FontInfo {
                    font_key: key,
                    font_name,
                    bytes,
                    units_per_em,
                },
            ))
        })
        .collect::<BTreeMap<_, _>>();

    Ok(map)
}

fn resolve_pdf_font_name(doc: &Document, dict: &Dictionary) -> Option<String> {
    let base = dict.get(b"BaseFont").ok().and_then(object_to_name_string);
    if let Some(base_font_name) = base {
        return Some(normalize_subset_font_name(&base_font_name));
    }

    let desc_obj = dict.get(b"FontDescriptor").ok();
    let desc_dict_opt = desc_obj.and_then(|descriptor_object| deref_to_dict(doc, descriptor_object));
    let desc_dict = desc_dict_opt?;
    let name = desc_dict
        .get(b"FontName")
        .ok()
        .and_then(object_to_name_string)?;
    Some(normalize_subset_font_name(&name))
}

fn extract_font_bytes(doc: &Document, dict: &Dictionary) -> (Option<Vec<u8>>, Option<u16>) {
    let desc_obj = match dict.get(b"FontDescriptor").ok() {
        Some(o) => o,
        None => return (None, None),
    };
    let desc = match deref_to_dict(doc, desc_obj) {
        Some(d) => d,
        None => return (None, None),
    };

    for key in [&b"FontFile"[..], &b"FontFile2"[..], &b"FontFile3"[..]] {
        if let Some(stream) = desc.get(key).ok().and_then(|o| deref_to_stream(doc, o)) {
            let bytes = stream.decompressed_content().ok();
            if let Some(b) = bytes {
                let units = ttf_parser::Face::parse(&b, 0)
                    .ok()
                    .map(|f| f.units_per_em());
                return (Some(b), units);
            }
        }
    }

    (None, None)
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

fn decode_pdf_text(obj: &Object) -> Option<String> {
    match obj {
        Object::String(raw_bytes, _) => {
            let decoded = lopdf::decode_text_string(obj)
                .ok()
                .filter(|text| !text.contains('\u{FFFD}'));
            match decoded {
                Some(text) => Some(text),
                None => Some(String::from_utf8_lossy(raw_bytes).to_string()),
            }
        }
        _ => None,
    }
}

fn object_to_f32(object: &Object) -> Option<f32> {
    match object {
        Object::Real(real_value) => Some(*real_value),
        Object::Integer(integer_value) => Some(*integer_value as f32),
        _ => None,
    }
}

fn object_to_name_string(object: &Object) -> Option<String> {
    match object {
        Object::Name(name_bytes) => Some(String::from_utf8_lossy(name_bytes).to_string()),
        _ => None,
    }
}

fn object_to_dict(object: &Object) -> Option<&Dictionary> {
    match object {
        Object::Dictionary(dictionary) => Some(dictionary),
        _ => None,
    }
}

fn deref_to_dict<'doc>(doc: &'doc Document, object: &'doc Object) -> Option<&'doc Dictionary> {
    match object {
        Object::Reference(object_id) => match doc.get_object(*object_id).ok()? {
            Object::Dictionary(d) => Some(d),
            _ => None,
        },
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

fn deref_to_stream<'doc>(doc: &'doc Document, object: &'doc Object) -> Option<&'doc Stream> {
    match object {
        Object::Reference(object_id) => match doc.get_object(*object_id).ok()? {
            Object::Stream(s) => Some(s),
            _ => None,
        },
        Object::Stream(s) => Some(s),
        _ => None,
    }
}
