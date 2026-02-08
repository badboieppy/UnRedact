# UnRedact
Un-redact files

## Build/Lint Policy
- Builds run through `clippy` and fail on any warning.
- Ensure clippy is installed: `rustup component add clippy`

## Available CLIs

### Font Detection CLI
Use the default binary to analyze embedded fonts and emit JSON metadata:

- Scan one or more files: `cargo run -- detect <FILE>...`
- Include verbose metadata: add `--details`
- Write output to a file: `--output path/to/report.json`

### Redaction Finder CLI
The `redaction_cli` binary surfaces annotation, drawn, and raster redactions:

- Basic run: `cargo run --bin redaction_cli -- path/to/file.pdf`
- Emit extra per-redaction metadata: `--details`
- Limit detection scope: `--mode annotations|drawn|all`
- Include full-page rectangles: `--include-full-page-rects`
- Skip raster analysis if you only need vector checks: `--no-image-analysis`
- Set render resolution for raster detection: `--raster-dpi 200`
- Write JSON to disk instead of stdout: `--output path/to/report.json`

Raster detection in the CLI uses the pure-Rust `hayro` crate.

### Redaction Guesser CLI
The `redaction_guess_cli` binary generates standalone guess reports from precomputed JSON inputs:

- Basic run: `cargo run --bin redaction_guess_cli --redactions redactions.json --fonts fonts.json`
- Include a custom dictionary: `--dictionary words.txt`
- Control search size: `--max-words 4 --max-candidates 50 --max-dictionary 2000`
- Control tolerance: `--tol-pt 4.0`
- Cap search work: `--max-nodes 50000`
- Write JSON to disk instead of stdout: `--output path/to/guesses.json`

Example orchestration:

```bash
cargo run --bin redaction_cli -- path/to/file.pdf --output redactions.json --details
cargo run -- detect path/to/file.pdf --output fonts.json --details
cargo run --bin redaction_guess_cli --redactions redactions.json --fonts fonts.json --output guesses.json
```

### Orchestrator CLI
The `unredact_cli` binary runs redaction detection, font detection, and guessing in one pass:

- Basic run: `cargo run --bin unredact_cli -- path/to/file.pdf`
- Output directory: `--output-dir path/to/out` (defaults to the system temp dir under `unredact`)
- Include a custom dictionary: `--dictionary words.txt`
- Control redaction detection: `--details --include-full-page-rects --no-image-analysis --raster-dpi 200`
- Control guessing: `--max-words 4 --max-candidates 50 --max-dictionary 2000 --tol-pt 4.0 --max-nodes 50000`
