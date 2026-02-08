use std::path::{Path, PathBuf};

use crate::redaction_finder::types::RedactionFinderConfig;
use crate::redaction_guess::types::GuessConfig;
use crate::redaction_visualizer::logic::VisualizerConfig;
use crate::unredact_orchestrator::data::{FontData, RedactionData};
use crate::unredact_orchestrator::dependency::FileStore;

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestratorConfig {
    pub include_details: bool,
    pub include_full_page_rects: bool,
    pub enable_image_analysis: bool,
    pub raster_dpi: f32,
    pub guess: GuessConfig,
    pub visualize: bool,
    pub visualizer: VisualizerConfig,
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
            visualize: false,
            visualizer: VisualizerConfig::default(),
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
    pub visualized_pdf_path: Option<PathBuf>,
}

#[inline]
pub fn run_orchestrator(req: OrchestratorRequest) -> Result<OrchestratorOutputs, String> {
    let outputs = build_output_paths(&req.input, &req.output_dir)?;

    let redaction_data = RedactionData::new();
    let font_data = FontData::new();
    let file_store = FileStore;

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

    let redactions_for_visualizer = redactions.clone();
    let guess_report = crate::redaction_guess::service::run_from_paths(
        &outputs.redactions_path,
        &outputs.fonts_path,
        &req.input,
        req.dictionary_path.as_deref(),
        req.cfg.guess,
    )?;
    let guesses_json = serde_json::to_vec_pretty(&guess_report)
        .map_err(|e| format!("failed to encode guesses json: {e}"))?;
    file_store.write(&outputs.guesses_path, &guesses_json)?;

    if req.cfg.visualize {
        let output_path = outputs
            .visualized_pdf_path
            .clone()
            .ok_or_else(|| "visualized pdf path missing".to_owned())?;
        let accessor = crate::font_detection::dependency::file_accessor::OsFileAccessor::new();
        let font_runs =
            crate::font_detection::service::entry::detect_font_runs(&accessor, &req.input)?;
        crate::redaction_visualizer::service::run_from_report(
            &req.input,
            &redactions_for_visualizer,
            Some(&guess_report),
            Some(&font_runs),
            &output_path,
            req.cfg.visualizer,
        )?;
    }

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
    let visualized_pdf_path = Some(output_dir.join(format!("{stem}.visualized.pdf")));
    Ok(OrchestratorOutputs {
        redactions_path,
        fonts_path,
        guesses_path,
        visualized_pdf_path,
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
        assert_eq!(
            out.visualized_pdf_path,
            Some(PathBuf::from("C:/out/report.visualized.pdf"))
        );
    }
}
