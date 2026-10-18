#!/usr/bin/env node
// Checks a wasm-bindgen package against the reviewed TypeScript contract.
//
//   node scripts/check-exports.mjs <pkg-dir> <name> <contract.d.ts>
//
// 1. The generated <name>.d.ts public API equals the contract (comments and whitespace
//    ignored; the InitOutput interface, which lists raw wasm exports, is left out).
// 2. The .wasm exports are exactly the ones <name>_bg.wasm.d.ts declares.
// 3. The .wasm imports come only from the wasm-bindgen glue, and the glue provides each one.
//    An `env` or `wasi_*` import would mean the module cannot be instantiated in a browser.
import { readFileSync } from "node:fs";
import { join } from "node:path";

const [pkg, name, contractPath] = process.argv.slice(2);
if (!pkg || !name || !contractPath) {
  console.error("usage: check-exports.mjs <pkg-dir> <name> <contract.d.ts>");
  process.exit(2);
}

const errors = [];

function normalise(dts) {
  return dts
    .replace(/\r\n/g, "\n")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "")
    .replace(/export interface InitOutput \{[\s\S]*?\n\}/, "")
    .split("\n")
    .map((l) => l.trim().replace(/\s+/g, " "))
    .filter(Boolean);
}

// 1. Contract.
const generated = normalise(readFileSync(join(pkg, `${name}.d.ts`), "utf8"));
const contract = normalise(readFileSync(contractPath, "utf8"));
if (generated.join("\n") !== contract.join("\n")) {
  const gen = new Set(generated);
  const con = new Set(contract);
  const extra = generated.filter((l) => !con.has(l));
  const missing = contract.filter((l) => !gen.has(l));
  errors.push(
    `generated ${name}.d.ts differs from ${contractPath}` +
      (extra.length ? `\n  + generated only:\n    ${extra.join("\n    ")}` : "") +
      (missing.length ? `\n  - contract only:\n    ${missing.join("\n    ")}` : "") +
      (!extra.length && !missing.length ? "\n  (same lines, different order)" : ""),
  );
}

// 2. Exports.
const wasm = new WebAssembly.Module(readFileSync(join(pkg, `${name}_bg.wasm`)));
const exported = new Set(WebAssembly.Module.exports(wasm).map((e) => e.name));
const declared = new Set(
  [...readFileSync(join(pkg, `${name}_bg.wasm.d.ts`), "utf8").matchAll(/export (?:const|function) ([A-Za-z0-9_$]+)/g)].map((m) => m[1]),
);
for (const e of exported) if (!declared.has(e)) errors.push(`wasm export ${e} is not declared in ${name}_bg.wasm.d.ts`);
for (const d of declared) if (!exported.has(d)) errors.push(`${name}_bg.wasm.d.ts declares ${d}, which the wasm does not export`);

// 3. Imports.
const glue = readFileSync(join(pkg, `${name}.js`), "utf8");
const imports = WebAssembly.Module.imports(wasm);
const allowedModules = new Set(["wbg", `./${name}_bg.js`]);
for (const imp of imports) {
  if (!allowedModules.has(imp.module)) errors.push(`wasm imports ${imp.module}.${imp.name}: not provided by the wasm-bindgen glue`);
  else if (!glue.includes(imp.name)) errors.push(`wasm imports ${imp.module}.${imp.name}, which ${name}.js does not define`);
}

if (errors.length) {
  console.error(`export check FAILED for ${name}:\n- ` + errors.join("\n- "));
  process.exit(1);
}
console.log(`export check passed for ${name}: ${exported.size} exports, ${imports.length} imports (modules: ${[...new Set(imports.map((i) => i.module))].join(", ") || "none"}); API matches the contract`);
