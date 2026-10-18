#!/usr/bin/env sh
# Adds an `upstream` remote (the original storytold repository) in each fork under upstream/,
# so syncing is `git -C upstream/<repo> fetch upstream && git -C upstream/<repo> merge upstream/main`.
# Remotes are local git config and are not stored in the repository, hence this script.
set -eu
cd "$(dirname "$0")/.."
git submodule update --init
for repo in pdfcraft craft-libs; do
  dir="upstream/$repo"
  url="https://github.com/storytold/$repo"
  if git -C "$dir" remote get-url upstream >/dev/null 2>&1; then
    git -C "$dir" remote set-url upstream "$url"
  else
    git -C "$dir" remote add upstream "$url"
  fi
  echo "$dir: origin=$(git -C "$dir" remote get-url origin) upstream=$url"
done
