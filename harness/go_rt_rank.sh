#!/usr/bin/env bash
# go_rt_rank.sh — per-file ranking of the go renderer's runtime call sites.
# Same seam + body rule as harness/backend_runtime_metric.sh (SEAM_go /
# SEAM_BODY_go), but reported per corpus file so a lowering target is chosen
# by evidence instead of by guess.
#
# Usage: harness/go_rt_rank.sh [N]      (default 25)
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
WT="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$WT/../../.." && pwd)"
CLI="${OTRANSPILERL_BIN:-$ROOT/otranspilerl/target/debug/otranspilerl-cli}"
RENDER="${GO_SHIR_RENDER:-$WT/target/debug/shir_render}"
SEAM='s2s|capRun|capCmd|runCmd|redirRun|redirCap'
BODY='^func main\(\)'
N="${1:-25}"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for f in "$ROOT"/sh2perl/examples/*.sh; do
    [ -f "$f" ] || continue
    b=$(basename "$f")
    shir=$("$CLI" --target shir "$f" 2>/dev/null) || continue
    [ -n "$shir" ] || continue
    printf '%s' "$shir" | "$RENDER" --target=go - > "$tmp/$b.go" 2>/dev/null || continue
    n=$(awk -v m="$BODY" '$0 ~ m { on = 1 } on' "$tmp/$b.go" | grep -oE "$SEAM" | wc -l)
    [ "$n" -gt 0 ] && printf '%d\t%s\n' "$n" "$b"
done | sort -rn | head -"$N"
