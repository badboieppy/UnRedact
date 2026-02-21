import init, { run_unredact_web } from "./pkg/unredact.js";

const textDecoder = new TextDecoder();

const RESULTS_DB_NAME = "unredact_web_results";
const RESULTS_DB_VERSION = 1;
const RESULTS_STORE = "outputs";

const pdfFileInput = document.getElementById("pdfFile");
const pdfDirectoryInput = document.getElementById("pdfDirectory");
const dictionaryFileInput = document.getElementById("dictionaryFile");
const enableImageAnalysisInput = document.getElementById("enableImageAnalysis");
const shouldVisuallyScoreInput = document.getElementById("shouldVisuallyScore");
const visualizeOutputInput = document.getElementById("visualizeOutput");
const runButton = document.getElementById("runButton");
const clearResultsButton = document.getElementById("clearResultsButton");
const statusElement = document.getElementById("status");
const benchmarkSummaryElement = document.getElementById("benchmarkSummary");
const batchResultsElement = document.getElementById("batchResults");
const summaryElement = document.getElementById("summary");
const downloadsElement = document.getElementById("downloads");
const guessVisualizationElement = document.getElementById("guessVisualization");
const pdfPreviewStateElement = document.getElementById("pdfPreviewState");
const pdfPreviewElement = document.getElementById("pdfPreview");

let wasmReady = false;
let isRunning = false;
let nextResultId = 1;
let openDbPromise = null;
let selectedResultId = null;
let selectedGuessCache = null;
let selectedOutputUrls = null;
const batchResults = [];

function setStatus(message) {
  statusElement.textContent = message;
}

function setPdfPreviewState(message) {
  pdfPreviewStateElement.textContent = message;
}

