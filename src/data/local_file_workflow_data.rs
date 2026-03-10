use std::path::{Path, PathBuf};

use crate::dependency::file_store::FileStore;

#[derive(Debug, Clone, Copy)]
pub struct LocalFileWorkflowData {
    file_store: FileStore,
}

impl LocalFileWorkflowData {
    #[inline]
    pub fn new() -> Self {
        Self {
            file_store: FileStore,
        }
    }

    #[inline]
    pub fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, String> {
        self.file_store.read(path)
    }

    #[inline]
    pub fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        self.file_store.create_dir_all(path)
    }

    #[inline]
    pub fn read_dir_paths(&self, path: &Path) -> Result<Vec<PathBuf>, String> {
        self.file_store.read_dir(path)
    }

    #[inline]
    pub fn exists(&self, path: &Path) -> Result<bool, String> {
        self.file_store.exists(path)
    }

    #[inline]
    pub fn is_dir(&self, path: &Path) -> Result<bool, String> {
        self.file_store.is_dir(path)
    }
}

impl Default for LocalFileWorkflowData {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
