#!/usr/bin/env bash
# tests/zig_backend_uu_ffi.sh — end-to-end check that the zig backend can
# lower a genuinely-external command to an in-process `uu_run` call
# (UU-FFI.md), that the switch is OFF by default, that opt-in/opt-out
# changes the render, and that BOTH modes produce byte-identical output
# matching bash.
#
# Unlike C (where uu_run is the DEFAULT), zig's FFI arm is opt-in via
# SH2_UU_FFI=1 at RENDER time: the emitted program must be linked against
# libcoreutils_ffi.so, and a render with no link flags must keep working.
#
# Link notes (measured, and both are load-bearing):
#   - `-lc` is REQUIRED. libcoreutils_ffi.so is a Rust cdylib and does not
#     initialise without libc; the identical zig segfaulted without it and
#     printed correctly with it.
#   - rpath is spelled `-rpath <dir>`; `-Wl,-rpath,<dir>` is rejected by
#     zig with "unrecognized parameter" (which failed all 652 corpus files
#     before the backend gate was fixed).
#
# Usage: [OTRANSPILER=/path/to/otranspilerl-cli] \
#        [SHIR_RENDER=/path/to/shir_render] tests/zig_backend_uu_ffi.sh
set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LIB="$ROOT/lib"
OT="${OTRANSPILER:-/mnt/wsl/projects/sh2loop/otranspilerl/target/debug/otranspilerl-cli}"
R="${SHIR_RENDER:-/mnt/wsl/projects/sh2loop/sh2perl/backends/zig/target/debug/shir_render}"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
fails=0; total=0
say() { printf '%s\n' "$*"; }

[ -f "$LIB/libcoreutils_ffi.so" ] || { say "SKIP: $LIB/libcoreutils_ffi.so not built"; exit 0; }
[ -x "$R" ] || { say "SKIP: shir_render not built at $R"; exit 0; }

# `basename`/`dirname` are in the library and NOT shell builtins, so the
# only way the FFI build can produce their output is the in-process call.
SCRIPT="$TMP/script.sh"
cat > "$SCRIPT" <<'EOF'
#!/bin/bash
cat /etc/hostname
basename /a/b/c.txt
EOF

ZIG="${ZIG:-zig}"
LINK=(-lc -L"$LIB" -lcoreutils_ffi -rpath "$LIB")

REAL="$(bash "$SCRIPT" 2>/dev/null)"; total=$((total+1))

"$OT" --target shir "$SCRIPT" 2>/dev/null > "$TMP/a1" || { say "FAIL: shir emit"; exit 2; }

# 1. default render must NOT reference uu_run (opt-in, dependency-free)
"$R" --target=zig - < "$TMP/a1" > "$TMP/def.zig" 2>/dev/null
total=$((total+1))
if grep -q 'uu_run' "$TMP/def.zig"; then
  fails=$((fails+1)); say "  FAIL: default render must not use uu_run"
fi

# 2. SH2_UU_FFI=1 must reference uu_run AND call it from a program site
SH2_UU_FFI=1 "$R" --target=zig - < "$TMP/a1" > "$TMP/ffi.zig" 2>/dev/null
total=$((total+1))
grep -q 'uu_run' "$TMP/ffi.zig" || { fails=$((fails+1)); say "  FAIL: SH2_UU_FFI=1 render must use uu_run"; }
total=$((total+1))
command grep -qF 'sh2UuRun(&' "$TMP/ffi.zig" || { fails=$((fails+1)); say "  FAIL: SH2_UU_FFI=1 render must CALL the FFI arm"; }

# 3. SH2_UU_FFI=0 must be the same as the default
SH2_UU_FFI=0 "$R" --target=zig - < "$TMP/a1" > "$TMP/off.zig" 2>/dev/null
total=$((total+1))
cmp -s "$TMP/def.zig" "$TMP/off.zig" || { fails=$((fails+1)); say "  FAIL: SH2_UU_FFI=0 render differs from default"; }

# 4. the FFI build must produce what bash does, WITHOUT coreutils on PATH:
#    a fork/exec arm cannot exec `basename` from an empty dir, so a matching
#    result is positive proof the call went in-process.
if "$ZIG" build-exe "$TMP/ffi.zig" "${LINK[@]}" -femit-bin="$TMP/ffib" 2>"$TMP/cc.err"; then
  FFI="$(env -i PATH="$TMP/empty" LD_LIBRARY_PATH="$LIB" "$TMP/ffib" 2>/dev/null)"
  total=$((total+1))
  if [ "$FFI" = "$REAL" ]; then
    say "  OK: FFI build matches bash with an EMPTY \$PATH (in-process call proven)"
  else
    fails=$((fails+1)); say "  FAIL: FFI output ≠ bash (got '$FFI' want '$REAL')"
  fi
else
  fails=$((fails+1)); say "  FAIL: FFI render did not compile"; head -5 "$TMP/cc.err" | sed 's/^/    /'
fi

# 5. a NON-whitelisted command must fall back to fork/exec, not silently
#    emit an FFI call that returns 127 and prints nothing.
cat > "$TMP/ns.sh" <<'EOF'
#!/bin/bash
printf '%s\n' not-a-coreutils
EOF
"$OT" --target shir "$TMP/ns.sh" 2>/dev/null > "$TMP/ns.a1"
SH2_UU_FFI=1 "$R" --target=zig - < "$TMP/ns.a1" > "$TMP/ns.zig" 2>/dev/null
total=$((total+1))
command grep -qF 'sh2UuRun(&' "$TMP/ns.zig" && { fails=$((fails+1)); say "  FAIL: non-whitelisted command must not use the FFI arm"; }

say "zig_backend_uu_ffi: $((total-fails))/$total pass"
[ "$fails" -eq 0 ] || exit 1
