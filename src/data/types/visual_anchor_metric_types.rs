use std::collections::BTreeMap;

use crate::types::diagnostic_types::DiagnosticValue;
use crate::types::redaction_types::Rect;

#[derive(Debug, Clone, PartialEq)]
pub struct CollectVisualAnchorMetricsRequest<'a> {
    pub input_name: &'a str,
    pub anchors: &'a [VisualAnchorRowInput],
    pub pdf_bytes: &'a [u8],
    pub collect_diagnostics: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualAnchorRowInput {
    pub row_id: String,
    pub redaction_id: String,
    pub page_index: u32,
    pub redaction_bbox: Rect,
    pub current_anchor_mode: String,
    pub current_target_width_pt: f32,
    pub current_left: Option<VisualAnchorSideInput>,
    pub current_right: Option<VisualAnchorSideInput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualAnchorSideInput {
    pub text: String,
    pub bbox: Rect,
    pub edge_x_pt: f32,
    pub gap_pt: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualAnchorMetricsSet {
    pub input_redactions: String,
    pub input_anchors: String,
    pub analysis: DataVisualAnchorAnalysisConfig,
    pub pages: Vec<DataVisualAnchorPageSummary>,
    pub rows: Vec<DataVisualAnchorMetricRow>,
    pub summary: DataVisualAnchorMetricsSummary,
    pub diagnostics: Vec<VisualAnchorMetricsDiagnostic>,
    pub crops: Vec<VisualAnchorCropImage>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataVisualAnchorAnalysisConfig {
    pub render_dpi: u16,
    pub strict_luminance_max: u8,
    pub relaxed_luminance_max: u8,
    pub min_component_area_px: u32,
    pub search_horizontal_padding_pt: f32,
    pub search_vertical_padding_min_pt: f32,
    pub search_vertical_padding_height_ratio: f32,
    pub grouped_span_max_gap_px: u32,
    pub grouped_span_min_vertical_overlap_ratio: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataVisualAnchorPageSummary {
    pub page_index: u32,
    pub page_box: Rect,
    pub width_px: u32,
    pub height_px: u32,
    pub row_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataVisualAnchorMetricsSummary {
    pub row_count: u32,
    pub current_anchor_count: u32,
    pub current_anchor_visible_count: u32,
    pub current_anchor_empty_count: u32,
    pub row_current_anchor_empty_count: u32,
    pub row_visual_neighbor_available_count: u32,
    pub row_visual_grouped_span_available_count: u32,
    pub likely_hidden_text_layer_anchor_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataVisualAnchorMetricRow {
    pub row_id: String,
    pub redaction_id: String,
    pub page_index: u32,
    pub redaction_bbox: Rect,
    pub current_anchor_mode: String,
    pub current_left: Option<DataVisualCurrentAnchorSide>,
    pub current_right: Option<DataVisualCurrentAnchorSide>,
    pub redaction_dark_component: Option<DataVisualDarkComponent>,
    pub nearest_left: Option<DataVisualComponentSpan>,
    pub nearest_right: Option<DataVisualComponentSpan>,
    pub grouped_left: Option<DataVisualComponentSpan>,
    pub grouped_right: Option<DataVisualComponentSpan>,
    pub width_comparison: DataVisualAnchorWidthComparison,
    pub flags: DataVisualAnchorRowFlags,
    pub search_window_bbox: Rect,
    pub crop_file_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataVisualCurrentAnchorSide {
    pub text: String,
    pub bbox: Rect,
    pub edge_x_pt: f32,
    pub gap_pt: Option<f32>,
    pub dark_pixel_count: u32,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataVisualDarkComponent {
    pub bbox: Rect,
    pub width_pt: f32,
    pub height_pt: f32,
    pub pixel_area: u32,
    pub dark_pixel_count: u32,
    pub fill_ratio: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataVisualComponentSpan {
    pub bbox: Rect,
    pub gap_pt: f32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub component_count: u32,
    pub pixel_area: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataVisualAnchorWidthComparison {
    pub redaction_box_width_pt: f32,
    pub redaction_dark_component_width_pt: Option<f32>,
    pub current_anchor_target_width_pt: f32,
    pub nearest_visual_span_width_pt: Option<f32>,
    pub grouped_visual_span_width_pt: Option<f32>,
    pub current_vs_redaction_box_delta_pt: f32,
    pub current_vs_redaction_dark_delta_pt: Option<f32>,
    pub current_vs_nearest_visual_delta_pt: Option<f32>,
    pub current_vs_grouped_visual_delta_pt: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataVisualAnchorRowFlags {
    pub current_anchor_visually_empty: bool,
    pub visual_neighbor_available: bool,
    pub visual_grouped_span_available: bool,
    pub likely_hidden_text_layer_anchor: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualAnchorCropImage {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisualAnchorMetricsDiagnostic {
    pub row_id: Option<String>,
    pub redaction_id: Option<String>,
    pub page_index: Option<u32>,
    pub bbox: Option<Rect>,
    pub code: String,
    pub message: String,
    pub is_warning: bool,
    pub metrics: BTreeMap<String, DiagnosticValue>,
}
