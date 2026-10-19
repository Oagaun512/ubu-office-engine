#!/usr/bin/env sh
# Fetches the presentation corpus into corpus/cache/ (see corpus/README.md). Sparse, shallow,
# pinned: Apache POI's test-data/slideshow, real PowerPoint files (PPTX, PPTM, PPSX, POTX, and
# old .ppt, which the engine refuses: D26), many attached to bug reports. The same pinned POI
# commit as the DOCX and spreadsheet corpora.
set -eu
cd "$(dirname "$0")/.."
POI_COMMIT=bf7c9b6a1490046517cbd8e1504e702a89be4bc9
dest=corpus/cache/poi-slideshow
if [ "$(git -C "$dest" rev-parse HEAD 2>/dev/null || true)" = "$POI_COMMIT" ]; then
  echo "POI slideshow corpus already at $POI_COMMIT"
  exit 0
fi
rm -rf "$dest"
mkdir -p "$dest"
git -C "$dest" init -q
git -C "$dest" remote add origin https://github.com/apache/poi
git -C "$dest" sparse-checkout set --no-cone \
  '/test-data/slideshow/*.pptx' '/test-data/slideshow/*.pptm' '/test-data/slideshow/*.ppsx' \
  '/test-data/slideshow/*.potx' '/test-data/slideshow/*.ppt' \
  '/legal/LICENSE' '/legal/NOTICE'
git -C "$dest" fetch -q --depth 1 --filter=blob:none origin "$POI_COMMIT"
git -C "$dest" checkout -q FETCH_HEAD
echo "POI slideshow corpus: $(find "$dest/test-data/slideshow" -type f | wc -l) files at $POI_COMMIT"
