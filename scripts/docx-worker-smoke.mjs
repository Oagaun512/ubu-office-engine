#!/usr/bin/env node
// Runs the DOCX engine's browser build inside a worker thread, the way Ubunex runs it inside a
// Web Worker (upstream wordcraft only ever runs on the page's main thread). The worker has no
// `window` or `document`: anything in the engine that reached for them would fail here.
//
//   node scripts/docx-worker-smoke.mjs <pkg-dir> <sample.docx>
//
// It opens the sample, checks the page count, renders page 1 (with ink on it), reads the text,
// exports PDF and saves DOCX, and reports the timings.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

if (isMainThread) {
  const [pkg, sample] = process.argv.slice(2);
  if (!pkg || !sample) {
    console.error("usage: docx-worker-smoke.mjs <pkg-dir> <sample.docx>");
    process.exit(2);
  }
  const worker = new Worker(new URL(import.meta.url), { workerData: { pkg: resolve(pkg), sample: resolve(sample) } });
  worker.on("message", (m) => {
    console.log(m.ok ? `docx worker smoke test passed: ${m.report}` : `docx worker smoke test FAILED: ${m.error}`);
    process.exitCode = m.ok ? 0 : 1;
  });
  worker.on("error", (e) => {
    console.error(`docx worker smoke test FAILED (worker error): ${e.stack ?? e}`);
    process.exitCode = 1;
  });
} else {
  const { pkg, sample } = workerData;
  const check = (cond, what) => {
    if (!cond) throw new Error(what);
  };
  try {
    check(typeof globalThis.window === "undefined" && typeof globalThis.document === "undefined", "a worker has no window");
    const t0 = performance.now();
    const glue = await import(pathToFileURL(resolve(pkg, "ubu_docx.js")).href);
    await glue.default({ module_or_path: readFileSync(resolve(pkg, "ubu_docx_bg.wasm")) });
    const t1 = performance.now();
    const doc = glue.DocxDocument.open(readFileSync(sample), "sample.docx", null);
    const t2 = performance.now();
    check(doc.pageCount === 2, `2 pages, got ${doc.pageCount}`);
    const size = doc.pageSize(0);
    check(size.widthPt > 500 && size.heightPt > 700, `a full page, got ${size.widthPt}x${size.heightPt}`);
    size.free();
    const page = doc.renderPage(0, 1);
    const pageWidth = page.width;
    const rgba = page.rgba();
    let dark = 0;
    for (let i = 0; i < rgba.length; i += 4) if (rgba[i] < 100) dark++;
    check(dark > 50, `ink on page 1 (${dark} dark pixels)`);
    const t3 = performance.now();
    check(doc.pageText(1).includes("Hello Ubu 2"), `page 2's text, got ${JSON.stringify(doc.pageText(1))}`);
    const pdf = doc.exportPdf();
    check(String.fromCharCode(...pdf.subarray(0, 5)) === "%PDF-", "PDF export");
    const docx = doc.saveDocx();
    check(docx[0] === 0x50 && docx[1] === 0x4b, "DOCX save (a zip)");
    check(glue.addFont(new Uint8Array([1, 2, 3])) === 0, "garbage is not a font");
    doc.relayout();
    const version = glue.engineVersion();
    page.free();
    doc.free();
    const ms = (a, b) => `${Math.round(b - a)} ms`;
    parentPort.postMessage({
      ok: true,
      report: `init ${ms(t0, t1)}, open ${ms(t1, t2)}, render p1 ${ms(t2, t3)} (${pageWidth}px wide), pdf ${pdf.length} B, docx ${docx.length} B; ${version}`,
    });
  } catch (e) {
    parentPort.postMessage({ ok: false, error: e.stack ?? String(e) });
  }
}
