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
  bo=$(cd "$tmp" && timeout -k 2 15 bash "$f" 2>/dev/null); brc=$?
  fo=$(cd "$tmp" && SH2_ARGV0="$f" timeout -k 2 15 fish "$tmp/r.fish" 2>/dev/null); frc=$?
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
