use clap::Parser;
use std::path::PathBuf;

use unredact::service::unredact_cli_entry::{
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

    #[arg(long)]
    no_image_analysis: bool,

    #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
    should_visually_score: bool,

    #[arg(long, default_value_t = false)]
    visualize: bool,
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

    let output_dir = args.output_dir.clone().unwrap_or_else(default_output_dir);

    let cfg = UnredactServiceConfig {
        enable_image_analysis: !args.no_image_analysis,
        guess: GuessConfig {
            visual_score: args.should_visually_score,
            ..GuessConfig::default()
        },
        visualize: args.visualize,
        visualizer: VisualizerConfig::default(),
        ..UnredactServiceConfig::default()
    };

    if args.input.is_dir() {
        let batch =
            run_batch_from_paths(&args.input, &output_dir, args.dictionary.as_deref(), cfg)?;
        println!(
            "processed={} success={} failed={} elapsed_ms={} manifest={}",
            batch.results.len(),
            batch.success_count,
            batch.failure_count,
            batch.elapsed_ms,
            batch.manifest_path.display()
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
