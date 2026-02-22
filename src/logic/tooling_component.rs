use std::collections::BTreeMap;

use crate::data::dictionary_data::DictionaryData;
use crate::data::redactions_data::{
    PdfFileRetriever, PdfPageRenderer, RedactionDataRetriever as _, RedactionsData,
};
use crate::types::guess_types::{GuessConfig, GuessReport};
use crate::types::redaction_types::{
    PdfRenderer as _, RedactionReport, RenderedPage, UnderlyingTextHit,
};

use super::redaction_guessing_component::{run_guess_from_bytes, RunGuessFromBytesRequest};

#[derive(Debug, Clone, PartialEq)]
pub struct ToolingDictionaryInputs {
    pub dictionary: Vec<String>,
    pub diagnostics: Vec<String>,
}

pub struct ToolingGuessRequest<'a> {
    pub input_name: &'a str,
    pub pdf_bytes: &'a [u8],
    pub redactions: &'a RedactionReport,
    pub dictionary: &'a [String],
    pub dictionary_diagnostics: &'a [String],
    pub guess: &'a GuessConfig,
}

pub struct ToolingPdfRenderer {
    renderer: PdfPageRenderer,
}

impl ToolingPdfRenderer {
    #[inline]
    pub fn new_from_bytes(pdf_bytes: &[u8]) -> Result<Self, String> {
        let renderer = RedactionsData::new().build_renderer(pdf_bytes)?;
        Ok(Self { renderer })
    }

    #[inline]
    pub fn page_count(&self) -> usize {
        self.renderer.page_count()
    }

    #[inline]
    pub fn render_page_to_rgba(&self, page_index: usize, dpi: f32) -> Result<RenderedPage, String> {
        self.renderer.render_page_to_rgba(page_index, dpi)
    }
}

#[inline]
pub fn default_name_dictionary_entries() -> &'static [&'static str] {
    crate::data::default_name_dictionary::DEFAULT_NAME_DICTIONARY
}

#[inline]
pub fn load_dictionary_from_bytes(
    dictionary_bytes: Option<&[u8]>,
) -> Result<ToolingDictionaryInputs, String> {
    let loaded = DictionaryData::new().load_dictionary_from_bytes(dictionary_bytes)?;
    Ok(ToolingDictionaryInputs {
        dictionary: loaded.dictionary,
        diagnostics: loaded.diagnostics,
    })
}

#[inline]
pub fn collect_underlying_text_hits_by_page(
    pdf_bytes: &[u8],
) -> Result<BTreeMap<u32, Vec<UnderlyingTextHit>>, String> {
    let retriever = PdfFileRetriever::new_from_bytes(pdf_bytes, None)?;
    let mut by_page = BTreeMap::<u32, Vec<UnderlyingTextHit>>::new();
    for page_index in retriever.page_indices() {
        let hits = retriever.underlying_text_hits(page_index)?;
        if !hits.is_empty() {
            by_page.insert(page_index, hits);
        }
    }
    Ok(by_page)
}

#[inline]
pub fn run_guess_from_redactions(req: ToolingGuessRequest<'_>) -> Result<GuessReport, String> {
    run_guess_from_bytes(RunGuessFromBytesRequest {
        pdf_name: req.input_name,
        pdf_bytes: req.pdf_bytes,
        redactions: req.redactions,
        dictionary: req.dictionary,
        diagnostics: req.dictionary_diagnostics,
        cfg: req.guess,
    })
}
