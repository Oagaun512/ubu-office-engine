#!/usr/bin/env node
// Size report for built artifacts: raw, gzip -9 and brotli -11, as a Markdown table.
//   node scripts/size-report.mjs <file>... > SIZE.md
import { readFileSync } from "node:fs";
import { basename } from "node:path";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";

const files = process.argv.slice(2);
const fmt = (n) => (n >= 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(2)} MiB` : `${(n / 1024).toFixed(1)} KiB`);
const rows = files.map((f) => {
  const buf = readFileSync(f);
  const gz = gzipSync(buf, { level: 9 }).length;
  const br = brotliCompressSync(buf, { params: { [constants.BROTLI_PARAM_QUALITY]: 11 } }).length;
  return `| \`${basename(f)}\` | ${fmt(buf.length)} (${buf.length} B) | ${fmt(gz)} | ${fmt(br)} |`;
});
console.log(["| File | Raw | gzip -9 | brotli -11 |", "|---|---|---|---|", ...rows].join("\n"));
