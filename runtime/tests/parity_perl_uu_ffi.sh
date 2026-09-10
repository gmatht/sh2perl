#!/usr/bin/env bash
# parity_perl_uu_ffi.sh — both-mode parity for the perl external-command
# seam (UU-FFI.md): the FFI arm (libcoreutils_ffi.so present) and the
# guarded fork/exec fallback (.so absent) must agree with each other AND
# with bash, per corpus file.
#
# Usage: runtime/tests/parity_perl_uu_ffi.sh [corpus-glob]
set -uo pipefail
HERE="$(cd "$(dirname "$0")/../.." && pwd)"          # sh2perl/
WS="$(cd "$HERE/.." && pwd)"                          # workspace root
CLI="${OTRANSPILERL:-$WS/otranspilerl/target/debug/otranspilerl-cli}"
SO="$HERE/runtime/lib/libcoreutils_ffi.so"
CORPUS="${1:-$HERE/examples/*.sh}"

[ -x "$CLI" ] || { echo "SKIP: no otranspilerl-cli at $CLI"; exit 0; }
[ -e "$SO" ] || { echo "SKIP: no libcoreutils_ffi.so at $SO (build_uu_ffi.sh)"; exit 0; }

export OTRANSPILER_ROOT="$WS"
pass=0; skip=0; fail=0; fails=""
for f in $CORPUS; do
  bash -n "$f" 2>/dev/null || { skip=$((skip+1)); continue; }
  "$CLI" --target pl "$f" 2>/dev/null > /tmp/ppuu_$$.pl || { skip=$((skip+1)); continue; }
  [ -s /tmp/ppuu_$$.pl ] || { skip=$((skip+1)); continue; }
  bs=$(bash "$f" 2>/dev/null)
  ffi=$(SH2_UU_LIB="$SO" perl /tmp/ppuu_$$.pl 2>/dev/null)
  fb=$(env -u SH2_UU_LIB perl /tmp/ppuu_$$.pl 2>/dev/null)
  rm -f /tmp/ppuu_$$.pl
  if [ "$ffi" = "$fb" ] && [ "$ffi" = "$bs" ]; then
    pass=$((pass+1))
  else
    fail=$((fail+1)); fails="$fails $(basename "$f")"
  fi
done
echo "PERL-UU-PARITY: $pass passed, $fail failed, $skip skipped (FFI arm ≡ fallback arm ≡ bash)"
[ $fail -gt 0 ] && { echo "failed:$fails"; exit 1; }
exit 0
