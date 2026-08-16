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

use std::alloc::{alloc, dealloc, Layout};
use std::ptr::null_mut;
use std::slice;

use crate::estree::ast_to_estree_json;
use crate::glsl_backend::{shir_to_glsl_opts, ShGlslOptions};
use crate::shir::ast_to_ir_raw;
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

/// `debashc_to_glsl(input, input_len) -> *mut u8` — shell → **GLSL ES 1.00
/// render fragment** (the MIMEcroft shader pipeline): the bash program
/// becomes a fragment shader with the frag_x/frag_y/vcolor_*/uv_*/tex_*
/// bridges (see glsl_backend) — so the browser can compile bash-authored
/// shaders through the otranspiler wasm, no native binary needed.
#[no_mangle]
pub extern "C" fn debashc_to_glsl(input: *const u8, input_len: usize) -> *mut u8 {
    let input = unsafe { slice::from_raw_parts(input, input_len) };
    let input = String::from_utf8_lossy(input);
    match Parser::new(&input).parse() {
        Ok(commands) => {
            let prog = ast_to_ir_raw(&commands);
            let glsl = shir_to_glsl_opts(
                &prog,
                &ShGlslOptions {
                    es100: true,
                    color_out: true,
                    tex_size: 16,
                    // max_view stays the Default (0): the coordinate
                    // range is EMBEDDER-owned (core request
                    // estree-20260813-232001-glsl-options-build-fix) —
                    // the browser goes through the otranspilerl crate's
                    // view-parameterized entry points; this legacy frag
                    // entry must not bake in a canvas size.
                    ..Default::default()
                },
            );
            alloc_string(&ok_json(&glsl))
        }
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

// ── otranspilerl_* — the unified ABI (all nine backends) ────────
//
// Same memory contract as debashc_*: every string export returns a
// pointer to the data area of a [u32 len][data][0] buffer;
// otranspilerl_str_len reads the len prefix, otranspilerl_free
// releases it. otranspilerl_alloc(len) returns the data pointer of a
// buffer with the same prefix layout.
//
// Pipeline: sh source → A1 shIR (the debashl core) → any of the nine
// renderers (c, go, java, js/estree, perl, python, rust, sh, zig).
// Non-sh SOURCE languages need the frontend process (not in wasm yet).

/// `otranspilerl_alloc(len)` — a [u32 len][data][0] buffer, returns the
/// data pointer (the caller writes `len` bytes at it).
#[no_mangle]
pub extern "C" fn otranspilerl_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return null_mut();
    }
    let total = 4 + len + 1;
    let layout = Layout::from_size_align(total, 4).expect("valid layout");
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        return null_mut();
    }
    unsafe {
        *(ptr as *mut u32) = (len as u32).to_le();
        ptr.add(4)
    }
}

/// `otranspilerl_str_len(ptr)` — payload length in bytes (excludes NUL).
#[no_mangle]
pub extern "C" fn otranspilerl_str_len(ptr: *const u8) -> u32 {
    debashc_str_len(ptr)
}

/// `otranspilerl_free(ptr)` — release a buffer (result or alloc'd input).
#[no_mangle]
pub extern "C" fn otranspilerl_free(ptr: *mut u8) {
    debashc_free(ptr);
}

/// `otranspilerl_version() -> *mut u8` — "otranspilerl <version>" (JSON
/// envelope).
#[no_mangle]
pub extern "C" fn otranspilerl_version() -> *mut u8 {
    alloc_string(&ok_json(&format!("otranspilerl {}", env!("CARGO_PKG_VERSION"))))
}

fn read_str(ptr: *const u8, len: usize) -> String {
    if ptr.is_null() || len == 0 {
        return String::new();
    }
    unsafe { String::from_utf8_lossy(slice::from_raw_parts(ptr, len)).into_owned() }
}

fn parse_to_ir(src: &str) -> Result<crate::ir::IrProgram, String> {
    let (commands, lines) = Parser::new(src).parse_with_lines().map_err(|e| e.to_string())?;
    Ok(crate::shir::ast_to_ir_with_lines(&commands, &lines))
}

