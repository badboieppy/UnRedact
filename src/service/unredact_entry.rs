use std::path::{Path, PathBuf};

use crate::logic::{run_orchestrator, OrchestratorConfig, OrchestratorRequest};
use crate::types::guess_types::GuessConfig;
use crate::types::visualizer_config::VisualizerConfig;

#[derive(Debug, Clone, PartialEq)]
pub struct UnredactServiceConfig {
    pub include_details: bool,
    pub include_full_page_rects: bool,
    pub enable_image_analysis: bool,
    pub raster_dpi: f32,
    pub guess: GuessConfig,
    pub visualize: bool,
    pub visualizer: VisualizerConfig,
}

impl Default for UnredactServiceConfig {
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
pub struct UnredactServiceRequest {
    pub input: PathBuf,
    pub output_dir: PathBuf,
    pub dictionary_path: Option<PathBuf>,
    pub cfg: UnredactServiceConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnredactServiceOutputs {
    pub redactions_path: PathBuf,
    pub fonts_path: PathBuf,
    pub guesses_path: PathBuf,
    pub visualized_pdf_path: Option<PathBuf>,
}

#[inline]
pub fn run_from_paths(
    input: &Path,
    output_dir: &Path,
    dictionary_path: Option<&Path>,
    cfg: UnredactServiceConfig,
) -> Result<UnredactServiceOutputs, String> {
    run(UnredactServiceRequest {
        input: input.to_path_buf(),
        output_dir: output_dir.to_path_buf(),
        dictionary_path: dictionary_path.map(PathBuf::from),
        cfg,
    })
}

#[inline]
pub fn run(req: UnredactServiceRequest) -> Result<UnredactServiceOutputs, String> {
    let orchestrator_req = OrchestratorRequest {
        input: req.input,
        output_dir: req.output_dir,
        dictionary_path: req.dictionary_path,
        cfg: OrchestratorConfig {
            include_details: req.cfg.include_details,
            include_full_page_rects: req.cfg.include_full_page_rects,
            enable_image_analysis: req.cfg.enable_image_analysis,
            raster_dpi: req.cfg.raster_dpi,
            guess: req.cfg.guess,
            visualize: req.cfg.visualize,
            visualizer: req.cfg.visualizer,
        },
    };
    let out = run_orchestrator(orchestrator_req)?;
    Ok(UnredactServiceOutputs {
        redactions_path: out.redactions_path,
        fonts_path: out.fonts_path,
        guesses_path: out.guesses_path,
        visualized_pdf_path: out.visualized_pdf_path,
    })
}
