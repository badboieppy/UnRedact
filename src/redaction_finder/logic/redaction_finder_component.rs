use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::redaction_finder::data::file_retriever::{PdfFileRetriever, RedactionDataRetriever};
use crate::redaction_finder::types::{
    PdfRenderer, RedactionFinderConfig, RedactionFinderOutput, RedactionKind, RedactionMode,
    RedactionOccurrence, RedactionReport, UnderlyingTextHit,
};

pub fn run_redaction_finder_from_bytes(
    bytes: &[u8],
    renderer: Option<&dyn PdfRenderer>,
    cfg: RedactionFinderConfig,
) -> Result<RedactionFinderOutput, String> {
    let retriever = PdfFileRetriever::new_from_bytes(bytes, renderer)?;
    Ok(run_redaction_finder(&retriever, cfg))
}

pub fn run_redaction_finder(
    retriever: &dyn RedactionDataRetriever,
    cfg: RedactionFinderConfig,
) -> RedactionFinderOutput {
    let mut all: Vec<RedactionOccurrence> = Vec::new();
    let mut diagnostics: Vec<String> = Vec::new();

    for page_index in retriever.page_indices() {
        match cfg.mode {
            RedactionMode::Annotations | RedactionMode::All => match retriever
                .annotation_redactions(page_index, cfg.include_details)
            {
                Ok(v) => all.extend(v),
                Err(m) => diagnostics.push(format!("page_index={page_index} annotation_error={m}")),
            },
            RedactionMode::Drawn => {}
        }

        match cfg.mode {
            RedactionMode::Drawn | RedactionMode::All => match retriever.drawn_redactions(
                page_index,
                cfg.include_details,
                cfg.include_full_page_rects,
            ) {
                Ok(v) => all.extend(v),
                Err(m) => diagnostics.push(format!("page_index={page_index} drawn_error={m}")),
            },
            RedactionMode::Annotations => {}
        }

        if cfg.enable_image_analysis {
            match retriever.raster_redactions(page_index, &cfg) {
                Ok(v) => all.extend(v),
                Err(m) => {
                    diagnostics.push(format!("page_index={page_index} raster_page_error={m}"))
                }
            }
        }

        attach_underlying_text(retriever, page_index, &mut all, &mut diagnostics);
    }

    RedactionFinderOutput {
        redactions: dedup_occurrences(all),
        diagnostics,
    }
}

pub fn build_report(input: &Path, output: RedactionFinderOutput) -> RedactionReport {
    let mut occs = output.redactions;
    occs.sort_by(|a, b| {
        a.page_index
            .cmp(&b.page_index)
            .then_with(|| {
                a.bbox
                    .x0
                    .partial_cmp(&b.bbox.x0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                a.bbox
                    .y0
                    .partial_cmp(&b.bbox.y0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let mut page_counts: BTreeMap<u32, u32> = BTreeMap::new();
    for occurrence in &occs {
        *page_counts.entry(occurrence.page_index).or_insert(0) += 1;
    }

    RedactionReport {
        input: input.to_string_lossy().to_string(),
        redactions: occs.clone(),
        count: occs.len() as u32,
        page_counts,
        diagnostics: output.diagnostics,
    }
}

fn dedup_occurrences(items: Vec<RedactionOccurrence>) -> Vec<RedactionOccurrence> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();

    for item in items {
        let key = format!(
            "{}:{:.2}:{:.2}:{:.2}:{:.2}:{:?}",
            item.page_index, item.bbox.x0, item.bbox.y0, item.bbox.x1, item.bbox.y1, item.kind
        );
        if seen.insert(key) {
            out.push(item);
        }
    }

    out
}

fn attach_underlying_text(
    retriever: &dyn RedactionDataRetriever,
    page_index: u32,
    occs: &mut [RedactionOccurrence],
    diagnostics: &mut Vec<String>,
) {
    let page_redactions = occs
        .iter_mut()
        .filter(|occurrence| occurrence.page_index == page_index)
        .collect::<Vec<_>>();

    if page_redactions.is_empty() {
        return;
    }

    let hits = match retriever.underlying_text_hits(page_index) {
        Ok(v) => v,
        Err(m) => {
            diagnostics.push(format!("page_index={page_index} underlying_text_error={m}"));
            return;
        }
    };

    if hits.is_empty() {
        return;
    }

    for redaction in page_redactions {
        redaction.underlying_text = hits
            .iter()
            .filter(|hit| rects_intersect(&redaction.bbox, &hit.bbox))
            .cloned()
            .collect::<Vec<UnderlyingTextHit>>();
    }
}

fn rects_intersect(
    a: &crate::redaction_finder::types::Rect,
    b: &crate::redaction_finder::types::Rect,
) -> bool {
    let x_overlap = a.x0 < b.x1 && a.x1 > b.x0;
    let y_overlap = a.y0 < b.y1 && a.y1 > b.y0;
    x_overlap && y_overlap
}

pub fn is_unknown_kind(kind: &RedactionKind) -> bool {
    matches!(kind, RedactionKind::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_kind_helper_is_deterministic() {
        assert!(is_unknown_kind(&RedactionKind::Unknown));
        assert!(!is_unknown_kind(&RedactionKind::Annotation));
    }
}
