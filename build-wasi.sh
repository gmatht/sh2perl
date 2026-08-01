#!/usr/bin/env bash
#
# build-wasi.sh — build debashc for WASI: a command module AND a library module.
#
# One source, three artifacts (see src/wasi_api.rs, cli/src/wasi_api.rs and
# the `wasi-lib`/`wasi-cli` features):
#
#   debashc.wasm   WASI command — `wasmtime run --dir . debashc.wasm file --perl foo.sh`
#   debashl.wasm   WASM library — plain C-ABI exports callable from any embedder
#                  (wasmtime/wasmer embedding APIs, Node `node:wasi`, Python
#                  `wasmtime` pkg, C/C++...). Memory contract + JSON result
#                  envelope are documented in src/wasi_api.rs. Core transpiler
#                  ONLY (to_perl/to_estree/lex) — no CLI processing.
#   debashcl.wasm  WASM library — the FULL command-line processing as a
#                  library call (debashc_cli_run(argc, argv) / _run_json),
#                  i.e. "debashc in three lines of JS"; see cli/src/wasi_api.rs
#                  + cli/example-wasi.mjs.
#
# Why three artifacts and not one "dual" module?
#   A single core module *can* technically export both `_start` (command entry)
#   and library functions; runtimes that launch it as a command call `_start`,
#   embedders can call the exports directly. But strict runtimes (Node's
#   node:wasi, component tooling) reject a module that exports BOTH `_start`
#   and `_initialize` — a module must be a command *or* a reactor. So the
#   command build (bin target) and the library builds (cdylib + `wasi-lib` /
#   `wasi-cli` features) stay separate, sharing 100% of the
#   parsing/transpiling code.
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
echo "==> Building ${OUT}/debashl.wasm (WASM library — core transpiler)"
cargo build --release --target "${TARGET}" -p debashl --features wasi-lib --lib

# 4. CLI-layer library module (reactor: _initialize + debashc_cli_run(_json) —
#    the FULL command-line processing as a library call, for JS/Python/C
#    embedders; see cli/src/wasi_api.rs + cli/example-wasi.mjs)
echo "==> Building ${OUT}/debashcl.wasm (WASM library — full CLI ABI)"
cargo build --release --target "${TARGET}" -p debashcl --features wasi-cli --lib

echo
echo "Built:"
ls -lh "${OUT}"/debashc.wasm "${OUT}"/debashl.wasm "${OUT}"/debashcl.wasm
echo
cat <<'EOF'
Usage:
  # as a WASI command (files need a --dir preopen):
  wasmtime run --dir . target/wasm32-wasip1/release/debashc.wasm file --perl script.sh
  wasmtime run --dir . target/wasm32-wasip1/release/debashc.wasm file --estree script.sh

  # as a WASM library: instantiate debashl.wasm and call
  #   debashc_to_perl(input, len) / debashc_to_estree(input, len)
  # (JSON envelope {"ok":true,"output":...}; free results with debashc_free)

  # as a WASM library with FULL CLI processing: instantiate debashcl.wasm and call
  #   debashc_cli_run(argc, argv) / debashc_cli_run_json(json_args, len)
  #   debashc_cli_run_with_input(argc, argv, input, len)  # file cmds via stdin `-`
  # (JSON envelope {"ok":true,"exit":0}; CLI output on the wasi stdout stream)
  # demos: node cli/example-wasi.mjs parse 'echo hi'
  #        node cli/example-wasi-file.mjs examples/001_simple.sh --estree
EOF
