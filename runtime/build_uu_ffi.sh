#!/usr/bin/env bash
# build_uu_ffi.sh — build the uu-ffi shared library (libcoreutils_ffi.so)
# into runtime/lib/.
#
# UU-FFI.md proposal: an in-process, cross-backend coreutils binding so the
# C/Go/Python/Perl backends can call genuinely-external commands (sort, sed,
# grep, awk, wc, ls, cat) as a library CALL instead of fork/exec.
#
# The uu-ffi source lives at /root/src/coreutils/uu-ffi (a standalone crate
# over the published uutils utility crates). This script builds it
# reproducibly with the toolchain its dependencies require (edition 2024,
# rustc >= 1.88) and copies the `.so` + header here.
#
# Usage: runtime/build_uu_ffi.sh [UU_FFI_DIR]
#   UU_FFI_DIR defaults to /root/src/coreutils/uu-ffi.
set -euo pipefail

TOOLCHAIN="${UU_FFI_TOOLCHAIN:-1.96.1}"
FEATURES="cat echo wc ls sort sed awk grep"
UU_FFI_DIR="${1:-/root/src/coreutils/uu-ffi}"
HERE="$(cd "$(dirname "$0")" && pwd)"
LIBDIR="$HERE/lib"

echo "== uu-ffi build (proposal UU-FFI.md) =="
echo "   source : $UU_FFI_DIR"
echo "  features: $FEATURES"
echo "  toolchain: $TOOLCHAIN"

[ -d "$UU_FFI_DIR" ] || { echo "ERROR: $UU_FFI_DIR not found" >&2; exit 1; }

# Build the cdylib. `diff`/`cmp` are NOT enabled: the vendored diffutils
# fork does not currently compile against the published utility crates
# (unresolved import diffutilslib::diff — a known blocker, UU-FFI.md §8).
(
  cd "$UU_FFI_DIR"
  rustup run "$TOOLCHAIN" cargo build --release --features "$FEATURES"
)

mkdir -p "$LIBDIR"
cp "$UU_FFI_DIR/target/release/libcoreutils_ffi.so" "$LIBDIR/"
# copy the C header too (the runtime .h can #include <uu_ffi.h>)
cp "$UU_FFI_DIR/coreutils-ffi/uu_ffi.h" "$LIBDIR/" 2>/dev/null \
  || cp "$UU_FFI_DIR/src/uu_ffi.h" "$LIBDIR/" 2>/dev/null \
  || true

echo "== done. artifacts in $LIBDIR =="
ls -l "$LIBDIR"
