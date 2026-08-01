//! WASI library ABI for debashc.
//!
//! Plain C-ABI exports (no wasm-bindgen, no JS glue) so the *same crate* can
//! be loaded as a library from any WASI embedder: wasmtime, wasmer, Node's
//! `node:wasi`, Python (`wasmtime` pkg), C/C++, ...
//!
//! ## Memory contract
//!
//! Every `debashc_*` string export returns a pointer into WASM linear memory
//! pointing at a NUL-terminated UTF-8 buffer laid out as:
//!
//! ```text
//! [u32 data_len, little-endian][data bytes][0]
//! ^ ptr-4                     ^ returned ptr
//! ```
//!
//! - `debashc_str_len(ptr)` → byte length of `data` (excludes the NUL).
//! - `debashc_free(ptr)` → release the buffer (required for every returned ptr).
//!
//! ## Results
//!
//! Errors are surfaced as JSON envelopes so any consumer can parse them:
//!
//! ```json
//! {"ok":true,"output":"..."}   /   {"ok":false,"error":"..."}
//! ```
//!
//! This module is compiled only for `wasm32-wasip1` builds (see lib.rs).

use std::alloc::{Layout, alloc, dealloc};
use std::ptr::null_mut;
use std::slice;

use crate::estree::ast_to_estree_json;
use crate::{Generator, Lexer, Parser};

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

fn ok_json(output: &str) -> String {
    format!(
        r#"{{"ok":true,"output":{}}}"#,
        serde_json::to_string(output).unwrap_or_else(|_| "\"\"".into())
    )
}

fn err_json(e: &dyn std::fmt::Display) -> String {
    format!(
        r#"{{"ok":false,"error":{}}}"#,
        serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"\"".into())
    )
}

/// Reactor entry point: runtimes that require it (e.g. Node's `node:wasi`)
/// call `_initialize` once before allowing wasi imports (like `random_get`)
/// to be used. The binary build gets `_start` instead; this module is the
/// library/repository counterpart.
#[no_mangle]
pub extern "C" fn _initialize() {}

/// `debashc_version() -> *mut u8` — `"debashc <crate-version>"` (JSON envelope,
/// needs `debashc_free`).
#[no_mangle]
pub extern "C" fn debashc_version() -> *mut u8 {
    alloc_string(&ok_json(concat!("debashc ", env!("CARGO_PKG_VERSION"))))
}

/// `debashc_to_perl(input, input_len) -> *mut u8` — transpile shell → Perl.
/// Returns a JSON envelope; `output` holds the generated Perl source.
#[no_mangle]
pub extern "C" fn debashc_to_perl(input: *const u8, input_len: usize) -> *mut u8 {
    let input = unsafe { slice::from_raw_parts(input, input_len) };
    let input = String::from_utf8_lossy(input);
    match Parser::new(&input).parse() {
        Ok(commands) => {
            let mut gen = Generator::new();
            alloc_string(&ok_json(&gen.generate(&commands)))
        }
        Err(e) => alloc_string(&err_json(&e)),
    }
}

/// `debashc_to_estree(input, input_len) -> *mut u8` — shell → **standard
/// ESTree JSON** (the PLAN.md §1.2 contract; `sh2.*` runtime namespace).
/// `output` holds the raw ESTree JSON document.
#[no_mangle]
pub extern "C" fn debashc_to_estree(input: *const u8, input_len: usize) -> *mut u8 {
    let input = unsafe { slice::from_raw_parts(input, input_len) };
    let input = String::from_utf8_lossy(input);
    match Parser::new(&input).parse() {
        Ok(commands) => match ast_to_estree_json(&commands) {
            Ok(json) => alloc_string(&ok_json(&json)),
            Err(e) => alloc_string(&err_json(&e)),
        },
        Err(e) => alloc_string(&err_json(&e)),
    }
}

/// `debashc_lex(input, input_len) -> *mut u8` — token dump (debug helper).
#[no_mangle]
pub extern "C" fn debashc_lex(input: *const u8, input_len: usize) -> *mut u8 {
    let input = unsafe { slice::from_raw_parts(input, input_len) };
    let input = String::from_utf8_lossy(input);
    let mut lexer = Lexer::new(&input);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.peek() {
        let token_text = lexer.get_current_text().unwrap_or_default().to_string();
        tokens.push(format!("{:?}('{}')", token, token_text));
        lexer.next();
    }
    alloc_string(&ok_json(&tokens.join("\n")))
}

/// `debashc_str_len(ptr) -> u32` — payload length in bytes (excludes NUL).
#[no_mangle]
pub extern "C" fn debashc_str_len(ptr: *const u8) -> u32 {
    if ptr.is_null() {
        return 0;
    }
    unsafe { u32::from_le(*(ptr.sub(4) as *const u32)) }
}

/// `debashc_free(ptr)` — release a buffer returned by a `debashc_*` export.
#[no_mangle]
pub extern "C" fn debashc_free(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        let n = u32::from_le(*(ptr.sub(4) as *const u32)) as usize;
        let total = 4 + n + 1;
        let layout = Layout::from_size_align(total, 4).expect("valid layout");
        dealloc(ptr.sub(4), layout);
    }
}
