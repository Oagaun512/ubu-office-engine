#!/usr/bin/env node
// Runs the spreadsheet engine's browser build inside a worker thread, the way Ubunex runs it
// inside a Web Worker. The worker has no `window` or `document`: anything in the engine that
// reached for them would fail here.
//
//   node scripts/xlsx-worker-smoke.mjs <pkg-dir> <sample.xlsx>
//
// It opens the sample (examples/make_sample.rs), reads the sheets, the layout and a window of
// cells, edits a cell and checks the formula recalculates, undoes, saves XLSX and CSV, exports
// PDF, and reports the timings.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

if (isMainThread) {
  const [pkg, sample] = process.argv.slice(2);
  if (!pkg || !sample) {
    console.error("usage: xlsx-worker-smoke.mjs <pkg-dir> <sample.xlsx>");
    process.exit(2);
  }
  const worker = new Worker(new URL(import.meta.url), { workerData: { pkg: resolve(pkg), sample: resolve(sample) } });
  worker.on("message", (m) => {
    console.log(m.ok ? `xlsx worker smoke test passed: ${m.report}` : `xlsx worker smoke test FAILED: ${m.error}`);
    process.exitCode = m.ok ? 0 : 1;
  });
  worker.on("error", (e) => {
    console.error(`xlsx worker smoke test FAILED (worker error): ${e.stack ?? e}`);
    process.exitCode = 1;
  });
} else {
  const { pkg, sample } = workerData;
  const check = (cond, what) => {
    if (!cond) throw new Error(what);
  };
  const cellAt = (win, row, col) => win.cells.find((c) => c[0] === row && c[1] === col);
  try {
    check(typeof globalThis.window === "undefined" && typeof globalThis.document === "undefined", "a worker has no window");
    const t0 = performance.now();
    const glue = await import(pathToFileURL(resolve(pkg, "ubu_sheets.js")).href);
    await glue.default({ module_or_path: readFileSync(resolve(pkg, "ubu_sheets_bg.wasm")) });
    const t1 = performance.now();
    const book = glue.SheetBook.open(readFileSync(sample), "sample.xlsx", null);
    const t2 = performance.now();
    const sheets = JSON.parse(book.sheets());
    check(sheets.length === 2 && book.activeSheet === 0, `2 sheets, the first active, got ${JSON.stringify(sheets)}`);
    const layout = JSON.parse(book.layout(0));
    check(JSON.stringify(layout.merges) === "[[0,0,0,2]]" && JSON.stringify(layout.freeze) === "[1,0]", `merge and freeze, got ${JSON.stringify(layout)}`);
    const win = JSON.parse(book.window(0, 0, 0, 20, 10, 7));
    const t3 = performance.now();
    const title = cellAt(win, 0, 0);
    check(title && title[2] === "Ubu Sheets", `the title, got ${JSON.stringify(title)}`);
    const look = win.styles[title[3]];
    check(look.bold === true && look.fill === "#FFFF00", `bold on yellow, got ${JSON.stringify(look)}`);
    check(cellAt(win, 1, 2)?.[2] === "6", `=A2*B2 is 6, got ${JSON.stringify(cellAt(win, 1, 2))}`);
    check(book.cellInput(0, 1, 2) === "=A2*B2", "the formula bar shows the formula");
    const edit = JSON.parse(book.run("cell.set", JSON.stringify({ cell: "A2", input: "5" })));
    check(edit.changed === true && book.isDirty && book.canUndo, `an edit, got ${JSON.stringify(edit)}`);
    check(cellAt(JSON.parse(book.window(0, 1, 2, 1, 2, 7)), 1, 2)?.[2] === "15", "the formula recalculates");
    book.run("edit.undo", "");
    check(cellAt(JSON.parse(book.window(0, 1, 2, 1, 2, 7)), 1, 2)?.[2] === "6", "undo");
    let refused = false;
    try {
      book.run("no.such.command", "{}");
    } catch (e) {
      refused = e.name === "SheetsError" && e.code === "invalid_command";
    }
    check(refused, "an unknown command is a SheetsError, not a trap");
    const xlsx = book.saveXlsx();
    check(xlsx[0] === 0x50 && xlsx[1] === 0x4b, "XLSX save (a zip)");
    const again = glue.SheetBook.open(xlsx, "copy.xlsx", null);
    check(cellAt(JSON.parse(again.window(0, 0, 0, 1, 2, 7)), 1, 2)?.[2] === "6", "the saved copy reopens with its values");
    again.free();
    const csv = new TextDecoder().decode(book.saveCsv(0));
    check(csv.includes("Ubu Sheets") && csv.includes("6"), `CSV, got ${JSON.stringify(csv)}`);
    check(book.pdfUnwritable(0) === undefined, "the sample is all Windows-1252");
    const pdf = book.exportPdf();
    check(String.fromCharCode(...pdf.subarray(0, 5)) === "%PDF-", "PDF export");
    const version = glue.engineVersion();
    book.free();
    const ms = (a, b) => `${Math.round(b - a)} ms`;
    parentPort.postMessage({
      ok: true,
      report: `init ${ms(t0, t1)}, open ${ms(t1, t2)}, layout+window ${ms(t2, t3)}, xlsx ${xlsx.length} B, pdf ${pdf.length} B; ${version}`,
    });
  } catch (e) {
    parentPort.postMessage({ ok: false, error: e.stack ?? String(e) });
  }
}
