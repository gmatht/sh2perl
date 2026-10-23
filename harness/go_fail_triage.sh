#!/usr/bin/env bash
# go_fail_triage.sh — one-line root-cause hint per FAIL-EQ file: the first
# differing line of `go run` vs `bash`. Feeds lowering decisions.
#
# Usage: harness/go_fail_triage.sh [file.sh ...]
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
WT="$(cd "$HERE/.." && pwd)"
ROOT="${SH2LOOP_ROOT:-/mnt/wsl/projects/sh2loop}"
CLI="${OTRANSPILERL_BIN:-$ROOT/otranspilerl/target/debug/otranspilerl-cli}"
BIN="${GO_SHIR_RENDER:-$WT/target/debug/shir_render}"
CACHE="${GO_FAST_CACHE:-/mnt/wsl/tmpbtrfs/go-fast-cache}"
export GOCACHE="$CACHE/gocache" GOMODCACHE="$CACHE/gomodcache" GOFLAGS=-mod=mod
for f in "$@"; do
    d=$(mktemp -d)
    shir=$("$CLI" --target shir "$f" 2>/dev/null) || { echo "EMIT  $f"; continue; }
    printf '%s' "$shir" | "$BIN" --target=go - > "$d/x.go" 2>/dev/null || { echo "REND  $f"; continue; }
    ( cd "$ROOT" && timeout 30 go run "$d/x.go" > "$d/out" 2>/dev/null )
    ( cd "$ROOT" && timeout 15 bash "$f" > "$d/ref" 2>/dev/null )
    if diff -q "$d/out" "$d/ref" >/dev/null 2>&1; then
        echo "OK    $(basename "$f")"
    else
        first=$(diff "$d/out" "$d/ref" | head -4 | tr '\n' '|')
        echo "DIFF  $(basename "$f") :: $first"
    fi
    rm -rf "$d"
done
