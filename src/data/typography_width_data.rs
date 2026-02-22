use lopdf::{Dictionary, Document, Object};
use std::collections::BTreeMap;

use crate::types::file_types::FontAsset;
use crate::types::runtime_defaults::GLYPH_UNITS_SCALE;
use crate::types::typography_shaping::shaping_features;
use crate::types::typography_types::{
    TypographyMeasureInput, TypographyMeasuredWidth, TypographyProfile, TypographyWidthSource,
    TypographyWidthTable, TypographyWidthTableKey,
};

#[inline]
pub fn build_typography_profile_from_pdf_bytes(
    pdf_bytes: &[u8],
) -> Result<TypographyProfile, String> {
    let doc = Document::load_mem(pdf_bytes).map_err(|error| error.to_string())?;
    let pages = doc.get_pages();
    let mut width_tables = BTreeMap::<TypographyWidthTableKey, TypographyWidthTable>::new();

    for (page_no, page_id) in pages {
        let page_index = page_no.saturating_sub(1);
        let (resources_opt, _unused_pages) = doc
            .get_page_resources(page_id)
            .map_err(|error| error.to_string())?;
        let resources = match resources_opt {
            Some(resources) => resources,
            None => continue,
        };
        let font_object = match resources.get(b"Font").ok() {
            Some(object) => object,
            None => continue,
        };
        let font_dict = match deref_to_width_dict(&doc, font_object)
            .or_else(|| object_to_width_dict(font_object))
        {
            Some(dictionary) => dictionary,
            None => continue,
        };
        for (key_bytes, value_object) in font_dict.iter() {
            let font_key = String::from_utf8_lossy(key_bytes).to_string();
            let dict = match deref_to_width_dict(&doc, value_object)
                .or_else(|| object_to_width_dict(value_object))
            {
                Some(dictionary) => dictionary,
                None => continue,
            };
            let width_dict = match resolve_width_target_dict(&doc, dict) {
                Some(dictionary) => dictionary,
                None => continue,
            };
            let first_char = width_dict
                .get(b"FirstChar")
                .ok()
                .and_then(object_to_width_u16);
            let widths = width_dict
                .get(b"Widths")
                .ok()
                .and_then(object_to_width_f64_array);
            let (first_char, widths) = match (first_char, widths) {
                (Some(first), Some(widths)) if !widths.is_empty() => (first, widths),
                _ => continue,
            };
            width_tables.insert(
                TypographyWidthTableKey {
                    page_index,
                    font_key,
                },
                TypographyWidthTable { first_char, widths },
            );
        }
    }

    Ok(TypographyProfile::from_width_tables(width_tables))
}

#[inline]
pub fn measure_text_width_from_profile(
    input: &TypographyMeasureInput<'_>,
    asset: Option<&FontAsset>,
    profile: &TypographyProfile,
) -> Option<TypographyMeasuredWidth> {
    if input.text.is_empty() {
        return Some(measured_width_from_points(
            0.0_f64,
            input.metrics_dpi,
            TypographyWidthSource::Fallback,
        ));
    }

    if let Some(asset_value) = asset {
        if let Some(width) = measure_text_width_pt(
            asset_value,
            input.text,
            input.font_size_pt,
            input.h_scale_pct,
            input.metrics_dpi,
        ) {
            if width.pt.is_finite() && width.pt > 0.0_f64 {
                return Some(width);
            }
        }
    }

    if let Some(table) = profile.width_table(input.page_index, input.font_key) {
        if let Some(width) = width_from_table(table, input.text, input.font_size_pt) {
            if width.is_finite() && width > 0.0_f64 {
                let scale = (input.h_scale_pct as f64 / 100.0_f64).max(0.01_f64);
                return Some(measured_width_from_points(
                    width * scale,
                    input.metrics_dpi,
                    TypographyWidthSource::PdfWidthTable,
                ));
            }
        }
    }

    width_from_core_font(input.font_name, input.text, input.font_size_pt).and_then(|width| {
        let scale = (input.h_scale_pct as f64 / 100.0_f64).max(0.01_f64);
        let width_pt = width * scale;
        (width_pt.is_finite() && width_pt > 0.0_f64).then_some(measured_width_from_points(
            width_pt,
            input.metrics_dpi,
            TypographyWidthSource::CoreFont,
        ))
    })
}

#[inline]
pub fn fallback_typography_width(
    text: &str,
    fallback_char_width: f64,
    fallback_space_width: f64,
    metrics_dpi: f32,
) -> TypographyMeasuredWidth {
    let width_pt = text
        .chars()
        .map(|ch| {
            if ch.is_whitespace() {
                fallback_space_width
            } else {
                fallback_char_width.max(0.1_f64)
            }
        })
        .sum::<f64>();
    measured_width_from_points(width_pt, metrics_dpi, TypographyWidthSource::Fallback)
}

