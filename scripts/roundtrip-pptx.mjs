#!/usr/bin/env node
// Round-trip and fidelity report over a presentation corpus (Ubu Office Phase 5, P2).
//
//   node scripts/roundtrip-pptx.mjs <roundtrip-binary> <corpus-dir> > ROUNDTRIP.md
//
// Runs ubu-slides' `roundtrip` example once per presentation under <corpus-dir> (PPTX, PPTM,
// PPSX, POTX, and old PPT, which the engine refuses: D26), each in its own process with a
// timeout, so a crash or hang is recorded as a result instead of stopping the run. Report-only:
// the exit code is 0 whatever the results, unless the runner itself is broken (no files, binary
// missing). The P2 gate (whether the viewer opens to admins) is decided by a person reading it.
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { extname, join, relative } from "node:path";

const [bin, dir] = process.argv.slice(2);
if (!bin || !dir || !existsSync(bin)) {
  console.error("usage: roundtrip-pptx.mjs <roundtrip-binary> <corpus-dir>");
  process.exit(2);
}
const TIMEOUT_MS = 60_000;
// Each file gets at most this much address space (KiB), so a presentation that expands without
// bound is recorded as that file's crash instead of the runner killing the whole job (as in the
// spreadsheet run). Linux only (ulimit -v); elsewhere the file runs unbounded.
const MEMORY_KIB = 3 * 1024 * 1024;
const KINDS = [".pptx", ".pptm", ".ppsx", ".potx", ".ppt"];

function walk(d) {
  return readdirSync(d).flatMap((name) => {
    const p = join(d, name);
    return statSync(p).isDirectory() ? walk(p) : KINDS.includes(extname(name).toLowerCase()) ? [p] : [];
  });
}
const files = walk(dir).sort();
if (!files.length) {
  console.error(`no presentations under ${dir}`);
  process.exit(2);
}

const results = [];
for (const file of files) {
  const t0 = Date.now();
  const bounded = process.platform === "linux";
  const r = bounded
    ? spawnSync("sh", ["-c", `ulimit -v ${MEMORY_KIB} && exec "$0" "$1"`, bin, file], { encoding: "utf8", timeout: TIMEOUT_MS, maxBuffer: 16 * 1024 * 1024 })
    : spawnSync(bin, [file], { encoding: "utf8", timeout: TIMEOUT_MS, maxBuffer: 16 * 1024 * 1024 });
  if (results.length % 25 === 0) console.error(`${results.length}/${files.length} ${relative(dir, file)}`);
  const name = relative(dir, file).replaceAll("\\", "/");
  const line = (r.stdout || "").trim().split("\n").pop();
  let res;
  if (r.error?.code === "ETIMEDOUT" || r.signal === "SIGTERM") res = { stage: "timeout", ok: false, error: `no result after ${TIMEOUT_MS / 1000} s` };
  else if (r.status !== 0 || !line) {
    // The panic message (the engine's own hook logs nothing natively; Rust prints it to stderr).
    const why = (r.stderr || "").trim().split("\n").filter(Boolean).slice(-3).join(" / ");
    res = { stage: "crash", ok: false, error: `exit ${r.status ?? r.signal}${why ? `: ${why}` : ""}` };
  } else {
    try {
      res = JSON.parse(line);
    } catch {
      res = { stage: "crash", ok: false, error: `unparseable output: ${line.slice(0, 200)}` };
    }
  }
  if (r.status !== 0 && r.stderr && /memory allocation|out of memory/i.test(r.stderr)) res = { stage: "memory", ok: false, error: `more than ${MEMORY_KIB / 1024 / 1024} GB of memory` };
  results.push({ name, kind: extname(name).toLowerCase().slice(1), ms: Date.now() - t0, ...res });
}

const n = results.length;
const ok = results.filter((r) => r.ok);
const crashes = results.filter((r) => r.stage === "crash" || r.stage === "timeout" || r.stage === "memory");
const ooxml = results.filter((r) => r.kind !== "ppt");
const ooxmlCrashes = crashes.filter((r) => r.kind !== "ppt");
const refused = results.filter((r) => r.stage === "open");
const refusedPpt = refused.filter((r) => r.kind === "ppt" && r.code === "unsupported");
const opened = results.filter((r) => !["read", "open", "crash", "timeout", "memory"].includes(r.stage));
// Opened, every slide rendered with its text layer, shapes, text and notes read, and the PDF exported.
const viewed = opened.filter((r) => !["render", "text", "pdf"].includes(r.stage));
const slidesLost = ok.filter((r) => r.slidesAfter !== r.slides);
const slidesChanged = ok.filter((r) => r.slidesChanged > 0);
const lossy = results.filter((r) => Array.isArray(r.lost) && r.lost.length);
const withBlank = opened.filter((r) => r.blank > 0);
const withWarnings = opened.filter((r) => r.warnings > 0);
const lostKinds = new Map();
for (const r of lossy) for (const k of r.lost) lostKinds.set(k, (lostKinds.get(k) ?? 0) + 1);
const byStage = new Map();
for (const r of results.filter((r) => !r.ok)) byStage.set(r.stage, (byStage.get(r.stage) ?? 0) + 1);
const byKind = new Map();
for (const r of results) {
  const k = byKind.get(r.kind) ?? { n: 0, ok: 0 };
  k.n++;
  if (r.ok) k.ok++;
  byKind.set(r.kind, k);
}
const pct = (k, of = n) => `${((100 * k) / Math.max(of, 1)).toFixed(1)}%`;
const esc = (s) => String(s ?? "").replaceAll("|", "\\|").replaceAll("\n", " ").slice(0, 300);
const sorted = (xs) => xs.slice().sort((a, b) => a - b);
const openTimes = sorted(opened.map((r) => r.openMs ?? 0));
const pctile = (xs, p) => (xs.length ? xs[Math.min(xs.length - 1, Math.floor((p / 100) * xs.length))] : 0);
const slides = opened.reduce((s, r) => s + (r.slides ?? 0), 0);
const perSlide = sorted(opened.filter((r) => r.slides > 0 && r.renderMs !== undefined).map((r) => Math.round(r.renderMs / r.slides)));

