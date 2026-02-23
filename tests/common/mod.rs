use std::path::{Path, PathBuf};

use unredact::types::guess_types::GuessReport;

pub fn test_output_dir(tag: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "unredact_test_{}_{}_{}",
        tag,
        std::process::id(),
        stamp
    ))
}

pub fn load_guess_report(path: &Path) -> GuessReport {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("failed to read guesses {}: {error}", path.display()));
    serde_json::from_slice::<GuessReport>(&bytes)
        .unwrap_or_else(|error| panic!("failed to parse guesses {}: {error}", path.display()))
}
