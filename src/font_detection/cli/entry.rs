use crate::font_detection::dependency::file_accessor::OsFileAccessor;
use crate::font_detection::logic::types::file_types::{FontProcessInput, OutputFormat};
use crate::font_detection::service::entry::run_font_detection;
use clap::{Parser, ValueEnum};
use std::io::{self, Write as _};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Clone, Parser)]
#[command(name = "unredact")]
#[command(about = "Font detection utilities", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum Command {
    Detect(DetectArgs),
}

#[derive(Debug, Clone, Parser)]
pub struct DetectArgs {
    #[arg(value_name = "FILE", required = true)]
    pub inputs: Vec<PathBuf>,

    #[arg(long, value_enum, default_value_t = CliOutputFormat::Json)]
    pub format: CliOutputFormat,

    #[arg(long, default_value_t = false)]
    pub details: bool,

    #[arg(long)]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CliOutputFormat {
    Json,
}

#[inline]
pub fn run() -> ExitCode {
    let cli = Cli::parse();
    let result = dispatch(cli);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            write_error_line(&message);
            ExitCode::from(2)
        }
    }
}

fn write_error_line(message: &str) {
    let _write_result = writeln!(io::stderr().lock(), "{message}");
}

fn dispatch(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Detect(args) => handle_detect(args),
    }
}

fn handle_detect(args: DetectArgs) -> Result<(), String> {
    let input = map_args(args)?;
    let accessor = OsFileAccessor::new();
    let encoded = run_font_detection(&accessor, input)?;
    write_output(&encoded.path, encoded.bytes)
}

fn map_args(args: DetectArgs) -> Result<FontProcessInput, String> {
    let format = map_format(args.format);
    Ok(FontProcessInput {
        inputs: args.inputs,
        output: args.output,
        format,
        include_details: args.details,
    })
}

fn map_format(fmt: CliOutputFormat) -> OutputFormat {
    match fmt {
        CliOutputFormat::Json => OutputFormat::Json,
    }
}

fn write_output(path: &Option<PathBuf>, bytes: Vec<u8>) -> Result<(), String> {
    match path.as_ref() {
        None => write_to_stdout(bytes),
        Some(path_buf) => write_to_file(path_buf, bytes),
    }
}

fn write_to_stdout(bytes: Vec<u8>) -> Result<(), String> {
    use std::io::Write as _;

    let mut out = std::io::stdout().lock();
    out.write_all(&bytes).map_err(|error| error.to_string())?;
    out.flush().map_err(|error| error.to_string())?;
    Ok(())
}

fn write_to_file(path: &PathBuf, bytes: Vec<u8>) -> Result<(), String> {
    use std::io::Write as _;

    let mut file = std::fs::File::create(path).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_detection::logic::types::file_types::OutputFormat;

    #[test]
    fn cli_output_format_is_value_enum_and_eq() {
        let a = CliOutputFormat::Json;
        let b = CliOutputFormat::Json;
        assert_eq!(a, b);
    }

    #[test]
    fn map_format_json() {
        let out = map_format(CliOutputFormat::Json);
        assert_eq!(out, OutputFormat::Json);
    }

    #[test]
    fn map_args_copies_fields() {
        let args = DetectArgs {
            inputs: vec![PathBuf::from("a.pdf"), PathBuf::from("b.png")],
            format: CliOutputFormat::Json,
            details: true,
            output: Some(PathBuf::from("out.json")),
        };

        let out = map_args(args).expect("expected value in test");

        assert_eq!(
            out.inputs,
            vec![PathBuf::from("a.pdf"), PathBuf::from("b.png")]
        );
        assert_eq!(out.output, Some(PathBuf::from("out.json")));
        assert_eq!(out.format, OutputFormat::Json);
        assert!(out.include_details);
    }

    #[test]
    fn write_output_stdout_branch() {
        let out = write_output(&None, b"{}\n".to_vec());
        out.expect("expected value in test");
    }

    #[test]
    fn write_output_file_branch_creates_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("unredact_font_detection_cli_entry_test_out.json");
        let _ignored_remove_result = std::fs::remove_file(&path);

        let out = write_output(
            &Some(path.clone()),
            b"{\"ok\":true}\n".to_vec(),
        );
        out.expect("expected value in test");

        let bytes = std::fs::read(&path).expect("expected value in test");
        assert_eq!(bytes, b"{\"ok\":true}\n".to_vec());

        let _ignored_remove_result = std::fs::remove_file(&path);
    }
}
