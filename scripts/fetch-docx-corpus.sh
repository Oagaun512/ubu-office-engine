#!/usr/bin/env sh
# Fetches the DOCX corpus into corpus/cache/ (see corpus/README.md). Sparse, shallow, pinned.
# Apache POI's test-data/document: real Word files, many attached to bug reports.
set -eu
cd "$(dirname "$0")/.."
POI_COMMIT=bf7c9b6a1490046517cbd8e1504e702a89be4bc9
dest=corpus/cache/poi
if [ "$(git -C "$dest" rev-parse HEAD 2>/dev/null || true)" = "$POI_COMMIT" ]; then
  echo "POI corpus already at $POI_COMMIT"
  exit 0
fi
rm -rf "$dest"
mkdir -p "$dest"
git -C "$dest" init -q
git -C "$dest" remote add origin https://github.com/apache/poi
git -C "$dest" sparse-checkout set --no-cone '/test-data/document/*.docx' '/legal/LICENSE' '/legal/NOTICE'
git -C "$dest" fetch -q --depth 1 --filter=blob:none origin "$POI_COMMIT"
git -C "$dest" checkout -q FETCH_HEAD
echo "POI corpus: $(find "$dest/test-data/document" -iname '*.docx' | wc -l) DOCX at $POI_COMMIT"
