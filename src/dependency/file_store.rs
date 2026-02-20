use std::fs;
use std::io::ErrorKind;
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

    #[inline]
    pub fn create_dir_all(&self, path: &Path) -> Result<(), String> {
        fs::create_dir_all(path)
            .map_err(|error| format!("failed to create directory {}: {error}", path.display()))
    }

    #[inline]
    pub fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>, String> {
        let entries = fs::read_dir(path)
            .map_err(|error| format!("failed to read directory {}: {error}", path.display()))?;
        let mut out = Vec::<PathBuf>::new();
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!(
                    "failed to read directory entry in {}: {error}",
                    path.display()
                )
            })?;
            out.push(entry.path());
        }
        Ok(out)
    }

    #[inline]
    pub fn exists(&self, path: &Path) -> Result<bool, String> {
        path.try_exists()
            .map_err(|error| format!("failed to inspect {}: {error}", path.display()))
    }

    #[inline]
    pub fn is_dir(&self, path: &Path) -> Result<bool, String> {
        match fs::metadata(path) {
            Ok(metadata) => Ok(metadata.is_dir()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("failed to inspect {}: {error}", path.display())),
        }
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
