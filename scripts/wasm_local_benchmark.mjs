#!/usr/bin/env node

import { performance } from "node:perf_hooks";
import crypto from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { TextDecoder } from "node:util";

import init, { run_unredact_web } from "../web/pkg/unredact.js";

const DEFAULT_OUTPUT = "benchmark/wasm_local_benchmark.json";
const DEFAULT_REPEATS = 3;
const DEFAULT_WARMUP = 1;
const DEFAULT_RASTER_DPI = 200.0;
function printUsage() {
  const lines = [
    "Usage: node scripts/wasm_local_benchmark.mjs [options]",
    "",
    "Options:",
    "  --pdf <path>                    Benchmark a single PDF (repeatable).",
    "  --pdf-dir <path>                Benchmark all PDFs in a directory recursively.",
    "  --dictionary <path>             Optional dictionary text file (one entry per line).",
    "  --repeats <n>                   Number of measured runs per PDF (default: 3).",
    "  --warmup <n>                    Number of warmup runs per PDF (default: 1).",
    "  --enable-image-analysis <bool>  Enable raster/image analysis (default: true).",
    "  --visualize <bool>              Render visualized PDF output (default: false).",
    "  --raster-dpi <float>            Raster DPI (default: 200).",
    "  --out <path>                    Output report JSON path (default: benchmark/wasm_local_benchmark.json).",
    "  --help                          Show this help.",
    "",
    "Examples:",
    "  node scripts/wasm_local_benchmark.mjs --pdf test_data/EFTA00101126.pdf --repeats 5",
    "  node scripts/wasm_local_benchmark.mjs --pdf-dir test_data --out benchmark/wasm_batch.json",
  ];
  console.log(lines.join("\n"));
}

function parseBool(raw, name) {
  const normalized = String(raw).trim().toLowerCase();
  if (normalized === "true" || normalized === "1" || normalized === "yes") {
    return true;
  }
  if (normalized === "false" || normalized === "0" || normalized === "no") {
    return false;
  }
  throw new Error(`invalid boolean for ${name}: ${raw}`);
}

function parsePositiveInt(raw, name) {
  const value = Number.parseInt(String(raw), 10);
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error(`invalid positive integer for ${name}: ${raw}`);
  }
  return value;
}

function parseNonNegativeInt(raw, name) {
  const value = Number.parseInt(String(raw), 10);
  if (!Number.isFinite(value) || value < 0) {
    throw new Error(`invalid non-negative integer for ${name}: ${raw}`);
  }
  return value;
}

function parsePositiveFloat(raw, name) {
  const value = Number.parseFloat(String(raw));
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error(`invalid positive number for ${name}: ${raw}`);
  }
  return value;
}

