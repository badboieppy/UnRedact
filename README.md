# UnRedact
UnRedact is a tool that analyzes redacted PDF files and makes best-effort guesses about hidden text.

It works by combining:
- redaction box detection,
- nearby visible text,
- PDF font/width measurements (including kerning and ligatures),
- visual scoring against the rendered page.

## Important Note
UnRedact generates guesses. It does not guarantee the true hidden text.

## Who This Is For
- Researchers and journalists
- Investigators reviewing released records
- Anyone who wants structured, repeatable analysis of redacted PDFs

## Install
1. Install Rust: https://rustup.rs
2. Clone this repository.
3. Build:

```bash
cargo build --release
```

## Quick Start (Single PDF)
Run UnRedact on one file:

```bash
cargo run --bin unredact -- path/to/file.pdf --output-dir path/to/output
```

This creates:
- `file.redactions.json` (detected redaction regions)
- `file.fonts.json` (detected text/font runs)
- `file.guesses.json` (best guesses, scores, and diagnostics)

## Create a Visualized PDF
If you want an output PDF with overlays:

```bash
cargo run --bin unredact -- path/to/file.pdf --output-dir path/to/output --visualize
```

This also creates:
- `file.visualized.pdf`

## Process a Whole Folder (Batch Mode)
You can pass a folder instead of a single file:

```bash
cargo run --bin unredact -- path/to/folder --output-dir path/to/output
```

Batch mode now:
- scans subfolders automatically,
- processes supported files (`.pdf`) only,
- runs serially,
- always writes a JSON batch manifest to `output_dir/batch_manifest.json`.

The batch manifest includes per-file success/failure and runtime.

## Use Your Own Dictionary (Optional)
You can provide a custom dictionary file (one entry per line):

```bash
cargo run --bin unredact -- path/to/file.pdf --dictionary path/to/dictionary.txt
```

If you do not provide one, UnRedact uses the built-in names list.

## Most Useful Runtime Controls
- `--no-image-analysis`: disable raster/image-based redaction detection and run text/shape analysis only
- `--should-visually-score true|false`: enable/disable visual scoring
- `--visualize`: write a visualized PDF with overlay guides

Show all `unredact` options:

```bash
cargo run --bin unredact -- --help
```

## Accuracy Benchmark
Use this benchmark to track quality and consistency over time:

```bash
cargo run --bin guess_accuracy_benchmark -- --out benchmark/guess_accuracy.json --repeats 2 --consistency-out benchmark/guess_consistency.json
```

This reports:
- recall and ranking metrics,
- visual error metrics,
- stage timing metrics,
- run-to-run consistency metrics.

Use `--determinism` as shorthand for `--repeats 3`.

Show benchmark options:

```bash
cargo run --bin guess_accuracy_benchmark -- --help
```

## PDF to PNG Helper
Use this to convert PDF pages into PNG images (useful for documentation and visual checks):

```bash
cargo run --bin pdf_to_png -- path/to/file.pdf --page 2 --dpi 200
```

Show options:

```bash
cargo run --bin pdf_to_png -- --help
```

## Example
Original sample (page 8):

![Original PDF preview](example/EFTA00101126.png)

Visualized sample (page 8):

![Visualized PDF preview](example/EFTA00101126.visualized.png)
