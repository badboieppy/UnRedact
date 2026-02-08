use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct FileStore;

impl FileStore {
    #[inline]
    pub fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
        fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))
    }

    #[inline]
    pub fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create directory {}: {e}", parent.display()))?;
        }
        let mut file = fs::File::create(path)
            .map_err(|e| format!("failed to create {}: {e}", path.display()))?;
        use std::io::Write as _;
        file.write_all(bytes)
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|e| format!("failed to write {}: {e}", path.display()))
    }
}
