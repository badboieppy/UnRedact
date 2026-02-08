use clap::Parser;
use std::path::PathBuf;

use unredact::redaction_guess::types::GuessConfig;
use unredact::unredact_orchestrator::logic::OrchestratorConfig;
use unredact::unredact_orchestrator::service::run_from_paths;

#[derive(Debug, Parser)]
#[command(
    name = "unredact_orchestrator_cli",
    about = "Run redaction detection, font detection, and guessing in one pass"
)]
struct Args {
    input: PathBuf,

    #[arg(long)]
    output_dir: Option<PathBuf>,

    #[arg(long)]
    dictionary: Option<PathBuf>,

    #[arg(long)]
    details: bool,

    #[arg(long)]
    include_full_page_rects: bool,

    #[arg(long)]
    no_image_analysis: bool,

    #[arg(long, default_value_t = 200.0_f32)]
    raster_dpi: f32,

    #[arg(long, default_value_t = 4)]
    max_words: usize,

    #[arg(long, default_value_t = 50)]
    max_candidates: usize,

    #[arg(long, default_value_t = 2000)]
    max_dictionary: usize,

    #[arg(long, default_value_t = 4.0)]
    tol_pt: f64,

    #[arg(long, default_value_t = 50_000)]
    max_nodes: usize,

    #[arg(long, default_value_t = false)]
    visualize: bool,

    #[arg(long, default_value_t = 1.0)]
    visualize_border: f32,
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

    if !args.raster_dpi.is_finite() || args.raster_dpi <= 0.0 {
        return Err(format!("invalid --raster-dpi value: {}", args.raster_dpi));
    }
    if args.max_words == 0 {
        return Err("max_words must be > 0".to_owned());
    }
    if args.max_candidates == 0 {
        return Err("max_candidates must be > 0".to_owned());
    }
    if args.max_dictionary == 0 {
        return Err("max_dictionary must be > 0".to_owned());
    }
    if !args.tol_pt.is_finite() || args.tol_pt <= 0.0 {
        return Err("tol_pt must be finite and > 0".to_owned());
    }
    if args.max_nodes == 0 {
        return Err("max_nodes must be > 0".to_owned());
    }
    if !args.visualize_border.is_finite() || args.visualize_border <= 0.0 {
        return Err("visualize_border must be finite and > 0".to_owned());
    }

    let output_dir = args.output_dir.clone().unwrap_or_else(default_output_dir);

    let cfg = OrchestratorConfig {
        include_details: args.details,
        include_full_page_rects: args.include_full_page_rects,
        enable_image_analysis: !args.no_image_analysis,
        raster_dpi: args.raster_dpi,
        guess: GuessConfig {
            max_words: args.max_words,
            max_candidates: args.max_candidates,
            max_dictionary: args.max_dictionary,
            tol_pt: args.tol_pt,
            max_nodes: args.max_nodes,
        },
        visualize: args.visualize,
        visualizer: unredact::redaction_visualizer::logic::VisualizerConfig {
            color: [1.0, 0.0, 0.0],
            border_width: args.visualize_border,
        },
    };

    let outputs = run_from_paths(&args.input, &output_dir, args.dictionary.as_deref(), cfg)?;
    println!(
        "{}",
        outputs
            .guesses_path
            .parent()
            .unwrap_or(&output_dir)
            .display()
    );

    Ok(())
}

fn default_output_dir() -> PathBuf {
    std::env::temp_dir().join("unredact")
}
