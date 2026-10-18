#!/usr/bin/env node
// Our root Cargo.toml repeats pdfcraft's [patch.crates-io] entries (patches only apply from the
// root of the workspace being built). Fails when they drift: a pdfcraft patch missing here, a
// path that differs, or a patch here that pdfcraft doesn't have. UI-only patches are exempt.
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