function setBenchmarkSummary(message) {
  benchmarkSummaryElement.textContent = message;
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

function normalizeNumber(value, fallback = 0) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function formatBytes(value) {
  if (!Number.isFinite(value) || value < 0) {
    return "n/a";
  }
  const units = ["B", "KB", "MB", "GB"];
  let index = 0;
  let scaled = value;
  while (scaled >= 1024 && index < units.length - 1) {
    scaled /= 1024;
    index += 1;
  }
  return `${scaled.toFixed(index === 0 ? 0 : 2)} ${units[index]}`;
}

function formatMs(value) {
  if (!Number.isFinite(value) || value < 0) {
    return "n/a";
  }
  return `${value.toFixed(1)} ms`;
}

function fileDisplayLabel(file) {
  if (
    typeof file.webkitRelativePath === "string" &&
    file.webkitRelativePath.trim() !== ""
  ) {
    return file.webkitRelativePath;
  }
  return file.name;
}

function collectPdfFiles() {
  const out = [];
  const seen = new Set();
  const addFile = (file) => {
    if (
      !file ||
      typeof file.name !== "string" ||
      !file.name.toLowerCase().endsWith(".pdf")
    ) {
      return;
    }
    const label = fileDisplayLabel(file);
    const key = `${label}|${file.size}|${file.lastModified}`;
    if (seen.has(key)) {
      return;
    }
    seen.add(key);
    out.push(file);
  };
  for (const file of pdfFileInput.files ?? []) {
    addFile(file);
  }
  for (const file of pdfDirectoryInput.files ?? []) {
    addFile(file);
  }
  out.sort((left, right) =>
    fileDisplayLabel(left).localeCompare(fileDisplayLabel(right)),
  );
  return out;
}

function topGuessText(row) {
  if (Array.isArray(row?.exact_matches) && row.exact_matches.length > 0) {
    return String(row.exact_matches[0]);
  }
  if (Array.isArray(row?.candidates) && row.candidates.length > 0) {
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

function summarizeGuessReportCompact(report) {
  const guesses = Array.isArray(report?.guesses) ? report.guesses : [];
  return {
    guessCount: guesses.length,
    topGuess: guesses.length > 0 ? topGuessText(guesses[0]) : "(no guess)",
  };
}

function clearGuessVisualization(message) {
  guessVisualizationElement.innerHTML = "";
  guessVisualizationElement.classList.add("empty-state");
  guessVisualizationElement.textContent = message;
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

function downloadAnchorFromUrl(fileName, url) {
  const link = document.createElement("a");
  link.className = "download-link";
  link.href = url;
  link.download = fileName;
  link.textContent = `Download ${fileName}`;
  return link;
}

function parseJsonBytes(bytes, label) {
  const decoded = textDecoder.decode(bytes);
  try {
    return JSON.parse(decoded);
  } catch (error) {
    throw new Error(`failed to parse ${label}: ${error}`);
  }
}

function parseJsonText(text, label) {
  try {
    return JSON.parse(text);
  } catch (error) {
    throw new Error(`failed to parse ${label}: ${error}`);
  }
}

async function readFileBytes(file) {
  let buffer;
  try {
    buffer = await file.arrayBuffer();
  } catch (error) {
    const label = file?.name ?? "input";
    throw new Error(`failed to read ${label}: ${error}`);
  }
  return new Uint8Array(buffer);
}

function outputBaseName(label, id) {
  const normalized = label
    .replace(/\\/g, "/")
    .replace(/\.pdf$/i, "")
    .replace(/\//g, "__")
    .replace(/[^A-Za-z0-9._-]/g, "_");
  return normalized.trim() !== "" ? normalized : `file_${id}`;
}

function requestAsPromise(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () =>
      reject(request.error ?? new Error("indexeddb request failed"));
  });
}

function txAsPromise(tx) {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () =>
      reject(tx.error ?? new Error("indexeddb transaction failed"));
    tx.onabort = () =>
      reject(tx.error ?? new Error("indexeddb transaction aborted"));
  });
}

function openResultsDb() {
  if (openDbPromise) {
    return openDbPromise;
  }
  openDbPromise = new Promise((resolve, reject) => {
    if (!("indexedDB" in window)) {
      reject(new Error("IndexedDB is not available in this browser."));
      return;
    }
    const request = indexedDB.open(RESULTS_DB_NAME, RESULTS_DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(RESULTS_STORE)) {
        db.createObjectStore(RESULTS_STORE, { keyPath: "id" });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () =>
      reject(request.error ?? new Error("failed to open indexeddb"));
  });
  return openDbPromise;
}

async function idbClearOutputs() {
  const db = await openResultsDb();
  const tx = db.transaction(RESULTS_STORE, "readwrite");
  tx.objectStore(RESULTS_STORE).clear();
  await txAsPromise(tx);
}

async function idbPutOutput(record) {
  const db = await openResultsDb();
  const tx = db.transaction(RESULTS_STORE, "readwrite");
  tx.objectStore(RESULTS_STORE).put(record);
  await txAsPromise(tx);
}

async function idbGetOutput(id) {
  const db = await openResultsDb();
  const tx = db.transaction(RESULTS_STORE, "readonly");
  const request = tx.objectStore(RESULTS_STORE).get(id);
  const result = await requestAsPromise(request);
  await txAsPromise(tx);
  return result ?? null;
}

function revokeSelectedOutputUrls() {
  if (!selectedOutputUrls) {
    return;
  }
  if (selectedOutputUrls.redactionsJsonUrl) {
    URL.revokeObjectURL(selectedOutputUrls.redactionsJsonUrl);
  }
  if (selectedOutputUrls.fontsJsonUrl) {
    URL.revokeObjectURL(selectedOutputUrls.fontsJsonUrl);
  }
  if (selectedOutputUrls.guessesJsonUrl) {
    URL.revokeObjectURL(selectedOutputUrls.guessesJsonUrl);
  }
  if (selectedOutputUrls.visualizedPdfUrl) {
    URL.revokeObjectURL(selectedOutputUrls.visualizedPdfUrl);
  }
  selectedOutputUrls = null;
}

function clearPdfPreview() {
  pdfPreviewElement.removeAttribute("src");
  pdfPreviewElement.classList.remove("visible");
}

function renderPdfPreview(url, label) {
  clearPdfPreview();
  if (!url) {
    setPdfPreviewState(
      'No visualized PDF for this file. Enable "Generate visualized PDF" before running.',
    );
    return;
  }
  pdfPreviewElement.src = `${url}#view=FitH`;
  pdfPreviewElement.classList.add("visible");
  setPdfPreviewState(`Inline preview: ${label}`);
}

function setDownloadsForSelected(result, urls) {
  if (!result || !urls) {
    setDownloads([]);
    return;
  }
  const links = [
    downloadAnchorFromUrl(
      result.outputNames.redactions,
      urls.redactionsJsonUrl,
    ),
    downloadAnchorFromUrl(result.outputNames.fonts, urls.fontsJsonUrl),
    downloadAnchorFromUrl(result.outputNames.guesses, urls.guessesJsonUrl),
  ];
  if (urls.visualizedPdfUrl && result.outputNames.visualized) {
    links.push(
      downloadAnchorFromUrl(
        result.outputNames.visualized,
        urls.visualizedPdfUrl,
      ),
    );
  }
  setDownloads(links);
}

function overallBatchSummary() {
  const total = batchResults.length;
  const success = batchResults.filter(
    (result) => result.status === "ok",
  ).length;
  const failed = total - success;
  const totalOutputBytes = batchResults
    .filter((result) => result.status === "ok")
    .reduce((sum, result) => sum + (result.totalOutputBytes ?? 0), 0);
  return [
    `files processed: ${total}`,
    `success: ${success}`,
    `failed: ${failed}`,
    `indexeddb output size (this run): ${formatBytes(totalOutputBytes)}`,
    "select a successful row in Batch Results to inspect details",
  ].join("\n");
}

function renderBatchResults() {
  batchResultsElement.innerHTML = "";
  if (batchResults.length === 0) {
    batchResultsElement.classList.add("empty-state");
    batchResultsElement.textContent = "No run yet.";
    return;
  }
  batchResultsElement.classList.remove("empty-state");

  const list = document.createElement("div");
  list.className = "batch-list";
  for (const result of batchResults) {
    const row = document.createElement("article");
    row.className = "batch-row";
    if (result.id === selectedResultId) {
      row.classList.add("selected");
    }

    const heading = document.createElement("div");
    heading.className = "batch-row-heading";
    const fileName = document.createElement("span");
    fileName.className = "batch-file-label";
    fileName.textContent = result.label;
    heading.appendChild(fileName);

    const badge = document.createElement("span");
    badge.className = `batch-status-badge ${result.status}`;
    badge.textContent = result.status === "ok" ? "OK" : "ERROR";
    heading.appendChild(badge);
    row.appendChild(heading);

    const meta = document.createElement("p");
    meta.className = "batch-row-meta";
    if (result.status === "ok") {
      meta.textContent = `redactions=${result.guessCount} | top=${result.topGuess} | output=${formatBytes(result.totalOutputBytes)} | elapsed=${formatMs(result.elapsedMs)}`;
    } else {
      meta.textContent = result.errorMessage;
    }
    row.appendChild(meta);

    const actions = document.createElement("div");
    actions.className = "batch-row-actions";
    if (result.status === "ok") {
      const inspectButton = document.createElement("button");
      inspectButton.type = "button";
      inspectButton.className = "batch-view-button";
      inspectButton.textContent = "Inspect";
      inspectButton.addEventListener("click", () => {
        void selectResult(result.id);
      });
      actions.appendChild(inspectButton);
    }
    row.appendChild(actions);

    list.appendChild(row);
  }

  batchResultsElement.appendChild(list);
}

function createHeapSample(label, metrics) {
  const memory = performance?.memory;
  if (!memory) {
    return null;
  }
  return {
    label,
    used: memory.usedJSHeapSize,
    total: memory.totalJSHeapSize,
    limit: memory.jsHeapSizeLimit,
    processed: metrics.processed,
    elapsedMs: performance.now() - metrics.startedAt,
  };
}

async function storageEstimate() {
  if (!navigator.storage?.estimate) {
    return null;
  }
  try {
    return await navigator.storage.estimate();
  } catch (_) {
    return null;
  }
}

function renderBenchmarkProgress(metrics, activeLabel = null) {
  const elapsedMs = performance.now() - metrics.startedAt;
  const averageFileMs =
    metrics.fileElapsedMs.length > 0
      ? metrics.fileElapsedMs.reduce((sum, value) => sum + value, 0) /
        metrics.fileElapsedMs.length
      : 0;
  const lines = [
    `files processed: ${metrics.processed}/${metrics.totalFiles}`,
    `success: ${metrics.success}`,
    `failed: ${metrics.failed}`,
    `elapsed: ${formatMs(elapsedMs)}`,
    `avg file time: ${formatMs(averageFileMs)}`,
    `input bytes read: ${formatBytes(metrics.inputBytes)}`,
    `output bytes stored: ${formatBytes(metrics.outputBytes)}`,
  ];

  if (activeLabel) {
    lines.push(`active file: ${activeLabel}`);
  }

  if (metrics.storageBefore) {
    lines.push(
      `storage before: ${formatBytes(metrics.storageBefore.usage ?? 0)} / ${formatBytes(metrics.storageBefore.quota ?? 0)}`,
    );
  }
  if (metrics.storageAfter) {
    lines.push(
      `storage after: ${formatBytes(metrics.storageAfter.usage ?? 0)} / ${formatBytes(metrics.storageAfter.quota ?? 0)}`,
    );
  }

  if (metrics.heapSamples.length > 0) {
    const usedValues = metrics.heapSamples.map((sample) => sample.used);
    const current = usedValues[usedValues.length - 1];
    const max = Math.max(...usedValues);
    const min = Math.min(...usedValues);
    lines.push(`js heap current: ${formatBytes(current)}`);
    lines.push(`js heap min/max: ${formatBytes(min)} / ${formatBytes(max)}`);
  } else {
    lines.push(
      "js heap metrics: unavailable (browser does not expose performance.memory)",
    );
  }

  setBenchmarkSummary(lines.join("\n"));
}

function clearUiVisuals() {
  clearGuessVisualization("No run yet.");
  clearPdfPreview();
  setPdfPreviewState(
    'Run with "Generate visualized PDF" enabled to preview inline.',
  );
  summaryElement.textContent = "No run yet.";
  setDownloads([]);
}

async function resetBatchState() {
  revokeSelectedOutputUrls();
  selectedResultId = null;
  selectedGuessCache = null;
  batchResults.length = 0;
  renderBatchResults();
  clearUiVisuals();
  setBenchmarkSummary("No run yet.");
  await idbClearOutputs();
}

async function clearAllResults() {
  if (isRunning) {
    return;
  }
  await resetBatchState();
  setStatus("Ready.");
}

function setRunningUi(busy) {
  runButton.disabled = busy || !wasmReady;
  clearResultsButton.disabled = busy;
}

function buildRequestConfig() {
  return {
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
  };
}

async function selectResult(resultId) {
  const result = batchResults.find((item) => item.id === resultId);
  if (!result || result.status !== "ok") {
    return;
  }

  setStatus(`Loading details for ${result.label}...`);
  const stored = await idbGetOutput(result.id);
  if (!stored) {
    setStatus(`Stored output for ${result.label} is not available.`);
    return;
  }

  revokeSelectedOutputUrls();
  const urls = {
    redactionsJsonUrl: URL.createObjectURL(stored.redactionsBlob),
    fontsJsonUrl: URL.createObjectURL(stored.fontsBlob),
    guessesJsonUrl: URL.createObjectURL(stored.guessesBlob),
    visualizedPdfUrl: stored.visualizedBlob
      ? URL.createObjectURL(stored.visualizedBlob)
      : null,
  };
  selectedOutputUrls = urls;
  selectedResultId = result.id;
  renderBatchResults();

  let report;
  if (selectedGuessCache && selectedGuessCache.resultId === result.id) {
    report = selectedGuessCache.report;
  } else {
    const guessesText = await stored.guessesBlob.text();
    report = parseJsonText(guessesText, `${result.label} guesses`);
    selectedGuessCache = { resultId: result.id, report };
  }

  summaryElement.textContent = summarizeGuessReport(report);
  renderGuessVisualization(report);
  renderPdfPreview(urls.visualizedPdfUrl, result.label);
  setDownloadsForSelected(result, urls);
  setStatus(`Ready. Showing ${result.label}`);
}

function createSuccessResultMeta(
  file,
  id,
  compact,
  elapsedMs,
  totalOutputBytes,
  outputNames,
) {
  return {
    id,
    label: fileDisplayLabel(file),
    status: "ok",
    errorMessage: "",
    guessCount: compact.guessCount,
    topGuess: compact.topGuess,
    elapsedMs,
    totalOutputBytes,
    outputNames,
  };
}

function createErrorResultMeta(file, id, error, elapsedMs) {
  return {
    id,
    label: fileDisplayLabel(file),
    status: "error",
    errorMessage: String(error),
    guessCount: 0,
    topGuess: "(error)",
    elapsedMs,
    totalOutputBytes: 0,
    outputNames: null,
  };
}

async function processOneFile(file, dictionaryBytes, cfg, metrics) {
  const id = nextResultId;
  nextResultId += 1;
  const label = fileDisplayLabel(file);
  const started = performance.now();

  try {
    const pdfBytes = await readFileBytes(file);
    metrics.inputBytes += pdfBytes.byteLength;

    const response = run_unredact_web({
      input_name: label,
      pdf_bytes: pdfBytes,
      dictionary_file_bytes: dictionaryBytes,
      cfg,
    });

    const redactionsBytes = asUint8Array(response.redactions_json);
    const fontsBytes = asUint8Array(response.fonts_json);
    const guessesBytes = asUint8Array(response.guesses_json);
    const visualizedBytes = asUint8Array(response.visualized_pdf_bytes);

    const redactionsBlob = new Blob([redactionsBytes], {
      type: "application/json",
    });
    const fontsBlob = new Blob([fontsBytes], { type: "application/json" });
    const guessesBlob = new Blob([guessesBytes], { type: "application/json" });
    const visualizedBlob = visualizedBytes
      ? new Blob([visualizedBytes], { type: "application/pdf" })
      : null;

    const outputBytesTotal =
      redactionsBlob.size +
      fontsBlob.size +
      guessesBlob.size +
      (visualizedBlob ? visualizedBlob.size : 0);
    metrics.outputBytes += outputBytesTotal;

    const report = parseJsonBytes(guessesBytes, `${label} guesses`);
    const compact = summarizeGuessReportCompact(report);
    const baseName = outputBaseName(label, id);
    const outputNames = {
      redactions: `${baseName}.redactions.json`,
      fonts: `${baseName}.fonts.json`,
      guesses: `${baseName}.guesses.json`,
      visualized: visualizedBlob ? `${baseName}.visualized.pdf` : null,
    };

    await idbPutOutput({
      id,
      label,
      createdAt: Date.now(),
      redactionsBlob,
      fontsBlob,
      guessesBlob,
      visualizedBlob,
    });

    const elapsedMs = performance.now() - started;
    return createSuccessResultMeta(
      file,
      id,
      compact,
      elapsedMs,
      outputBytesTotal,
      outputNames,
    );
  } catch (error) {
    const elapsedMs = performance.now() - started;
    return createErrorResultMeta(file, id, error, elapsedMs);
  }
}

function delayTick() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

async function runAnalysis() {
  if (!wasmReady || isRunning) {
    return;
  }

  const files = collectPdfFiles();
  if (files.length === 0) {
    setStatus("Select one or more PDF files (or a PDF directory) first.");
    return;
  }

  isRunning = true;
  setRunningUi(true);
  try {
    await resetBatchState();
    const dictionaryFile = dictionaryFileInput.files?.[0];
    const dictionaryBytes = dictionaryFile
      ? await readFileBytes(dictionaryFile)
      : null;
    const cfg = buildRequestConfig();

    const metrics = {
      totalFiles: files.length,
      startedAt: performance.now(),
      processed: 0,
      success: 0,
      failed: 0,
      inputBytes: 0,
      outputBytes: 0,
      fileElapsedMs: [],
      heapSamples: [],
      storageBefore: await storageEstimate(),
      storageAfter: null,
    };
    const initialHeap = createHeapSample("start", metrics);
    if (initialHeap) {
      metrics.heapSamples.push(initialHeap);
    }
    renderBenchmarkProgress(metrics);
    setStatus(`Running batch: 0/${files.length}`);

    for (const file of files) {
      const label = fileDisplayLabel(file);
      setStatus(`Running ${metrics.processed + 1}/${files.length}: ${label}`);
      const resultMeta = await processOneFile(
        file,
        dictionaryBytes,
        cfg,
        metrics,
      );
      batchResults.push(resultMeta);
      metrics.processed += 1;
      metrics.fileElapsedMs.push(resultMeta.elapsedMs);
      if (resultMeta.status === "ok") {
        metrics.success += 1;
      } else {
        metrics.failed += 1;
      }
      const heapSample = createHeapSample(label, metrics);
      if (heapSample) {
        metrics.heapSamples.push(heapSample);
      }
      renderBatchResults();
      renderBenchmarkProgress(metrics, label);
      summaryElement.textContent = overallBatchSummary();
      await delayTick();
    }

    metrics.storageAfter = await storageEstimate();
    const finalHeap = createHeapSample("end", metrics);
    if (finalHeap) {
      metrics.heapSamples.push(finalHeap);
    }
    renderBenchmarkProgress(metrics);

    const lastSuccess = [...batchResults]
      .reverse()
      .find((result) => result.status === "ok");
    if (lastSuccess) {
      await selectResult(lastSuccess.id);
      setStatus(
        `Done. Processed=${metrics.processed}, success=${metrics.success}, failed=${metrics.failed}`,
      );
    } else {
      clearGuessVisualization("No successful files to visualize.");
      clearPdfPreview();
      setPdfPreviewState("No successful files to preview.");
      setDownloads([]);
      setStatus("Completed with no successful files.");
    }
  } catch (error) {
    setStatus(`Run failed: ${error}`);
  } finally {
    isRunning = false;
    setRunningUi(false);
  }
}

async function boot() {
  try {
    setStatus("Initializing WebAssembly module...");
    await init();
    await openResultsDb();
    wasmReady = true;
    setRunningUi(false);
    setStatus("Ready.");
  } catch (error) {
    wasmReady = false;
    setRunningUi(true);
    setStatus(`Failed to initialize WebAssembly or IndexedDB: ${error}`);
  }
}

runButton.addEventListener("click", () => {
  void runAnalysis();
});

clearResultsButton.addEventListener("click", () => {
  void clearAllResults();
});

boot();