fn measure_text_width_pt(
    asset: &FontAsset,
    text: &str,
    font_size_pt: f32,
    h_scale_pct: f32,
    metrics_dpi: f32,
) -> Option<TypographyMeasuredWidth> {
    let face = rustybuzz::Face::from_slice(&asset.bytes, 0)?;
    let units_per_em = asset.units_per_em.max(1) as f32;
    let scale = (h_scale_pct as f64 / 100.0_f64).max(0.01_f64);
    let font_size = font_size_pt.abs().max(1.0_f32);
    let width_pt = (advance_pt(&face, text, font_size, units_per_em) as f64) * scale;
    if !width_pt.is_finite() || width_pt <= 0.0_f64 {
        return None;
    }
    Some(measured_width_from_points(
        width_pt,
        metrics_dpi,
        TypographyWidthSource::Asset,
    ))
}

fn measured_width_from_points(
    width_pt: f64,
    _metrics_dpi: f32,
    source: TypographyWidthSource,
) -> TypographyMeasuredWidth {
    TypographyMeasuredWidth {
        pt: width_pt,
        source,
    }
}

fn resolve_width_target_dict<'a>(
    doc: &'a Document,
    dict: &'a Dictionary,
) -> Option<&'a Dictionary> {
    if dict.has(b"Widths") {
        return Some(dict);
    }

    let subtype = dict.get(b"Subtype").ok().and_then(object_to_width_name);
    if subtype.as_deref() == Some("Type0") {
        let descendants = dict
            .get(b"DescendantFonts")
            .ok()
            .and_then(object_to_width_array);
        let first = descendants
            .and_then(|array| array.first())
            .and_then(|object| deref_to_width_dict(doc, object));
        if let Some(descendant) = first {
            if descendant.has(b"Widths") {
                return Some(descendant);
            }
        }
    }

    None
}

fn width_from_table(table: &TypographyWidthTable, text: &str, font_size_pt: f32) -> Option<f64> {
    let mut sum = 0.0_f64;
    let mut any = false;
    for ch in text.chars() {
        let codepoint = ch as u32;
        if codepoint > u16::MAX as u32 {
            continue;
        }
        let codepoint = codepoint as u16;
        if codepoint < table.first_char {
            continue;
        }
        let index = (codepoint - table.first_char) as usize;
        if index >= table.widths.len() {
            continue;
        }
        sum += table.widths[index] * ((font_size_pt as f64) / 1000.0_f64);
        any = true;
    }
    any.then_some(sum)
}

fn width_from_core_font(font_name: &str, text: &str, font_size_pt: f32) -> Option<f64> {
    let normalized = font_name.to_ascii_lowercase();
    let table: fn(char) -> i32 = if normalized.contains("times") && normalized.contains("roman") {
        times_roman_width as fn(char) -> i32
    } else if normalized.contains("helvetica") {
        helvetica_width as fn(char) -> i32
    } else {
        return None;
    };
    let mut total_units = 0.0_f64;
    for ch in text.chars() {
        total_units += table(ch) as f64;
    }
    Some(total_units * ((font_size_pt as f64) / 1000.0_f64))
}

fn object_to_width_u16(object: &Object) -> Option<u16> {
    match object {
        Object::Integer(value) => (*value).try_into().ok(),
        Object::Real(value) => (*value as i64).try_into().ok(),
        _ => None,
    }
}

fn object_to_width_f64(object: &Object) -> Option<f64> {
    match object {
        Object::Real(value) => Some(*value as f64),
        Object::Integer(value) => Some(*value as f64),
        _ => None,
    }
}

