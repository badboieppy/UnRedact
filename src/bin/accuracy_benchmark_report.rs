use clap::Parser;
use std::path::PathBuf;

use unredact::benchmarks::service::accuracy_benchmark_report_cli_entry::{
    run, AccuracyBenchmarkReportCliRequest,
};

const DEFAULT_OUTPUT_DIR: &str = "analysis/accuracy_benchmark_report";

#[derive(Debug, Clone, Parser)]
#[command(
    name = "accuracy_benchmark_report",
    about = "Run the consolidated accuracy benchmark report workflow and emit stage + signal artifacts."
)]
struct CliOptions {
    #[arg(long = "output-dir", default_value = DEFAULT_OUTPUT_DIR)]
    output_dir: PathBuf,
    #[arg(long, default_value_t = 2_usize)]
    repeats: usize,
    #[arg(long)]
    compact: bool,
}

fn main() -> Result<(), String> {
    let opts = CliOptions::parse();
    let outputs = run(AccuracyBenchmarkReportCliRequest {
        output_dir: opts.output_dir,
        repeats: opts.repeats,
        compact: opts.compact,
    })?;
    println!("{}", outputs.summary_path.display());
    Ok(())
}
