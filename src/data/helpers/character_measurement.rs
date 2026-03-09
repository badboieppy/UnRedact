use crate::data::types::redaction_evidence_types::CandidateWidthModel;

pub(crate) fn measure_text(model: &CandidateWidthModel, text: &str) -> Result<f32, String> {
    let mut total = 0.0_f32;
    let chars = text.chars().collect::<Vec<_>>();
    let last_index = chars.len().saturating_sub(1);
    for (index, ch) in chars.iter().copied().enumerate() {
        let Some(width) = model.base_advances_pt.get(&ch).copied() else {
            return Err(format!("unsupported character '{}' for {}", ch, model.font_name));
        };
        total += width;
        if index >= last_index {
            continue;
        }
        total += model.char_spacing_pt;
        if ch.is_whitespace() {
            total += model.word_spacing_pt;
        }
    }
    Ok(total)
}

pub(crate) fn supports_text(model: &CandidateWidthModel, text: &str) -> bool {
    text.chars().all(|ch| model.base_advances_pt.contains_key(&ch))
}

#[cfg(test)]
mod tests {
    use super::measure_text;
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
        assert!((width - 7.5_f32).abs() < 0.0001_f32, "unexpected width {width}");
    }
}
