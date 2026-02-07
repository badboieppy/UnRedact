pub mod font_detection;
pub mod redaction_finder;

pub use redaction_finder::{
    build_report, find_redactions_in_pdf_bytes_vector_only,
    find_redactions_in_pdf_bytes_with_renderer, find_redactions_in_pdf_path_with_renderer,
    HayroRenderer, PdfRenderer, Rect, RedactionError, RedactionFinderConfig, RedactionFinderOutput,
    RedactionKind, RedactionMode, RedactionOccurrence, RedactionReport, RenderedPage,
    UnderlyingTextHit,
};
