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
const guessVisualizationElement = document.getElementById("guessVisualization");
const pdfPreviewStateElement = document.getElementById("pdfPreviewState");
const pdfPreviewElement = document.getElementById("pdfPreview");

let wasmReady = false;
let currentPdfPreviewUrl = null;

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

function clearPdfPreview() {
  if (currentPdfPreviewUrl) {
    URL.revokeObjectURL(currentPdfPreviewUrl);
    currentPdfPreviewUrl = null;
  }
  pdfPreviewElement.removeAttribute("src");
  pdfPreviewElement.classList.remove("visible");
}

function setPdfPreviewState(message) {
  pdfPreviewStateElement.textContent = message;
}

function renderPdfPreview(visualizedPdfBytes) {
  clearPdfPreview();
  if (!visualizedPdfBytes || visualizedPdfBytes.length === 0) {
    setPdfPreviewState(
      'No visualized PDF returned. Keep "Generate visualized PDF" enabled to preview inline.',
    );
    return;
  }
  const blob = new Blob([visualizedPdfBytes], { type: "application/pdf" });
  currentPdfPreviewUrl = URL.createObjectURL(blob);
  pdfPreviewElement.src = `${currentPdfPreviewUrl}#view=FitH`;
  pdfPreviewElement.classList.add("visible");
  setPdfPreviewState("Inline preview loaded.");
}

function parseJsonReport(bytes, label) {
  const decoded = textDecoder.decode(bytes);
  try {
    return JSON.parse(decoded);
  } catch (error) {
    throw new Error(`failed to parse ${label}: ${error}`);
  }
}

function topGuessText(row) {
  if (Array.isArray(row.exact_matches) && row.exact_matches.length > 0) {
    return String(row.exact_matches[0]);
  }
  if (Array.isArray(row.candidates) && row.candidates.length > 0) {
    return String(row.candidates[0].text ?? "(candidate)");
  }
  return "(no guess)";
}

function summarizeGuessReport(report) {
  const guesses = Array.isArray(report?.guesses) ? report.guesses : [];
  const top = guesses
    .slice(0, 3)
    .map((entry, index) => `${index + 1}. ${topGuessText(entry)}`);
  return [
    `redactions guessed: ${guesses.length}`,
    top.length > 0 ? "top rows:" : "top rows: none",
    ...top,
  ].join("\n");
}

function clearGuessVisualization(message) {
  guessVisualizationElement.innerHTML = "";
  guessVisualizationElement.classList.add("empty-state");
  guessVisualizationElement.textContent = message;
}

function normalizeNumber(value, fallback = 0) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function buildGuessViewRows(report) {
  const guesses = Array.isArray(report?.guesses) ? report.guesses : [];
  const rows = guesses.map((row, index) => {
    const bbox = row?.bbox ?? {};
    const x0 = normalizeNumber(bbox.x0, 0);
    const x1 = normalizeNumber(bbox.x1, 0);
    const widthPt = Math.max(0, x1 - x0);
    const context = row?.context ?? {};
    return {
      key: `${normalizeNumber(row?.page_index, 0)}-${index}`,
      pageIndex: normalizeNumber(row?.page_index, 0),
      rowIndex: index,
      widthPt,
      guessText: topGuessText(row),
      leftContext: String(context.left_anchor_text ?? "").trim(),
      rightContext: String(context.right_anchor_text ?? "").trim(),
      candidateCount: Array.isArray(row?.candidates)
        ? row.candidates.length
        : 0,
      anchorMode: String(context.anchor_mode ?? "unknown"),
      bboxLabel: `x=${x0.toFixed(1)}..${x1.toFixed(1)}`,
    };
  });
  const maxWidthPt = rows.reduce((max, row) => Math.max(max, row.widthPt), 0);
  return { rows, maxWidthPt };
}

function stripWidthPercent(widthPt, maxWidthPt) {
  if (maxWidthPt <= 0) {
    return 38;
  }
  const ratio = Math.min(1, Math.max(0, widthPt / maxWidthPt));
  return Math.round(24 + ratio * 76);
}

