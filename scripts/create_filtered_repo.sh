#!/usr/bin/env bash
set -euo pipefail
# Create a mirror repository with binary/test/build artifacts removed from history.
#
# Usage: run from the repository root:
#   ./scripts/create_filtered_repo.sh
#
# Requirements: git, file, and git-filter-repo (https://github.com/newren/git-filter-repo)

REPO_ROOT="$(pwd)"
OUTDIR="$REPO_ROOT/filtered.git"
TMPPATH="$(mktemp -t removed_paths.XXXXXX)"
trap 'rm -f "$TMPPATH"' EXIT

echo "Scanning tracked files for binary or non-text files..."
# Produce a newline-separated list of tracked files that 'file --mime' marks as binary
git ls-files -z | xargs -0 -n1 file --mime | awk -F: '/charset=binary|application\/octet-stream|image\// {sub(/^[ \\t]+/, "", $1); print $1}' | sort -u > "$TMPPATH"

if [ ! -s "$TMPPATH" ]; then
  echo "No binary files detected in the current tracked files. Nothing to remove."
  exit 0
fi

echo "Found $(wc -l < \"$TMPPATH\") tracked paths to remove from history."
echo "Writing path list to: $REPO_ROOT/paths-to-remove.txt"
cp "$TMPPATH" "$REPO_ROOT/paths-to-remove.txt"

if ! command -v git-filter-repo >/dev/null 2>&1; then
  cat <<'MSG'
git-filter-repo is required but not found on PATH.
Install it from: https://github.com/newren/git-filter-repo

Alternatively, install via your package manager (if available) or pip:
  pip3 install git-filter-repo

After installing, re-run this script.
MSG
  exit 1
fi

if [ -e "$OUTDIR" ]; then
  echo "Output path $OUTDIR already exists. Remove it first or move it aside."
  exit 1
fi

echo "Cloning a mirrored copy of the current repository..."
git clone --mirror "$REPO_ROOT" "$OUTDIR"

echo "Running git-filter-repo to remove the listed paths from history..."
cd "$OUTDIR"
# Use --invert-paths with the list we created in the original repo root
git filter-repo --invert-paths --paths-from-file "$REPO_ROOT/paths-to-remove.txt"

echo "Filtered repository created at: $OUTDIR"
echo "You can inspect it with: git --git-dir=$OUTDIR log --all --stat"

echo "If the result looks good, you can push the filtered repo to a new remote."
