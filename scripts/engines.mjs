// What each engine's browser build is made of, for the SBOM and licence scripts.
//
// - manifest: the workspace the engine crate lives in (the DOCX engine has its own; see
//   docx/Cargo.toml).
// - forks: the upstream forks whose own licence files cover crates that carry none (path crates
//   inside the fork); their LICENSE/NOTICE/ATTRIBUTION files are copied into <out>/<name>/.
// - assets: bundled-asset licences cargo cannot see, copied into <out>/assets/.
// - note: the line THIRD-PARTY.md gives about the fork-covered crates.
import { relative, resolve } from "node:path";

const inside = (dir) => (p) => p.source === null && !relative(resolve(dir), resolve(p.manifest_path)).startsWith("..");

export const ENGINES = {
  "ubu-pdf": {
    manifest: "Cargo.toml",
    forks: [
      {
        name: "pdfcraft",
        dir: "upstream/pdfcraft",
        files: ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "ATTRIBUTION.md"],
        covers: (p) => p.source === null && p.name.startsWith("pdfcraft-"),
      },
    ],
    assets: [["upstream/pdfcraft/vendor/hayro-interpret/assets/LICENSE_FOXIT", "LICENSE_FOXIT"]],
    note: "The pdfcraft-* crates are covered by pdfcraft/ next to this file.",
  },
  "ubu-docx": {
    manifest: "docx/Cargo.toml",
    forks: [
      {
        name: "wordcraft",
        dir: "upstream/wordcraft",
        files: ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "ATTRIBUTION.md"],
        covers: inside("upstream/wordcraft"),
      },
      {
        name: "craft-libs",
        dir: "upstream/craft-libs",
        files: ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE"],
        covers: inside("upstream/craft-libs"),
      },
    ],
    // The OFL faces wordcraft compiles in (crates/fonts/src/fontdb.rs, BUNDLED).
    assets: ["SourceSans3", "SourceSerif4", "Inter", "JetBrainsMono"].map((f) => [`upstream/wordcraft/assets/fonts/OFL-${f}.txt`, `OFL-${f}.txt`]),
    note:
      "The wordcraft-* crates are covered by wordcraft/, and craft-pdf and krilla by craft-libs/, next to this file. " +
      "The fonts compiled into the engine are under the SIL Open Font License, in assets/.",
  },
  "ubu-sheets": {
    manifest: "xlsx/Cargo.toml",
    forks: [
      {
        name: "gridcraft",
        dir: "upstream/gridcraft",
        files: ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "ATTRIBUTION.md"],
        covers: inside("upstream/gridcraft"),
      },
    ],
    // The PDF export's standard-14 widths come from Adobe's AFM metrics (crates/pdf/src/metrics.rs).
    assets: [["licenses/adobe-core14-afm.txt", "adobe-core14-afm.txt"]],
    note:
      "The gridcraft-* crates are covered by gridcraft/ next to this file. " +
      "The PDF export's standard-14 font widths are reproduced under Adobe's terms, in assets/.",
  },
};

export function engine(crate) {
  const e = ENGINES[crate];
  if (!e) throw new Error(`unknown engine crate ${crate}; known: ${Object.keys(ENGINES).join(", ")}`);
  return e;
}

