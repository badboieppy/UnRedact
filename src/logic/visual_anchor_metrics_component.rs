use crate::data::types::visual_anchor_metric_types::{
    CollectVisualAnchorMetricsRequest, DataVisualAnchorMetricRow, VisualAnchorMetricsSet,
    VisualAnchorRowInput, VisualAnchorSideInput,
};
use crate::data::visual_anchor_metrics_data::VisualAnchorMetricsData;
use crate::logic::types::NamedBinaryArtifact;
use crate::types::diagnostic_types::{DiagnosticLevel, DiagnosticRecord};
use crate::types::guess_types::{AnchorDecisionRecord, AnchorSideDecision, GuessReport};
use crate::types::visual_anchor_metric_types::{
    VisualAnchorAnalysisConfig, VisualAnchorMetricRow, VisualAnchorMetricsReport,
    VisualAnchorMetricsSummary, VisualAnchorPageSummary, VisualAnchorRowFlags,
    VisualAnchorWidthComparison, VisualComponentSpan, VisualCurrentAnchorSide, VisualDarkComponent,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunVisualAnchorMetricsRequest<'a> {
    pub pdf_name: &'a str,
    pub pdf_bytes: &'a [u8],
    pub guesses: &'a GuessReport,
    pub collect_diagnostics: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RunVisualAnchorMetricsOutput {
    pub report: VisualAnchorMetricsReport,
    pub diagnostics: Vec<DiagnosticRecord>,
    pub crops: Vec<NamedBinaryArtifact>,
}

#[inline]
pub(crate) fn run_visual_anchor_metrics(
    req: RunVisualAnchorMetricsRequest<'_>,
) -> Result<RunVisualAnchorMetricsOutput, String> {
    let data = VisualAnchorMetricsData::new();
    let row_inputs = req
        .guesses
        .anchors
        .iter()
        .map(map_anchor_input)
        .collect::<Vec<_>>();
    let set = data.collect(CollectVisualAnchorMetricsRequest {
        input_name: req.pdf_name,
        anchors: row_inputs.as_slice(),
        pdf_bytes: req.pdf_bytes,
        collect_diagnostics: req.collect_diagnostics,
    })?;
    Ok(RunVisualAnchorMetricsOutput {
        report: map_report(&set),
        diagnostics: set.diagnostics.iter().map(translate_diagnostic).collect(),
        crops: set
            .crops
            .iter()
            .map(|crop| NamedBinaryArtifact {
                file_name: crop.file_name.clone(),
                bytes: crop.bytes.clone(),
            })
            .collect(),
    })
}

fn map_anchor_input(anchor: &AnchorDecisionRecord) -> VisualAnchorRowInput {
    VisualAnchorRowInput {
        row_id: anchor.anchor_row_id.clone(),
        redaction_id: anchor
            .redaction_id
            .clone()
            .unwrap_or_else(|| anchor.anchor_row_id.clone()),
        page_index: anchor.page_index,
        redaction_bbox: anchor.bbox,
        current_anchor_mode: anchor.anchor_mode.clone(),
        current_target_width_pt: anchor.target_width_pt,
        current_left: anchor
            .left
            .as_ref()
            .map(|side| map_anchor_side_input(side, anchor.selected_left_gap_pt)),
        current_right: anchor
            .right
            .as_ref()
            .map(|side| map_anchor_side_input(side, anchor.selected_right_gap_pt)),
    }
}

fn map_anchor_side_input(side: &AnchorSideDecision, gap_pt: Option<f32>) -> VisualAnchorSideInput {
    VisualAnchorSideInput {
        text: side.text.clone(),
        bbox: side.bbox,
        edge_x_pt: side.x,
        gap_pt,
    }
}

fn map_report(set: &VisualAnchorMetricsSet) -> VisualAnchorMetricsReport {
    VisualAnchorMetricsReport {
        input_redactions: set.input_redactions.clone(),
        input_anchors: set.input_anchors.clone(),
        analysis: VisualAnchorAnalysisConfig {
            render_dpi: set.analysis.render_dpi,
            strict_luminance_max: set.analysis.strict_luminance_max,
            relaxed_luminance_max: set.analysis.relaxed_luminance_max,
            min_component_area_px: set.analysis.min_component_area_px,
            search_horizontal_padding_pt: set.analysis.search_horizontal_padding_pt,
            search_vertical_padding_min_pt: set.analysis.search_vertical_padding_min_pt,
            search_vertical_padding_height_ratio: set.analysis.search_vertical_padding_height_ratio,
            grouped_span_max_gap_px: set.analysis.grouped_span_max_gap_px,
            grouped_span_min_vertical_overlap_ratio: set
                .analysis
                .grouped_span_min_vertical_overlap_ratio,
        },
        pages: set
            .pages
            .iter()
            .map(|page| VisualAnchorPageSummary {
                page_index: page.page_index,
                page_box: page.page_box,
                width_px: page.width_px,
                height_px: page.height_px,
                row_count: page.row_count,
            })
            .collect(),
        rows: set.rows.iter().map(map_row).collect(),
        summary: VisualAnchorMetricsSummary {
            row_count: set.summary.row_count,
            current_anchor_count: set.summary.current_anchor_count,
            current_anchor_visible_count: set.summary.current_anchor_visible_count,
            current_anchor_empty_count: set.summary.current_anchor_empty_count,
            row_current_anchor_empty_count: set.summary.row_current_anchor_empty_count,
            row_visual_neighbor_available_count: set.summary.row_visual_neighbor_available_count,
            row_visual_grouped_span_available_count: set
                .summary
                .row_visual_grouped_span_available_count,
            likely_hidden_text_layer_anchor_count: set
                .summary
                .likely_hidden_text_layer_anchor_count,
        },
    }
}

fn map_row(row: &DataVisualAnchorMetricRow) -> VisualAnchorMetricRow {
    VisualAnchorMetricRow {
        row_id: row.row_id.clone(),
        redaction_id: row.redaction_id.clone(),
        page_index: row.page_index,
        redaction_bbox: row.redaction_bbox,
        current_anchor_mode: row.current_anchor_mode.clone(),
        current_left: row.current_left.as_ref().map(map_current_anchor_side),
        current_right: row.current_right.as_ref().map(map_current_anchor_side),
        redaction_dark_component: row
            .redaction_dark_component
            .as_ref()
            .map(map_dark_component),
        nearest_left: row.nearest_left.as_ref().map(map_component_span),
        nearest_right: row.nearest_right.as_ref().map(map_component_span),
        grouped_left: row.grouped_left.as_ref().map(map_component_span),
        grouped_right: row.grouped_right.as_ref().map(map_component_span),
        width_comparison: VisualAnchorWidthComparison {
            redaction_box_width_pt: row.width_comparison.redaction_box_width_pt,
            redaction_dark_component_width_pt: row
                .width_comparison
                .redaction_dark_component_width_pt,
            current_anchor_target_width_pt: row.width_comparison.current_anchor_target_width_pt,
            nearest_visual_span_width_pt: row.width_comparison.nearest_visual_span_width_pt,
            grouped_visual_span_width_pt: row.width_comparison.grouped_visual_span_width_pt,
            current_vs_redaction_box_delta_pt: row
                .width_comparison
                .current_vs_redaction_box_delta_pt,
            current_vs_redaction_dark_delta_pt: row
                .width_comparison
                .current_vs_redaction_dark_delta_pt,
            current_vs_nearest_visual_delta_pt: row
                .width_comparison
                .current_vs_nearest_visual_delta_pt,
            current_vs_grouped_visual_delta_pt: row
                .width_comparison
                .current_vs_grouped_visual_delta_pt,
        },
        flags: VisualAnchorRowFlags {
            current_anchor_visually_empty: row.flags.current_anchor_visually_empty,
            visual_neighbor_available: row.flags.visual_neighbor_available,
            visual_grouped_span_available: row.flags.visual_grouped_span_available,
            likely_hidden_text_layer_anchor: row.flags.likely_hidden_text_layer_anchor,
        },
        search_window_bbox: row.search_window_bbox,
        crop_path: format!("visual_crops/{}", row.crop_file_name),
    }
}

fn map_current_anchor_side(
    side: &crate::data::types::visual_anchor_metric_types::DataVisualCurrentAnchorSide,
) -> VisualCurrentAnchorSide {
    VisualCurrentAnchorSide {
        text: side.text.clone(),
        bbox: side.bbox,
        edge_x_pt: side.edge_x_pt,
        gap_pt: side.gap_pt,
        dark_pixel_count: side.dark_pixel_count,
        visible: side.visible,
    }
}

fn map_dark_component(
    component: &crate::data::types::visual_anchor_metric_types::DataVisualDarkComponent,
) -> VisualDarkComponent {
    VisualDarkComponent {
        bbox: component.bbox,
        width_pt: component.width_pt,
        height_pt: component.height_pt,
        pixel_area: component.pixel_area,
        dark_pixel_count: component.dark_pixel_count,
        fill_ratio: component.fill_ratio,
    }
}

fn map_component_span(
    span: &crate::data::types::visual_anchor_metric_types::DataVisualComponentSpan,
) -> VisualComponentSpan {
    VisualComponentSpan {
        bbox: span.bbox,
        gap_pt: span.gap_pt,
        width_pt: span.width_pt,
        height_pt: span.height_pt,
        component_count: span.component_count,
        pixel_area: span.pixel_area,
    }
}

fn translate_diagnostic(
    diagnostic: &crate::data::types::visual_anchor_metric_types::VisualAnchorMetricsDiagnostic,
) -> DiagnosticRecord {
    let mut record = if diagnostic.is_warning {
        DiagnosticRecord::warning(
            "data",
            "visual_anchor_metrics",
            &diagnostic.code,
            &diagnostic.message,
        )
    } else {
        DiagnosticRecord::info("data", "visual_anchor_metrics", &diagnostic.code)
    };
    if !diagnostic.is_warning {
        record.level = DiagnosticLevel::Info;
        record.message = Some(diagnostic.message.clone());
    }
    record.row_id = diagnostic.row_id.clone();
    record.redaction_id = diagnostic.redaction_id.clone();
    record.page_index = diagnostic.page_index;
    record.bbox = diagnostic.bbox;
    record.metrics = diagnostic.metrics.clone();
    record
}
