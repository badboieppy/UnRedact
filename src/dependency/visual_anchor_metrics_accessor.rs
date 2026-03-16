use std::collections::{BTreeMap, VecDeque};

use image::{ImageBuffer, Rgba};
use lopdf::Document;

use crate::dependency::hayro_renderer::HayroRenderer;
use crate::dependency::pdf_redaction::page_render_box_from_page;
use crate::types::redaction_types::{PdfRenderer as _, Rect};

pub(crate) const VISUAL_RENDER_DPI: u16 = 200;
pub(crate) const STRICT_LUMINANCE_MAX: u8 = 180;
pub(crate) const RELAXED_LUMINANCE_MAX: u8 = 220;
pub(crate) const MIN_COMPONENT_AREA_PX: u32 = 8;
pub(crate) const SEARCH_HORIZONTAL_PADDING_PT: f32 = 200.0_f32;
pub(crate) const SEARCH_VERTICAL_PADDING_MIN_PT: f32 = 18.0_f32;
pub(crate) const SEARCH_VERTICAL_PADDING_HEIGHT_RATIO: f32 = 1.5_f32;
pub(crate) const GROUPED_SPAN_MAX_GAP_PX: u32 = 6;
pub(crate) const GROUPED_SPAN_MIN_VERTICAL_OVERLAP_RATIO: f32 = 0.5_f32;
const REDACTION_COLOR: Rgba<u8> = Rgba([220, 38, 38, 255]);
const CURRENT_LEFT_COLOR: Rgba<u8> = Rgba([37, 99, 235, 255]);
const CURRENT_RIGHT_COLOR: Rgba<u8> = Rgba([5, 150, 105, 255]);
const NEAREST_LEFT_COLOR: Rgba<u8> = Rgba([249, 115, 22, 255]);
const NEAREST_RIGHT_COLOR: Rgba<u8> = Rgba([234, 179, 8, 255]);
const GROUPED_LEFT_COLOR: Rgba<u8> = Rgba([168, 85, 247, 255]);
const GROUPED_RIGHT_COLOR: Rgba<u8> = Rgba([14, 165, 233, 255]);
const RECT_STROKE_WIDTH_PX: u32 = 2;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CollectVisualAnchorMetricsDependencyRequest<'a> {
    pub pdf_bytes: &'a [u8],
    pub rows: &'a [DependencyVisualAnchorRowRequest],
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DependencyVisualAnchorRowRequest {
    pub row_id: String,
    pub page_index: u32,
    pub redaction_bbox: Rect,
    pub current_left_bbox: Option<Rect>,
    pub current_right_bbox: Option<Rect>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CollectVisualAnchorMetricsDependencyOutput {
    pub pages: Vec<DependencyVisualPageSummary>,
    pub rows: Vec<DependencyVisualAnchorRowOutput>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DependencyVisualPageSummary {
    pub page_index: u32,
    pub page_box: Rect,
    pub width_px: u32,
    pub height_px: u32,
    pub row_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DependencyVisualAnchorRowOutput {
    pub row_id: String,
    pub page_index: u32,
    pub search_window_bbox: Rect,
    pub current_left_dark_pixel_count: Option<u32>,
    pub current_right_dark_pixel_count: Option<u32>,
    pub redaction_dark_component: Option<DependencyDarkComponent>,
    pub nearest_left: Option<DependencyComponentSpan>,
    pub nearest_right: Option<DependencyComponentSpan>,
    pub grouped_left: Option<DependencyComponentSpan>,
    pub grouped_right: Option<DependencyComponentSpan>,
    pub crop_png: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DependencyDarkComponent {
    pub bbox: Rect,
    pub width_pt: f32,
    pub height_pt: f32,
    pub pixel_area: u32,
    pub dark_pixel_count: u32,
    pub fill_ratio: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DependencyComponentSpan {
    pub bbox: Rect,
    pub gap_pt: f32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub component_count: u32,
    pub pixel_area: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelRect {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl PixelRect {
    #[inline]
    fn width(self) -> u32 {
        self.x1.saturating_sub(self.x0)
    }

    #[inline]
    fn height(self) -> u32 {
        self.y1.saturating_sub(self.y0)
    }

    #[inline]
    fn area(self) -> u32 {
        self.width().saturating_mul(self.height())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelComponent {
    bbox: PixelRect,
    pixel_area: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CropOverlayRects {
    current_left: Option<PixelRect>,
    current_right: Option<PixelRect>,
    nearest_left: Option<PixelRect>,
    nearest_right: Option<PixelRect>,
    grouped_left: Option<PixelRect>,
    grouped_right: Option<PixelRect>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RenderedPageContext<'a> {
    page_box: Rect,
    width_px: u32,
    height_px: u32,
    pixels: &'a [u8],
}

#[derive(Debug, Clone)]
pub(crate) struct VisualAnchorMetricsAccessor;

impl VisualAnchorMetricsAccessor {
    #[inline]
    pub(crate) fn new() -> Self {
        Self
    }

    #[inline]
    pub(crate) fn collect(
        &self,
        req: CollectVisualAnchorMetricsDependencyRequest<'_>,
    ) -> Result<CollectVisualAnchorMetricsDependencyOutput, String> {
        let page_rows = rows_by_page(req.rows);
        if page_rows.is_empty() {
            return Ok(CollectVisualAnchorMetricsDependencyOutput {
                pages: Vec::new(),
                rows: Vec::new(),
            });
        }

        let document = Document::load_mem(req.pdf_bytes)
            .map_err(|error| format!("visual_anchor_metrics_document_load_failed:{error}"))?;
        let page_boxes = build_page_boxes(&document);
        let renderer = HayroRenderer::new_from_bytes(req.pdf_bytes)
            .map_err(|error| format!("visual_anchor_metrics_renderer_failed:{error}"))?;

        let mut page_summaries = Vec::<DependencyVisualPageSummary>::new();
        let mut row_outputs = Vec::<DependencyVisualAnchorRowOutput>::new();

        for (page_index, rows) in page_rows {
            let rendered = renderer
                .render_page_to_rgba(page_index as usize, f32::from(VISUAL_RENDER_DPI))
                .map_err(|error| {
                    format!("visual_anchor_metrics_page_render_failed:page={page_index}:{error}")
                })?;
            let page_box = page_boxes
                .get(&page_index)
                .copied()
                .unwrap_or(Rect::new(0.0_f32, 0.0_f32, 612.0_f32, 792.0_f32));
            let context = RenderedPageContext {
                page_box,
                width_px: rendered.width_px,
                height_px: rendered.height_px,
                pixels: rendered.pixels.as_slice(),
            };
            page_summaries.push(DependencyVisualPageSummary {
                page_index,
                page_box,
                width_px: rendered.width_px,
                height_px: rendered.height_px,
                row_count: rows.len() as u32,
            });
            for row in rows {
                row_outputs.push(analyze_row(context, row)?);
            }
        }

        page_summaries.sort_by(|left, right| left.page_index.cmp(&right.page_index));
        row_outputs.sort_by(|left, right| {
            left.page_index
                .cmp(&right.page_index)
                .then_with(|| left.row_id.cmp(&right.row_id))
        });

        Ok(CollectVisualAnchorMetricsDependencyOutput {
            pages: page_summaries,
            rows: row_outputs,
        })
    }
}

impl Default for VisualAnchorMetricsAccessor {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

fn rows_by_page(
    rows: &[DependencyVisualAnchorRowRequest],
) -> BTreeMap<u32, Vec<&DependencyVisualAnchorRowRequest>> {
    let mut grouped = BTreeMap::<u32, Vec<&DependencyVisualAnchorRowRequest>>::new();
    for row in rows {
        grouped.entry(row.page_index).or_default().push(row);
    }
    grouped
}

fn build_page_boxes(document: &Document) -> BTreeMap<u32, Rect> {
    let mut boxes = BTreeMap::<u32, Rect>::new();
    for (page_number, page_id) in document.get_pages() {
        let page_index = page_number.saturating_sub(1);
        let page_box = page_render_box_from_page(document, page_id)
            .unwrap_or(Rect::new(0.0_f32, 0.0_f32, 612.0_f32, 792.0_f32));
        boxes.insert(page_index, page_box);
    }
    boxes
}

fn analyze_row(
    context: RenderedPageContext<'_>,
    row: &DependencyVisualAnchorRowRequest,
) -> Result<DependencyVisualAnchorRowOutput, String> {
    let search_window_bbox = search_window_bbox(context.page_box, row.redaction_bbox);
    let search_window_px = rect_to_pixel(
        search_window_bbox,
        context.page_box,
        context.width_px,
        context.height_px,
    );
    let redaction_px = rect_to_pixel(
        row.redaction_bbox,
        context.page_box,
        context.width_px,
        context.height_px,
    );

    let relaxed_components = extract_components(
        context,
        search_window_px,
        RELAXED_LUMINANCE_MAX,
        Some(redaction_px),
    );
    let redaction_dark_component = dominant_redaction_component(context, redaction_px);
    let current_left_dark_pixel_count = row.current_left_bbox.map(|bbox| {
        count_dark_pixels(
            context,
            rect_to_pixel(bbox, context.page_box, context.width_px, context.height_px),
            RELAXED_LUMINANCE_MAX,
        )
    });
    let current_right_dark_pixel_count = row.current_right_bbox.map(|bbox| {
        count_dark_pixels(
            context,
            rect_to_pixel(bbox, context.page_box, context.width_px, context.height_px),
            RELAXED_LUMINANCE_MAX,
        )
    });
    let nearest_left =
        select_nearest_span(context, redaction_px, relaxed_components.as_slice(), true);
    let nearest_right =
        select_nearest_span(context, redaction_px, relaxed_components.as_slice(), false);
    let grouped_left = nearest_left.clone().and_then(|span| {
        select_grouped_span(
            context,
            redaction_px,
            relaxed_components.as_slice(),
            span,
            true,
        )
    });
    let grouped_right = nearest_right.clone().and_then(|span| {
        select_grouped_span(
            context,
            redaction_px,
            relaxed_components.as_slice(),
            span,
            false,
        )
    });
    let overlay_rects = CropOverlayRects {
        current_left: row
            .current_left_bbox
            .map(|bbox| rect_to_pixel(bbox, context.page_box, context.width_px, context.height_px)),
        current_right: row
            .current_right_bbox
            .map(|bbox| rect_to_pixel(bbox, context.page_box, context.width_px, context.height_px)),
        nearest_left: nearest_left.as_ref().map(|span| {
            rect_to_pixel(
                span.bbox,
                context.page_box,
                context.width_px,
                context.height_px,
            )
        }),
        nearest_right: nearest_right.as_ref().map(|span| {
            rect_to_pixel(
                span.bbox,
                context.page_box,
                context.width_px,
                context.height_px,
            )
        }),
        grouped_left: grouped_left.as_ref().map(|span| {
            rect_to_pixel(
                span.bbox,
                context.page_box,
                context.width_px,
                context.height_px,
            )
        }),
        grouped_right: grouped_right.as_ref().map(|span| {
            rect_to_pixel(
                span.bbox,
                context.page_box,
                context.width_px,
                context.height_px,
            )
        }),
    };
    let crop_png = build_crop_png(context, search_window_px, redaction_px, overlay_rects)?;

    Ok(DependencyVisualAnchorRowOutput {
        row_id: row.row_id.clone(),
        page_index: row.page_index,
        search_window_bbox,
        current_left_dark_pixel_count,
        current_right_dark_pixel_count,
        redaction_dark_component,
        nearest_left,
        nearest_right,
        grouped_left,
        grouped_right,
        crop_png,
    })
}

fn search_window_bbox(page_box: Rect, redaction_bbox: Rect) -> Rect {
    let vertical_padding_pt = SEARCH_VERTICAL_PADDING_MIN_PT
        .max(redaction_bbox.height().abs() * SEARCH_VERTICAL_PADDING_HEIGHT_RATIO);
    Rect::new(
        (redaction_bbox.x0 - SEARCH_HORIZONTAL_PADDING_PT).max(page_box.x0),
        (redaction_bbox.y0 - vertical_padding_pt).max(page_box.y0),
        (redaction_bbox.x1 + SEARCH_HORIZONTAL_PADDING_PT).min(page_box.x1),
        (redaction_bbox.y1 + vertical_padding_pt).min(page_box.y1),
    )
}

fn rect_to_pixel(rect: Rect, page_box: Rect, width_px: u32, height_px: u32) -> PixelRect {
    if width_px == 0 || height_px == 0 {
        return PixelRect {
            x0: 0,
            y0: 0,
            x1: 0,
            y1: 0,
        };
    }
    let page_width = page_box.width().abs().max(f32::EPSILON);
    let page_height = page_box.height().abs().max(f32::EPSILON);
    let x0 = (((rect.x0 - page_box.x0) / page_width) * width_px as f32)
        .floor()
        .clamp(0.0_f32, width_px as f32) as u32;
    let x1 = (((rect.x1 - page_box.x0) / page_width) * width_px as f32)
        .ceil()
        .clamp(0.0_f32, width_px as f32) as u32;
    let y0 = (((page_box.y1 - rect.y1) / page_height) * height_px as f32)
        .floor()
        .clamp(0.0_f32, height_px as f32) as u32;
    let y1 = (((page_box.y1 - rect.y0) / page_height) * height_px as f32)
        .ceil()
        .clamp(0.0_f32, height_px as f32) as u32;
    PixelRect { x0, y0, x1, y1 }
}

fn pixel_to_rect(rect: PixelRect, page_box: Rect, width_px: u32, height_px: u32) -> Rect {
    if width_px == 0 || height_px == 0 {
        return Rect::new(page_box.x0, page_box.y0, page_box.x0, page_box.y0);
    }
    let page_width = page_box.width().abs();
    let page_height = page_box.height().abs();
    let x0 = page_box.x0 + (rect.x0 as f32 / width_px as f32) * page_width;
    let x1 = page_box.x0 + (rect.x1 as f32 / width_px as f32) * page_width;
    let y1 = page_box.y1 - (rect.y0 as f32 / height_px as f32) * page_height;
    let y0 = page_box.y1 - (rect.y1 as f32 / height_px as f32) * page_height;
    Rect::new(x0, y0, x1, y1)
}

fn width_px_to_pt(width_px: u32, page_box: Rect, rendered_width_px: u32) -> f32 {
    if rendered_width_px == 0 {
        return 0.0_f32;
    }
    page_box.width().abs() * (width_px as f32 / rendered_width_px as f32)
}

fn height_px_to_pt(height_px: u32, page_box: Rect, rendered_height_px: u32) -> f32 {
    if rendered_height_px == 0 {
        return 0.0_f32;
    }
    page_box.height().abs() * (height_px as f32 / rendered_height_px as f32)
}

fn extract_components(
    context: RenderedPageContext<'_>,
    search_window_px: PixelRect,
    threshold: u8,
    excluded_rect: Option<PixelRect>,
) -> Vec<PixelComponent> {
    let width = search_window_px.width();
    let height = search_window_px.height();
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let local_width = width as usize;
    let local_height = height as usize;
    let mut visited = vec![false; local_width.saturating_mul(local_height)];
    let mut queue = VecDeque::<(u32, u32)>::new();
    let mut components = Vec::<PixelComponent>::new();

    for local_y in 0..height {
        for local_x in 0..width {
            let index = (local_y as usize)
                .saturating_mul(local_width)
                .saturating_add(local_x as usize);
            if visited[index] {
                continue;
            }
            let x = search_window_px.x0.saturating_add(local_x);
            let y = search_window_px.y0.saturating_add(local_y);
            visited[index] = true;
            if is_excluded_pixel(excluded_rect, x, y) || !is_dark_pixel(context, x, y, threshold) {
                continue;
            }
            queue.clear();
            queue.push_back((x, y));
            let mut min_x = x;
            let mut min_y = y;
            let mut max_x = x;
            let mut max_y = y;
            let mut pixel_area = 0_u32;

            while let Some((current_x, current_y)) = queue.pop_front() {
                pixel_area = pixel_area.saturating_add(1);
                min_x = min_x.min(current_x);
                min_y = min_y.min(current_y);
                max_x = max_x.max(current_x);
                max_y = max_y.max(current_y);
                for (next_x, next_y) in neighbors(current_x, current_y) {
                    if next_x < search_window_px.x0
                        || next_x >= search_window_px.x1
                        || next_y < search_window_px.y0
                        || next_y >= search_window_px.y1
                    {
                        continue;
                    }
                    let next_local_x = next_x.saturating_sub(search_window_px.x0);
                    let next_local_y = next_y.saturating_sub(search_window_px.y0);
                    let next_index = (next_local_y as usize)
                        .saturating_mul(local_width)
                        .saturating_add(next_local_x as usize);
                    if visited[next_index] {
                        continue;
                    }
                    visited[next_index] = true;
                    if is_excluded_pixel(excluded_rect, next_x, next_y)
                        || !is_dark_pixel(context, next_x, next_y, threshold)
                    {
                        continue;
                    }
                    queue.push_back((next_x, next_y));
                }
            }

            if pixel_area < MIN_COMPONENT_AREA_PX {
                continue;
            }
            components.push(PixelComponent {
                bbox: PixelRect {
                    x0: min_x,
                    y0: min_y,
                    x1: max_x.saturating_add(1),
                    y1: max_y.saturating_add(1),
                },
                pixel_area,
            });
        }
    }

    components.sort_by(|left, right| {
        left.bbox
            .x0
            .cmp(&right.bbox.x0)
            .then_with(|| left.bbox.y0.cmp(&right.bbox.y0))
            .then_with(|| left.bbox.x1.cmp(&right.bbox.x1))
            .then_with(|| left.bbox.y1.cmp(&right.bbox.y1))
    });
    components
}

fn dominant_redaction_component(
    context: RenderedPageContext<'_>,
    redaction_px: PixelRect,
) -> Option<DependencyDarkComponent> {
    let components = extract_components(context, redaction_px, STRICT_LUMINANCE_MAX, None);
    let component = components.into_iter().max_by(|left, right| {
        left.pixel_area
            .cmp(&right.pixel_area)
            .then_with(|| left.bbox.x0.cmp(&right.bbox.x0))
            .then_with(|| left.bbox.y0.cmp(&right.bbox.y0))
    })?;
    let bbox = pixel_to_rect(
        component.bbox,
        context.page_box,
        context.width_px,
        context.height_px,
    );
    let redaction_area = redaction_px.area().max(1);
    Some(DependencyDarkComponent {
        bbox,
        width_pt: width_px_to_pt(component.bbox.width(), context.page_box, context.width_px),
        height_pt: height_px_to_pt(component.bbox.height(), context.page_box, context.height_px),
        pixel_area: component.pixel_area,
        dark_pixel_count: component.pixel_area,
        fill_ratio: component.pixel_area as f32 / redaction_area as f32,
    })
}

fn count_dark_pixels(context: RenderedPageContext<'_>, rect: PixelRect, threshold: u8) -> u32 {
    let mut count = 0_u32;
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            if is_dark_pixel(context, x, y, threshold) {
                count = count.saturating_add(1);
            }
        }
    }
    count
}

fn select_nearest_span(
    context: RenderedPageContext<'_>,
    redaction_px: PixelRect,
    components: &[PixelComponent],
    left_side: bool,
) -> Option<DependencyComponentSpan> {
    let mut candidates = components
        .iter()
        .copied()
        .filter(|component| component_on_side(component.bbox, redaction_px, left_side))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        candidate_gap_px(left.bbox, redaction_px, left_side)
            .cmp(&candidate_gap_px(right.bbox, redaction_px, left_side))
            .then_with(|| {
                vertical_overlap_px(left.bbox, redaction_px)
                    .cmp(&vertical_overlap_px(right.bbox, redaction_px))
                    .reverse()
            })
            .then_with(|| left.pixel_area.cmp(&right.pixel_area).reverse())
            .then_with(|| left.bbox.x0.cmp(&right.bbox.x0))
            .then_with(|| left.bbox.y0.cmp(&right.bbox.y0))
    });
    let component = candidates.first().copied()?;
    Some(component_to_span(
        context,
        redaction_px,
        component,
        1,
        left_side,
    ))
}

fn select_grouped_span(
    context: RenderedPageContext<'_>,
    redaction_px: PixelRect,
    components: &[PixelComponent],
    nearest: DependencyComponentSpan,
    left_side: bool,
) -> Option<DependencyComponentSpan> {
    let nearest_bbox = rect_to_pixel(
        nearest.bbox,
        context.page_box,
        context.width_px,
        context.height_px,
    );
    let mut candidates = components
        .iter()
        .copied()
        .filter(|component| component_on_side(component.bbox, redaction_px, left_side))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        if left_side {
            right
                .bbox
                .x1
                .cmp(&left.bbox.x1)
                .then_with(|| left.bbox.y0.cmp(&right.bbox.y0))
        } else {
            left.bbox
                .x0
                .cmp(&right.bbox.x0)
                .then_with(|| left.bbox.y0.cmp(&right.bbox.y0))
        }
    });
    let start_index = candidates
        .iter()
        .position(|component| component.bbox == nearest_bbox)?;
    let mut grouped_bbox = nearest_bbox;
    let mut pixel_area = 0_u32;
    let mut component_count = 0_u32;

    for component in &candidates[start_index..] {
        if component_count == 0 {
            grouped_bbox = component.bbox;
            pixel_area = component.pixel_area;
            component_count = 1;
            continue;
        }
        let gap_px = grouped_gap_px(grouped_bbox, component.bbox, left_side);
        let overlap_ratio = vertical_overlap_ratio(grouped_bbox, component.bbox);
        if gap_px > GROUPED_SPAN_MAX_GAP_PX
            || overlap_ratio < GROUPED_SPAN_MIN_VERTICAL_OVERLAP_RATIO
        {
            break;
        }
        grouped_bbox = PixelRect {
            x0: grouped_bbox.x0.min(component.bbox.x0),
            y0: grouped_bbox.y0.min(component.bbox.y0),
            x1: grouped_bbox.x1.max(component.bbox.x1),
            y1: grouped_bbox.y1.max(component.bbox.y1),
        };
        pixel_area = pixel_area.saturating_add(component.pixel_area);
        component_count = component_count.saturating_add(1);
    }

    Some(span_from_parts(
        context,
        redaction_px,
        grouped_bbox,
        pixel_area,
        component_count,
        left_side,
    ))
}

fn component_on_side(component: PixelRect, redaction_px: PixelRect, left_side: bool) -> bool {
    if left_side {
        component.x1 <= redaction_px.x0
    } else {
        component.x0 >= redaction_px.x1
    }
}

fn component_to_span(
    context: RenderedPageContext<'_>,
    redaction_px: PixelRect,
    component: PixelComponent,
    component_count: u32,
    left_side: bool,
) -> DependencyComponentSpan {
    span_from_parts(
        context,
        redaction_px,
        component.bbox,
        component.pixel_area,
        component_count,
        left_side,
    )
}

fn span_from_parts(
    context: RenderedPageContext<'_>,
    redaction_px: PixelRect,
    bbox: PixelRect,
    pixel_area: u32,
    component_count: u32,
    left_side: bool,
) -> DependencyComponentSpan {
    DependencyComponentSpan {
        bbox: pixel_to_rect(bbox, context.page_box, context.width_px, context.height_px),
        gap_pt: width_px_to_pt(
            candidate_gap_px(bbox, redaction_px, left_side),
            context.page_box,
            context.width_px,
        ),
        width_pt: width_px_to_pt(bbox.width(), context.page_box, context.width_px),
        height_pt: height_px_to_pt(bbox.height(), context.page_box, context.height_px),
        component_count,
        pixel_area,
    }
}

fn candidate_gap_px(component: PixelRect, redaction_px: PixelRect, left_side: bool) -> u32 {
    if left_side {
        redaction_px.x0.saturating_sub(component.x1)
    } else {
        component.x0.saturating_sub(redaction_px.x1)
    }
}

fn grouped_gap_px(group_bbox: PixelRect, component_bbox: PixelRect, left_side: bool) -> u32 {
    if left_side {
        group_bbox.x0.saturating_sub(component_bbox.x1)
    } else {
        component_bbox.x0.saturating_sub(group_bbox.x1)
    }
}

fn vertical_overlap_px(left: PixelRect, right: PixelRect) -> u32 {
    left.y1.min(right.y1).saturating_sub(left.y0.max(right.y0))
}

fn vertical_overlap_ratio(left: PixelRect, right: PixelRect) -> f32 {
    let overlap = vertical_overlap_px(left, right);
    let min_height = left.height().min(right.height()).max(1);
    overlap as f32 / min_height as f32
}

fn neighbors(x: u32, y: u32) -> [(u32, u32); 4] {
    [
        (x.saturating_sub(1), y),
        (x.saturating_add(1), y),
        (x, y.saturating_sub(1)),
        (x, y.saturating_add(1)),
    ]
}

fn is_excluded_pixel(excluded_rect: Option<PixelRect>, x: u32, y: u32) -> bool {
    if let Some(rect) = excluded_rect {
        return x >= rect.x0 && x < rect.x1 && y >= rect.y0 && y < rect.y1;
    }
    false
}

fn is_dark_pixel(context: RenderedPageContext<'_>, x: u32, y: u32, threshold: u8) -> bool {
    if x >= context.width_px || y >= context.height_px {
        return false;
    }
    let index = ((y as usize)
        .saturating_mul(context.width_px as usize)
        .saturating_add(x as usize))
    .saturating_mul(4);
    let alpha = context.pixels.get(index + 3).copied().unwrap_or(0);
    if alpha == 0 {
        return false;
    }
    let red = context.pixels.get(index).copied().unwrap_or(u8::MAX);
    let green = context.pixels.get(index + 1).copied().unwrap_or(u8::MAX);
    let blue = context.pixels.get(index + 2).copied().unwrap_or(u8::MAX);
    luminance(red, green, blue) <= threshold as f32
}

fn luminance(red: u8, green: u8, blue: u8) -> f32 {
    (red as f32 * 0.2126_f32) + (green as f32 * 0.7152_f32) + (blue as f32 * 0.0722_f32)
}

fn build_crop_png(
    context: RenderedPageContext<'_>,
    crop_rect: PixelRect,
    redaction_rect: PixelRect,
    overlay_rects: CropOverlayRects,
) -> Result<Vec<u8>, String> {
    let crop_width = crop_rect.width();
    let crop_height = crop_rect.height();
    let mut bytes = Vec::<u8>::with_capacity(crop_width as usize * crop_height as usize * 4);
    for y in crop_rect.y0..crop_rect.y1 {
        let start = ((y as usize)
            .saturating_mul(context.width_px as usize)
            .saturating_add(crop_rect.x0 as usize))
        .saturating_mul(4);
        let end = start.saturating_add(crop_width as usize * 4);
        bytes.extend_from_slice(
            context
                .pixels
                .get(start..end)
                .ok_or_else(|| "visual_anchor_metrics_crop_slice_out_of_bounds".to_owned())?,
        );
    }
    let mut image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(crop_width, crop_height, bytes)
        .ok_or_else(|| "visual_anchor_metrics_invalid_crop_buffer".to_owned())?;
    draw_rect_outline(
        &mut image,
        relative_rect(redaction_rect, crop_rect),
        REDACTION_COLOR,
    );
    if let Some(rect) = overlay_rects.current_left {
        draw_rect_outline(
            &mut image,
            relative_rect(rect, crop_rect),
            CURRENT_LEFT_COLOR,
        );
    }
    if let Some(rect) = overlay_rects.current_right {
        draw_rect_outline(
            &mut image,
            relative_rect(rect, crop_rect),
            CURRENT_RIGHT_COLOR,
        );
    }
    if let Some(rect) = overlay_rects.nearest_left {
        draw_rect_outline(
            &mut image,
            relative_rect(rect, crop_rect),
            NEAREST_LEFT_COLOR,
        );
    }
    if let Some(rect) = overlay_rects.nearest_right {
        draw_rect_outline(
            &mut image,
            relative_rect(rect, crop_rect),
            NEAREST_RIGHT_COLOR,
        );
    }
    if let Some(rect) = overlay_rects.grouped_left {
        draw_rect_outline(
            &mut image,
            relative_rect(rect, crop_rect),
            GROUPED_LEFT_COLOR,
        );
    }
    if let Some(rect) = overlay_rects.grouped_right {
        draw_rect_outline(
            &mut image,
            relative_rect(rect, crop_rect),
            GROUPED_RIGHT_COLOR,
        );
    }
    let mut out = Vec::<u8>::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut out);
    image::ImageEncoder::write_image(
        encoder,
        image.as_raw(),
        image.width(),
        image.height(),
        image::ColorType::Rgba8.into(),
    )
    .map_err(|error| format!("visual_anchor_metrics_crop_encode_failed:{error}"))?;
    Ok(out)
}

fn relative_rect(rect: PixelRect, crop_rect: PixelRect) -> PixelRect {
    PixelRect {
        x0: rect.x0.saturating_sub(crop_rect.x0),
        y0: rect.y0.saturating_sub(crop_rect.y0),
        x1: rect.x1.saturating_sub(crop_rect.x0),
        y1: rect.y1.saturating_sub(crop_rect.y0),
    }
}

fn draw_rect_outline(image: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, rect: PixelRect, color: Rgba<u8>) {
    if rect.x0 >= rect.x1 || rect.y0 >= rect.y1 {
        return;
    }
    let max_x = image.width().saturating_sub(1);
    let max_y = image.height().saturating_sub(1);
    let x0 = rect.x0.min(max_x);
    let x1 = rect.x1.saturating_sub(1).min(max_x);
    let y0 = rect.y0.min(max_y);
    let y1 = rect.y1.saturating_sub(1).min(max_y);
    for stroke in 0..RECT_STROKE_WIDTH_PX {
        let top = y0.saturating_add(stroke).min(max_y);
        let bottom = y1.saturating_sub(stroke);
        for x in x0..=x1 {
            image.put_pixel(x, top, color);
            image.put_pixel(x, bottom, color);
        }
        let left = x0.saturating_add(stroke).min(max_x);
        let right = x1.saturating_sub(stroke);
        for y in y0..=y1 {
            image.put_pixel(left, y, color);
            image.put_pixel(right, y, color);
        }
    }
}
