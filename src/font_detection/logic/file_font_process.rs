use crate::font_detection::data::file_data_builder::{build_file_font_report, DataBuildConfig, FileDataBuilder};
use crate::font_detection::logic::types::file_types::{
    aggregate_counts, distinct_fonts_from_counts, FileFontReport, FontDetectionReport, FontId, FontOccurrence,
    FontOccurrences, FontsFound, InputFileKind, OutputFormat, Rect, Region, TextSourceKind,
};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontProcessConfig {
    pub include_details: bool,
}



pub fn process_many(
    builder: &FileDataBuilder<'_>,
    inputs: &[PathBuf],
    config: FontProcessConfig,
) -> Result<FontDetectionReport, String> {
    let _ = validate_inputs(inputs)?;
    let items = inputs
        .iter()
        .map(|p| process_one(builder, p, config))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FontDetectionReport { inputs: items })
}

pub fn process_one(
    builder: &FileDataBuilder<'_>,
    input: &Path,
    config: FontProcessConfig,
) -> Result<FileFontReport, String> {
    let data_cfg = DataBuildConfig {
        include_details: config.include_details,
    };
    let report = build_file_font_report(builder, input, data_cfg)?;
    Ok(finalize_report(report, config))
}

pub fn finalize_report(report: FileFontReport, config: FontProcessConfig) -> FileFontReport {
    let extracted = report.occurrences.clone();

    let counts = extracted
        .as_ref()
        .map(|o| aggregate_counts(&o.items))
        .unwrap_or_else(Vec::new);

    let distinct = distinct_fonts_from_counts(&counts);

    let occurrences = occurrences_for_output(extracted, config);

    FileFontReport {
        fonts: FontsFound { distinct, counts },
        occurrences,
        ..report
    }
}

pub fn occurrences_for_output(
    occurrences: Option<FontOccurrences>,
    config: FontProcessConfig,
) -> Option<FontOccurrences> {
    let enabled = config.include_details;
    if !enabled {
        return None;
    }
    occurrences
}

pub fn encode_report(report: &FontDetectionReport, format: OutputFormat) -> Result<Vec<u8>, String> {
    match format {
        OutputFormat::Json => encode_json(report),
    }
}

fn encode_json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}



pub fn validate_inputs(inputs: &[PathBuf]) -> Result<(), String> {
    let empty = inputs.is_empty();
    if empty {
        return Err("no inputs provided".to_string());
    }
    Ok(())
}

pub fn normalize_subset_font_name(raw: &str) -> String {
    let parts = raw.split('+').collect::<Vec<_>>();
    normalize_from_parts(&parts)
}

fn normalize_from_parts(parts: &[&str]) -> String {
    let is_subset = is_subset_prefix(parts);
    if !is_subset {
        return parts.join("+");
    }

    let second = parts.get(1).copied().unwrap_or("");
    if second.is_empty() {
        return parts.join("+");
    }

    second.to_string()
}

fn is_subset_prefix(parts: &[&str]) -> bool {
    let has_two = parts.len() == 2;
    if !has_two {
        return false;
    }

    let prefix = parts[0];
    let len_ok = prefix.len() == 6;
    if !len_ok {
        return false;
    }

    prefix.chars().all(|c| c.is_ascii_uppercase())
}

pub fn font_id_from_name(name: &str) -> FontId {
    let (family, variant) = split_font_family_variant(name);
    FontId { family, variant }
}

pub fn split_font_family_variant(name: &str) -> (String, Option<String>) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return ("".to_string(), None);
    }

    let parts = trimmed.split('-').collect::<Vec<_>>();
    let two = parts.len() == 2;

    if !two {
        return (trimmed.to_string(), None);
    }

    let family = parts[0].trim().to_string();
    let variant = parts[1].trim().to_string();

    let variant = if variant.is_empty() { None } else { Some(variant) };
    (family, variant)
}

pub fn classify_kind_from_path(path: &Path) -> InputFileKind {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let ext = ext.to_ascii_lowercase();

    if ext == "pdf" {
        return InputFileKind::Pdf;
    }

    let is_image = matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "webp");
    if is_image {
        return InputFileKind::Image;
    }

    InputFileKind::Unknown
}

pub fn default_text_source_for_kind(kind: InputFileKind) -> TextSourceKind {
    match kind {
        InputFileKind::Pdf => TextSourceKind::EmbeddedText,
        InputFileKind::Image => TextSourceKind::Ocr,
        InputFileKind::Unknown => TextSourceKind::Unknown,
    }
}

