# Flags Inventory

## Compile-Time Flags

### Cargo Features

| Feature | Source | Line |
|---|---|---:|
| cli-entry | Cargo.toml | 23 |
| default | Cargo.toml | 15 |
| local-file-workflow | Cargo.toml | 22 |
| shared-bytes-workflow | Cargo.toml | 21 |
| web-entry | Cargo.toml | 24 |

### cfg Mentions Across Rust Files

| File | Line | Snippet |
|---|---:|---|
| src/bin/pdf_to_png.rs | 161 | #[cfg(test)] |
| src/data/dictionary_data.rs | 130 | #[cfg(test)] |
| src/data/fonts_data.rs | 78 | #[cfg(test)] |
| src/data/mod.rs | 4 | #[cfg(feature = "local-file-workflow")] |
| src/data/mod.rs | 6 | #[cfg(feature = "local-file-workflow")] |
| src/data/mod.rs | 9 | #[cfg(feature = "local-file-workflow")] |
| src/data/mod.rs | 16 | #[cfg(feature = "local-file-workflow")] |
| src/data/mod.rs | 18 | #[cfg(feature = "local-file-workflow")] |
| src/data/mod.rs | 20 | #[cfg(feature = "local-file-workflow")] |
| src/data/mod.rs | 22 | #[cfg(feature = "local-file-workflow")] |
| src/data/visualization_data.rs | 782 | #[cfg(test)] |
| src/dependency/pdf_font_occurrence_accessor.rs | 665 | #[cfg(test)] |
| src/dependency/pdf_font_run_accessor.rs | 1105 | #[cfg(test)] |
| src/dependency/pdf_redaction_accessor.rs | 1939 | #[cfg(test)] |
| src/logic/dictionary_list_convertion_component.rs | 41 | #[cfg(test)] |
| src/logic/local_file_workflow_component.rs | 173 | #[cfg(test)] |
| src/logic/mod.rs | 3 | #[cfg(feature = "local-file-workflow")] |
| src/logic/mod.rs | 14 | #[cfg(feature = "local-file-workflow")] |
| src/logic/redaction_guessing_component.rs | 3689 | #[cfg(test)] |
| src/logic/redaction_guessing_component.rs | 4346 | #[cfg(test)] |
| src/logic/redaction_guessing_component.rs | 5873 | #[cfg(test)] |
| src/logic/time.rs | 1 | #[cfg(target_family = "wasm")] |
| src/logic/time.rs | 7 | #[cfg(target_family = "wasm")] |
| src/logic/time.rs | 23 | #[cfg(not(target_family = "wasm"))] |
| src/service/mod.rs | 1 | #[cfg(feature = "cli-entry")] |
| src/service/mod.rs | 3 | #[cfg(all(feature = "web-entry", target_family = "wasm"))] |
| src/service/mod.rs | 5 | #[cfg(feature = "web-entry")] |
| src/service/unredact_cli_entry.rs | 283 | #[cfg(test)] |
| src/types/file_types.rs | 178 | #[cfg(test)] |
| tests/shared_workflow_purity.rs | 101 | if trimmed.starts_with("#[cfg(test)]") { |
| tests/web_ui_batch_benchmark.rs | 11 | let npm = if cfg!(target_os = "windows") { |

## Runtime Flags

### Clap-Defined Flags (from #[arg(...)])

| Flag | Field | File | Line | Attribute |
|---|---|---|---:|---|
| --all-pages | all_pages | src/bin/pdf_to_png.rs | 20 | #[arg(long, default_value_t = false)] |
| --dictionary | dictionary | src/main.rs | 21 | #[arg(long)] |
| --dictionary-size | dictionary_size | src/bin/visual_score_impact_benchmark.rs | 49 | #[arg( long, default_value_t = 8_000_usize, help = "Maximum dictionary size (after normalization)." )] |
| --dpi | dpi | src/bin/pdf_to_png.rs | 31 | #[arg(long, default_value_t = 200.0_f32)] |
| --input | input | src/bin/visual_score_impact_benchmark.rs | 25 | #[arg( long, default_value = "test_data/EFTA00038617.pdf", help = "Path to source PDF used for randomized synthetic redactions." )] |
| --no-image-analysis | no_image_analysis | src/main.rs | 24 | #[arg(long)] |
| --out | out | src/bin/visual_score_impact_benchmark.rs | 61 | #[arg( long, default_value = "benchmark/visual_score_impact.json", help = "Output path for JSON results." )] |
| --output | output | src/bin/pdf_to_png.rs | 24 | #[arg(long)] |
| --output-dir | output_dir | src/bin/pdf_to_png.rs | 28 | #[arg(long)] |
| --output-dir | output_dir | src/main.rs | 18 | #[arg(long)] |
| --page | page | src/bin/pdf_to_png.rs | 16 | #[arg(long, default_value_t = 1)] |
| --page | page | src/bin/visual_score_impact_benchmark.rs | 31 | #[arg( long, default_value_t = 2_u32, help = "1-based page number to sample targets from." )] |
| --seed | seed | src/bin/visual_score_impact_benchmark.rs | 55 | #[arg( long, default_value_t = 0xD1CE_BA5E_u64, help = "Seed for deterministic random target selection." )] |
| --should-visually-score | should_visually_score | src/main.rs | 27 | #[arg(long, action = clap::ArgAction::Set, default_value_t = true)] |
| --targets-per-trial | targets_per_trial | src/bin/visual_score_impact_benchmark.rs | 43 | #[arg( long, default_value_t = 10_usize, help = "Number of random targets per trial." )] |
| --trials | trials | src/bin/visual_score_impact_benchmark.rs | 37 | #[arg( long, default_value_t = 10_usize, help = "Number of randomized trials to run." )] |
| --visualize | visualize | src/main.rs | 30 | #[arg(long, default_value_t = false)] |

### CLI/Script Flags (--...)

| Flag | File | Line | Snippet |
|---|---|---:|---|
| --all-pages | src/bin/pdf_to_png.rs | 15 | /// Render this 1-based page number when --all-pages is not set. |
| --all-pages | src/bin/pdf_to_png.rs | 55 | return Err("--output cannot be used with --all-pages".to_owned()); |
| --bin | src/bin/guess_accuracy_benchmark.rs | 329 | "Usage: cargo run --bin guess_accuracy_benchmark -- [--out <path>] [--repeats <n>] [--single-run] [--determinism] [--require-deterministic] [--consistency-out <path>]" |
| --consistency-out | src/bin/guess_accuracy_benchmark.rs | 321 | "--consistency-out" => { |
| --consistency-out | src/bin/guess_accuracy_benchmark.rs | 323 | return Err("missing value for --consistency-out".to_owned()); |
| --consistency-out | src/bin/guess_accuracy_benchmark.rs | 329 | "Usage: cargo run --bin guess_accuracy_benchmark -- [--out <path>] [--repeats <n>] [--single-run] [--determinism] [--require-deterministic] [--consistency-out <path>]" |
| --determinism | src/bin/guess_accuracy_benchmark.rs | 318 | "--determinism" => repeats = 3_usize, |
| --determinism | src/bin/guess_accuracy_benchmark.rs | 329 | "Usage: cargo run --bin guess_accuracy_benchmark -- [--out <path>] [--repeats <n>] [--single-run] [--determinism] [--require-deterministic] [--consistency-out <path>]" |
| --dpi | src/bin/pdf_to_png.rs | 49 | return Err(format!("invalid --dpi value: {}", args.dpi)); |
| --help | src/bin/guess_accuracy_benchmark.rs | 327 | "--help" \\| "-h" => { |
| --out | src/bin/guess_accuracy_benchmark.rs | 301 | "--out" => { |
| --out | src/bin/guess_accuracy_benchmark.rs | 303 | return Err("missing value for --out".to_owned()); |
| --out | src/bin/guess_accuracy_benchmark.rs | 329 | "Usage: cargo run --bin guess_accuracy_benchmark -- [--out <path>] [--repeats <n>] [--single-run] [--determinism] [--require-deterministic] [--consistency-out <path>]" |
| --output | src/bin/pdf_to_png.rs | 55 | return Err("--output cannot be used with --all-pages".to_owned()); |
| --page | src/bin/pdf_to_png.rs | 52 | return Err("--page must be >= 1".to_owned()); |
| --repeats | src/bin/guess_accuracy_benchmark.rs | 307 | "--repeats" => { |
| --repeats | src/bin/guess_accuracy_benchmark.rs | 309 | return Err("missing value for --repeats".to_owned()); |
| --repeats | src/bin/guess_accuracy_benchmark.rs | 313 | .map_err(\\|error\\| format!("invalid --repeats value '{value}': {error}"))?; |
| --repeats | src/bin/guess_accuracy_benchmark.rs | 315 | return Err("--repeats must be > 0".to_owned()); |
| --repeats | src/bin/guess_accuracy_benchmark.rs | 329 | "Usage: cargo run --bin guess_accuracy_benchmark -- [--out <path>] [--repeats <n>] [--single-run] [--determinism] [--require-deterministic] [--consistency-out <path>]" |
| --require-deterministic | src/bin/guess_accuracy_benchmark.rs | 320 | "--require-deterministic" => require_deterministic = true, |
| --require-deterministic | src/bin/guess_accuracy_benchmark.rs | 329 | "Usage: cargo run --bin guess_accuracy_benchmark -- [--out <path>] [--repeats <n>] [--single-run] [--determinism] [--require-deterministic] [--consistency-out <path>]" |
| --single-run | src/bin/guess_accuracy_benchmark.rs | 319 | "--single-run" => repeats = 1_usize, |
| --single-run | src/bin/guess_accuracy_benchmark.rs | 329 | "Usage: cargo run --bin guess_accuracy_benchmark -- [--out <path>] [--repeats <n>] [--single-run] [--determinism] [--require-deterministic] [--consistency-out <path>]" |

### Environment Variables

| Env Var | File | Line | Snippet |
|---|---|---:|---|
| UNREDACT_RUN_WEB_UI_BENCHMARK | tests/web_ui_batch_benchmark.rs | 3 | let should_run = std::env::var("UNREDACT_RUN_WEB_UI_BENCHMARK") |
| UNREDACT_RUN_WEB_UI_BENCHMARK | tests/web_ui_batch_benchmark.rs | 7 | eprintln!("skipping web ui benchmark test; set UNREDACT_RUN_WEB_UI_BENCHMARK=1 to run it"); |
| UNREDACT_RUN_WEB_UI_BENCHMARK | web/pkg/README.md | 172 | UNREDACT_RUN_WEB_UI_BENCHMARK=1 cargo test --test web_ui_batch_benchmark -- --nocapture |
| UNREDACT_RUN_WEB_UI_BENCHMARK | web/pkg/README.md | 175 | Without `UNREDACT_RUN_WEB_UI_BENCHMARK=1`, that cargo test auto-skips. |

