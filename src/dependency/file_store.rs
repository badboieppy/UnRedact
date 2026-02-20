use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReadRequest {
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReadResponse {
    pub bytes: Vec<u8>,
}

pub trait FileAccessor {
    fn read(&self, req: FileReadRequest) -> Result<FileReadResponse, String>;
}

#[derive(Debug, Clone, Copy)]
pub struct FileStore;

impl FileStore {
    #[inline]
    pub fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
        fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))
    }

    #[inline]
    pub fn write_exact(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        ensure_parent_dir(path)?;
        fs::write(path, bytes).map_err(|e| format!("failed to write {}: {e}", path.display()))
    }

    #[inline]
    pub fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        ensure_parent_dir(path)?;
        let mut file = fs::File::create(path)
            .map_err(|e| format!("failed to create {}: {e}", path.display()))?;
        file.write_all(bytes)
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|e| format!("failed to write {}: {e}", path.display()))
    }
}

impl FileAccessor for FileStore {
    #[inline]
    fn read(&self, req: FileReadRequest) -> Result<FileReadResponse, String> {
        validate_read_request(&req)?;
        let bytes = self.read(&req.path)?;
        Ok(FileReadResponse { bytes })
    }
}

#[inline]
pub fn validate_read_request(req: &FileReadRequest) -> Result<(), String> {
    let empty = req.path.as_os_str().is_empty();
    if empty {
        return Err("path is empty".to_owned());
    }

    Ok(())
}

#[inline]
fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create directory {}: {e}", parent.display()))?;
        }
    }
    Ok(())
}
