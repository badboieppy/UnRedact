import init, { run_unredact_web } from "./pkg/unredact.js";

const textDecoder = new TextDecoder();
const textEncoder = new TextEncoder();

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
const downloadBatchZipButton = document.getElementById(
  "downloadBatchZipButton",
);
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
let isExportingBatchZip = false;
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

function successfulBatchResults() {
  return batchResults.filter((result) => result.status === "ok");
}

function updateBatchZipButtonState() {
  if (!downloadBatchZipButton) {
    return;
  }
  downloadBatchZipButton.disabled =
    !wasmReady ||
    isRunning ||
    isExportingBatchZip ||
    successfulBatchResults().length === 0;
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

function safeZipPath(path) {
  const parts = String(path ?? "")
    .replace(/\\/g, "/")
    .split("/")
    .map((part) => part.trim())
    .filter((part) => part !== "" && part !== "." && part !== "..");
  return parts.join("/");
}

function makeUniqueZipPath(path, usedLowerPaths) {
  const normalized = safeZipPath(path) || "output.bin";
  let candidate = normalized;
  let suffix = 2;
  while (usedLowerPaths.has(candidate.toLowerCase())) {
    const dot = normalized.lastIndexOf(".");
    if (dot > 0) {
      candidate = `${normalized.slice(0, dot)} (${suffix})${normalized.slice(dot)}`;
    } else {
      candidate = `${normalized} (${suffix})`;
    }
    suffix += 1;
  }
  usedLowerPaths.add(candidate.toLowerCase());
  return candidate;
}

function writeU16(view, offset, value) {
  view.setUint16(offset, value & 0xffff, true);
}

function writeU32(view, offset, value) {
  view.setUint32(offset, value >>> 0, true);
}

const CRC32_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let index = 0; index < 256; index += 1) {
    let value = index;
    for (let bit = 0; bit < 8; bit += 1) {
      value = (value & 1) !== 0 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
    }
    table[index] = value >>> 0;
  }
  return table;
})();

function crc32Bytes(bytes) {
  let value = 0xffffffff;
  for (let index = 0; index < bytes.length; index += 1) {
    value = CRC32_TABLE[(value ^ bytes[index]) & 0xff] ^ (value >>> 8);
  }
  return (value ^ 0xffffffff) >>> 0;
}

async function crc32Blob(blob) {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  return crc32Bytes(bytes);
}

function dosTimestamp(date) {
  const safeDate = date instanceof Date ? date : new Date();
  const year = Math.max(1980, safeDate.getFullYear());
  const month = safeDate.getMonth() + 1;
  const day = safeDate.getDate();
  const hours = safeDate.getHours();
  const minutes = safeDate.getMinutes();
  const seconds = Math.floor(safeDate.getSeconds() / 2);
  const dosDate =
    (((year - 1980) & 0x7f) << 9) | ((month & 0x0f) << 5) | (day & 0x1f);
  const dosTime =
    ((hours & 0x1f) << 11) | ((minutes & 0x3f) << 5) | (seconds & 0x1f);
  return { dosDate, dosTime };
}

function buildZipLocalHeader(nameBytes, crc32, size, dosDate, dosTime) {
  const header = new Uint8Array(30 + nameBytes.length);
  const view = new DataView(header.buffer);
  writeU32(view, 0, 0x04034b50);
  writeU16(view, 4, 20);
  writeU16(view, 6, 0);
  writeU16(view, 8, 0);
  writeU16(view, 10, dosTime);
  writeU16(view, 12, dosDate);
  writeU32(view, 14, crc32);
  writeU32(view, 18, size);
  writeU32(view, 22, size);
  writeU16(view, 26, nameBytes.length);
  writeU16(view, 28, 0);
  header.set(nameBytes, 30);
  return header;
}

function buildZipCentralHeader(
  nameBytes,
  crc32,
  size,
  dosDate,
  dosTime,
  localHeaderOffset,
) {
  const header = new Uint8Array(46 + nameBytes.length);
  const view = new DataView(header.buffer);
  writeU32(view, 0, 0x02014b50);
  writeU16(view, 4, 20);
  writeU16(view, 6, 20);
  writeU16(view, 8, 0);
  writeU16(view, 10, 0);
  writeU16(view, 12, dosTime);
  writeU16(view, 14, dosDate);
  writeU32(view, 16, crc32);
  writeU32(view, 20, size);
  writeU32(view, 24, size);
  writeU16(view, 28, nameBytes.length);
  writeU16(view, 30, 0);
  writeU16(view, 32, 0);
  writeU16(view, 34, 0);
  writeU16(view, 36, 0);
  writeU32(view, 38, 0);
  writeU32(view, 42, localHeaderOffset);
  header.set(nameBytes, 46);
  return header;
}

