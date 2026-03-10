use std::ops::Deref;

use crate::types::file_types::{FontAsset, FontRunReport, FontTextRun};

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PdfWidthMetrics {
    pub render_mode: u8,
    pub text_origin_x_pt: f32,
    pub text_origin_y_pt: f32,
    pub char_spacing_pt: f32,
    pub word_spacing_pt: f32,
    pub base_char_advances_pt: Vec<f32>,
    pub observed_char_advances_pt: Vec<f32>,
    pub tj_adjustments_pt_by_gap: Vec<f32>,
    pub base_glyph_width_pt: f32,
    pub explicit_char_spacing_total_pt: f32,
    pub explicit_word_spacing_total_pt: f32,
    pub explicit_tj_total_pt: f32,
    pub observed_width_pt: f32,
    pub residual_width_delta_pt: f32,
    pub glyph_count: u32,
    pub char_count: u32,
    pub has_cluster_substitution: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PdfFontTextRun {
    pub run: FontTextRun,
    pub width_metrics: PdfWidthMetrics,
}

impl Deref for PdfFontTextRun {
    type Target = FontTextRun;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.run
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PdfFontRunReport {
    pub input: String,
    pub runs: Vec<PdfFontTextRun>,
    pub assets: Vec<FontAsset>,
}

impl PdfFontRunReport {
    #[inline]
    pub fn to_public_report(&self) -> FontRunReport {
        FontRunReport {
            input: self.input.clone(),
            runs: self.runs.iter().map(|run| run.run.clone()).collect(),
            assets: self.assets.clone(),
        }
    }
}
