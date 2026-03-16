use serde::{Deserialize, Serialize};

use crate::types::redaction_types::Rect;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualAnchorMetricsReport {
    pub input_redactions: String,
    pub input_anchors: String,
    pub analysis: VisualAnchorAnalysisConfig,
    pub pages: Vec<VisualAnchorPageSummary>,
    pub rows: Vec<VisualAnchorMetricRow>,
    pub summary: VisualAnchorMetricsSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualAnchorAnalysisConfig {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualAnchorPageSummary {
    pub page_index: u32,
    pub page_box: Rect,
    pub width_px: u32,
    pub height_px: u32,
    pub row_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualAnchorMetricsSummary {
    pub row_count: u32,
    pub current_anchor_count: u32,
    pub current_anchor_visible_count: u32,
    pub current_anchor_empty_count: u32,
    pub row_current_anchor_empty_count: u32,
    pub row_visual_neighbor_available_count: u32,
    pub row_visual_grouped_span_available_count: u32,
    pub likely_hidden_text_layer_anchor_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualAnchorMetricRow {
    pub row_id: String,
    pub redaction_id: String,
    pub page_index: u32,
    pub redaction_bbox: Rect,
    pub current_anchor_mode: String,
    #[serde(default)]
    pub current_left: Option<VisualCurrentAnchorSide>,
    #[serde(default)]
    pub current_right: Option<VisualCurrentAnchorSide>,
    #[serde(default)]
    pub redaction_dark_component: Option<VisualDarkComponent>,
    #[serde(default)]
    pub nearest_left: Option<VisualComponentSpan>,
    #[serde(default)]
    pub nearest_right: Option<VisualComponentSpan>,
    #[serde(default)]
    pub grouped_left: Option<VisualComponentSpan>,
    #[serde(default)]
    pub grouped_right: Option<VisualComponentSpan>,
    pub width_comparison: VisualAnchorWidthComparison,
    pub flags: VisualAnchorRowFlags,
    pub search_window_bbox: Rect,
    pub crop_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualCurrentAnchorSide {
    pub text: String,
    pub bbox: Rect,
    pub edge_x_pt: f32,
    #[serde(default)]
    pub gap_pt: Option<f32>,
    pub dark_pixel_count: u32,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualDarkComponent {
    pub bbox: Rect,
    pub width_pt: f32,
    pub height_pt: f32,
    pub pixel_area: u32,
    pub dark_pixel_count: u32,
    pub fill_ratio: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualComponentSpan {
    pub bbox: Rect,
    pub gap_pt: f32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub component_count: u32,
    pub pixel_area: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisualAnchorWidthComparison {
    pub redaction_box_width_pt: f32,
    #[serde(default)]
    pub redaction_dark_component_width_pt: Option<f32>,
    pub current_anchor_target_width_pt: f32,
    #[serde(default)]
    pub nearest_visual_span_width_pt: Option<f32>,
    #[serde(default)]
    pub grouped_visual_span_width_pt: Option<f32>,
    pub current_vs_redaction_box_delta_pt: f32,
    #[serde(default)]
    pub current_vs_redaction_dark_delta_pt: Option<f32>,
    #[serde(default)]
    pub current_vs_nearest_visual_delta_pt: Option<f32>,
    #[serde(default)]
    pub current_vs_grouped_visual_delta_pt: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualAnchorRowFlags {
    pub current_anchor_visually_empty: bool,
    pub visual_neighbor_available: bool,
    pub visual_grouped_span_available: bool,
    pub likely_hidden_text_layer_anchor: bool,
}
