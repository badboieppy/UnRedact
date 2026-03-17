use clap::Parser;
use std::path::PathBuf;

use unredact::service::anchor_span_visual_benchmark_cli_entry::{
    run, AnchorSpanVisualBenchmarkRequest,
};

const DEFAULT_OUTPUT_DIR: &str = "analysis/anchor_span_visual_benchmark";

#[derive(Debug, Clone, Parser)]
#[command(
    name = "anchor_span_visual_benchmark",
    about = "Run benchmark-only visual span validation experiments against current test_data."
)]
struct CliOptions {
    #[arg(long = "output-dir", default_value = DEFAULT_OUTPUT_DIR)]
    output_dir: PathBuf,
    #[arg(long)]
    compact: bool,
}

fn main() -> Result<(), String> {
    let opts = CliOptions::parse();
    let outputs = run(AnchorSpanVisualBenchmarkRequest {
        output_dir: opts.output_dir,
        compact: opts.compact,
    })?;
    println!("{}", outputs.summary_path.display());
    Ok(())
}
