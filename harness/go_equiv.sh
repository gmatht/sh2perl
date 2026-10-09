#!/usr/bin/env bash
# Local go equivalence harness — mirrors setup_backends.sh's go arm, which is
# DEAD on this box: its eq_tool is hardcoded to /snap/go/current/bin/go, which
# does not exist, so eq_gate=0 and the corpus is never executed. This runs the
# same comparison with the real go on PATH.
#
#   usage: go_equiv.sh [-n N] [file.sh ...]      (default: whole corpus)
#   writes: <tsv>  file <TAB> verdict
set -u
ROOT=/mnt/wsl/projects/sh2loop
CLI="$ROOT/otranspilerl/target/debug/otranspilerl-cli"
R="${GO_SHIR_RENDER:-$ROOT/sh2perl/backends/go/target/debug/shir_render}"
GO=$(command -v go)
TMP=$(mktemp -d)
PASS=0; FAIL=0; SKIP=0; COMPILE=0
declare -a FAILS=()
limit=0
while [ $# -gt 0 ]; do
  case "$1" in
    -n) limit="$2"; shift 2;;
    *) break;;
  esac
done
if [ $# -gt 0 ]; then FILES=("$@"); else FILES=("$ROOT"/sh2perl/examples/*.sh); fi
# the reference runs from a per-file scratch dir, so every path must be
# absolute or bash cannot find the script there (relative args gave an empty
# reference and a phantom DIFF).
_resolved=()
for _f in "${FILES[@]}"; do
  _resolved+=("$(cd "$(dirname "$_f")" 2>/dev/null && pwd)/$(basename "$_f")")
done
FILES=("${_resolved[@]}")
n=0
for f in "${FILES[@]}"; do
  [ -f "$f" ] || continue
  n=$((n+1))
  [ "$limit" -gt 0 ] && [ "$n" -gt "$limit" ] && break
  base=$(basename "$f" .sh)
  shir=$("$CLI" --target shir "$f" 2>/dev/null) || { SKIP=$((SKIP+1)); continue; }
  [ -n "$shir" ] || { SKIP=$((SKIP+1)); continue; }
  printf '%s' "$shir" | "$R" --target=go - > "$TMP/$base.go" 2>/dev/null || { SKIP=$((SKIP+1)); continue; }
  # stub policy mirrors the gate
  if grep -qE 'TODO\(unsupported\)|sh2[A-Za-z_0-9]+\(' "$TMP/$base.go"; then
    FAIL=$((FAIL+1)); FAILS+=("$base STUB"); continue
  fi
  # hermetic scratch, same as the gate's eq_scratch rule
  work="$TMP/w_$base"; mkdir -p "$work"
  cp "$TMP/$base.go" "$work/main.go"
  ( cd "$work" && GOFLAGS=-mod=mod GOCACHE="$TMP/gocache" timeout 90 "$GO" run main.go ) \
      > "$TMP/$base.out" 2> "$TMP/$base.cerr"
  go_rc=$?
  if [ $go_rc -ge 124 ]; then FAIL=$((FAIL+1)); FAILS+=("$base TIMEOUT"); continue; fi
  if grep -qE '\.go:[0-9]+:[0-9]+: ' "$TMP/$base.cerr"; then
    COMPILE=$((COMPILE+1)); FAIL=$((FAIL+1)); FAILS+=("$base COMPILE"); continue
  fi
  ( cd "$work" && timeout 30 bash "$f" ) > "$TMP/$base.ref" 2>/dev/null
  bash_rc=$?
  # go is not in the gate's rc-equality group: translation rc==0 && bash rc==0
  if [ "$go_rc" = 0 ] && [ "$bash_rc" = 0 ] && diff -q "$TMP/$base.out" "$TMP/$base.ref" >/dev/null 2>&1; then
    PASS=$((PASS+1))
  else
    FAIL=$((FAIL+1)); FAILS+=("$base DIFF")
  fi
done
echo "PASS=$PASS FAIL=$FAIL SKIP=$SKIP (of which compile errors: $COMPILE)"
if [ ${#FAILS[@]} -gt 0 ]; then
  printf '%s\n' "${FAILS[@]}" | sed 's/^/  /'
fi
echo "workdir kept: $TMP"
