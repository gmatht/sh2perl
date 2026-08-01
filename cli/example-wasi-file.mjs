#!/usr/bin/env node
// debashcl.wasm — file-based commands without fs preopens.
//
// node:wasi has no filesystem preopens, so `file --estree x.sh` can't read
// x.sh inside the wasm. Instead the embedder reads the .sh with node's own
// fs and hands the bytes to `debashc_cli_run_with_input`, using the CLI's
// `-` (stdin) convention as the filename. Same CLI processing as the
// native binary — this file contains zero CLI logic.
//
// Build: ./build-wasi.sh
// Run:   node cli/example-wasi-file.mjs examples/000__01_basic.sh --estree
//        node cli/example-wasi-file.mjs examples/000__01_basic.sh --perl
//        (stdout = ESTree JSON / generated Perl)

import { WASI } from 'node:wasi';
import { readFile } from 'node:fs/promises';

const [script, mode = '--estree'] = process.argv.slice(2);
const content = await readFile(script); // node's fs, not the wasm's

const wasi = new WASI({ version: 'preview1', args: ['debashc'] });
const wasm = await readFile(new URL('../target/wasm32-wasip1/release/debashcl.wasm', import.meta.url));
const { instance } = await WebAssembly.instantiate(wasm, { wasi_snapshot_preview1: wasi.wasiImport });
wasi.initialize(instance);

// argv → wasm memory ([u32 len][data][0] buffers from debashc_alloc)
const enc = new TextEncoder();
const argv = ['debashc', 'file', mode, '-'];
const ptrs = [];
for (const a of argv) {
  const b = enc.encode(a);
  const p = instance.exports.debashc_alloc(b.length);
  new Uint8Array(instance.exports.memory.buffer, p, b.length).set(b);
  ptrs.push(p);
}
const table = instance.exports.debashc_alloc(4 * argv.length);
new Uint32Array(instance.exports.memory.buffer, table, argv.length).set(ptrs);

// script content → wasm memory
const pIn = instance.exports.debashc_alloc(content.length);
new Uint8Array(instance.exports.memory.buffer, pIn, content.length).set(content);

const res = instance.exports.debashc_cli_run_with_input(argv.length, table, pIn, content.length);
const n = instance.exports.debashc_str_len(res);
console.error(`\n[debashcl.wasm] envelope: ${new TextDecoder().decode(new Uint8Array(instance.exports.memory.buffer, res, n))}`);

for (const p of ptrs) instance.exports.debashc_free(p);
instance.exports.debashc_free(table);
instance.exports.debashc_free(pIn);
instance.exports.debashc_free(res);
