# Problem 4 Plan: Batch Processing For Directory Inputs

## Audience
New engineers implementing multi-file orchestration and batch-level reporting.

## Date
2026-02-17

## Problem Statement
There is no batch mode today. Users want to provide one directory containing many PDFs and process all files in one command.

## Current System Behavior
### Current CLI and service scope
- `src/main.rs` accepts a single `input` path intended for one PDF.
- `src/service/unredact_entry.rs` exposes single-file `run_from_paths`.
- Output filenames are derived from single input stem.

### Current limitation impact
- Users must script repeated invocations manually.
- No aggregate success or failure summary exists.
- No uniform batching diagnostics or throughput metrics.

## Expected Behavior After Fix
- CLI accepts directory input and processes all matching PDF files.
- Output layout is deterministic and collision-safe.
- Failures are isolated per file; one bad file does not abort entire batch (unless strict mode requested).
- Batch summary report is generated with per-file outcomes and timing.

## Feasibility
High.
- Single-file pipeline is already stable.
- Batch support is mostly orchestration, file discovery, and reporting.
- Existing service interface can be extended without breaking current callers.

## Decisions Made
1. Keep single-file mode unchanged and fully backward compatible.
2. Add batch mode to the same `unredact` binary, selected automatically when input path is directory.
3. Process files in deterministic sorted order.
4. Include both serial and parallel execution modes.
5. Emit a batch manifest JSON for auditability.

## Design
### CLI changes
Add flags:
- `--recursive` (default false)
- `--glob <pattern>` (default `*.pdf`)
- `--jobs <N>` (default CPU count, `1` for serial)
- `--batch-manifest <path>` optional output manifest path
- `--fail-fast` stop batch on first file error

Input behavior:
- if `input` is file -> existing behavior,
- if `input` is directory -> batch mode.

### Service API additions
Add new batch APIs:
- `run_batch_from_paths(req: UnredactBatchRequest) -> Result<UnredactBatchOutputs, String>`

New types:
- `UnredactBatchRequest`
  - input directory
  - output directory
  - dictionary path (global for now)
  - config
  - recursion and glob options
  - jobs
  - fail-fast flag
- `UnredactBatchFileResult`
  - input path
  - status (`ok` or `error`)
  - output paths when success
  - error message when failure
  - elapsed time
- `UnredactBatchOutputs`
  - results list
  - aggregate counts
  - total elapsed
  - manifest path

### Discovery and ordering
1. Enumerate candidate files based on input directory + recursion + glob.
2. Normalize paths and filter to `.pdf`.
3. Sort lexicographically to guarantee deterministic order.

### Output layout
Use per-file subdirectory to avoid name collisions:
- `<output_dir>/<relative_input_path_without_extension>/`

Example:
- input: `batch/A/EFTA00101126.pdf`
- output dir: `out/`
- files:
  - `out/A/EFTA00101126/EFTA00101126.redactions.json`
  - `out/A/EFTA00101126/EFTA00101126.fonts.json`
  - `out/A/EFTA00101126/EFTA00101126.guesses.json`
  - optional visualized PDF

### Parallel execution
- Use bounded worker pool (`jobs`).
- Each file task runs existing single-file pipeline.
- Collect results and sort by input path before writing manifest.
- Ensure deterministic final output ordering independent of worker scheduling.

### Error handling model
- Non-fail-fast mode:
  - continue on per-file failures,
  - return success with mixed result statuses,
  - process exit code configurable (recommended non-zero if any file failed).
- Fail-fast mode:
  - stop on first failure and return error immediately.

## Data To Collect For Better Future Batch Accuracy And Ops
Per file:
- page count,
- redaction count,
- guess row count,
- timing per major stage,
- error category if failed.

Per batch:
- throughput files per minute,
- success ratio,
- median and p95 file runtime.

## Testing And Benchmark Updates
### Unit tests
- discovery function:
  - recursive and non-recursive behavior,
  - glob filtering,
  - deterministic sorting.
- output path builder:
  - collision-safe relative layout.

### Integration tests
- create temp directory with multiple test PDFs and one invalid file.
- verify:
  - all valid files processed,
  - invalid file recorded as error,
  - manifest contains full status set.
- verify `--fail-fast` stops early.

### Benchmark updates
- add optional batch benchmark mode:
  - process predefined set in `test_data/`,
  - report throughput and per-file latency distribution.

## Detailed TODO List
### Phase 0: API scaffolding
- [ ] Define batch request and response structs in service layer.
- [ ] Add batch result status enum and serialization.
- [ ] Add manifest schema definitions.

### Phase 1: File discovery
- [ ] Implement directory scan helper.
- [ ] Add recursion option.
- [ ] Add glob filtering.
- [ ] Normalize and sort discovered paths.
- [ ] Add tests for discovery behavior.

### Phase 2: Batch orchestrator
- [ ] Implement serial batch execution path.
- [ ] Add per-file timer and result capture.
- [ ] Add fail-fast behavior option.
- [ ] Add summary aggregation.

### Phase 3: Parallel execution
- [ ] Add bounded worker pool implementation.
- [ ] Ensure deterministic result ordering post-collection.
- [ ] Add tests for deterministic ordering with `jobs > 1`.

### Phase 4: CLI integration
- [ ] Add new CLI flags.
- [ ] Auto-switch to batch mode when input is directory.
- [ ] Print concise batch summary to stdout.
- [ ] Add optional manifest output path handling.

### Phase 5: Manifest and reporting
- [ ] Write batch manifest JSON after run.
- [ ] Include per-file output paths and errors.
- [ ] Include aggregate stats and elapsed totals.

### Phase 6: Validation and docs
- [ ] Add integration tests for mixed success runs.
- [ ] Add integration tests for fail-fast mode.
- [ ] Document batch usage in README with examples.
- [ ] Add troubleshooting section for partial failures.

## Definition Of Done
- Directory input is fully supported in CLI and service APIs.
- Batch manifest is generated with deterministic ordering and full per-file status.
- Serial and parallel modes are both supported and tested.
- Single-file behavior remains backward compatible.
