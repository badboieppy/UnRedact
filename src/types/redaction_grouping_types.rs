use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::types::redaction_types::{Rect, RedactionKind};

pub const REDACTION_FLOW_GROUP_CONTRACT_ID: &str = "redaction_flow_group_contract";
pub const COMPOUND_REDACTION_SEGMENT_CONTRACT_ID: &str = "compound_redaction_segment_contract";
pub const TRUSTED_GROUP_DROP_POLICY_CONTRACT_ID: &str = "trusted_group_drop_policy";
pub const NO_CROSS_GROUP_ANCHOR_BORROW_CONTRACT_ID: &str = "no_cross_group_anchor_borrow_contract";
pub const REDACTION_FLOW_GROUP_SCHEMA_VERSION: usize = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionFlowCollectorOutput {
    pub grouped_flow: RedactionFlowCollection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupedRedactionGuessingInput {
    pub grouped_flow: RedactionFlowCollection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionFlowCollection {
    pub contract_id: String,
    pub schema_version: usize,
    pub groups: Vec<RedactionFlowGroup>,
    pub dropped_group_count: usize,
    pub dropped_redaction_count: usize,
    pub dropped_visible_span_count: usize,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

impl Default for RedactionFlowCollection {
    #[inline]
    fn default() -> Self {
        Self {
            contract_id: REDACTION_FLOW_GROUP_CONTRACT_ID.to_owned(),
            schema_version: REDACTION_FLOW_GROUP_SCHEMA_VERSION,
            groups: Vec::new(),
            dropped_group_count: 0,
            dropped_redaction_count: 0,
            dropped_visible_span_count: 0,
            diagnostics: Vec::new(),
        }
    }
}

impl RedactionFlowCollection {
    #[inline]
    pub fn new(groups: Vec<RedactionFlowGroup>) -> Self {
        Self {
            groups,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionFlowGroup {
    pub group_id: String,
    pub page_index: u32,
    pub bbox: Rect,
    pub reading_order: usize,
    pub trust: FlowTrustSummary,
    pub items: Vec<FlowItem>,
    pub segments: Vec<RedactionFlowSegment>,
    #[serde(default)]
    pub typography: Option<FlowTypographyProfile>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowTypographyProfile {
    #[serde(default)]
    pub font_key: Option<String>,
    #[serde(default)]
    pub font_name: Option<String>,
    #[serde(default)]
    pub font_size_pt: Option<f32>,
    #[serde(default)]
    pub h_scale_pct: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowTrustSummary {
    pub is_trusted: bool,
    #[serde(default)]
    pub trust_score: Option<f32>,
    #[serde(default)]
    pub policy: Option<FlowTrustPolicy>,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowTrustPolicy {
    Trusted,
    DroppedWeakGroup,
    DroppedSparseAnchors,
    DroppedMixedTypography,
    DroppedBrokenReadingOrder,
    DroppedCompoundSeparatorConflict,
    DroppedOther,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FlowItem {
    VisibleSpan(FlowVisibleSpan),
    RedactionSlot(FlowRedactionSlot),
}

impl FlowItem {
    #[inline]
    pub fn item_id(&self) -> &str {
        match self {
            Self::VisibleSpan(span) => span.item_id.as_str(),
            Self::RedactionSlot(slot) => slot.item_id.as_str(),
        }
    }

    #[inline]
    pub fn bbox(&self) -> Rect {
        match self {
            Self::VisibleSpan(span) => span.bbox,
            Self::RedactionSlot(slot) => slot.bbox,
        }
    }

    #[inline]
    pub fn reading_order(&self) -> usize {
        match self {
            Self::VisibleSpan(span) => span.reading_order,
            Self::RedactionSlot(slot) => slot.reading_order,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowVisibleSpan {
    pub item_id: String,
    pub text: String,
    pub bbox: Rect,
    pub reading_order: usize,
    pub line_bucket: i32,
    #[serde(default)]
    pub role_hint: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub font_key: Option<String>,
    #[serde(default)]
    pub font_name: Option<String>,
    #[serde(default)]
    pub font_size_pt: Option<f32>,
    #[serde(default)]
    pub h_scale_pct: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowRedactionSlot {
    pub item_id: String,
    pub redaction_index: usize,
    pub page_index: u32,
    pub bbox: Rect,
    pub kind: RedactionKind,
    pub reading_order: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RedactionFlowSegment {
    SingleSlot(SingleSlotRedactionSegment),
    Compound(CompoundRedactionSegment),
}

impl RedactionFlowSegment {
    #[inline]
    pub fn segment_id(&self) -> &str {
        match self {
            Self::SingleSlot(segment) => segment.segment_id.as_str(),
            Self::Compound(segment) => segment.segment_id.as_str(),
        }
    }

    #[inline]
    pub fn group_id(&self) -> &str {
        match self {
            Self::SingleSlot(segment) => segment.group_id.as_str(),
            Self::Compound(segment) => segment.group_id.as_str(),
        }
    }

    #[inline]
    pub fn slot_item_ids(&self) -> Vec<&str> {
        match self {
            Self::SingleSlot(segment) => vec![segment.slot_item_id.as_str()],
            Self::Compound(segment) => segment
                .slot_item_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        }
    }

    #[inline]
    pub fn neighbor_facts(&self) -> &SegmentNeighborFacts {
        match self {
            Self::SingleSlot(segment) => &segment.neighbor_facts,
            Self::Compound(segment) => &segment.neighbor_facts,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SingleSlotRedactionSegment {
    pub segment_id: String,
    pub group_id: String,
    pub slot_item_id: String,
    pub neighbor_facts: SegmentNeighborFacts,
    pub trust: FlowTrustSummary,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompoundRedactionSegment {
    pub segment_id: String,
    pub group_id: String,
    pub slot_item_ids: Vec<String>,
    pub neighbor_facts: SegmentNeighborFacts,
    pub trust: FlowTrustSummary,
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionSegmentKind {
    SingleSlot,
    Compound,
}

impl RedactionFlowSegment {
    #[inline]
    pub fn kind(&self) -> RedactionSegmentKind {
        match self {
            Self::SingleSlot(_) => RedactionSegmentKind::SingleSlot,
            Self::Compound(_) => RedactionSegmentKind::Compound,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RedactionFlowMembership {
    pub group_id: String,
    pub segment_id: String,
    pub segment_kind: RedactionSegmentKind,
    pub group_reading_order: usize,
    pub redaction_reading_order: usize,
    pub group_redaction_count: usize,
    pub segment_redaction_count: usize,
    #[serde(default)]
    pub context_spans: Vec<FlowVisibleSpan>,
    #[serde(default)]
    pub trusted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SegmentNeighborFacts {
    #[serde(default)]
    pub left_outer_anchor: Option<FlowLocalAnchor>,
    #[serde(default)]
    pub right_outer_anchor: Option<FlowLocalAnchor>,
    #[serde(default)]
    pub internal_separators: Vec<FlowLocalAnchor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowLocalAnchor {
    pub item_id: String,
    pub text: String,
    pub bbox: Rect,
    pub line_bucket: i32,
    pub role: FlowLocalAnchorRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowLocalAnchorRole {
    LeftOuter,
    RightOuter,
    InternalSeparator,
}

#[inline]
pub fn validate_redaction_flow_collection(
    collection: &RedactionFlowCollection,
) -> Result<(), String> {
    if collection.contract_id != REDACTION_FLOW_GROUP_CONTRACT_ID {
        return Err(format!(
            "grouped flow contract_id must equal {REDACTION_FLOW_GROUP_CONTRACT_ID}"
        ));
    }
    if collection.schema_version != REDACTION_FLOW_GROUP_SCHEMA_VERSION {
        return Err(format!(
            "grouped flow schema_version must equal {REDACTION_FLOW_GROUP_SCHEMA_VERSION}"
        ));
    }

    let mut group_ids = BTreeSet::<String>::new();
    for group in &collection.groups {
        validate_group(group, &mut group_ids)?;
    }

    Ok(())
}

fn validate_group(
    group: &RedactionFlowGroup,
    group_ids: &mut BTreeSet<String>,
) -> Result<(), String> {
    if group.group_id.trim().is_empty() {
        return Err("grouped flow contains group with empty group_id".to_owned());
    }
    if !group_ids.insert(group.group_id.clone()) {
        return Err(format!(
            "grouped flow contains duplicate group_id '{}'",
            group.group_id
        ));
    }
    if !group.trust.is_trusted {
        return Err(format!(
            "group '{}' is not trusted but is present in the trusted grouped flow contract",
            group.group_id
        ));
    }
    if group.items.is_empty() {
        return Err(format!("group '{}' has no flow items", group.group_id));
    }

    let mut item_ids = BTreeSet::<String>::new();
    let mut visible_item_ids = BTreeSet::<String>::new();
    let mut slot_item_ids = BTreeSet::<String>::new();
    let mut previous_reading_order = None::<usize>;

    for item in &group.items {
        let item_id = item.item_id();
        if item_id.trim().is_empty() {
            return Err(format!(
                "group '{}' has item with empty item_id",
                group.group_id
            ));
        }
        if !item_ids.insert(item_id.to_owned()) {
            return Err(format!(
                "group '{}' has duplicate item_id '{}'",
                group.group_id, item_id
            ));
        }
        if let Some(previous) = previous_reading_order {
            if item.reading_order() < previous {
                return Err(format!(
                    "group '{}' items are not sorted by reading_order",
                    group.group_id
                ));
            }
        }
        previous_reading_order = Some(item.reading_order());

        match item {
            FlowItem::VisibleSpan(span) => {
                if span.text.trim().is_empty() {
                    return Err(format!(
                        "group '{}' visible span '{}' has empty text",
                        group.group_id, span.item_id
                    ));
                }
                visible_item_ids.insert(span.item_id.clone());
            }
            FlowItem::RedactionSlot(slot) => {
                if slot.page_index != group.page_index {
                    return Err(format!(
                        "group '{}' slot '{}' page_index {} does not match group page_index {}",
                        group.group_id, slot.item_id, slot.page_index, group.page_index
                    ));
                }
                slot_item_ids.insert(slot.item_id.clone());
            }
        }
    }

    if !slot_item_ids.is_empty() && group.segments.is_empty() {
        return Err(format!(
            "group '{}' contains redaction slots but no segments",
            group.group_id
        ));
    }

    let mut assigned_slot_ids = BTreeSet::<String>::new();
    let mut segment_ids = BTreeSet::<String>::new();
    for segment in &group.segments {
        validate_segment(
            segment,
            group,
            &slot_item_ids,
            &visible_item_ids,
            &mut assigned_slot_ids,
            &mut segment_ids,
        )?;
    }

    if assigned_slot_ids != slot_item_ids {
        return Err(format!(
            "group '{}' segments do not cover exactly the group's slot items",
            group.group_id
        ));
    }

    Ok(())
}

fn validate_segment(
    segment: &RedactionFlowSegment,
    group: &RedactionFlowGroup,
    slot_item_ids: &BTreeSet<String>,
    visible_item_ids: &BTreeSet<String>,
    assigned_slot_ids: &mut BTreeSet<String>,
    segment_ids: &mut BTreeSet<String>,
) -> Result<(), String> {
    let segment_id = segment.segment_id();
    if segment_id.trim().is_empty() {
        return Err(format!(
            "group '{}' has segment with empty segment_id",
            group.group_id
        ));
    }
    if !segment_ids.insert(segment_id.to_owned()) {
        return Err(format!(
            "group '{}' has duplicate segment_id '{}'",
            group.group_id, segment_id
        ));
    }
    if segment.group_id() != group.group_id {
        return Err(format!(
            "segment '{}' belongs to '{}' but is stored in group '{}'",
            segment_id,
            segment.group_id(),
            group.group_id
        ));
    }

    let slot_ids = segment.slot_item_ids();
    match segment {
        RedactionFlowSegment::SingleSlot(_) if slot_ids.len() != 1 => {
            return Err(format!(
                "single-slot segment '{}' must reference exactly one slot",
                segment_id
            ));
        }
        RedactionFlowSegment::Compound(_) if slot_ids.len() < 2 => {
            return Err(format!(
                "compound segment '{}' must reference at least two slots",
                segment_id
            ));
        }
        _ => {}
    }

    for slot_id in slot_ids {
        if !slot_item_ids.contains(slot_id) {
            return Err(format!(
                "segment '{}' references unknown slot '{}' in group '{}'",
                segment_id, slot_id, group.group_id
            ));
        }
        if !assigned_slot_ids.insert(slot_id.to_owned()) {
            return Err(format!(
                "segment '{}' reuses slot '{}' already assigned in group '{}'",
                segment_id, slot_id, group.group_id
            ));
        }
    }

    validate_anchor_links(segment.neighbor_facts(), visible_item_ids, segment_id)
}

fn validate_anchor_links(
    neighbor_facts: &SegmentNeighborFacts,
    visible_item_ids: &BTreeSet<String>,
    segment_id: &str,
) -> Result<(), String> {
    let mut seen_anchor_ids = BTreeSet::<String>::new();

    if let Some(anchor) = &neighbor_facts.left_outer_anchor {
        validate_anchor(
            anchor,
            visible_item_ids,
            segment_id,
            FlowLocalAnchorRole::LeftOuter,
            &mut seen_anchor_ids,
        )?;
    }
    if let Some(anchor) = &neighbor_facts.right_outer_anchor {
        validate_anchor(
            anchor,
            visible_item_ids,
            segment_id,
            FlowLocalAnchorRole::RightOuter,
            &mut seen_anchor_ids,
        )?;
    }
    for anchor in &neighbor_facts.internal_separators {
        validate_anchor(
            anchor,
            visible_item_ids,
            segment_id,
            FlowLocalAnchorRole::InternalSeparator,
            &mut seen_anchor_ids,
        )?;
    }

    Ok(())
}

fn validate_anchor(
    anchor: &FlowLocalAnchor,
    visible_item_ids: &BTreeSet<String>,
    segment_id: &str,
    expected_role: FlowLocalAnchorRole,
    seen_anchor_ids: &mut BTreeSet<String>,
) -> Result<(), String> {
    if anchor.item_id.trim().is_empty() {
        return Err(format!(
            "segment '{}' has anchor with empty item_id",
            segment_id
        ));
    }
    if anchor.text.trim().is_empty() {
        return Err(format!(
            "segment '{}' anchor '{}' has empty text",
            segment_id, anchor.item_id
        ));
    }
    if anchor.role != expected_role {
        return Err(format!(
            "segment '{}' anchor '{}' has role {:?} but expected {:?}",
            segment_id, anchor.item_id, anchor.role, expected_role
        ));
    }
    if !visible_item_ids.contains(&anchor.item_id) {
        return Err(format!(
            "segment '{}' anchor '{}' is not a visible span from the owning group",
            segment_id, anchor.item_id
        ));
    }
    if !seen_anchor_ids.insert(anchor.item_id.clone()) {
        return Err(format!(
            "segment '{}' reuses anchor '{}' across multiple neighbor facts",
            segment_id, anchor.item_id
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        validate_redaction_flow_collection, CompoundRedactionSegment, FlowItem, FlowLocalAnchor,
        FlowLocalAnchorRole, FlowRedactionSlot, FlowTrustPolicy, FlowTrustSummary, FlowVisibleSpan,
        RedactionFlowCollection, RedactionFlowGroup, RedactionFlowSegment, SegmentNeighborFacts,
    };
    use crate::types::redaction_types::{Rect, RedactionKind};

    #[test]
    fn grouped_flow_contract_accepts_local_anchor_references() {
        let group = sample_group();
        let collection = RedactionFlowCollection::new(vec![group]);

        let validation = validate_redaction_flow_collection(&collection);
        assert!(validation.is_ok(), "validation should pass: {validation:?}");
    }

    #[test]
    fn grouped_flow_contract_rejects_anchor_references_outside_the_group() {
        let mut group = sample_group();
        let RedactionFlowSegment::Compound(segment) = &mut group.segments[0] else {
            panic!("expected compound segment in sample_group");
        };
        segment.neighbor_facts.left_outer_anchor = Some(FlowLocalAnchor {
            item_id: "foreign-anchor".to_owned(),
            text: "FOREIGN".to_owned(),
            bbox: Rect::new(0.0_f32, 0.0_f32, 1.0_f32, 1.0_f32),
            line_bucket: 0,
            role: FlowLocalAnchorRole::LeftOuter,
        });
        let collection = RedactionFlowCollection::new(vec![group]);

        let validation = validate_redaction_flow_collection(&collection);
        assert!(
            validation.is_err(),
            "validation should fail for foreign anchors"
        );
    }

    fn sample_group() -> RedactionFlowGroup {
        let trust = FlowTrustSummary {
            is_trusted: true,
            trust_score: Some(1.0_f32),
            policy: Some(FlowTrustPolicy::Trusted),
            reasons: vec!["local anchors only".to_owned()],
        };
        let left_anchor = FlowVisibleSpan {
            item_id: "span-left".to_owned(),
            text: "Jean".to_owned(),
            bbox: Rect::new(10.0_f32, 10.0_f32, 32.0_f32, 20.0_f32),
            reading_order: 0,
            line_bucket: 5,
            role_hint: "left".to_owned(),
            source: "test".to_owned(),
            font_key: Some("F1".to_owned()),
            font_name: Some("Helvetica".to_owned()),
            font_size_pt: Some(11.0_f32),
            h_scale_pct: Some(100.0_f32),
        };
        let first_slot = FlowRedactionSlot {
            item_id: "slot-1".to_owned(),
            redaction_index: 0,
            page_index: 1,
            bbox: Rect::new(33.0_f32, 10.0_f32, 55.0_f32, 20.0_f32),
            kind: RedactionKind::DrawnRect,
            reading_order: 1,
        };
        let second_slot = FlowRedactionSlot {
            item_id: "slot-2".to_owned(),
            redaction_index: 1,
            page_index: 1,
            bbox: Rect::new(56.0_f32, 10.0_f32, 78.0_f32, 20.0_f32),
            kind: RedactionKind::DrawnRect,
            reading_order: 2,
        };
        let right_anchor = FlowVisibleSpan {
            item_id: "span-right".to_owned(),
            text: "Brunel".to_owned(),
            bbox: Rect::new(79.0_f32, 10.0_f32, 111.0_f32, 20.0_f32),
            reading_order: 3,
            line_bucket: 5,
            role_hint: "right".to_owned(),
            source: "test".to_owned(),
            font_key: Some("F1".to_owned()),
            font_name: Some("Helvetica".to_owned()),
            font_size_pt: Some(11.0_f32),
            h_scale_pct: Some(100.0_f32),
        };

        RedactionFlowGroup {
            group_id: "page-1-group-1".to_owned(),
            page_index: 1,
            bbox: Rect::new(10.0_f32, 10.0_f32, 111.0_f32, 20.0_f32),
            reading_order: 0,
            trust: trust.clone(),
            items: vec![
                FlowItem::VisibleSpan(left_anchor.clone()),
                FlowItem::RedactionSlot(first_slot),
                FlowItem::RedactionSlot(second_slot),
                FlowItem::VisibleSpan(right_anchor.clone()),
            ],
            segments: vec![RedactionFlowSegment::Compound(CompoundRedactionSegment {
                segment_id: "page-1-group-1-segment-1".to_owned(),
                group_id: "page-1-group-1".to_owned(),
                slot_item_ids: vec!["slot-1".to_owned(), "slot-2".to_owned()],
                neighbor_facts: SegmentNeighborFacts {
                    left_outer_anchor: Some(FlowLocalAnchor {
                        item_id: left_anchor.item_id.clone(),
                        text: left_anchor.text.clone(),
                        bbox: left_anchor.bbox,
                        line_bucket: left_anchor.line_bucket,
                        role: FlowLocalAnchorRole::LeftOuter,
                    }),
                    right_outer_anchor: Some(FlowLocalAnchor {
                        item_id: right_anchor.item_id.clone(),
                        text: right_anchor.text.clone(),
                        bbox: right_anchor.bbox,
                        line_bucket: right_anchor.line_bucket,
                        role: FlowLocalAnchorRole::RightOuter,
                    }),
                    internal_separators: Vec::new(),
                },
                trust,
                diagnostics: vec![
                    "compound segment created from consecutive redaction slots".to_owned()
                ],
            })],
            typography: None,
            diagnostics: vec!["trusted local flow group".to_owned()],
        }
    }
}
