use std::collections::BTreeMap;

use crate::data::types::visual_anchor_metric_types::{
    CollectVisualAnchorMetricsRequest, DataVisualAnchorAnalysisConfig, DataVisualAnchorMetricRow,
    DataVisualAnchorMetricsSummary, DataVisualAnchorPageSummary, DataVisualAnchorRowFlags,
    DataVisualAnchorWidthComparison, DataVisualComponentSpan, DataVisualCurrentAnchorSide,
    DataVisualDarkComponent, VisualAnchorCropImage, VisualAnchorMetricsDiagnostic,
    VisualAnchorMetricsSet, VisualAnchorRowInput,
};
use crate::dependency::visual_anchor_metrics_accessor::{
    CollectVisualAnchorMetricsDependencyOutput, CollectVisualAnchorMetricsDependencyRequest,
    DependencyComponentSpan, DependencyDarkComponent, DependencyVisualAnchorRowOutput,
    DependencyVisualAnchorRowRequest, VisualAnchorMetricsAccessor, GROUPED_SPAN_MAX_GAP_PX,
    GROUPED_SPAN_MIN_VERTICAL_OVERLAP_RATIO, MIN_COMPONENT_AREA_PX, RELAXED_LUMINANCE_MAX,
    SEARCH_HORIZONTAL_PADDING_PT, SEARCH_VERTICAL_PADDING_HEIGHT_RATIO,
    SEARCH_VERTICAL_PADDING_MIN_PT, STRICT_LUMINANCE_MAX, VISUAL_RENDER_DPI,
};
use crate::types::diagnostic_types::DiagnosticValue;

#[derive(Debug, Clone, Default)]
pub struct VisualAnchorMetricsData {
    accessor: VisualAnchorMetricsAccessor,
}

impl VisualAnchorMetricsData {
    #[inline]
    pub fn new() -> Self {
        Self {
            accessor: VisualAnchorMetricsAccessor::new(),
        }
    }

    #[inline]
    pub fn collect(
        &self,
        req: CollectVisualAnchorMetricsRequest<'_>,
    ) -> Result<VisualAnchorMetricsSet, String> {
        let dependency_rows = req
            .anchors
            .iter()
            .map(|row| DependencyVisualAnchorRowRequest {
                row_id: row.row_id.clone(),
                page_index: row.page_index,
                redaction_bbox: row.redaction_bbox,
                current_left_bbox: row.current_left.as_ref().map(|side| side.bbox),
                current_right_bbox: row.current_right.as_ref().map(|side| side.bbox),
            })
            .collect::<Vec<_>>();
        let dependency = self
            .accessor
            .collect(CollectVisualAnchorMetricsDependencyRequest {
                pdf_bytes: req.pdf_bytes,
                rows: dependency_rows.as_slice(),
            })
            .map_err(|error| format!("visual_anchor_metrics_dependency_failed:{error}"))?;
        Ok(build_metrics_set(req, dependency))
    }
}

fn build_metrics_set(
    req: CollectVisualAnchorMetricsRequest<'_>,
    dependency: CollectVisualAnchorMetricsDependencyOutput,
) -> VisualAnchorMetricsSet {
    let mut diagnostics = Vec::<VisualAnchorMetricsDiagnostic>::new();
    let pages = build_pages(req.collect_diagnostics, &dependency, &mut diagnostics);
    let rows = build_rows(
        req.anchors,
        &dependency,
        req.collect_diagnostics,
        &mut diagnostics,
    );
    let summary = build_summary(rows.as_slice());
    if req.collect_diagnostics {
        diagnostics.push(summary_diagnostic(&summary));
    }
    let crops = dependency
        .rows
        .iter()
        .map(|row| VisualAnchorCropImage {
            file_name: format!("{}.png", row.row_id),
            bytes: row.crop_png.clone(),
        })
        .collect();
    VisualAnchorMetricsSet {
        input_redactions: format!("memory://{}.redactions.json", req.input_name),
        input_anchors: format!("memory://{}.anchors.json", req.input_name),
        analysis: DataVisualAnchorAnalysisConfig {
            render_dpi: VISUAL_RENDER_DPI,
            strict_luminance_max: STRICT_LUMINANCE_MAX,
            relaxed_luminance_max: RELAXED_LUMINANCE_MAX,
            min_component_area_px: MIN_COMPONENT_AREA_PX,
            search_horizontal_padding_pt: SEARCH_HORIZONTAL_PADDING_PT,
            search_vertical_padding_min_pt: SEARCH_VERTICAL_PADDING_MIN_PT,
            search_vertical_padding_height_ratio: SEARCH_VERTICAL_PADDING_HEIGHT_RATIO,
            grouped_span_max_gap_px: GROUPED_SPAN_MAX_GAP_PX,
            grouped_span_min_vertical_overlap_ratio: GROUPED_SPAN_MIN_VERTICAL_OVERLAP_RATIO,
        },
        pages,
        rows,
        summary,
        diagnostics,
        crops,
    }
}