fn object_to_width_f64_array(object: &Object) -> Option<Vec<f64>> {
    match object {
        Object::Array(values) => {
            let mut out = Vec::with_capacity(values.len());
            for item in values {
                if let Some(value) = object_to_width_f64(item) {
                    out.push(value);
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn object_to_width_array(object: &Object) -> Option<&Vec<Object>> {
    match object {
        Object::Array(values) => Some(values),
        _ => None,
    }
}

fn object_to_width_name(object: &Object) -> Option<String> {
    match object {
        Object::Name(name_bytes) => Some(String::from_utf8_lossy(name_bytes).to_string()),
        _ => None,
    }
}

fn object_to_width_dict(object: &Object) -> Option<&Dictionary> {
    match object {
        Object::Dictionary(dictionary) => Some(dictionary),
        _ => None,
    }
}

fn deref_to_width_dict<'doc>(
    doc: &'doc Document,
    object: &'doc Object,
) -> Option<&'doc Dictionary> {
    match object {
        Object::Reference(object_id) => match doc.get_object(*object_id).ok()? {
            Object::Dictionary(dictionary) => Some(dictionary),
            _ => None,
        },
        Object::Dictionary(dictionary) => Some(dictionary),
        _ => None,
    }
}

fn advance_pt(face: &rustybuzz::Face<'_>, text: &str, font_size: f32, units_per_em: f32) -> f32 {
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    let out = rustybuzz::shape(face, shaping_features(), buffer);
    let units = out
        .glyph_positions()
        .iter()
        .map(|position| position.x_advance as f32)
        .sum::<f32>()
        / GLYPH_UNITS_SCALE as f32;
    units * (font_size / units_per_em.max(1.0_f32))
}

fn times_roman_width(ch: char) -> i32 {
    match ch {
        ' ' => 250,
        '!' => 333,
        '"' => 408,
        '#' => 500,
        '$' => 500,
        '%' => 833,
        '&' => 778,
        '\'' => 180,
        '(' => 333,
        ')' => 333,
        '*' => 500,
        '+' => 564,
        ',' => 250,
        '-' => 333,
        '.' => 250,
        '/' => 278,
        '0' => 500,
        '1' => 500,
        '2' => 500,
        '3' => 500,
        '4' => 500,
        '5' => 500,
        '6' => 500,
        '7' => 500,
        '8' => 500,
        '9' => 500,
        ':' => 278,
        ';' => 278,
        '<' => 564,
        '=' => 564,
        '>' => 564,
        '?' => 444,
        '@' => 921,
        'A' => 722,
        'B' => 667,
        'C' => 667,
        'D' => 722,
        'E' => 611,
        'F' => 556,
        'G' => 722,
        'H' => 722,
        'I' => 333,
        'J' => 389,
        'K' => 722,
        'L' => 611,
        'M' => 889,
        'N' => 722,
        'O' => 722,
        'P' => 556,
        'Q' => 722,
        'R' => 667,
        'S' => 556,
        'T' => 611,
        'U' => 722,
        'V' => 722,
        'W' => 944,
        'X' => 722,
        'Y' => 722,
        'Z' => 611,
        '[' => 333,
        '\\' => 278,
        ']' => 333,
        '^' => 469,
        '_' => 500,
        '`' => 333,
        'a' => 444,
        'b' => 500,
        'c' => 444,
        'd' => 500,
        'e' => 444,
        'f' => 333,
        'g' => 500,
        'h' => 500,
        'i' => 278,
        'j' => 278,
        'k' => 500,
        'l' => 278,
        'm' => 778,
        'n' => 500,
        'o' => 500,
        'p' => 500,
        'q' => 500,
        'r' => 333,
        's' => 389,
        't' => 278,
        'u' => 500,
        'v' => 500,
        'w' => 722,
        'x' => 500,
        'y' => 500,
        'z' => 444,
        '{' => 480,
        '|' => 200,
        '}' => 480,
        '~' => 541,
        _ => 500,
    }
}

fn helvetica_width(ch: char) -> i32 {
    match ch {
        ' ' => 278,
        '!' => 278,
        '"' => 355,
        '#' => 556,
        '$' => 556,
        '%' => 889,
        '&' => 667,
        '\'' => 191,
        '(' => 333,
        ')' => 333,
        '*' => 389,
        '+' => 584,
        ',' => 278,
        '-' => 333,
        '.' => 278,
        '/' => 278,
        '0' => 556,
        '1' => 556,
        '2' => 556,
        '3' => 556,
        '4' => 556,
        '5' => 556,
        '6' => 556,
        '7' => 556,
        '8' => 556,
        '9' => 556,
        ':' => 278,
        ';' => 278,
        '<' => 584,
        '=' => 584,
        '>' => 584,
        '?' => 556,
        '@' => 1015,
        'A' => 667,
        'B' => 667,
        'C' => 722,
        'D' => 722,
        'E' => 667,
        'F' => 611,
        'G' => 778,
        'H' => 722,
        'I' => 278,
        'J' => 500,
        'K' => 667,
        'L' => 556,
        'M' => 833,
        'N' => 722,
        'O' => 778,
        'P' => 667,
        'Q' => 778,
        'R' => 722,
        'S' => 667,
        'T' => 611,
        'U' => 722,
        'V' => 667,
        'W' => 944,
        'X' => 667,
        'Y' => 667,
        'Z' => 611,
        '[' => 278,
        '\\' => 278,
        ']' => 278,
        '^' => 469,
        '_' => 556,
        '`' => 222,
        'a' => 556,
        'b' => 556,
        'c' => 500,
        'd' => 556,
        'e' => 556,
        'f' => 278,
        'g' => 556,
        'h' => 556,
        'i' => 222,
        'j' => 222,
        'k' => 500,
        'l' => 222,
        'm' => 833,
        'n' => 556,
        'o' => 556,
        'p' => 556,
        'q' => 556,
        'r' => 333,
        's' => 500,
        't' => 278,
        'u' => 556,
        'v' => 500,
        'w' => 722,
        'x' => 500,
        'y' => 500,
        'z' => 500,
        '{' => 334,
        '|' => 260,
        '}' => 334,
        '~' => 584,
        _ => 500,
    }
}
