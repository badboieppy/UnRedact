use std::path::Path;

use crate::dependency::file_store::FileStore;
use crate::dependency::hayro_renderer::HayroRenderer;
use crate::dependency::pdf_redaction_accessor::PdfFileRetriever as DependencyPdfFileRetriever;
use crate::dependency::pdf_redaction_accessor::RedactionDataRetriever as DependencyRedactionDataRetriever;
use crate::types::redaction_types::RedactionReport;
use crate::types::redaction_types::{
    PdfRenderer, RedactionFinderConfig, RedactionOccurrence, UnderlyingTextHit,
};

#[derive(Debug, Clone, Copy)]
pub struct RedactionsData {
    file_store: FileStore,
}

impl RedactionsData {
    #[inline]
    pub fn new() -> Self {
        Self {
            file_store: FileStore,
        }
    }

    #[inline]
    pub fn read_input_bytes(&self, input: &Path) -> Result<Vec<u8>, String> {
        self.file_store.read(input)
    }

    #[inline]
    pub fn build_renderer(&self, bytes: &[u8]) -> Result<HayroRenderer, String> {
        HayroRenderer::new_from_bytes(bytes)
    }

    #[inline]
    pub fn write_redactions(&self, path: &Path, report: &RedactionReport) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(report)
            .map_err(|e| format!("failed to encode redactions json: {e}"))?;
        self.file_store.write(path, &json)
    }
}

impl Default for RedactionsData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

pub trait RedactionDataRetriever {
    fn page_indices(&self) -> Vec<u32>;
    fn annotation_redactions(
        &self,
        page_index: u32,
        include_details: bool,
    ) -> Result<Vec<RedactionOccurrence>, String>;
    fn drawn_redactions(
        &self,
        page_index: u32,
        include_details: bool,
        include_full_page_rects: bool,
    ) -> Result<Vec<RedactionOccurrence>, String>;
    fn raster_redactions(
        &self,
        page_index: u32,
        cfg: &RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String>;
    fn underlying_text_hits(&self, page_index: u32) -> Result<Vec<UnderlyingTextHit>, String>;
    fn ocr_context_hits(
        &self,
        page_index: u32,
        red_bbox: &crate::types::redaction_types::Rect,
        cfg: &RedactionFinderConfig,
    ) -> Result<Vec<UnderlyingTextHit>, String>;
}

pub struct PdfFileRetriever<'renderer> {
    inner: DependencyPdfFileRetriever<'renderer>,
}

impl<'renderer> PdfFileRetriever<'renderer> {
    #[inline]
    pub fn new_from_bytes(
        bytes: &[u8],
        renderer: Option<&'renderer dyn PdfRenderer>,
    ) -> Result<Self, String> {
        let inner = DependencyPdfFileRetriever::new_from_bytes(bytes, renderer)?;
        Ok(Self { inner })
    }
}

impl RedactionDataRetriever for PdfFileRetriever<'_> {
    #[inline]
    fn page_indices(&self) -> Vec<u32> {
        DependencyRedactionDataRetriever::page_indices(&self.inner)
    }

    #[inline]
    fn annotation_redactions(
        &self,
        page_index: u32,
        include_details: bool,
    ) -> Result<Vec<RedactionOccurrence>, String> {
        DependencyRedactionDataRetriever::annotation_redactions(
            &self.inner,
            page_index,
            include_details,
        )
    }

    #[inline]
    fn drawn_redactions(
        &self,
        page_index: u32,
        include_details: bool,
        include_full_page_rects: bool,
    ) -> Result<Vec<RedactionOccurrence>, String> {
        DependencyRedactionDataRetriever::drawn_redactions(
            &self.inner,
            page_index,
            include_details,
            include_full_page_rects,
        )
    }

    #[inline]
    fn raster_redactions(
        &self,
        page_index: u32,
        cfg: &RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String> {
        DependencyRedactionDataRetriever::raster_redactions(&self.inner, page_index, cfg)
    }

    #[inline]
    fn underlying_text_hits(&self, page_index: u32) -> Result<Vec<UnderlyingTextHit>, String> {
        DependencyRedactionDataRetriever::underlying_text_hits(&self.inner, page_index)
    }

    #[inline]
    fn ocr_context_hits(
        &self,
        page_index: u32,
        red_bbox: &crate::types::redaction_types::Rect,
        cfg: &RedactionFinderConfig,
    ) -> Result<Vec<UnderlyingTextHit>, String> {
        DependencyRedactionDataRetriever::ocr_context_hits(&self.inner, page_index, red_bbox, cfg)
    }
}
