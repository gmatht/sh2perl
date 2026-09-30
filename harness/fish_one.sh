#!/usr/bin/env bash
# fish_one.sh — single-file triage: bash-vs-fish diff for one corpus script.
#
# Renders $1 through the worktree's shir_render --target=fish, runs both
# interpreters in one hermetic scratch dir, and shows the first divergence.
# Used while fixing a category, where the full gate is too coarse.
#
# Usage: harness/fish_one.sh <script.sh> [--src | --render]
set -uo pipefail

WT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CORE="/home/llm/sh2loop/otranspilerl/target/debug/otranspilerl-cli"
export OTRANSPILER_ROOT="/home/llm/sh2loop/otranspilerl"
RENDER="$WT/target/debug/shir_render"

f="$(readlink -f "$1")"; shift
mode="${1:-}"

tmp=$(mktemp -d "$WT/.fishone.XXXXXX"); trap 'rm -rf "$tmp"' EXIT
shir="$tmp/a1.json"; out="$tmp/_r.fish"

"$CORE" --target shir "$f" > "$shir" 2>/dev/null || { echo "EMIT-FAIL $f"; exit 2; }
"$RENDER" --target=fish "$shir" > "$out" 2>/dev/null || { echo "RENDER-FAIL $f"; exit 2; }

[ "$mode" = "--src" ] && { cat "$out"; exit 0; }

run="$tmp/run"; mkdir -p "$run"; cp "$out" "$run/_r.fish"
bo=$(cd "$run" && timeout 15 bash "$f" 2>&1); brc=$?
fo=$(cd "$run" && timeout 15 fish "$run/_r.fish" 2>&1); frc=$?
# drop the path/line noise fish prefixes on errors
fo=$(printf '%s\n' "$fo" | sed -E "s#^[^:]*(fish|_r\.fish): ?#[#")

if [ "$bo" = "$fo" ]; then echo "OK   $(basename "$f")"; exit 0; fi
echo "DIFF $(basename "$f")  (bash rc=$brc fish rc=$frc)"
diff <(printf '%s\n' "$bo") <(printf '%s\n' "$fo") | head -"${DIFFLINES:-20}"
