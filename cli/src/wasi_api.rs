//! WASI library ABI for the debashcl CLI layer.
//!
//! debashl.wasm exposes the transpiler core (`debashc_to_perl` /
//! `debashc_to_estree`) but skips the CLI; debashc.wasm runs the CLI but
//! only as a process (`_start`). This module is the missing third
//! artifact: **the full command-line processing as a library call**.
//!
//! `debashc_cli_run(argc, argv)` runs the same `main_with_args` dispatch
//! the debashc binary uses, so an embedder can implement debashc in three
//! lines of JS:
//!
//! ```js
//! const { instance } = await WebAssembly.instantiate(wasm, { wasi_snapshot_preview1: wasi.wasiImport });
//! instance.exports._initialize?.();
//! const res = instance.exports.debashc_cli_run(argc, argv); // → {"ok":true,"exit":0}
//! ```
//!
//! Same memory contract as debashl's src/wasi_api.rs: every returned /
//! allocated pointer points at the `data` byte of a
//! `[u32 len][data][0]` NUL-terminated UTF-8 buffer. Read the payload
//! length with `debashc_str_len`, release with `debashc_free`. Inputs are
//! written into memory obtained from `debashc_alloc` (which uses the same
//! layout, so one `debashc_free` path serves both).
//!
//! CLI output (help text, generated Perl, ESTree JSON, AST dumps, parse
//! errors) goes to the WASI stdout/stderr streams the embedder configured.
//!
//! ## Platform notes
//!
//! - `node:wasi` (preview1) has no filesystem preopens → file-based
//!   commands (`file <name>`, `-i <file>`) need an embedder with preopens
//!   (wasmtime/wasmer `--dir`), same as debashc.wasm. String-input
//!   commands (`parse <input>`, `lex <input>`, `--version`, `--help`)
//!   work everywhere.
//! - Commands that exec `perl` (`file` without `--perl`/`--estree`, `-i`
//!   without `-o`) degrade to print-only on WASI: preview1 has no
//!   fork/exec, so the spawn silently no-ops.
//! - Error paths that call `process::exit(1)` terminate the wasm instance
//!   via `proc_exit` (the embedder sees a nonzero exit).
//!
//! Compiled only for wasm32-wasip1 with the `wasi-cli` feature (lib.rs);
//! the debashc bin keeps using plain `main_with_args`.
//!
//! See cli/example-wasi.mjs for a working JS embedder.

use std::alloc::{Layout, alloc, dealloc};
use std::ffi::CStr;
use std::ptr::null_mut;
use std::slice;

use crate::main_with_args;

/// Allocate a `[u32 len][data][0]` buffer and return a pointer to `data`.
fn alloc_string(s: &str) -> *mut u8 {
    let bytes = s.as_bytes();
    let n = bytes.len();
    let total = 4 + n + 1;
    let layout = Layout::from_size_align(total, 4).expect("valid layout");
    // Safety: total >= 5 so `alloc` is well-defined; the layout used here
    // exactly matches the one reconstructed in `debashc_free`.
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        return null_mut();
    }
    unsafe {
        *(ptr as *mut u32) = (n as u32).to_le();
        ptr.add(4).copy_from_nonoverlapping(bytes.as_ptr(), n);
        *ptr.add(4 + n) = 0;
        ptr.add(4)
    }
}

fn ok_json(exit: i32) -> String {
    format!(r#"{{"ok":true,"exit":{exit}}}"#)
}

fn err_json(e: &dyn std::fmt::Display) -> String {
    format!(
        r#"{{"ok":false,"error":{}}}"#,
        serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"\"".into())
    )
}

/// Reactor entry point: runtimes that require it (e.g. Node's `node:wasi`)
/// call `_initialize` once before allowing wasi imports (like `random_get`)
/// to be used. The binary build gets `_start` instead.
#[no_mangle]
pub extern "C" fn _initialize() {}

/// `debashc_alloc(len) -> *mut u8` — reserve `len` payload bytes in wasm
/// linear memory (as a `[u32 len][data][0]` buffer) for the embedder to
/// fill (argv strings, JSON payloads). Free with `debashc_free`.
#[no_mangle]
pub extern "C" fn debashc_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return null_mut();
    }
    let total = 4 + len + 1;
    let layout = Layout::from_size_align(total, 4).expect("valid layout");
    // Safety: same layout contract as alloc_string/debashc_free.
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        return null_mut();
    }
    unsafe {
        *(ptr as *mut u32) = (len as u32).to_le();
        *ptr.add(4 + len) = 0;
        ptr.add(4)
    }
}

/// `debashc_cli_run(argc, argv) -> *mut u8` — run the full debashc CLI
/// with the given argv (`argv[0]` = program name, e.g. `"debashc"`).
/// Each `argv[i]` points at a NUL-terminated UTF-8 string in wasm memory.
/// Returns a JSON envelope `{"ok":true,"exit":0}` (error paths either set
/// `ok:false` or terminate via `proc_exit`); CLI output rides on the WASI
/// stdout/stderr streams.
#[no_mangle]
pub extern "C" fn debashc_cli_run(argc: usize, argv: *const *const u8) -> *mut u8 {
    if argc == 0 || argv.is_null() {
        return alloc_string(&err_json(&"debashc_cli_run: no argv provided"));
    }
    let mut args = Vec::with_capacity(argc);
    for i in 0..argc {
        // Safety: embedder contract — argv[i] is a valid pointer to a
        // NUL-terminated string for 0 <= i < argc.
        let s = unsafe { CStr::from_ptr(*argv.add(i) as *const i8) };
        args.push(s.to_string_lossy().into_owned());
    }
    main_with_args(args);
    alloc_string(&ok_json(0))
}

/// `debashc_cli_run_json(input, input_len) -> *mut u8` — convenience for
/// script embedders: `input` is a JSON array of argv strings
/// (e.g. `["debashc","file","--estree","x.sh"]`). Same envelope and
/// stdout behavior as `debashc_cli_run`.
#[no_mangle]
pub extern "C" fn debashc_cli_run_json(input: *const u8, input_len: usize) -> *mut u8 {
    if input.is_null() || input_len == 0 {
        return alloc_string(&err_json(&"debashc_cli_run_json: empty input"));
    }
    // Safety: embedder contract — input points at input_len valid bytes.
    let input = unsafe { slice::from_raw_parts(input, input_len) };
    match serde_json::from_slice::<Vec<String>>(input) {
        Ok(args) => {
            if args.is_empty() {
                return alloc_string(&err_json(&"debashc_cli_run_json: empty argv array"));
            }
            main_with_args(args);
            alloc_string(&ok_json(0))
        }
        Err(e) => alloc_string(&err_json(&e)),
    }
}

/// `debashc_str_len(ptr) -> u32` — payload length in bytes (excludes NUL).
#[no_mangle]
pub extern "C" fn debashc_str_len(ptr: *const u8) -> u32 {
    if ptr.is_null() {
        return 0;
    }
    // Safety: embedder contract — ptr came from debashc_alloc/alloc_string.
    unsafe { u32::from_le(*(ptr.sub(4) as *const u32)) }
}

/// `debashc_free(ptr)` — release a buffer returned by `debashc_alloc` /
/// `debashc_cli_run` / `debashc_cli_run_json`.
#[no_mangle]
pub extern "C" fn debashc_free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    // Safety: same layout as alloc_string/debashc_alloc.
    unsafe {
        let n = u32::from_le(*(ptr.sub(4) as *const u32)) as usize;
        let total = 4 + n + 1;
        let layout = Layout::from_size_align(total, 4).expect("valid layout");
        dealloc(ptr.sub(4), layout);
    }
}
