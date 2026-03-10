#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { TextDecoder } from "node:util";

import init, { run_unredact_web } from "../web/pkg/unredact.js";

const rootDir = process.cwd();
const wasmBinaryPath = path.resolve(rootDir, "web/pkg/unredact_bg.wasm");
const samplePdfPath = path.resolve(rootDir, "test_data/EFTA00101126.pdf");
const textDecoder = new TextDecoder();
const nodeMajor = Number.parseInt(process.versions.node.split(".")[0] ?? "0", 10);

function asUint8Array(value, fieldName) {
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
  throw new Error(`unsupported byte payload for ${fieldName}: ${typeof value}`);
}

function decodeJson(bytes, fieldName) {
  assert(bytes && bytes.byteLength > 0, `${fieldName} should be non-empty`);
  return JSON.parse(textDecoder.decode(bytes));
}

async function main() {
  assert(
    Number.isFinite(nodeMajor) && nodeMajor >= 22,
    `Node.js 22 or newer is required for this wasm smoke test; found ${process.versions.node}`,
  );

  const [wasmBytes, pdfBytes] = await Promise.all([
    fs.readFile(wasmBinaryPath),
    fs.readFile(samplePdfPath),
  ]);

  await init({ module_or_path: new Uint8Array(wasmBytes) });

  const outputs = run_unredact_web({
    input_name: path.basename(samplePdfPath),
    pdf_bytes: new Uint8Array(pdfBytes),
    dictionary_file_bytes: null,
      cfg: {
        include_details: false,
        enable_image_analysis: true,
        guess: {},
        visualize: false,
        visualizer: {
          color: [1.0, 0.0, 0.0],
        text_color: [0.0, 0.4, 1.0],
        border_width: 1.0,
      },
    },
  });

  const redactions = decodeJson(
    asUint8Array(outputs.redactions_json, "redactions_json"),
    "redactions_json",
  );
  const guesses = decodeJson(
    asUint8Array(outputs.guesses_json, "guesses_json"),
    "guesses_json",
  );
  const anchors = decodeJson(
    asUint8Array(outputs.anchors_json, "anchors_json"),
    "anchors_json",
  );
  const diagnostics = decodeJson(
    asUint8Array(outputs.diagnostics_json, "diagnostics_json"),
    "diagnostics_json",
  );

  assert.equal(outputs.visualized_pdf_bytes ?? null, null);
  assert.ok(Array.isArray(redactions.redactions), "redactions payload should decode");
  assert.ok(redactions.redactions.length > 0, "expected detected redactions");
  assert.ok(Array.isArray(guesses.guesses), "guesses payload should decode");
  assert.ok(guesses.guesses.length > 0, "expected guessed rows");
  assert.ok(Array.isArray(anchors.decisions), "anchors payload should decode");
  assert.ok(Array.isArray(diagnostics.items), "diagnostics payload should decode");

  for (const row of guesses.guesses) {
    assert.equal(typeof row.page_index, "number");
    assert.ok(Array.isArray(row.candidates), "row candidates should be an array");
  }

  console.log(
    JSON.stringify(
      {
        input: path.basename(samplePdfPath),
        redaction_count: redactions.redactions.length,
        guess_rows: guesses.guesses.length,
        anchor_rows: anchors.decisions.length,
        diagnostics_count: diagnostics.items.length,
      },
      null,
      2,
    ),
  );
}

main().catch((error) => {
  console.error(`wasm smoke test failed: ${error?.message ?? error}`);
  process.exit(1);
});
