use clap::Parser;
use std::path::PathBuf;

use unredact::service::visual_anchor_metrics_cli_entry::{run, VisualAnchorMetricServiceRequest};

#[derive(Debug, Parser)]
#[command(
    name = "visual_anchor_metric_extractor",
    about = "Collect visual anchor metrics and comparison crops for a PDF or directory"
)]
struct Args {
    input: PathBuf,

    #[arg(long)]
    output_dir: PathBuf,

    #[arg(long, default_value_t = false)]
    compact: bool,
}

fn main() -> std::process::ExitCode {
    match run_cli() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::from(2)
        }
    }
}

fn run_cli() -> Result<(), String> {
    let args = Args::parse();
    let outputs = run(VisualAnchorMetricServiceRequest {
        input: args.input,
        output_dir: args.output_dir,
        compact: args.compact,
    })?;
    for report_path in outputs.report_paths {
        println!("{}", report_path.display());
    }
    Ok(())
}