fn render_ir(prog: &crate::ir::IrProgram, lang: &str) -> Result<String, String> {
    match lang {
        "c" => Ok(crate::c_backend::shir_to_c(prog)),
        "go" => Ok(crate::go_backend::shir_to_go(prog)),
        "java" => crate::java_backend::shir_to_java(prog),
        "js" => Ok(crate::js_backend::shir_to_js(prog)),
        "pl" | "perl" => Ok(crate::perl_backend::shir_to_perl(prog)),
        "py" | "python" => Ok(crate::python_backend::shir_to_python(prog)),
        "rs" | "rust" => Ok(crate::rust_backend::shir_to_rust(prog)),
        "sh" => crate::sh_backend::shir_to_sh(prog),
        "zig" => Ok(crate::zig_backend::shir_to_zig(prog)),
        other => Err(format!(
            "target \"{other}\" not wired (known: js, pl, c, go, py, sh, java, rs, zig, shir)"
        )),
    }
}

fn otranspilerl_transpile_impl(src: &str, src_lang: &str, tgt_lang: &str) -> Result<String, String> {
    if src_lang == "shir" {
        let mut prog = crate::shir_json_in::shir_json_to_ir(src)?;
        crate::shir_passes::strip_cfor(&mut prog);
        // the A1 optimizer family (estree-20260813-183713/182434/182435)
        crate::shir_passes::optimize::optimize(&mut prog);
        return render_ir(&prog, tgt_lang);
    }
    if src_lang == "sh" {
        let prog = parse_to_ir(src)?;
        return render_ir(&prog, tgt_lang);
    }
    Err(format!(
        "source language \"{src_lang}\" requires the frontend process spawn, not available in this build; feed the frontend's A1 JSON to render instead"
    ))
}

/// `otranspilerl_shir(src, len)` — shell source → A1 shIR JSON.
#[no_mangle]
pub extern "C" fn otranspilerl_shir(src: *const u8, src_len: usize) -> *mut u8 {
    let src = read_str(src, src_len);
    match parse_to_ir(&src).and_then(|prog| Ok(crate::shir_json::shir_to_shir_json(&prog))) {
        Ok(out) => alloc_string(&ok_json(&out)),
        Err(e) => alloc_string(&err_json(&e)),
    }
}

/// `otranspilerl_render(a1, len, lang, lang_len)` — A1 shIR JSON → target.
#[no_mangle]
pub extern "C" fn otranspilerl_render(a1: *const u8, a1_len: usize, lang: *const u8, lang_len: usize) -> *mut u8 {
    let a1 = read_str(a1, a1_len);
    let lang = read_str(lang, lang_len);
    let res = crate::shir_json_in::shir_json_to_ir(&a1)
        .and_then(|mut prog| {
            crate::shir_passes::strip_cfor(&mut prog);
            // the A1 optimizer family (estree-20260813-183713/182434/182435)
            crate::shir_passes::optimize::optimize(&mut prog);
            render_ir(&prog, &lang)
        });
    match res {
        Ok(out) => alloc_string(&ok_json(&out)),
        Err(e) => alloc_string(&err_json(&e)),
    }
}

/// `otranspilerl_transpile(src, len, srcLang, len, tgtLang, len)` —
/// source → target (sh and shir sources, in-process).
#[no_mangle]
pub extern "C" fn otranspilerl_transpile(
    src: *const u8,
    src_len: usize,
    src_lang: *const u8,
    src_lang_len: usize,
    tgt_lang: *const u8,
    tgt_lang_len: usize,
) -> *mut u8 {
    let src = read_str(src, src_len);
    let src_lang = read_str(src_lang, src_lang_len);
    let tgt_lang = read_str(tgt_lang, tgt_lang_len);
    match otranspilerl_transpile_impl(&src, &src_lang, &tgt_lang) {
        Ok(out) => alloc_string(&ok_json(&out)),
        Err(e) => alloc_string(&err_json(&e)),
    }
}
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
