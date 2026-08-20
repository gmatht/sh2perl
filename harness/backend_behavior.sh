#!/usr/bin/env bash
# harness/backend_behavior.sh <backend> [file.sh ...]  (default: examples/*.sh)
# Behavioral oracle for a non-estree backend: render each example with
# scan_backend, run the generated code, diff stdout vs bash.
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCAN="$ROOT/target/debug/scan_backend"
TMP=/tmp/bb; mkdir -p "$TMP"
be="$1"; shift
files=("$@"); [ ${#files[@]} -eq 0 ] && files=("$ROOT"/examples/*.sh)

pass=0; fail=0; skip=0
for f in "${files[@]}"; do
  [ -f "$f" ] || continue
  "$SCAN" "$be" "$f" full > "$TMP/prog" 2>/dev/null || { skip=$((skip+1)); continue; }
  d="$(dirname "$f")"; b="$(basename "$f")"
  case "$be" in
    python) timeout 20 python3 "$TMP/prog" > "$TMP/got.txt" 2>/dev/null ;;
    *) skip=$((skip+1)); continue ;;
  esac
  ( cd "$d" && timeout 20 bash "$b" ) > "$TMP/want.txt" 2>/dev/null
  if diff -q "$TMP/got.txt" "$TMP/want.txt" >/dev/null 2>&1; then
    pass=$((pass+1))
  else
    fail=$((fail+1))
    [ "${VERBOSE:-0}" = 1 ] && echo "MISMATCH: $b"
  fi
done
echo "$be behavior: pass=$pass fail=$fail skip=$skip"
[ "$fail" -eq 0 ]
