use clap::Parser;
use std::path::PathBuf;

#[path = "../dependency/pdf_font_metric_map.rs"]
mod pdf_font_metric_map;

use pdf_font_metric_map::collect_pdf_metric_inventory;

#[derive(Debug, Parser)]
#[command(
    name = "pdf_metric_extractor",
    about = "Inventory PDF font and width-related metrics for a file or directory"
)]
struct Args {
    input: PathBuf,

    #[arg(long)]
    output: Option<PathBuf>,

    #[arg(long, default_value_t = false)]
    compact: bool,
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse();
    let report = collect_pdf_metric_inventory(&args.input)?;
    let encoded = if args.compact {
        serde_json::to_vec(&report)
    } else {
        serde_json::to_vec_pretty(&report)
    }
    .map_err(|error| format!("failed to encode report: {error}"))?;

    if let Some(output) = args.output {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create output directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        std::fs::write(&output, &encoded)
            .map_err(|error| format!("failed to write {}: {error}", output.display()))?;
        println!("{}", output.display());
    } else {
        let text = String::from_utf8(encoded)
            .map_err(|error| format!("failed to render json output: {error}"))?;
        println!("{text}");
    }

    Ok(())
}
