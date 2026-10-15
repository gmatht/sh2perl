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
# The dispatch set the perl/C seams rely on (ir.rs SH_RUN_EXT_PREAMBLE's
# __SH_UU_OK + c_backend's uu list). Grown 2026-09-08: date/sha256sum/
# uname/basename/dirname/cp — uutils supports them all; the .so feature
# gate was the only limiter (UU-FFI.md "still open").
FEATURES="cat wc ls sort sed awk grep date sha256sum uname basename dirname cp"
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

# ── pipeline front end (coreutils-ffi-pipeline) ─────────────────────
# The Perl backend routes LINEAR pipelines (`a | b | c` + redirections)
# through `uu_pipeline_run_str` instead of `bash -c` (check_qx). The
# crate's `cdylib` ABI is Perl-friendly (FFI::Platypus). Its process
# backends exec the `coreutils-ffi-multicall` helper; ship it alongside
# (without it, `Auto` falls back to the in-process `sequential` backend).
(
  cd "$UU_FFI_DIR"
  rustup run "$TOOLCHAIN" cargo build --release -p coreutils-ffi-pipeline
  rustup run "$TOOLCHAIN" cargo build --release -p coreutils-ffi-multicall
)
cp "$UU_FFI_DIR/target/release/libcoreutils_ffi_pipeline.so" "$LIBDIR/"
cp "$UU_FFI_DIR/target/release/coreutils-ffi-multicall" "$LIBDIR/"
chmod +x "$LIBDIR/coreutils-ffi-multicall"

echo "== done. artifacts in $LIBDIR =="
ls -l "$LIBDIR"
