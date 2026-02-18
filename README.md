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
cargo run --bin unredact -- path/to/folder --output-dir path/to/output --recursive --glob *.pdf
```

Useful batch options:
- `--recursive`: include subfolders
- `--glob`: filename filter (`*` and `?` supported), default `*.pdf`
- `--jobs`: number of parallel workers
- `--fail-fast`: stop at the first failed file
- `--batch-manifest`: write a JSON summary report

The batch manifest includes per-file success/failure and runtime.

## Use Your Own Dictionary (Optional)
You can provide a custom dictionary file (one entry per line):

```bash
cargo run --bin unredact -- path/to/file.pdf --dictionary path/to/dictionary.txt
```

If you do not provide one, UnRedact uses the built-in names list.

## Most Useful Runtime Controls
- `--max-candidates`: number of guesses to keep per redaction
- `--max-dictionary`: maximum dictionary size loaded
- `--tol-pt`: width tolerance
- `--no-visual-score`: disable visual scoring
- `--visual-score-dpi`: render DPI for visual scoring
- `--visual-drop-threshold`: drop guesses above a visual error threshold

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
