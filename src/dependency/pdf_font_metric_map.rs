use lopdf::{Dictionary, Document, Object};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WidthMetricSource {
    EmbeddedFontProgram,
    PdfWidthTable,
    Standard14Font,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UnicodeMappingSource {
    ToUnicode,
    NamedEncoding,
    EncodingDifferences,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PdfMetricInventoryReport {
    pub summary: PdfMetricInventorySummary,
    pub inputs: Vec<PdfMetricInventoryInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub(crate) struct PdfMetricInventorySummary {
    pub file_count: usize,
    pub total_pages: usize,
    pub total_font_resources: usize,
    pub files_all_fonts_have_width_source: usize,
    pub files_all_fonts_have_unicode_source: usize,
    pub width_source_counts: BTreeMap<String, usize>,
    pub unicode_source_counts: BTreeMap<String, usize>,
    pub base_font_counts: BTreeMap<String, usize>,
    pub encoding_counts: BTreeMap<String, usize>,
    pub subtype_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PdfMetricInventoryInput {
    pub input: String,
    pub page_count: usize,
    pub summary: PdfMetricInventoryInputSummary,
    pub fonts: Vec<PdfFontMetricEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub(crate) struct PdfMetricInventoryInputSummary {
    pub font_resource_count: usize,
    pub pages_with_font_resources: usize,
    pub font_resources_with_explicit_widths: usize,
    pub font_resources_with_descendant_widths: usize,
    pub font_resources_with_embedded_program: usize,
    pub font_resources_with_to_unicode: usize,
    pub font_resources_with_encoding_differences: usize,
    pub font_resources_with_current_core_width_support: usize,
    pub font_resources_with_width_source: usize,
    pub font_resources_with_unicode_source: usize,
    pub subtype_counts: BTreeMap<String, usize>,
    pub base_font_counts: BTreeMap<String, usize>,
    pub encoding_counts: BTreeMap<String, usize>,
    pub width_source_counts: BTreeMap<String, usize>,
    pub unicode_source_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct PdfFontMetricEntry {
    pub page_index: u32,
    pub font_key: String,
    pub subtype: Option<String>,
    pub base_font: Option<String>,
    pub encoding: Option<String>,
    pub has_encoding_differences: bool,
    pub has_to_unicode: bool,
    pub has_explicit_widths: bool,
    pub has_descendant_widths: bool,
    pub has_embedded_font_program: bool,
    pub matches_current_core_width_support: bool,
    pub width_source: WidthMetricSource,
    pub unicode_source: UnicodeMappingSource,
}

pub(crate) fn collect_pdf_metric_inventory(
    input: &Path,
) -> Result<PdfMetricInventoryReport, String> {
    let paths = collect_pdf_paths(input)?;
    let mut inputs = Vec::with_capacity(paths.len());

    for path in &paths {
        let bytes = std::fs::read(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        inputs.push(collect_pdf_metric_inventory_from_bytes(path, &bytes)?);
    }

    Ok(PdfMetricInventoryReport {
        summary: summarize_inputs(&inputs),
        inputs,
    })
}

pub(crate) fn collect_pdf_metric_inventory_from_bytes(
    path: &Path,
    bytes: &[u8],
) -> Result<PdfMetricInventoryInput, String> {
    let doc = Document::load_mem(bytes)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    let pages = doc.get_pages();
    let mut fonts = Vec::new();
    let mut pages_with_font_resources = 0_usize;

    for (page_no, page_id) in pages.iter() {
        let page_index = page_no.saturating_sub(1);
        let (resources_opt, _unused_pages) = doc.get_page_resources(*page_id).map_err(|error| {
            format!(
                "failed to load page resources for {}: {error}",
                path.display()
            )
        })?;
        let Some(resources) = resources_opt else {
            continue;
        };
        let Some(font_object) = resources.get(b"Font").ok() else {
            continue;
        };
        let Some(font_dict) =
            deref_to_dict(&doc, font_object).or_else(|| object_to_dict(font_object))
        else {
            continue;
        };

        pages_with_font_resources += 1;

        for (font_key_bytes, value_object) in font_dict.iter() {
            let Some(dict) =
                deref_to_dict(&doc, value_object).or_else(|| object_to_dict(value_object))
            else {
                continue;
            };
            let font_key = String::from_utf8_lossy(font_key_bytes).to_string();
            let subtype = dict.get(b"Subtype").ok().and_then(object_to_name_string);
            let base_font = resolve_base_font_name(&doc, dict);
            let encoding = resolve_encoding_name(dict);
            let has_encoding_differences = encoding_has_differences(dict);
            let has_to_unicode = dict.get(b"ToUnicode").is_ok();
            let has_explicit_widths = dict.has(b"Widths");
            let has_descendant_widths = resolve_descendant_width_dict(&doc, dict).is_some();
            let has_embedded_font_program = has_embedded_font_program(&doc, dict);
            let matches_current_core_width_support = base_font
                .as_deref()
                .map(matches_standard_14_width_support)
                .unwrap_or(false);
            let width_source = classify_width_source(
                has_embedded_font_program,
                has_explicit_widths || has_descendant_widths,
                matches_current_core_width_support,
            );
            let unicode_source = classify_unicode_source(
                has_to_unicode,
                encoding.is_some(),
                has_encoding_differences,
            );

            fonts.push(PdfFontMetricEntry {
                page_index,
                font_key,
                subtype,
                base_font,
                encoding,
                has_encoding_differences,
                has_to_unicode,
                has_explicit_widths,
                has_descendant_widths,
                has_embedded_font_program,
                matches_current_core_width_support,
                width_source,
                unicode_source,
            });
        }
    }

    Ok(PdfMetricInventoryInput {
        input: path.display().to_string(),
        page_count: pages.len(),
        summary: summarize_font_entries(&fonts, pages_with_font_resources),
        fonts,
    })
}

fn summarize_inputs(inputs: &[PdfMetricInventoryInput]) -> PdfMetricInventorySummary {
    let mut summary = PdfMetricInventorySummary {
        file_count: inputs.len(),
        total_pages: inputs.iter().map(|input| input.page_count).sum(),
        total_font_resources: inputs
            .iter()
            .map(|input| input.summary.font_resource_count)
            .sum(),
        ..PdfMetricInventorySummary::default()
    };

    for input in inputs {
        if input.summary.font_resource_count > 0
            && input.summary.font_resources_with_width_source == input.summary.font_resource_count
        {
            summary.files_all_fonts_have_width_source += 1;
        }
        if input.summary.font_resource_count > 0
            && input.summary.font_resources_with_unicode_source == input.summary.font_resource_count
        {
            summary.files_all_fonts_have_unicode_source += 1;
        }

        merge_counts(
            &mut summary.width_source_counts,
            &input.summary.width_source_counts,
        );
        merge_counts(
            &mut summary.unicode_source_counts,
            &input.summary.unicode_source_counts,
        );
        merge_counts(
            &mut summary.base_font_counts,
            &input.summary.base_font_counts,
        );
        merge_counts(&mut summary.encoding_counts, &input.summary.encoding_counts);
        merge_counts(&mut summary.subtype_counts, &input.summary.subtype_counts);
    }

    summary
}

fn summarize_font_entries(
    fonts: &[PdfFontMetricEntry],
    pages_with_font_resources: usize,
) -> PdfMetricInventoryInputSummary {
    let mut summary = PdfMetricInventoryInputSummary {
        font_resource_count: fonts.len(),
        pages_with_font_resources,
        ..PdfMetricInventoryInputSummary::default()
    };

    for font in fonts {
        if font.has_explicit_widths {
            summary.font_resources_with_explicit_widths += 1;
        }
        if font.has_descendant_widths {
            summary.font_resources_with_descendant_widths += 1;
        }
        if font.has_embedded_font_program {
            summary.font_resources_with_embedded_program += 1;
        }
        if font.has_to_unicode {
            summary.font_resources_with_to_unicode += 1;
        }
        if font.has_encoding_differences {
            summary.font_resources_with_encoding_differences += 1;
        }
        if font.matches_current_core_width_support {
            summary.font_resources_with_current_core_width_support += 1;
        }
        if !matches!(font.width_source, WidthMetricSource::None) {
            summary.font_resources_with_width_source += 1;
        }
        if !matches!(font.unicode_source, UnicodeMappingSource::None) {
            summary.font_resources_with_unicode_source += 1;
        }
        bump_count(&mut summary.subtype_counts, font.subtype.clone());
        bump_count(&mut summary.base_font_counts, font.base_font.clone());
        bump_count(&mut summary.encoding_counts, font.encoding.clone());
        bump_count(
            &mut summary.width_source_counts,
            Some(width_source_key(&font.width_source).to_owned()),
        );
        bump_count(
            &mut summary.unicode_source_counts,
            Some(unicode_source_key(&font.unicode_source).to_owned()),
        );
    }

    summary
}

fn collect_pdf_paths(input: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    if input.is_file() {
        paths.push(input.to_path_buf());
    } else if input.is_dir() {
        collect_pdf_paths_recursive(input, &mut paths)?;
    } else {
        return Err(format!("input does not exist: {}", input.display()));
    }
    paths.sort();
    Ok(paths)
}

fn collect_pdf_paths_recursive(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("failed to read directory {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("failed to read directory entry: {error}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_pdf_paths_recursive(&path, out)?;
            continue;
        }
        let is_pdf = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("pdf"))
            .unwrap_or(false);
        if is_pdf {
            out.push(path);
        }
    }
    Ok(())
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

fn resolve_encoding_name(dict: &Dictionary) -> Option<String> {
    match dict.get(b"Encoding").ok() {
        Some(Object::Name(name_bytes)) => Some(String::from_utf8_lossy(name_bytes).to_string()),
        Some(Object::Dictionary(_)) => Some("dictionary".to_owned()),
        Some(Object::Reference(_)) => Some("reference".to_owned()),
        _ => None,
    }
}

fn encoding_has_differences(dict: &Dictionary) -> bool {
    match dict.get(b"Encoding").ok() {
        Some(Object::Dictionary(encoding_dict)) => encoding_dict.has(b"Differences"),
        _ => false,
    }
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

fn has_embedded_font_program(doc: &Document, dict: &Dictionary) -> bool {
    let descriptor = dict
        .get(b"FontDescriptor")
        .ok()
        .and_then(|object| deref_to_dict(doc, object));
    let Some(descriptor) = descriptor else {
        return false;
    };
    descriptor.has(b"FontFile") || descriptor.has(b"FontFile2") || descriptor.has(b"FontFile3")
}

fn classify_width_source(
    has_embedded_font_program: bool,
    has_width_table: bool,
    matches_current_core_width_support: bool,
) -> WidthMetricSource {
    if has_embedded_font_program {
        return WidthMetricSource::EmbeddedFontProgram;
    }
    if has_width_table {
        return WidthMetricSource::PdfWidthTable;
    }
    if matches_current_core_width_support {
        return WidthMetricSource::Standard14Font;
    }
    WidthMetricSource::None
}

fn classify_unicode_source(
    has_to_unicode: bool,
    has_named_encoding: bool,
    has_encoding_differences: bool,
) -> UnicodeMappingSource {
    if has_to_unicode {
        return UnicodeMappingSource::ToUnicode;
    }
    if has_encoding_differences {
        return UnicodeMappingSource::EncodingDifferences;
    }
    if has_named_encoding {
        return UnicodeMappingSource::NamedEncoding;
    }
    UnicodeMappingSource::None
}

fn matches_standard_14_width_support(font_name: &str) -> bool {
    matches!(
        font_name,
        "Courier" | "Helvetica" | "Helvetica-Bold" | "Times-Roman"
    )
}

fn width_source_key(value: &WidthMetricSource) -> &'static str {
    match value {
        WidthMetricSource::EmbeddedFontProgram => "embedded_font_program",
        WidthMetricSource::PdfWidthTable => "pdf_width_table",
        WidthMetricSource::Standard14Font => "standard_14_font",
        WidthMetricSource::None => "none",
    }
}

fn unicode_source_key(value: &UnicodeMappingSource) -> &'static str {
    match value {
        UnicodeMappingSource::ToUnicode => "to_unicode",
        UnicodeMappingSource::NamedEncoding => "named_encoding",
        UnicodeMappingSource::EncodingDifferences => "encoding_differences",
        UnicodeMappingSource::None => "none",
    }
}

fn bump_count(map: &mut BTreeMap<String, usize>, value: Option<String>) {
    let key = value.unwrap_or_else(|| "unknown".to_owned());
    *map.entry(key).or_insert(0) += 1;
}

fn merge_counts(target: &mut BTreeMap<String, usize>, source: &BTreeMap<String, usize>) {
    for (key, count) in source {
        *target.entry(key.clone()).or_insert(0) += count;
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
    use super::{
        classify_unicode_source, classify_width_source, matches_standard_14_width_support,
        normalize_subset_font_name, UnicodeMappingSource, WidthMetricSource,
    };

    #[test]
    fn classify_width_source_prefers_embedded_programs() {
        assert_eq!(
            classify_width_source(true, true, true),
            WidthMetricSource::EmbeddedFontProgram
        );
        assert_eq!(
            classify_width_source(false, true, true),
            WidthMetricSource::PdfWidthTable
        );
        assert_eq!(
            classify_width_source(false, false, true),
            WidthMetricSource::Standard14Font
        );
        assert_eq!(
            classify_width_source(false, false, false),
            WidthMetricSource::None
        );
    }

    #[test]
    fn classify_unicode_source_prefers_to_unicode() {
        assert_eq!(
            classify_unicode_source(true, true, true),
            UnicodeMappingSource::ToUnicode
        );
        assert_eq!(
            classify_unicode_source(false, false, true),
            UnicodeMappingSource::EncodingDifferences
        );
        assert_eq!(
            classify_unicode_source(false, true, false),
            UnicodeMappingSource::NamedEncoding
        );
        assert_eq!(
            classify_unicode_source(false, false, false),
            UnicodeMappingSource::None
        );
    }

    #[test]
    fn normalize_subset_font_name_strips_subset_prefix() {
        assert_eq!(normalize_subset_font_name("ABCDEF+ArialMT"), "ArialMT");
        assert_eq!(normalize_subset_font_name("Helvetica"), "Helvetica");
    }

    #[test]
    fn classify_width_source_labels_standard_14_support() {
        assert_eq!(
            super::width_source_key(&WidthMetricSource::Standard14Font),
            "standard_14_font"
        );
        assert!(matches_standard_14_width_support("Courier"));
    }
}
