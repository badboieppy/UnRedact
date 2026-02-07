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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OsFileAccessor;

impl Default for OsFileAccessor {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl OsFileAccessor {
    #[inline]
    pub fn new() -> Self {
        Self
    }
}

impl FileAccessor for OsFileAccessor {
    #[inline]
    fn read(&self, req: FileReadRequest) -> Result<FileReadResponse, String> {
        validate_read_request(&req)?;
        let bytes = std::fs::read(&req.path).map_err(|error| error.to_string())?;
        Ok(FileReadResponse { bytes })
    }
}

#[inline]
pub fn validate_read_request(req: &FileReadRequest) -> Result<(), String> {
    let empty = path_is_empty(&req.path);
    if empty {
        return Err("path is empty".to_owned());
    }

    Ok(())
}

fn path_is_empty(path: &Path) -> bool {
    path.as_os_str().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn file_read_request_is_eq() {
        let a = FileReadRequest {
            path: PathBuf::from("a.pdf"),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn file_read_response_is_eq() {
        let a = FileReadResponse {
            bytes: vec![1, 2, 3],
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn os_file_accessor_is_copy_eq() {
        let a = OsFileAccessor::new();
        let b = a;
        assert_eq!(a, b);
        assert_eq!(b, OsFileAccessor::new());
    }

    #[test]
    fn validate_read_request_rejects_empty_path() {
        let req = FileReadRequest {
            path: PathBuf::from(""),
        };
        let err = validate_read_request(&req).expect_err("expected error in test");
        assert_eq!(err, "path is empty".to_owned());
    }

    #[test]
    fn validate_read_request_accepts_non_empty_path() {
        let req = FileReadRequest {
            path: PathBuf::from("x.pdf"),
        };
        let ok = validate_read_request(&req);
        assert_eq!(ok.is_ok(), true);
    }

    #[test]
    fn os_file_accessor_read_fails_for_nonexistent_file() {
        let accessor = OsFileAccessor::new();
        let req = FileReadRequest {
            path: PathBuf::from("definitely-does-not-exist-12345.pdf"),
        };
        let err = accessor.read(req).expect_err("expected error in test");
        assert_eq!(err.is_empty(), false);
    }

    #[test]
    fn os_file_accessor_read_reads_file_bytes() {
        let dir = std::env::temp_dir();
        let path = dir.join("unredact_font_detection_dependency_file_accessor_test.bin");
        let _ignored_remove_result = std::fs::remove_file(&path);

        std::fs::write(&path, vec![9_u8, 8_u8, 7_u8]).expect("expected value in test");

        let accessor = OsFileAccessor::new();
        let resp = accessor
            .read(FileReadRequest { path: path.clone() })
            .expect("expected value in test");

        assert_eq!(resp.bytes, vec![9_u8, 8_u8, 7_u8]);

        let _ignored_remove_result = std::fs::remove_file(&path);
    }
}