function groupRowsByPage(rows) {
  const grouped = new Map();
  for (const row of rows) {
    const key = row.pageIndex;
    if (!grouped.has(key)) {
      grouped.set(key, []);
    }
    grouped.get(key).push(row);
  }
  return [...grouped.entries()].sort((left, right) => left[0] - right[0]);
}

function renderGuessVisualization(report) {
  const { rows, maxWidthPt } = buildGuessViewRows(report);
  if (rows.length === 0) {
    clearGuessVisualization("No guess rows were returned.");
    return;
  }

  guessVisualizationElement.innerHTML = "";
  guessVisualizationElement.classList.remove("empty-state");

  const pageGroups = groupRowsByPage(rows);
  for (const [pageIndex, pageRows] of pageGroups) {
    const pageBlock = document.createElement("section");
    pageBlock.className = "guess-page-group";

    const pageTitle = document.createElement("h3");
    pageTitle.className = "guess-page-title";
    pageTitle.textContent = `Page ${pageIndex + 1} (${pageRows.length} rows)`;
    pageBlock.appendChild(pageTitle);

    const rowList = document.createElement("div");
    rowList.className = "guess-row-list";

    for (const row of pageRows) {
      const card = document.createElement("article");
      card.className = "guess-row-card";
      card.dataset.key = row.key;

      const meta = document.createElement("p");
      meta.className = "guess-row-meta";
      meta.textContent = `row #${row.rowIndex + 1} | ${row.bboxLabel} | candidates=${row.candidateCount} | anchor=${row.anchorMode}`;
      card.appendChild(meta);

      const context = document.createElement("p");
      context.className = "guess-row-context";
      const left = row.leftContext || "…";
      const right = row.rightContext || "…";
      context.append(left, " ");
      const guessChip = document.createElement("span");
      guessChip.className = "guess-chip";
      guessChip.textContent = row.guessText;
      context.appendChild(guessChip);
      context.append(" ", right);
      card.appendChild(context);

      const stripTrack = document.createElement("div");
      stripTrack.className = "guess-strip-track";
      const strip = document.createElement("div");
      strip.className = "guess-strip";
      strip.style.width = `${stripWidthPercent(row.widthPt, maxWidthPt)}%`;
      strip.textContent = row.guessText;
      stripTrack.appendChild(strip);
      card.appendChild(stripTrack);

      rowList.appendChild(card);
    }

    pageBlock.appendChild(rowList);
    guessVisualizationElement.appendChild(pageBlock);
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
  clearGuessVisualization("Running...");
  setPdfPreviewState("Running...");
  clearPdfPreview();
  clearDownloads();

  try {
    const pdfBytes = await readFileBytes(pdfFile);
    const dictionaryFile = dictionaryFileInput.files?.[0];
    const dictionaryBytes = dictionaryFile
      ? await readFileBytes(dictionaryFile)
      : null;

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
    const guessReport = parseJsonReport(guessesJson, "guesses json");

    summaryElement.textContent = summarizeGuessReport(guessReport);
    renderGuessVisualization(guessReport);
    renderPdfPreview(visualizedPdf);
    setStatus("Done.");

    const fileStem = pdfFile.name.replace(/\.pdf$/i, "");
    const downloadItems = [
      downloadAnchor(
        `${fileStem}.redactions.json`,
        "application/json",
        redactionsJson,
      ),
      downloadAnchor(`${fileStem}.fonts.json`, "application/json", fontsJson),
      downloadAnchor(
        `${fileStem}.guesses.json`,
        "application/json",
        guessesJson,
      ),
    ];
    if (visualizedPdf) {
      downloadItems.push(
        downloadAnchor(
          `${fileStem}.visualized.pdf`,
          "application/pdf",
          visualizedPdf,
        ),
      );
    }
    setDownloads(downloadItems);
  } catch (error) {
    setStatus(`Run failed: ${error}`);
    summaryElement.textContent = "No successful output.";
    clearGuessVisualization("No successful output.");
    setPdfPreviewState("No successful output.");
    clearPdfPreview();
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
