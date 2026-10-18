# Test corpus

PDFs are **not committed** here. CI fetches them:

| Source | Pinned at | Licence | What |
|---|---|---|---|
| [mozilla/pdf.js](https://github.com/mozilla/pdf.js) `test/pdfs/*.pdf` | `157d30e833e5afbd282db5b1280aae4843f55c0a` | Apache-2.0 (repository) | The in-tree files of pdf.js's regression suite, including encrypted, damaged and unusual files. Files that pdf.js only links to (`*.link`) are skipped. Passwords come from its `test/test_manifest.json`. |

`scripts/fetch-corpus.sh` downloads them into `corpus/cache/` (git-ignored).

## Our own real-world files (decision D6, open)

Messy real-world PDFs from the founder or public sources go in a separate store, never in this
public repository if they hold personal data. Until D6 is settled the corpus is pdf.js only.
