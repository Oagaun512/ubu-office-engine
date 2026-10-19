#!/usr/bin/env node
// Runs the presentation engine's browser build inside a worker thread, the way Ubunex runs it
// inside a Web Worker. The worker has no `window` or `document`: anything in the engine that
// reached for them would fail here.
//
//   node scripts/pptx-worker-smoke.mjs <pkg-dir> <sample.pptx>
//
// It opens the sample (examples/make_sample.rs), reads the slides, renders the first one and
// its text layer, adds a slide and undoes it, saves PPTX and reopens it, exports PDF, and
// reports the timings.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";

if (isMainThread) {
  const [pkg, sample] = process.argv.slice(2);
  if (!pkg || !sample) {
    console.error("usage: pptx-worker-smoke.mjs <pkg-dir> <sample.pptx>");
    process.exit(2);
  }
  const worker = new Worker(new URL(import.meta.url), { workerData: { pkg: resolve(pkg), sample: resolve(sample) } });
  worker.on("message", (m) => {
    console.log(m.ok ? `pptx worker smoke test passed: ${m.report}` : `pptx worker smoke test FAILED: ${m.error}`);
    process.exitCode = m.ok ? 0 : 1;
  });
  worker.on("error", (e) => {
    console.error(`pptx worker smoke test FAILED (worker error): ${e.stack ?? e}`);
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
    const glue = await import(pathToFileURL(resolve(pkg, "ubu_slides.js")).href);
    await glue.default({ module_or_path: readFileSync(resolve(pkg, "ubu_slides_bg.wasm")) });
    const t1 = performance.now();
    const deck = glue.SlideDeck.open(readFileSync(sample), "sample.pptx");
    const t2 = performance.now();
    const info = JSON.parse(deck.info());
    check(info.slides === 2 && info.width > info.height, `2 wide slides, got ${JSON.stringify(info)}`);
    const slides = JSON.parse(deck.slides());
    check(slides[0].notes === true && deck.notes(0) === "Speaker notes", `notes, got ${JSON.stringify(slides)}`);
    const pic = deck.render(0, 1);
    const pixels = pic.pixels;
    const t3 = performance.now();
    check(pic.width === Math.round(info.width) && pixels.length === pic.width * pic.height * 4, `a slide at 1 px/pt, got ${pic.width}×${pic.height}`);
    let ink = 0;
    for (let i = 0; i < pixels.length; i += 4) if (pixels[i] < 128) ink++;
    check(ink > 50, `the slide has dark ink (text), counted ${ink}`);
    pic.free();
    const spans = JSON.parse(deck.textLayer(0));
    check(spans.some((s) => s.text.includes("Ubu Slides") && s.x2 > s.x), `the text layer, got ${JSON.stringify(spans)}`);
    check(deck.slideText(0).includes("Ubu Slides"), "the slide's text");
    const added = JSON.parse(deck.run("slide.new", JSON.stringify({ layout: "blank" })));
    check(added.changed && added.slides === 3 && deck.isDirty && deck.canUndo, `a new slide, got ${JSON.stringify(added)}`);
    check(JSON.parse(deck.run("edit.undo", "")).slides === 2, "undo");
    let refused = false;
    try {
      deck.run("no.such.command", "{}");
    } catch (e) {
      refused = e.name === "SlidesError" && e.code === "invalid_command";
    }
    check(refused, "an unknown command is a SlidesError, not a trap");
    const pptx = deck.savePptx();
    check(pptx[0] === 0x50 && pptx[1] === 0x4b, "PPTX save (a zip)");
    const again = glue.SlideDeck.open(pptx, "copy.pptx");
    check(JSON.parse(again.info()).slides === 2 && again.slideText(0) === deck.slideText(0), "the saved copy reopens with its slides and text");
    again.free();
    const pdf = deck.exportPdf();
    check(String.fromCharCode(...pdf.subarray(0, 5)) === "%PDF-", "PDF export");
    const version = glue.engineVersion();
    deck.free();
    const ms = (a, b) => `${Math.round(b - a)} ms`;
    parentPort.postMessage({
      ok: true,
      report: `init ${ms(t0, t1)}, open ${ms(t1, t2)}, render ${ms(t2, t3)}, pptx ${pptx.length} B, pdf ${pdf.length} B; ${version}`,
    });
  } catch (e) {
    parentPort.postMessage({ ok: false, error: e.stack ?? String(e) });
  }
}
