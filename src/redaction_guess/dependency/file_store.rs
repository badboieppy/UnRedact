use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct FileStore;

impl FileStore {
    #[inline]
    pub fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
        fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))
    }
}
