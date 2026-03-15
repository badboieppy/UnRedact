use std::collections::BTreeMap;

use lopdf::{Dictionary, Document, Encoding, Object};

use crate::dependency::helpers::standard_14_widths::{
    standard_14_font_width, supports_standard_14_font,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FontWidthSource {
    PdfWidthTable,
    Standard14Font,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FontUnicodeSource {
    ToUnicode,
    EncodingDictionary,
    NamedEncoding,
    StandardDefaultEncoding,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FontResourceKey {
    pub page_index: u32,
    pub font_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PdfFontTruthCatalog {
    pub input: String,
    pub resources: BTreeMap<FontResourceKey, PdfFontTruthEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PdfFontTruthEntry {
    pub page_index: u32,
    pub font_key: String,
    pub subtype: Option<String>,
    pub base_font: Option<String>,
    pub encoding_source: FontUnicodeSource,
    pub width_source: FontWidthSource,
    pub has_to_unicode: bool,
    pub has_encoding_dictionary: bool,
    pub has_named_encoding: bool,
    pub has_explicit_widths: bool,
    pub unicode_to_codes: BTreeMap<char, Vec<u16>>,
    pub code_to_width_units: BTreeMap<u16, i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnicodeMappingResolution {
    source: FontUnicodeSource,
    unicode_to_codes: BTreeMap<char, Vec<u16>>,
}

pub(crate) fn build_font_truth_catalog_from_bytes(
    input_name: &str,
    bytes: &[u8],
) -> Result<PdfFontTruthCatalog, String> {
    let doc = Document::load_mem(bytes)
        .map_err(|error| format!("failed to parse {input_name}: {error}"))?;
    let pages = doc.get_pages();
    let mut resources = BTreeMap::new();

    for (page_no, page_id) in pages {
        let page_index = page_no.saturating_sub(1);
        let (resources_opt, _unused_pages) = doc.get_page_resources(page_id).map_err(|error| {
            format!(
                "failed to load page resources for {input_name} page {}: {error}",
                page_no
            )
        })?;
        let Some(resources_dict) = resources_opt else {
            continue;
        };
        let Some(font_object) = resources_dict.get(b"Font").ok() else {
            continue;
        };
        let Some(font_dict) =
            deref_to_dict(&doc, font_object).or_else(|| object_to_dict(font_object))
        else {
            continue;
        };

        for (font_key_bytes, value_object) in font_dict.iter() {
            let Some(dict) =
                deref_to_dict(&doc, value_object).or_else(|| object_to_dict(value_object))
            else {
                continue;
            };
            let font_key = String::from_utf8_lossy(font_key_bytes).to_string();
            let entry = build_font_truth_entry(page_index, &font_key, &doc, dict);
            resources.insert(
                FontResourceKey {
                    page_index,
                    font_key,
                },
                entry,
            );
        }
    }

    Ok(PdfFontTruthCatalog {
        input: input_name.to_owned(),
        resources,
    })
}

fn build_font_truth_entry(
    page_index: u32,
    font_key: &str,
    doc: &Document,
    dict: &Dictionary,
) -> PdfFontTruthEntry {
    let subtype = dict.get(b"Subtype").ok().and_then(object_to_name_string);
    let base_font = resolve_base_font_name(doc, dict);
    let explicit_widths = extract_explicit_widths(doc, dict);
    let has_explicit_widths = dict.has(b"Widths")
        || resolve_descendant_width_dict(doc, dict)
            .map(|descendant| descendant.has(b"Widths") || descendant.has(b"W"))
            .unwrap_or(false);
    let has_to_unicode = dict.get(b"ToUnicode").is_ok();
    let has_encoding_dictionary = resolve_encoding_dictionary(doc, dict).is_some();
    let has_named_encoding = matches!(dict.get(b"Encoding").ok(), Some(Object::Name(_)));
    let unicode_resolution = resolve_unicode_mapping(doc, dict, explicit_widths.keys());
    let width_source = if !explicit_widths.is_empty() {
        FontWidthSource::PdfWidthTable
    } else if base_font
        .as_deref()
        .map(supports_standard_14_font)
        .unwrap_or(false)
    {
        FontWidthSource::Standard14Font
    } else {
        FontWidthSource::None
    };
    let code_to_width_units = match width_source {
        FontWidthSource::PdfWidthTable => explicit_widths,
        FontWidthSource::Standard14Font => build_standard_14_code_widths(
            base_font.as_deref(),
            &unicode_resolution.unicode_to_codes,
        ),
        FontWidthSource::None => BTreeMap::new(),
    };

    PdfFontTruthEntry {
        page_index,
        font_key: font_key.to_owned(),
        subtype,
        base_font,
        encoding_source: unicode_resolution.source,
        width_source,
        has_to_unicode,
        has_encoding_dictionary,
        has_named_encoding,
        has_explicit_widths,
        unicode_to_codes: unicode_resolution.unicode_to_codes,
        code_to_width_units,
    }
}

fn resolve_unicode_mapping<'a>(
    doc: &Document,
    dict: &Dictionary,
    explicit_width_codes: impl Iterator<Item = &'a u16>,
) -> UnicodeMappingResolution {
    if let Some(unicode_to_codes) = resolve_to_unicode_mapping(doc, dict, explicit_width_codes) {
        if !unicode_to_codes.is_empty() {
            return UnicodeMappingResolution {
                source: FontUnicodeSource::ToUnicode,
                unicode_to_codes,
            };
        }
    }

    if let Some(encoding_dict) = resolve_encoding_dictionary(doc, dict) {
        let unicode_to_codes = resolve_encoding_dictionary_mapping(doc, dict, encoding_dict);
        return UnicodeMappingResolution {
            source: FontUnicodeSource::EncodingDictionary,
            unicode_to_codes,
        };
    }

    if dict
        .get(b"Encoding")
        .ok()
        .and_then(object_to_name_string)
        .is_some()
    {
        if let Some(unicode_to_codes) = build_one_byte_mapping_from_font(doc, dict) {
            return UnicodeMappingResolution {
                source: FontUnicodeSource::NamedEncoding,
                unicode_to_codes,
            };
        }
    }

    if let Some(unicode_to_codes) = build_default_encoding_mapping(doc, dict) {
        return UnicodeMappingResolution {
            source: FontUnicodeSource::StandardDefaultEncoding,
            unicode_to_codes,
        };
    }

    UnicodeMappingResolution {
        source: FontUnicodeSource::None,
        unicode_to_codes: BTreeMap::new(),
    }
}

fn resolve_to_unicode_mapping<'a>(
    doc: &Document,
    dict: &Dictionary,
    explicit_width_codes: impl Iterator<Item = &'a u16>,
) -> Option<BTreeMap<char, Vec<u16>>> {
    let encoding = dict.get_font_encoding(doc).ok()?;
    let Encoding::UnicodeMapEncoding(cmap) = encoding else {
        return None;
    };
    let mut unicode_to_codes = BTreeMap::<char, Vec<u16>>::new();
    let codes = explicit_width_codes.copied().collect::<Vec<_>>();
    if codes.is_empty() {
        for code in 0_u16..=255_u16 {
            let Some(values) = cmap.get(code) else {
                continue;
            };
            if values.len() != 1 {
                continue;
            }
            let Some(ch) = char::from_u32(u32::from(values[0])) else {
                continue;
            };
            unicode_to_codes.entry(ch).or_default().push(code);
        }
    } else {
        for code in codes {
            let Some(values) = cmap.get(code) else {
                continue;
            };
            if values.len() != 1 {
                continue;
            }
            let Some(ch) = char::from_u32(u32::from(values[0])) else {
                continue;
            };
            unicode_to_codes.entry(ch).or_default().push(code);
        }
    }
    Some(unicode_to_codes)
}

fn resolve_encoding_dictionary_mapping(
    doc: &Document,
    font_dict: &Dictionary,
    encoding_dict: &Dictionary,
) -> BTreeMap<char, Vec<u16>> {
    let mut unicode_to_codes = build_one_byte_mapping_from_font(doc, font_dict)
        .or_else(|| build_default_encoding_mapping(doc, font_dict))
        .unwrap_or_default();

    let Some(differences) = encoding_dict
        .get(b"Differences")
        .ok()
        .and_then(object_to_array)
    else {
        return unicode_to_codes;
    };

    let mut current_code = None::<u16>;
    for item in differences {
        match item {
            Object::Integer(value) if *value >= 0 && *value <= i64::from(u16::MAX) => {
                current_code = Some(*value as u16);
            }
            Object::Name(name_bytes) => {
                let Some(code) = current_code else {
                    continue;
                };
                remove_code_mapping(&mut unicode_to_codes, code);
                let name = String::from_utf8_lossy(name_bytes);
                if let Some(ch) = glyph_name_to_char(&name) {
                    unicode_to_codes.entry(ch).or_default().push(code);
                }
                current_code = code.checked_add(1);
            }
            _ => {}
        }
    }

    normalize_code_lists(&mut unicode_to_codes);
    unicode_to_codes
}

fn remove_code_mapping(unicode_to_codes: &mut BTreeMap<char, Vec<u16>>, code: u16) {
    let keys = unicode_to_codes.keys().copied().collect::<Vec<_>>();
    for key in keys {
        if let Some(codes) = unicode_to_codes.get_mut(&key) {
            codes.retain(|candidate| *candidate != code);
            if codes.is_empty() {
                unicode_to_codes.remove(&key);
            }
        }
    }
}

fn normalize_code_lists(unicode_to_codes: &mut BTreeMap<char, Vec<u16>>) {
    for codes in unicode_to_codes.values_mut() {
        codes.sort_unstable();
        codes.dedup();
    }
}

fn build_default_encoding_mapping(
    doc: &Document,
    dict: &Dictionary,
) -> Option<BTreeMap<char, Vec<u16>>> {
    let subtype = dict.get(b"Subtype").ok().and_then(object_to_name_string);
    if subtype.as_deref() == Some("Type0") {
        return None;
    }
    build_one_byte_mapping_from_font(doc, dict)
}

fn build_one_byte_mapping_from_font(
    doc: &Document,
    dict: &Dictionary,
) -> Option<BTreeMap<char, Vec<u16>>> {
    let encoding = dict.get_font_encoding(doc).ok()?;
    let Encoding::OneByteEncoding(set) = encoding else {
        return None;
    };
    let mut unicode_to_codes = BTreeMap::<char, Vec<u16>>::new();
    for (code, code_point) in set.iter().enumerate() {
        let Some(code_point) = code_point else {
            continue;
        };
        let Some(ch) = char::from_u32(u32::from(*code_point)) else {
            continue;
        };
        unicode_to_codes.entry(ch).or_default().push(code as u16);
    }
    Some(unicode_to_codes)
}

fn build_standard_14_code_widths(
    base_font: Option<&str>,
    unicode_to_codes: &BTreeMap<char, Vec<u16>>,
) -> BTreeMap<u16, i32> {
    let Some(base_font) = base_font else {
        return BTreeMap::new();
    };
    let mut code_to_width_units = BTreeMap::new();
    for (ch, codes) in unicode_to_codes {
        let Some(width) = standard_14_font_width(base_font, *ch) else {
            continue;
        };
        for code in codes {
            code_to_width_units.entry(*code).or_insert(width);
        }
    }
    code_to_width_units
}

fn extract_explicit_widths(doc: &Document, dict: &Dictionary) -> BTreeMap<u16, i32> {
    let simple_widths = extract_simple_widths(doc, dict);
    if !simple_widths.is_empty() {
        return simple_widths;
    }
    extract_descendant_widths(doc, dict)
}

fn extract_simple_widths(doc: &Document, dict: &Dictionary) -> BTreeMap<u16, i32> {
    let Some(first_char) = dict
        .get_deref(b"FirstChar", doc)
        .ok()
        .and_then(object_to_i64)
        .and_then(|value| u16::try_from(value).ok())
    else {
        return BTreeMap::new();
    };
    let Some(widths) = dict
        .get_deref(b"Widths", doc)
        .ok()
        .and_then(object_to_array)
    else {
        return BTreeMap::new();
    };
    let mut code_to_width_units = BTreeMap::new();
    for (index, width_object) in widths.iter().enumerate() {
        let Some(width) = object_to_i32(width_object) else {
            continue;
        };
        code_to_width_units.insert(first_char.saturating_add(index as u16), width);
    }
    code_to_width_units
}

fn extract_descendant_widths(doc: &Document, dict: &Dictionary) -> BTreeMap<u16, i32> {
    let Some(descendant) = resolve_descendant_width_dict(doc, dict) else {
        return BTreeMap::new();
    };
    if descendant.has(b"Widths") {
        return extract_simple_widths_from_descendant(doc, descendant);
    }
    let Some(widths) = descendant
        .get_deref(b"W", doc)
        .ok()
        .and_then(object_to_array)
    else {
        return BTreeMap::new();
    };

    let mut code_to_width_units = BTreeMap::new();
    let mut cursor = 0_usize;
    while cursor < widths.len() {
        let Some(start_code) = widths
            .get(cursor)
            .and_then(object_to_i64)
            .and_then(|value| u16::try_from(value).ok())
        else {
            break;
        };
        let Some(next) = widths.get(cursor + 1) else {
            break;
        };
        match next {
            Object::Array(width_values) => {
                for (index, width_value) in width_values.iter().enumerate() {
                    if let Some(width) = object_to_i32(width_value) {
                        code_to_width_units.insert(start_code.saturating_add(index as u16), width);
                    }
                }
                cursor += 2;
            }
            _ => {
                let Some(end_code) =
                    object_to_i64(next).and_then(|value| u16::try_from(value).ok())
                else {
                    break;
                };
                let Some(width) = widths.get(cursor + 2).and_then(object_to_i32) else {
                    break;
                };
                for code in start_code..=end_code {
                    code_to_width_units.insert(code, width);
                }
                cursor += 3;
            }
        }
    }

    code_to_width_units
}

fn extract_simple_widths_from_descendant(doc: &Document, dict: &Dictionary) -> BTreeMap<u16, i32> {
    let Some(first_char) = dict
        .get_deref(b"FirstChar", doc)
        .ok()
        .and_then(object_to_i64)
        .and_then(|value| u16::try_from(value).ok())
    else {
        return BTreeMap::new();
    };
    let Some(widths) = dict
        .get_deref(b"Widths", doc)
        .ok()
        .and_then(object_to_array)
    else {
        return BTreeMap::new();
    };
    let mut code_to_width_units = BTreeMap::new();
    for (index, width_object) in widths.iter().enumerate() {
        let Some(width) = object_to_i32(width_object) else {
            continue;
        };
        code_to_width_units.insert(first_char.saturating_add(index as u16), width);
    }
    code_to_width_units
}

fn resolve_base_font_name(doc: &Document, dict: &Dictionary) -> Option<String> {
    if let Some(name) = dict.get(b"BaseFont").ok().and_then(object_to_name_string) {
        return Some(normalize_subset_font_name(&name));
    }
    let descriptor = dict
        .get(b"FontDescriptor")
        .ok()
        .and_then(|object| deref_to_dict(doc, object))?;
    descriptor
        .get(b"FontName")
        .ok()
        .and_then(object_to_name_string)
        .map(|name| normalize_subset_font_name(&name))
}

fn resolve_encoding_dictionary<'a>(
    doc: &'a Document,
    dict: &'a Dictionary,
) -> Option<&'a Dictionary> {
    dict.get_deref(b"Encoding", doc)
        .ok()
        .and_then(object_to_dict)
}

fn resolve_descendant_width_dict<'a>(
    doc: &'a Document,
    dict: &'a Dictionary,
) -> Option<&'a Dictionary> {
    let subtype = dict.get(b"Subtype").ok().and_then(object_to_name_string);
    if subtype.as_deref() != Some("Type0") {
        return None;
    }
    let descendants = dict
        .get(b"DescendantFonts")
        .ok()
        .and_then(object_to_array)?;
    descendants
        .first()
        .and_then(|object| deref_to_dict(doc, object))
        .filter(|descendant| descendant.has(b"Widths") || descendant.has(b"W"))
}

