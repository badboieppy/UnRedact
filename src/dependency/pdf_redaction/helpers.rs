use crate::types::redaction_types::Rect;
use lopdf::{Dictionary, Document, Object, Stream};

pub fn decode_pdf_text(obj: &Object) -> Option<String> {
    match obj {
        Object::String(raw_bytes, _) => {
            let decoded = lopdf::decode_text_string(obj)
                .ok()
                .filter(|text| !text.contains('\u{FFFD}'));
            match decoded {
                Some(text) => Some(normalize_decoded_text(&text)),
                None => Some(normalize_decoded_text(&String::from_utf8_lossy(raw_bytes))),
            }
        }
        _ => None,
    }
}

pub fn normalize_decoded_text(text: &str) -> String {
    text.chars()
        .map(|char_value| match char_value {
            '‰' | '‹' | '«' => '<',
            '›' | '»' => '>',
            _ => char_value,
        })
        .collect::<String>()
}

pub fn deref_to_array(doc: &Document, obj: &Object) -> Option<Vec<Object>> {
    match obj {
        Object::Reference(oid) => match doc.get_object(*oid).ok()? {
            Object::Array(a) => Some(a.clone()),
            _ => None,
        },
        Object::Array(a) => Some(a.clone()),
        _ => None,
    }
}

pub fn deref_to_dict<'doc>(doc: &'doc Document, obj: &'doc Object) -> Option<&'doc Dictionary> {
    match obj {
        Object::Reference(oid) => match doc.get_object(*oid).ok()? {
            Object::Dictionary(d) => Some(d),
            _ => None,
        },
        Object::Dictionary(d) => Some(d),
        _ => None,
    }
}

pub fn deref_to_stream<'doc>(doc: &'doc Document, obj: &'doc Object) -> Option<&'doc Stream> {
    match obj {
        Object::Reference(oid) => match doc.get_object(*oid).ok()? {
            Object::Stream(s) => Some(s),
            _ => None,
        },
        Object::Stream(s) => Some(s),
        _ => None,
    }
}

pub fn object_to_f32(o: &Object) -> Option<f32> {
    match o {
        Object::Real(r) => Some(*r),
        Object::Integer(i) => Some(*i as f32),
        _ => None,
    }
}

pub fn object_to_name_string(o: &Object) -> Option<String> {
    match o {
        Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        _ => None,
    }
}

pub fn object_to_string_lossy(o: &Object) -> Option<String> {
    match o {
        Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
        _ => None,
    }
}

pub fn object_to_rect(o: &Object) -> Option<Rect> {
    let a = match o {
        Object::Array(a) => a,
        _ => return None,
    };

    if a.len() != 4 {
        return None;
    }

    let x0 = a.first().and_then(object_to_f32)?;
    let y0 = a.get(1).and_then(object_to_f32)?;
    let x1 = a.get(2).and_then(object_to_f32)?;
    let y1 = a.get(3).and_then(object_to_f32)?;

    Some(Rect::new(x0, y0, x1, y1))
}
