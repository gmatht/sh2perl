#!/usr/bin/env bash
# Fast local go equivalence harness — the same comparison as go_equiv.sh and
# setup_backends.sh's go arm, but with a PERSISTENT GOCACHE and module cache
# so each corpus file is an incremental `go build` rather than a cold
# `go run` compile. Measured ~0.35s/file warm vs ~1.2s/file for go run.
#
#   usage: go_fast_equiv.sh [-n N] [file.sh ...]      (default: whole corpus)
#   -j N   parallel jobs (default: 1, matching the gate's serial arm)
#   env:   GO_SHIR_RENDER, GO_FAST_CACHE (cache dir), GO_FAST_JOBS
#   writes: <tsv>  file <TAB> verdict   when $GO_FAST_TSV is set
set -u
ROOT=/mnt/wsl/projects/sh2loop
CLI="$ROOT/otranspilerl/target/debug/otranspilerl-cli"
R="${GO_SHIR_RENDER:-$ROOT/sh2perl/backends/go/target/debug/shir_render}"
GO=$(command -v go)
CACHE="${GO_FAST_CACHE:-/mnt/wsl/tmpbtrfs/go-fast-cache}"
mkdir -p "$CACHE/gocache" "$CACHE/gomodcache" 2>/dev/null || CACHE=$(mktemp -d)
export GOCACHE="$CACHE/gocache" GOMODCACHE="$CACHE/gomodcache" GOFLAGS=-mod=mod
TMP=$(mktemp -d "$CACHE/run.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

limit=0; jobs="${GO_FAST_JOBS:-1}"
while [ $# -gt 0 ]; do
  case "$1" in
    -n) limit="$2"; shift 2;;
    -j) jobs="$2"; shift 2;;
    *) break;;
  esac
done
if [ $# -gt 0 ]; then FILES=("$@"); else FILES=("$ROOT"/sh2perl/examples/*.sh); fi
_resolved=()
for _f in "${FILES[@]}"; do
  _resolved+=("$(cd "$(dirname "$_f")" 2>/dev/null && pwd)/$(basename "$_f")")
done
FILES=("${_resolved[@]}")

one() {
  local f="$1" base rc
  base=$(basename "$f" .sh)
  local shir
  shir=$("$CLI" --target shir "$f" 2>/dev/null) || { printf "%s\tSKIP\n" "$base"; return; }
  [ -n "$shir" ] || { printf "%s\tSKIP\n" "$base"; return; }
  printf '%s' "$shir" | "$R" --target=go - > "$TMP/$base.go" 2>/dev/null || { printf "%s\tSKIP\n" "$base"; return; }
  if grep -qE 'TODO\(unsupported\)|sh2[A-Za-z_0-9]+\(' "$TMP/$base.go"; then
    printf "%s\tSTUB\n" "$base"; return
  fi
  local work="$TMP/w_$base"; mkdir -p "$work"
  cp "$TMP/$base.go" "$work/main.go"
  ( cd "$work" && timeout 90 "$GO" build -o "$work/prog" main.go ) 2> "$TMP/$base.cerr"
  rc=$?
  if [ $rc -ge 124 ]; then printf "%s\tTIMEOUT\n" "$base"; return; fi
  if [ $rc -ne 0 ]; then printf "%s\tCOMPILE\n" "$base"; return; fi
  ( cd "$work" && timeout 30 "$work/prog" ) > "$TMP/$base.out" 2>/dev/null
  local go_rc=$?
  ( cd "$work" && timeout 30 bash "$f" ) > "$TMP/$base.ref" 2>/dev/null
  local bash_rc=$?
  # go is not in the gate's rc-equality group: translation rc==0 && bash rc==0
  if [ "$go_rc" = 0 ] && [ "$bash_rc" = 0 ] && diff -q "$TMP/$base.out" "$TMP/$base.ref" >/dev/null 2>&1; then
    printf "%s\tPASS\n" "$base"
  else
    printf "%s\tDIFF:%s:%s\n" "$base" "$go_rc" "$bash_rc"
  fi
}

export -f one; export CLI R GO TMP

if [ "$jobs" -gt 1 ]; then
  printf '%s\n' "${FILES[@]}" | xargs -P "$jobs" -I{} bash -c 'one "$@"' _ {}
else
  for f in "${FILES[@]}"; do one "$f"; done
fi | { if [ "$limit" -gt 0 ]; then head -n "$limit"; else cat; fi; } > "$TMP/results.tsv"

awk -F'\t' '
  { v=$2; sub(/:.*/,"",v); c[v]++ }
  END { printf "PASS=%d DIFF=%d STUB=%d COMPILE=%d TIMEOUT=%d SKIP=%d\n",
        c["PASS"], c["DIFF"], c["STUB"], c["COMPILE"], c["TIMEOUT"], c["SKIP"] }' "$TMP/results.tsv"
grep -vE 'PASS$' "$TMP/results.tsv" | sed 's/^/  /'
[ -n "${GO_FAST_TSV:-}" ] && cp "$TMP/results.tsv" "$GO_FAST_TSV"
exit 0
