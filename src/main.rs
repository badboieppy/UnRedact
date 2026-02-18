use clap::Parser;
use std::path::PathBuf;

use unredact::service::unredact_entry::{
    run_batch_from_paths, run_from_paths, UnredactServiceConfig,
};
use unredact::types::guess_types::GuessConfig;
use unredact::types::visualizer_config::VisualizerConfig;

#[derive(Debug, Parser)]
#[command(
    name = "unredact",
    about = "Run redaction detection, font detection, and guessing in one pass"
)]
struct Args {
    input: PathBuf,

    #[arg(long)]
    output_dir: Option<PathBuf>,

    #[arg(long)]
    dictionary: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    recursive: bool,

    #[arg(long, default_value = "*.pdf")]
    glob: String,

    #[arg(long)]
    jobs: Option<usize>,

    #[arg(long, default_value_t = false)]
    fail_fast: bool,

    #[arg(long)]
    batch_manifest: Option<PathBuf>,

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
    no_visual_score: bool,

    #[arg(long, default_value_t = 200.0_f32)]
    visual_score_dpi: f32,

    #[arg(long, default_value_t = 64_u32)]
    visual_min_ink_pixels: u32,

    #[arg(long)]
    visual_drop_threshold: Option<f32>,

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
    if args.glob.trim().is_empty() {
        return Err("glob must not be empty".to_owned());
    }
    if let Some(jobs) = args.jobs {
        if jobs == 0 {
            return Err("jobs must be > 0".to_owned());
        }
    }
    if !args.visual_score_dpi.is_finite() || args.visual_score_dpi <= 0.0_f32 {
        return Err(format!(
            "visual_score_dpi must be finite and > 0, got {}",
            args.visual_score_dpi
        ));
    }
    if args.visual_min_ink_pixels == 0 {
        return Err("visual_min_ink_pixels must be > 0".to_owned());
    }
    if let Some(threshold) = args.visual_drop_threshold {
        if !threshold.is_finite() || threshold < 0.0_f32 {
            return Err(format!(
                "visual_drop_threshold must be finite and >= 0, got {threshold}"
            ));
        }
    }
    if !args.visualize_border.is_finite() || args.visualize_border <= 0.0 {
        return Err("visualize_border must be finite and > 0".to_owned());
    }

    let output_dir = args.output_dir.clone().unwrap_or_else(default_output_dir);
    let jobs = args.jobs.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
    });

    let cfg = UnredactServiceConfig {
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
            visual_score: !args.no_visual_score,
            visual_score_dpi: args.visual_score_dpi,
            visual_min_ink_pixels: args.visual_min_ink_pixels,
            visual_drop_threshold: args.visual_drop_threshold,
        },
        visualize: args.visualize,
        visualizer: VisualizerConfig {
            color: [1.0, 0.0, 0.0],
            text_color: [0.0, 0.4, 1.0],
            border_width: args.visualize_border,
        },
    };

    if args.input.is_dir() {
        let default_manifest = output_dir.join("batch_manifest.json");
        let manifest = args.batch_manifest.as_deref().or(Some(&default_manifest));
        let batch = run_batch_from_paths(
            &args.input,
            &output_dir,
            args.dictionary.as_deref(),
            cfg,
            args.recursive,
            args.glob.trim(),
            jobs,
            args.fail_fast,
            manifest,
        )?;
        println!(
            "processed={} success={} failed={} elapsed_ms={} manifest={}",
            batch.results.len(),
            batch.success_count,
            batch.failure_count,
            batch.elapsed_ms,
            batch
                .manifest_path
                .as_deref()
                .map(|value| value.display().to_string())
                .unwrap_or_else(|| "-".to_owned())
        );
    } else {
        let outputs = run_from_paths(&args.input, &output_dir, args.dictionary.as_deref(), cfg)?;
        println!(
            "{}",
            outputs
                .guesses_path
                .parent()
                .unwrap_or(&output_dir)
                .display()
        );
    }

    Ok(())
}

fn default_output_dir() -> PathBuf {
    std::env::temp_dir().join("unredact")
}
