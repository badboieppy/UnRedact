mod font_detection;

use std::process::ExitCode;

fn main() -> ExitCode {
    font_detection::cli::entry::run()
}