fn build_pages(
    collect_diagnostics: bool,
    dependency: &CollectVisualAnchorMetricsDependencyOutput,
    diagnostics: &mut Vec<VisualAnchorMetricsDiagnostic>,
) -> Vec<DataVisualAnchorPageSummary> {
    let mut pages = dependency
        .pages
        .iter()
        .map(|page| {
            if collect_diagnostics {
                diagnostics.push(page_diagnostic(page));
            }
            DataVisualAnchorPageSummary {
                page_index: page.page_index,
                page_box: page.page_box,
                width_px: page.width_px,
                height_px: page.height_px,
                row_count: page.row_count,
            }
        })
        .collect::<Vec<_>>();
    pages.sort_by(|left, right| left.page_index.cmp(&right.page_index));
    pages
}

fn build_rows(
    anchors: &[VisualAnchorRowInput],
    dependency: &CollectVisualAnchorMetricsDependencyOutput,
    collect_diagnostics: bool,
    diagnostics: &mut Vec<VisualAnchorMetricsDiagnostic>,
) -> Vec<DataVisualAnchorMetricRow> {
    let outputs_by_row = dependency
        .rows
        .iter()
        .map(|row| (row.row_id.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let mut rows = Vec::<DataVisualAnchorMetricRow>::new();
    for anchor in anchors {
        let Some(measurement) = outputs_by_row.get(anchor.row_id.as_str()) else {
            continue;
        };
        let current_left = anchor
            .current_left
            .as_ref()
            .map(|side| DataVisualCurrentAnchorSide {
                text: side.text.clone(),
                bbox: side.bbox,
                edge_x_pt: side.edge_x_pt,
                gap_pt: side.gap_pt,
                dark_pixel_count: measurement.current_left_dark_pixel_count.unwrap_or(0),
                visible: measurement.current_left_dark_pixel_count.unwrap_or(0) > 0,
            });
        let current_right = anchor
            .current_right
            .as_ref()
            .map(|side| DataVisualCurrentAnchorSide {
                text: side.text.clone(),
                bbox: side.bbox,
                edge_x_pt: side.edge_x_pt,
                gap_pt: side.gap_pt,
                dark_pixel_count: measurement.current_right_dark_pixel_count.unwrap_or(0),
                visible: measurement.current_right_dark_pixel_count.unwrap_or(0) > 0,
            });
        let redaction_dark_component = measurement
            .redaction_dark_component
            .as_ref()
            .map(map_dark_component);
        let nearest_left = measurement.nearest_left.as_ref().map(map_component_span);
        let nearest_right = measurement.nearest_right.as_ref().map(map_component_span);
        let grouped_left = measurement.grouped_left.as_ref().map(map_component_span);
        let grouped_right = measurement.grouped_right.as_ref().map(map_component_span);
        let flags = build_flags(
            current_left.as_ref(),
            current_right.as_ref(),
            nearest_left.as_ref(),
            nearest_right.as_ref(),
            grouped_left.as_ref(),
            grouped_right.as_ref(),
        );
        let width_comparison = build_width_comparison(
            anchor.current_target_width_pt,
            anchor.redaction_bbox.width().abs(),
            redaction_dark_component.as_ref(),
            nearest_left.as_ref(),
            nearest_right.as_ref(),
            grouped_left.as_ref(),
            grouped_right.as_ref(),
        );
        let row = DataVisualAnchorMetricRow {
            row_id: anchor.row_id.clone(),
            redaction_id: anchor.redaction_id.clone(),
            page_index: anchor.page_index,
            redaction_bbox: anchor.redaction_bbox,
            current_anchor_mode: anchor.current_anchor_mode.clone(),
            current_left,
            current_right,
            redaction_dark_component,
            nearest_left,
            nearest_right,
            grouped_left,
            grouped_right,
            width_comparison,
            flags,
            search_window_bbox: measurement.search_window_bbox,
            crop_file_name: format!("{}.png", anchor.row_id),
        };
        if collect_diagnostics {
            diagnostics.extend(row_diagnostics(&row, measurement));
        }
        rows.push(row);
    }
    rows.sort_by(|left, right| {
        left.page_index
            .cmp(&right.page_index)
            .then_with(|| left.row_id.cmp(&right.row_id))
    });
    rows
}

fn build_flags(
    current_left: Option<&DataVisualCurrentAnchorSide>,
    current_right: Option<&DataVisualCurrentAnchorSide>,
    nearest_left: Option<&DataVisualComponentSpan>,
    nearest_right: Option<&DataVisualComponentSpan>,
    grouped_left: Option<&DataVisualComponentSpan>,
    grouped_right: Option<&DataVisualComponentSpan>,
) -> DataVisualAnchorRowFlags {
    let current_anchor_visually_empty = current_left.is_some_and(|side| !side.visible)
        || current_right.is_some_and(|side| !side.visible);
    let visual_neighbor_available = nearest_left.is_some() || nearest_right.is_some();
    let visual_grouped_span_available = grouped_left.is_some() || grouped_right.is_some();
    let likely_hidden_text_layer_anchor = (current_left.is_some_and(|side| !side.visible)
        && nearest_left.is_some())
        || (current_right.is_some_and(|side| !side.visible) && nearest_right.is_some());
    DataVisualAnchorRowFlags {
        current_anchor_visually_empty,
        visual_neighbor_available,
        visual_grouped_span_available,
        likely_hidden_text_layer_anchor,
    }
}

fn build_width_comparison(
    current_anchor_target_width_pt: f32,
    redaction_box_width_pt: f32,
    redaction_dark_component: Option<&DataVisualDarkComponent>,
    nearest_left: Option<&DataVisualComponentSpan>,
    nearest_right: Option<&DataVisualComponentSpan>,
    grouped_left: Option<&DataVisualComponentSpan>,
    grouped_right: Option<&DataVisualComponentSpan>,
) -> DataVisualAnchorWidthComparison {
    let redaction_dark_component_width_pt =
        redaction_dark_component.map(|component| component.width_pt);
    let nearest_visual_span_width_pt = span_width(nearest_left, nearest_right);
    let grouped_visual_span_width_pt = span_width(grouped_left, grouped_right);
    DataVisualAnchorWidthComparison {
        redaction_box_width_pt,
        redaction_dark_component_width_pt,
        current_anchor_target_width_pt,
        nearest_visual_span_width_pt,
        grouped_visual_span_width_pt,
        current_vs_redaction_box_delta_pt: current_anchor_target_width_pt - redaction_box_width_pt,
        current_vs_redaction_dark_delta_pt: redaction_dark_component_width_pt
            .map(|width| current_anchor_target_width_pt - width),
        current_vs_nearest_visual_delta_pt: nearest_visual_span_width_pt
            .map(|width| current_anchor_target_width_pt - width),
        current_vs_grouped_visual_delta_pt: grouped_visual_span_width_pt
            .map(|width| current_anchor_target_width_pt - width),
    }
}

fn span_width(
    left: Option<&DataVisualComponentSpan>,
    right: Option<&DataVisualComponentSpan>,
) -> Option<f32> {
    Some(right?.bbox.x0 - left?.bbox.x1)
}

fn build_summary(rows: &[DataVisualAnchorMetricRow]) -> DataVisualAnchorMetricsSummary {
    let mut summary = DataVisualAnchorMetricsSummary {
        row_count: rows.len() as u32,
        current_anchor_count: 0,
        current_anchor_visible_count: 0,
        current_anchor_empty_count: 0,
        row_current_anchor_empty_count: 0,
        row_visual_neighbor_available_count: 0,
        row_visual_grouped_span_available_count: 0,
        likely_hidden_text_layer_anchor_count: 0,
    };
    for row in rows {
        for side in [row.current_left.as_ref(), row.current_right.as_ref()]
            .into_iter()
            .flatten()
        {
            summary.current_anchor_count = summary.current_anchor_count.saturating_add(1);
            if side.visible {
                summary.current_anchor_visible_count =
                    summary.current_anchor_visible_count.saturating_add(1);
            } else {
                summary.current_anchor_empty_count =
                    summary.current_anchor_empty_count.saturating_add(1);
            }
        }
        if row.flags.current_anchor_visually_empty {
            summary.row_current_anchor_empty_count =
                summary.row_current_anchor_empty_count.saturating_add(1);
        }
        if row.flags.visual_neighbor_available {
            summary.row_visual_neighbor_available_count = summary
                .row_visual_neighbor_available_count
                .saturating_add(1);
        }
        if row.flags.visual_grouped_span_available {
            summary.row_visual_grouped_span_available_count = summary
                .row_visual_grouped_span_available_count
                .saturating_add(1);
        }
        if row.flags.likely_hidden_text_layer_anchor {
            summary.likely_hidden_text_layer_anchor_count = summary
                .likely_hidden_text_layer_anchor_count
                .saturating_add(1);
        }
    }
    summary
}

fn map_dark_component(component: &DependencyDarkComponent) -> DataVisualDarkComponent {
    DataVisualDarkComponent {
        bbox: component.bbox,
        width_pt: component.width_pt,
        height_pt: component.height_pt,
        pixel_area: component.pixel_area,
        dark_pixel_count: component.dark_pixel_count,
        fill_ratio: component.fill_ratio,
    }
}

fn map_component_span(span: &DependencyComponentSpan) -> DataVisualComponentSpan {
    DataVisualComponentSpan {
        bbox: span.bbox,
        gap_pt: span.gap_pt,
        width_pt: span.width_pt,
        height_pt: span.height_pt,
        component_count: span.component_count,
        pixel_area: span.pixel_area,
    }
}

fn page_diagnostic(
    page: &crate::dependency::visual_anchor_metrics_accessor::DependencyVisualPageSummary,
) -> VisualAnchorMetricsDiagnostic {
    let mut metrics = BTreeMap::<String, DiagnosticValue>::new();
    metrics.insert(
        "width_px".to_owned(),
        DiagnosticValue::Integer(page.width_px as i64),
    );
    metrics.insert(
        "height_px".to_owned(),
        DiagnosticValue::Integer(page.height_px as i64),
    );
    metrics.insert(
        "row_count".to_owned(),
        DiagnosticValue::Integer(page.row_count as i64),
    );
    VisualAnchorMetricsDiagnostic {
        row_id: None,
        redaction_id: None,
        page_index: Some(page.page_index),
        bbox: Some(page.page_box),
        code: "visual_page_rendered".to_owned(),
        message: "visual page rendered".to_owned(),
        is_warning: false,
        metrics,
    }
}

fn row_diagnostics(
    row: &DataVisualAnchorMetricRow,
    measurement: &DependencyVisualAnchorRowOutput,
) -> Vec<VisualAnchorMetricsDiagnostic> {
    let mut diagnostics = Vec::<VisualAnchorMetricsDiagnostic>::new();
    if let Some(component) = row.redaction_dark_component.as_ref() {
        diagnostics.push(component_diagnostic(row, component));
    }
    if let Some(side) = row.current_left.as_ref() {
        diagnostics.push(current_anchor_diagnostic(row, side, "left"));
        if !side.visible {
            diagnostics.push(current_anchor_empty_diagnostic(row, side.bbox, "left"));
        }
    }
    if let Some(side) = row.current_right.as_ref() {
        diagnostics.push(current_anchor_diagnostic(row, side, "right"));
        if !side.visible {
            diagnostics.push(current_anchor_empty_diagnostic(row, side.bbox, "right"));
        }
    }
    if let Some(span) = row.nearest_left.as_ref() {
        diagnostics.push(component_span_diagnostic(
            row,
            span,
            "visual_nearest_component_selected",
            "left",
        ));
    }
    if let Some(span) = row.nearest_right.as_ref() {
        diagnostics.push(component_span_diagnostic(
            row,
            span,
            "visual_nearest_component_selected",
            "right",
        ));
    }
    if let Some(span) = row.grouped_left.as_ref() {
        diagnostics.push(component_span_diagnostic(
            row,
            span,
            "visual_grouped_span_selected",
            "left",
        ));
    }
    if let Some(span) = row.grouped_right.as_ref() {
        diagnostics.push(component_span_diagnostic(
            row,
            span,
            "visual_grouped_span_selected",
            "right",
        ));
    }
    diagnostics.push(width_comparison_diagnostic(row));
    diagnostics.push(VisualAnchorMetricsDiagnostic {
        row_id: Some(row.row_id.clone()),
        redaction_id: Some(row.redaction_id.clone()),
        page_index: Some(row.page_index),
        bbox: Some(measurement.search_window_bbox),
        code: "visual_search_window".to_owned(),
        message: "visual search window measured".to_owned(),
        is_warning: false,
        metrics: BTreeMap::new(),
    });
    diagnostics
}

fn component_diagnostic(
    row: &DataVisualAnchorMetricRow,
    component: &DataVisualDarkComponent,
) -> VisualAnchorMetricsDiagnostic {
    let mut metrics = BTreeMap::<String, DiagnosticValue>::new();
    metrics.insert(
        "pixel_area".to_owned(),
        DiagnosticValue::Integer(component.pixel_area as i64),
    );
    metrics.insert(
        "fill_ratio".to_owned(),
        DiagnosticValue::Float(component.fill_ratio as f64),
    );
    metrics.insert(
        "width_pt".to_owned(),
        DiagnosticValue::Float(component.width_pt as f64),
    );
    metrics.insert(
        "height_pt".to_owned(),
        DiagnosticValue::Float(component.height_pt as f64),
    );
    VisualAnchorMetricsDiagnostic {
        row_id: Some(row.row_id.clone()),
        redaction_id: Some(row.redaction_id.clone()),
        page_index: Some(row.page_index),
        bbox: Some(component.bbox),
        code: "visual_redaction_component_measured".to_owned(),
        message: "redaction dark component measured".to_owned(),
        is_warning: false,
        metrics,
    }
}

fn current_anchor_diagnostic(
    row: &DataVisualAnchorMetricRow,
    side: &DataVisualCurrentAnchorSide,
    side_name: &str,
) -> VisualAnchorMetricsDiagnostic {
    let mut metrics = BTreeMap::<String, DiagnosticValue>::new();
    metrics.insert(
        "side".to_owned(),
        DiagnosticValue::Text(side_name.to_owned()),
    );
    metrics.insert(
        "dark_pixel_count".to_owned(),
        DiagnosticValue::Integer(side.dark_pixel_count as i64),
    );
    metrics.insert("visible".to_owned(), DiagnosticValue::Bool(side.visible));
    if let Some(gap_pt) = side.gap_pt {
        metrics.insert("gap_pt".to_owned(), DiagnosticValue::Float(gap_pt as f64));
    }
    VisualAnchorMetricsDiagnostic {
        row_id: Some(row.row_id.clone()),
        redaction_id: Some(row.redaction_id.clone()),
        page_index: Some(row.page_index),
        bbox: Some(side.bbox),
        code: "visual_current_anchor_measured".to_owned(),
        message: "current anchor visually measured".to_owned(),
        is_warning: false,
        metrics,
    }
}

fn current_anchor_empty_diagnostic(
    row: &DataVisualAnchorMetricRow,
    bbox: crate::types::redaction_types::Rect,
    side_name: &str,
) -> VisualAnchorMetricsDiagnostic {
    let mut metrics = BTreeMap::<String, DiagnosticValue>::new();
    metrics.insert(
        "side".to_owned(),
        DiagnosticValue::Text(side_name.to_owned()),
    );
    VisualAnchorMetricsDiagnostic {
        row_id: Some(row.row_id.clone()),
        redaction_id: Some(row.redaction_id.clone()),
        page_index: Some(row.page_index),
        bbox: Some(bbox),
        code: "visual_current_anchor_empty".to_owned(),
        message: "current anchor bbox has no visible dark pixels".to_owned(),
        is_warning: true,
        metrics,
    }
}

fn component_span_diagnostic(
    row: &DataVisualAnchorMetricRow,
    span: &DataVisualComponentSpan,
    code: &str,
    side_name: &str,
) -> VisualAnchorMetricsDiagnostic {
    let mut metrics = BTreeMap::<String, DiagnosticValue>::new();
    metrics.insert(
        "side".to_owned(),
        DiagnosticValue::Text(side_name.to_owned()),
    );
    metrics.insert(
        "gap_pt".to_owned(),
        DiagnosticValue::Float(span.gap_pt as f64),
    );
    metrics.insert(
        "width_pt".to_owned(),
        DiagnosticValue::Float(span.width_pt as f64),
    );
    metrics.insert(
        "component_count".to_owned(),
        DiagnosticValue::Integer(span.component_count as i64),
    );
    VisualAnchorMetricsDiagnostic {
        row_id: Some(row.row_id.clone()),
        redaction_id: Some(row.redaction_id.clone()),
        page_index: Some(row.page_index),
        bbox: Some(span.bbox),
        code: code.to_owned(),
        message: "visual component span selected".to_owned(),
        is_warning: false,
        metrics,
    }
}

fn width_comparison_diagnostic(row: &DataVisualAnchorMetricRow) -> VisualAnchorMetricsDiagnostic {
    let mut metrics = BTreeMap::<String, DiagnosticValue>::new();
    metrics.insert(
        "redaction_box_width_pt".to_owned(),
        DiagnosticValue::Float(row.width_comparison.redaction_box_width_pt as f64),
    );
    metrics.insert(
        "current_anchor_target_width_pt".to_owned(),
        DiagnosticValue::Float(row.width_comparison.current_anchor_target_width_pt as f64),
    );
    if let Some(value) = row.width_comparison.redaction_dark_component_width_pt {
        metrics.insert(
            "redaction_dark_component_width_pt".to_owned(),
            DiagnosticValue::Float(value as f64),
        );
    }
    if let Some(value) = row.width_comparison.nearest_visual_span_width_pt {
        metrics.insert(
            "nearest_visual_span_width_pt".to_owned(),
            DiagnosticValue::Float(value as f64),
        );
    }
    if let Some(value) = row.width_comparison.grouped_visual_span_width_pt {
        metrics.insert(
            "grouped_visual_span_width_pt".to_owned(),
            DiagnosticValue::Float(value as f64),
        );
    }
    VisualAnchorMetricsDiagnostic {
        row_id: Some(row.row_id.clone()),
        redaction_id: Some(row.redaction_id.clone()),
        page_index: Some(row.page_index),
        bbox: Some(row.redaction_bbox),
        code: "visual_width_comparison".to_owned(),
        message: "visual width comparison computed".to_owned(),
        is_warning: false,
        metrics,
    }
}

fn summary_diagnostic(summary: &DataVisualAnchorMetricsSummary) -> VisualAnchorMetricsDiagnostic {
    let mut metrics = BTreeMap::<String, DiagnosticValue>::new();
    metrics.insert(
        "row_count".to_owned(),
        DiagnosticValue::Integer(summary.row_count as i64),
    );
    metrics.insert(
        "current_anchor_empty_count".to_owned(),
        DiagnosticValue::Integer(summary.current_anchor_empty_count as i64),
    );
    metrics.insert(
        "row_current_anchor_empty_count".to_owned(),
        DiagnosticValue::Integer(summary.row_current_anchor_empty_count as i64),
    );
    metrics.insert(
        "likely_hidden_text_layer_anchor_count".to_owned(),
        DiagnosticValue::Integer(summary.likely_hidden_text_layer_anchor_count as i64),
    );
    VisualAnchorMetricsDiagnostic {
        row_id: None,
        redaction_id: None,
        page_index: None,
        bbox: None,
        code: "visual_anchor_metric_summary".to_owned(),
        message: "visual anchor metric summary computed".to_owned(),
        is_warning: false,
        metrics,
    }
}
