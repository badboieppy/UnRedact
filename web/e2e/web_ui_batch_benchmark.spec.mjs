import fs from "node:fs/promises";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { expect, test } from "@playwright/test";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, "..", "..");
const webRoot = path.resolve(repoRoot, "web");
const benchmarkDir = path.resolve(repoRoot, "benchmark");
const wasmPath = path.resolve(webRoot, "pkg", "unredact_bg.wasm");
const samplePdfA = path.resolve(repoRoot, "test_data", "EFTA00101126.pdf");
const samplePdfB = path.resolve(repoRoot, "test_data", "EFTA00038617.pdf");

const mimeByExt = new Map([
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".css", "text/css; charset=utf-8"],
  [".json", "application/json; charset=utf-8"],
  [".wasm", "application/wasm"],
  [".pdf", "application/pdf"],
  [".txt", "text/plain; charset=utf-8"],
  [".map", "application/json; charset=utf-8"],
]);

function resolveWebPath(urlPath) {
  const clean = decodeURIComponent((urlPath ?? "/").split("?")[0]);
  const normalized = clean === "/" ? "/index.html" : clean;
  const filePath = path.resolve(webRoot, `.${normalized}`);
  if (!filePath.startsWith(webRoot)) {
    return null;
  }
  return filePath;
}

async function createWebServer() {
  const server = http.createServer(async (req, res) => {
    try {
      const resolved = resolveWebPath(req.url);
      if (!resolved) {
        res.statusCode = 403;
        res.end("forbidden");
        return;
      }
      const fileBytes = await fs.readFile(resolved);
      const ext = path.extname(resolved).toLowerCase();
      res.statusCode = 200;
      res.setHeader(
        "Content-Type",
        mimeByExt.get(ext) ?? "application/octet-stream",
      );
      res.end(fileBytes);
    } catch (error) {
      if (error && error.code === "ENOENT") {
        res.statusCode = 404;
        res.end("not found");
      } else {
        res.statusCode = 500;
        res.end(`server error: ${error}`);
      }
    }
  });

  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve());
  });
  const address = server.address();
  if (!address || typeof address === "string") {
    throw new Error("failed to bind web server");
  }
  const url = `http://127.0.0.1:${address.port}`;
  return { server, url };
}

test.describe("web ui batch benchmark", () => {
  let serverHandle = null;
  let baseUrl = "";

  test.beforeAll(async () => {
    const requiredPaths = [wasmPath, samplePdfA, samplePdfB];
    for (const required of requiredPaths) {
      const exists = await fs
        .stat(required)
        .then((value) => value.isFile())
        .catch(() => false);
      if (!exists) {
        throw new Error(
          `required file is missing: ${required}. Build wasm first and ensure test_data PDFs are present.`,
        );
      }
    }

    const created = await createWebServer();
    serverHandle = created.server;
    baseUrl = created.url;
  });

  test.afterAll(async () => {
    if (!serverHandle) {
      return;
    }
    await new Promise((resolve) => {
      serverHandle.close(() => resolve());
    });
    serverHandle = null;
  });

  test("runs batch flow and emits benchmark/memory panel metrics", async ({
    page,
  }) => {
    test.setTimeout(5 * 60 * 1000);

    await page.goto(baseUrl, { waitUntil: "networkidle" });
    await expect(page.locator("#status")).toContainText("Ready.", {
      timeout: 60_000,
    });

    await page.setInputFiles("#pdfFile", [samplePdfA, samplePdfB]);
    await page.click("#runButton");

    await expect(page.locator("#benchmarkSummary")).toContainText(
      "files processed: 2/2",
      {
        timeout: 4 * 60 * 1000,
      },
    );
    await expect(page.locator("#status")).toContainText("Done.", {
      timeout: 4 * 60 * 1000,
    });
    await expect(page.locator(".batch-row")).toHaveCount(2, {
      timeout: 4 * 60 * 1000,
    });

    const benchmarkText = await page.locator("#benchmarkSummary").innerText();
    expect(benchmarkText).toContain("avg file time:");
    expect(benchmarkText).toContain("output bytes stored:");
    expect(benchmarkText).toContain("input bytes read:");
    expect(
      benchmarkText.includes("js heap current:") ||
        benchmarkText.includes("js heap metrics: unavailable"),
    ).toBeTruthy();

    const summaryText = await page.locator("#summary").innerText();
    expect(summaryText).toContain("redactions guessed:");

    await fs.mkdir(benchmarkDir, { recursive: true });
    const artifact = {
      generated_at_utc: new Date().toISOString(),
      base_url: baseUrl,
      benchmark_summary: benchmarkText,
      summary: summaryText,
    };
    const outPath = path.join(
      benchmarkDir,
      "web_ui_batch_benchmark.latest.json",
    );
    await fs.writeFile(
      outPath,
      `${JSON.stringify(artifact, null, 2)}\n`,
      "utf8",
    );
  });
});
