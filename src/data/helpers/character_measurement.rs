use crate::data::types::redaction_evidence_types::CandidateWidthModel;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MeasuredTextWidth {
    pub glyph_width_sum_pt: f32,
    pub char_spacing_total_pt: f32,
    pub word_spacing_total_pt: f32,
    pub width_pt: f32,
}

pub(crate) fn measure_text_width(
    model: &CandidateWidthModel,
    text: &str,
) -> Result<MeasuredTextWidth, String> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut glyph_width_sum_pt = 0.0_f32;
    let mut space_count = 0_u32;
    for ch in chars.iter().copied() {
        let Some(width) = model.base_advances_pt.get(&ch).copied() else {
            return Err(format!(
                "unsupported character '{}' for {}",
                ch, model.font_name
            ));
        };
        glyph_width_sum_pt += width;
        if ch == ' ' {
            space_count += 1;
        }
    }
    let char_count = chars.len() as u32;
    let char_spacing_total_pt = model.char_spacing_pt * char_count.saturating_sub(1) as f32;
    let word_spacing_total_pt = model.word_spacing_pt * space_count as f32;
    Ok(MeasuredTextWidth {
        glyph_width_sum_pt,
        char_spacing_total_pt,
        word_spacing_total_pt,
        width_pt: glyph_width_sum_pt + char_spacing_total_pt + word_spacing_total_pt,
    })
}

pub(crate) fn measure_text(model: &CandidateWidthModel, text: &str) -> Result<f32, String> {
    measure_text_width(model, text).map(|measurement| measurement.width_pt)
}

pub(crate) fn supports_text(model: &CandidateWidthModel, text: &str) -> bool {
    text.chars()
        .all(|ch| model.base_advances_pt.contains_key(&ch))
}

#[cfg(test)]
mod tests {
    use super::{measure_text, measure_text_width};
    use crate::data::types::redaction_evidence_types::CandidateWidthModel;
    use std::collections::BTreeMap;

    #[test]
    fn measure_text_applies_character_and_word_spacing() {
        let mut base_advances_pt = BTreeMap::new();
        base_advances_pt.insert('A', 2.0_f32);
        base_advances_pt.insert(' ', 1.0_f32);
        let model = CandidateWidthModel {
            font_name: "Times-Roman".to_owned(),
            font_size_pt: 12.0_f32,
            h_scale_pct: 100.0_f32,
            char_spacing_pt: 0.5_f32,
            word_spacing_pt: 1.5_f32,
            base_advances_pt,
        };

        let width = measure_text(&model, "A A").expect("measurement should succeed");
        assert!(
            (width - 7.5_f32).abs() < 0.0001_f32,
            "unexpected width {width}"
        );
    }

    #[test]
    fn measure_text_width_decomposes_width_components() {
        let mut base_advances_pt = BTreeMap::new();
        base_advances_pt.insert('A', 2.0_f32);
        base_advances_pt.insert('B', 3.0_f32);
        base_advances_pt.insert(' ', 1.0_f32);
        let model = CandidateWidthModel {
            font_name: "Times-Roman".to_owned(),
            font_size_pt: 12.0_f32,
            h_scale_pct: 100.0_f32,
            char_spacing_pt: 0.25_f32,
            word_spacing_pt: 1.5_f32,
            base_advances_pt,
        };

        let measurement =
            measure_text_width(&model, "A B").expect("measurement should succeed");
        assert!((measurement.glyph_width_sum_pt - 6.0_f32).abs() < 0.0001_f32);
        assert!((measurement.char_spacing_total_pt - 0.5_f32).abs() < 0.0001_f32);
        assert!((measurement.word_spacing_total_pt - 1.5_f32).abs() < 0.0001_f32);
        assert!((measurement.width_pt - 8.0_f32).abs() < 0.0001_f32);
    }
}
