#!/usr/bin/env bash
#
# build-wasi.sh — build debashc for WASI: a command module AND a library module.
#
# One source, two artifacts (see src/wasi_api.rs and the `wasi-lib` feature):
#
#   debashc.wasm   WASI command — `wasmtime run --dir . debashc.wasm file --perl foo.sh`
#   debashl.wasm   WASM library — plain C-ABI exports callable from any embedder
#                  (wasmtime/wasmer embedding APIs, Node `node:wasi`, Python
#                  `wasmtime` pkg, C/C++...). Memory contract + JSON result
#                  envelope are documented in src/wasi_api.rs.
#
# Why two artifacts and not one "dual" module?
#   A single core module *can* technically export both `_start` (command entry)
#   and library functions; runtimes that launch it as a command call `_start`,
#   embedders can call the exports directly. But strict runtimes (Node's
#   node:wasi, component tooling) reject a module that exports BOTH `_start`
#   and `_initialize` — a module must be a command *or* a reactor. So the
#   command build (bin target) and the library build (cdylib + `wasi-lib`
#   feature) stay separate, sharing 100% of the parsing/transpiling code.
#
# The ESTree emitter (debashc file --estree / debashc_to_estree) is the
# PLAN.md §1.2 contract — sh2runtime can consume this WASI binary as a tool.
#
# Requires: Rust with the wasm32-wasip1 target (auto-installed on first run).
set -euo pipefail
cd "$(dirname "$0")"

TARGET=wasm32-wasip1
OUT=target/${TARGET}/release

# 1. Ensure the WASI target is installed
if ! rustup target list --installed | grep -qx "${TARGET}"; then
    echo "==> Installing Rust target ${TARGET}"
    rustup target add "${TARGET}"
fi

# 2. Command module (exports _start; clean WASI command)
#    debashc bin lives in the debashcl crate (workspace member `cli/`)
echo "==> Building ${OUT}/debashc.wasm (WASI command)"
cargo build --release --target "${TARGET}" -p debashcl --bin debashc

# 3. Library module (exports _initialize + debashc_* C-ABI functions)
echo "==> Building ${OUT}/debashl.wasm (WASM library)"
cargo build --release --target "${TARGET}" -p debashl --features wasi-lib --lib

echo
echo "Built:"
ls -lh "${OUT}"/debashc.wasm "${OUT}"/debashl.wasm
echo
cat <<'EOF'
Usage:
  # as a WASI command (files need a --dir preopen):
  wasmtime run --dir . target/wasm32-wasip1/release/debashc.wasm file --perl script.sh
  wasmtime run --dir . target/wasm32-wasip1/release/debashc.wasm file --estree script.sh

  # as a WASM library: instantiate debashl.wasm and call
  #   debashc_to_perl(input, len) / debashc_to_estree(input, len)
  # (JSON envelope {"ok":true,"output":...}; free results with debashc_free)
EOF
