use std::path::{Path, PathBuf};

use crate::redaction_finder::types::RedactionFinderConfig;
use crate::redaction_guess::types::GuessConfig;
use crate::unredact_cli::data::{DictionaryData, FontData, GuessData, RedactionData};

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestratorConfig {
    pub include_details: bool,
    pub include_full_page_rects: bool,
    pub enable_image_analysis: bool,
    pub raster_dpi: f32,
    pub guess: GuessConfig,
}

impl Default for OrchestratorConfig {
    #[inline]
    fn default() -> Self {
        Self {
            include_details: false,
            include_full_page_rects: false,
            enable_image_analysis: true,
            raster_dpi: 200.0_f32,
            guess: GuessConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestratorRequest {
    pub input: PathBuf,
    pub output_dir: PathBuf,
    pub dictionary_path: Option<PathBuf>,
    pub cfg: OrchestratorConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestratorOutputs {
    pub redactions_path: PathBuf,
    pub fonts_path: PathBuf,
    pub guesses_path: PathBuf,
}

#[inline]
pub fn run_orchestrator(req: OrchestratorRequest) -> Result<OrchestratorOutputs, String> {
    let outputs = build_output_paths(&req.input, &req.output_dir)?;

    let redaction_data = RedactionData::new();
    let font_data = FontData::new();
    let dictionary_data = DictionaryData::new();
    let guess_data = GuessData::new();

    let bytes = redaction_data.read_input_bytes(&req.input)?;
    let redaction_cfg = RedactionFinderConfig {
        include_details: req.cfg.include_details,
        mode: crate::redaction_finder::types::RedactionMode::All,
        include_full_page_rects: req.cfg.include_full_page_rects,
        enable_image_analysis: req.cfg.enable_image_analysis,
        raster_dpi: req.cfg.raster_dpi,
    };
    let redactions = redaction_data.detect_redactions(&req.input, &bytes, &redaction_cfg)?;
    redaction_data.write_redactions(&outputs.redactions_path, &redactions)?;

    let fonts = font_data.detect_fonts(&req.input, req.cfg.include_details)?;
    font_data.write_fonts(&outputs.fonts_path, &fonts)?;

    let (dictionary, mut diagnostics) = dictionary_data.build_dictionary(
        req.dictionary_path.as_deref(),
        &fonts,
        req.cfg.guess.max_dictionary,
    )?;

    diagnostics.push(format!("redactions_count={}", redactions.redactions.len()));

    let guess_report = guess_data.build_guess_report(
        &outputs.redactions_path,
        &outputs.fonts_path,
        redactions,
        dictionary,
        diagnostics,
        &req.cfg.guess,
    );
    guess_data.write_guesses(&outputs.guesses_path, &guess_report)?;

    Ok(outputs)
}

#[inline]
pub fn build_output_paths(input: &Path, output_dir: &Path) -> Result<OrchestratorOutputs, String> {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "input file has no stem".to_owned())?;
    let redactions_path = output_dir.join(format!("{stem}.redactions.json"));
    let fonts_path = output_dir.join(format!("{stem}.fonts.json"));
    let guesses_path = output_dir.join(format!("{stem}.guesses.json"));
    Ok(OrchestratorOutputs {
        redactions_path,
        fonts_path,
        guesses_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_output_paths_uses_stem_and_dir() {
        let input = Path::new("C:/data/report.pdf");
        let out = build_output_paths(input, Path::new("C:/out")).expect("expected value in test");
        assert_eq!(
            out.redactions_path,
            PathBuf::from("C:/out/report.redactions.json")
        );
        assert_eq!(out.fonts_path, PathBuf::from("C:/out/report.fonts.json"));
        assert_eq!(
            out.guesses_path,
            PathBuf::from("C:/out/report.guesses.json")
        );
    }
}
