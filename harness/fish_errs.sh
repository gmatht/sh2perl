#!/usr/bin/env bash
# fish_errs.sh — dump per-file fish parse errors for the whole corpus.
#
# fish_parsecheck.sh CLASSIFIES parse errors; this one prints them VERBATIM
# (file + fish's message), and with KEEP=1 it also saves the offending render
# and shows the surrounding lines so you can find the emitter in
# src/fish_backend.rs.
#
# Usage: harness/fish_errs.sh [file-prefix]      KEEP=1 to save renders to .fisherrs/
set -uo pipefail

WT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="$WT/../../.."
CORE="$ROOT/otranspilerl/target/debug/otranspilerl-cli"
RENDER="$WT/target/debug/shir_render"
export OTRANSPILER_ROOT="$ROOT/otranspilerl"

PREFIX="${1:-}"
KEEPDIR="$WT/.fisherrs"

mapfile -t corpus < <(
  { ls "$ROOT/sh2perl/examples/"*.sh 2>/dev/null
    ls "$ROOT/sh2perl/frontends/"*/testdata/*.sh 2>/dev/null; } | sort
)
[ -n "$PREFIX" ] && mapfile -t corpus < <(printf '%s\n' "${corpus[@]}" | grep -F "$PREFIX")

if [ "${KEEP:-0}" = 1 ]; then
  rm -rf "$KEEPDIR"; mkdir -p "$KEEPDIR"
else
  KEEPDIR=$(mktemp -d "$WT/.fisherrs.XXXXXX")
fi
trap '[ "${KEEP:-0}" = 1 ] || rm -rf "$KEEPDIR"' EXIT

for f in "${corpus[@]}"; do
  base=$(basename "$f")
  shir=$("$CORE" --target shir "$f" 2>/dev/null) || continue
  [ -n "$shir" ] || continue
  bash -n "$f" 2>/dev/null || continue
  out="$KEEPDIR/$base.fish"
  printf '%s' "$shir" | "$RENDER" --target=fish - > "$out" 2>/dev/null || continue
  err=$(fish -n "$out" 2>&1 >/dev/null | head -1)
  [ -n "$err" ] || continue
  ln=$(printf '%s' "$err" | sed -nE 's#^[^ ]* \(([0-9]+)\).*#\1#p')
  msg=$(printf '%s' "$err" | sed -E 's#^[^ ]* \([0-9]+\): ##')
  printf '%-46s L%-4s %s\n' "$base" "${ln:-?}" "$msg"
  if [ "${KEEP:-0}" = 1 ] && [ -n "${ln:-}" ]; then
    sed -n "$((ln > 1 ? ln - 1 : 1)),$((ln + 1))p" "$out" | sed 's/^/       | /'
  fi
done