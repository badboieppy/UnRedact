use std::collections::{BTreeMap, BTreeSet};

use crate::types::redaction_grouping_types::{
    CompoundRedactionSegment, FlowItem, FlowLocalAnchor, FlowLocalAnchorRole, FlowRedactionSlot,
    FlowTrustPolicy, FlowTrustSummary, FlowVisibleSpan, RedactionFlowCollection,
    RedactionFlowGroup, RedactionFlowMembership, RedactionFlowSegment, RedactionSegmentKind,
    SegmentNeighborFacts, SingleSlotRedactionSegment,
};
use crate::types::redaction_types::{Rect, RedactionKind};

const FLOW_GROUP_MAX_GAP_PT: f32 = 96.0_f32;
const FLOW_GROUP_WRAP_LINE_MAX_Y_GAP_PT: f32 = 18.0_f32;
const FLOW_GROUP_SAME_LINE_MAX_Y_DELTA_PT: f32 = 4.0_f32;
const FLOW_GROUP_WRAP_RESET_X_DELTA_PT: f32 = 16.0_f32;
const FLOW_GROUP_WRAP_FORWARD_X_ALLOWANCE_PT: f32 = 28.0_f32;

#[derive(Debug, Clone)]
pub(crate) struct PreparedFlowContext {
    pub index: usize,
    pub page_index: u32,
    pub bbox: Rect,
    pub kind: RedactionKind,
    pub left_context: String,
    pub right_context: String,
    pub context_spans: Vec<FlowVisibleSpan>,
}

pub(crate) fn collect_redaction_flow_memberships(
    prepared: &[PreparedFlowContext],
) -> (
    RedactionFlowCollection,
    BTreeMap<usize, RedactionFlowMembership>,
) {
    let collection = build_redaction_flow_collection(prepared);
    let mut memberships = BTreeMap::<usize, RedactionFlowMembership>::new();

    for group in &collection.groups {
        let group_context_spans = group
            .items
            .iter()
            .filter_map(|item| match item {
                FlowItem::VisibleSpan(span) => Some(span.clone()),
                FlowItem::RedactionSlot(_) => None,
            })
            .collect::<Vec<_>>();
        let group_redaction_count = group
            .items
            .iter()
            .filter(|item| matches!(item, FlowItem::RedactionSlot(_)))
            .count();
        let mut segment_lookup = BTreeMap::<String, (String, RedactionSegmentKind, usize)>::new();
        for segment in &group.segments {
            let segment_kind = segment.kind();
            let segment_id = segment.segment_id().to_owned();
            let segment_redaction_count = segment.slot_item_ids().len();
            for slot_item_id in segment.slot_item_ids() {
                segment_lookup.insert(
                    slot_item_id.to_owned(),
                    (segment_id.clone(), segment_kind, segment_redaction_count),
                );
            }
        }
        for item in &group.items {
            let FlowItem::RedactionSlot(slot) = item else {
                continue;
            };
            let Some((segment_id, segment_kind, segment_redaction_count)) =
                segment_lookup.get(slot.item_id.as_str())
            else {
                continue;
            };
            memberships.insert(
                slot.redaction_index,
                RedactionFlowMembership {
                    group_id: group.group_id.clone(),
                    segment_id: segment_id.clone(),
                    segment_kind: *segment_kind,
                    group_reading_order: group.reading_order,
                    redaction_reading_order: slot.reading_order,
                    group_redaction_count,
                    segment_redaction_count: *segment_redaction_count,
                    context_spans: group_context_spans.clone(),
                    trusted: group.trust.is_trusted,
                },
            );
        }
    }

    (collection, memberships)
}

