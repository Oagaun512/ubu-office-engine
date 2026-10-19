# Test corpus

PDFs are **not committed** here. CI fetches them:

| Source | Pinned at | Licence | What |
|---|---|---|---|
| [mozilla/pdf.js](https://github.com/mozilla/pdf.js) `test/pdfs/*.pdf` | `157d30e833e5afbd282db5b1280aae4843f55c0a` | Apache-2.0 (repository) | The in-tree files of pdf.js's regression suite, including encrypted, damaged and unusual files. Files that pdf.js only links to (`*.link`) are skipped. Passwords come from its `test/test_manifest.json`. |

`scripts/fetch-corpus.sh` downloads them into `corpus/cache/` (git-ignored).

## DOCX (Ubu Office Phase 3, D4)

| Source | Pinned at | Licence | What |
|---|---|---|---|
| [apache/poi](https://github.com/apache/poi) `test-data/document/*.docx` | `bf7c9b6a1490046517cbd8e1504e702a89be4bc9` | Apache-2.0 (repository; `legal/LICENSE`, `legal/NOTICE` fetched with them) | 130 Word files from Apache POI's test data, many attached to real bug reports: unusual and damaged structure on purpose. |

`scripts/fetch-docx-corpus.sh` downloads them into `corpus/cache/poi/`, and
`scripts/roundtrip-docx.mjs` writes the round-trip and fidelity report (ROUNDTRIP.md, vendored
into Ubunex with the engine). Nothing here is committed or shipped.

## Spreadsheets (Ubu Office Phase 4, S2)

| Source | Pinned at | Licence | What |
|---|---|---|---|
| [apache/poi](https://github.com/apache/poi) `test-data/spreadsheet/*.{xlsx,xlsm,xltx,xls,xlsb,ods}` | `bf7c9b6a1490046517cbd8e1504e702a89be4bc9` | Apache-2.0 (repository; `legal/LICENSE`, `legal/NOTICE` fetched with them) | Apache POI's spreadsheet test data, many from real bug reports: Excel 2007+ packages, Excel 97-2003 files, binary and OpenDocument workbooks, password-protected files. |

`scripts/fetch-xlsx-corpus.sh` downloads them into `corpus/cache/poi-spreadsheet/`, and
`scripts/roundtrip-xlsx.mjs` writes the round-trip report (ROUNDTRIP.md, vendored into Ubunex
with the engine). Nothing here is committed or shipped.

## Our own real-world files (decision D6, open)

Messy real-world PDFs and Word documents from the founder or public sources go in a separate
store, never in this public repository if they hold personal data. Until D6 is settled the
corpora are pdf.js and Apache POI only.