fn glyph_name_to_char(name: &str) -> Option<char> {
    match name {
        "space" | "nbspace" => Some(' '),
        "exclam" => Some('!'),
        "quotedbl" => Some('"'),
        "numbersign" => Some('#'),
        "dollar" => Some('$'),
        "percent" => Some('%'),
        "ampersand" => Some('&'),
        "quotesingle" => Some('\''),
        "quoteright" | "quoteleft" => Some('’'),
        "parenleft" => Some('('),
        "parenright" => Some(')'),
        "asterisk" => Some('*'),
        "plus" => Some('+'),
        "comma" => Some(','),
        "hyphen" | "sfthyphen" => Some('-'),
        "period" => Some('.'),
        "slash" => Some('/'),
        "colon" => Some(':'),
        "semicolon" => Some(';'),
        "less" => Some('<'),
        "equal" => Some('='),
        "greater" => Some('>'),
        "question" => Some('?'),
        "at" => Some('@'),
        "bracketleft" => Some('['),
        "backslash" => Some('\\'),
        "bracketright" => Some(']'),
        "asciicircum" | "circumflex" => Some('^'),
        "underscore" => Some('_'),
        "grave" => Some('`'),
        "braceleft" => Some('{'),
        "bar" | "brokenbar" => Some('|'),
        "braceright" => Some('}'),
        "asciitilde" | "tilde" => Some('~'),
        "endash" => Some('–'),
        "emdash" => Some('—'),
        "quotedblleft" | "quotedblright" => Some('"'),
        "quotedblbase" => Some('"'),
        "quotesinglbase" => Some('\''),
        "guillemotleft" | "guilsinglleft" => Some('<'),
        "guillemotright" | "guilsinglright" => Some('>'),
        "bullet" => Some('•'),
        "dagger" => Some('†'),
        "daggerdbl" => Some('‡'),
        "ellipsis" => Some('…'),
        "trademark" => Some('™'),
        "Euro" => Some('€'),
        "copyright" => Some('©'),
        "registered" => Some('®'),
        "paragraph" => Some('¶'),
        "periodcentered" => Some('·'),
        "plusminus" => Some('±'),
        "divide" => Some('÷'),
        "multiply" => Some('×'),
        "logicalnot" => Some('¬'),
        "sterling" => Some('£'),
        "yen" => Some('¥'),
        "currency" => Some('¤'),
        "section" => Some('§'),
        "cent" => Some('¢'),
        "degree" => Some('°'),
        "florin" => Some('ƒ'),
        "mu" => Some('µ'),
        "ae" => Some('æ'),
        "AE" => Some('Æ'),
        "oe" => Some('œ'),
        "OE" => Some('Œ'),
        "lslash" => Some('ł'),
        "Lslash" => Some('Ł'),
        "oslash" => Some('ø'),
        "Oslash" => Some('Ø'),
        "germandbls" => Some('ß'),
        "dotlessi" => Some('ı'),
        "Aacute" => Some('Á'),
        "Acircumflex" => Some('Â'),
        "Adieresis" => Some('Ä'),
        "Agrave" => Some('À'),
        "Aring" => Some('Å'),
        "Atilde" => Some('Ã'),
        "Ccedilla" => Some('Ç'),
        "Eacute" => Some('É'),
        "Ecircumflex" => Some('Ê'),
        "Edieresis" => Some('Ë'),
        "Egrave" => Some('È'),
        "Iacute" => Some('Í'),
        "Icircumflex" => Some('Î'),
        "Idieresis" => Some('Ï'),
        "Igrave" => Some('Ì'),
        "Ntilde" | "Nacute" => Some('Ñ'),
        "Oacute" => Some('Ó'),
        "Ocircumflex" => Some('Ô'),
        "Odieresis" => Some('Ö'),
        "Ograve" => Some('Ò'),
        "Otilde" => Some('Õ'),
        "Uacute" => Some('Ú'),
        "Ucircumflex" => Some('Û'),
        "Udieresis" => Some('Ü'),
        "Ugrave" => Some('Ù'),
        "Yacute" => Some('Ý'),
        "Ydieresis" => Some('Ÿ'),
        "aacute" => Some('á'),
        "acircumflex" => Some('â'),
        "adieresis" => Some('ä'),
        "agrave" => Some('à'),
        "aring" => Some('å'),
        "atilde" => Some('ã'),
        "ccedilla" => Some('ç'),
        "eacute" => Some('é'),
        "ecircumflex" => Some('ê'),
        "edieresis" => Some('ë'),
        "egrave" => Some('è'),
        "iacute" => Some('í'),
        "icircumflex" => Some('î'),
        "idieresis" => Some('ï'),
        "igrave" => Some('ì'),
        "ntilde" | "nacute" => Some('ń'),
        "oacute" => Some('ó'),
        "ocircumflex" => Some('ô'),
        "odieresis" => Some('ö'),
        "ograve" => Some('ò'),
        "otilde" => Some('õ'),
        "uacute" => Some('ú'),
        "ucircumflex" => Some('û'),
        "udieresis" => Some('ü'),
        "ugrave" => Some('ù'),
        "yacute" => Some('ý'),
        "ydieresis" => Some('ÿ'),
        value if value.len() == 1 => value.chars().next(),
        _ => None,
    }
}