pub fn build_occurrence(
    font: FontId,
    page_index: Option<u32>,
    bbox: Rect,
    text: Option<String>,
    confidence: Option<f32>,
) -> FontOccurrence {
    FontOccurrence {
        font,
        location: crate::font_detection::logic::types::file_types::DocumentLocation {
            page_index,
            region: Region { bbox },
        },
        text,
        confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_detection::dependency::file_accessor::{FileAccessor, FileReadRequest, FileReadResponse};
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;

    struct FakeAccessor {
        files: BTreeMap<String, Vec<u8>>,
        err: Option<String>,
    }

    impl FakeAccessor {
        fn ok(files: BTreeMap<String, Vec<u8>>) -> Self {
            Self { files, err: None }
        }

        fn fail(message: &str) -> Self {
            Self {
                files: BTreeMap::new(),
                err: Some(message.to_string()),
            }
        }
    }

    impl FileAccessor for FakeAccessor {
        fn read(&self, req: FileReadRequest) -> Result<FileReadResponse, String> {
            let err = self.err.clone();
            if let Some(e) = err {
                return Err(e);
            }

            let key = req.path.to_string_lossy().to_string();
            let bytes = self.files.get(&key).cloned();
            match bytes {
                None => Err("not found".to_string()),
                Some(b) => Ok(FileReadResponse { bytes: b }),
            }
        }
    }

    #[test]
    fn validate_inputs_rejects_empty() {
        let err = validate_inputs(&[]).unwrap_err();
        assert_eq!(err, "no inputs provided".to_string());
    }

    #[test]
    fn validate_inputs_accepts_non_empty() {
        let ok = validate_inputs(&[PathBuf::from("a.pdf")]);
        assert_eq!(ok.is_ok(), true);
    }

    #[test]
    fn occurrences_for_output_none_when_disabled() {
        let occ = Some(FontOccurrences { items: vec![] });
        let out = occurrences_for_output(occ, FontProcessConfig { include_details: false });
        assert_eq!(out, None);
    }

    #[test]
    fn occurrences_for_output_some_when_enabled() {
        let occ = Some(FontOccurrences { items: vec![] });
        let out = occurrences_for_output(occ.clone(), FontProcessConfig { include_details: true });
        assert_eq!(out, occ);
    }

    #[test]
    fn encode_report_json_appends_newline() {
        let report = FontDetectionReport { inputs: vec![] };
        let bytes = encode_report(&report, OutputFormat::Json).unwrap();
        assert_eq!(bytes.last().copied(), Some(b'\n'));
    }

    #[test]
    fn normalize_subset_font_name_strips_prefix() {
        let out = normalize_subset_font_name("ABCDEF+Calibri");
        assert_eq!(out, "Calibri".to_string());
    }

    #[test]
    fn normalize_subset_font_name_keeps_non_subset() {
        let out = normalize_subset_font_name("TimesNewRomanPSMT");
        assert_eq!(out, "TimesNewRomanPSMT".to_string());
    }

    #[test]
    fn normalize_subset_font_name_keeps_if_prefix_not_upper() {
        let out = normalize_subset_font_name("AbCDEF+Calibri");
        assert_eq!(out, "AbCDEF+Calibri".to_string());
    }

    #[test]
    fn normalize_subset_font_name_keeps_if_second_empty() {
        let out = normalize_subset_font_name("ABCDEF+");
        assert_eq!(out, "ABCDEF+".to_string());
    }

    #[test]
    fn normalize_subset_font_name_keeps_if_more_than_one_plus() {
        let out = normalize_subset_font_name("ABCDEF+Cal+ibri");
        assert_eq!(out, "ABCDEF+Cal+ibri".to_string());
    }

    #[test]
    fn split_font_family_variant_splits_on_dash() {
        let (family, variant) = split_font_family_variant("Calibri-Bold");
        assert_eq!(family, "Calibri".to_string());
        assert_eq!(variant, Some("Bold".to_string()));
    }

    #[test]
    fn split_font_family_variant_keeps_whole_when_no_dash() {
        let (family, variant) = split_font_family_variant("TimesNewRomanPSMT");
        assert_eq!(family, "TimesNewRomanPSMT".to_string());
        assert_eq!(variant, None);
    }

    #[test]
    fn split_font_family_variant_keeps_variant_none_when_empty() {
        let (family, variant) = split_font_family_variant("Calibri-");
        assert_eq!(family, "Calibri".to_string());
        assert_eq!(variant, None);
    }

    #[test]
    fn split_font_family_variant_empty_string() {
        let (family, variant) = split_font_family_variant("   ");
        assert_eq!(family, "".to_string());
        assert_eq!(variant, None);
    }

    #[test]
    fn font_id_from_name_wraps_split() {
        let id = font_id_from_name("Arial-Italic");
        assert_eq!(
            id,
            FontId {
                family: "Arial".to_string(),
                variant: Some("Italic".to_string())
            }
        );
    }

    #[test]
    fn classify_kind_from_path_pdf() {
        let k = classify_kind_from_path(Path::new("x.PDF"));
        assert_eq!(k, InputFileKind::Pdf);
    }

    #[test]
    fn classify_kind_from_path_image() {
        let k = classify_kind_from_path(Path::new("x.jpeg"));
        assert_eq!(k, InputFileKind::Image);
    }

    #[test]
    fn classify_kind_from_path_unknown() {
        let k = classify_kind_from_path(Path::new("x.bin"));
        assert_eq!(k, InputFileKind::Unknown);
    }

    #[test]
    fn default_text_source_for_kind_pdf() {
        let s = default_text_source_for_kind(InputFileKind::Pdf);
        assert_eq!(s, TextSourceKind::EmbeddedText);
    }

    #[test]
    fn default_text_source_for_kind_image() {
        let s = default_text_source_for_kind(InputFileKind::Image);
        assert_eq!(s, TextSourceKind::Ocr);
    }

    #[test]
    fn default_text_source_for_kind_unknown() {
        let s = default_text_source_for_kind(InputFileKind::Unknown);
        assert_eq!(s, TextSourceKind::Unknown);
    }

    #[test]
    fn build_occurrence_populates_fields() {
        let font = FontId {
            family: "Arial".to_string(),
            variant: None,
        };
        let bbox = Rect::new(1.0, 2.0, 3.0, 4.0);
        let occ = build_occurrence(font.clone(), Some(2), bbox, Some("Hi".to_string()), Some(0.8));

        assert_eq!(occ.font, font);
        assert_eq!(occ.location.page_index, Some(2));
        assert_eq!(occ.location.region.bbox.x0, 1.0);
        assert_eq!(occ.text, Some("Hi".to_string()));
        assert_eq!(occ.confidence, Some(0.8));
    }

    #[test]
    fn finalize_report_computes_fonts_from_occurrences_when_details_enabled() {
        let arial = FontId {
            family: "Arial".to_string(),
            variant: None,
        };
        let calibri = FontId {
            family: "Calibri".to_string(),
            variant: None,
        };

        let occs = FontOccurrences {
            items: vec![
                build_occurrence(arial.clone(), Some(0), Rect::new(0.0, 0.0, 1.0, 1.0), None, None),
                build_occurrence(arial.clone(), Some(0), Rect::new(1.0, 0.0, 2.0, 1.0), None, None),
                build_occurrence(calibri.clone(), Some(1), Rect::new(0.0, 1.0, 2.0, 2.0), None, None),
            ],
        };

        let report = FileFontReport {
            path: "x.pdf".to_string(),
            kind: InputFileKind::Pdf,
            text_source: TextSourceKind::EmbeddedText,
            fonts: FontsFound {
                distinct: vec![],
                counts: vec![],
            },
            occurrences: Some(occs),
        };

        let out = finalize_report(report, FontProcessConfig { include_details: true });
        let map = out
            .fonts
            .counts
            .iter()
            .map(|c| (c.font.clone(), c.count))
            .collect::<BTreeMap<_, _>>();

        assert_eq!(out.occurrences.is_some(), true);
        assert_eq!(map.get(&arial).copied(), Some(2));
        assert_eq!(map.get(&calibri).copied(), Some(1));
        assert_eq!(out.fonts.distinct, vec![arial, calibri]);
    }

    #[test]
    fn finalize_report_removes_occurrences_when_details_disabled_and_fonts_are_still_computed() {
        let arial = FontId {
            family: "Arial".to_string(),
            variant: None,
        };
        let report = FileFontReport {
            path: "x.pdf".to_string(),
            kind: InputFileKind::Pdf,
            text_source: TextSourceKind::EmbeddedText,
            fonts: FontsFound {
                distinct: vec![],
                counts: vec![],
            },
            occurrences: Some(FontOccurrences {
                items: vec![build_occurrence(
                    arial.clone(),
                    Some(0),
                    Rect::new(0.0, 0.0, 1.0, 1.0),
                    None,
                    None,
                )],
            }),
        };

        let out = finalize_report(report, FontProcessConfig { include_details: false });

        assert_eq!(out.occurrences, None);
        assert_eq!(out.fonts.distinct, vec![arial.clone()]);
        assert_eq!(
            out.fonts.counts,
            vec![crate::font_detection::logic::types::file_types::FontCount {
                font: arial,
                count: 1
            }]
        );
    }

    #[test]
    fn run_font_process_rejects_empty_inputs() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let input = FontProcessInput {
            inputs: vec![],
            output: None,
            format: OutputFormat::Json,
            include_details: true,
        };
        let err = run_font_process(&accessor, input).unwrap_err();
        assert_eq!(err, "no inputs provided".to_string());
    }

    #[test]
    fn process_many_rejects_empty_inputs() {
        let accessor = FakeAccessor::ok(BTreeMap::new());
        let builder = FileDataBuilder::new(&accessor);
        let err = process_many(&builder, &[], FontProcessConfig { include_details: true }).unwrap_err();
        assert_eq!(err, "no inputs provided".to_string());
    }

    #[test]
    fn process_one_propagates_data_builder_error() {
        let accessor = FakeAccessor::fail("io");
        let builder = FileDataBuilder::new(&accessor);
        let err = process_one(&builder, Path::new("x.pdf"), FontProcessConfig { include_details: true }).unwrap_err();
        assert_eq!(err, "io".to_string());
    }
}
