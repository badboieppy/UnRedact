use crate::dependency::hayro_renderer::HayroRenderer;
use crate::dependency::pdf_redaction::{
    PdfFileRetriever as DependencyPdfFileRetriever,
    RedactionDataRetriever as DependencyRedactionDataRetriever,
};
use crate::types::redaction_types::{
    PdfRenderer, RedactionFinderConfig, RedactionOccurrence, RenderedPage, UnderlyingTextHit,
};

#[derive(Debug, Clone, Copy)]
pub struct RedactionsData;

pub struct PdfPageRenderer {
    inner: HayroRenderer,
}

impl PdfPageRenderer {
    #[inline]
    pub fn new_from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let inner = HayroRenderer::new_from_bytes(bytes)?;
        Ok(Self { inner })
    }
}

impl PdfRenderer for PdfPageRenderer {
    #[inline]
    fn page_count(&self) -> usize {
        self.inner.page_count()
    }

    #[inline]
    fn render_page_to_rgba(
        &self,
        page_index: usize,
        target_dpi: f32,
    ) -> Result<RenderedPage, String> {
        self.inner.render_page_to_rgba(page_index, target_dpi)
    }
}

impl RedactionsData {
    #[inline]
    pub fn new() -> Self {
        Self
    }

    #[inline]
    pub fn build_renderer(&self, bytes: &[u8]) -> Result<PdfPageRenderer, String> {
        PdfPageRenderer::new_from_bytes(bytes)
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
        cfg: RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String>;
    fn drawn_redactions(
        &self,
        page_index: u32,
        cfg: RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String>;
    fn raster_redactions(
        &self,
        page_index: u32,
        cfg: RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String>;
    fn underlying_text_hits(&self, page_index: u32) -> Result<Vec<UnderlyingTextHit>, String>;
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
        cfg: RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String> {
        DependencyRedactionDataRetriever::annotation_redactions(&self.inner, page_index, cfg)
    }

    #[inline]
    fn drawn_redactions(
        &self,
        page_index: u32,
        cfg: RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String> {
        DependencyRedactionDataRetriever::drawn_redactions(&self.inner, page_index, cfg)
    }

    #[inline]
    fn raster_redactions(
        &self,
        page_index: u32,
        cfg: RedactionFinderConfig,
    ) -> Result<Vec<RedactionOccurrence>, String> {
        DependencyRedactionDataRetriever::raster_redactions(&self.inner, page_index, cfg)
    }

    #[inline]
    fn underlying_text_hits(&self, page_index: u32) -> Result<Vec<UnderlyingTextHit>, String> {
        DependencyRedactionDataRetriever::underlying_text_hits(&self.inner, page_index)
    }
}