fn normalize_subset_font_name(raw: &str) -> String {
    let parts = raw.split('+').collect::<Vec<_>>();
    if parts.len() == 2
        && parts[0].len() == 6
        && parts[0].chars().all(|ch| ch.is_ascii_uppercase())
        && !parts[1].is_empty()
    {
        return parts[1].to_owned();
    }
    raw.to_owned()
}

fn object_to_name_string(object: &Object) -> Option<String> {
    match object {
        Object::Name(name_bytes) => Some(String::from_utf8_lossy(name_bytes).to_string()),
        _ => None,
    }
}

fn object_to_dict(object: &Object) -> Option<&Dictionary> {
    match object {
        Object::Dictionary(dict) => Some(dict),
        _ => None,
    }
}

fn object_to_array(object: &Object) -> Option<&Vec<Object>> {
    match object {
        Object::Array(values) => Some(values),
        _ => None,
    }
}

fn object_to_i64(object: &Object) -> Option<i64> {
    object.as_i64().ok()
}

fn object_to_i32(object: &Object) -> Option<i32> {
    match object {
        Object::Integer(value) => i32::try_from(*value).ok(),
        Object::Real(value) => Some(*value as i32),
        _ => None,
    }
}

fn deref_to_dict<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    match object {
        Object::Reference(object_id) => match doc.get_object(*object_id).ok()? {
            Object::Dictionary(dict) => Some(dict),
            _ => None,
        },
        Object::Dictionary(dict) => Some(dict),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_one_byte_mapping_from_font, glyph_name_to_char};
    use lopdf::{Dictionary, Document, Object};

    #[test]
    fn glyph_name_lookup_covers_named_dictionary_characters() {
        assert_eq!(glyph_name_to_char("quoteright"), Some('’'));
        assert_eq!(glyph_name_to_char("endash"), Some('–'));
        assert_eq!(glyph_name_to_char("oslash"), Some('ø'));
        assert_eq!(glyph_name_to_char("nacute"), Some('ń'));
    }

    #[test]
    fn named_encoding_mapping_supports_win_ansi_accented_letters() {
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"Font".to_vec()));
        dict.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
        let doc = Document::with_version("1.5");
        let mapping =
            build_one_byte_mapping_from_font(&doc, &dict).expect("expected win ansi mapping");
        assert!(mapping.contains_key(&'é'));
        assert!(mapping.contains_key(&'ø'));
    }
}
