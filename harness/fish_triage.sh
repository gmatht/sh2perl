#!/usr/bin/env bash
# fish_triage.sh — ONE PASS over the corpus that captures, per failing file,
# everything needed to fix it: the render, fish's parse error, the bash source
# line that produced it, and the bash-vs-fish output diff.
#
# fish_gate.sh tells you HOW MANY fail. fish_parsecheck.sh tells you the error
# CLASS. Neither tells you which source construct caused it — this does, so a
# fix round is one read instead of three.
#
# Usage: harness/fish_triage.sh [file-prefix] [--keep]
set -uo pipefail

WT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="$WT/../../.."
CORE="$ROOT/otranspilerl/target/debug/otranspilerl-cli"
RENDER="$WT/target/debug/shir_render"
export OTRANSPILER_ROOT="$ROOT/otranspilerl"

PREFIX=""; KEEP=0
for a in "$@"; do
  case "$a" in
    --keep) KEEP=1 ;;
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

tmp=$(mktemp -d "$WT/.fishtriage.XXXXXX")
trap '[ $KEEP = 1 ] || rm -rf "$tmp"' EXIT
outdir="$tmp/out"; mkdir -p "$outdir"

pass=0; fail=0; skip=0; parse=0; eq=0; emit=0
rows="$tmp/rows.tsv"; : > "$rows"

for f in "${corpus[@]}"; do
  base=$(basename "$f")
  shir=$("$CORE" --target shir "$f" 2>/dev/null) || { emit=$((emit+1)); fail=$((fail+1)); continue; }
  [ -n "$shir" ] || { emit=$((emit+1)); fail=$((fail+1)); continue; }
  bash -n "$f" 2>/dev/null || { skip=$((skip+1)); continue; }

  out="$outdir/$base.fish"
  printf '%s' "$shir" | "$RENDER" --target=fish - > "$out" 2>/dev/null || { fail=$((fail+1)); continue; }

  # STUB GATE — a CALL sh2<ident>( or a TODO(unsupported) marker.
  s=$(grep -cE 'TODO\(unsupported\)|sh2[A-Za-z_0-9]+\(' "$out" || true)
  if [ "$s" -ne 0 ]; then
    fail=$((fail+1)); printf '%s\tSTUB(%s)\t\n' "$base" "$s" >> "$rows"; continue
  fi

  perr=$(fish -n "$out" 2>&1 >/dev/null | head -1)

  run="$tmp/run"; rm -rf "$run"; mkdir -p "$run"
  bo=$(cd "$run" && timeout 15 bash "$f" 2>/dev/null)
  cp "$out" "$run/_r.fish"
  fo=$(cd "$run" && timeout 15 fish "$run/_r.fish" 2>/dev/null)

  if [ -n "$perr" ]; then
    parse=$((parse+1)); fail=$((fail+1))
    ln=$(printf '%s' "$perr" | sed -nE 's#^[^ ]* \(([0-9]+)\).*#\1#p')
    msg=$(printf '%s' "$perr" | sed -E 's#^[^ ]* \([0-9]+\): ##')
    rline=$([ -n "$ln" ] && sed -n "${ln}p" "$out")
    printf '%s\tPARSE\tL%s: %s\n' "$base" "${ln:-?}" "$msg" >> "$rows"
    printf '\n=== %s PARSE L%s\n  %s\n  | %s\n' "$base" "${ln:-?}" "$msg" "$rline"
    continue
  fi

  if [ "$bo" != "$fo" ]; then
    eq=$((eq+1)); fail=$((fail+1))
    printf '%s\tEQ\t\n' "$base" >> "$rows"
    printf '\n=== %s EQ\n' "$base"
    diff <(printf '%s\n' "$bo") <(printf '%s\n' "$fo") | head -"${DIFFLINES:-10}" | sed 's/^/  /'
    continue
  fi

  pass=$((pass+1))
done

total=$((pass+fail+skip))
echo
echo "[fish] $pass/$total OK, $fail fail ($parse parse, $eq equiv, $emit emit)"
[ $KEEP = 1 ] && echo "[fish] renders kept in $outdir"