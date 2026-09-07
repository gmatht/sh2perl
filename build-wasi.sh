#!/usr/bin/env bash
#
# build-wasi.sh — build the transpiler for WASI: a command module AND
# library modules.
#
# Two artifacts (see src/wasi_api.rs and ../otranspilerl/src/wasi.rs):
#
#   otranspilerl-cli.wasm  WASI command — the full CLI (the otranspilerl
#                          workspace crate's bin; flags like
#                          `--target pl foo.sh` / `--target estree foo.sh`)
#   debashl.wasm           WASM library — plain C-ABI exports callable from
#                          any embedder (wasmtime/wasmer embedding APIs, Node
#                          `node:wasi`, Python `wasmtime` pkg, C/C++...).
#                          Memory contract + JSON result envelope are
#                          documented in src/wasi_api.rs. Core transpiler
#                          ONLY (to_perl/to_estree/lex) — no CLI processing.
#   otranspilerl.wasm      WASM library — the unified otranspilerl_* ABI:
#                          shir/lex/compile/render/shir_opt plus
#                          otranspilerl_cli (the FULL command-line
#                          processing as a library call); see
#                          ../otranspilerl/src/wasi.rs.
#
# Why separate command and library modules?
#   A single core module *can* technically export both `_start` (command
#   entry) and library functions; runtimes that launch it as a command call
#   `_start`, embedders can call the exports directly. But strict runtimes
#   (Node's node:wasi, component tooling) reject a module that exports BOTH
#   `_start` and `_initialize` — a module must be a command *or* a reactor.
#   So the command build (bin target) and the library builds (cdylib +
#   `wasi-lib`) stay separate, sharing 100% of the parsing/transpiling code.
#
# The ESTree emitter (otranspilerl-cli --target estree) is the PLAN.md §1.2
# contract — sh2runtime can consume the WASI binary as a tool.
#
# Requires: Rust with the wasm32-wasip1 target (auto-installed on first run).
set -euo pipefail
cd "$(dirname "$0")"

TARGET=wasm32-wasip1
OUT=target/${TARGET}/release
OTRANS_CRATE="$(pwd)/../otranspilerl"

# 1. Ensure the WASI target is installed
if ! rustup target list --installed | grep -qx "${TARGET}"; then
    echo "==> Installing Rust target ${TARGET}"
    rustup target add "${TARGET}"
fi

# 2. Command module (exports _start; clean WASI command)
echo "==> Building ${OUT}/otranspilerl-cli.wasm (WASI command)"
cargo build --release --target "${TARGET}" --manifest-path "${OTRANS_CRATE}/Cargo.toml" --bin otranspilerl-cli

# 3. Library module (exports _initialize + sh2perl_* C-ABI functions)
echo "==> Building ${OUT}/debashl.wasm (WASM library — core transpiler)"
cargo build --release --target "${TARGET}" --features wasi-lib --lib

# 4. Unified library module (reactor: _initialize + otranspilerl_* exports —
#    the unified render ABI + full CLI call, for JS/Python/C embedders;
#    see ../otranspilerl/src/wasi.rs)
echo "==> Building ${OUT}/otranspilerl.wasm (WASM library — unified ABI + full CLI)"
cargo build --release --target "${TARGET}" --manifest-path "${OTRANS_CRATE}/Cargo.toml" --lib

echo
echo "Built:"
ls -lh "${OUT}"/otranspilerl-cli.wasm "${OUT}"/debashl.wasm "${OUT}"/otranspilerl.wasm
echo
cat <<'EOF'
Usage:
  # as a WASI command (files need a --dir preopen):
  wasmtime run --dir . target/wasm32-wasip1/release/otranspilerl-cli.wasm --target pl script.sh
  wasmtime run --dir . target/wasm32-wasip1/release/otranspilerl-cli.wasm --target estree script.sh

  # as a WASM library: instantiate debashl.wasm and call
  #   sh2perl_to_perl(input, len) / sh2perl_to_estree(input, len)
  # (JSON envelope {"ok":true,"output":...}; free results with sh2perl_free)

  # as a WASM library with the UNIFIED ABI + FULL CLI: instantiate
  # otranspilerl.wasm and call
  #   otranspilerl_shir / otranspilerl_lex / otranspilerl_compile /
  #   otranspilerl_render / otranspilerl_shir_opt
  #   otranspilerl_cli(args, args_len)   # the full CLI, args newline-joined
  # (JSON envelopes; see ../otranspilerl/src/wasi.rs for the memory contract)
EOF
