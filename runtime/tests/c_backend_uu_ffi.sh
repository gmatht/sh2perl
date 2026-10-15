#!/usr/bin/env bash
# tests/c_backend_uu_ffi.sh — end-to-end check that the C backend lowers
# genuinely-external commands to in-process `_sh_uu_run` calls (not
# `bash -c`) BY DEFAULT, with opt-out via SH2_UU_FFI=0 at RENDER time,
# and that BOTH modes produce byte-identical output matching bash.
#
# UU-FFI.md §5/§9: C is the first consumer. The uu-ffi arm is the default
# (dynamic words included — argv is built at RUNTIME via value_c); the
# shell-out path remains as the SH2_UU_FFI=0 fallback for glob words,
# env-prefixes, and commands outside the built library's set.
#
# Usage: [OTRANSPILER=/path/to/otranspilerl-cli] tests/c_backend_uu_ffi.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LIB="$ROOT/lib"
OT="${OTRANSPILER:-/home/llm/sh2loop/otranspilerl/target/debug/otranspilerl-cli}"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
fails=0; total=0

SCRIPT="$TMP/script.sh"
cat > "$SCRIPT" <<'EOF'
#!/bin/bash
sort /etc/hostname
grep -i root /etc/passwd
sed s/root/ROOT/ /etc/hostname
echo "all done"
EOF

# reference output (real bash)
REAL="$(bash "$SCRIPT" 2>/dev/null)"; total=$((total+1))

# default render: must use uu_run for the external commands, and match bash
"$OT" "$SCRIPT" - --target c 2>/dev/null > "$TMP/def.c" || { echo "default render failed"; exit 2; }
total=$((total+1))
grep -q "_sh_uu_run" "$TMP/def.c" || { fails=$((fails+1)); echo "  FAIL: default render must use uu_run"; }
if gcc -I"$ROOT" -I"$LIB" -o "$TMP/def" "$TMP/def.c" "$ROOT/uu_run.c" "$LIB/libcoreutils_ffi.so" -lpthread -Wl,-rpath,"$LIB" 2>/dev/null; then
  DEF="$("$TMP/def" 2>/dev/null)"
  total=$((total+1))
  [ "$DEF" = "$REAL" ] || { fails=$((fails+1)); echo "  FAIL: default render output ≠ bash"; }
else
  fails=$((fails+1)); echo "  FAIL: default render did not compile"
fi

# SH2_UU_FFI=0 render: must still shell out (no uu_run), and match bash
SH2_UU_FFI=0 "$OT" "$SCRIPT" - --target c 2>/dev/null > "$TMP/sh.c" || { echo "fallback render failed"; exit 2; }
total=$((total+1))
if grep -q "_sh_uu_run" "$TMP/sh.c"; then
  fails=$((fails+1)); echo "  FAIL: SH2_UU_FFI=0 render must NOT use uu_run (fallback)"
fi
if gcc -o "$TMP/sh" "$TMP/sh.c" 2>/dev/null; then
  SH="$("$TMP/sh" 2>/dev/null)"
  total=$((total+1))
  [ "$SH" = "$REAL" ] || { fails=$((fails+1)); echo "  FAIL: fallback render output ≠ bash"; }
else
  fails=$((fails+1)); echo "  FAIL: fallback render did not compile"
fi

echo "C backend uu-ffi: $total checks, $fails failures"
[ "$fails" -eq 0 ]
