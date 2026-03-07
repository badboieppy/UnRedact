use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypographyWidthSource {
    PdfWidthTable,
    CoreFont,
    Fallback,
}

impl TypographyWidthSource {
    #[inline]
    pub fn as_str(self) -> &'static str {
        match self {
            TypographyWidthSource::PdfWidthTable => "pdf_width_table",
            TypographyWidthSource::CoreFont => "core_font",
            TypographyWidthSource::Fallback => "fallback",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypographyMeasuredWidth {
    pub pt: f64,
    pub source: TypographyWidthSource,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TypographyWidthTableKey {
    pub page_index: u32,
    pub font_key: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypographyWidthTable {
    pub first_char: u16,
    pub widths: Vec<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct TypographyProfile {
    width_tables: BTreeMap<TypographyWidthTableKey, TypographyWidthTable>,
}

impl TypographyProfile {
    #[inline]
    pub fn from_width_tables(
        width_tables: BTreeMap<TypographyWidthTableKey, TypographyWidthTable>,
    ) -> Self {
        Self { width_tables }
    }

    #[inline]
    pub fn width_table_count(&self) -> usize {
        self.width_tables.len()
    }

    #[inline]
    pub fn has_width_table_for_font(&self, page_index: u32, font_key: &str) -> bool {
        self.width_tables.contains_key(&TypographyWidthTableKey {
            page_index,
            font_key: font_key.to_owned(),
        })
    }

    #[inline]
    pub(crate) fn width_table(
        &self,
        page_index: u32,
        font_key: &str,
    ) -> Option<&TypographyWidthTable> {
        self.width_tables.get(&TypographyWidthTableKey {
            page_index,
            font_key: font_key.to_owned(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TypographyMeasureInput<'a> {
    pub page_index: u32,
    pub font_key: &'a str,
    pub font_name: &'a str,
    pub font_size_pt: f32,
    pub h_scale_pct: f32,
    pub text: &'a str,
    pub metrics_dpi: f32,
}
