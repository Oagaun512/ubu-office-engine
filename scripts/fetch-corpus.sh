#!/usr/bin/env sh
# Fetches the PDF corpus into corpus/cache/ (see corpus/README.md). Sparse, shallow, pinned.
set -eu
cd "$(dirname "$0")/.."
PDFJS_COMMIT=157d30e833e5afbd282db5b1280aae4843f55c0a
dest=corpus/cache/pdfjs
if [ "$(git -C "$dest" rev-parse HEAD 2>/dev/null || true)" = "$PDFJS_COMMIT" ]; then
  echo "pdf.js corpus already at $PDFJS_COMMIT"
  exit 0
fi
rm -rf "$dest"
mkdir -p "$dest"
git -C "$dest" init -q
git -C "$dest" remote add origin https://github.com/mozilla/pdf.js
git -C "$dest" sparse-checkout set --no-cone '/test/pdfs/*.pdf' '/test/test_manifest.json' '/LICENSE'
git -C "$dest" fetch -q --depth 1 --filter=blob:none origin "$PDFJS_COMMIT"
git -C "$dest" checkout -q FETCH_HEAD
echo "pdf.js corpus: $(find "$dest/test/pdfs" -name '*.pdf' | wc -l) PDFs at $PDFJS_COMMIT"
