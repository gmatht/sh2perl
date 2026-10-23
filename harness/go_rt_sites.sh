#!/usr/bin/env bash
# go_rt_sites.sh — the exact runtime call SITES in the go render, with the
# source file and the emitted line, so a lowering is chosen by evidence.
#
# Same seam + body rule as harness/backend_runtime_metric.sh (SEAM_go /
# SEAM_BODY_go): the runtime preamble is the library, not the renderer's
# calls, so only lines from `func main()` on are counted — enforced
# PER FILE (awk's `on` flag otherwise leaks across the glob and counts the
# next file's preamble).
#
# Usage: harness/go_rt_sites.sh [helper]     (default: every helper)
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
WT="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$WT/../../.." && pwd)"
CLI="${OTRANSPILERL_BIN:-$ROOT/otranspilerl/target/debug/otranspilerl-cli}"
RENDER="${GO_SHIR_RENDER:-$WT/target/debug/shir_render}"
SEAM="${1:-s2s|capRun|capCmd|runCmd|redirRun|redirCap}"
BODY='^func main\(\)'

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for f in "$ROOT"/sh2perl/examples/*.sh; do
    [ -f "$f" ] || continue
    b=$(basename "$f")
    shir=$("$CLI" --target shir "$f" 2>/dev/null) || continue
    [ -n "$shir" ] || continue
    printf '%s' "$shir" | "$RENDER" --target=go - > "$tmp/$b.go" 2>/dev/null || continue
    awk -v m="$BODY" '$0 ~ m { on = 1 } on' "$tmp/$b.go" \
        | grep -E "$SEAM" \
        | sed -e "s|^|$b: |"
done
