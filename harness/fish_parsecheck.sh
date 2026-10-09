#!/usr/bin/env bash
# fish_parsecheck.sh — measure how many renders fish can actually PARSE.
#
# fish_gate.sh measures equivalence, which conflates a wrong translation with
# a translation fish cannot even read. This script separates the two: it runs
# `fish -n` over every render and buckets the syntax errors, so a fix can be
# aimed at a parse-error class rather than at a noisy equivalence diff.
#
# Usage: harness/fish_parsecheck.sh [file-prefix] [--verbose]
set -uo pipefail

WT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="$WT/../../.."
CORE="$ROOT/otranspilerl/target/debug/otranspilerl-cli"
RENDER="$WT/target/debug/shir_render"
export OTRANSPILER_ROOT="$ROOT/otranspilerl"

VERBOSE=0
PREFIX=""
for a in "$@"; do
  case "$a" in
    --verbose) VERBOSE=1 ;;
    *) PREFIX="$a" ;;
  esac
done

for b in "$CORE" "$RENDER"; do
  [ -x "$b" ] || { echo "missing $b — build it first" >&2; exit 2; }
done

mapfile -t corpus < <(
  { ls "$ROOT/sh2perl/examples/"*.sh 2>/dev/null
    ls "$ROOT/sh2perl/frontends/"*/testdata/*.sh 2>/dev/null; } | sort
)
[ -n "$PREFIX" ] && mapfile -t corpus < <(printf '%s\n' "${corpus[@]}" | grep -F "$PREFIX")

tmp=$(mktemp -d "$WT/.fishpc.XXXXXX"); trap 'rm -rf "$tmp"' EXIT
errs="$tmp/errs.txt"; : > "$errs"

rendered=0
for f in "${corpus[@]}"; do
  base=$(basename "$f")
  shir=$("$CORE" --target shir "$f" 2>/dev/null) || continue
  [ -n "$shir" ] || continue
  bash -n "$f" 2>/dev/null || continue
  out="$tmp/$base.fish"
  printf '%s' "$shir" | "$RENDER" --target=fish - > "$out" 2>/dev/null || continue
  rendered=$((rendered + 1))
  # keep the render for --verbose triage
  err=$(fish -n "$out" 2>&1 >/dev/null | head -1)
  if [ -n "$err" ]; then
    printf '%s\t%s\n' "$base" "$err" >> "$errs"
    [ $VERBOSE = 1 ] && cp "$out" "$tmp/keep-$base"
  fi
done

n=$(wc -l < "$errs")
echo "[fish-parse] $((rendered - n))/$rendered renders parse, $n fail"
if [ "$n" -gt 0 ]; then
  echo "[fish-parse] error classes:"
  sed -E 's/^[^\t]*\t//' "$errs" \
    | sed -E 's/^[^:]*: //' \
    | sed -E "s/'[^']*'/'X'/g; s/\"[^\"]*\"/Q/g; s/[0-9]+/N/g" \
    | sort | uniq -c | sort -rn | head -20 | sed 's/^/  /'
  echo "[fish-parse] failing files:"
  cut -f1 "$errs" | sed 's/^/  /' | head -40
fi