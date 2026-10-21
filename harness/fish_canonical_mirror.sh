#!/usr/bin/env bash
# fish_canonical_mirror.sh — measure the fish backend on the canonical gate's
# EXACT contract (setup_backends.sh --backend-gate fish) without its redundant
# core/worktree rebuilds (which queue behind the fleet's .build-lock).
#
# Contract replicated from setup_backends.sh lines ~1730-2160:
#   1. A1 via the CORE otranspilerl-cli --target shir (main tree)
#   2. bash -n gate: unparseable source is a SKIP
#   3. render via the WORKTREE shir_render --target=fish -
#   4. STUB gate: TODO(unsupported) | sh2<ident>(  -> FAIL
#   5. EQUIVALENCE: bash "$f" vs fish with SH2_ARGV0="$f", stdout only,
#      rc-equal for fish (it is in the rc-equality group), 124 -> fail
set -uo pipefail
ROOT=/mnt/wsl/projects/sh2loop
WT="$ROOT/sh2perl/backends/fish"
CORE="$ROOT/otranspilerl/target/debug/otranspilerl-cli"
RENDER="$WT/target/debug/shir_render"
export OTRANSPILER_ROOT="$ROOT/otranspilerl"
VERBOSE=${VERBOSE:-0}

mapfile -t corpus < <({ ls "$ROOT"/sh2perl/examples/*.sh
                        ls "$ROOT"/sh2perl/frontends/*/testdata/*.sh; } 2>/dev/null | sort)

pass=0; fail=0; skip=0; stub_files=0; eqfail=0
fails=""
tmp=$(mktemp -d /tmp/fishmirror.XXXXXX); trap 'rm -rf "$tmp"' EXIT

for f in "${corpus[@]}"; do
  base=$(basename "$f")
  shir=$("$CORE" --target shir "$f" 2>/dev/null) || { fail=$((fail+1)); fails="$f $fails"; echo "  EMIT-FAIL $base"; continue; }
  [ -z "$shir" ] && { fail=$((fail+1)); fails="$f $fails"; echo "  EMIT-EMPTY $base"; continue; }
  bash -n "$f" 2>/dev/null || { skip=$((skip+1)); continue; }
  if ! out=$(printf '%s' "$shir" | "$RENDER" --target=fish - 2>/dev/null); then
    fail=$((fail+1)); fails="$f $fails"; [ "$VERBOSE" = 1 ] && echo "  RENDER-FAIL $base"; continue
  fi
  s=$(printf '%s' "$out" | grep -cE 'TODO\(unsupported\)|sh2[A-Za-z_0-9]+\(' || true)
  if [ "$s" -ne 0 ]; then
    fail=$((fail+1)); stub_files=$((stub_files+1)); fails="$f $fails"
    [ "$VERBOSE" = 1 ] && echo "  STUB($s) $base"; continue
  fi
  printf '%s' "$out" > "$tmp/r.fish"
  # BASH_ENV is deliberately cleared for the bash side: this agent's shell
  # exports one carrying `grep`/`find` WRAPPER FUNCTIONS (pir's shell
  # integration). Those wrappers exist only in bash — fish runs the real
  # binaries — so an inherited BASH_ENV makes the two sides differ for reasons
  # that have nothing to do with the renderer (015_grep_advanced's
  # `--color=always` emitted 01;31/m/K under real GNU grep vs 0m;1;31 under
  # the wrapper; 016_grep_basic gained a spurious line). The canonical gate
  # runs from systemd, which has no BASH_ENV at all.
  # CWD PARITY WITH THE CANONICAL GATE: fish runs from $WORKSPACE, not from a
  # scratch dir (only perl/c/rust get an eq_scratch there). Running both sides
  # in a temp dir hid the workspace-glob class entirely -- 058/064 glob
  # '*.{txt,log,dat}' and the workspace root holds ~77 such files, five of them
  # written by the pass itself. Measuring from a clean dir is a DIFFERENT
  # contract from the one the gate scores, so replicate the gate.
  bo=$(cd "$ROOT" && env -u BASH_ENV timeout -k 2 15 bash "$f" 2>/dev/null); brc=$?
  fo=$(cd "$ROOT" && SH2_ARGV0="$f" timeout -k 2 15 fish "$tmp/r.fish" 2>/dev/null); frc=$?
  if [ "$frc" = 124 ] || [ "$brc" = 124 ] || [ "$frc" != "$brc" ] || [ "$bo" != "$fo" ]; then
    fail=$((fail+1)); eqfail=$((eqfail+1)); fails="$f $fails"
    if [ "$VERBOSE" = 1 ]; then
      echo "  EQ-FAIL $base (bash rc=$brc fish rc=$frc)"
      diff <(printf '%s\n' "$bo") <(printf '%s\n' "$fo") | head -8 | sed 's/^/      /'
    fi
    continue
  fi
  pass=$((pass+1))
done

echo "[fish] $pass/$((pass+skip+fail)) corpus render OK, $fail fail ($stub_files stubs, $eqfail equiv), $skip skip"
echo "fails:"
for x in $fails; do echo "$x"; done
