#!/usr/bin/env bash
# go_rt_by_helper.sh — aggregate the go renderer's runtime call sites by helper
# name over the whole corpus (same seam + body rule as
# harness/backend_runtime_metric.sh: SEAM_go counted from `func main`).
#
# Usage: harness/go_rt_by_helper.sh
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
WT="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$WT/../../.." && pwd)"
CLI="${OTRANSPILERL_BIN:-$ROOT/otranspilerl/target/debug/otranspilerl-cli}"
RENDER="${GO_SHIR_RENDER:-$WT/target/debug/shir_render}"
SEAM='s2s|capRun|capCmd|runCmd|redirRun|redirCap'

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
# BODY ONLY, and per file: concatenating renders first would let each later
# file's PREAMBLE (which defines s2s, capRun, ...) fall inside the first
# `func main()` slice and be counted as a call site.
for f in "$ROOT"/sh2perl/examples/*.sh; do
    [ -f "$f" ] || continue
    shir=$("$CLI" --target shir "$f" 2>/dev/null) || continue
    [ -n "$shir" ] || continue
    printf '%s' "$shir" | "$RENDER" --target=go - 2>/dev/null \
        | sed -n '/^func main()/,$p'
done > "$tmp/body.txt"
grep -oE "$SEAM" "$tmp/body.txt" | sort | uniq -c | sort -rn
printf '%-8s %d\n' TOTAL "$(grep -oE "$SEAM" "$tmp/body.txt" | wc -l)"