fn build_redaction_flow_collection(prepared: &[PreparedFlowContext]) -> RedactionFlowCollection {
    if prepared.is_empty() {
        return RedactionFlowCollection::default();
    }

    let mut sorted = prepared.iter().collect::<Vec<_>>();
    sorted.sort_by(compare_prepared_redactions);
    let mut groups = Vec::<RedactionFlowGroup>::new();
    let mut visited = BTreeSet::<usize>::new();
    let mut dropped_group_count = 0_usize;
    let mut dropped_redaction_count = 0_usize;

    for item in &sorted {
        if visited.contains(&item.index) {
            continue;
        }
        let mut current = Vec::<&PreparedFlowContext>::new();
        let mut queue = vec![*item];
        visited.insert(item.index);
        while let Some(current_item) = queue.pop() {
            current.push(current_item);
            for candidate in &sorted {
                if visited.contains(&candidate.index) {
                    continue;
                }
                if rows_are_group_linked(current_item, candidate) {
                    visited.insert(candidate.index);
                    queue.push(candidate);
                }
            }
        }
        current.sort_by(compare_prepared_redactions);
        if let Some(group) = finalize_group(groups.len(), &current) {
            groups.push(group);
        } else {
            dropped_group_count += 1;
            dropped_redaction_count += current.len();
        }
    }

    let dropped_visible_span_count = prepared
        .iter()
        .map(|item| item.context_spans.len())
        .sum::<usize>()
        .saturating_sub(
            groups
                .iter()
                .map(|group| {
                    group
                        .items
                        .iter()
                        .filter(|item| matches!(item, FlowItem::VisibleSpan(_)))
                        .count()
                })
                .sum::<usize>(),
        );

    RedactionFlowCollection {
        groups,
        dropped_group_count,
        dropped_redaction_count,
        dropped_visible_span_count,
        diagnostics: Vec::new(),
        ..RedactionFlowCollection::default()
    }
}

fn finalize_group(
    group_reading_order: usize,
    prepared: &[&PreparedFlowContext],
) -> Option<RedactionFlowGroup> {
    if prepared.is_empty() {
        return None;
    }
    let page_index = prepared.first()?.page_index;
    let group_id = format!("page{page_index}_group{group_reading_order:03}");

    let has_context_signal = prepared.iter().any(|item| {
        !item.left_context.trim().is_empty()
            || !item.right_context.trim().is_empty()
            || !item.context_spans.is_empty()
    });
    if !has_context_signal {
        return None;
    }

    let visible_spans = collect_group_visible_spans(&group_id, prepared);
    let slots = collect_group_slots(&group_id, prepared);
    let items = collect_group_items(&visible_spans, &slots);
    if items.is_empty() {
        return None;
    }
    let segments = build_segments(&group_id, &items);
    let bbox = union_item_bboxes(&items)?;
    let trust = FlowTrustSummary {
        is_trusted: true,
        trust_score: Some(1.0_f32),
        policy: Some(FlowTrustPolicy::Trusted),
        reasons: Vec::new(),
    };

    Some(RedactionFlowGroup {
        group_id,
        page_index,
        bbox,
        reading_order: group_reading_order,
        trust,
        items,
        segments,
        typography: None,
        diagnostics: Vec::new(),
    })
}

fn collect_group_visible_spans(
    group_id: &str,
    prepared: &[&PreparedFlowContext],
) -> Vec<FlowVisibleSpan> {
    let mut dedup = BTreeSet::<String>::new();
    let mut spans = Vec::<FlowVisibleSpan>::new();
    for item in prepared {
        for span in &item.context_spans {
            let text = span.text.trim();
            if text.is_empty() {
                continue;
            }
            let key = format!(
                "{}:{:.1}:{:.1}:{:.1}:{:.1}:{}",
                span.line_bucket, span.bbox.x0, span.bbox.y0, span.bbox.x1, span.bbox.y1, text
            );
            if !dedup.insert(key) {
                continue;
            }
            spans.push(span.clone());
        }
    }
    spans.sort_by(compare_visible_spans);
    for (index, span) in spans.iter_mut().enumerate() {
        span.item_id = format!("{group_id}_span_{index:03}");
        span.reading_order = index;
    }
    spans
}

