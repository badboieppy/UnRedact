use std::path::Path;

use crate::redaction_finder::types::{Rect, RedactionReport};
use crate::redaction_visualizer::dependency::FileStore;

#[derive(Debug, Clone)]
pub struct VisualizationInputs {
    pub pdf_bytes: Vec<u8>,
    pub rects: Vec<(u32, Rect)>,
}

pub trait VisualizationDataSource {
    fn load_inputs(&self, pdf_path: &Path, report: &RedactionReport) -> Result<VisualizationInputs, String>;
    fn write_output(&self, output_path: &Path, bytes: &[u8]) -> Result<(), String>;
}

#[derive(Debug, Clone, Copy)]
pub struct VisualizationData {
    file_store: FileStore,
}

impl VisualizationData {
    #[inline]
    pub fn new() -> Self {
        Self {
            file_store: FileStore,
        }
    }
}

impl Default for VisualizationData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl VisualizationDataSource for VisualizationData {
    #[inline]
    fn load_inputs(&self, pdf_path: &Path, report: &RedactionReport) -> Result<VisualizationInputs, String> {
        let pdf_bytes = self.file_store.read(pdf_path)?;
        let mut rects = Vec::with_capacity(report.redactions.len());
        for r in &report.redactions {
            rects.push((r.page_index, r.bbox));
        }
        Ok(VisualizationInputs { pdf_bytes, rects })
    }

    #[inline]
    fn write_output(&self, output_path: &Path, bytes: &[u8]) -> Result<(), String> {
        self.file_store.write(output_path, bytes)
    }
}
