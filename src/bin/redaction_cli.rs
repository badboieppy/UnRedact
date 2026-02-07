use clap::{Parser, ValueEnum};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use unredact::{
    build_report, find_redactions_in_pdf_bytes_vector_only,
    find_redactions_in_pdf_bytes_with_renderer, HayroRenderer, RedactionFinderConfig,
    RedactionMode,
};

#[derive(Debug, Parser)]
#[command(
    name = "redaction_cli",
    about = "Detect redactions (annotations, drawn shapes, raster regions) in PDFs"
)]
struct Args {
    /// PDF file to scan
    input: PathBuf,

    /// Optional path to write the JSON report (stdout if omitted)
    #[arg(long)]
    output: Option<PathBuf>,

    /// Include metadata/details for each detected redaction
    #[arg(long)]
    details: bool,

    /// Detection mode (annotations only, drawn only, or all)
    #[arg(long, default_value_t = CliRedactionMode::All, value_enum)]
    mode: CliRedactionMode,

    /// Include full-page rectangles in drawn detection results
    #[arg(long)]
    include_full_page_rects: bool,

    /// Skip raster image analysis (vector-only detection)
    #[arg(long)]
    no_image_analysis: bool,

    /// Raster render DPI used by the renderer-backed pipeline
    #[arg(long, default_value_t = 200.0)]
    raster_dpi: f32,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliRedactionMode {
    Annotations,
    Drawn,
    All,
}

impl From<CliRedactionMode> for RedactionMode {
    fn from(mode: CliRedactionMode) -> Self {
        match mode {
            CliRedactionMode::Annotations => RedactionMode::Annotations,
            CliRedactionMode::Drawn => RedactionMode::Drawn,
            CliRedactionMode::All => RedactionMode::All,
        }
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            std::process::ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse();

    let mut cfg = RedactionFinderConfig::default();
    cfg.include_details = args.details;
    cfg.mode = args.mode.into();
    cfg.include_full_page_rects = args.include_full_page_rects;
    cfg.enable_image_analysis = !args.no_image_analysis;
    cfg.raster_dpi = args.raster_dpi;

    if !cfg.raster_dpi.is_finite() || cfg.raster_dpi <= 0.0 {
        return Err(format!("invalid --raster-dpi value: {}", cfg.raster_dpi));
    }

    let output = if args.no_image_analysis {
        let bytes = fs::read(&args.input).map_err(|e| format!("failed to read input: {e}"))?;
        find_redactions_in_pdf_bytes_vector_only(&bytes, cfg)
            .map_err(|e| format!("redaction_scan_failed: {e}"))?
    } else {
        let bytes = fs::read(&args.input).map_err(|e| format!("failed to read input: {e}"))?;
        let renderer = HayroRenderer::new_from_bytes(&bytes)
            .map_err(|e| format!("failed to initialize hayro renderer: {e}"))?;
        find_redactions_in_pdf_bytes_with_renderer(&bytes, &renderer, cfg)
            .map_err(|e| format!("redaction_scan_failed: {e}"))?
    };

    let report = build_report(&args.input, output);
    let json =
        serde_json::to_vec_pretty(&report).map_err(|e| format!("failed to encode json: {e}"))?;

    match args.output {
        Some(path) => write_to_file(&path, &json)?,
        None => write_to_stdout(&json)?,
    }

    Ok(())
}

fn write_to_stdout(bytes: &[u8]) -> Result<(), String> {
    let mut out = io::stdout().lock();
    out.write_all(bytes)
        .and_then(|_| out.write_all(b"\n"))
        .map_err(|e| format!("failed to write stdout: {e}"))
}

fn write_to_file(path: &PathBuf, bytes: &[u8]) -> Result<(), String> {
    let mut file =
        fs::File::create(path).map_err(|e| format!("failed to create {}: {e}", path.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|e| format!("failed to write {}: {e}", path.display()))
}
