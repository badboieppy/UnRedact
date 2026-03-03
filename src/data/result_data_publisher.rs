use std::path::Path;

use crate::dependency::file_store::FileStore;

#[derive(Debug, Clone, Copy)]
pub struct ResultPublishPaths<'a> {
    pub redactions_path: &'a Path,
    pub fonts_path: &'a Path,
    pub guesses_path: &'a Path,
    pub anchors_path: &'a Path,
    pub visualized_pdf_path: Option<&'a Path>,
}

#[derive(Debug, Clone, Copy)]
pub struct ResultPublishPayload<'a> {
    pub redactions_json: &'a [u8],
    pub fonts_json: &'a [u8],
    pub guesses_json: &'a [u8],
    pub anchors_json: &'a [u8],
    pub visualized_pdf_bytes: Option<&'a [u8]>,
}

#[derive(Debug, Clone, Copy)]
pub struct ResultPublishRequest<'a> {
    pub paths: ResultPublishPaths<'a>,
    pub payload: ResultPublishPayload<'a>,
}

#[derive(Debug, Clone, Copy)]
pub struct ResultDataPublisher {
    file_store: FileStore,
}

impl ResultDataPublisher {
    #[inline]
    pub fn new() -> Self {
        Self {
            file_store: FileStore,
        }
    }

    #[inline]
    pub fn publish(&self, req: ResultPublishRequest<'_>) -> Result<(), String> {
        self.file_store
            .write_exact(req.paths.redactions_path, req.payload.redactions_json)?;
        self.file_store
            .write_exact(req.paths.fonts_path, req.payload.fonts_json)?;
        self.file_store
            .write_exact(req.paths.guesses_path, req.payload.guesses_json)?;
        self.file_store
            .write_exact(req.paths.anchors_path, req.payload.anchors_json)?;
        if let (Some(path), Some(bytes)) = (
            req.paths.visualized_pdf_path,
            req.payload.visualized_pdf_bytes,
        ) {
            self.file_store.write_exact(path, bytes)?;
        }
        Ok(())
    }

    #[inline]
    pub fn publish_bytes(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        self.file_store.write_exact(path, bytes)
    }
}

impl Default for ResultDataPublisher {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
