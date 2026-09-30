#!/usr/bin/env bash
# fish_gate.sh — local mirror of setup_backends.sh's fish backend gate.
#
# The canonical gate is setup_backends.sh (workspace root, single-owner).
# This is a worktree-local reimplementation of the SAME contract so a fish
# worker can iterate without contending for the workspace gate:
#
#   1. emit A1 with the CORE otranspilerl (--target shir) — core truth
#   2. render with THIS worktree's shir_render --target=fish
#   3. STUB GATE: sh2*() / TODO(unsupported) in the output = FAIL
#   4. EQUIVALENCE: run the render under fish, diff stdout vs bash
#
# Usage: harness/fish_gate.sh [file-prefix] [--verbose]
set -uo pipefail

WT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ROOT="$WT/../../.."                 # /home/llm/sh2loop
CORE="$ROOT/otranspilerl/target/debug/otranspilerl-cli"
RENDER="$WT/target/debug/shir_render"
SH2="$ROOT/sh2loop/harness/setsid-tree.sh"   # detached run tree (optional)
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

pass=0; fail=0; skip=0; stub=0; eqfail=0; emitfail=0
tmp=$(mktemp -d "$WT/.fishgate.XXXXXX"); trap 'rm -rf "$tmp"' EXIT

for f in "${corpus[@]}"; do
  base=$(basename "$f")

  # 1. core A1 emit — an emit failure is a MEASUREMENT failure, not a skip.
  shir=$("$CORE" --target shir "$f" 2>/dev/null) || { emitfail=$((emitfail+1)); fail=$((fail+1)); echo "  EMIT-FAIL $base"; continue; }
  [ -z "$shir" ] && { emitfail=$((emitfail+1)); fail=$((fail+1)); echo "  EMIT-EMPTY $base"; continue; }

  # unparseable source is not a translation target (bash exits 2 pre-run).
  bash -n "$f" 2>/dev/null || { skip=$((skip+1)); continue; }

  # 2. render through the worktree renderer
  if ! out=$(printf '%s' "$shir" | "$RENDER" --target=fish - 2>/dev/null); then
    fail=$((fail+1)); [ $VERBOSE = 1 ] && echo "  RENDER-FAIL $base"; continue
  fi

  # 3. STUB GATE — requires a CALL sh2<ident>( , not a diagnostic string.
  s=$(printf '%s' "$out" | grep -cE 'TODO\(unsupported\)|sh2[A-Za-z_0-9]+\(' || true)
  stub=$((stub + s))
  if [ "$s" -ne 0 ]; then
    fail=$((fail+1)); [ $VERBOSE = 1 ] && echo "  STUB($s) $base"; continue
  fi

  # 4. EQUIVALENCE — both sides from the same hermetic scratch dir.
  scratch="$tmp/run"; rm -rf "$scratch"; mkdir -p "$scratch"
  bash_out=$(cd "$scratch" && timeout 15 bash "$f" 2>/dev/null); bash_rc=$?
  printf '%s' "$out" > "$scratch/_r.fish"
  fish_out=$(cd "$scratch" && timeout 15 fish "$scratch/_r.fish" 2>/dev/null); fish_rc=$?

  if [ "$bash_out" != "$fish_out" ]; then
    eqfail=$((eqfail+1)); fail=$((fail+1))
    if [ $VERBOSE = 1 ]; then
      echo "  EQ-OUT $base"
      diff <(printf '%s\n' "$bash_out") <(printf '%s\n' "$fish_out") | head -12 | sed 's/^/      /'
    fi
    continue
  fi
  # rc parity only for clean scripts (bash's own rc is meaningful for the rest)
  [ "$bash_rc" = 0 ] && [ "$fish_rc" != 0 ] && { eqfail=$((eqfail+1)); fail=$((fail+1)); [ $VERBOSE = 1 ] && echo "  EQ-RC $base (bash=0 fish=$fish_rc)"; continue; }

  pass=$((pass+1))
done

total=$((pass+fail+skip))
echo
echo "[fish] $pass/$total corpus render OK, $fail fail ($stub stubs, $eqfail equiv, 0 static-analysis), $skip skip"
echo "[fish] stubs emitted: $stub; emit-failed: $emitfail"
echo "[fish] passes: $pass"
exit 0
