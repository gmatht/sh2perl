#!/usr/bin/env bash
# tests/c_backend_uu_ffi.sh — end-to-end check that the C backend, when
# SH2_UU_FFI=1 is set at RENDER time, lowers a static genuinely-external
# command to an in-process `sh2_uu_run` call (not `bash -c`), and that BOTH
# modes (default and uu-ffi) produce byte-identical output matching bash.
#
# UU-FFI.md §5/§9: C is the first consumer. This keeps the property that the
# default (env-unset) render is byte-identical to before the change — the
# no-regression gate — while proving the opt-in path works and matches.
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

# default render: must still shell out (no uu_run), and match bash
"$OT" "$SCRIPT" - --target c 2>/dev/null > "$TMP/def.c" || { echo "default render failed"; exit 2; }
total=$((total+1))
if grep -q "sh2_uu_run" "$TMP/def.c"; then
  fails=$((fails+1)); echo "  FAIL: default render must NOT use uu_run (no-regression)"
fi
if gcc -o "$TMP/def" "$TMP/def.c" 2>/dev/null; then
  DEF="$("$TMP/def" 2>/dev/null)"
  total=$((total+1))
  [ "$DEF" = "$REAL" ] || { fails=$((fails+1)); echo "  FAIL: default render output ≠ bash"; }
else
  fails=$((fails+1)); echo "  FAIL: default render did not compile"
fi

# SH2_UU_FFI render: must use uu_run for the static external commands
SH2_UU_FFI=1 "$OT" "$SCRIPT" - --target c 2>/dev/null > "$TMP/uu.c" || { echo "uu render failed"; exit 2; }
total=$((total+1))
grep -q "sh2_uu_run" "$TMP/uu.c" || { fails=$((fails+1)); echo "  FAIL: SH2_UU_FFI render must use uu_run"; }
if gcc -I"$ROOT" -I"$LIB" -o "$TMP/uu" "$TMP/uu.c" "$ROOT/uu_run.c" "$LIB/libcoreutils_ffi.so" -lpthread -Wl,-rpath,"$LIB" 2>/dev/null; then
  UU="$("$TMP/uu" 2>/dev/null)"
  total=$((total+1))
  [ "$UU" = "$REAL" ] || { fails=$((fails+1)); echo "  FAIL: uu_ffi render output ≠ bash"; }
else
  fails=$((fails+1)); echo "  FAIL: uu_ffi render did not compile"
fi

echo "C backend uu-ffi: $total checks, $fails failures"
[ "$fails" -eq 0 ]
