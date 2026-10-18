#!/usr/bin/env node
// Our root Cargo.toml repeats pdfcraft's [patch.crates-io] entries (patches only apply from the
// root of the workspace being built). Fails when they drift: a pdfcraft patch missing here, a
// path that differs, or a patch here that pdfcraft doesn't have. UI-only patches are exempt.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const UI_ONLY = new Set(["winit", "egui-wgpu"]);
const PREFIX = "upstream/pdfcraft/";

function patches(file) {
  const text = readFileSync(file, "utf8");
  const section = text.split(/^\[patch\.crates-io\]\s*$/m)[1];
  if (!section) throw new Error(`${file}: no [patch.crates-io] section`);
  const body = section.split(/^\[/m)[0];
  const out = new Map();
  for (const line of body.split("\n")) {
    const m = line.match(/^\s*([A-Za-z0-9_-]+)\s*=\s*\{\s*path\s*=\s*"([^"]+)"\s*\}/);
    if (m) out.set(m[1], m[2]);
  }
  return out;
}

const upstream = patches("upstream/pdfcraft/Cargo.toml");
const ours = patches("Cargo.toml");
const errors = [];
for (const [name, path] of upstream) {
  if (UI_ONLY.has(name)) continue;
  if (!ours.has(name)) errors.push(`missing patch: ${name} = { path = "${PREFIX}${path}" }`);
  else if (ours.get(name) !== PREFIX + path) errors.push(`${name}: ours points at ${ours.get(name)}, pdfcraft at ${PREFIX}${path}`);
}
for (const name of ours.keys()) {
  if (!upstream.has(name)) errors.push(`patch ${name} is not in pdfcraft's Cargo.toml`);
}
if (errors.length) {
  console.error("Cargo.toml [patch.crates-io] drifted from upstream/pdfcraft:\n  " + errors.join("\n  "));
  process.exit(1);
}
console.log(`patches in sync (${ours.size} crates; UI-only exempt: ${[...UI_ONLY].join(", ")})`);

// The DOCX workspace (docx/Cargo.toml) mounts wordcraft's craft-libs git dependencies from our
// craft-libs fork instead of fetching them. That is only the same code when the fork is checked
// out at the very commit wordcraft pins, for every craft-libs crate the engine links (craft-pdf,
// and krilla through wordcraft's [patch.crates-io]); and each must be redirected to a path.
const CRAFT_LIBS_GIT = "https://github.com/storytold/craft-libs";
const LINKED = { "craft-pdf": "../upstream/craft-libs/crates/pdf", krilla: "../upstream/craft-libs/vendor/krilla" };
const wordcraft = readFileSync("upstream/wordcraft/Cargo.toml", "utf8");
const docx = readFileSync("docx/Cargo.toml", "utf8");
const forkHead = execFileSync("git", ["-C", "upstream/craft-libs", "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
const docxErrors = [];
for (const [name, path] of Object.entries(LINKED)) {
  const pin = wordcraft.match(new RegExp(`^${name}\\s*=\\s*\\{[^}]*git\\s*=\\s*"${CRAFT_LIBS_GIT}"[^}]*rev\\s*=\\s*"([0-9a-f]{40})"`, "m"));
  if (!pin) docxErrors.push(`wordcraft no longer takes ${name} from ${CRAFT_LIBS_GIT}: review docx/Cargo.toml`);
  else if (pin[1] !== forkHead) docxErrors.push(`wordcraft pins ${name} at ${pin[1]}, upstream/craft-libs is at ${forkHead}`);
  if (!new RegExp(`^${name}\\s*=\\s*\\{\\s*path\\s*=\\s*"${path.replaceAll(".", "\\.")}"\\s*\\}`, "m").test(docx)) {
    docxErrors.push(`docx/Cargo.toml must patch ${name} = { path = "${path}" }`);
  }
}
const wordcraftPatches = (wordcraft.split(/^\[patch\.crates-io\]\s*$/m)[1] ?? "").split(/^\[/m)[0].match(/^[A-Za-z0-9_-]+(?=\s*=)/gm) ?? [];
for (const name of wordcraftPatches) if (!(name in LINKED)) docxErrors.push(`wordcraft patches ${name}, which docx/Cargo.toml does not repeat`);
if (docxErrors.length) {
  console.error("docx/Cargo.toml drifted from upstream/wordcraft:\n  " + docxErrors.join("\n  "));
  process.exit(1);
}
console.log(`docx patches in sync (craft-libs at ${forkHead.slice(0, 7)} for ${Object.keys(LINKED).join(", ")})`);