fn collect_group_slots(
    group_id: &str,
    prepared: &[&PreparedFlowContext],
) -> Vec<FlowRedactionSlot> {
    let mut sorted = prepared.to_vec();
    sorted.sort_by(compare_prepared_redactions);
    sorted
        .into_iter()
        .enumerate()
        .map(|(index, item)| FlowRedactionSlot {
            item_id: format!("{group_id}_slot_{index:03}"),
            redaction_index: item.index,
            page_index: item.page_index,
            bbox: item.bbox,
            kind: item.kind.clone(),
            reading_order: index,
        })
        .collect()
}

fn collect_group_items(
    visible_spans: &[FlowVisibleSpan],
    slots: &[FlowRedactionSlot],
) -> Vec<FlowItem> {
    let mut items = visible_spans
        .iter()
        .cloned()
        .map(FlowItem::VisibleSpan)
        .chain(slots.iter().cloned().map(FlowItem::RedactionSlot))
        .collect::<Vec<_>>();
    items.sort_by(compare_flow_items);
    for (reading_order, item) in items.iter_mut().enumerate() {
        match item {
            FlowItem::VisibleSpan(span) => span.reading_order = reading_order,
            FlowItem::RedactionSlot(slot) => slot.reading_order = reading_order,
        }
    }
    items
}

fn build_segments(group_id: &str, items: &[FlowItem]) -> Vec<RedactionFlowSegment> {
    let mut segments = Vec::<RedactionFlowSegment>::new();
    let mut current_slot_ids = Vec::<String>::new();
    let mut segment_index = 0_usize;

    let flush_current = |segments: &mut Vec<RedactionFlowSegment>,
                         current_slot_ids: &mut Vec<String>,
                         segment_index: &mut usize| {
        if current_slot_ids.is_empty() {
            return;
        }
        let neighbor_facts = build_segment_neighbor_facts(items, current_slot_ids);
        let trust = FlowTrustSummary {
            is_trusted: true,
            trust_score: Some(1.0_f32),
            policy: Some(FlowTrustPolicy::Trusted),
            reasons: Vec::new(),
        };
        let segment_id = format!("{group_id}_segment_{:03}", *segment_index);
        if current_slot_ids.len() == 1 {
            segments.push(RedactionFlowSegment::SingleSlot(
                SingleSlotRedactionSegment {
                    segment_id,
                    group_id: group_id.to_owned(),
                    slot_item_id: current_slot_ids[0].clone(),
                    neighbor_facts,
                    trust,
                    diagnostics: Vec::new(),
                },
            ));
        } else {
            segments.push(RedactionFlowSegment::Compound(CompoundRedactionSegment {
                segment_id,
                group_id: group_id.to_owned(),
                slot_item_ids: current_slot_ids.clone(),
                neighbor_facts,
                trust,
                diagnostics: Vec::new(),
            }));
        }
        current_slot_ids.clear();
        *segment_index += 1;
    };

    for item in items {
        match item {
            FlowItem::VisibleSpan(_) => {
                flush_current(&mut segments, &mut current_slot_ids, &mut segment_index);
            }
            FlowItem::RedactionSlot(slot) => current_slot_ids.push(slot.item_id.clone()),
        }
    }
    flush_current(&mut segments, &mut current_slot_ids, &mut segment_index);
    segments
}

