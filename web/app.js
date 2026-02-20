import init, { run_unredact_web } from "./pkg/unredact.js";

const textDecoder = new TextDecoder();

const pdfFileInput = document.getElementById("pdfFile");
const dictionaryFileInput = document.getElementById("dictionaryFile");
const enableImageAnalysisInput = document.getElementById("enableImageAnalysis");
const shouldVisuallyScoreInput = document.getElementById("shouldVisuallyScore");
const visualizeOutputInput = document.getElementById("visualizeOutput");
const runButton = document.getElementById("runButton");
const statusElement = document.getElementById("status");
const summaryElement = document.getElementById("summary");
const downloadsElement = document.getElementById("downloads");

let wasmReady = false;

function setStatus(message) {
  statusElement.textContent = message;
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
  throw new Error("Unsupported byte payload shape");
}

function downloadAnchor(fileName, mimeType, bytes) {
  const blob = new Blob([bytes], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.className = "download-link";
  link.href = url;
  link.download = fileName;
  link.textContent = `Download ${fileName}`;
  return link;
}

function clearDownloads() {
  downloadsElement.innerHTML = "";
}

function setDownloads(items) {
  clearDownloads();
  if (items.length === 0) {
    downloadsElement.textContent = "No outputs yet.";
    return;
  }
  for (const item of items) {
    downloadsElement.appendChild(item);
  }
}

function summarizeGuessReport(bytes) {
  try {
    const text = textDecoder.decode(bytes);
    const parsed = JSON.parse(text);
    const guesses = Array.isArray(parsed.guesses) ? parsed.guesses : [];
    const top = guesses.slice(0, 3).map((entry, index) => {
      const best = Array.isArray(entry.exact_matches) && entry.exact_matches.length > 0
        ? entry.exact_matches[0]
        : "(no exact match)";
      return `${index + 1}. ${best}`;
    });
    return [
      `redactions guessed: ${guesses.length}`,
      top.length > 0 ? "top rows:" : "top rows: none",
      ...top,
    ].join("\n");
  } catch (error) {
    return `failed to parse guesses json: ${error}`;
  }
}

async function readFileBytes(file) {
  const buffer = await file.arrayBuffer();
  return new Uint8Array(buffer);
}

async function runAnalysis() {
  if (!wasmReady) {
    setStatus("WebAssembly module is not ready yet.");
    return;
  }

  const pdfFile = pdfFileInput.files?.[0];
  if (!pdfFile) {
    setStatus("Please choose a PDF file.");
    return;
  }

  runButton.disabled = true;
  setStatus("Reading files...");
  summaryElement.textContent = "Running...";
  clearDownloads();

  try {
    const pdfBytes = await readFileBytes(pdfFile);
    const dictionaryFile = dictionaryFileInput.files?.[0];
    const dictionaryBytes = dictionaryFile ? await readFileBytes(dictionaryFile) : null;

    const request = {
      input_name: pdfFile.name,
      pdf_bytes: pdfBytes,
      dictionary_file_bytes: dictionaryBytes,
      cfg: {
        include_details: false,
        enable_image_analysis: Boolean(enableImageAnalysisInput.checked),
        raster_dpi: 200.0,
        guess: {
          visual_score: Boolean(shouldVisuallyScoreInput.checked),
          visual_score_dpi: 200.0,
        },
        visualize: Boolean(visualizeOutputInput.checked),
        visualizer: {
          color: [1.0, 0.0, 0.0],
          text_color: [0.0, 0.4, 1.0],
          border_width: 1.0,
        },
      },
    };

    setStatus("Running analysis...");
    const result = run_unredact_web(request);

    const redactionsJson = asUint8Array(result.redactions_json);
    const fontsJson = asUint8Array(result.fonts_json);
    const guessesJson = asUint8Array(result.guesses_json);
    const visualizedPdf = asUint8Array(result.visualized_pdf_bytes);

    summaryElement.textContent = summarizeGuessReport(guessesJson);
    setStatus("Done.");

    const fileStem = pdfFile.name.replace(/\.pdf$/i, "");
    const downloadItems = [
      downloadAnchor(`${fileStem}.redactions.json`, "application/json", redactionsJson),
      downloadAnchor(`${fileStem}.fonts.json`, "application/json", fontsJson),
      downloadAnchor(`${fileStem}.guesses.json`, "application/json", guessesJson),
    ];
    if (visualizedPdf) {
      downloadItems.push(
        downloadAnchor(`${fileStem}.visualized.pdf`, "application/pdf", visualizedPdf),
      );
    }
    setDownloads(downloadItems);
  } catch (error) {
    setStatus(`Run failed: ${error}`);
    summaryElement.textContent = "No successful output.";
    setDownloads([]);
  } finally {
    runButton.disabled = false;
  }
}

async function boot() {
  try {
    setStatus("Initializing WebAssembly module...");
    await init();
    wasmReady = true;
    runButton.disabled = false;
    setStatus("Ready.");
  } catch (error) {
    setStatus(`Failed to initialize WebAssembly: ${error}`);
    runButton.disabled = true;
  }
}

runButton.addEventListener("click", runAnalysis);
boot();
