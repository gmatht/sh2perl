#!/usr/bin/env node
// debashcl.wasm — "debashc in three lines of JS" demo.
//
// Build:   ./build-wasi.sh
// Run:     node cli/example-wasi.mjs parse 'echo hi'
//          node cli/example-wasi.mjs lex 'echo hi'
//          node cli/example-wasi.mjs --version
//          node cli/example-wasi.mjs --help
//
// The core is three lines (instantiate → _initialize → debashc_cli_run):
// everything else here is just argv marshalling (C-ABI) and output
// plumbing. The debashcl crate's main_with_args does ALL the command-line
// processing — this file contains zero CLI logic.
//
// node:wasi (preview1) has no filesystem preopens, so file-based commands
// (`file <name>`, `-i <file>`) need wasmtime/wasmer with --dir; the
// string-input commands above work here as-is.

import { WASI } from 'node:wasi';
import { readFile } from 'node:fs/promises';

// stdout/stderr default to fds 1/2: the CLI's output goes straight to the
// terminal; the JSON envelope comes back as the debashc_cli_run return value.
const wasi = new WASI({
  version: 'preview1',
  args: ['debashc', ...process.argv.slice(2)],
});

// ── three lines (modulo loading the file) ─────────────────────────────
const wasm = await readFile(new URL('../target/wasm32-wasip1/release/debashcl.wasm', import.meta.url));
const { instance } = await WebAssembly.instantiate(wasm, { wasi_snapshot_preview1: wasi.wasiImport });
wasi.initialize(instance); // reactor: initializes wasi imports + calls _initialize
// ──────────────────────────────────────────────────────────────────────

// argv → wasm memory: [u32 len][data][0] buffers from debashc_alloc
const enc = new TextEncoder();
const argv = ['debashc', ...process.argv.slice(2)];
const ptrs = [];
for (const a of argv) {
  const b = enc.encode(a);
  const p = instance.exports.debashc_alloc(b.length);
  new Uint8Array(instance.exports.memory.buffer, p, b.length).set(b);
  ptrs.push(p);
}
const table = instance.exports.debashc_alloc(4 * argv.length);
new Uint32Array(instance.exports.memory.buffer, table, argv.length).set(ptrs);

const res = instance.exports.debashc_cli_run(argv.length, table);
const dec = new TextDecoder();
const n = instance.exports.debashc_str_len(res);
console.error(`\n[debashcl.wasm] envelope: ${dec.decode(new Uint8Array(instance.exports.memory.buffer, res, n))}`);

// optional: same call via the JSON convenience export
// const json = enc.encode(JSON.stringify(argv));
// const pj = instance.exports.debashc_alloc(json.length);
// new Uint8Array(instance.exports.memory.buffer, pj, json.length).set(json);
// instance.exports.debashc_cli_run_json(pj, json.length);
// instance.exports.debashc_free(pj);

for (const p of ptrs) instance.exports.debashc_free(p);
instance.exports.debashc_free(table);
instance.exports.debashc_free(res);
