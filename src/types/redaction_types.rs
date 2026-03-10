use crate::types::runtime_defaults::RASTER_HIGHPASS_DPI;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    #[inline]
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        let (min_x, max_x) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (min_y, max_y) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        Self {
            x0: min_x,
            y0: min_y,
            x1: max_x,
            y1: max_y,
        }
    }

    #[inline]
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    #[inline]
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }

    #[inline]
    pub fn area(&self) -> f32 {
        self.width().max(0.0) * self.height().max(0.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnderlyingTextHit {
    pub page_index: u32,
    pub bbox: Rect,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionOccurrence {
    pub page_index: u32,
    pub bbox: Rect,
    pub kind: RedactionKind,
    pub score: f32,
    pub meta: BTreeMap<String, String>,
    pub underlying_text: Vec<UnderlyingTextHit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionKind {
    Annotation,
    DrawnRect,
    DrawnPathRect,
    RasterDarkRegion,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionReport {
    pub input: String,
    pub redactions: Vec<RedactionOccurrence>,
    pub count: u32,
    pub page_counts: BTreeMap<u32, u32>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionMode {
    Annotations,
    Drawn,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RedactionFinderConfig {
    pub include_details: bool,
    pub mode: RedactionMode,
    pub include_full_page_rects: bool,
    pub enable_image_analysis: bool,
    pub raster_dpi: f32,
}

impl Default for RedactionFinderConfig {
    #[inline]
    fn default() -> Self {
        Self {
            include_details: false,
            mode: RedactionMode::All,
            include_full_page_rects: false,
            enable_image_analysis: true,
            raster_dpi: RASTER_HIGHPASS_DPI,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RedactionFinderOutput {
    pub redactions: Vec<RedactionOccurrence>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RenderedPage {
    pub width_px: u32,
    pub height_px: u32,
    pub dpi: f32,
    pub pixels: Vec<u8>,
}

pub trait PdfRenderer {
    fn page_count(&self) -> usize;
    fn render_page_to_rgba(
        &self,
        page_index: usize,
        target_dpi: f32,
    ) -> Result<RenderedPage, String>;
}
