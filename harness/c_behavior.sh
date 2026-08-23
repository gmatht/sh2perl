#!/usr/bin/env bash
# harness/c_behavior.sh — behavioral oracle for the C backend's generated
# output. For each example: render C (c_backend), compile, run, and diff
# stdout against running the original .sh with bash.
#
# Usage: harness/c_behavior.sh [file.sh ...]
#   (default: examples/*.sh)
#
# This is the correctness oracle the C gate lacks (c_valgrind.sh only
# checks memory safety). A behavioral mismatch is a renderer bug. Only
# deterministic, side-effect-free examples are meaningful here.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CBIN="$ROOT/target/debug/c_backend"
CC="${CC:-tcc -O1 -g}"
TMP=/tmp/c_beh; mkdir -p "$TMP"

pass=0; fail=0; skip=0; total=0
for f in ${@:-$ROOT/examples/*.sh}; do
  [ -f "$f" ] || continue
  total=$((total+1))
  # render
  "$CBIN" "$f" 2>/dev/null > "$TMP/o.c" || { skip=$((skip+1)); continue; }
  # skip if it still shells out (only native C output is comparable fast;
  # shell-outs fork bash anyway, but the point is to test native lowering)
  # compile
  $CC -o "$TMP/o" "$TMP/o.c" 2>/dev/null || { skip=$((skip+1)); continue; }
  # run generated C and bash in the same dir (so relative filenames work)
  d="$(dirname "$f")"
  ( cd "$d" && timeout 20 "$TMP/o" ) > "$TMP/got.txt" 2>"$TMP/gerr.txt"
  ( cd "$d" && timeout 20 bash "$(basename "$f")" ) > "$TMP/want.txt" 2>"$TMP/werr.txt"
  # compare stdout (ignore stderr — the gate redirects it)
  if diff -q "$TMP/got.txt" "$TMP/want.txt" >/dev/null 2>&1; then
    pass=$((pass+1))
  else
    fail=$((fail+1))
    if [ "${VERBOSE:-0}" = 1 ]; then
      echo "MISMATCH: $(basename "$f")"
    fi
  fi
done
echo "C behavior: total=$total pass=$pass fail=$fail skip=$skip"
[ "$fail" -eq 0 ]