function buildZipEndRecord(entryCount, centralSize, centralOffset) {
  const record = new Uint8Array(22);
  const view = new DataView(record.buffer);
  writeU32(view, 0, 0x06054b50);
  writeU16(view, 4, 0);
  writeU16(view, 6, 0);
  writeU16(view, 8, entryCount);
  writeU16(view, 10, entryCount);
  writeU32(view, 12, centralSize);
  writeU32(view, 16, centralOffset);
  writeU16(view, 20, 0);
  return record;
}

async function buildStoredZipBlobFromEntries(entries, onProgress) {
  const payloadParts = [];
  const centralParts = [];
  let localOffset = 0;
  const timestamp = dosTimestamp(new Date());

  if (entries.length > 0xffff) {
    throw new Error("too many files for standard ZIP export");
  }

  for (let index = 0; index < entries.length; index += 1) {
    const entry = entries[index];
    if (typeof onProgress === "function") {
      onProgress(index + 1, entries.length, entry.name);
    }
    const nameBytes = textEncoder.encode(entry.name);
    if (nameBytes.length > 0xffff) {
      throw new Error(`zip entry name is too long: ${entry.name}`);
    }

    const size = entry.blob.size;
    if (size > 0xffffffff) {
      throw new Error(`zip entry is too large: ${entry.name}`);
    }

    const crc = await crc32Blob(entry.blob);
    const localHeader = buildZipLocalHeader(
      nameBytes,
      crc,
      size,
      timestamp.dosDate,
      timestamp.dosTime,
    );
    const centralHeader = buildZipCentralHeader(
      nameBytes,
      crc,
      size,
      timestamp.dosDate,
      timestamp.dosTime,
      localOffset,
    );
    payloadParts.push(localHeader, entry.blob);
    centralParts.push(centralHeader);
    localOffset += localHeader.byteLength + size;
  }

  const centralSize = centralParts.reduce(
    (sum, part) => sum + part.byteLength,
    0,
  );
  const endRecord = buildZipEndRecord(entries.length, centralSize, localOffset);
  return new Blob([...payloadParts, ...centralParts, endRecord], {
    type: "application/zip",
  });
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
  const rowsWithExact = guesses.filter(
    (row) => Array.isArray(row?.exact_matches) && row.exact_matches.length > 0,
  ).length;
  const rowsWithCandidates = guesses.filter(
    (row) => Array.isArray(row?.candidates) && row.candidates.length > 0,
  ).length;
  return [
    `redactions guessed: ${guesses.length}`,
    `rows with exact matches: ${rowsWithExact}`,
    `rows with candidate lists: ${rowsWithCandidates}`,
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

function formatMaybeNumber(value, digits = 2) {
  const number = Number(value);
  return Number.isFinite(number) ? number.toFixed(digits) : "—";
}

function valueOrDash(value) {
  if (value == null) {
    return "—";
  }
  const text = String(value).trim();
  return text === "" ? "—" : text;
}

function exactMatchesText(row) {
  if (!Array.isArray(row?.exact_matches) || row.exact_matches.length === 0) {
    return "—";
  }
  return row.exact_matches.join("; ");
}

function topCandidate(row) {
  if (!Array.isArray(row?.candidates) || row.candidates.length === 0) {
    return null;
  }
  return row.candidates[0];
}

function anchorFontDisplay(context) {
  const key = valueOrDash(context?.anchor_font_key);
  const name = valueOrDash(context?.anchor_font_name);
  if (key === "—" && name === "—") {
    return "—";
  }
  if (key === "—") {
    return name;
  }
  if (name === "—") {
    return key;
  }
  return `${key} (${name})`;
}

function guessBboxLabel(bbox) {
  const x0 = normalizeNumber(bbox?.x0, 0);
  const y0 = normalizeNumber(bbox?.y0, 0);
  const x1 = normalizeNumber(bbox?.x1, 0);
  const y1 = normalizeNumber(bbox?.y1, 0);
  const width = Math.max(0, x1 - x0);
  const height = Math.max(0, y1 - y0);
  return `x=${x0.toFixed(1)}..${x1.toFixed(1)}, y=${y0.toFixed(1)}..${y1.toFixed(1)}, w=${width.toFixed(1)}, h=${height.toFixed(1)}`;
}

function buildFoundRedactionRows(report) {
  const guesses = Array.isArray(report?.guesses) ? report.guesses : [];
  return guesses.map((row, index) => {
    const bestCandidate = topCandidate(row);
    const context = row?.context ?? {};
    return {
      rowNumber: index + 1,
      pageNumber: normalizeNumber(row?.page_index, 0) + 1,
      guessText: topGuessText(row),
      exactMatches: exactMatchesText(row),
      candidateCount: Array.isArray(row?.candidates)
        ? row.candidates.length
        : 0,
      topScore: formatMaybeNumber(bestCandidate?.score, 4),
      topErrorPt: formatMaybeNumber(bestCandidate?.error_pt, 3),
      topWidthPt: formatMaybeNumber(bestCandidate?.width_pt, 3),
      bboxLabel: guessBboxLabel(row?.bbox),
      gapPt: formatMaybeNumber(context.gap_pt, 2),
      anchorMode: valueOrDash(context.anchor_mode),
      anchorFont: anchorFontDisplay(context),
      confidence: formatMaybeNumber(context.confidence_score, 3),
      visualMeanAbsDiff: formatMaybeNumber(row?.visual_mean_abs_diff, 3),
      visualChangedRatio: formatMaybeNumber(row?.visual_changed_pixel_ratio, 4),
      visualDropped: row?.visual_dropped ? "yes" : "no",
      leftContext: valueOrDash(context.left_anchor_text),
      rightContext: valueOrDash(context.right_anchor_text),
    };
  });
}

function appendTextCell(row, text, className = "") {
  const cell = document.createElement("td");
  if (className) {
    cell.className = className;
  }
  cell.textContent = text;
  row.appendChild(cell);
}

function appendGuessCell(row, guessText) {
  appendTextCell(row, guessText, "cell-top-guess");
}

function buildFoundRedactionsTable(rows) {
  const columns = [
    "Row",
    "Page",
    "Top Guess",
    "Exact Matches",
    "Candidate Count",
    "Top Score",
    "Top Error (pt)",
    "Top Width (pt)",
    "BBox (pt)",
    "Gap (pt)",
    "Anchor Mode",
    "Anchor Font",
    "Confidence",
    "Visual MAD",
    "Visual Changed",
    "Visual Dropped",
    "Left Context",
    "Right Context",
  ];

  const wrapper = document.createElement("div");
  wrapper.className = "redaction-table-wrap";

  const table = document.createElement("table");
  table.className = "redaction-table";

  const head = document.createElement("thead");
  const headRow = document.createElement("tr");
  for (const column of columns) {
    const th = document.createElement("th");
    th.textContent = column;
    headRow.appendChild(th);
  }
  head.appendChild(headRow);
  table.appendChild(head);

  const body = document.createElement("tbody");
  for (const rowData of rows) {
    const row = document.createElement("tr");
    appendTextCell(row, String(rowData.rowNumber));
    appendTextCell(row, String(rowData.pageNumber));
    appendGuessCell(row, rowData.guessText);
    appendTextCell(row, rowData.exactMatches, "cell-exact-matches");
    appendTextCell(row, String(rowData.candidateCount));
    appendTextCell(row, rowData.topScore);
    appendTextCell(row, rowData.topErrorPt);
    appendTextCell(row, rowData.topWidthPt);
    appendTextCell(row, rowData.bboxLabel);
    appendTextCell(row, rowData.gapPt);
    appendTextCell(row, rowData.anchorMode);
    appendTextCell(row, rowData.anchorFont);
    appendTextCell(row, rowData.confidence);
    appendTextCell(row, rowData.visualMeanAbsDiff);
    appendTextCell(row, rowData.visualChangedRatio);
    appendTextCell(row, rowData.visualDropped);
    appendTextCell(row, rowData.leftContext, "cell-context");
    appendTextCell(row, rowData.rightContext, "cell-context");
    body.appendChild(row);
  }
  table.appendChild(body);

  wrapper.appendChild(table);
  return wrapper;
}

function renderGuessVisualization(report) {
  const rows = buildFoundRedactionRows(report);
  if (rows.length === 0) {
    clearGuessVisualization("No redactions were found.");
    return;
  }

  guessVisualizationElement.innerHTML = "";
  guessVisualizationElement.classList.remove("empty-state");
  guessVisualizationElement.appendChild(buildFoundRedactionsTable(rows));
}

function downloadAnchorFromUrl(fileName, url) {
  const link = document.createElement("a");
  link.className = "download-link";
  link.href = url;
  link.download = fileName;
  link.textContent = `Download ${fileName}`;
  return link;
}

function triggerBlobDownload(fileName, blob) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = fileName;
  link.style.display = "none";
  document.body.appendChild(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 30_000);
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

function zipFolderNameForResult(result, sequenceIndex) {
  const order = String(sequenceIndex + 1).padStart(4, "0");
  return `results/${order}_${outputBaseName(result.label, result.id)}`;
}

async function collectBatchZipEntries(results, onProgress) {
  const entries = [];
  const usedPaths = new Set();
  const manifestFiles = [];

  for (let index = 0; index < results.length; index += 1) {
    const result = results[index];
    if (typeof onProgress === "function") {
      onProgress(index + 1, results.length, result.label);
    }

    const stored = await idbGetOutput(result.id);
    if (!stored) {
      throw new Error(`stored output missing for: ${result.label}`);
    }

    const folder = safeZipPath(zipFolderNameForResult(result, index));
    const addFile = (name, blob) => {
      if (!blob || !name) {
        return null;
      }
      const path = makeUniqueZipPath(`${folder}/${name}`, usedPaths);
      entries.push({ name: path, blob });
      return path;
    };

    const manifestItem = {
      result_id: result.id,
      label: result.label,
      status: result.status,
      output_files: {
        redactions: addFile(
          result.outputNames?.redactions,
          stored.redactionsBlob,
        ),
        fonts: addFile(result.outputNames?.fonts, stored.fontsBlob),
        guesses: addFile(result.outputNames?.guesses, stored.guessesBlob),
        visualized: addFile(
          result.outputNames?.visualized,
          stored.visualizedBlob,
        ),
      },
    };
    manifestFiles.push(manifestItem);
  }

  const manifest = {
    generated_at_utc: new Date().toISOString(),
    successful_results: results.length,
    files: manifestFiles,
  };
  const manifestBlob = new Blob([`${JSON.stringify(manifest, null, 2)}\n`], {
    type: "application/json",
  });
  entries.unshift({
    name: makeUniqueZipPath("results/batch_manifest.json", usedPaths),
    blob: manifestBlob,
  });

  return entries;
}

function batchZipFileName() {
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  return `unredact-results-${stamp}.zip`;
}

async function downloadBatchZip() {
  if (isRunning || isExportingBatchZip) {
    return;
  }

  const results = successfulBatchResults();
  if (results.length === 0) {
    setStatus("No successful results are available to export.");
    updateBatchZipButtonState();
    return;
  }

  isExportingBatchZip = true;
  updateBatchZipButtonState();
  try {
    setStatus(`Preparing ZIP entries (0/${results.length})...`);
    const entries = await collectBatchZipEntries(
      results,
      (current, total, label) => {
        setStatus(`Preparing ZIP entries (${current}/${total}): ${label}`);
      },
    );

    let lastProgressUpdate = -1;
    const zipBlob = await buildStoredZipBlobFromEntries(
      entries,
      (current, total, entryName) => {
        const shouldLog =
          current === 1 ||
          current === total ||
          current - lastProgressUpdate >= 10;
        if (shouldLog) {
          lastProgressUpdate = current;
          setStatus(`Building ZIP (${current}/${total}): ${entryName}`);
        }
      },
    );

    const fileName = batchZipFileName();
    triggerBlobDownload(fileName, zipBlob);
    setStatus(
      `Batch ZIP downloaded: ${fileName} (${formatBytes(zipBlob.size)})`,
    );
  } catch (error) {
    setStatus(`Failed to build batch ZIP: ${error}`);
  } finally {
    isExportingBatchZip = false;
    updateBatchZipButtonState();
  }
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
    updateBatchZipButtonState();
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
      inspectButton.textContent = "Show Visualized";
      inspectButton.addEventListener("click", () => {
        void selectResult(result.id);
      });
      actions.appendChild(inspectButton);
    }
    row.appendChild(actions);

    list.appendChild(row);
  }

  batchResultsElement.appendChild(list);
  updateBatchZipButtonState();
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
  updateBatchZipButtonState();
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

downloadBatchZipButton.addEventListener("click", () => {
  void downloadBatchZip();
});

boot();