function parseArgs(argv) {
  const options = {
    pdfs: [],
    pdfDir: null,
    dictionary: null,
    repeats: DEFAULT_REPEATS,
    warmup: DEFAULT_WARMUP,
    enableImageAnalysis: true,
    visualize: false,
    rasterDpi: DEFAULT_RASTER_DPI,
    outPath: DEFAULT_OUTPUT,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = () => {
      if (i + 1 >= argv.length) {
        throw new Error(`missing value for ${arg}`);
      }
      i += 1;
      return argv[i];
    };

    if (arg === "--help" || arg === "-h") {
      printUsage();
      process.exit(0);
    } else if (arg === "--pdf") {
      options.pdfs.push(next());
    } else if (arg === "--pdf-dir") {
      options.pdfDir = next();
    } else if (arg === "--dictionary") {
      options.dictionary = next();
    } else if (arg === "--repeats") {
      options.repeats = parsePositiveInt(next(), "--repeats");
    } else if (arg === "--warmup") {
      options.warmup = parseNonNegativeInt(next(), "--warmup");
    } else if (arg === "--enable-image-analysis") {
      options.enableImageAnalysis = parseBool(
        next(),
        "--enable-image-analysis",
      );
    } else if (arg === "--visualize") {
      options.visualize = parseBool(next(), "--visualize");
    } else if (arg === "--raster-dpi") {
      options.rasterDpi = parsePositiveFloat(next(), "--raster-dpi");
    } else if (arg === "--out") {
      options.outPath = next();
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }

  return options;
}

async function collectPdfPaths(rootDir, options) {
  const seen = new Set();
  const out = [];

  async function addIfPdf(filePath) {
    const resolved = path.resolve(rootDir, filePath);
    const stat = await fs.stat(resolved);
    if (!stat.isFile()) {
      throw new Error(`not a file: ${resolved}`);
    }
    if (!resolved.toLowerCase().endsWith(".pdf")) {
      return;
    }
    if (seen.has(resolved)) {
      return;
    }
    seen.add(resolved);
    out.push(resolved);
  }

  async function walkDirectory(dirPath) {
    const entries = await fs.readdir(dirPath, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = path.join(dirPath, entry.name);
      if (entry.isDirectory()) {
        await walkDirectory(fullPath);
      } else if (entry.isFile() && fullPath.toLowerCase().endsWith(".pdf")) {
        if (!seen.has(fullPath)) {
          seen.add(fullPath);
          out.push(fullPath);
        }
      }
    }
  }

  for (const pdfPath of options.pdfs) {
    await addIfPdf(pdfPath);
  }

  if (options.pdfDir) {
    const resolvedDir = path.resolve(rootDir, options.pdfDir);
    const stat = await fs.stat(resolvedDir);
    if (!stat.isDirectory()) {
      throw new Error(`--pdf-dir is not a directory: ${resolvedDir}`);
    }
    await walkDirectory(resolvedDir);
  }

  if (out.length === 0) {
    const defaultPdf = path.resolve(rootDir, "test_data/EFTA00101126.pdf");
    const fallbackExists = await fs
      .stat(defaultPdf)
      .then((value) => value.isFile())
      .catch(() => false);
    if (!fallbackExists) {
      throw new Error(
        "no PDFs selected and fallback test_data/EFTA00101126.pdf is missing",
      );
    }
    out.push(defaultPdf);
  }

  out.sort();
  return out;
}

function asUint8Array(value) {
  if (value == null) {
    return null;
  }
  if (value instanceof Uint8Array) {
    return value;
  }
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  if (Array.isArray(value)) {
    return Uint8Array.from(value);
  }
  throw new Error(`unsupported byte payload shape: ${typeof value}`);
}

function decodeJsonBytes(bytes) {
  const decoder = new TextDecoder();
  return JSON.parse(decoder.decode(bytes));
}

function quantile(sorted, q) {
  if (sorted.length === 0) {
    return 0;
  }
  if (sorted.length === 1) {
    return sorted[0];
  }
  const position = (sorted.length - 1) * q;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  if (lower === upper) {
    return sorted[lower];
  }
  const weight = position - lower;
  return sorted[lower] * (1 - weight) + sorted[upper] * weight;
}

function summarizeSeries(values) {
  if (values.length === 0) {
    return {
      count: 0,
      min: 0,
      max: 0,
      mean: 0,
      p50: 0,
      p90: 0,
    };
  }
  const sorted = [...values].sort((a, b) => a - b);
  const sum = sorted.reduce((acc, value) => acc + value, 0);
  return {
    count: sorted.length,
    min: sorted[0],
    max: sorted[sorted.length - 1],
    mean: sum / sorted.length,
    p50: quantile(sorted, 0.5),
    p90: quantile(sorted, 0.9),
  };
}

function parseStageTimings(stageTimings) {
  const stageMap = new Map();
  for (const item of stageTimings ?? []) {
    if (
      !item ||
      typeof item.stage !== "string" ||
      !Number.isFinite(Number(item.value_ms))
    ) {
      continue;
    }
    stageMap.set(item.stage, Number(item.value_ms));
  }
  return stageMap;
}

function stableTopGuessSignature(report) {
  const rows = (report?.guesses ?? []).map((guess) => {
    const topCandidate =
      Array.isArray(guess.candidates) && guess.candidates.length > 0
        ? String(guess.candidates[0]?.text ?? "")
        : "";
    const topGuess = topCandidate;
    const bbox = guess?.bbox ?? {};
    return [
      String(guess?.page_index ?? ""),
      Number(bbox.x0 ?? 0).toFixed(3),
      Number(bbox.y0 ?? 0).toFixed(3),
      Number(bbox.x1 ?? 0).toFixed(3),
      Number(bbox.y1 ?? 0).toFixed(3),
      topGuess,
    ].join("|");
  });
  return crypto.createHash("sha256").update(rows.join("\n")).digest("hex");
}

function topSignatureSummary(signatureHashes) {
  const counts = new Map();
  for (const hash of signatureHashes) {
    counts.set(hash, (counts.get(hash) ?? 0) + 1);
  }
  const items = Array.from(counts.entries())
    .map(([hash, count]) => ({ hash, count }))
    .sort((a, b) => b.count - a.count || a.hash.localeCompare(b.hash));
  const dominant = items[0] ?? { hash: "", count: 0 };
  const total = signatureHashes.length;
  const ratio = total === 0 ? 0 : dominant.count / total;
  return {
    unique_signatures: items.length,
    dominant_signature: dominant.hash,
    dominant_signature_count: dominant.count,
    dominant_signature_ratio: ratio,
    signature_counts: items,
  };
}

function stageSummaryFromRuns(stageMaps) {
  const perStage = new Map();
  for (const map of stageMaps) {
    for (const [stage, value] of map.entries()) {
      if (!perStage.has(stage)) {
        perStage.set(stage, []);
      }
      perStage.get(stage).push(value);
    }
  }
  const summary = {};
  for (const [stage, values] of perStage.entries()) {
    summary[stage] = summarizeSeries(values);
  }
  return summary;
}

async function loadDictionary(rootDir, dictionaryPath) {
  if (!dictionaryPath) {
    return {
      bytes: null,
      source: "built_in_fallback",
      entry_count: null,
      byte_length: 0,
    };
  }

  const resolved = path.resolve(rootDir, dictionaryPath);
  const fileBytes = await fs.readFile(resolved);
  const lines = fileBytes
    .toString("utf8")
    .split(/\r?\n/)
    .map((value) => value.trim())
    .filter((value) => value.length > 0);

  return {
    bytes: new Uint8Array(fileBytes),
    source: resolved,
    entry_count: lines.length,
    byte_length: fileBytes.length,
  };
}

function buildRequest(pdfPath, pdfBytes, dictionaryBytes, options) {
  return {
    input_name: path.basename(pdfPath),
    pdf_bytes: pdfBytes,
    dictionary_file_bytes: dictionaryBytes,
    cfg: {
      include_details: false,
      enable_image_analysis: options.enableImageAnalysis,
      raster_dpi: options.rasterDpi,
      guess: {},
      visualize: options.visualize,
      visualizer: {
        color: [1.0, 0.0, 0.0],
        text_color: [0.0, 0.4, 1.0],
        border_width: 1.0,
      },
    },
  };
}

async function runCase(pdfPath, dictionaryBytes, options, rootDir) {
  const pdfBytes = new Uint8Array(await fs.readFile(pdfPath));
  const request = buildRequest(pdfPath, pdfBytes, dictionaryBytes, options);

  for (let i = 0; i < options.warmup; i += 1) {
    run_unredact_web(request);
  }

  const totalMs = [];
  const stageMaps = [];
  const redactionRows = [];
  const guessableRows = [];
  const signatureHashes = [];
  const outputBytes = [];

  for (let i = 0; i < options.repeats; i += 1) {
    const started = performance.now();
    const outputs = run_unredact_web(request);
    const elapsedMs = performance.now() - started;
    totalMs.push(elapsedMs);

    const guessesJson = asUint8Array(outputs.guesses_json);
    const guessesReport = decodeJsonBytes(guessesJson);

    const stageMap = parseStageTimings(guessesReport.stage_timings ?? []);
    stageMaps.push(stageMap);

    const guesses = guessesReport.guesses ?? [];
    redactionRows.push(guesses.length);
    guessableRows.push(
      guesses.reduce(
        (acc, guess) => acc + (guess?.context?.has_anchor_pair ? 1 : 0),
        0,
      ),
    );

    signatureHashes.push(stableTopGuessSignature(guessesReport));

    const redactionsBytes = asUint8Array(outputs.redactions_json);
    const fontsBytes = asUint8Array(outputs.fonts_json);
    const visualizedBytes = asUint8Array(outputs.visualized_pdf_bytes);
    outputBytes.push(
      (redactionsBytes?.byteLength ?? 0) +
        (fontsBytes?.byteLength ?? 0) +
        (guessesJson?.byteLength ?? 0) +
        (visualizedBytes?.byteLength ?? 0),
    );
  }

  return {
    input_pdf: path.relative(rootDir, pdfPath).replace(/\\/g, "/"),
    input_pdf_bytes: pdfBytes.byteLength,
    measured_runs: options.repeats,
    warmup_runs: options.warmup,
    total_ms: summarizeSeries(totalMs),
    stage_ms: stageSummaryFromRuns(stageMaps),
    redaction_rows: summarizeSeries(redactionRows),
    guessable_rows: summarizeSeries(guessableRows),
    output_bytes: summarizeSeries(outputBytes),
    consistency: topSignatureSummary(signatureHashes),
  };
}

function overallSummary(cases) {
  const allTotals = [];
  const allRedactionRows = [];
  const allGuessableRows = [];
  const allOutputBytes = [];

  for (const item of cases) {
    allTotals.push(item.total_ms.mean);
    allRedactionRows.push(item.redaction_rows.mean);
    allGuessableRows.push(item.guessable_rows.mean);
    allOutputBytes.push(item.output_bytes.mean);
  }

  return {
    cases: cases.length,
    mean_total_ms_across_cases: summarizeSeries(allTotals),
    mean_redaction_rows_across_cases: summarizeSeries(allRedactionRows),
    mean_guessable_rows_across_cases: summarizeSeries(allGuessableRows),
    mean_output_bytes_across_cases: summarizeSeries(allOutputBytes),
  };
}

async function main() {
  const rootDir = process.cwd();
  const options = parseArgs(process.argv.slice(2));

  const wasmBinaryPath = path.resolve(rootDir, "web/pkg/unredact_bg.wasm");
  const wasmBytes = await fs.readFile(wasmBinaryPath).catch(() => {
    throw new Error(
      `missing ${path.relative(rootDir, wasmBinaryPath)}. Build first with wasm-pack.`,
    );
  });

  await init({ module_or_path: new Uint8Array(wasmBytes) });

  const dictionary = await loadDictionary(rootDir, options.dictionary);
  const pdfPaths = await collectPdfPaths(rootDir, options);

  const started = performance.now();
  const caseResults = [];
  for (const pdfPath of pdfPaths) {
    console.log(`benchmarking ${path.relative(rootDir, pdfPath)} ...`);
    const caseResult = await runCase(
      pdfPath,
      dictionary.bytes,
      options,
      rootDir,
    );
    caseResults.push(caseResult);
    console.log(
      `  mean=${caseResult.total_ms.mean.toFixed(2)}ms p90=${caseResult.total_ms.p90.toFixed(2)}ms consistency=${(caseResult.consistency.dominant_signature_ratio * 100).toFixed(1)}%`,
    );
  }
  const elapsed = performance.now() - started;

  const report = {
    generated_at_utc: new Date().toISOString(),
    runtime_ms_total: elapsed,
    wasm_binary: path.relative(rootDir, wasmBinaryPath).replace(/\\/g, "/"),
    config: {
      pdf_count: pdfPaths.length,
      repeats: options.repeats,
      warmup: options.warmup,
      enable_image_analysis: options.enableImageAnalysis,
      visualize: options.visualize,
      raster_dpi: options.rasterDpi,
      dictionary_source: dictionary.source,
      dictionary_entries: dictionary.entry_count,
      dictionary_byte_length: dictionary.byte_length,
    },
    cases: caseResults,
    overall: overallSummary(caseResults),
  };

  const outPath = path.resolve(rootDir, options.outPath);
  await fs.mkdir(path.dirname(outPath), { recursive: true });
  await fs.writeFile(outPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");

  console.log(
    `\nWASM benchmark report written to ${path.relative(rootDir, outPath)}`,
  );
}

main().catch((error) => {
  console.error(`wasm benchmark failed: ${error?.message ?? error}`);
  process.exit(1);
});
