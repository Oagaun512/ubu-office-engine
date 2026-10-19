#!/usr/bin/env sh
# Fetches the spreadsheet corpus into corpus/cache/ (see corpus/README.md). Sparse, shallow,
# pinned: Apache POI's test-data/spreadsheet, real Excel files (XLSX, XLSM, XLS, XLSB, ODS),
# many attached to bug reports. The same pinned POI commit as the DOCX corpus.
set -eu
cd "$(dirname "$0")/.."
POI_COMMIT=bf7c9b6a1490046517cbd8e1504e702a89be4bc9
dest=corpus/cache/poi-spreadsheet
if [ "$(git -C "$dest" rev-parse HEAD 2>/dev/null || true)" = "$POI_COMMIT" ]; then
  echo "POI spreadsheet corpus already at $POI_COMMIT"
  exit 0
fi
rm -rf "$dest"
mkdir -p "$dest"
git -C "$dest" init -q
git -C "$dest" remote add origin https://github.com/apache/poi
git -C "$dest" sparse-checkout set --no-cone \
  '/test-data/spreadsheet/*.xlsx' '/test-data/spreadsheet/*.xlsm' '/test-data/spreadsheet/*.xltx' \
  '/test-data/spreadsheet/*.xls' '/test-data/spreadsheet/*.xlsb' '/test-data/spreadsheet/*.ods' \
  '/legal/LICENSE' '/legal/NOTICE'
git -C "$dest" fetch -q --depth 1 --filter=blob:none origin "$POI_COMMIT"
git -C "$dest" checkout -q FETCH_HEAD
echo "POI spreadsheet corpus: $(find "$dest/test-data/spreadsheet" -type f | wc -l) files at $POI_COMMIT"
