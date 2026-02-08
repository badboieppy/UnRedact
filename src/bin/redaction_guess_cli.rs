use clap::Parser;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use unredact::redaction_guess::service::run_from_paths;
use unredact::redaction_guess::types::GuessConfig;

#[derive(Debug, Parser)]
#[command(
    name = "redaction_guess_cli",
    about = "Generate standalone guesses for redacted text using precomputed JSON reports"
)]
struct Args {
    #[arg(long)]
    redactions: PathBuf,

    #[arg(long)]
    fonts: PathBuf,

    #[arg(long)]
    pdf: PathBuf,

    #[arg(long)]
    dictionary: Option<PathBuf>,

    #[arg(long)]
    output: Option<PathBuf>,

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

    let cfg = GuessConfig {
        max_words: args.max_words,
        max_candidates: args.max_candidates,
        max_dictionary: args.max_dictionary,
        tol_pt: args.tol_pt,
        max_nodes: args.max_nodes,
    };

    let report = run_from_paths(
        &args.redactions,
        &args.fonts,
        &args.pdf,
        args.dictionary.as_deref(),
        cfg,
    )?;

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
