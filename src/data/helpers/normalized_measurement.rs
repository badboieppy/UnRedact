use crate::dependency::pdf_font_run_types::PdfFontTextRun;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NormalizedWidthProfile {
    pub font_name: String,
    pub font_size_centi_pt: i32,
    pub h_scale_tenths_pct: i32,
    pub char_spacing_centi_pt: i32,
    pub word_spacing_centi_pt: i32,
}

#[inline]
pub(crate) fn normalized_font_name(font_name: &str) -> String {
    font_name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

#[inline]
pub(crate) fn normalize_font_size_pt(font_size_pt: f32) -> f32 {
    ((font_size_pt as f64) * 100.0_f64).round() as f32 / 100.0_f32
}

#[inline]
pub(crate) fn normalize_h_scale_pct(h_scale_pct: f32) -> f32 {
    ((h_scale_pct as f64) * 10.0_f64).round() as f32 / 10.0_f32
}

#[inline]
pub(crate) fn normalize_spacing_pt(spacing_pt: f32) -> f32 {
    ((spacing_pt as f64) * 100.0_f64).round() as f32 / 100.0_f32
}

#[inline]
pub(crate) fn width_profile_from_run(run: &PdfFontTextRun) -> NormalizedWidthProfile {
    width_profile_from_parts(
        &run.font_name,
        run.font_size_pt,
        run.h_scale_pct,
        run.width_metrics.char_spacing_pt,
        run.width_metrics.word_spacing_pt,
    )
}

#[inline]
pub(crate) fn width_profile_from_parts(
    font_name: &str,
    font_size_pt: f32,
    h_scale_pct: f32,
    char_spacing_pt: f32,
    word_spacing_pt: f32,
) -> NormalizedWidthProfile {
    let normalized_font_name = normalized_font_name(font_name);
    let normalized_font_size_pt = normalize_font_size_pt(font_size_pt);
    let normalized_h_scale_pct = normalize_h_scale_pct(h_scale_pct);
    let normalized_char_spacing_pt = normalize_spacing_pt(char_spacing_pt);
    let normalized_word_spacing_pt = normalize_spacing_pt(word_spacing_pt);
    NormalizedWidthProfile {
        font_name: normalized_font_name,
        font_size_centi_pt: (normalized_font_size_pt * 100.0_f32).round() as i32,
        h_scale_tenths_pct: (normalized_h_scale_pct * 10.0_f32).round() as i32,
        char_spacing_centi_pt: (normalized_char_spacing_pt * 100.0_f32).round() as i32,
        word_spacing_centi_pt: (normalized_word_spacing_pt * 100.0_f32).round() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::{normalized_font_name, width_profile_from_run};
    use crate::dependency::pdf_font_run_types::{PdfFontTextRun, PdfWidthMetrics};
    use crate::types::file_types::{FontTextRun, Rect};

    fn sample_run(
        font_name: &str,
        font_size_pt: f32,
        h_scale_pct: f32,
        char_spacing_pt: f32,
        word_spacing_pt: f32,
    ) -> PdfFontTextRun {
        PdfFontTextRun {
            run: FontTextRun {
                page_index: 0,
                text: "Sample".to_owned(),
                bbox: Rect::new(0.0, 0.0, 10.0, 10.0),
                font_key: "F1".to_owned(),
                font_name: font_name.to_owned(),
                font_size_pt,
                h_scale_pct,
                measured_width_pt: None,
                measured_width_px: None,
                measured_dpi: None,
                char_advances_pt: vec![2.0; 6],
                char_advances_px: Vec::new(),
            },
            width_metrics: PdfWidthMetrics {
                char_spacing_pt,
                word_spacing_pt,
                ..PdfWidthMetrics::default()
            },
        }
    }

    #[test]
    fn normalized_font_name_collapses_whitespace() {
        assert_eq!(
            normalized_font_name("  Times   Roman  "),
            "Times Roman".to_owned()
        );
    }

    #[test]
    fn normalized_identity_merges_float_noise() {
        let left = sample_run(" Times-Roman ", 12.004_f32, 99.96_f32, 0.004_f32, 3.004_f32);
        let right = sample_run("Times-Roman", 12.003_f32, 100.04_f32, 0.003_f32, 3.003_f32);
        assert_eq!(width_profile_from_run(&left), width_profile_from_run(&right));
    }

    #[test]
    fn normalized_identity_keeps_distinct_measurement_families_separate() {
        let left = sample_run("Times-Roman", 12.00_f32, 100.0_f32, 0.0_f32, 3.0_f32);
        let right = sample_run("Times-Roman", 12.00_f32, 100.0_f32, 0.5_f32, 3.0_f32);
        assert_ne!(width_profile_from_run(&left), width_profile_from_run(&right));
    }
}
