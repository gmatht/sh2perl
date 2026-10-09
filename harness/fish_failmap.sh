#!/usr/bin/env bash
# fish_failmap.sh — enumerate the gate's failing files and bucket them by
# FIRST divergence, so a fix can be aimed at a class instead of a noisy list.
#
# Same contract as harness/fish_gate.sh (render via shir_render, run both sides
# in one hermetic scratch dir); this one only classifies. Classification is
# cheap and deterministic:
#
#   RENDER  the renderer refused the IR (unsupported construct)
#   STUB    output carries a sh2*() stub / TODO(unsupported) marker
#   PARSE   fish -n rejects the render (never reaches the diff)
#   BASHERR bash itself errors on the source (reference is not a real baseline)
#   TIMEOUT a side exceeded the 15s bound
#   OUT     stdout divergence  (+ the first differing line, for the bucket key)
#   RC      stdout agrees, exit codes differ
#
# Usage: harness/fish_failmap.sh [file-prefix] [--out DIR] [--quiet]
set -uo pipefail

WT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="$WT/../../.."
CORE="$ROOT/otranspilerl/target/debug/otranspilerl-cli"
RENDER="$WT/target/debug/shir_render"
export OTRANSPILER_ROOT="$ROOT/otranspilerl"

PREFIX=""; OUTDIR=""; QUIET=0
while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUTDIR="$2"; shift 2 ;;
    --quiet) QUIET=1; shift ;;
    *) PREFIX="$1"; shift ;;
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

tmp=$(mktemp -d "$WT/.fishmap.XXXXXX"); trap 'rm -rf "$tmp"' EXIT
# per-file artifacts (render, ref stdout, fish stdout) survive for triage
[ -n "$OUTDIR" ] && mkdir -p "$OUTDIR"

: > "$tmp/buckets.tsv"
pass=0; fail=0; skip=0

for f in "${corpus[@]}"; do
  base=$(basename "$f")
  shir=$("$CORE" --target shir "$f" 2>/dev/null) || { echo -e "RENDER\t$base\tcore emit failed" >> "$tmp/buckets.tsv"; fail=$((fail+1)); continue; }
  [ -z "$shir" ] && { echo -e "RENDER\t$base\tcore emit empty" >> "$tmp/buckets.tsv"; fail=$((fail+1)); continue; }
  bash -n "$f" 2>/dev/null || { skip=$((skip+1)); continue; }

  rerr="$tmp/rerr.txt"
  if ! out=$(printf '%s' "$shir" | "$RENDER" --target=fish - 2>"$rerr"); then
    reason=$(head -1 "$rerr" | sed 's/^shir_render: fish: //')
    echo -e "RENDER\t$base\t$reason" >> "$tmp/buckets.tsv"; fail=$((fail+1)); continue
  fi
  if printf '%s' "$out" | grep -qE 'TODO\(unsupported\)|sh2[A-Za-z_0-9]+\('; then
    echo -e "STUB\t$base\tstub/TODO marker in render" >> "$tmp/buckets.tsv"; fail=$((fail+1)); continue
  fi

  scratch="$tmp/run"; rm -rf "$scratch"; mkdir -p "$scratch"
  printf '%s' "$out" > "$scratch/_r.fish"
  [ -n "$OUTDIR" ] && printf '%s' "$out" > "$OUTDIR/$base.fish"
  perr=$(fish -n "$scratch/_r.fish" 2>&1 >/dev/null | head -1)
  if [ -n "$perr" ]; then
    echo -e "PARSE\t$base\t$perr" >> "$tmp/buckets.tsv"; fail=$((fail+1)); continue
  fi

  bash_out=$(cd "$scratch" && timeout 15 bash "$f" 2>/dev/null); bash_rc=$?
  fish_out=$(cd "$scratch" && timeout 15 fish "$scratch/_r.fish" 2>/dev/null); fish_rc=$?
  [ -n "$OUTDIR" ] && { printf '%s\n' "$bash_out" > "$OUTDIR/$base.bash"; printf '%s\n' "$fish_out" > "$OUTDIR/$base.fishout"; }
  if [ "$bash_rc" = 124 ] || [ "$fish_rc" = 124 ]; then
    echo -e "TIMEOUT\t$base\tbash=$bash_rc fish=$fish_rc" >> "$tmp/buckets.tsv"; fail=$((fail+1)); continue
  fi
  if [ "$bash_out" != "$fish_out" ]; then
    key=$(diff <(printf '%s\n' "$bash_out") <(printf '%s\n' "$fish_out") | grep -E '^[<>]' | head -2 | tr '\n' '|' | cut -c1-160)
    echo -e "OUT\t$base\t$key" >> "$tmp/buckets.tsv"; fail=$((fail+1)); continue
  fi
  if [ "$bash_rc" = 0 ] && [ "$fish_rc" != 0 ]; then
    echo -e "RC\t$base\tbash=0 fish=$fish_rc" >> "$tmp/buckets.tsv"; fail=$((fail+1)); continue
  fi
  pass=$((pass+1))
done

echo
echo "[fish-failmap] $pass pass, $fail fail, $skip skip"
echo "[fish-failmap] bucket counts:"
cut -f1 "$tmp/buckets.tsv" | sort | uniq -c | sort -rn | sed 's/^/    /'
cp "$tmp/buckets.tsv" "${OUTDIR:-$tmp}/buckets.tsv" 2>/dev/null
echo "[fish-failmap] buckets.tsv: ${OUTDIR:-$tmp}/buckets.tsv"
[ $QUIET = 0 ] && cut -f1,2 "$tmp/buckets.tsv" | sort | sed 's/^/    /'
exit 0