fn build_segment_neighbor_facts(
    items: &[FlowItem],
    slot_item_ids: &[String],
) -> SegmentNeighborFacts {
    let mut slot_positions = slot_item_ids
        .iter()
        .filter_map(|slot_item_id| {
            items.iter().position(
                |item| matches!(item, FlowItem::RedactionSlot(slot) if slot.item_id == *slot_item_id),
            )
        })
        .collect::<Vec<_>>();
    slot_positions.sort_unstable();
    let Some(first_slot_position) = slot_positions.first().copied() else {
        return SegmentNeighborFacts::default();
    };
    let Some(last_slot_position) = slot_positions.last().copied() else {
        return SegmentNeighborFacts::default();
    };

    let left_outer_anchor = items[..first_slot_position]
        .iter()
        .rev()
        .find_map(|item| visible_item_to_anchor(item, FlowLocalAnchorRole::LeftOuter));
    let right_outer_anchor = items[(last_slot_position + 1)..]
        .iter()
        .find_map(|item| visible_item_to_anchor(item, FlowLocalAnchorRole::RightOuter));
    let internal_separators = if last_slot_position > first_slot_position {
        items[(first_slot_position + 1)..last_slot_position]
            .iter()
            .filter_map(|item| visible_item_to_anchor(item, FlowLocalAnchorRole::InternalSeparator))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    SegmentNeighborFacts {
        left_outer_anchor,
        right_outer_anchor,
        internal_separators,
    }
}

fn visible_item_to_anchor(item: &FlowItem, role: FlowLocalAnchorRole) -> Option<FlowLocalAnchor> {
    let FlowItem::VisibleSpan(span) = item else {
        return None;
    };
    Some(FlowLocalAnchor {
        item_id: span.item_id.clone(),
        text: span.text.clone(),
        bbox: span.bbox,
        line_bucket: span.line_bucket,
        role,
    })
}

fn union_item_bboxes(items: &[FlowItem]) -> Option<Rect> {
    let mut iter = items.iter();
    let first = iter.next()?;
    let mut x0 = first.bbox().x0;
    let mut y0 = first.bbox().y0;
    let mut x1 = first.bbox().x1;
    let mut y1 = first.bbox().y1;
    for item in iter {
        let bbox = item.bbox();
        x0 = x0.min(bbox.x0);
        y0 = y0.min(bbox.y0);
        x1 = x1.max(bbox.x1);
        y1 = y1.max(bbox.y1);
    }
    Some(Rect::new(x0, y0, x1, y1))
}

fn compare_prepared_redactions(
    left: &&PreparedFlowContext,
    right: &&PreparedFlowContext,
) -> std::cmp::Ordering {
    let left_center_y = (left.bbox.y0 + left.bbox.y1) * 0.5_f32;
    let right_center_y = (right.bbox.y0 + right.bbox.y1) * 0.5_f32;
    left.page_index
        .cmp(&right.page_index)
        .then_with(|| {
            left_center_y
                .partial_cmp(&right_center_y)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| {
            left.bbox
                .x0
                .partial_cmp(&right.bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| left.index.cmp(&right.index))
}

fn compare_visible_spans(left: &FlowVisibleSpan, right: &FlowVisibleSpan) -> std::cmp::Ordering {
    let left_center_y = (left.bbox.y0 + left.bbox.y1) * 0.5_f32;
    let right_center_y = (right.bbox.y0 + right.bbox.y1) * 0.5_f32;
    left_center_y
        .partial_cmp(&right_center_y)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| {
            left.bbox
                .x0
                .partial_cmp(&right.bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| left.text.cmp(&right.text))
}

fn compare_flow_items(left: &FlowItem, right: &FlowItem) -> std::cmp::Ordering {
    let left_bbox = left.bbox();
    let right_bbox = right.bbox();
    let left_center_y = (left_bbox.y0 + left_bbox.y1) * 0.5_f32;
    let right_center_y = (right_bbox.y0 + right_bbox.y1) * 0.5_f32;
    left_center_y
        .partial_cmp(&right_center_y)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| {
            left_bbox
                .x0
                .partial_cmp(&right_bbox.x0)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| match (left, right) {
            (FlowItem::VisibleSpan(_), FlowItem::RedactionSlot(_)) => std::cmp::Ordering::Less,
            (FlowItem::RedactionSlot(_), FlowItem::VisibleSpan(_)) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        })
        .then_with(|| left.item_id().cmp(right.item_id()))
}

fn rows_are_group_contiguous(left: &PreparedFlowContext, right: &PreparedFlowContext) -> bool {
    if left.page_index != right.page_index {
        return false;
    }
    let left_center_y = (left.bbox.y0 + left.bbox.y1) * 0.5_f32;
    let right_center_y = (right.bbox.y0 + right.bbox.y1) * 0.5_f32;
    let y_delta = (right_center_y - left_center_y).abs();
    if y_delta <= FLOW_GROUP_SAME_LINE_MAX_Y_DELTA_PT {
        let x_gap = (right.bbox.x0 - left.bbox.x1).max(0.0_f32);
        return x_gap <= FLOW_GROUP_MAX_GAP_PT;
    }
    if right_center_y <= left_center_y || y_delta > FLOW_GROUP_WRAP_LINE_MAX_Y_GAP_PT {
        return false;
    }
    let wraps_to_new_line = right.bbox.x0 + FLOW_GROUP_WRAP_RESET_X_DELTA_PT < left.bbox.x0;
    let continues_forward = right.bbox.x0 <= left.bbox.x1 + FLOW_GROUP_WRAP_FORWARD_X_ALLOWANCE_PT;
    wraps_to_new_line || continues_forward
}

fn rows_are_group_linked(left: &PreparedFlowContext, right: &PreparedFlowContext) -> bool {
    rows_are_group_contiguous(left, right) || rows_are_group_contiguous(right, left)
}

#[cfg(test)]
mod tests {
    use super::collect_redaction_flow_memberships;
    use super::PreparedFlowContext;
    use crate::types::redaction_grouping_types::{FlowVisibleSpan, RedactionSegmentKind};
    use crate::types::redaction_types::{Rect, RedactionKind};

    #[test]
    fn groups_consecutive_slots_into_compound_segments() {
        let prepared = vec![
            PreparedFlowContext {
                index: 0,
                page_index: 1,
                bbox: Rect::new(100.0, 500.0, 120.0, 510.0),
                kind: RedactionKind::DrawnRect,
                left_context: "Jean".to_owned(),
                right_context: String::new(),
                context_spans: vec![FlowVisibleSpan {
                    item_id: String::new(),
                    text: "Jean".to_owned(),
                    bbox: Rect::new(70.0, 500.0, 95.0, 510.0),
                    reading_order: 0,
                    line_bucket: 250,
                    role_hint: "left".to_owned(),
                    source: "test".to_owned(),
                    font_key: None,
                    font_name: None,
                    font_size_pt: None,
                    h_scale_pct: None,
                }],
            },
            PreparedFlowContext {
                index: 1,
                page_index: 1,
                bbox: Rect::new(124.0, 500.0, 144.0, 510.0),
                kind: RedactionKind::DrawnRect,
                left_context: String::new(),
                right_context: "Brunel".to_owned(),
                context_spans: vec![FlowVisibleSpan {
                    item_id: String::new(),
                    text: "Brunel".to_owned(),
                    bbox: Rect::new(150.0, 500.0, 186.0, 510.0),
                    reading_order: 0,
                    line_bucket: 250,
                    role_hint: "right".to_owned(),
                    source: "test".to_owned(),
                    font_key: None,
                    font_name: None,
                    font_size_pt: None,
                    h_scale_pct: None,
                }],
            },
        ];

        let (collection, memberships) = collect_redaction_flow_memberships(&prepared);
        assert_eq!(collection.groups.len(), 1);
        assert_eq!(collection.groups[0].segments.len(), 1);
        assert_eq!(
            memberships.get(&0).map(|value| value.segment_kind),
            Some(RedactionSegmentKind::Compound)
        );
        assert_eq!(
            memberships.get(&1).map(|value| value.segment_kind),
            Some(RedactionSegmentKind::Compound)
        );
        assert_eq!(
            memberships
                .get(&0)
                .map(|value| value.context_spans.len())
                .unwrap_or_default(),
            2
        );
    }
}
