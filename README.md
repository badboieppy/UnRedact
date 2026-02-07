# UnRedact
Un-redact files

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
