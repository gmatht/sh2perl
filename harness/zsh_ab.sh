#!/usr/bin/env bash
# zsh_ab.sh — A/B two shir_render binaries over one file list, N repeats.
#
# The gate is not deterministic on some corpus files (they read the terminal,
# use mktemp globs, or depend on the scratch dir path). Comparing ONE gate
# number against another therefore measures noise, not the change. This runs
# each file REPEATS times per binary and reports per-file pass counts, so a
# real fix shows as N/N and a flaky file shows as k/N for both binaries.
#
# Usage: harness/zsh_ab.sh <binA> <binB> <repeats> <file...>
set -uo pipefail

BIN_A="$1"; BIN_B="$2"; REPEATS="$3"; shift 3

ROOT=/home/llm/sh2loop
CORE="$ROOT/otranspilerl/target/debug/otranspilerl-cli"
export OTRANSPILER_ROOT="$ROOT/otranspilerl"

# resolve basenames against the gate's own corpus
files=()
for n in "$@"; do
  if [ -f "$n" ]; then files+=( "$n" ); continue; fi
  hit=$(ls "$ROOT/sh2perl"/examples/"$n" "$ROOT/sh2perl"/frontends/*/testdata/"$n" 2>/dev/null | head -1)
  [ -n "$hit" ] || { echo "no corpus file: $n" >&2; exit 2; }
  files+=( "$hit" )
done

tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT

run_one() {  # $1=bin $2=file -> prints ok|fail
  local bin="$1" f="$2" w
  w=$(mktemp -d "$tmp/w.XXXXXX")
  local shir out
  shir=$("$CORE" --target shir "$f" 2>/dev/null) || { echo skip; return; }
  [ -n "$shir" ] || { echo skip; return; }
  bash -n "$f" 2>/dev/null || { echo skip; return; }
  printf '%s' "$shir" | "$bin" --target=zsh - > "$w/out.zsh" 2>/dev/null || { echo fail; return; }
  grep -qE 'TODO\(unsupported\)|sh2[A-Za-z_0-9]+\(' "$w/out.zsh" && { echo fail; return; }
  bash "$f" > "$w/bash.out" 2>/dev/null; local brc=$?
  ( cd "$w" && zsh "$w/out.zsh" > "$w/zsh.out" 2>/dev/null ); local zrc=$?
  if cmp -s "$w/bash.out" "$w/zsh.out" && [ "$brc" = "$zrc" ]; then echo ok; else echo fail; fi
}

printf '%-46s %-9s %-9s %s\n' FILE A B VERDICT
printf '%-46s %-9s %-9s %s\n' '----' '---------' '---------' '-------'
for f in "${files[@]}"; do
  b=$(basename "$f")
  aok=0; bok=0
  for _ in $(seq "$REPEATS"); do
    [ "$(run_one "$BIN_A" "$f")" = ok ] && aok=$((aok+1))
    [ "$(run_one "$BIN_B" "$f")" = ok ] && bok=$((bok+1))
  done
  v="same"
  [ "$aok" -ne "$bok" ] && v="=> A" && [ "$bok" -gt "$aok" ] && v="=> B wins"
  printf '%-46s %-9s %-9s %s\n' "$b" "$aok/$REPEATS" "$bok/$REPEATS" "$v"
done