const out = [
  "# Round-trip and fidelity report: ubu-slides",
  "",
  "Each file: open → every slide rendered at 0.5 px per point, its text layer, shapes, text and",
  "notes → export PDF → save PPTX → reopen the copy → same slides, each slide's text, notes and",
  "shape count the same. One process per file,",
  `${TIMEOUT_MS / 1000} s timeout, ${MEMORY_KIB / 1024 / 1024} GB of memory, \`panic = abort\` as in the browser. Generated by \`scripts/roundtrip-pptx.mjs\`.`,
  "",
  `- Files: **${n}** (${[...byKind].map(([k, v]) => `${k} ${v.ok}/${v.n}`).join(", ")} round-tripped)`,
  `- PowerPoint 2007+ files (pptx, pptm, ppsx, potx): **${ooxml.length}**; crashes, hangs or memory blow-ups among them: **${ooxmlCrashes.length}**`,
  `- Old .ppt files refused as \`unsupported\` (D26): **${refusedPpt.length}** of ${results.filter((r) => r.kind === "ppt").length}`,
  `- Opened and viewed (every slide rendered, text layer, shapes, text, notes; PDF): **${viewed.length}** (${pct(viewed.length, ooxml.length)} of 2007+ files), ${slides} slides`,
  `- Full round trip (saved copy reopens): **${ok.length}** (${pct(ok.length, ooxml.length)} of 2007+ files)`,
  `- Crashes, hangs or memory blow-ups, all files: **${crashes.length}** (${pct(crashes.length)})`,
  `- Failed or refused: **${n - ok.length}**`,
  ...[...byStage].sort((a, b) => b[1] - a[1]).map(([stage, k]) => `  - at \`${stage}\`: ${k}`),
  `- Saved copy with a different number of slides: **${slidesLost.length}**; with slides whose text, notes or shape count changed: **${slidesChanged.length}**`,
  `- Saved copy that lost content (what the P8 check must warn about): **${lossy.length}**`,
  ...[...lostKinds].sort((a, b) => b[1] - a[1]).map(([kind, k]) => `  - ${kind}: ${k}`),
  `- Opened with a warning (slides left out, media that plays only in PowerPoint): **${withWarnings.length}**`,
  `- Files with a slide that rendered as one flat colour: **${withBlank.length}** (blank slides exist; a finding to look at, not a failure)`,
  `- Open time, opened files: median ${pctile(openTimes, 50)} ms, 95th percentile ${pctile(openTimes, 95)} ms, slowest ${openTimes.at(-1) ?? 0} ms`,
  `- Render time per slide at 0.5 px/pt (native): median ${pctile(perSlide, 50)} ms, 95th percentile ${pctile(perSlide, 95)} ms`,
  "",
  "Corpus files are often unusual on purpose (many come from bug reports), so a failure here is a",
  "finding to triage, not necessarily a bug. Fonts are the engine's compiled-in last-resort face",
  "only (the viewer adds the open look-alikes, D25), so text widths differ from PowerPoint here.",
  "",
  "## Failures and refusals",
  "",
  "| File | Stage | Error |",
  "|---|---|---|",
  ...results.filter((r) => !r.ok).map((r) => `| \`${esc(r.name)}\` | ${r.stage}${r.code ? ` (${r.code})` : ""} | ${esc(r.error)} |`),
  "",
  "## Saved copies that changed",
  "",
  "| File | Slides | Slides after | Slides changed | First change | Lost |",
  "|---|---|---|---|---|---|",
  ...results
    .filter((r) => r.ok && (r.slidesAfter !== r.slides || r.slidesChanged > 0 || r.lost?.length))
    .map((r) => `| \`${esc(r.name)}\` | ${r.slides} | ${r.slidesAfter} | ${r.slidesChanged} | ${esc(r.firstChange)} | ${esc((r.lost ?? []).join(", "))} |`),
  "",
].join("\n");
process.stdout.write(out);
console.error(`pptx round trip: ${ok.length}/${n} ok, ${crashes.length} crashes or hangs (${ooxmlCrashes.length} on 2007+ files), ${slidesChanged.length} with changed slides, ${lossy.length} lossy saves`);
