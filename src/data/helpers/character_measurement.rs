use crate::data::types::redaction_evidence_types::CandidateWidthModel;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeasuredTextWidth {
    pub glyph_width_sum_units: i32,
    pub glyph_width_sum_pt: f32,
    pub char_spacing_total_pt: f32,
    pub word_spacing_total_pt: f32,
    pub width_pt: f32,
    pub chosen_codes: Vec<u16>,
    pub ambiguous_choices: Vec<AmbiguousCodeChoice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AmbiguousCodeChoice {
    pub char_index: usize,
    pub ch: char,
    pub available_codes: Vec<u16>,
    pub chosen_code: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MeasurementFailure {
    RowUnicodeBackendMissing,
    RowWidthBackendMissing,
    UnicodeCharUnmapped {
        char_index: usize,
        ch: char,
    },
    WidthEntryMissing {
        char_index: usize,
        ch: char,
        available_codes: Vec<u16>,
    },
}

impl MeasurementFailure {
    #[inline]
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::RowUnicodeBackendMissing => "row_unicode_backend_missing",
            Self::RowWidthBackendMissing => "row_width_backend_missing",
            Self::UnicodeCharUnmapped { .. } => "candidate_unicode_char_unmapped",
            Self::WidthEntryMissing { .. } => "candidate_width_entry_missing",
        }
    }

    #[inline]
    pub fn message(&self, font_name: &str) -> String {
        match self {
            Self::RowUnicodeBackendMissing => {
                format!("row font {font_name} has no unicode backend")
            }
            Self::RowWidthBackendMissing => {
                format!("row font {font_name} has no width backend")
            }
            Self::UnicodeCharUnmapped { ch, .. } => {
                format!("candidate character '{ch}' is not mapped by {font_name}")
            }
            Self::WidthEntryMissing { ch, .. } => {
                format!("candidate character '{ch}' has no width entry for {font_name}")
            }
        }
    }
}

pub(crate) fn measure_text_width(
    model: &CandidateWidthModel,
    text: &str,
) -> Result<MeasuredTextWidth, MeasurementFailure> {
    if matches!(model.encoding_source.as_str(), "none") || model.unicode_to_codes.is_empty() {
        return Err(MeasurementFailure::RowUnicodeBackendMissing);
    }
    if matches!(model.width_source.as_str(), "none") || model.code_to_width_units.is_empty() {
        return Err(MeasurementFailure::RowWidthBackendMissing);
    }

    let chars = text.chars().collect::<Vec<_>>();
    let mut glyph_width_units = 0_i32;
    let mut chosen_codes = Vec::with_capacity(chars.len());
    let mut ambiguous_choices = Vec::new();
    let mut space_count = 0_u32;

    for (char_index, ch) in chars.iter().copied().enumerate() {
        let Some(codes) = model.unicode_to_codes.get(&ch) else {
            return Err(MeasurementFailure::UnicodeCharUnmapped { char_index, ch });
        };
        let mut available_codes = codes
            .iter()
            .copied()
            .filter(|code| model.code_to_width_units.contains_key(code))
            .collect::<Vec<_>>();
        if available_codes.is_empty() {
            return Err(MeasurementFailure::WidthEntryMissing {
                char_index,
                ch,
                available_codes: codes.clone(),
            });
        }
        available_codes.sort_unstable();
        let chosen_code = available_codes[0];
        if available_codes.len() > 1 {
            ambiguous_choices.push(AmbiguousCodeChoice {
                char_index,
                ch,
                available_codes,
                chosen_code,
            });
        }
        glyph_width_units += model
            .code_to_width_units
            .get(&chosen_code)
            .copied()
            .unwrap_or_default();
        chosen_codes.push(chosen_code);
        if ch == ' ' {
            space_count += 1;
        }
    }

    let scale = (model.h_scale_pct / 100.0_f32).max(0.01_f32);
    let glyph_width_sum_pt = glyph_width_units as f32 * (model.font_size_pt / 1000.0_f32) * scale;
    let char_spacing_total_pt = model.char_spacing_pt * chars.len().saturating_sub(1) as f32;
    let word_spacing_total_pt = model.word_spacing_pt * space_count as f32;

    Ok(MeasuredTextWidth {
        glyph_width_sum_units: glyph_width_units,
        glyph_width_sum_pt,
        char_spacing_total_pt,
        word_spacing_total_pt,
        width_pt: glyph_width_sum_pt + char_spacing_total_pt + word_spacing_total_pt,
        chosen_codes,
        ambiguous_choices,
    })
}

pub(crate) fn measure_text(
    model: &CandidateWidthModel,
    text: &str,
) -> Result<f32, MeasurementFailure> {
    measure_text_width(model, text).map(|measurement| measurement.width_pt)
}

#[cfg(test)]
mod tests {
    use super::{measure_text, measure_text_width, MeasurementFailure};
    use crate::data::types::redaction_evidence_types::{
        CandidateWidthModel, MeasurementEncodingSource, MeasurementFontKey, MeasurementWidthSource,
    };
    use std::collections::BTreeMap;

    fn sample_model() -> CandidateWidthModel {
        let mut unicode_to_codes = BTreeMap::new();
        unicode_to_codes.insert('A', vec![65]);
        unicode_to_codes.insert(' ', vec![32]);
        let mut code_to_width_units = BTreeMap::new();
        code_to_width_units.insert(65, 200_i32);
        code_to_width_units.insert(32, 100_i32);
        CandidateWidthModel {
            resource_key: MeasurementFontKey {
                page_index: 0,
                font_key: "F1".to_owned(),
            },
            font_key: "F1".to_owned(),
            font_name: "Times-Roman".to_owned(),
            base_font: Some("Times-Roman".to_owned()),
            subtype: Some("Type1".to_owned()),
            font_size_pt: 10.0,
            h_scale_pct: 100.0,
            char_spacing_pt: 0.5,
            word_spacing_pt: 1.5,
            width_source: MeasurementWidthSource::PdfWidthTable,
            encoding_source: MeasurementEncodingSource::NamedEncoding,
            has_to_unicode: false,
            has_encoding_dictionary: false,
            has_named_encoding: true,
            has_explicit_widths: true,
            unicode_to_codes,
            code_to_width_units,
        }
    }

    #[test]
    fn measure_text_applies_character_and_word_spacing() {
        let width = measure_text(&sample_model(), "A A").expect("measurement should succeed");
        assert!((width - 7.5_f32).abs() < 0.0001_f32);
    }

    #[test]
    fn measure_text_width_tracks_ambiguous_codes() {
        let mut model = sample_model();
        model.unicode_to_codes.insert('A', vec![65, 193]);
        model.code_to_width_units.insert(193, 200_i32);
        let measurement = measure_text_width(&model, "A").expect("measurement should succeed");
        assert_eq!(measurement.chosen_codes, vec![65]);
        assert_eq!(measurement.ambiguous_choices.len(), 1);
    }

    #[test]
    fn measure_text_width_rejects_unmapped_characters() {
        let error = measure_text_width(&sample_model(), "B").expect_err("expected unmapped char");
        assert!(matches!(
            error,
            MeasurementFailure::UnicodeCharUnmapped { ch: 'B', .. }
        ));
    }
}
