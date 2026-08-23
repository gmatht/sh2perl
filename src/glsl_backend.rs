//! GLSL backend renderer — SKETCH (merged into the core like the other
//! backends; the canonical home is the `backends/glsl` worktree, branch
//! `backend/glsl`).
//!
//! Consumes the ShIR in-process and emits a **GLSL ES 3.00 (WebGL 2)
//! fragment shader** that computes the shell program's stdout into a
//! global byte buffer and encodes it as the fragment color. The sketch
//! target is the pure-computation subset of bash (assignment, integer
//! arithmetic, echo/printf, if/while/for, case, user functions); anything
//! that needs a process, file, or external binary is fundamentally
//! unrepresentable on a GPU and renders as a `/* TODO(unsupported) */`
//! marker (the C backend's refuse-over-guess idiom).
//!
//! ## Why a fragment shader, not a vertex/compute shader
//! A fragment shader is the only universally-available GLSL stage that
//! can write results readable back to the CPU (`readPixels`), and its
//! `main()` runs once per fragment — a 1×N canvas gives N byte slots.
//!
//! ## String model
//! GLSL has no string/char type. A string is an `ivec2 (offset, len)`
//! into the global `const int s_tab[]` string table (ASCII codes).
//! Runtime concatenation / number-to-string materialize into the
//! `s_scratch` region (`cat` / `itos` always return a FRESH scratch
//! string, so expression results are stable until the next materialize).
//!
//! ## Output encoding
//! `u_mode == 0` (default): `outColor = (len/255, byte0, byte1, byte2)`.
//! `u_mode == 1`: one byte per fragment across `gl_FragCoord.x` — render
//! a 1 × OUT_CAP canvas and read the red channel of each pixel.
//!
//! ## Known limitations (documented, not hidden)
//! - bash arithmetic is i64-wrapping; GLSL ES 3.00 `int` is i32 — large
//!   values wrap differently (the `choose_width` table in shir.rs is the
//!   planned bridge; a future pass could pack i64 in two ints).
//! - `getVar("N")` positionals at TOP LEVEL are empty (no argv on the
//!   GPU); inside functions they map to the `g_pa[]` param array.
//! - functions are `void` (shell status codes dropped), recursion is
//!   illegal in GLSL (bash can recurse).
//! - `printf` is supported only for literal `%s`/`%d`/`%i`/`%%` formats.

use crate::ir::{
    ArithAst, BinOpKind, Decl, IrCaseClause, IrExpr, IrProgram, IrRedirect, IrStmt, IrSub,
    IrType, InterpPart,
};
use std::collections::{BTreeMap, BTreeSet};

const OUT_CAP: usize = 4096;    // stdout byte buffer
const SCRATCH_CAP: usize = 4096; // runtime string materialization
const PARAM_CAP: usize = 64;    // function param array
const ARR_CAP: usize = 1024;    // array stores
const FIT_CAP: usize = 1024;    // for-iter element materialization

#[derive(Clone, Copy, PartialEq)]
enum Ty {
    Num,
    Str,
    Bool,
}

/// Where emit() writes: body text is buffered until the string table is
/// complete (render-time literals register new entries).
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Header,
    Body,
}

pub struct Render {
    out: Vec<String>,
    body: Vec<String>, // body text (phase Body) — spliced after the table
    phase: Phase,
    depth: usize,
    types: BTreeMap<String, IrType>,
    vars: BTreeSet<String>,   // scalar vars (int or ivec2 by A2 verdict)
    arith_vars: BTreeSet<String>, // vars used in arithmetic → forced Int
    arrays: BTreeSet<String>, // indexed arrays
    arith_assigned: BTreeSet<String>, // vars assigned from $((...)) → Num in GLSL
    float_vars: BTreeSet<String>, // vars assigned from a float bc capture → GLSL float
    used_str: bool,   // the program uses strings/scratch (the ES-3.00-only runtime)
    used_putb: bool,  // the program emits output bytes (putb)
    used_pa: bool,    // function params / positionals in fn (the g_pa array)
    used_fit: bool,   // string for-loops over array literals (the g_fit array)
    used_ipow: bool,  // arithmetic ** (pow)
    used_isqrt: bool, // arithmetic sqrt
    putb_pos: usize,  // ES 1.00: the fixed out_buf slot for the next putb
    fns: BTreeSet<String>,    // user function names
    fn_bodies: BTreeMap<String, Vec<IrStmt>>, // Function stmt bodies (hoisted)
    fn_order: Vec<String>,    // first-seen order (deterministic emission)
    str_tab: Vec<i32>,        // string table (ASCII codes)
    str_offsets: BTreeMap<String, (u32, u32)>, // literal -> (off, len)
    in_fn: bool,              // rendering inside a Function body
    todo: usize,
    opts: ShGlslOptions,
    // the input bridges
    // texture-fetch load sinking: groups whose fetch + per-channel seeds
    // move into the single block that dominates every read of their
    // bridge vars (computed in shir_to_glsl_opts before the body
    // renders) — the untouched path costs zero fetches.
    lazy_tex_sinks: Vec<LazyTexSink>,
    // true when the PROGRAM reads the uv_x/uv_y bridges directly (the
    // texture samples wrap via fract(vUv) and don't need the texel-grid
    // seeds; only a genuine uv read does)
    reads_uv: bool,
}

// ── texture-fetch load sinking ──────────────────────────────────
// A texture bridge group (tex → uTex, crack → uCrack) is fetched at
// main() start whenever ANY of its channel vars is referenced. When
// every reference of the group's channels lives inside ONE block (an
// if/else arm or a bare block), the fetch + per-channel extraction is
// instead emitted at the top of that block, so the other path costs
// zero fetches. MIMEcroft's crack overlay reads cr_r/g/b/a only inside
// `if damage > 0` — undamaged fragments never sample uCrack.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TexGroup { Tex, Crack }

#[derive(Clone, Copy)]
struct LazyTexSink {
    group: TexGroup,
    block: *const [IrStmt],
}

fn tex_group_channels(g: TexGroup) -> &'static [&'static str] {
    match g {
        TexGroup::Tex => &["tex_r", "tex_g", "tex_b"],
        TexGroup::Crack => &["cr_r", "cr_g", "cr_b", "cr_a"],
    }
}

fn record_tex_channel(n: &str, blk: *const [IrStmt], out: &mut Vec<(TexGroup, *const [IrStmt])>) {
    if n == "tex_r" || n == "tex_g" || n == "tex_b" {
        out.push((TexGroup::Tex, blk));
    } else if n == "cr_r" || n == "cr_g" || n == "cr_b" || n == "cr_a" {
        out.push((TexGroup::Crack, blk));
    }
}

// Record every tex/crack channel READ with the innermost block that
// contains it. A block is a Vec<IrStmt> slice; the top-level statement
// list is the root — never a sink target. Missing a nested body only
// attributes its reads to the enclosing block, which still dominates
// them, so the fetch is never placed somewhere that fails to dominate
// a read (the walk below covers every block-carrying variant the
// backend renders).
fn collect_tex_reads(
    stmts: &[IrStmt],
    blk: *const [IrStmt],
    out: &mut Vec<(TexGroup, *const [IrStmt])>,
) {
    for s in stmts {
        tex_reads_in_stmt(s, blk, out);
        match s {
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_tex_reads(then, then.as_slice() as *const [IrStmt], out);
                for (_, b) in elsifs {
                    collect_tex_reads(b, b.as_slice() as *const [IrStmt], out);
                }
                collect_tex_reads(else_, else_.as_slice() as *const [IrStmt], out);
            }
            IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => {
                collect_tex_reads(body, body.as_slice() as *const [IrStmt], out);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                collect_tex_reads(body, body.as_slice() as *const [IrStmt], out);
            }
            IrStmt::For { body, .. } => {
                collect_tex_reads(body, body.as_slice() as *const [IrStmt], out);
            }
            IrStmt::ForInit { body, .. } => {
                collect_tex_reads(body, body.as_slice() as *const [IrStmt], out);
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    collect_tex_reads(&c.body, c.body.as_slice() as *const [IrStmt], out);
                }
            }
            IrStmt::Function { body, .. } => {
                collect_tex_reads(body, body.as_slice() as *const [IrStmt], out);
            }
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_tex_reads(st, st.as_slice() as *const [IrStmt], out);
                }
            }
            _ => {}
        }
    }
}

// Walk a statement's SCALAR expressions (conds, values, outputs) at the
// current block. Block-carrying fields are walked separately by
// collect_tex_reads with their own block pointer — double-walking them
// here would misattribute the reads to the enclosing block.
fn tex_reads_in_stmt(s: &IrStmt, blk: *const [IrStmt], out: &mut Vec<(TexGroup, *const [IrStmt])>) {
    match s {
        IrStmt::Output { value, .. } => tex_reads_in_expr(value, blk, out),
        IrStmt::WriteFile { path, content, .. } => {
            tex_reads_in_expr(path, blk, out);
            tex_reads_in_expr(content, blk, out);
        }
        IrStmt::Assign { expr, .. } => tex_reads_in_expr(expr, blk, out),
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                tex_reads_in_expr(i, blk, out);
            }
        }
        IrStmt::DeclareArray { elements, .. } => {
            for e in elements {
                tex_reads_in_expr(e, blk, out);
            }
        }
        IrStmt::If { cond, .. } => tex_reads_in_expr(cond, blk, out),
        IrStmt::While { cond, .. } | IrStmt::DoWhile { cond, .. } => tex_reads_in_expr(cond, blk, out),
        IrStmt::ForInit { cond, .. } => tex_reads_in_expr(cond, blk, out),
        IrStmt::For { iter, .. } => tex_reads_in_expr(iter, blk, out),
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => tex_reads_in_expr(expr, blk, out),
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            tex_reads_in_expr(cmd, blk, out);
            for a in args {
                tex_reads_in_expr(a, blk, out);
            }
            for r in redirects {
                tex_reads_in_expr(r, blk, out);
            }
            for (_, v) in env {
                tex_reads_in_expr(v, blk, out);
            }
        }
        IrStmt::Case { discriminant, .. } => tex_reads_in_expr(discriminant, blk, out),
        IrStmt::Redirect { redirects, .. } => {
            for r in redirects {
                tex_reads_in_expr(&r.target, blk, out);
            }
        }
        IrStmt::Return(Some(e)) => tex_reads_in_expr(e, blk, out),
        IrStmt::Exit(Some(e)) => tex_reads_in_expr(e, blk, out),
        IrStmt::SetChildError(e) => tex_reads_in_expr(e, blk, out),
        IrStmt::Expr(e) => tex_reads_in_expr(e, blk, out),
        _ => {}
    }
}

fn tex_reads_in_expr(e: &IrExpr, blk: *const [IrStmt], out: &mut Vec<(TexGroup, *const [IrStmt])>) {
    match e {
        IrExpr::Var(n, _) => record_tex_channel(n, blk, out),
        IrExpr::Call { func, args } => {
            if func == "getVar" {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    record_tex_channel(n, blk, out);
                }
            }
            for a in args {
                tex_reads_in_expr(a, blk, out);
            }
        }
        IrExpr::Index { key, .. } => tex_reads_in_expr(key, blk, out),
        IrExpr::BinOp { lhs, rhs, .. } => {
            tex_reads_in_expr(lhs, blk, out);
            tex_reads_in_expr(rhs, blk, out);
        }
        IrExpr::MethodCall { obj, args, .. } => {
            tex_reads_in_expr(obj, blk, out);
            for a in args {
                tex_reads_in_expr(a, blk, out);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            tex_reads_in_expr(cond, blk, out);
            tex_reads_in_expr(then, blk, out);
            tex_reads_in_expr(else_, blk, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            tex_reads_in_expr(expr, blk, out);
            tex_reads_in_expr(default, blk, out);
        }
        IrExpr::Capture { expr, .. } => tex_reads_in_expr(expr, blk, out),
        IrExpr::Arrow(body) => collect_tex_reads(body, body.as_slice() as *const [IrStmt], out),
        IrExpr::Array(items) => {
            for i in items {
                tex_reads_in_expr(i, blk, out);
            }
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            tex_reads_in_expr(iter, blk, out);
            tex_reads_in_expr(elem, blk, out);
            if let Some(c) = cond {
                tex_reads_in_expr(c, blk, out);
            }
        }
        IrExpr::Arith(a) => tex_reads_in_arith(a, blk, out),
        _ => {}
    }
}

fn tex_reads_in_arith(
    a: &ArithAst,
    blk: *const [IrStmt],
    out: &mut Vec<(TexGroup, *const [IrStmt])>,
) {
    match a {
        ArithAst::Var(n) | ArithAst::Ident(n) => record_tex_channel(n, blk, out),
        ArithAst::Index { var, key } => {
            record_tex_channel(var, blk, out);
            tex_reads_in_arith(key, blk, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            tex_reads_in_arith(lhs, blk, out);
            tex_reads_in_arith(rhs, blk, out);
        }
        ArithAst::Un { arg, .. } => tex_reads_in_arith(arg, blk, out),
        ArithAst::Cond { test, then, else_ } => {
            tex_reads_in_arith(test, blk, out);
            tex_reads_in_arith(then, blk, out);
            tex_reads_in_arith(else_, blk, out);
        }
        ArithAst::Assign { rhs, .. } => tex_reads_in_arith(rhs, blk, out),
        ArithAst::IncDec { var, .. } => record_tex_channel(var, blk, out),
        ArithAst::Cast { arg, .. } => tex_reads_in_arith(arg, blk, out),
        ArithAst::Num(_) | ArithAst::Sizeof(_) => {}
    }
}

// The groups whose reads all live in one non-top-level block → sink the
// fetch there. Top-level-only reads (e.g. tex_r used unconditionally)
// keep the current main()-start seeding.
fn compute_lazy_tex_sinks(prog: &IrProgram) -> Vec<LazyTexSink> {
    let top = prog.stmts.as_slice() as *const [IrStmt];
    let mut reads: Vec<(TexGroup, *const [IrStmt])> = Vec::new();
    collect_tex_reads(&prog.stmts, top, &mut reads);
    let mut out = Vec::new();
    for g in [TexGroup::Crack, TexGroup::Tex] {
        let mut blocks: Vec<*const [IrStmt]> = Vec::new();
        let mut any = false;
        for (rg, b) in &reads {
            if *rg == g {
                any = true;
                if !blocks.iter().any(|x| std::ptr::eq(*x, *b)) {
                    blocks.push(*b);
                }
            }
        }
        if any && blocks.len() == 1 && !std::ptr::eq(blocks[0], top) {
            out.push(LazyTexSink { group: g, block: blocks[0] });
        }
    }
    out
}

/// Renderer options — the default (ES 3.00 stdout-computation) is the
/// original sketch; `es100`/`color_out` switch to a **render fragment**
/// that pairs with a hand-written ES 1.00 vertex shader (the MIMEcroft
/// game writes its fragment shader in bash and compiles it here);
/// `vert_out` switches to a **render vertex shader** — the MIMEcroft
/// game now authors BOTH stages in bash.
#[derive(Clone, Copy, Debug)]
pub struct ShGlslOptions {
    /// Emit GLSL ES 1.00 (WebGL1): no `#version 300 es`, fragment input
    /// via `varying`, output via `gl_FragColor`, ES 1.00 array syntax.
    pub es100: bool,
    /// Render mode: the bash program's `out_buf` bytes 0..3 are the
    /// fragment colour (`gl_FragColor = vec4(out_buf[0..3]) / 255`), and
    /// two input bridges are seeded at main() start when referenced:
    ///   frag_x/frag_y       ← int(gl_FragCoord.xy)
    ///   vcolor_r/g/b        ← int(vColor.rgb * 255.0)  (varying input)
    /// `putb N` emits a single byte into out_buf.
    pub color_out: bool,
    /// Vertex mode: emit a **vertex shader** instead of a fragment
    /// shader. Input bridges come from the vertex attributes and the
    /// camera/object uniforms (all ×1000 so bash stays integer); the
    /// program sets float `vp_x/y/z/w` (gl_Position) and int
    /// `vc_r/g/b/a` + `vu_u/v` (×1000 — the vColor/vUv varyings), which
    /// the backend writes out at the end of main(). No `putb`/byte
    /// output in this mode — there is no fragment colour.
    pub vert_out: bool,
    /// When > 0, seed the texture bridges (uv_x/uv_y = the texel index
    /// from the `vUv` varying at this size; tex_r/g/b = the sampled
    /// colour at that texel) and declare `uniform sampler2D uTex;` +
    /// `varying vec2 vUv;` — the bash program can then read the block
    /// texture per pixel, all in integer arithmetic.
    pub tex_size: u32,
    /// The render target's larger canvas extent (width or height). Seeds
    /// the frag_x/frag_y input bridges (0..max_view) for the mediump
    /// interval proof — the shader reads gl_FragCoord, whose exact range
    /// only the caller knows (the sh2runtime device canvas is 800×600).
    /// 0 = unknown → `mediump int` is never emitted (the ES 1.00
    /// mandatory fragment precision is only safe when every integer
    /// intermediate provably fits ±2^15). mediump FLOAT additionally
    /// requires max_view ≤ 2048 (its 10-bit mantissa represents exact
    /// integers only to 2^11, so gl_FragCoord loses pixel accuracy past
    /// that).
    pub max_view: u32,
}

impl Default for ShGlslOptions {
    fn default() -> Self {
        Self { es100: false, color_out: false, vert_out: false, tex_size: 0, max_view: 0 }
    }
}

/// Render an `IrProgram` to a GLSL ES 3.00 fragment shader.
pub fn shir_to_glsl(prog: &IrProgram) -> String {
    shir_to_glsl_opts(prog, &ShGlslOptions::default())
}

/// Render with options (see [`ShGlslOptions`]).
pub fn shir_to_glsl_opts(prog: &IrProgram, opts: &ShGlslOptions) -> String {
    let mut prog = prog.clone();
    // builtin-op fallback arm (shir-builtin-op-20260816): the glsl
    // backend has NOT accepted the `builtin` op — render as exec.
    crate::transforms::builtin::fallback_builtin_to_exec(&mut prog);
    // A2: the raw ShIR carries no type verdicts; run the analysis so
    // int vars become native GLSL ints (like the C backend does).
    prog.var_types = crate::shir::analyze_var_types(&prog);
    let mut r = Render::default();
    r.opts = *opts;
    r.types = prog.var_types.iter().cloned().collect();
    // pass 1: collect vars / arrays / functions / string literals
    for s in &prog.stmts {
        r.collect_stmt(s);
    }
    for sub in &prog.subs {
        r.fns.insert(sub.name.clone());
        for s in &sub.body {
            r.collect_stmt(s);
        }
    }
    // texture-fetch load sinking: a group whose channel reads all live
    // in one branch/block is fetched there, not at main() start (only
    // the top-level program — function bodies render in their own
    // scope, where the main() uv/damage seeds are not visible).
    r.lazy_tex_sinks = compute_lazy_tex_sinks(&prog);
    // Input bridges are seeded/declared ONLY when the program references
    // them (see the color_out seeding below) — pass 1 collected every
    // reference into r.vars (direct Var reads, $(( )) arith, test
    // strings, getVar). Referenced bridges are native ints: the numeric
    // verdict comes from arith_vars (a bridge has no var_types entry —
    // the program never assigns it).
    if opts.color_out {
        let mut bridges: Vec<&str> = vec![
            "frag_x", "frag_y", "vcolor_r", "vcolor_g", "vcolor_b",
        ];
        if opts.tex_size > 0 {
            bridges.extend([
                "uv_x", "uv_y", "tex_r", "tex_g", "tex_b",
                "damage", "cr_r", "cr_g", "cr_b", "cr_a",
            ]);
        }
        for n in bridges {
            if r.vars.contains(n) {
                r.arith_vars.insert(n.to_string());
            }
        }
        // the texture sample coordinate reads the uv bridges even when
        // the program only references tex/cr — imply them so they get
        // declared (the seeds below write g_uv_x/g_uv_y).
        if opts.tex_size > 0
            && r.uses_any(&["uv_x", "uv_y", "tex_r", "tex_g", "tex_b",
                            "cr_r", "cr_g", "cr_b", "cr_a"])
        {
            for n in ["uv_x", "uv_y"] {
                r.vars.insert(n.to_string());
                r.arith_vars.insert(n.to_string());
            }
        }
    }
    if opts.vert_out {
        // vertex mode: the input bridges (attributes + uniforms, all
        // ×1000 so bash stays integer) are native ints — same rule as
        // the fragment bridges: force any referenced one to Int.
        let bridges: Vec<&str> = vec![
            "ap_x", "ap_y", "ap_z",          // aPosition  ×1000 (±500)
            "ash_r", "ash_g", "ash_b",        // aShade     ×1000 (450..1000)
            "auv_u", "auv_v",                  // aUv        ×1000 (0..1000)
            "ucp_x", "ucp_y", "ucp_z",        // uCamPos    ×1000 (world)
            "ucy_m",                           // uCamYaw    ×1000 (milli-degrees)
            "ucs",                             // uCamShift  ×1000 (milli-NDC strafe)
            "uop_x", "uop_y", "uop_z",        // uObjPos    ×1000 (cell centre)
            "usc_x", "usc_y", "usc_z",        // uScale     ×1000 (1 → 1000)
            "ublk_r", "ublk_g", "ublk_b",     // uBlockColor ×1000 (0..1000)
            "uov",                             // uOverlay   ×1000 (0 or 1000)
        ];
        for n in bridges {
            if r.vars.contains(n) {
                r.arith_vars.insert(n.to_string());
            }
        }
        // the output vars the emission reads back — force them into
        // r.vars so they are ALWAYS declared (the program may set only
        // some of them; the final gl_Position/vColor/vUv lines read all).
        // vp_* become floats via the bc captures; vc_*/vu_* stay ints
        // (they are only ever assigned int expressions), so force them
        // into arith_vars too — the A2 verdict alone would leave a
        // plain `vu_u=$auv_u` string-typed.
        for n in [
            "vp_x", "vp_y", "vp_z", "vp_w",
            "vc_r", "vc_g", "vc_b", "vc_a",
            "vu_u", "vu_v",
        ] {
            r.vars.insert(n.to_string());
        }
        for n in ["vc_r", "vc_g", "vc_b", "vc_a", "vu_u", "vu_v"] {
            r.arith_vars.insert(n.to_string());
        }
    }
    // pass 2: RENDER THE BODY FIRST — the string table must be COMPLETE
    // before it is emitted (render-time literals — bc folds, test
    // operands — register new entries). The body text is buffered in
    // r.body; the header/table/helpers are assembled afterwards.
    r.phase = Phase::Body;
    for sub in &prog.subs {
        r.fn_bodies.insert(sub.name.clone(), sub.body.clone());
        if !r.fn_order.contains(&sub.name) {
            r.fn_order.push(sub.name.clone());
        }
    }
    r.emit_fn_defs();
    if r.opts.vert_out {
        // render vertex: the attribute/uniform inputs (each declared
        // ONLY when the program references its bridge — the device
        // binds attributes by name and skips inactive ones, and a
        // WebGL uniform write to an undeclared uniform is a no-op, so
        // the game's unconditional uCamPos/uObjPos/… writes stay safe)
        // and the two varyings the fragment shader may consume.
        if r.uses_any(&["ap_x", "ap_y", "ap_z"]) {
            r.emit("attribute vec3 aPosition;");
        }
        if r.uses_any(&["ash_r", "ash_g", "ash_b"]) {
            r.emit("attribute vec3 aShade;");
        }
        if r.uses_any(&["auv_u", "auv_v"]) {
            r.emit("attribute vec2 aUv;");
        }
        if r.uses_any(&["ucp_x", "ucp_y", "ucp_z"]) {
            r.emit("uniform vec3 uCamPos;");
        }
        if r.vars.contains("ucy_m") {
            r.emit("uniform float uCamYaw;");
        }
        if r.vars.contains("ucs") {
            r.emit("uniform float uCamShift;");
        }
        if r.uses_any(&["uop_x", "uop_y", "uop_z"]) {
            r.emit("uniform vec3 uObjPos;");
        }
        if r.uses_any(&["usc_x", "usc_y", "usc_z"]) {
            r.emit("uniform vec3 uScale;");
        }
        if r.uses_any(&["ublk_r", "ublk_g", "ublk_b"]) {
            r.emit("uniform vec3 uBlockColor;");
        }
        if r.vars.contains("uov") {
            r.emit("uniform float uOverlay;");
        }
        // the varyings — always written at the end of main() (the
        // fragment shader declares the ones it consumes; a vertex-only
        // varying is legal ES 1.00 and links fine).
        r.emit("varying highp vec4 vColor;");
        r.emit("varying highp vec2 vUv;");
    } else if !r.opts.color_out {
        if r.opts.es100 {
            // ES 1.00 has no `out` — outColor is a local, written to
            // gl_FragColor at the end of main().
            r.emit("vec4 outColor;");
        } else {
            r.emit("out vec4 outColor;");
        }
        r.emit("uniform int u_mode;");
    } else {
        // render fragment: the varying arrives from the vertex shader.
        // Declare each input ONLY when the program references it (a
        // WebGL uniform write to an undeclared uniform is a no-op, and
        // an unconsumed vertex varying links fine — the game's
        // unconditional uTex/uCrack/uDamage writes stay safe, and a
        // texture-less fragment carries none of the machinery).
        let vcolor = r.uses_any(&["vcolor_r", "vcolor_g", "vcolor_b"]);
        let uv = r.uses_any(&["uv_x", "uv_y", "tex_r", "tex_g", "tex_b",
                              "cr_r", "cr_g", "cr_b", "cr_a"]);
        let tex = r.uses_any(&["tex_r", "tex_g", "tex_b"]);
        let crack = r.uses_any(&["cr_r", "cr_g", "cr_b", "cr_a"]);
        if vcolor {
            r.emit(if r.opts.es100 { "varying highp vec4 vColor;" } else { "in highp vec4 vColor;" });
        }
        if uv {
            // vUv carries WORLD coordinates for the camera-following
            // background planes (usc_x > 1100 → the vertex shader
            // outputs p.xz, up to ±35 world units) — a mediump read
            // (fp16 on Vulkan/Metal ANGLE backends) loses the
            // fractional part and jitters the texel-grid selection by
            // ±1 texel, so the floor texture flaked. highp read, like
            // the hand-written fallback (fs_fb) always had.
            r.emit(if r.opts.es100 { "varying highp vec2 vUv;" } else { "in highp vec2 vUv;" });
        }
        if tex {
            r.emit("uniform sampler2D uTex;");
        }
        if crack {
            r.emit("uniform sampler2D uCrack;");
        }
        if r.vars.contains("damage") {
            r.emit("uniform int uDamage;");
        }
    }
    r.emit("");
    r.emit("void main() {");
    r.depth += 1;
    // scalar program vars are main() locals when there are no user
    // functions (see emit_globals) — declare them before the bridge
    // seeding so every assignment below is a plain local write.
    if r.fns.is_empty() {
        let locals: Vec<String> = r.vars.iter().map(|n| r.ident(n)).collect();
        for n in locals {
            // float_vars stores the UNMANGLED var name
            let name = n.strip_prefix("g_").unwrap_or(&n);
            if r.float_vars.contains(name) {
                r.emit(&format!("float {n};"));
            } else if r.is_num_ident(&n) {
                r.emit(&format!("int {n};"));
            } else {
                r.emit(&format!("ivec2 {n};"));
            }
        }
        if !r.vars.is_empty() {
            r.emit("");
        }
    }
    if r.opts.color_out {
        // input bridges — bash reads these as ints; seed only the ones
        // the program references (pass 1 collected them into r.vars).
        if r.vars.contains("frag_x") {
            r.emit("g_frag_x = int(gl_FragCoord.x);");
        }
        if r.vars.contains("frag_y") {
            r.emit("g_frag_y = int(gl_FragCoord.y);");
        }
        if r.vars.contains("vcolor_r") {
            r.emit("g_vcolor_r = int(vColor.r * 127.0);");
        }
        if r.vars.contains("vcolor_g") {
            r.emit("g_vcolor_g = int(vColor.g * 127.0);");
        }
        if r.vars.contains("vcolor_b") {
            r.emit("g_vcolor_b = int(vColor.b * 127.0);");
        }
        if opts.tex_size > 0 {
            let uv_needed = r.uses_any(&[
                "uv_x", "uv_y", "tex_r", "tex_g", "tex_b",
                "cr_r", "cr_g", "cr_b", "cr_a",
            ]);
            if uv_needed {
                // uv_x/uv_y: the texel index (0..tex_size) from the
                // varying — a program that READS the uv bridges directly
                // needs these seeds; the texture samples themselves use
                // fract(vUv) (precision-safe wrapping) and don't.
                if r.reads_uv {
                    let f = |v: u32| format!("{v}.0");
                    let sz = f(opts.tex_size);
                    r.emit(&format!("g_uv_x = int(vUv.x * {sz});"));
                    r.emit(&format!("g_uv_y = int(vUv.y * {sz});"));
                }
                // Sample each texture ONCE into a vec4 local, then
                // swizzle — the three tex (resp. four crack) seeds use
                // the same coordinates, and drivers may not CSE
                // identical texture2D calls, so this is 2 fetches per
                // fragment instead of 7. A group whose channel reads
                // all live in one block (compute_lazy_tex_sinks) is
                // fetched at the top of that block instead — the
                // untouched path costs zero fetches.
                if !r.is_tex_sunk(TexGroup::Tex) {
                    r.emit_tex_seeds(TexGroup::Tex);
                }
                // the crack overlay: uDamage (0..3) + the crack texel
                if r.vars.contains("damage") {
                    r.emit("g_damage = uDamage;");
                }
                if !r.is_tex_sunk(TexGroup::Crack) {
                    r.emit_tex_seeds(TexGroup::Crack);
                }
            }
        }
    }
    if r.opts.vert_out {
        // vertex input bridges — attributes/uniforms ×1000, seeded only
        // when the program references them (pass 1 collected them).
        // uCamYaw arrives in DEGREES with milli precision (0..360000
        // milli-degrees — the game's fmt_pos string); the other
        // position/colour values are world units ×1000.
        if r.vars.contains("ap_x") {
            r.emit("g_ap_x = int(aPosition.x * 1000.0);");
        }
        if r.vars.contains("ap_y") {
            r.emit("g_ap_y = int(aPosition.y * 1000.0);");
        }
        if r.vars.contains("ap_z") {
            r.emit("g_ap_z = int(aPosition.z * 1000.0);");
        }
        if r.vars.contains("ash_r") {
            r.emit("g_ash_r = int(aShade.r * 1000.0);");
        }
        if r.vars.contains("ash_g") {
            r.emit("g_ash_g = int(aShade.g * 1000.0);");
        }
        if r.vars.contains("ash_b") {
            r.emit("g_ash_b = int(aShade.b * 1000.0);");
        }
        if r.vars.contains("auv_u") {
            r.emit("g_auv_u = int(aUv.x * 1000.0);");
        }
        if r.vars.contains("auv_v") {
            r.emit("g_auv_v = int(aUv.y * 1000.0);");
        }
        if r.vars.contains("ucp_x") {
            r.emit("g_ucp_x = int(uCamPos.x * 1000.0);");
        }
        if r.vars.contains("ucp_y") {
            r.emit("g_ucp_y = int(uCamPos.y * 1000.0);");
        }
        if r.vars.contains("ucp_z") {
            r.emit("g_ucp_z = int(uCamPos.z * 1000.0);");
        }
        if r.vars.contains("ucy_m") {
            r.emit("g_ucy_m = int(uCamYaw * 1000.0);");
        }
        if r.vars.contains("ucs") {
            r.emit("g_ucs = int(uCamShift * 1000.0);");
        }
        if r.vars.contains("uop_x") {
            r.emit("g_uop_x = int(uObjPos.x * 1000.0);");
        }
        if r.vars.contains("uop_y") {
            r.emit("g_uop_y = int(uObjPos.y * 1000.0);");
        }
        if r.vars.contains("uop_z") {
            r.emit("g_uop_z = int(uObjPos.z * 1000.0);");
        }
        if r.vars.contains("usc_x") {
            r.emit("g_usc_x = int(uScale.x * 1000.0);");
        }
        if r.vars.contains("usc_y") {
            r.emit("g_usc_y = int(uScale.y * 1000.0);");
        }
        if r.vars.contains("usc_z") {
            r.emit("g_usc_z = int(uScale.z * 1000.0);");
        }
        if r.vars.contains("ublk_r") {
            r.emit("g_ublk_r = int(uBlockColor.r * 1000.0);");
        }
        if r.vars.contains("ublk_g") {
            r.emit("g_ublk_g = int(uBlockColor.g * 1000.0);");
        }
        if r.vars.contains("ublk_b") {
            r.emit("g_ublk_b = int(uBlockColor.b * 1000.0);");
        }
        if r.vars.contains("uov") {
            r.emit("g_uov = int(uOverlay * 1000.0);");
        }
    }
    for s in &prog.stmts {
        r.stmt(s);
    }
    if r.opts.vert_out {
        // the bash program's float vp_* (gl_Position, world units) and
        // int vc_*/vu_* (×1000 — the vColor/vUv varyings)
        r.emit("gl_Position = vec4(g_vp_x, g_vp_y, g_vp_z, g_vp_w);");
        r.emit("vColor = vec4(float(g_vc_r) / 1000.0, float(g_vc_g) / 1000.0, float(g_vc_b) / 1000.0, float(g_vc_a) / 1000.0);");
        r.emit("vUv = vec2(float(g_vu_u) / 1000.0, float(g_vu_v) / 1000.0);");
    } else if r.opts.color_out {
        // the bash program's out_buf bytes 0..3 are the fragment colour
        r.emit("gl_FragColor = vec4(float(out_buf[0]) / 255.0, float(out_buf[1]) / 255.0, float(out_buf[2]) / 255.0, float(out_buf[3]) / 255.0);");
    } else {
        r.emit("if (u_mode == 1) {");
        r.depth += 1;
        r.emit("int i = int(gl_FragCoord.x);");
        r.emit("outColor = vec4(0.0);");
        r.emit("if (i >= 0 && i < OUT_CAP && i < out_len) { outColor.r = float(out_buf[i]) / 255.0; }");
        r.depth -= 1;
        r.emit("} else {");
        r.depth += 1;
        r.emit(
            "outColor = vec4(float(out_len) / 255.0, float(out_buf[0]) / 255.0, float(out_buf[1]) / 255.0, float(out_buf[2]) / 255.0);",
        );
        r.depth -= 1;
        r.emit("}");
        if r.opts.es100 {
            r.emit("gl_FragColor = outColor;");
        }
    }
    r.depth -= 1;
    r.emit("}");
    // pass 3: assemble — header, then the (now complete) table, then body
    r.phase = Phase::Header;
    let body = std::mem::take(&mut r.body);
    if !r.opts.es100 {
        r.emit("#version 300 es");
    } else {
        r.emit("// GLSL ES 1.00 (WebGL1) — generated by the sh→GLSL backend");
    }
    // mediump gate: emit the ES 1.00 MANDATORY fragment precision when a
    // proof shows it is safe. highp int / highp float are OPTIONAL in
    // ES 1.00 fragment shaders — the generated shader compiles on every
    // WebGL1 implementation only when it uses the mandatory mediump
    // forms. The interval proof (fits_mediump_int) must bound every
    // integer intermediate within ±2^15; mediump float additionally
    // needs the canvas within its exact-integer range (gl_FragCoord).
    let mediump_int = r.opts.es100
        && r.opts.color_out
        && r.opts.max_view > 0
        && !r.used_str
        && prog.subs.is_empty()
        && fits_mediump_int(&prog, &r.opts);
    // highp is REQUIRED in ES 1.00 vertex shaders (only fragment
    // shaders may drop to the mandatory mediump) — a render vertex
    // always keeps highp float/int for the position math.
    let mediump_float = r.opts.es100
        && !r.opts.vert_out
        && r.opts.max_view > 0
        && r.opts.max_view <= 2048;
    r.emit(if mediump_float {
        "precision mediump float;"
    } else {
        "precision highp float;"
    });
    r.emit(if mediump_int {
        "precision mediump int;"
    } else {
        "precision highp int;"
    });
    if r.used_str || (!r.opts.color_out && !r.opts.vert_out)
        || (r.used_putb && !r.opts.es100)
    {
        // OUT_CAP is referenced by the putCh guard (string runtime / ES
        // 3.00 putb) and the ES 3.00 u_mode readback; a pure ES 1.00
        // render fragment (putb at fixed slots) never needs it, and a
        // render vertex writes gl_Position — no byte buffer at all.
        r.emit(&format!("const int OUT_CAP = {OUT_CAP};"));
    }
    r.emit("");
    r.emit_table();
    r.emit_globals();
    r.emit_helpers();
    r.emit_fn_prototypes();
    r.out.extend(body);
    // post-pass: strip parens that wrap a bare atom (identifier or
    // integer literal) — `(6)` → `6`, `(g_r)` → `g_r`. Atoms have no
    // operator precedence to disturb, so this is semantics-neutral; it
    // removes most of the defensive-paren noise from the arithmetic
    // pipelines. Comments are skipped.
    let out = format!(
        "{}\n// TODO(unsupported): {} construct(s) — see shir_to_glsl limitations\n",
        r.out.join("\n"),
        r.todo
    );
    strip_atom_parens(&out)
}

/// True when `rest` begins with an atom (GLSL identifier or integer
/// literal) directly followed by `)` — i.e. `rest` is `"atom)"`.
fn atom_paren(rest: &str) -> (bool, usize) {
    let b = rest.as_bytes();
    if b.is_empty() {
        return (false, 0);
    }
    let first = b[0] as char;
    let mut j = 0;
    if first.is_ascii_digit() {
        while j < b.len() && (b[j] as char).is_ascii_digit() {
            j += 1;
        }
    } else if first.is_ascii_alphabetic() || first == '_' {
        while j < b.len() {
            let c = b[j] as char;
            if c.is_ascii_alphanumeric() || c == '_' {
                j += 1;
            } else {
                break;
            }
        }
    } else {
        return (false, 0);
    }
    (j > 0 && j < b.len() && b[j] == b')', j)
}

/// `(atom)` → `atom` everywhere outside comments. Only parens whose
/// content is exactly one identifier or integer literal are removed
/// (`(out_buf[0])`, `(255.0)`, `texture2D(`, `(g_x / 6)` etc. are all
/// left untouched — they are not atoms). A `(` directly after an
/// identifier is a CALL/constructor paren (`itos(g_x)`) and is never
/// touched. Iterated to a fixpoint: `((1))` → `(1)` → `1`.
fn strip_atom_parens(s: &str) -> String {
    let mut cur = s.to_string();
    loop {
        let next = strip_atom_parens_once(&cur);
        if next == cur {
            return cur;
        }
        cur = next;
    }
}

fn strip_atom_parens_once(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        // `//` comment: copy to end of line untouched
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            let end = s[i..]
                .find('\n')
                .map(|p| i + p)
                .unwrap_or(s.len());
            out.push_str(&s[i..end]);
            i = end;
            continue;
        }
        // `/* */` comment: copy the whole block untouched
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            match s[i + 2..].find("*/") {
                Some(p) => {
                    out.push_str(&s[i..i + 2 + p + 2]);
                    i = i + 2 + p + 2;
                }
                None => {
                    out.push_str(&s[i..]);
                    break;
                }
            }
            continue;
        }
        if b[i] == b'(' {
            // `(` directly after an identifier char is a CALL/constructor
            // paren (`itos(g_x)`, `float(x)`), not a grouping paren —
            // stripping it would fuse the callee into the argument
            // (`itosg_x`). Only grouping parens (preceded by space/
            // operator/paren/etc.) may wrap a bare atom.
            let prev_is_ident = i > 0 && {
                let c = b[i - 1] as char;
                c.is_ascii_alphanumeric() || c == '_'
            };
            if !prev_is_ident {
                let (is_atom, len) = atom_paren(&s[i + 1..]);
                if is_atom {
                    // `(atom)` → `atom` (the closing `)` is consumed too)
                    out.push_str(&s[i + 1..i + 1 + len]);
                    i = i + 1 + len + 1;
                    continue;
                }
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

impl Default for Render {
    fn default() -> Self {
        Render {
            out: Vec::new(),
            body: Vec::new(),
            phase: Phase::Header,
            depth: 0,
            types: BTreeMap::new(),
            vars: BTreeSet::new(),
            arith_vars: BTreeSet::new(),
            arrays: BTreeSet::new(),
            arith_assigned: BTreeSet::new(),
            float_vars: BTreeSet::new(),
            used_str: false,
            used_putb: false,
            used_pa: false,
            used_fit: false,
            used_ipow: false,
            used_isqrt: false,
            putb_pos: 0,
            lazy_tex_sinks: Vec::new(),
            reads_uv: false,
            fns: BTreeSet::new(),
            fn_bodies: BTreeMap::new(),
            fn_order: Vec::new(),
            str_tab: Vec::new(),
            str_offsets: BTreeMap::new(),
            in_fn: false,
            todo: 0,
            opts: ShGlslOptions::default(),
        }
    }
}

impl Render {
    fn emit(&mut self, s: &str) {
        let line = if s.is_empty() {
            String::new()
        } else {
            format!("{}{}", "    ".repeat(self.depth), s)
        };
        match self.phase {
            Phase::Body => self.body.push(line),
            Phase::Header => self.out.push(line),
        }
    }

    fn mark_todo(&mut self, what: &str) {
        self.todo += 1;
        self.emit(&format!("/* TODO(unsupported): {} */", sanitize(what)));
    }

    fn ident(&self, name: &str) -> String {
        let mut s: String = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
            .collect();
        if s.is_empty() || s.chars().all(|c| c == '_') {
            s.push('v');
        }
        format!("g_{s}")
    }

    /// True when pass 1 collected a reference to any of `names` (used to
    /// gate the input-bridge declarations/seeding on actual use).
    fn uses_any(&self, names: &[&str]) -> bool {
        names.iter().any(|n| self.vars.contains(*n))
    }

    fn var_ty(&self, name: &str) -> Ty {
        if self.arith_vars.contains(name) || self.arith_assigned.contains(name) {
            return Ty::Num; // used in $(( )) → native int wins
        }
        match self.types.get(name) {
            Some(IrType::Int) | Some(IrType::Int32) | Some(IrType::Int64)
            | Some(IrType::UInt32) | Some(IrType::UInt64) | Some(IrType::Float(_)) => Ty::Num,
            _ => Ty::Str, // Str / Any / absent → conservative string
        }
    }

    fn is_num(&self, name: &str) -> bool {
        self.var_ty(name) == Ty::Num
    }

    // ── string table ────────────────────────────────────────────────
    fn strlit(&mut self, s: &str) -> String {
        if self.phase == Phase::Body {
            self.used_str = true;
        }
        if let Some(&(o, l)) = self.str_offsets.get(s) {
            return format!("ivec2({o}, {l})");
        }
        let off = self.str_tab.len() as u32;
        for c in s.chars() {
            let code = if c.is_ascii() { c as i32 } else { 0x3F }; // non-ASCII → '?'
            self.str_tab.push(code);
        }
        let len = s.chars().count() as u32;
        self.str_offsets.insert(s.to_string(), (off, len));
        format!("ivec2({off}, {len})")
    }

    fn emit_table(&mut self) {
        // ES 1.00 has no array constructors at all — and the string
        // machinery (s_tab, s2i/itos/cat/strEq/scratch) is an ES-3.00
        // runtime (dynamic indexing of non-uniform arrays is illegal in
        // ES 1.00). Render fragments like the game's are pure integer
        // pipelines and use NO strings — omit the table entirely then.
        if !self.used_str {
            return;
        }
        // ES 1.00 needs the size in the constructor: int[N](...) — ES
        // 3.00 allows the shorthand int[](...).
        let ctor = |n: usize, items: &str| {
            if self.opts.es100 {
                format!("int[{n}]({items})")
            } else {
                format!("int[]({items})")
            }
        };
        if self.str_tab.is_empty() {
            self.emit(&format!("const int s_tab[1] = {};", ctor(1, "0")));
            return;
        }
        let body: Vec<String> = self.str_tab.iter().map(|c| c.to_string()).collect();
        self.emit(&format!(
            "const int s_tab[{}] = {};",
            body.len(),
            ctor(body.len(), &body.join(", "))
        ));
    }

    // ── globals ─────────────────────────────────────────────────────
    fn emit_globals(&mut self) {
        let arrays: Vec<String> = self.arrays.iter().map(|n| self.ident(n)).collect();
        let vars: Vec<String> = self.vars.iter().map(|n| self.ident(n)).collect();
        for n in arrays {
            if self.is_num_ident(&n) {
                self.emit(&format!("int {n}[{ARR_CAP}];"));
                self.emit(&format!("int {n}_n;"));
            } else {
                self.emit(&format!("ivec2 {n}[{ARR_CAP}];"));
                self.emit(&format!("int {n}_n;"));
            }
        }
        // Scalar vars are file-scope globals ONLY when user functions
        // exist (they may read/write them from their own scope). With no
        // functions every read/write happens inside main(), so they are
        // declared as main() locals instead (see shir_to_glsl_opts): the
        // GLSL compiler gets real registers and the shader carries no
        // mutable global state. Arrays stay global — ES 1.00 restricts
        // dynamic indexing of local arrays.
        if !self.fns.is_empty() {
            for n in vars {
                // float_vars stores the UNMANGLED var name
                let name = n.strip_prefix("g_").unwrap_or(&n);
                if self.float_vars.contains(name) {
                    self.emit(&format!("float {n};"));
                } else if self.is_num_ident(&n) {
                    self.emit(&format!("int {n};"));
                } else {
                    self.emit(&format!("ivec2 {n};"));
                }
            }
        }
        if self.used_str {
            self.emit(&format!("int s_scratch[{SCRATCH_CAP}];"));
            self.emit("int s_spos = 0;");
        }
        // The output byte buffer. The dynamic writer is putCh (the
        // OUT_CAP guard + out_len counter); it is live when the string
        // runtime is (putStr/printf/echo) or when ES 3.00 putb lowers to
        // it. A pure ES 1.00 render fragment writes putb bytes at FIXED
        // const slots and the colour line reads back 0..3 — a 4-slot (or
        // putb-sized) buffer is enough, with no counter and no cap const.
        let dyn_out = self.used_str || (self.used_putb && !self.opts.es100);
        if !self.opts.vert_out && (!self.opts.color_out || dyn_out) {
            self.emit(&format!("int out_buf[{OUT_CAP}];"));
            self.emit("int out_len = 0;");
        } else if !self.opts.vert_out {
            let cap = std::cmp::max(4, self.putb_pos);
            self.emit(&format!("int out_buf[{cap}];"));
        }
        if self.used_pa {
            self.emit(&format!("ivec2 g_pa[{PARAM_CAP}];"));
            self.emit("int g_pa_n = 0;");
        }
        if self.used_fit {
            self.emit(&format!("ivec2 g_fit[{FIT_CAP}];"));
            self.emit("int g_fit_n = 0;");
        }
    }

    /// like `is_num(name)` but for an already-mangled `g_` ident
    fn is_num_ident(&self, mangled: &str) -> bool {
        let name = mangled.strip_prefix("g_").unwrap_or(mangled);
        self.is_num(name)
    }

    // ── runtime helpers ─────────────────────────────────────────────
    fn emit_helpers(&mut self) {
        // the string/scratch runtime — only when the program uses it
        if !self.used_str {
            // ES 3.00 `putb` lowers to putCh — keep that one helper
            // (its out_len/OUT_CAP deps are emitted by emit_globals).
            if self.used_putb && !self.opts.es100 {
                self.emit("void putCh(int c) { if (out_len < OUT_CAP) out_buf[out_len++] = c; }");
                self.emit("");
            }
            return;
        }
        self.emit("int s2i(ivec2 s) {");
        self.emit("    int v = 0; int i = 0; int sign = 1;");
        self.emit("    if (s.y > 0 && s_tab[s.x] == 45) { sign = -1; i = 1; }");
        self.emit("    for (; i < s.y; i++) {");
        self.emit("        int c = s_tab[s.x + i];");
        self.emit("        if (c < 48 || c > 57) break;");
        self.emit("        v = v * 10 + (c - 48);");
        self.emit("    }");
        self.emit("    return v * sign;");
        self.emit("}");
        self.emit("");
        self.emit("ivec2 itos(int v) {");
        self.emit("    int base = s_spos;");
        self.emit("    int n = v;");
        self.emit("    int sign = 0;");
        self.emit("    if (n < 0) { sign = 1; n = -n; }");
        self.emit("    int start = s_spos + sign;");
        self.emit("    int hi = start;");
        self.emit("    do { s_scratch[hi++] = 48 + n % 10; n = n / 10; } while (n != 0);");
        self.emit("    int nd = hi - start;");
        self.emit("    for (int i = 0; i < nd / 2; i++) {");
        self.emit("        int t = s_scratch[start + i];");
        self.emit("        s_scratch[start + i] = s_scratch[start + nd - 1 - i];");
        self.emit("        s_scratch[start + nd - 1 - i] = t;");
        self.emit("    }");
        self.emit("    if (sign == 1) s_scratch[base] = 45;");
        self.emit("    s_spos = hi;");
        self.emit("    return ivec2(base, nd + sign);");
        self.emit("}");
        self.emit("");
        self.emit("ivec2 cat(ivec2 a, ivec2 b) {");
        self.emit("    int base = s_spos;");
        self.emit("    for (int i = 0; i < a.y; i++) { s_scratch[s_spos++] = s_tab[a.x + i]; }");
        self.emit("    for (int i = 0; i < b.y; i++) { s_scratch[s_spos++] = s_tab[b.x + i]; }");
        self.emit("    return ivec2(base, a.y + b.y);");
        self.emit("}");
        self.emit("");
        self.emit("void putCh(int c) { if (out_len < OUT_CAP) out_buf[out_len++] = c; }");
        self.emit("void putStr(ivec2 s) { for (int i = 0; i < s.y; i++) { putCh(s_tab[s.x + i]); } }");
        self.emit("void putStrLn(ivec2 s) { putStr(s); putCh(10); }");
        self.emit("");
        self.emit("bool strEq(ivec2 a, ivec2 b) {");
        self.emit("    if (a.y != b.y) return false;");
        self.emit("    for (int i = 0; i < a.y; i++) { if (s_tab[a.x + i] != s_tab[b.x + i]) return false; }");
        self.emit("    return true;");
        self.emit("}");
        self.emit("");
        self.emit("bool globMatch(ivec2 s, ivec2 p) {");
        self.emit("    int si = 0, pi = 0, star = -1, mark = 0;");
        self.emit("    while (si < s.y) {");
        self.emit("        if (pi < p.y && (s_tab[p.x + pi] == 63 || s_tab[p.x + pi] == s_tab[s.x + si])) { si++; pi++; }");
        self.emit("        else if (pi < p.y && s_tab[p.x + pi] == 42) { star = pi++; mark = si; }");
        self.emit("        else if (star >= 0) { pi = star + 1; si = ++mark; }");
        self.emit("        else return false;");
        self.emit("    }");
        self.emit("    while (pi < p.y && s_tab[p.x + pi] == 42) pi++;");
        self.emit("    return pi == p.y;");
        self.emit("}");
        self.emit("");
        if self.used_ipow {
        self.emit("int ipow(int a, int b) {");
        self.emit("    int r = 1;");
        self.emit("    for (int i = 0; i < b; i++) { r = r * a; }");
        self.emit("    return r;");
        self.emit("}");
        self.emit("");
        }
        if self.used_isqrt {
        self.emit("int isqrt32(int v) {");
        self.emit("    if (v <= 0) return 0;");
        self.emit("    int lo = 0;");
        self.emit("    int hi = 1;");
        self.emit("    while (hi <= 46340 && hi * hi <= v) { hi = hi * 2; }");
        self.emit("    while (lo + 1 < hi) {");
        self.emit("        int mid = lo + (hi - lo) / 2;");
        self.emit("        if (mid * mid <= v) { lo = mid; } else { hi = mid; }");
        self.emit("    }");
        self.emit("    return lo;");
        self.emit("}");
        self.emit("");
        }
    }

    fn emit_fn_prototypes(&mut self) {
        let names: Vec<String> = self.fn_order.clone();
        for name in names {
            self.emit(&format!("void {}();", self.ident(&name)));
        }
        if !self.fn_order.is_empty() {
            self.emit("");
        }
    }

    /// Hoisted GLSL function definitions (file scope — GLSL has no
    /// nested functions, so Function statements render here, not inline).
    fn emit_fn_defs(&mut self) {
        let names: Vec<String> = self.fn_order.clone();
        for name in names {
            if let Some(body) = self.fn_bodies.get(&name) {
                let body = body.clone();
                self.render_fn(&name, &body);
            }
        }
    }

    // ── texture-fetch load sinking (the LazyTexSink hooks) ────────
    fn is_tex_sunk(&self, g: TexGroup) -> bool {
        self.lazy_tex_sinks.iter().any(|s| s.group == g)
    }

    // Emit the ONE fetch + per-channel seeds for a texture group. The
    // per-channel seeds are use-gated (only referenced channels emit);
    // at a sink site every referenced channel is read inside the block.
    fn emit_tex_seeds(&mut self, g: TexGroup) {
        if self.opts.tex_size == 0 {
            return;
        }
        // Sample through `fract(vUv)` — the wrap happens here, in [0,1)
        // space, so the texture2D coordinate stays small and is EXACT at
        // any precision. The old `(g_uv_x + 0.5) / sz` form built a
        // coordinate up to ±35 for the camera-following background planes
        // (vUv = world xz) — a MEDIUMP float (fp16 on Vulkan/Metal
        // ANGLE) quantized its fractional part to ±1 texel, so the floor
        // texture's selection jittered ("sometimes shows"). fract() keeps
        // the wrap value tiny (fp16-exact), and REPEAT wrap mode is no
        // longer required for the sampling either.
        let uv = "fract(vUv)".to_string();
        match g {
            TexGroup::Tex => {
                if self.vars.contains("tex_r")
                    || self.vars.contains("tex_g")
                    || self.vars.contains("tex_b")
                {
                    self.emit(&format!("vec4 _tex = texture2D(uTex, {uv});"));
                    if self.vars.contains("tex_r") {
                        self.emit("g_tex_r = int(_tex.r * 255.0);");
                    }
                    if self.vars.contains("tex_g") {
                        self.emit("g_tex_g = int(_tex.g * 255.0);");
                    }
                    if self.vars.contains("tex_b") {
                        self.emit("g_tex_b = int(_tex.b * 255.0);");
                    }
                }
            }
            TexGroup::Crack => {
                if self.vars.contains("cr_r")
                    || self.vars.contains("cr_g")
                    || self.vars.contains("cr_b")
                    || self.vars.contains("cr_a")
                {
                    self.emit(&format!("vec4 _crack = texture2D(uCrack, {uv});"));
                    if self.vars.contains("cr_r") {
                        self.emit("g_cr_r = int(_crack.r * 127.0);");
                    }
                    if self.vars.contains("cr_g") {
                        self.emit("g_cr_g = int(_crack.g * 127.0);");
                    }
                    if self.vars.contains("cr_b") {
                        self.emit("g_cr_b = int(_crack.b * 127.0);");
                    }
                    if self.vars.contains("cr_a") {
                        self.emit("g_cr_a = int(_crack.a * 127.0);");
                    }
                }
            }
        }
    }

    // When a block is a recorded sink site, emit the group's fetch +
    // seeds as its first statements (the block dominates every read).
    fn emit_lazy_tex_seeds(&mut self, blk: &[IrStmt]) {
        let blk = blk as *const [IrStmt];
        let groups: Vec<TexGroup> = self
            .lazy_tex_sinks
            .iter()
            .filter(|s| std::ptr::eq(s.block, blk))
            .map(|s| s.group)
            .collect();
        for g in groups {
            self.emit_tex_seeds(g);
        }
    }

    fn render_fn(&mut self, name: &str, body: &[IrStmt]) {
        self.emit(&format!("void {}() {{", self.ident(name)));
        self.depth += 1;
        let saved = self.in_fn;
        self.in_fn = true;
        for s in body {
            self.stmt(s);
        }
        self.in_fn = saved;
        self.depth -= 1;
        self.emit("}");
        self.emit("");
    }

    // ── pass 1: collect names and string literals ───────────────────
    fn collect_stmt(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    if t.indices.is_empty() {
                        // `arr=(...)` with a setArray value is an ARRAY
                        // even though the target has no indices
                        let is_array_val = matches!(
                            expr,
                            IrExpr::Call { func, args }
                                if (func == "setArray" || func == "setArrayAppend")
                                    && matches!(args.first(), Some(IrExpr::Str(n, _)) if n == &t.var)
                        );
                        if is_array_val {
                            self.arrays.insert(t.var.clone());
                        } else if !self.arrays.contains(&t.var) {
                            self.vars.insert(t.var.clone());
                        }
                    } else {
                        self.arrays.insert(t.var.clone());
                    }
                }
                self.collect_expr(expr);
                // GLSL div/mod are well-defined (no bash abort) — a var
                // whose assignment source is a $((...)) arith is numeric
                // here even though the shared numeric-lift analysis keeps
                // `/` `%` sources string-typed (a JS runtime concern).
                if let IrExpr::Arith(_) = expr {
                    for t in targets {
                        if t.indices.is_empty() {
                            self.arith_assigned.insert(t.var.clone());
                        }
                    }
                }
                // a float bc capture (`v=$(echo "scale=K; …0.5" | bc)`) —
                // detected in pass 1 so the main() locals (declared
                // before the body renders) declare it GLSL float. A
                // float-var copy (`v=$w` — w already float) propagates
                // the verdict the same way.
                if targets.len() == 1 && targets[0].indices.is_empty() {
                    if self.is_float_bc_capture(expr) {
                        self.float_vars.insert(targets[0].var.clone());
                    } else if let Some(n) = self.var_name_of(expr) {
                        if self.float_vars.contains(n) {
                            self.float_vars.insert(targets[0].var.clone());
                        }
                    }
                }
            }
            IrStmt::Declare { vars, init, .. } => {
                for Decl { name, .. } in vars {
                    self.vars.insert(name.clone());
                }
                if let Some(i) = init {
                    self.collect_expr(i);
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                self.arrays.insert(var.clone());
                for e in elements {
                    self.collect_expr(e);
                }
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                self.collect_expr(cond);
                for s in then {
                    self.collect_stmt(s);
                }
                for (e, b) in elsifs {
                    self.collect_expr(e);
                    for s in b {
                        self.collect_stmt(s);
                    }
                }
                for s in else_ {
                    self.collect_stmt(s);
                }
            }
            IrStmt::For { var, iter, body } => {
                self.vars.insert(var.clone());
                self.collect_expr(iter);
                for s in body {
                    self.collect_stmt(s);
                }
            }
            IrStmt::ForInit {
                init,
                cond,
                step,
                body,
            } => {
                for s in init {
                    self.collect_stmt(s);
                }
                self.collect_expr(cond);
                for s in step {
                    self.collect_stmt(s);
                }
                for s in body {
                    self.collect_stmt(s);
                }
            }
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                self.collect_expr(cond);
                for s in body {
                    self.collect_stmt(s);
                }
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                self.collect_expr(discriminant);
                for IrCaseClause { patterns, body } in clauses {
                    for p in patterns {
                        self.strlit(p); // patterns live in the string table
                    }
                    for s in body {
                        self.collect_stmt(s);
                    }
                }
            }
            IrStmt::Function { name, body, .. } => {
                if !self.fns.contains(name) {
                    self.fn_order.push(name.clone());
                }
                self.fns.insert(name.clone());
                self.fn_bodies.insert(name.clone(), body.clone());
                for s in body {
                    self.collect_stmt(s);
                }
            }
            IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => {
                for s in body {
                    self.collect_stmt(s);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                for s in inner {
                    self.collect_stmt(s);
                }
                for r in redirects {
                    self.collect_expr(&r.target);
                }
            }
            IrStmt::Exec {
                cmd,
                args,
                redirects,
                env,
                ..
            } => {
                self.collect_expr(cmd);
                for a in args {
                    self.collect_expr(a);
                }
                for r in redirects {
                    self.collect_expr(r);
                }
                for (_, v) in env {
                    self.collect_expr(v);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    for s in st {
                        self.collect_stmt(s);
                    }
                }
            }
            IrStmt::Expr(e) => {
                // exec("local", [..]) / setVar("name", ..) introduce vars
                // that must exist as globals even though the neutral IR
                // has no Declare for them.
                if let IrExpr::Call { func, args } = e {
                    if func == "setVar" || func == "assign" {
                        if let Some(IrExpr::Str(name, _)) = args.first() {
                            if let Some(base) = base_var_name(name) {
                                self.vars.insert(base.to_string());
                                // float bc capture via the setVar call
                                // form (same pass-1 detection); float-var
                                // copies propagate the verdict too.
                                if args.len() >= 2 {
                                    if self.is_float_bc_capture(&args[1]) {
                                        self.float_vars.insert(base.to_string());
                                    } else if let Some(n) = self.var_name_of(&args[1]) {
                                        if self.float_vars.contains(n) {
                                            self.float_vars.insert(base.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if func == "exec" {
                        if let Some(IrExpr::Str(cmd, _)) = args.first() {
                            if cmd == "local" {
                                for item in self.exec_items(&args[1..]) {
                                    self.local_names(&item);
                                }
                            }
                        }
                    }
                }
                self.collect_expr(e);
            }
            IrStmt::Return(Some(e)) => self.collect_expr(e),
            IrStmt::Exit(Some(e)) => self.collect_expr(e),
            IrStmt::SetChildError(e) => self.collect_expr(e),
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => self.collect_expr(expr),
            _ => {}
        }
    }

    fn collect_expr(&mut self, e: &IrExpr) {
        match e {
            IrExpr::Str(s, _) => {
                self.strlit(s);
            }
            IrExpr::Var(n, _) => {
                if n == "uv_x" || n == "uv_y" {
                    self.reads_uv = true;
                }
                self.vars.insert(n.clone());
            }
            IrExpr::Index { var, key } => {
                self.arrays.insert(var.clone());
                self.collect_expr(key);
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                self.collect_expr(lhs);
                self.collect_expr(rhs);
            }
            IrExpr::Call { func, args } => {
                for a in args {
                    self.collect_expr(a);
                }
                // arith("n % i") / test("$i -lt 3") reference vars by
                // STRING — parse them so the globals get declared.
                if func == "arith" {
                    if let Some(IrExpr::Str(text, _)) = args.first() {
                        if let Some(a) = crate::shir::parse_arith(text) {
                            self.collect_arith(&a);
                        }
                    }
                }
                if func == "test" {
                    for a in args {
                        if let IrExpr::Str(text, _) = a {
                            self.collect_test_vars(text);
                        }
                    }
                }
                if func == "getVar" {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        if let Some(base) = base_var_name(name) {
                            self.vars.insert(base.to_string());
                        }
                    }
                }
            }
            IrExpr::MethodCall { obj, args, .. } => {
                self.collect_expr(obj);
                for a in args {
                    self.collect_expr(a);
                }
            }
            IrExpr::Ternary {
                cond,
                then,
                else_,
            } => {
                self.collect_expr(cond);
                self.collect_expr(then);
                self.collect_expr(else_);
            }
            IrExpr::DefinedOr { expr, default } => {
                self.collect_expr(expr);
                self.collect_expr(default);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => {
                            self.strlit(s);
                        }
                        InterpPart::Expr(e) => self.collect_expr(e),
                    }
                }
            }
            IrExpr::Capture { expr, .. } => self.collect_expr(expr),
            IrExpr::Regex { pattern, .. } => {
                self.strlit(pattern);
            }
            IrExpr::Arrow(body) => {
                for s in body {
                    self.collect_stmt(s);
                }
            }
            IrExpr::Array(items) => {
                for i in items {
                    self.collect_expr(i);
                }
            }
            IrExpr::Arith(a) => self.collect_arith(a),
            IrExpr::Object(pairs) => {
                for (_, v) in pairs {
                    self.collect_expr(v);
                }
            }
            IrExpr::Ident(n) => {
                self.vars.insert(n.clone());
            }
            _ => {}
        }
    }

    /// `$name` / `${name}` / `"$name"` / `$(( … ))` tokens inside a test
    /// string.
    fn collect_test_vars(&mut self, text: &str) {
        let mut rest = text;
        while let Some(pos) = rest.find('$') {
            let after = &rest[pos + 1..];
            if after.starts_with("((") {
                // $(( arith )) — collect the vars the arithmetic reads
                // (test operands parse the same expression at render
                // time; without this the bridge gating would under-
                // declare a `[ $((frag_x * 2)) -gt 3 ]` reference).
                let chars: Vec<char> = after.chars().collect();
                let mut depth = 0i32;
                let mut j = 0;
                let mut started = false;
                while j < chars.len() {
                    if chars[j] == '(' {
                        depth += 1;
                        started = true;
                    } else if chars[j] == ')' {
                        depth -= 1;
                    }
                    j += 1;
                    if started && depth == 0 {
                        break;
                    }
                }
                let inner = after[2..j.saturating_sub(2)].to_string();
                if let Some(a) = crate::shir::parse_arith(&inner) {
                    self.collect_arith(&a);
                }
                rest = &after[j.min(after.len())..];
                continue;
            }
            let mut name = String::new();
            if after.starts_with('{') {
                for c in after[1..].chars() {
                    if c == '}' {
                        break;
                    }
                    name.push(c);
                }
            } else {
                for c in after.chars() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        name.push(c);
                    } else {
                        break;
                    }
                }
            }
            if !name.is_empty() && !is_positional(&name) {
                if let Some(base) = base_var_name(&name) {
                    self.vars.insert(base.to_string());
                }
            }
            // advance past this token
            let skip = if after.starts_with('{') {
                after.find('}').map(|i| i + 1).unwrap_or(after.len())
            } else {
                name.len()
            };
            rest = &after[skip.min(after.len())..];
        }
    }

    fn collect_arith(&mut self, a: &ArithAst) {
        match a {
            ArithAst::Var(n) => {
                self.arith_vars.insert(n.clone());
                self.vars.insert(n.clone());
            }
            ArithAst::Index { var, key } => {
                self.arith_vars.insert(var.clone());
                self.arrays.insert(var.clone());
                self.collect_arith(key);
            }
            ArithAst::Bin { lhs, rhs, .. } | ArithAst::Cond { test: lhs, then: rhs, .. } => {
                self.collect_arith(lhs);
                self.collect_arith(rhs);
            }
            ArithAst::Un { arg, .. } => self.collect_arith(arg),
            ArithAst::Assign { var, rhs, .. } => {
                self.arith_vars.insert(var.clone());
                self.vars.insert(var.clone());
                self.collect_arith(rhs);
            }
            ArithAst::IncDec { var, .. } => {
                self.arith_vars.insert(var.clone());
                self.vars.insert(var.clone());
            }
            _ => {}
        }
    }

    // ── type inference ──────────────────────────────────────────────
    fn ty(&self, e: &IrExpr) -> Ty {
        match e {
            IrExpr::Int(_) => Ty::Num,
            IrExpr::Bool(_) => Ty::Bool,
            IrExpr::Str(_, _) => Ty::Str,
            IrExpr::Var(n, _) => self.var_ty(n),
            IrExpr::Ident(n) => self.var_ty(n),
            IrExpr::Index { var, .. } => self.var_ty(var),
            IrExpr::BinOp { op, .. } => match op {
                BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Lt | BinOpKind::Gt
                | BinOpKind::Le | BinOpKind::Ge | BinOpKind::And | BinOpKind::Or
                | BinOpKind::Not => Ty::Bool,
                BinOpKind::Concat => Ty::Str,
                _ => Ty::Num,
            },
            IrExpr::Call { func, .. } => match func.as_str() {
                "test" => Ty::Bool,
                "arith" | "param" | "len" => Ty::Num,
                "getVar" | "setVar" => Ty::Str,
                _ => Ty::Bool, // exec / fnCall status
            },
            IrExpr::Ternary { then, else_, .. } => {
                if self.ty(then) == Ty::Num && self.ty(else_) == Ty::Num {
                    Ty::Num
                } else {
                    Ty::Str
                }
            }
            IrExpr::Arith(_) | IrExpr::Range { .. } | IrExpr::RawExpr(_) => Ty::Num,
            _ => Ty::Str,
        }
    }

    // ── expression rendering ────────────────────────────────────────
    fn expr(&mut self, e: &IrExpr) -> String {
        match self.ty(e) {
            Ty::Num => self.expr_num(e),
            Ty::Bool => self.expr_bool(e),
            Ty::Str => self.expr_str(e),
        }
    }

    fn expr_num(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Int(n) => {
                let n = *n;
                if n < i32::MIN as i64 || n > i32::MAX as i64 {
                    self.todo += 1;
                    format!("/* TODO(i64→i32 wrap: {n}) */ 0")
                } else {
                    format!("({n})")
                }
            }
            IrExpr::Str(s, _) => match s.trim().parse::<i64>() {
                Ok(n) if n >= i32::MIN as i64 && n <= i32::MAX as i64 => format!("({n})"),
                Ok(n) => {
                    self.todo += 1;
                    format!("/* TODO(i64→i32 wrap: {n}) */ 0")
                }
                Err(_) => {
                    self.todo += 1;
                    format!("/* TODO(non-numeric str: {}) */ 0", sanitize(s))
                }
            },
            IrExpr::Bool(b) => format!("({})", if *b { 1 } else { 0 }),
            IrExpr::Var(n, _) | IrExpr::Ident(n) => {
                if self.float_vars.contains(n) {
                    format!("int({})", self.ident(n))
                } else if self.is_num(n) {
                    self.ident(n)
                } else {
                    format!("s2i({})", self.ident(n))
                }
            }
            IrExpr::Index { var, key } => {
                let k = self.expr_num(key);
                format!("{}[{}]", self.ident(var), k)
            }
            IrExpr::BinOp { lhs, op, rhs } => self.binop_num(lhs, op, rhs),
            IrExpr::Ternary {
                cond,
                then,
                else_,
            } => {
                let c = self.expr_num(cond);
                let t = self.expr_num(then);
                let el = self.expr_num(else_);
                format!("((({c}) != 0) ? ({t}) : ({el}))")
            }
            IrExpr::Call { func, args } => match func.as_str() {
                "getVar" => self.getvar_num(args),
                "capture" | "captureWords" => {
                    if let Some(pipe) = self.capture_pipeline(args) {
                        if let Some(s) = self.bc_capture(pipe) {
                            return format!("s2i({s})");
                        }
                    }
                    self.todo += 1;
                    "/* TODO(cmdsub num) */ 0".to_string()
                }
                "arith" => {
                    if let Some(IrExpr::Str(text, _)) = args.first() {
                        match crate::shir::parse_arith(text) {
                            Some(a) => self.arith(&a),
                            None => {
                                self.todo += 1;
                                "/* TODO(arith parse) */ 0".to_string()
                            }
                        }
                    } else {
                        self.todo += 1;
                        "/* TODO(arith arg) */ 0".to_string()
                    }
                }
                _ => {
                    self.todo += 1;
                    format!("/* TODO(call {}) */ 0", sanitize(func))
                }
            },
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Range { start, end } => {
                let _ = end;
                format!("({start})")
            }
            IrExpr::DefinedOr { expr, .. } => self.expr_num(expr),
            IrExpr::Capture { expr, .. } => {
                // the first-class Capture node (core request
                // zsh-sh-go-20260814-230503) — unwrap the Arrow like the
                // Call-capture arm below before the bc fold.
                if let Some(pipe) = self.capture_pipeline(std::slice::from_ref(expr.as_ref())) {
                    if let Some(s) = self.bc_capture(pipe) {
                        return format!("s2i({s})");
                    }
                }
                self.todo += 1;
                "/* TODO(cmdsub num) */ 0".to_string()
            }
            IrExpr::Call { func, args } if func == "capture" || func == "captureWords" => {
                if let Some(pipe) = self.capture_pipeline(args) {
                    if let Some(s) = self.bc_capture(pipe) {
                        return format!("s2i({s})");
                    }
                }
                self.todo += 1;
                "/* TODO(cmdsub num) */ 0".to_string()
            }
            _ => {
                self.todo += 1;
                format!("/* TODO(expr_num {:?}) */ 0", std::mem::discriminant(e))
            }
        }
    }

    fn binop_num(&mut self, lhs: &IrExpr, op: &BinOpKind, rhs: &IrExpr) -> String {
        let l = self.expr_num(lhs);
        let r = self.expr_num(rhs);
        let glsl_op = match op {
            BinOpKind::Add => "+",
            BinOpKind::Sub => "-",
            BinOpKind::Mul => "*",
            BinOpKind::Div => "/",
            BinOpKind::Mod => {
                // ES 1.00 has no integer % — lower to a - b*(a/b) (GLSL
                // int division truncates toward zero, like bash's %)
                return if self.opts.es100 {
                    format!("(({l}) - (({r}) * (({l}) / ({r}))))")
                } else {
                    format!("(({l}) % ({r}))")
                };
            }
            BinOpKind::Pow => {
                self.used_ipow = true;
                return format!("ipow({l}, {r})");
            }
            BinOpKind::BitAnd => "&",
            BinOpKind::BitOr => "|",
            BinOpKind::BitXor => "^",
            BinOpKind::ShiftL => "<<",
            BinOpKind::ShiftR => ">>",
            BinOpKind::And | BinOpKind::Or | BinOpKind::Not => {
                let gop = match op {
                    BinOpKind::And => "&&",
                    BinOpKind::Or => "||",
                    _ => "&&",
                };
                return format!("((({l}) != 0) {gop} (({r}) != 0)) ? 1 : 0");
            }
            BinOpKind::Eq => "==",
            BinOpKind::Ne => "!=",
            BinOpKind::Lt => "<",
            BinOpKind::Gt => ">",
            BinOpKind::Le => "<=",
            BinOpKind::Ge => ">=",
            BinOpKind::Concat => {
                self.todo += 1;
                return format!("/* TODO(concat in num ctx) */ 0");
            }
        };
        format!("(({l}) {glsl_op} ({r}))")
    }

    fn expr_bool(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Bool(b) => format!("({})", if *b { "true" } else { "false" }),
            IrExpr::BinOp { lhs, op, rhs } => {
                let l = self.expr_num(lhs);
                let r = self.expr_num(rhs);
                let gop = match op {
                    BinOpKind::Eq => "==",
                    BinOpKind::Ne => "!=",
                    BinOpKind::Lt => "<",
                    BinOpKind::Gt => ">",
                    BinOpKind::Le => "<=",
                    BinOpKind::Ge => ">=",
                    BinOpKind::And => "&&",
                    BinOpKind::Or => "||",
                    _ => {
                        self.todo += 1;
                        return format!("/* TODO(bool op {op:?}) */ false");
                    }
                };
                if matches!(op, BinOpKind::And | BinOpKind::Or) {
                    format!("((({l}) != 0) {gop} (({r}) != 0))")
                } else {
                    format!("(({l}) {gop} ({r}))")
                }
            }
            IrExpr::Call { func, args } if func == "test" => self.test_call(args),
            IrExpr::Call { func, .. } => {
                self.todo += 1;
                format!("/* TODO(cond call {}) */ false", sanitize(func))
            }
            IrExpr::Ternary {
                cond,
                then,
                else_,
            } => {
                let c = self.expr_num(cond);
                let t = self.expr_bool(then);
                let el = self.expr_bool(else_);
                format!("((({c}) != 0) ? ({t}) : ({el}))")
            }
            _ => {
                // bash truthiness: non-empty string / nonzero number
                let s = self.expr_str(e);
                format!("({s}.y > 0)")
            }
        }
    }

    fn expr_str(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => self.strlit(s),
            IrExpr::Int(n) => self.strlit(&n.to_string()),
            IrExpr::Bool(b) => self.strlit(if *b { "1" } else { "0" }),
            IrExpr::Var(n, _) | IrExpr::Ident(n) => {
                if self.is_num(n) {
                    format!("itos({})", self.ident(n))
                } else {
                    self.ident(n)
                }
            }
            IrExpr::Index { var, key } => {
                let k = self.expr_num(key);
                if self.is_num(var) {
                    format!("itos({}[{}])", self.ident(var), k)
                } else {
                    format!("{}[{}]", self.ident(var), k)
                }
            }
            IrExpr::BinOp { lhs, op: BinOpKind::Concat, rhs } => {
                let l = self.expr_str(lhs);
                let r = self.expr_str(rhs);
                format!("cat({l}, {r})")
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                // numeric op in string context: compute then itos
                let _ = op;
                let l = self.expr_num(e);
                let _ = (lhs, rhs);
                format!("itos({l})")
            }
            IrExpr::Interpolate(parts) => {
                let mut acc: Option<String> = None;
                let mut pieces = Vec::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => pieces.push(self.strlit(s)),
                        InterpPart::Expr(e) => pieces.push(self.expr_str(e)),
                    }
                }
                for p in pieces {
                    acc = Some(match acc {
                        None => p,
                        Some(a) => format!("cat({a}, {p})"),
                    });
                }
                acc.unwrap_or_else(|| self.strlit(""))
            }
            IrExpr::Call { func, args } => match func.as_str() {
                "getVar" => self.getvar_str(args),
                "capture" | "captureWords" => {
                    if let Some(pipe) = self.capture_pipeline(args) {
                        if let Some(s) = self.bc_capture(pipe) {
                            return s;
                        }
                    }
                    self.todo += 1;
                    "/* TODO(command substitution) */ ivec2(0, 0)".to_string()
                }
                "exec" | "fnCall" | "call" => {
                    self.todo += 1;
                    format!("/* TODO(call {}) */ ivec2(0, 0)", sanitize(func))
                }
                "arith" => {
                    let n = self.expr_num(e);
                    format!("itos({n})")
                }
                _ => {
                    self.todo += 1;
                    format!("/* TODO(call {}) */ ivec2(0, 0)", sanitize(func))
                }
            },
            IrExpr::Ternary {
                cond,
                then,
                else_,
            } => {
                let c = self.expr_num(cond);
                let t = self.expr_str(then);
                let el = self.expr_str(else_);
                format!("((({c}) != 0) ? ({t}) : ({el}))")
            }
            IrExpr::DefinedOr { expr, .. } => self.expr_str(expr),
            IrExpr::Capture { expr, .. } => {
                // the first-class Capture node (core request
                // zsh-sh-go-20260814-230503) — unwrap the Arrow like the
                // Call-capture arm below before the bc fold.
                if let Some(pipe) = self.capture_pipeline(std::slice::from_ref(expr.as_ref())) {
                    if let Some(s) = self.bc_capture(pipe) {
                        return s;
                    }
                }
                self.todo += 1;
                "/* TODO(command substitution) */ ivec2(0, 0)".to_string()
            }
            IrExpr::Call { func, args } if func == "capture" || func == "captureWords" => {
                if let Some(pipe) = self.capture_pipeline(args) {
                    if let Some(s) = self.bc_capture(pipe) {
                        return s;
                    }
                }
                self.todo += 1;
                "/* TODO(command substitution) */ ivec2(0, 0)".to_string()
            }
            IrExpr::Regex { .. } => {
                self.todo += 1;
                "/* TODO(regex) */ ivec2(0, 0)".to_string()
            }
            IrExpr::Array(items) => {
                // a bare array in string context: join with spaces
                let space = self.strlit(" ");
                let mut acc: Option<String> = None;
                for item in items {
                    let p = self.expr_str(item);
                    acc = Some(match acc {
                        None => p,
                        Some(a) => format!("cat(cat({a}, {space}), {p})"),
                    });
                }
                acc.unwrap_or_else(|| self.strlit(""))
            }
            _ => {
                self.todo += 1;
                format!("/* TODO(expr_str {:?}) */ ivec2(0, 0)", std::mem::discriminant(e))
            }
        }
    }

    /// `getVar("name")` / `getVar("1")` — var read (numeric context).
    /// Param-expansion names (`MAXWAIT:-10`) normalize to their base id.
    fn getvar_num(&mut self, args: &[IrExpr]) -> String {
        if let Some(IrExpr::Str(name, _)) = args.first() {
            if let Some(v) = self.special_var_num(name) {
                return v;
            }
            if is_positional(name) {
                return self.positional_num(name);
            }
            let Some(base) = base_var_name(name) else {
                self.todo += 1;
                return format!("/* TODO(getVar {}) */ 0", sanitize(name));
            };
            if self.float_vars.contains(base) {
                format!("int({})", self.ident(base))
            } else if self.is_num(base) {
                self.ident(base)
            } else {
                format!("s2i({})", self.ident(base))
            }
        } else {
            self.todo += 1;
            "/* TODO(getVar arg) */ 0".to_string()
        }
    }

    /// `getVar("name")` / `getVar("1")` — var read (string context).
    fn getvar_str(&mut self, args: &[IrExpr]) -> String {
        if let Some(IrExpr::Str(name, _)) = args.first() {
            if let Some(v) = self.special_var_str(name) {
                return v;
            }
            if is_positional(name) {
                return self.positional_str(name);
            }
            let Some(base) = base_var_name(name) else {
                self.todo += 1;
                return format!("/* TODO(getVar {}) */ ivec2(0, 0)", sanitize(name));
            };
            if self.is_num(base) {
                format!("itos({})", self.ident(base))
            } else {
                self.ident(base)
            }
        } else {
            self.todo += 1;
            "/* TODO(getVar arg) */ ivec2(0, 0)".to_string()
        }
    }

    /// Shell special vars with no GPU analogue: `$?` status, `$#` arg
    /// count, `$@`/`$*` positional list. Inside functions `$#` is the
    /// param count (g_pa_n); everything else is a conservative 0/"".
    fn special_var_num(&mut self, name: &str) -> Option<String> {
        match name {
            "?" => {
                self.todo += 1;
                Some("/* TODO($?) */ 0".to_string())
            }
            "#" => Some(if self.in_fn {
                self.used_pa = true;
                "g_pa_n".to_string()
            } else {
                "0".to_string()
            }),
            "@" | "*" => {
                self.todo += 1;
                Some("/* TODO($@) */ 0".to_string())
            }
            _ => None,
        }
    }

    fn special_var_str(&mut self, name: &str) -> Option<String> {
        match name {
            "?" | "@" | "*" => {
                self.todo += 1;
                Some("/* TODO($special) */ ivec2(0, 0)".to_string())
            }
            "#" => Some(if self.in_fn {
                self.used_pa = true;
                "itos(g_pa_n)".to_string()
            } else {
                "ivec2(0, 0)".to_string()
            }),
            _ => None,
        }
    }

    fn positional_num(&mut self, name: &str) -> String {
        if self.in_fn {
            self.used_pa = true;
            let i = name.parse::<usize>().unwrap_or(1).saturating_sub(1).min(PARAM_CAP - 1);
            format!("s2i(g_pa[{i}])")
        } else {
            "0".to_string() // no argv on the GPU
        }
    }

    fn positional_str(&mut self, name: &str) -> String {
        if self.in_fn {
            self.used_pa = true;
            let i = name.parse::<usize>().unwrap_or(1).saturating_sub(1).min(PARAM_CAP - 1);
            format!("g_pa[{i}]")
        } else {
            "ivec2(0, 0)".to_string() // no argv on the GPU
        }
    }

    // ── arithmetic AST ───────────────────────────────────────────────
    fn arith(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Num(n) => {
                let n = *n;
                if n < i32::MIN as i64 || n > i32::MAX as i64 {
                    self.todo += 1;
                    format!("/* TODO(i64→i32 wrap: {n}) */ 0")
                } else {
                    format!("({n})")
                }
            }
            ArithAst::Var(name) | ArithAst::Ident(name) => {
                if self.is_num(name) {
                    self.ident(name)
                } else {
                    format!("s2i({})", self.ident(name))
                }
            }
            ArithAst::Index { var, key } => {
                let k = self.arith(key);
                format!("{}[{}]", self.ident(var), k)
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let l = self.arith(lhs);
                let r = self.arith(rhs);
                match op.as_str() {
                    "**" => {
                        self.used_ipow = true;
                        format!("ipow({l}, {r})")
                    }
                    "&&" | "||" => {
                        let g = if op == "&&" { "&&" } else { "||" };
                        format!("((({l}) != 0) {g} (({r}) != 0)) ? 1 : 0")
                    }
                    "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                        format!("(({l}) {op} ({r})) ? 1 : 0")
                    }
                    "%" if self.opts.es100 => {
                        format!("(({l}) - (({r}) * (({l}) / ({r}))))")
                    }
                    _ => format!("(({l}) {op} ({r}))"),
                }
            }
            ArithAst::Un { op, arg } => {
                let x = self.arith(arg);
                match op.as_str() {
                    "-" => format!("(-({x}))"),
                    "+" => format!("({x})"),
                    "!" => format!("((({x}) == 0) ? 1 : 0)"),
                    "~" => format!("(~({x}))"),
                    _ => {
                        self.todo += 1;
                        format!("/* TODO(un {op}) */ 0")
                    }
                }
            }
            ArithAst::Cond {
                test,
                then,
                else_,
            } => {
                let t = self.arith(test);
                let a = self.arith(then);
                let b = self.arith(else_);
                format!("((({t}) != 0) ? ({a}) : ({b}))")
            }
            ArithAst::Assign { var, op, rhs } => {
                let v = self.ident(var);
                let r = self.arith(rhs);
                match op.as_str() {
                    "=" => format!("({v} = {r})"),
                    "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" => {
                        format!("({v} {op} {r})")
                    }
                    _ => {
                        self.todo += 1;
                        format!("/* TODO(assign {op}) */ ({v})")
                    }
                }
            }
            ArithAst::IncDec {
                var,
                delta,
                prefix,
            } => {
                let v = self.ident(var);
                if *prefix {
                    if *delta > 0 {
                        format!("({v} += {delta})")
                    } else {
                        format!("({v} -= {})", -delta)
                    }
                } else if *delta > 0 {
                    format!("({v}++)")
                } else {
                    format!("({v}--)")
                }
            }
            ArithAst::Sizeof(t) => format!("({})", t.c_sizeof().unwrap_or(4)),
            ArithAst::Cast { arg, .. } => self.arith(arg),
        }
    }

    // ── test calls ──────────────────────────────────────────────────
    fn test_call(&mut self, args: &[IrExpr]) -> String {
        match args {
            [IrExpr::Str(text, _)] => self.test_text(text),
            [IrExpr::Str(text, _), IrExpr::Str(kind, _)] if kind == "[[" => self.test_text(text),
            [a, op, b] => {
                let op = match op {
                    IrExpr::Str(s, _) => s.as_str(),
                    _ => {
                        self.todo += 1;
                        return "false".to_string();
                    }
                };
                match op {
                    "=" | "==" => {
                        let l = self.expr_str(a);
                        let r = self.expr_str(b);
                        format!("strEq({l}, {r})")
                    }
                    "!=" => {
                        let l = self.expr_str(a);
                        let r = self.expr_str(b);
                        format!("(!strEq({l}, {r}))")
                    }
                    "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" => {
                        let l = self.expr_num(a);
                        let r = self.expr_num(b);
                        let g = match op {
                            "-eq" => "==",
                            "-ne" => "!=",
                            "-lt" => "<",
                            "-le" => "<=",
                            "-gt" => ">",
                            _ => ">=",
                        };
                        format!("({l} {g} {r})")
                    }
                    _ => {
                        self.todo += 1;
                        format!("/* TODO(test op {op}) */ false")
                    }
                }
            }
            _ => {
                self.todo += 1;
                "/* TODO(test args) */ false".to_string()
            }
        }
    }

    /// `[ $i -lt 3 ]` → `(g_i < 3)`. Tokens: whitespace-split, but a
    /// comparison operator glued between quoted operands
    /// (`"$x"="root"`, `"$a"=="$b"`) splits at the operator too.
    fn test_text(&mut self, text: &str) -> String {
        let toks: Vec<String> = test_tokenize(text);
        let mut i = 0;
        let mut negate = false;
        if i < toks.len() && toks[i] == "!" {
            negate = true;
            i += 1;
        }
        let refs: Vec<&str> = toks[i..].iter().map(|s| s.as_str()).collect();
        let inner = match refs.as_slice() {
            [] => {
                self.todo += 1;
                "false".to_string()
            }
            rest => self.test_tokens(rest),
        };
        if negate {
            format!("(!({inner}))")
        } else {
            inner
        }
    }

    fn test_tokens(&mut self, toks: &[&str]) -> String {
        match toks {
            [a, op, b] => {
                let op = *op;
                match op {
                    "=" | "==" => {
                        let l = self.test_operand_str(a);
                        let r = self.test_operand_str(b);
                        if b.contains('*') || b.contains('?') || b.contains('[') {
                            format!("globMatch({l}, {r})")
                        } else {
                            format!("strEq({l}, {r})")
                        }
                    }
                    "!=" => {
                        let l = self.test_operand_str(a);
                        let r = self.test_operand_str(b);
                        if b.contains('*') || b.contains('?') || b.contains('[') {
                            format!("(!globMatch({l}, {r}))")
                        } else {
                            format!("(!strEq({l}, {r}))")
                        }
                    }
                    "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" => {
                        let l = self.test_operand_num(a);
                        let r = self.test_operand_num(b);
                        let g = match op {
                            "-eq" => "==",
                            "-ne" => "!=",
                            "-lt" => "<",
                            "-le" => "<=",
                            "-gt" => ">",
                            _ => ">=",
                        };
                        format!("({l} {g} {r})")
                    }
                    _ => {
                        self.todo += 1;
                        format!("/* TODO(test op {op}) */ false")
                    }
                }
            }
            [op, operand] if *op == "-n" || *op == "-z" => {
                let s = self.test_operand_str(operand);
                if *op == "-n" {
                    format!("({s}.y > 0)")
                } else {
                    format!("({s}.y == 0)")
                }
            }
            [op, _] if op.starts_with('-') => {
                // file tests (-f/-d/-e/...) — no filesystem on the GPU
                self.todo += 1;
                format!("/* TODO(file test {op}) */ false")
            }
            [a] => {
                let s = self.test_operand_str(a);
                // numeric vars are native ints — compare != 0, never
                // swizzle an int (`(itos(g_i)).y` is a scalar swizzle
                // error under GLSL ES).
                let t = a.trim();
                let t = t
                    .strip_prefix('"')
                    .and_then(|x| x.strip_suffix('"'))
                    .unwrap_or(t);
                let t = t
                    .strip_prefix('\'')
                    .and_then(|x| x.strip_suffix('\''))
                    .unwrap_or(t);
                if let Some(name) = t.strip_prefix('$') {
                    let name = name
                        .strip_prefix('{')
                        .and_then(|x| x.strip_suffix('}'))
                        .unwrap_or(name);
                    if self.is_num(name) {
                        return format!("({} != 0)", self.ident(name));
                    }
                }
                format!("({s}.y > 0)")
            }
            _ => {
                self.todo += 1;
                "/* TODO(test shape) */ false".to_string()
            }
        }
    }

    fn test_operand_str(&mut self, tok: &str) -> String {
        let t = tok.trim();
        if let Some(inner) = t.strip_prefix("$((").and_then(|x| x.strip_suffix("))")) {
            if let Some(a) = crate::shir::parse_arith(inner.trim()) {
                return format!("itos({})", self.arith(&a));
            }
        }
        if t.contains("$(") || t.contains('`') {
            // command substitution inside a test — a subprocess, no GPU
            self.todo += 1;
            return format!("/* TODO(cmdsub in test) */ ivec2(0, 0)");
        }
        let t = t
            .strip_prefix('"')
            .and_then(|x| x.strip_suffix('"'))
            .unwrap_or(t);
        let t = t
            .strip_prefix('\'')
            .and_then(|x| x.strip_suffix('\''))
            .unwrap_or(t);
        if let Some(name) = t.strip_prefix('$') {
            let name = name
                .strip_prefix('{')
                .and_then(|x| x.strip_suffix('}'))
                .unwrap_or(name);
            if let Some(v) = self.special_var_str(name) {
                return v;
            }
            if is_positional(name) {
                return self.positional_str(name);
            }
            let Some(base) = base_var_name(name) else {
                self.todo += 1;
                return format!("/* TODO(test var {}) */ ivec2(0, 0)", sanitize(name));
            };
            if self.is_num(base) {
                return format!("itos({})", self.ident(base));
            }
            return self.ident(base);
        }
        self.strlit(t)
    }

    /// Numeric test operand: `$i` → `g_i` (native int), `3` → `(3)`,
    /// anything else → `s2i(<string>)`.
    fn test_operand_num(&mut self, tok: &str) -> String {
        let t = tok.trim();
        if let Some(inner) = t.strip_prefix("$((").and_then(|x| x.strip_suffix("))")) {
            if let Some(a) = crate::shir::parse_arith(inner.trim()) {
                return self.arith(&a);
            }
        }
        if t.contains("$(") || t.contains('`') {
            self.todo += 1;
            return format!("/* TODO(cmdsub in test) */ 0");
        }
        let t = t
            .strip_prefix('"')
            .and_then(|x| x.strip_suffix('"'))
            .unwrap_or(t);
        let t = t
            .strip_prefix('\'')
            .and_then(|x| x.strip_suffix('\''))
            .unwrap_or(t);
        if let Some(name) = t.strip_prefix('$') {
            let name = name
                .strip_prefix('{')
                .and_then(|x| x.strip_suffix('}'))
                .unwrap_or(name);
            if let Some(v) = self.special_var_num(name) {
                return v;
            }
            if is_positional(name) {
                return self.positional_num(name);
            }
            let Some(base) = base_var_name(name) else {
                self.todo += 1;
                return format!("/* TODO(test var {}) */ 0", sanitize(name));
            };
            if self.is_num(base) {
                return self.ident(base);
            }
            return format!("s2i({})", self.ident(base));
        }
        if let Ok(n) = t.parse::<i64>() {
            if n >= i32::MIN as i64 && n <= i32::MAX as i64 {
                return format!("({n})");
            }
            self.todo += 1;
        }
        format!("s2i({})", self.strlit(t))
    }

    // ── bc capture lowering (echo EXPR | bc) ────────────────────────
    // GNU bc is EXACT decimal fixed-point (v/10^scale, truncation, the
    // `.5`/`3.00` output format — src/bc.rs is the reference, 77/77 vs
    // real bc). A float mapping would diverge: fp32 has a 24-bit
    // mantissa and rounds, bc truncates to the exact decimal digits
    // (`.33` is NOT 0.33333334; `1/6` at scale 2 is `.16`, while
    // %.2f of the double rounds to `.17`). So the GLSL lowering keeps
    // bc on INTEGERS: static programs fold at render time through
    // crate::bc::eval (exact), the `sqrt($var)` shape becomes an
    // integer isqrt, and var-operand arithmetic renders as scale-0
    // integer ops (the estree path's documented 2^53-integer operand
    // assumption, tightened to i32 here).

    /// `echo EXPR | bc` — returns the echo args, or None if the pipeline
    /// is not that shape (mirrors the estree path's detector).
    fn pipeline_echo_bc(&mut self, pipe: &IrExpr) -> Option<Vec<IrExpr>> {
        let IrExpr::Call { func, args } = pipe else { return None };
        if func != "pipeline" {
            return None;
        }
        let [IrExpr::Array(stages)] = args.as_slice() else { return None };
        if stages.len() != 2 {
            return None;
        }
        let [IrExpr::Arrow(s1), IrExpr::Arrow(s2)] = stages.as_slice() else { return None };
        let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = s1.as_slice() else {
            return None;
        };
        if !matches!(f1.as_str(), "exec" | "builtin") {
            return None;
        }
        let [IrExpr::Str(name1, _), IrExpr::Array(echo_args)] = a1.as_slice() else {
            return None;
        };
        if name1 != "echo" {
            return None;
        }
        let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = s2.as_slice() else {
            return None;
        };
        if f2 != "exec" {
            return None;
        }
        let [IrExpr::Str(name2, _), IrExpr::Array(bc_args)] = a2.as_slice() else {
            return None;
        };
        if name2 != "bc" || !bc_args.is_empty() {
            return None;
        }
        Some(echo_args.clone())
    }

    /// The single real echo arg (flags `-e`/`-n` skipped; multiple args
    /// would join into a multi-statement program — keep the spawn).
    /// Returns (arg, no_newline).
    fn bc_single_arg(&mut self, echo_args: &[IrExpr]) -> Option<(IrExpr, bool)> {
        let mut no_newline = false;
        let mut arg: Option<IrExpr> = None;
        for a in echo_args {
            if arg.is_none() {
                if let IrExpr::Str(sv, _) = a {
                    if sv == "-n" {
                        no_newline = true;
                        continue;
                    }
                    if sv == "-e" {
                        continue;
                    }
                }
            }
            if arg.is_some() {
                return None;
            }
            arg = Some(a.clone());
        }
        arg.map(|a| (a, no_newline))
    }

    /// The bc program text when the arg is a Str or an all-Lit
    /// Interpolate (the quoted form — expandWord joins the parts).
    fn bc_static_text(&mut self, arg: &IrExpr) -> Option<String> {
        match arg {
            IrExpr::Str(sv, _) => Some(sv.clone()),
            IrExpr::Interpolate(parts)
                if parts.iter().all(|p| matches!(p, InterpPart::Lit(_))) =>
            {
                Some(
                    parts
                        .iter()
                        .map(|p| match p {
                            InterpPart::Lit(s) => s.clone(),
                            _ => unreachable!("all-Lit checked"),
                        })
                        .collect(),
                )
            }
            _ => None,
        }
    }

    /// Unwrap a `capture(...)` call's args to the wrapped pipeline expr
    /// (`Arrow([Expr(pipeline)])` / `Array([Arrow([Expr(pipeline)])])`).
    fn capture_pipeline<'a>(&self, args: &'a [IrExpr]) -> Option<&'a IrExpr> {
        match args.first() {
            Some(IrExpr::Arrow(body)) => match body.as_slice() {
                [IrStmt::Expr(pipe)] => Some(pipe),
                _ => None,
            },
            Some(IrExpr::Array(items)) => match items.as_slice() {
                [IrExpr::Arrow(body)] => match body.as_slice() {
                    [IrStmt::Expr(pipe)] => Some(pipe),
                    _ => None,
                },
                _ => None,
            },
            Some(pipe @ IrExpr::Call { func, .. }) if func == "pipeline" => Some(pipe),
            _ => None,
        }
    }

    /// Statement form `echo EXPR | bc` — emit the exact bc stdout.
    fn bc_statement(&mut self, pipe: &IrExpr) -> bool {
        let Some(echo_args) = self.pipeline_echo_bc(pipe) else {
            return false;
        };
        let Some((arg, no_newline)) = self.bc_single_arg(&echo_args) else {
            return false;
        };
        if let Some(text) = self.bc_static_text(&arg) {
            return match crate::bc::eval(&text) {
                Ok(out) => {
                    // bc's stdout: statements joined by \n, echo | bc
                    // emits each line (GNU bc adds the final newline)
                    let out = if out.is_empty() {
                        out
                    } else if no_newline {
                        format!("{out}")
                    } else {
                        format!("{out}\n")
                    };
                    let l = self.strlit(&out);
                    self.emit(&format!("putStr({l});"));
                    true
                }
                Err(_) => {
                    // bc errors print nothing, exit nonzero
                    self.emit("/* bc: no stdout (error) */");
                    true
                }
            };
        }
        if let Some(glsl) = self.bc_dynamic(&arg) {
            self.emit(&format!("putStr(itos({glsl}));"));
            if !no_newline {
                self.emit("putCh(10);");
            }
            return true;
        }
        false
    }

    /// Capture form `$(echo EXPR | bc)` — the GLSL string expression
    /// (capture strips the trailing newline).
    fn bc_capture(&mut self, pipe: &IrExpr) -> Option<String> {
        let echo_args = self.pipeline_echo_bc(pipe)?;
        let (arg, _no_newline) = self.bc_single_arg(&echo_args)?;
        if let Some(text) = self.bc_static_text(&arg) {
            return match crate::bc::eval(&text) {
                Ok(out) => Some(self.strlit(out.trim_end_matches('\n'))),
                Err(_) => Some(self.strlit("")), // bc error → empty capture
            };
        }
        // dynamic: the int glsl expression wrapped as a string
        self.bc_dynamic(&arg).map(|g| format!("itos({g})"))
    }

    /// Dynamic forms: `sqrt($var)` (integer isqrt) and var-operand
    /// scale-0 integer arithmetic (`$sum + $i`, …).
    fn bc_dynamic(&mut self, arg: &IrExpr) -> Option<String> {
        let IrExpr::Interpolate(parts) = arg else { return None };
        if let [InterpPart::Lit(l1), InterpPart::Expr(inner), InterpPart::Lit(l2)] =
            parts.as_slice()
        {
            if l1.trim_end() == "sqrt(" && l2.trim_start() == ")" {
                let n = self.expr_num(inner);
                self.used_isqrt = true;
                return Some(format!(
                    "(({n}) >= 0 ? isqrt32({n}) : /* TODO(bc neg sqrt) */ 0)"
                ));
            }
        }
        // var-operand integer arithmetic: every Expr slot is an operand,
        // the literal text must be bc's scale-0 integer grammar.
        let mut src = String::new();
        let mut slots: Vec<&IrExpr> = Vec::new();
        for p in parts {
            match p {
                InterpPart::Lit(s) => {
                    if !s.chars().all(|c| {
                        c.is_ascii_digit()
                            || c.is_whitespace()
                            || matches!(c, '+' | '-' | '*' | '/' | '%' | '(' | ')')
                    }) {
                        return None;
                    }
                    src.push_str(s);
                }
                InterpPart::Expr(e) => {
                    slots.push(&e);
                    src.push_str(&format!("__bcv{}", slots.len() - 1));
                }
            }
        }
        if slots.is_empty() {
            return None;
        }
        let ast = crate::shir::parse_arith(&src)?;
        let glsl = self.bc_arith(&ast, &slots);
        let mut divs = Vec::new();
        self.bc_divisors(&ast, &slots, &mut divs);
        if divs.is_empty() {
            return Some(glsl);
        }
        // bc aborts the whole program (no stdout) on ANY zero divisor
        let guard = divs
            .iter()
            .map(|d| format!("({d} == 0)"))
            .collect::<Vec<_>>()
            .join(" || ");
        Some(format!(
            "(({guard}) ? /* TODO(bc div-by-zero) */ 0 : ({glsl}))"
        ))
    }

    // ── float bc captures ──────────────────────────────────────────
    // `v=$(echo "scale=4; $x * 0.5" | bc)` — the bash shader authors
    // FLOAT math with bc; the GLSL lowering emits the exact float
    // expression (fp32 — bc's exact decimal truncation vs float
    // rounding is fine for a visual shader). The var becomes a GLSL
    // float (see float_vars); reads in int contexts cast via int().
    // ArithAst's Num is i64-only, so a tiny precedence parser handles
    // the float grammar directly (numbers may carry a decimal point).
    //
    // Pass-1 twin: [`Self::is_float_bc_capture`] detects the shape
    // during collect (without emitting) so the main() locals — which
    // are declared BEFORE the body renders — know the var is a float.
    fn bc_float_expr(&mut self, pipe: &IrExpr) -> Option<String> {
        // unwrap the cmdsub wrapper: `$(…)` arrives as Capture/capture
        let pipe = match pipe {
            IrExpr::Capture { expr, .. } => {
                // the first-class Capture node (core request
                // zsh-sh-go-20260814-230503) — unwrap the Arrow like the
                // Call-capture arm below (capture_pipeline does the
                // Arrow → pipeline unwrap).
                let Some(p) = self.capture_pipeline(std::slice::from_ref(expr.as_ref())) else {
                    return None;
                };
                p
            }
            IrExpr::Call { func, args } if func == "capture" || func == "captureWords" => {
                self.capture_pipeline(args)?
            }
            other => other,
        };
        let echo_args = self.pipeline_echo_bc(pipe)?;
        let (arg, _no_newline) = self.bc_single_arg(&echo_args)?;
        let IrExpr::Interpolate(parts) = &arg else { return None };
        let mut src = String::new();
        let mut slots: Vec<&IrExpr> = Vec::new();
        let mut has_float = false;
        for p in parts {
            match p {
                InterpPart::Lit(t) => {
                    let mut t = t.as_str();
                    // strip a leading `scale=K;` / `scale=K ` statement
                    if src.is_empty() {
                        if let Some(rest) = t.strip_prefix("scale=") {
                            let mut it = rest.splitn(2, |c| c == ';' || c == ' ');
                            if let Some(_k) = it.next() {
                                if let Some(rest2) = it.next() {
                                    t = rest2;
                                }
                            }
                        }
                    }
                    if t.contains('.') {
                        has_float = true;
                    }
                    // `c(` / `s(` = the bc trig functions (cos/sin —
                    // GNU bc's c()/s(), the vertex shader's camera
                    // rotation); anything else alphabetic is rejected
                    // so a typo fails loudly (TODO) instead of parsing
                    // silently wrong.
                    if !t.chars().all(|c| {
                        c.is_ascii_digit()
                            || c.is_whitespace()
                            || matches!(
                                c,
                                '+' | '-' | '*' | '/' | '%' | '(' | ')' | '.' | '^' | 'c' | 's'
                            )
                    }) {
                        return None;
                    }
                    src.push_str(t);
                }
                InterpPart::Expr(e) => {
                    slots.push(e);
                    src.push_str(&format!("__bcv{}", slots.len() - 1));
                }
            }
        }
        if !has_float || slots.is_empty() {
            return None;
        }
        Some(self.parse_float_expr(&src, &slots))
    }

    /// Pass-1 twin of [`Self::bc_float_expr`]: does `pipe` have the
    /// float-bc-capture shape (`echo "scale=K; …0.5" | bc` with a
    /// decimal literal AND at least one var slot)? Mirrors the
    /// unwrap/scale-strip/char-filter of bc_float_expr without emitting
    /// (it only pattern-matches and clones — safe during collect).
    fn is_float_bc_capture(&mut self, pipe: &IrExpr) -> bool {
        let pipe = match pipe {
            IrExpr::Capture { expr, .. } => {
                // the first-class Capture node (core request
                // zsh-sh-go-20260814-230503) — unwrap the Arrow like the
                // Call-capture arm below.
                let Some(p) = self.capture_pipeline(std::slice::from_ref(expr.as_ref())) else {
                    return false;
                };
                p
            }
            IrExpr::Call { func, args }
                if func == "capture" || func == "captureWords" =>
            {
                let Some(p) = self.capture_pipeline(args) else { return false };
                p
            }
            other => other,
        };
        let Some(echo_args) = self.pipeline_echo_bc(pipe) else { return false };
        let Some((arg, _no_newline)) = self.bc_single_arg(&echo_args) else {
            return false;
        };
        let IrExpr::Interpolate(parts) = &arg else { return false };
        let mut has_float = false;
        let mut slots = 0;
        for p in parts {
            match p {
                InterpPart::Lit(t) => {
                    let mut t = t.as_str();
                    if t.contains('.') {
                        has_float = true;
                    }
                    // strip a leading `scale=K;` / `scale=K ` (the same
                    // strip bc_float_expr applies before the filter)
                    if t.starts_with("scale=") {
                        if let Some(rest) = t.strip_prefix("scale=") {
                            let mut it = rest.splitn(2, |c| c == ';' || c == ' ');
                            if let Some(_k) = it.next() {
                                if let Some(rest2) = it.next() {
                                    t = rest2;
                                }
                            }
                        }
                    }
                    if !t.chars().all(|c| {
                        c.is_ascii_digit()
                            || c.is_whitespace()
                            || matches!(
                                c,
                                '+' | '-' | '*' | '/' | '%' | '(' | ')' | '.' | '^' | 'c' | 's'
                            )
                    }) {
                        return false;
                    }
                }
                InterpPart::Expr(_) => slots += 1,
            }
        }
        has_float && slots > 0
    }

    /// The var name when `e` is a direct variable reference in any of
    /// the shapes the ShIR uses (Var/Ident nodes and `getVar("name")`
    /// calls — the interpolated `$rad` in a bc capture is a getVar).
    fn var_name_of<'a>(&self, e: &'a IrExpr) -> Option<&'a str> {
        match e {
            IrExpr::Var(n, _) | IrExpr::Ident(n) => Some(n),
            IrExpr::Call { func, args } if func == "getVar" => match args.first() {
                Some(IrExpr::Str(n, _)) => Some(n.as_str()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Tokenize + precedence-climb `src` (numbers, `__bcvK`, + - * / % ^
    /// and parens) into a GLSL float expression. `^` right-assoc (bc);
    /// unary minus binds tighter than `^` (`-2^2` → 4, so `^` here is
    /// left-recursive and the unary applies first — bc parity). The bc
    /// trig functions `c(…)`/`s(…)` become the GLSL `cos`/`sin`
    /// built-ins (same fp32 semantics in the browser).
    fn parse_float_expr(&mut self, src: &str, slots: &[&IrExpr]) -> String {
        let toks = self.lex_float(src);
        let (v, rest) = self.float_prec(&toks, 0, 0, slots);
        let _ = rest;
        v
    }

    fn lex_float(&mut self, src: &str) -> Vec<(String, usize)> {
        let mut out = Vec::new();
        let mut i = 0;
        let b = src.as_bytes();
        while i < b.len() {
            let c = b[i] as char;
            if c.is_whitespace() {
                i += 1;
                continue;
            }
            if c.is_ascii_digit() || c == '.' {
                let mut j = i;
                let mut dot = false;
                while j < b.len() {
                    let ch = b[j] as char;
                    if ch.is_ascii_digit() {
                        j += 1;
                    } else if ch == '.' && !dot {
                        dot = true;
                        j += 1;
                    } else {
                        break;
                    }
                }
                out.push((src[i..j].to_string(), i));
                i = j;
                continue;
            }
            if src[i..].starts_with("__bcv") {
                let mut j = i + 5;
                while j < b.len() && (b[j] as char).is_ascii_digit() {
                    j += 1;
                }
                out.push((src[i..j].to_string(), i));
                i = j;
                continue;
            }
            // bc trig: `c(` / `s(` — cos/sin (only when followed by a
            // paren, so `c`/`s` cannot appear as bare unknowns).
            if (c == 'c' || c == 's') && i + 1 < b.len() && b[i + 1] as char == '(' {
                out.push((format!("{c}("), i));
                i += 2;
                continue;
            }
            if matches!(c, '+' | '-' | '*' | '/' | '%' | '(' | ')') {
                out.push((c.to_string(), i));
                i += 1;
                continue;
            }
            if c == '^' {
                out.push(("^".to_string(), i));
                i += 1;
                continue;
            }
            i += 1; // unknown char — skip (bc would error too)
        }
        out
    }

    /// Precedence-climbing: returns (glsl, next-token-index).
    fn float_prec(
        &mut self,
        toks: &[(String, usize)],
        start: usize,
        min_prec: u8,
        slots: &[&IrExpr],
    ) -> (String, usize) {
        let (mut lhs, mut idx) = self.float_unary(toks, start, slots);
        while idx < toks.len() {
            let (op, _) = &toks[idx];
            let (p, right_assoc) = match op.as_str() {
                "+" | "-" => (1, false),
                "*" | "/" | "%" => (2, false),
                "^" => (3, true),
                _ => break,
            };
            if p < min_prec {
                break;
            }
            idx += 1;
            let (rhs, ni) =
                self.float_prec(toks, idx, p + if right_assoc { 0 } else { 1 }, slots);
            idx = ni;
            let r = match op.as_str() {
                "^" => format!("pow({lhs}, {rhs})"),
                "%" => format!("mod({lhs}, {rhs})"),
                _ => format!("(({lhs}) {op} ({rhs}))"),
            };
            lhs = r;
        }
        (lhs, idx)
    }

    fn float_unary(
        &mut self,
        toks: &[(String, usize)],
        idx: usize,
        slots: &[&IrExpr],
    ) -> (String, usize) {
        if idx >= toks.len() {
            return ("0.0".to_string(), idx);
        }
        let (t, _) = &toks[idx];
        match t.as_str() {
            "c(" | "s(" => {
                // bc trig → GLSL cos/sin: parse the parenthesised
                // argument, emit the built-in (fp32 in the browser,
                // same as the hand-written shader's cos()/sin()).
                let (v, ni) = self.float_prec(toks, idx + 1, 0, slots);
                let name = if t == "c(" { "cos" } else { "sin" };
                if ni < toks.len() && toks[ni].0 == ")" {
                    (format!("{name}({v})"), ni + 1)
                } else {
                    (format!("{name}({v})"), ni)
                }
            }
            "-" | "+" => {
                let (v, ni) = self.float_unary(toks, idx + 1, slots);
                (format!("({}{v})", if t == "-" { "-" } else { "" }), ni)
            }
            "(" => {
                let (v, ni) = self.float_prec(toks, idx + 1, 0, slots);
                if ni < toks.len() && toks[ni].0 == ")" {
                    (format!("({v})"), ni + 1)
                } else {
                    (v, ni)
                }
            }
            _ => {
                if t.starts_with("__bcv") {
                    let k: usize = t[5..].parse().unwrap_or(0);
                    let e = slots.get(k).copied().unwrap_or(&IrExpr::Int(0));
                    // a float var slot is emitted DIRECTLY — a
                    // float(int()) round-trip would truncate the value
                    // to an integer and break chained float math
                    // (rad → c/s → rel → NDC).
                    match self.var_name_of(e) {
                        Some(n) if self.float_vars.contains(n) => {
                            (self.ident(n), idx + 1)
                        }
                        _ => (format!("float({})", self.expr_num(e)), idx + 1),
                    }
                } else {
                    (t.clone(), idx + 1)
                }
            }
        }
    }

    /// ArithAst → GLSL int, mapping `__bcvK` placeholders to the slot
    /// expressions. parse_arith over a bc_var_lit_ok source can only
    /// produce Num/Var/Bin/Un/Cond — the rest delegate to [`Self::arith`].
    fn bc_arith(&mut self, a: &ArithAst, slots: &[&IrExpr]) -> String {
        match a {
            ArithAst::Var(n) if n.starts_with("__bcv") => {
                let idx: usize = n[5..].parse().unwrap_or(0);
                let e = slots.get(idx).copied().unwrap_or(&IrExpr::Int(0));
                self.expr_num(e)
            }
            ArithAst::Var(n) => {
                if self.is_num(n) {
                    self.ident(n)
                } else {
                    format!("s2i({})", self.ident(n))
                }
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let l = self.bc_arith(lhs, slots);
                let r = self.bc_arith(rhs, slots);
                match op.as_str() {
                    "**" => format!("ipow({l}, {r})"),
                    "&&" | "||" => {
                        let g = if op == "&&" { "&&" } else { "||" };
                        format!("((({l}) != 0) {g} (({r}) != 0)) ? 1 : 0")
                    }
                    "==" | "!=" | "<" | "<=" | ">" | ">=" => {
                        format!("(({l}) {op} ({r})) ? 1 : 0")
                    }
                    "%" if self.opts.es100 => {
                        format!("(({l}) - (({r}) * (({l}) / ({r}))))")
                    }
                    _ => format!("(({l}) {op} ({r}))"),
                }
            }
            ArithAst::Un { op, arg } => {
                let x = self.bc_arith(arg, slots);
                match op.as_str() {
                    "-" => format!("(-({x}))"),
                    "+" => format!("({x})"),
                    "!" => format!("((({x}) == 0) ? 1 : 0)"),
                    "~" => format!("(~({x}))"),
                    _ => {
                        self.todo += 1;
                        format!("/* TODO(un {op}) */ 0")
                    }
                }
            }
            ArithAst::Cond {
                test,
                then,
                else_,
            } => {
                let t = self.bc_arith(test, slots);
                let a = self.bc_arith(then, slots);
                let b = self.bc_arith(else_, slots);
                format!("((({t}) != 0) ? ({a}) : ({b}))")
            }
            other => self.arith(other),
        }
    }

    /// Collect the GLSL divisor expressions of every `/`/`%` node (for
    /// the bc zero-divisor abort guard).
    fn bc_divisors(&mut self, a: &ArithAst, slots: &[&IrExpr], out: &mut Vec<String>) {
        match a {
            ArithAst::Bin { op, lhs, rhs } => {
                if op == "/" || op == "%" {
                    out.push(self.bc_arith(rhs, slots));
                }
                self.bc_divisors(lhs, slots, out);
                self.bc_divisors(rhs, slots, out);
            }
            ArithAst::Un { arg, .. } => self.bc_divisors(arg, slots, out),
            ArithAst::Cond {
                test, then, else_,
            } => {
                self.bc_divisors(test, slots, out);
                self.bc_divisors(then, slots, out);
                self.bc_divisors(else_, slots, out);
            }
            _ => {}
        }
    }

    // ── statements ──────────────────────────────────────────────────
    fn stmt(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Expr(e) => self.expr_stmt(e),
            IrStmt::Ext(_) => panic!("glsl backend: Ext node unsupported"),
            IrStmt::Assign { targets, expr, asm, .. } => {
                // Declarator-position asm label (core request
                // c-sh-go-toplevelasmargument-20260814-042952) — no GLSL
                // rendering; refuse loudly (refuse > guess).
                if let Some(spec) = asm {
                    self.emit(&format!(
                        "// TODO(unsupported): asm label '{}' on an assign",
                        spec.template
                    ));
                    return;
                }
                // `arr=(...)` / `arr+=(...)` may surface as Assign with a
                // setArray/setArrayAppend call value — lower like
                // DeclareArray (append is a sketch TODO, still compiles).
                if let IrExpr::Call { func, args } = expr {
                    if (func == "setArray" || func == "setArrayAppend")
                        && targets.len() == 1
                        && targets[0].indices.is_empty()
                    {
                        if let Some(IrExpr::Str(nm, _)) = args.first() {
                            if nm == &targets[0].var {
                                if let Some(IrExpr::Array(elements)) = args.get(1) {
                                    self.declare_array(&targets[0].var, elements);
                                    if func == "setArrayAppend" {
                                        self.mark_todo("array append");
                                    }
                                    return;
                                }
                            }
                        }
                    }
                }
                // an Assign targeting an ARRAY with a non-array value
                // (`z+=${arr[@]:0:1}`) — element/whole-array write is
                // not modeled; keep the shader compiling.
                if targets.iter().any(|t| t.indices.is_empty() && self.arrays.contains(&t.var)) {
                    for t in targets {
                        if t.indices.is_empty() && self.arrays.contains(&t.var) {
                            self.mark_todo(&format!("array assign {}", t.var));
                        }
                    }
                    return;
                }
                // `y+=2` → Assign { targets: [y], expr: assign("y","+=","2") }
                // (numeric vars only — bash `+=` on a plain var is a
                // STRING append; a Str-typed target stays a TODO below)
                if let IrExpr::Call { func, args } = expr {
                    if func == "assign"
                        && args.len() >= 3
                        && targets.len() == 1
                        && targets[0].indices.is_empty()
                        && self.is_num(&targets[0].var)
                    {
                        if let Some(IrExpr::Str(nm, _)) = args.first() {
                            if nm == &targets[0].var {
                                if let Some(IrExpr::Str(op, _)) = args.get(1) {
                                    if matches!(op.as_str(), "=" | "+=" | "-=" | "*=" | "/=" | "%=") {
                                        let v = self.ident(nm);
                                        let r = self.expr_num(&args[2]);
                                        self.emit(&format!("{v} {op} {r};"));
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
                // a float bc capture (`v=$(echo "scale=K; …0.5" | bc)`)
                // → the var becomes a GLSL float holding the expression
                if let Some(glsl) = self.bc_float_expr(expr) {
                    if targets.len() == 1 && targets[0].indices.is_empty() {
                        self.float_vars.insert(targets[0].var.clone());
                        self.emit(&format!("{} = {glsl};", self.ident(&targets[0].var)));
                        return;
                    }
                }
                for t in targets {
                    if t.indices.is_empty() {
                        let v = self.ident(&t.var);
                        // a float var assigned from a float var (`vp_x=$wx`
                        // in the vertex shader) is a direct float copy —
                        // expr_num would truncate through int().
                        if self.float_vars.contains(&t.var) {
                            if let Some(n) = self.var_name_of(expr) {
                                if self.float_vars.contains(n) {
                                    self.emit(&format!("{v} = {};", self.ident(n)));
                                    return;
                                }
                            }
                        }
                        let val = if self.is_num(&t.var) {
                            self.expr_num(expr)
                        } else {
                            // fresh-copy materialization: string values
                            // from scratch are stabilized on assignment
                            format!("cat({}, ivec2(0, 0))", self.expr_str(expr))
                        };
                        self.emit(&format!("{v} = {val};"));
                    } else {
                        // indexed target
                        let keys: Vec<String> =
                            t.indices.iter().map(|k| self.expr_num(k)).collect();
                        for k in keys {
                            let v = self.ident(&t.var);
                            let val = if self.is_num(&t.var) {
                                self.expr_num(expr)
                            } else {
                                format!("cat({}, ivec2(0, 0))", self.expr_str(expr))
                            };
                            self.emit(&format!("{v}[{k}] = {val};"));
                        }
                    }
                }
            }
            IrStmt::Declare { vars, init, .. } => {
                if let Some(i) = init {
                    let expr = i;
                    for Decl { name, .. } in vars {
                        let v = self.ident(name);
                        let val = if self.is_num(name) {
                            self.expr_num(expr)
                        } else {
                            format!("cat({}, ivec2(0, 0))", self.expr_str(expr))
                        };
                        self.emit(&format!("{v} = {val};"));
                    }
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                self.declare_array(var, elements);
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                let c = self.expr_bool(cond);
                self.emit(&format!("if ({c}) {{"));
                self.depth += 1;
                self.emit_lazy_tex_seeds(then.as_slice());
                for s in then {
                    self.stmt(s);
                }
                self.depth -= 1;
                for (e, b) in elsifs {
                    let c = self.expr_bool(e);
                    self.emit(&format!("}} else if ({c}) {{"));
                    self.depth += 1;
                    self.emit_lazy_tex_seeds(b.as_slice());
                    for s in b {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                }
                if !else_.is_empty() {
                    self.emit("} else {");
                    self.depth += 1;
                    self.emit_lazy_tex_seeds(else_.as_slice());
                    for s in else_ {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                }
                self.emit("}");
            }
            IrStmt::For { var, iter, body } => {
                self.for_loop(var, iter, body);
            }
            IrStmt::ForInit {
                init,
                cond,
                step,
                body,
            } => {
                self.emit("{");
                self.depth += 1;
                for s in init {
                    self.stmt(s);
                }
                let c = self.expr_bool(cond);
                self.emit(&format!("while ({c}) {{"));
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                for s in step {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::While { cond, body } => {
                let c = self.expr_bool(cond);
                self.emit(&format!("while ({c}) {{"));
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::DoWhile { body, cond, .. } => {
                self.emit("do {");
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                let c = self.expr_bool(cond);
                self.emit(&format!("}} while ({c});"));
            }
            IrStmt::Continue => self.emit("continue;"),
            IrStmt::Break => self.emit("break;"),
            IrStmt::Block(body) => {
                self.emit("{");
                self.depth += 1;
                self.emit_lazy_tex_seeds(body.as_slice());
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                self.case(discriminant, clauses);
            }
            IrStmt::Function { name, .. } => {
                self.emit(&format!("/* function {} — defined above */", self.ident(name)));
            }
            IrStmt::Return(Some(_)) | IrStmt::Return(None) => {
                if self.in_fn {
                    self.emit("return;");
                } else {
                    self.mark_todo("top-level return");
                }
            }
            IrStmt::Exit(None) => self.emit("/* exit 0 */"),
            IrStmt::Exit(Some(_)) => {
                self.mark_todo("exit");
                self.emit("discard;");
            }
            IrStmt::Redirect { .. } => self.mark_todo("redirect"),
            IrStmt::Subshell(_) => self.mark_todo("subshell"),
            IrStmt::Background(_) => self.mark_todo("background"),
            IrStmt::Exec { .. } => self.mark_todo("exec"),
            IrStmt::Pipeline { .. } => self.mark_todo("pipeline"),
            IrStmt::WriteFile { .. } => self.mark_todo("write-file"),
            IrStmt::Die { .. } => self.mark_todo("die"),
            IrStmt::Warn { .. } => self.mark_todo("warn"),
            IrStmt::Try { .. } => self.mark_todo("try"),
            IrStmt::Select { .. } => self.mark_todo("select"),
            IrStmt::Asm { .. } => self.mark_todo("asm"),
            IrStmt::Output {
                value,
                newline,
                target: _,
            } => {
                let v = self.expr_str(value);
                if *newline {
                    self.emit(&format!("putStrLn({v});"));
                } else {
                    self.emit(&format!("putStr({v});"));
                }
            }
            IrStmt::Require(_) => self.mark_todo("require"),
            IrStmt::SetChildError(_) => self.mark_todo("set-child-error"),
            IrStmt::Label(_) | IrStmt::Goto(_) => self.mark_todo("goto"),
            IrStmt::RawText(_) => self.mark_todo("raw-text"),
        }
    }

    /// `arr=(a b c)` — set element count + values (native int/ivec2
    /// array globals).
    fn declare_array(&mut self, var: &str, elements: &[IrExpr]) {
        let v = self.ident(var);
        self.emit(&format!("{}_n = {};", v, elements.len()));
        for (i, e) in elements.iter().enumerate() {
            let val = if self.is_num(var) {
                self.expr_num(e)
            } else {
                format!("cat({}, ivec2(0, 0))", self.expr_str(e))
            };
            self.emit(&format!("{v}[{i}] = {val};"));
        }
    }

    fn expr_stmt(&mut self, e: &IrExpr) {
        match e {
            // `putb N` — emit one byte into out_buf (the render-mode
            // colour channel). GLSL has no char type; N is an int 0-255.
            IrExpr::Call { func, args } if func == "putb" => {
                self.used_putb = true;
                if let Some(n) = args.first() {
                    let v = self.expr_num(n);
                    if self.opts.es100 {
                        // ES 1.00: a fixed const index (the runtime's
                        // out_buf[out_len++] is dynamic — not allowed)
                        self.emit(&format!("out_buf[{}] = {v};", self.putb_pos));
                        self.putb_pos += 1;
                    } else {
                        self.emit(&format!("putCh({v});"));
                    }
                } else {
                    self.mark_todo("putb no arg");
                }
            }
            IrExpr::Call { func, args } if func == "exec" => self.exec_call(args),
            IrExpr::Call { func, args } if func == "pipeline" => {
                let _ = args;
                if !self.bc_statement(e) {
                    self.mark_todo("pipeline");
                }
            }
            IrExpr::Call { func, args } if func == "setVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    let Some(base) = base_var_name(name) else {
                        self.mark_todo(&format!("setVar {}", sanitize(name)));
                        return;
                    };
                    if args.len() >= 2 {
                        // a float bc capture (`v=$(echo "scale=K; …0.5" | bc)`)
                        // → the var becomes a GLSL float holding the expr
                        if let Some(glsl) = self.bc_float_expr(&args[1]) {
                            self.float_vars.insert(base.to_string());
                            self.emit(&format!("{} = {glsl};", self.ident(base)));
                            return;
                        }
                        let v = self.ident(base);
                        // a float var assigned from a float var is a
                        // direct float copy (expr_num truncates to int)
                        if self.float_vars.contains(base) {
                            if let Some(n) = self.var_name_of(&args[1]) {
                                if self.float_vars.contains(n) {
                                    self.emit(&format!("{v} = {};", self.ident(n)));
                                    return;
                                }
                            }
                        }
                        let val = if self.is_num(base) {
                            self.expr_num(&args[1])
                        } else {
                            format!("cat({}, ivec2(0, 0))", self.expr_str(&args[1]))
                        };
                        self.emit(&format!("{v} = {val};"));
                    } else {
                        self.mark_todo("setVar with no value");
                    }
                } else {
                    self.mark_todo("setVar non-literal name");
                }
            }
            IrExpr::Call { func, args } if func == "assign" => {
                // assign("name", value) — runtime-shaped assignment;
                // assign("name", "+=", n) — compound (num vars).
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if args.len() >= 3 {
                        if let Some(IrExpr::Str(op, _)) = args.get(1) {
                            let v = self.ident(name);
                            let r = self.expr_num(&args[2]);
                            let g = match op.as_str() {
                                "=" => "=",
                                "+=" => "+=",
                                "-=" => "-=",
                                "*=" => "*=",
                                "/=" => "/=",
                                "%=" => "%=",
                                _ => {
                                    self.mark_todo(&format!("assign op {op}"));
                                    return;
                                }
                            };
                            self.emit(&format!("{v} {g} {r};"));
                        } else {
                            self.mark_todo("assign compound non-literal op");
                        }
                    } else {
                        self.expr_stmt(&IrExpr::Call {
                            func: "setVar".to_string(),
                            args: args.clone(),
                        });
                    }
                } else {
                    self.mark_todo("assign non-literal name");
                }
            }
            IrExpr::Call { func, args } if func == "test" => {
                let _ = args;
                let c = self.expr_bool(e);
                self.emit(&format!("({c});"));
            }
            IrExpr::Call { func, .. } if func == "true" || func == ":" => {
                self.emit("/* true */");
            }
            IrExpr::Call { func, args } if func == "arith" => {
                let _ = args;
                let n = self.expr_num(e);
                self.emit(&format!("{n};"));
            }
            IrExpr::Call { func, .. } => {
                self.mark_todo(&format!("call {func}"));
            }
            _ => {
                let v = self.expr_str(e);
                self.emit(&format!("({v});"));
            }
        }
    }

    /// exec("echo", [..]) / exec("printf", ..) / exec("local", ..) /
    /// exec("<user fn>", [args]) — everything else is TODO.
    fn exec_call(&mut self, args: &[IrExpr]) {
        let Some(IrExpr::Str(cmd, _)) = args.first() else {
            self.mark_todo("exec non-literal cmd");
            return;
        };
        match cmd.as_str() {
            "echo" | "print" => {
                let items = self.exec_items(&args[1..]);
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.emit("putCh(32);");
                    }
                    let v = self.expr_str(item);
                    self.emit(&format!("putStr({v});"));
                }
                self.emit("putCh(10);");
            }
            "printf" => self.printf(&args[1..]),
            // `putb N` — the render-mode byte output (out_buf channel)
            "putb" => {
                let items = self.exec_items(&args[1..]);
                if items.len() == 1 {
                    // unquoted `$var` arrives wrapped in split(...) —
                    // the value is already an int here, unwrap it
                    let item = match &items[0] {
                        IrExpr::Call { func, args } if func == "split" => {
                            args.first().cloned().unwrap_or_else(|| items[0].clone())
                        }
                        _ => items[0].clone(),
                    };
                    let v = self.expr_num(&item);
                    self.used_putb = true;
                    if self.opts.es100 {
                        // ES 1.00: a fixed const index (the runtime's
                        // out_buf[out_len++] is dynamic — not allowed)
                        self.emit(&format!("out_buf[{}] = {v};", self.putb_pos));
                        self.putb_pos += 1;
                    } else {
                        self.emit(&format!("putCh({v});"));
                    }
                } else {
                    self.mark_todo("putb arity");
                }
            }
            "local" => self.local_decl(&args[1..]),
            "true" | ":" => {}
            name if self.fns.contains(name) => {
                let items = self.exec_items(&args[1..]);
                let k = items.len();
                self.used_pa = true;
                self.emit("{");
                self.depth += 1;
                self.emit(&format!("g_pa_n = {k};"));
                for (i, item) in items.iter().enumerate() {
                    let v = self.expr_str(item);
                    self.emit(&format!("g_pa[{i}] = cat({v}, ivec2(0, 0));"));
                }
                self.emit(&format!("{}();", self.ident(cmd)));
                self.depth -= 1;
                self.emit("}");
            }
            _ => self.mark_todo(&format!("exec {cmd}")),
        }
    }

    /// echo/function args: `args` is `[Array(items)]` or a bare value.
    /// Collect the var name(s) a `local` item introduces
    /// (`Str("s=")` → s, `Str("s")` → s, `Str("s=$1")` → s,
    /// `Interpolate([Lit("s="),..])` → s).
    fn local_names(&mut self, item: &IrExpr) {
        match item {
            IrExpr::Str(s, _) => {
                let name = s.split('=').next().unwrap_or(s);
                if !name.is_empty() {
                    self.vars.insert(name.to_string());
                }
            }
            IrExpr::Interpolate(parts) => {
                if let Some(InterpPart::Lit(first)) = parts.first() {
                    let name = first.split('=').next().unwrap_or(first);
                    if !name.is_empty() {
                        self.vars.insert(name.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    fn exec_items(&mut self, args: &[IrExpr]) -> Vec<IrExpr> {
        match args.first() {
            Some(IrExpr::Array(items)) => items.clone(),
            Some(other) => vec![other.clone()],
            None => vec![],
        }
    }

    fn printf(&mut self, args: &[IrExpr]) {
        let Some(IrExpr::Str(fmt, _)) = args.first() else {
            self.mark_todo("printf non-literal format");
            return;
        };
        let mut vals = self.exec_items(&args[1..]);
        let mut vi = 0;
        let mut chars = fmt.chars().peekable();
        let mut out: Vec<String> = Vec::new();
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some('n') => out.push("putCh(10);".to_string()),
                    Some('t') => out.push("putCh(9);".to_string()),
                    Some('r') => out.push("putCh(13);".to_string()),
                    Some('\\') => out.push("putCh(92);".to_string()),
                    Some(other) => {
                        let l = self.strlit(&other.to_string());
                        out.push(format!("putStr({l});"));
                    }
                    None => {}
                },
                '%' => match chars.next() {
                    Some('%') => out.push("putCh(37);".to_string()),
                    Some('s') => {
                        if vi < vals.len() {
                            let v = self.expr_str(&vals[vi]);
                            out.push(format!("putStr({v});"));
                        }
                        vi += 1;
                    }
                    Some('d') | Some('i') => {
                        if vi < vals.len() {
                            let n = self.expr_num(&vals[vi]);
                            out.push(format!("putStr(itos({n}));"));
                        }
                        vi += 1;
                    }
                    Some(other) => {
                        let l = self.strlit(&format!("%{other}"));
                        out.push(format!("putStr({l});"));
                    }
                    None => out.push("putCh(37);".to_string()),
                },
                other => {
                    let l = self.strlit(&other.to_string());
                    out.push(format!("putStr({l});"));
                }
            }
        }
        for o in out {
            self.emit(&o);
        }
    }

    fn local_decl(&mut self, args: &[IrExpr]) {
        let items = self.exec_items(args);
        let mut i = 0;
        while i < items.len() {
            match &items[i] {
                IrExpr::Str(s, _) if s.ends_with('=') => {
                    let name = &s[..s.len() - 1];
                    let v = self.ident(name);
                    // `local s=value` — value is the following item, or
                    // empty (`local s=`).
                    let val = if i + 1 < items.len() {
                        let expr = &items[i + 1];
                        i += 1;
                        if self.is_num(name) {
                            format!("{}", self.expr_num(expr))
                        } else {
                            format!("cat({}, ivec2(0, 0))", self.expr_str(expr))
                        }
                    } else {
                        self.strlit("")
                    };
                    self.emit(&format!("{v} = {val};"));
                }
                IrExpr::Str(s, _) if s.contains('=') => {
                    // `local name=value` — value may be a positional
                    // (`$1`), a var read, or a literal.
                    let (name, rest) = s.split_once('=').unwrap_or((s.as_str(), ""));
                    let v = self.ident(name);
                    if self.is_num(name) {
                        let val = self.simple_word_num(rest);
                        self.emit(&format!("{v} = {val};"));
                    } else {
                        let val = self.simple_word(rest);
                        self.emit(&format!("{v} = cat({val}, ivec2(0, 0));"));
                    }
                }
                IrExpr::Str(s, _) => {
                    self.emit(&format!("/* local {} (declare-only) */", self.ident(s)));
                }
                IrExpr::Interpolate(parts) => {
                    // `local s="sum"` as a single item: [Lit("s="), value...]
                    if let Some(InterpPart::Lit(first)) = parts.first() {
                        if let Some(name) = first.split('=').next() {
                            if first.contains('=') && !name.is_empty() {
                                let rest = &parts[1..];
                                let mut acc: Option<String> = None;
                                for p in rest {
                                    match p {
                                        InterpPart::Lit(t) => {
                                            let l = self.strlit(t);
                                            acc = Some(match acc {
                                                None => l,
                                                Some(a) => format!("cat({a}, {l})"),
                                            });
                                        }
                                        InterpPart::Expr(e) => {
                                            let v = self.expr_str(e);
                                            acc = Some(match acc {
                                                None => v,
                                                Some(a) => format!("cat({a}, {v})"),
                                            });
                                        }
                                    }
                                }
                                let val = acc.unwrap_or_else(|| self.strlit(""));
                                let v = self.ident(name);
                                self.emit(&format!("{v} = cat({val}, ivec2(0, 0));"));
                            }
                        }
                    }
                }
                _ => self.mark_todo("local complex"),
            }
            i += 1;
        }
    }

    /// A `local name=value` word in NUMERIC context (the var is Int):
    /// `2` → `(2)`, `$1` → positional, `$name` → int var read.
    fn simple_word_num(&mut self, w: &str) -> String {
        let t = w.trim();
        if let Some(name) = t.strip_prefix("${").and_then(|x| x.strip_suffix('}')) {
            return self.simple_ref_num(name);
        }
        if let Some(name) = t.strip_prefix('$') {
            return self.simple_ref_num(name);
        }
        if let Ok(n) = t.parse::<i64>() {
            if n >= i32::MIN as i64 && n <= i32::MAX as i64 {
                return format!("({n})");
            }
            self.todo += 1;
        }
        self.todo += 1;
        format!("/* TODO(local num: {}) */ 0", sanitize(t))
    }

    fn simple_ref_num(&mut self, name: &str) -> String {
        if let Some(v) = self.special_var_num(name) {
            return v;
        }
        if is_positional(name) {
            return self.positional_num(name);
        }
        if self.is_num(name) {
            self.ident(name)
        } else {
            format!("s2i({})", self.ident(name))
        }
    }

    /// A `local` value word: `$1` → positional read, `$name` → var
    /// read, `${name}` → var read, plain text → string literal.
    fn simple_word(&mut self, w: &str) -> String {
        let t = w.trim();
        if let Some(name) = t.strip_prefix("${").and_then(|x| x.strip_suffix('}')) {
            if is_positional(name) {
                return self.positional_str(name);
            }
            return if self.is_num(name) {
                format!("itos({})", self.ident(name))
            } else {
                self.ident(name)
            };
        }
        if let Some(name) = t.strip_prefix('$') {
            if is_positional(name) {
                return self.positional_str(name);
            }
            return if self.is_num(name) {
                format!("itos({})", self.ident(name))
            } else {
                self.ident(name)
            };
        }
        self.strlit(t)
    }

    fn for_loop(&mut self, var: &str, iter: &IrExpr, body: &[IrStmt]) {
        let v = self.ident(var);
        match iter {
            IrExpr::Range { start, end } => {
                self.emit(&format!("for ({v} = {start}; {v} <= {end}; {v}++) {{"));
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrExpr::Array(items) => {
                if self.is_num(var) {
                    // numeric loop var: iterate the literal values
                    self.emit(&format!("for (int _fi = 0; _fi < {}; _fi++) {{", items.len()));
                    self.depth += 1;
                    for (i, item) in items.iter().enumerate() {
                        let val = self.expr_num(item);
                        self.emit(&format!("if (_fi == {i}) {{ {v} = {val}; }}"));
                    }
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                } else {
                    // string loop var: materialize the items, iterate
                    self.used_fit = true;
                    self.emit("{");
                    self.depth += 1;
                    self.emit(&format!("g_fit_n = {};", items.len()));
                    for (i, item) in items.iter().enumerate() {
                        let val = self.expr_str(item);
                        self.emit(&format!("g_fit[{i}] = cat({val}, ivec2(0, 0));"));
                    }
                    self.emit("for (int _fi = 0; _fi < g_fit_n; _fi++) {");
                    self.depth += 1;
                    self.emit(&format!("{v} = g_fit[_fi];"));
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                    self.depth -= 1;
                    self.emit("}");
                }
            }
            _ => {
                self.mark_todo("for-iter (word-split / capture)");
                self.emit("{");
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
            }
        }
    }

    fn case(&mut self, discriminant: &IrExpr, clauses: &[IrCaseClause]) {
        let d = self.expr_str(discriminant);
        let mut first = true;
        for IrCaseClause { patterns, body } in clauses {
            if patterns.iter().any(|p| p == "*") {
                // default branch
                if first {
                    self.emit("{");
                } else {
                    self.emit("} else {");
                }
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                first = false;
                continue;
            }
            let conds: Vec<String> = patterns
                .iter()
                .map(|p| format!("globMatch({d}, {})", self.strlit(p)))
                .collect();
            let cond = conds.join(" || ");
            if first {
                self.emit(&format!("if ({cond}) {{"));
            } else {
                self.emit(&format!("}} else if ({cond}) {{"));
            }
            self.depth += 1;
            for s in body {
                self.stmt(s);
            }
            self.depth -= 1;
            first = false;
        }
        if !first {
            self.emit("}");
        }
    }
}

fn is_positional(name: &str) -> bool {
    name.chars().all(|c| c.is_ascii_digit())
}

/// Leading shell-identifier part of a getVar name. Param-expansion
/// forms (`MAXWAIT:-10`, `arr[0]`, `var+x`) all normalize to the base
/// variable so collection and rendering agree (and no duplicate/empty
/// mangled globals appear).
fn base_var_name(name: &str) -> Option<&str> {
    let s = name
        .strip_prefix('$')
        .and_then(|x| x.strip_prefix('{'))
        .and_then(|x| x.strip_suffix('}'))
        .unwrap_or(name);
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    let base = &s[..end];
    if base.is_empty() || base.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        None
    } else {
        Some(base)
    }
}

/// Test-string tokenizer: whitespace splits, quoted segments stay whole,
/// and a comparison operator (`= == != < <= > >=`) glued between two
/// operands (`"$x"="root"`) becomes its own token.
fn test_tokenize(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut toks: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '$' && i + 1 < chars.len() && chars[i + 1] == '(' {
            // `$((...))` — keep the whole arith expression as one token
            let mut depth = 0;
            let mut j = i;
            let mut started = false;
            while j < chars.len() {
                if chars[j] == '(' {
                    depth += 1;
                    started = true;
                } else if chars[j] == ')' {
                    depth -= 1;
                }
                j += 1;
                if started && depth == 0 {
                    break;
                }
            }
            let seg: String = chars[i..j.min(chars.len())].iter().collect();
            if !cur.is_empty() {
                toks.push(std::mem::take(&mut cur));
            }
            toks.push(seg);
            i = j;
            continue;
        }
        if c == '"' || c == '\'' {
            // quoted segment stays one token (no operator split inside)
            cur.push(c);
            i += 1;
            while i < chars.len() && chars[i] != c {
                cur.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                cur.push(chars[i]);
                i += 1;
            }
            continue;
        }
        if c.is_whitespace() {
            if !cur.is_empty() {
                toks.push(std::mem::take(&mut cur));
            }
            i += 1;
            continue;
        }
        let two: String = chars
            .get(i..i + 2)
            .map(|s| s.iter().collect())
            .unwrap_or_default();
        if matches!(two.as_str(), "==" | "!=" | "<=" | ">=") || c == '=' || c == '<' || c == '>' {
            if !cur.is_empty() {
                toks.push(std::mem::take(&mut cur));
            }
            if two.len() == 2 && matches!(two.as_str(), "==" | "!=" | "<=" | ">=") {
                toks.push(two);
                i += 2;
            } else {
                toks.push(c.to_string());
                i += 1;
            }
            continue;
        }
        cur.push(c);
        i += 1;
    }
    if !cur.is_empty() {
        toks.push(cur);
    }
    toks
}

/// Make arbitrary shell text safe inside a `/* */` comment (comments do
/// not nest in GLSL — a `*/` in a variable name or string value would
/// terminate the marker early and break the shader).
fn sanitize(s: &str) -> String {
    s.replace("*/", "*_/").replace("/*", "/_*")
}

// ── mediump gate: interval proof that every int intermediate fits ────
// GLSL ES 1.00 fragment shaders MUST support `mediump int`/`mediump
// float`; `highp` is optional there. The generated shader compiles on
// every WebGL1 implementation only when it stays within the mandatory
// mediump forms. A forward interval analysis over the rendered
// arithmetic proves that no integer intermediate exceeds ±2^15 (the
// mediump int guarantee), including the `%` emulation `a - b*(a/b)`
// (three intermediates of its own). Unprovable constructs fail closed →
// highp. Everything is deliberately conservative: this gate may refuse
// mediump, but must never emit it unsoundly.

/// The mediump int guarantee: values in [-32768, 32767] (16-bit signed).
const MEDIUMP_I16: i128 = 32767;

#[derive(Clone, Copy)]
struct Range {
    lo: i128,
    hi: i128,
}

impl Range {
    fn point(v: i64) -> Option<Range> {
        let v = v as i128;
        if v < -MEDIUMP_I16 - 1 || v > MEDIUMP_I16 {
            None // the literal itself does not fit mediump int
        } else {
            Some(Range { lo: v, hi: v })
        }
    }

    fn fits(self) -> bool {
        self.lo >= -MEDIUMP_I16 - 1 && self.hi <= MEDIUMP_I16
    }

    fn join(a: Range, b: Range) -> Range {
        Range { lo: a.lo.min(b.lo), hi: a.hi.max(b.hi) }
    }

    fn abs_max(self) -> i128 {
        self.lo.abs().max(self.hi.abs())
    }

    fn add(a: Option<Range>, b: Option<Range>) -> Option<Range> {
        let (a, b) = (a?, b?);
        let r = Range { lo: a.lo + b.lo, hi: a.hi + b.hi };
        r.fits().then_some(r)
    }

    fn sub(a: Option<Range>, b: Option<Range>) -> Option<Range> {
        let (a, b) = (a?, b?);
        let r = Range { lo: a.lo - b.hi, hi: a.hi - b.lo };
        r.fits().then_some(r)
    }

    fn mul(a: Option<Range>, b: Option<Range>) -> Option<Range> {
        let (a, b) = (a?, b?);
        let c = [
            a.lo * b.lo,
            a.lo * b.hi,
            a.hi * b.lo,
            a.hi * b.hi,
        ];
        let r = Range { lo: c.iter().min().copied().unwrap(), hi: c.iter().max().copied().unwrap() };
        r.fits().then_some(r)
    }

    /// Truncating integer division (GLSL int `/`): with a zero-free
    /// divisor, |a/b| ≤ ceil(max|a| / min|b|) — truncation only shrinks
    /// magnitude, so this is a sound bound. Sign is discarded (the
    /// result may be either), which is fine for the mediump check.
    fn div(a: Option<Range>, b: Option<Range>) -> Option<Range> {
        let (a, b) = (a?, b?);
        if b.lo <= 0 && b.hi >= 0 {
            return None; // divisor may be zero → undefined
        }
        let mb = b.lo.abs().min(b.hi.abs());
        let bound = (a.abs_max() + mb - 1) / mb; // ceil
        if bound > MEDIUMP_I16 {
            return None;
        }
        // Sign-aware: a non-negative (resp. non-positive) dividend with a
        // positive divisor truncates to a non-negative (resp. non-positive)
        // quotient — keeping the sign instead of widening to ±bound is what
        // lets the 0..127 tint's [0,32385]/128 stay [0,254] (not [-254,254]);
        // a signed dividend still widens to ±bound. This is what keeps the
        // damage blend's `r - (r-cr_r)*mix/256` (with r-cr_r ≥ -127) inside
        // mediump int — the widened negative would otherwise overflow the
        // CRT term that follows.
        if a.lo >= 0 {
            Some(Range { lo: 0, hi: bound })
        } else if a.hi <= 0 {
            Some(Range { lo: -bound, hi: 0 })
        } else {
            Some(Range { lo: -bound, hi: bound })
        }
    }

    /// The `%` emulation `a - b*(a/b)` — THREE intermediates must fit:
    /// `a/b`, `b*(a/b)`, and the subtraction. The remainder's own range
    /// is |a%b| < |b|.
    fn rem(a: Option<Range>, b: Option<Range>) -> Option<Range> {
        let (a, b) = (a?, b?);
        if b.lo <= 0 && b.hi >= 0 {
            return None;
        }
        let mb_min = b.lo.abs().min(b.hi.abs());
        let mb_max = b.lo.abs().max(b.hi.abs());
        let ma = a.abs_max();
        let q = (ma + mb_min - 1) / mb_min; // ceil |a/b|
        let bq = mb_max * q; // |b*(a/b)|
        if q > MEDIUMP_I16 || bq > MEDIUMP_I16 || ma + bq > MEDIUMP_I16 {
            return None;
        }
        let r = Range { lo: -mb_max, hi: mb_max };
        r.fits().then_some(r)
    }
}

/// Forward interval analysis over the A1 statements. Returns false when
/// any construct cannot be proven (loops, functions, arrays, unknown
/// calls) or when any integer intermediate exceeds ±2^15.
fn fits_mediump_int(prog: &IrProgram, opts: &ShGlslOptions) -> bool {
    if !prog.subs.is_empty() {
        return false; // function bodies / recursion → unprovable
    }
    let mut vars: std::collections::HashMap<String, Option<Range>> = std::collections::HashMap::new();
    // input bridge ranges (the renderer's seeds; only referenced ones
    // are ever read)
    let seed = |vars: &mut std::collections::HashMap<String, Option<Range>>, name: &str, hi: i64| {
        vars.insert(name.to_string(), Some(Range { lo: 0, hi: hi as i128 })); // [0, hi]
    };
    if opts.color_out {
        seed(&mut vars, "frag_x", opts.max_view as i64);
        seed(&mut vars, "frag_y", opts.max_view as i64);
        seed(&mut vars, "vcolor_r", 127);
        seed(&mut vars, "vcolor_g", 127);
        seed(&mut vars, "vcolor_b", 127);
        if opts.tex_size > 0 {
            seed(&mut vars, "uv_x", opts.tex_size as i64);
            seed(&mut vars, "uv_y", opts.tex_size as i64);
            seed(&mut vars, "tex_r", 255);
            seed(&mut vars, "tex_g", 255);
            seed(&mut vars, "tex_b", 255);
            seed(&mut vars, "cr_r", 127);
            seed(&mut vars, "cr_g", 127);
            seed(&mut vars, "cr_b", 127);
            seed(&mut vars, "cr_a", 127);
            seed(&mut vars, "damage", 3);
        }
    }
    walk_stmts(&prog.stmts, &mut vars)
}

/// Range of a numeric IrExpr; None = unprovable (fails the gate).
fn expr_range(e: &IrExpr, vars: &std::collections::HashMap<String, Option<Range>>) -> Option<Range> {
    match e {
        IrExpr::Int(n) => Range::point(*n),
        IrExpr::Str(s, _) => s.trim().parse::<i64>().ok().and_then(Range::point),
        IrExpr::Var(n, _) | IrExpr::Ident(n) => vars.get(n).copied().flatten(),
        IrExpr::Arith(a) => arith_range(a, vars),
        IrExpr::BinOp { lhs, op, rhs } => match op {
            BinOpKind::Add => Range::add(expr_range(lhs, vars), expr_range(rhs, vars)),
            BinOpKind::Sub => Range::sub(expr_range(lhs, vars), expr_range(rhs, vars)),
            BinOpKind::Mul => Range::mul(expr_range(lhs, vars), expr_range(rhs, vars)),
            BinOpKind::Div => Range::div(expr_range(lhs, vars), expr_range(rhs, vars)),
            BinOpKind::Mod => Range::rem(expr_range(lhs, vars), expr_range(rhs, vars)),
            BinOpKind::Eq | BinOpKind::Ne | BinOpKind::Lt | BinOpKind::Gt
            | BinOpKind::Le | BinOpKind::Ge | BinOpKind::And | BinOpKind::Or
            | BinOpKind::Not => Range::point(1), // comparisons/booleans → 0/1
            BinOpKind::Concat | BinOpKind::Pow | BinOpKind::BitAnd | BinOpKind::BitOr
            | BinOpKind::BitXor | BinOpKind::ShiftL | BinOpKind::ShiftR => None,
        },
        IrExpr::Ternary { cond, then, else_ } => {
            expr_range(cond, vars)?;
            let t = expr_range(then, vars)?;
            let e = expr_range(else_, vars)?;
            Some(Range::join(t, e))
        }
        IrExpr::Call { func, args } => match func.as_str() {
            "arith" => match args.first() {
                Some(IrExpr::Str(text, _)) => {
                    crate::shir::parse_arith(text).and_then(|a| arith_range(&a, vars))
                }
                _ => None,
            },
            "getVar" => match args.first() {
                Some(IrExpr::Str(name, _)) => {
                    let n = name.trim_start_matches('$');
                    vars.get(n).copied().flatten()
                }
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// Range of an arithmetic AST node; None = unprovable.
fn arith_range(a: &ArithAst, vars: &std::collections::HashMap<String, Option<Range>>) -> Option<Range> {
    match a {
        ArithAst::Num(n) => Range::point(*n),
        ArithAst::Var(n) | ArithAst::Ident(n) => vars.get(n).copied().flatten(),
        ArithAst::Index { .. } => None, // arrays out of scope
        ArithAst::Bin { op, lhs, rhs } => {
            let l = arith_range(lhs, vars);
            let r = arith_range(rhs, vars);
            match op.as_str() {
                "+" => Range::add(l, r),
                "-" => Range::sub(l, r),
                "*" => Range::mul(l, r),
                "/" => Range::div(l, r),
                "%" => Range::rem(l, r),
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||" => Range::point(1),
                _ => None, // **, shifts, unknown
            }
        }
        ArithAst::Un { op, arg } => {
            let x = arith_range(arg, vars)?;
            match op.as_str() {
                "-" => Some(Range { lo: -x.hi, hi: -x.lo }),
                "+" => Some(x),
                "!" => Range::point(1),
                "~" => Range::add(Range::point(-1), Some(Range { lo: -x.hi, hi: -x.lo })),
                _ => None,
            }
        }
        ArithAst::Cond { test, then, else_ } => {
            arith_range(test, vars)?;
            let t = arith_range(then, vars)?;
            let e = arith_range(else_, vars)?;
            Some(Range::join(t, e))
        }
        ArithAst::Assign { var, op, rhs } => {
            // `x op= e` inside $(( )) — the read-modify-write intermediate
            let cur = vars.get(var).copied().flatten();
            let r = arith_range(rhs, vars);
            let next = match op.as_str() {
                "=" => r,
                "+=" => Range::add(cur, r),
                "-=" => Range::sub(cur, r),
                "*=" => Range::mul(cur, r),
                "/=" => Range::div(cur, r),
                "%=" => Range::rem(cur, r),
                _ => None,
            };
            next
        }
        ArithAst::IncDec { var, delta, .. } => {
            let cur = vars.get(var).copied().flatten()?;
            Range::add(Some(cur), Range::point(*delta as i64))
        }
        ArithAst::Sizeof(_) => Range::point(4),
        ArithAst::Cast { arg, .. } => arith_range(arg, vars),
    }
}

/// Numeric tokens inside a test string must fit (`[ "$x" -gt 150 ]`, or
/// `[ $((fx * 7)) -gt 3 ]`): literals ≤ ±2^15 and `$((…))` sub-arithmetic
/// provable. Bare `$var` operands are covered by their assignments.
fn test_text_ok(text: &str, vars: &std::collections::HashMap<String, Option<Range>>) -> bool {
    for tok in test_tokenize(text) {
        let t = tok.trim();
        if let Some(inner) = t.strip_prefix("$((").and_then(|x| x.strip_suffix("))")) {
            match crate::shir::parse_arith(inner) {
                Some(a) => {
                    if arith_range(&a, vars).is_none() {
                        return false;
                    }
                }
                None => return false,
            }
        } else if let Ok(n) = t.parse::<i64>() {
            if Range::point(n).is_none() {
                return false;
            }
        }
        // `$var` / quoted words / operators — no integer value of its
        // own beyond the assignments already checked.
    }
    true
}

/// `split(getVar("r"))` → `getVar("r")` (the putb arg wrapper the shell
/// lowering adds for unquoted reads). Any other shape is passed through.
fn unwrap_split(e: &IrExpr) -> IrExpr {
    if let IrExpr::Call { func, args } = e {
        if func == "split" {
            if let Some(inner) = args.first() {
                return inner.clone();
            }
        }
    }
    e.clone()
}

fn test_ok(cond: &IrExpr, vars: &std::collections::HashMap<String, Option<Range>>) -> bool {
    match cond {
        IrExpr::Call { func, args } if func == "test" => args.iter().all(|a| match a {
            IrExpr::Str(text, _) => test_text_ok(text, vars),
            _ => false,
        }),
        _ => false,
    }
}

/// Guard-aware refinement: a numeric single-var test (`"$x" -gt 40`)
/// tightens the var's range inside the taken / not-taken branch. Returns
/// (then, else) as (name, refined range); None side = no refinement.
/// This is what makes the vignette's clamp chain provable: inside
/// `if [ "$edge" -gt 150 ]` edge ≥ 151, so after the `dim>40` cap the
/// range is [1, 40] and `r - r*dim/255` stays within ±2^15.
fn test_refine(
    cond: &IrExpr,
    vars: &std::collections::HashMap<String, Option<Range>>,
) -> (
    Option<(String, Range)>,
    Option<(String, Range)>,
) {
    let text = match cond {
        IrExpr::Call { func, args } if func == "test" => match args.first() {
            Some(IrExpr::Str(t, _)) => t.as_str(),
            _ => return (None, None),
        },
        _ => return (None, None),
    };
    fn strip(t: &str) -> &str {
        let t = t.trim();
        t.strip_prefix('"')
            .and_then(|x| x.strip_suffix('"'))
            .unwrap_or(t)
    }
    let var_of = |t: &str| -> Option<String> {
        let t = strip(t);
        let t = t.strip_prefix('$')?;
        let name = t
            .strip_prefix('{')
            .and_then(|x| x.strip_suffix('}'))
            .unwrap_or(t);
        if name.is_empty() || is_positional(name) {
            return None;
        }
        Some(name.to_string())
    };
    let num_of = |t: &str| -> Option<i64> { strip(t).parse().ok() };
    let toks = test_tokenize(text);
    let [a, op, b] = toks.as_slice() else {
        return (None, None);
    };
    let refine_for = |name: &str,
                      op: &str,
                      n: i64,
                      flipped: bool|
     -> (Option<(String, Range)>, Option<(String, Range)>) {
        let cur = match vars.get(name).copied().flatten() {
            Some(r) => r,
            None => return (None, None),
        };
        let n = n as i128;
        // normalise: op is always "var op N"
        let (then_r, else_r): (Range, Option<Range>) = match (flipped, op) {
            (false, "-gt") => (Range { lo: n + 1, hi: cur.hi }, Some(Range { lo: cur.lo, hi: n })),
            (false, "-ge") => (Range { lo: n, hi: cur.hi }, Some(Range { lo: cur.lo, hi: n - 1 })),
            (false, "-lt") => (Range { lo: cur.lo, hi: n - 1 }, Some(Range { lo: n, hi: cur.hi })),
            (false, "-le") => (Range { lo: cur.lo, hi: n }, Some(Range { lo: n + 1, hi: cur.hi })),
            (false, "-eq") => (Range { lo: n, hi: n }, None),
            (false, "-ne") => (Range { lo: cur.lo, hi: cur.hi }, Some(Range { lo: n, hi: n })),
            (true, "-gt") => (Range { lo: cur.lo, hi: n - 1 }, Some(Range { lo: n, hi: cur.hi })),
            (true, "-ge") => (Range { lo: cur.lo, hi: n }, Some(Range { lo: n + 1, hi: cur.hi })),
            (true, "-lt") => (Range { lo: n + 1, hi: cur.hi }, Some(Range { lo: cur.lo, hi: n })),
            (true, "-le") => (Range { lo: n, hi: cur.hi }, Some(Range { lo: cur.lo, hi: n - 1 })),
            (true, "-eq") => (Range { lo: n, hi: n }, None),
            (true, "-ne") => (Range { lo: cur.lo, hi: cur.hi }, Some(Range { lo: n, hi: n })),
            _ => return (None, None),
        };
        (
            Some((name.to_string(), then_r)),
            else_r.map(|r| (name.to_string(), r)),
        )
    };
    if let (Some(name), Some(n)) = (var_of(a), num_of(b)) {
        return refine_for(&name, op, n, false);
    }
    if let (Some(n), Some(name)) = (num_of(a), var_of(b)) {
        return refine_for(&name, op, n, true);
    }
    (None, None)
}

fn walk_stmts(stmts: &[IrStmt], vars: &mut std::collections::HashMap<String, Option<Range>>) -> bool {
    for s in stmts {
        if !walk_stmt(s, vars) {
            if std::env::var("GLSL_MEDIUMP_DEBUG").is_ok() {
                eprintln!("mediump gate: failing stmt = {s:?}");
            }
            return false;
        }
    }
    true
}

fn walk_stmt(s: &IrStmt, vars: &mut std::collections::HashMap<String, Option<Range>>) -> bool {
    match s {
        IrStmt::Ext(n) => crate::shir_nodes::ExtNode::children(&**n).into_iter().any(|c| walk_stmt(c, vars)),
        IrStmt::Assign { targets, expr, .. } => {
            let r = expr_range(expr, vars);
            if r.is_none() {
                return false;
            }
            for t in targets {
                if !t.indices.is_empty() {
                    return false; // array element write
                }
                vars.insert(t.var.clone(), r);
            }
            true
        }
        IrStmt::Declare { vars: decls, init, .. } => {
            let r = match init {
                Some(i) => match expr_range(i, vars) {
                    Some(r) => Some(r),
                    None => return false,
                },
                None => None,
            };
            for Decl { name, .. } in decls {
                vars.insert(name.clone(), r);
            }
            true
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            if !test_ok(cond, vars) {
                return false;
            }
            let (t_ref, e_ref) = test_refine(cond, vars);
            let mut branches: Vec<std::collections::HashMap<String, Option<Range>>> = Vec::new();
            let mut t = vars.clone();
            if let Some((name, r)) = &t_ref {
                t.insert(name.clone(), Some(*r));
            }
            if !walk_stmts(then, &mut t) {
                return false;
            }
            branches.push(t);
            for (e, body) in elsifs {
                if !test_ok(e, vars) {
                    return false;
                }
                let (et_ref, _) = test_refine(e, vars);
                let mut b = vars.clone();
                if let Some((name, r)) = &et_ref {
                    b.insert(name.clone(), Some(*r));
                }
                if !walk_stmts(body, &mut b) {
                    return false;
                }
                branches.push(b);
            }
            let mut el = vars.clone();
            if let Some((name, r)) = &e_ref {
                el.insert(name.clone(), Some(*r));
            }
            if !walk_stmts(else_, &mut el) {
                return false;
            }
            branches.push(el);
            // union of every branch + the pre-branch state (a var
            // unknown/unbounded in ANY branch poisons the merge)
            let names: BTreeSet<String> =
                branches.iter().flat_map(|m| m.keys().cloned()).collect();
            for n in names {
                let mut acc: Option<Range> = None;
                let mut poisoned = false;
                for b in &branches {
                    match b.get(&n) {
                        Some(Some(r)) => {
                            acc = Some(match acc {
                                Some(a) => Range::join(a, *r),
                                None => *r,
                            })
                        }
                        Some(None) => poisoned = true,
                        None => {}
                    }
                }
                vars.insert(n, if poisoned { None } else { acc });
            }
            true
        }
        IrStmt::Block(body) => walk_stmts(body, vars),
        IrStmt::Expr(e) => match e {
            IrExpr::Call { func, args } if func == "putb" => match args.first() {
                Some(a) => expr_range(a, vars).is_some(),
                None => true,
            },
            IrExpr::Call { func, args } if func == "exec" => match args.first() {
                // `putb $r` lowers to exec("putb", [split(getVar("r"))])
                Some(IrExpr::Str(cmd, _)) if cmd == "putb" => {
                    match args.get(1) {
                        Some(IrExpr::Array(items)) => {
                            for item in items {
                                if expr_range(&unwrap_split(item), vars).is_none() {
                                    return false;
                                }
                            }
                            true
                        }
                        Some(other) => expr_range(&unwrap_split(other), vars).is_some(),
                        None => true,
                    }
                }
                _ => false, // any other exec → fail closed
            },
            IrExpr::Call { func, args } if func == "setVar" || func == "assign" => {
                match args.first() {
                    Some(IrExpr::Str(name, _)) => {
                        if args.len() >= 3 {
                            // assign("name", op, rhs) — read-modify-write
                            if let Some(IrExpr::Str(op, _)) = args.get(1) {
                                let cur = vars.get(name).copied().flatten();
                                let rhs = expr_range(&args[2], vars);
                                let next = match op.as_str() {
                                    "=" => rhs,
                                    "+=" => Range::add(cur, rhs),
                                    "-=" => Range::sub(cur, rhs),
                                    "*=" => Range::mul(cur, rhs),
                                    "/=" => Range::div(cur, rhs),
                                    "%=" => Range::rem(cur, rhs),
                                    _ => None,
                                };
                                match next {
                                    Some(r) => {
                                        vars.insert(name.clone(), Some(r));
                                        true
                                    }
                                    None => false,
                                }
                            } else {
                                false
                            }
                        } else if args.len() >= 2 {
                            match expr_range(&args[1], vars) {
                                Some(r) => {
                                    vars.insert(name.clone(), Some(r));
                                    true
                                }
                                None => false,
                            }
                        } else {
                            false
                        }
                    }
                    _ => false,
                }
            }
            IrExpr::Call { func, args } if func == "test" => test_ok(e, vars),
            IrExpr::Call { func, .. } if func == "arith" => expr_range(e, vars).is_some(),
            _ => false,
        },
        // loops, functions, case, pipelines, subshells, redirects,
        // files, control flow — fail closed (unprovable)
        IrStmt::While { .. }
        | IrStmt::DoWhile { .. }
        | IrStmt::For { .. }
        | IrStmt::ForInit { .. }
        | IrStmt::Case { .. }
        | IrStmt::Function { .. }
        | IrStmt::Subshell(_)
        | IrStmt::Background(_)
        | IrStmt::Pipeline { .. }
        | IrStmt::Redirect { .. }
        | IrStmt::Exec { .. }
        | IrStmt::Output { .. }
        | IrStmt::Require(_)
        | IrStmt::WriteFile { .. }
        | IrStmt::Return(_)
        | IrStmt::Exit(_)
        | IrStmt::Die { .. }
        | IrStmt::Warn { .. }
        | IrStmt::Try { .. }
        | IrStmt::Select { .. }
        | IrStmt::Asm { .. }
        | IrStmt::SetChildError(_)
        | IrStmt::Label(_)
        | IrStmt::Goto(_)
        | IrStmt::RawText(_)
        | IrStmt::DeclareArray { .. }
        | IrStmt::Continue
        | IrStmt::Break => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(src: &str) -> String {
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("shir json");
        shir_to_glsl(&prog)
    }

    #[test]
    fn echo_and_arith() {
        let shader = render(
            r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"stmt_lines":[],"stmts":[
              {"type":"Assign","targets":[{"var":"x","indices":[],"sigil":null}],"expr":{"type":"Str","value":"5","style":"DoubleQuoted"}},
              {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Interpolate","parts":[{"kind":"lit","text":"x="},{"kind":"expr","expr":{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"x","style":"DoubleQuoted"}]}}]}]}]}}
            ],"subs":[],"var_types":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[]}"#,
        );
        assert!(shader.contains("#version 300 es"));
        assert!(shader.contains("g_x = 5;"));
        assert!(shader.contains("putCh(10);"));
    }

    #[test]
    fn test_cond_renders() {
        let shader = render(
            r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"stmt_lines":[],"stmts":[
              {"type":"Assign","targets":[{"var":"i","indices":[],"sigil":null}],"expr":{"type":"Str","value":"0","style":"DoubleQuoted"}},
              {"type":"While","cond":{"type":"Call","func":"test","purity":"Emulable","args":[{"type":"Str","value":"$i -lt 3","style":"DoubleQuoted"}]},"body":[
                {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Str","value":"hi","style":"DoubleQuoted"}]}]}},
                {"type":"Assign","targets":[{"var":"i","indices":[],"sigil":null}],"expr":{"type":"Arith","ast":{"type":"Bin","op":"+","lhs":{"type":"Var","name":"i"},"rhs":{"type":"Num","value":1}}}}
              ]}
            ],"subs":[],"var_types":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[]}"#,
        );
        assert!(shader.contains("while ((g_i < 3))"));
        assert!(shader.contains("g_i = (g_i + 1);"));
    }

    #[test]
    fn exec_todo() {
        let shader = render(
            r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"stmt_lines":[],"stmts":[
              {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Spawn","args":[{"type":"Str","value":"ls","style":"DoubleQuoted"},{"type":"Array","elements":[]}]}}
            ],"subs":[],"var_types":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[]}"#,
        );
        assert!(shader.contains("TODO(unsupported): exec ls"));
    }

    // ── shIR transform: dead runtime DCE + scalar promotion ──────

    fn render_opts(src: &str, opts: ShGlslOptions) -> String {
        let prog = crate::shir_json_in::shir_json_to_ir(src).expect("shir json");
        shir_to_glsl_opts(&prog, &opts)
    }

    #[test]
    fn dce_runtime_arrays() {
        // no user functions, no string for-loops → g_pa/g_fit must not
        // be emitted at all (previously always present).
        let shader = render(
            r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"stmt_lines":[],"stmts":[
              {"type":"Assign","targets":[{"var":"x","indices":[],"sigil":null}],"expr":{"type":"Str","value":"5","style":"DoubleQuoted"}},
              {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"echo","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Str","value":"hi","style":"DoubleQuoted"}]}]}}
            ],"subs":[],"var_types":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[]}"#,
        );
        assert!(!shader.contains("g_pa"), "g_pa emitted with no functions");
        assert!(!shader.contains("g_fit"), "g_fit emitted with no string for-loops");
    }

    #[test]
    fn scalars_promoted_to_main_locals() {
        let shader = render(
            r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"stmt_lines":[],"stmts":[
              {"type":"Assign","targets":[{"var":"x","indices":[],"sigil":null}],"expr":{"type":"Str","value":"5","style":"DoubleQuoted"}}
            ],"subs":[],"var_types":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[]}"#,
        );
        // declared inside main(), not at file scope
        let main_idx = shader.find("void main() {").expect("main");
        let gx = shader.find("int g_x;").expect("g_x decl");
        assert!(gx > main_idx, "g_x must be a main() local");
        assert_eq!(shader.matches("int g_x;").count(), 1, "g_x must not also be a file-scope global");
        // and no file-scope declaration before main
        let before_main = &shader[..main_idx];
        assert!(!before_main.contains("g_x"), "g_x declared before main");
    }

    // ── mediump gate (ES 1.00 mandatory fragment precision) ─────────

    /// A frag program: vcolor → small arithmetic → putb. With max_view
    /// it must prove mediump int.
    const MEDIUMP_OK_PROG: &str = r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"stmt_lines":[],"stmts":[
      {"type":"Assign","targets":[{"var":"r","indices":[],"sigil":null}],"expr":{"type":"Arith","ast":{"type":"Var","name":"vcolor_r"}}},
      {"type":"Assign","targets":[{"var":"r","indices":[],"sigil":null}],"expr":{"type":"Arith","ast":{"type":"Bin","op":"*","lhs":{"type":"Var","name":"r"},"rhs":{"type":"Num","value":90}}}},
      {"type":"Assign","targets":[{"var":"r","indices":[],"sigil":null}],"expr":{"type":"Arith","ast":{"type":"Bin","op":"/","lhs":{"type":"Var","name":"r"},"rhs":{"type":"Num","value":100}}}},
      {"type":"Expr","expr":{"type":"Call","func":"exec","purity":"Emulable","args":[{"type":"Str","value":"putb","style":"DoubleQuoted"},{"type":"Array","elements":[{"type":"Call","func":"split","purity":"Emulable","args":[{"type":"Call","func":"getVar","purity":"Emulable","args":[{"type":"Str","value":"r","style":"DoubleQuoted"}]}]}]}]}}
    ],"subs":[],"var_types":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[]}"#;

    #[test]
    fn mediump_emitted_when_provable() {
        let shader = render_opts(
            MEDIUMP_OK_PROG,
            ShGlslOptions { es100: true, color_out: true, vert_out: false, tex_size: 0, max_view: 800 },
        );
        assert!(shader.contains("precision mediump int;"), "mediump int not proven");
        assert!(shader.contains("precision mediump float;"), "mediump float not proven");
    }

    #[test]
    fn mediump_refused_on_overflow() {
        // r*tex_r with both 0..255 → 65025 > 32767 → must stay highp int.
        // r is sourced from tex_r (not vcolor_r): the vcolor bridge seed
        // dropped to 0..127 (f71d804 colour-scale bridges), so a vcolor-
        // rooted r can no longer reach the overflow (127*255 = 32385 ≤
        // 32767) — tex_r stays 0..255, keeping the test's premise.
        let prog = MEDIUMP_OK_PROG
            .replace(
                "\"ast\":{\"type\":\"Var\",\"name\":\"vcolor_r\"}",
                "\"ast\":{\"type\":\"Var\",\"name\":\"tex_r\"}",
            )
            .replace(
                "\"rhs\":{\"type\":\"Num\",\"value\":90}",
                "\"rhs\":{\"type\":\"Var\",\"name\":\"tex_r\"}",
            );
        let shader = render_opts(
            &prog,
            ShGlslOptions { es100: true, color_out: true, vert_out: false, tex_size: 32, max_view: 800 },
        );
        assert!(shader.contains("precision highp int;"), "overflow must refuse mediump int");
        assert!(shader.contains("precision mediump float;"), "float side stays provable");
    }

    #[test]
    fn mediump_refused_without_max_view() {
        let shader = render_opts(
            MEDIUMP_OK_PROG,
            ShGlslOptions { es100: true, color_out: true, vert_out: false, tex_size: 0, max_view: 0 },
        );
        assert!(shader.contains("precision highp int;"), "max_view=0 must refuse mediump");
        assert!(shader.contains("precision highp float;"));
    }

    #[test]
    fn mediump_refused_on_big_literal() {
        // x = 100000 → the literal itself exceeds mediump int
        let prog = MEDIUMP_OK_PROG.replace(
            "{\"type\":\"Var\",\"name\":\"vcolor_r\"}",
            "{\"type\":\"Num\",\"value\":100000}",
        );
        let shader = render_opts(
            &prog,
            ShGlslOptions { es100: true, color_out: true, vert_out: false, tex_size: 0, max_view: 800 },
        );
        assert!(shader.contains("precision highp int;"), "big literal must refuse mediump");
    }

    #[test]
    fn mediump_float_requires_small_viewport() {
        // max_view > 2048 → gl_FragCoord is not exact in mediump float
        let shader = render_opts(
            MEDIUMP_OK_PROG,
            ShGlslOptions { es100: true, color_out: true, vert_out: false, tex_size: 0, max_view: 4096 },
        );
        assert!(shader.contains("precision highp float;"), "4096-wide canvas must keep highp float");
        assert!(shader.contains("precision mediump int;"), "int side may still be proven");
    }

    #[test]
    fn render_fragment_is_lean() {
        // the es100+color_out render fragment (the sh2glsl/wasm path):
        // putb writes fixed slots → 4-byte out_buf, no OUT_CAP, no
        // out_len, no g_pa/g_fit, no string runtime.
        let shader = render_opts(
            r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"stmt_lines":[],"stmts":[
              {"type":"Expr","expr":{"type":"Call","func":"putb","purity":"Emulable","args":[{"type":"Str","value":"255","style":"DoubleQuoted"}]}}
            ],"subs":[],"var_types":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[]}"#,
            ShGlslOptions { es100: true, color_out: true, vert_out: false, tex_size: 32, max_view: 0 },
        );
        assert!(!shader.contains("OUT_CAP"), "OUT_CAP in render fragment");
        assert!(!shader.contains("out_len"), "out_len in render fragment");
        assert!(!shader.contains("g_pa"), "g_pa in render fragment");
        assert!(!shader.contains("g_fit"), "g_fit in render fragment");
        assert!(!shader.contains("s_scratch"), "s_scratch in render fragment");
        assert!(shader.contains("int out_buf[4];"), "out_buf not sized for the 4 colour slots");
        assert!(shader.contains("out_buf[0] = 255;"), "putb write missing");
    }

    #[test]
    fn es3_putb_keeps_putch() {
        // es3+color_out putb lowers to putCh — the helper must survive
        // even with no string runtime (latent bug fix).
        let shader = render_opts(
            r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"stmt_lines":[],"stmts":[
              {"type":"Expr","expr":{"type":"Call","func":"putb","purity":"Emulable","args":[{"type":"Str","value":"255","style":"DoubleQuoted"}]}}
            ],"subs":[],"var_types":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[]}"#,
            ShGlslOptions { es100: false, color_out: true, vert_out: false, tex_size: 0, max_view: 0 },
        );
        assert!(shader.contains("void putCh(int c)"), "putCh helper missing");
        assert!(shader.contains("const int OUT_CAP"), "OUT_CAP missing for putCh");
        assert!(shader.contains("int out_len = 0;"), "out_len missing for putCh");
    }

    #[test]
    fn bridges_use_gated() {
        // a program referencing only frag_x gets NO vColor varying, no
        // texture uniforms, no texture2D seeds — the texture machinery
        // must not be emitted for a texture-less fragment.
        let shader = render_opts(
            r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"stmt_lines":[],"stmts":[
              {"type":"Assign","targets":[{"var":"fx","indices":[],"sigil":null}],"expr":{"type":"Arith","ast":{"type":"Var","name":"frag_x"}}},
              {"type":"Expr","expr":{"type":"Call","func":"putb","purity":"Emulable","args":[{"type":"Str","value":"255","style":"DoubleQuoted"}]}}
            ],"subs":[],"var_types":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[]}"#,
            ShGlslOptions { es100: true, color_out: true, vert_out: false, tex_size: 32, max_view: 0 },
        );
        assert!(shader.contains("g_frag_x = int(gl_FragCoord.x);"), "frag_x seed missing");
        assert!(!shader.contains("vColor"), "vColor declared unused");
        assert!(!shader.contains("uTex"), "uTex declared unused");
        assert!(!shader.contains("uCrack"), "uCrack declared unused");
        assert!(!shader.contains("uDamage"), "uDamage declared unused");
        assert!(!shader.contains("texture2D"), "texture2D emitted unused");
        assert!(!shader.contains("g_uv_x"), "uv seed emitted unused");
        assert!(!shader.contains("g_tex_r"), "tex seed emitted unused");
    }

    #[test]
    fn bridges_tex_group_dependency() {
        // referencing tex_r must pull in vUv + the uv seeds (the sample
        // coordinate reads them) but NOT the crack/damage machinery. The
        // uv seed is gated on a DIRECT uv_x/uv_y read (b5ae282: the
        // texture sample itself uses fract(vUv), only programs that read
        // the uv bridges need the seed), so the program reads uv_x too.
        let shader = render_opts(
            r#"{"type":"Program","contract_version":1,"imports":[],"requires":[],"stmt_lines":[],"stmts":[
              {"type":"Assign","targets":[{"var":"t","indices":[],"sigil":null}],"expr":{"type":"Arith","ast":{"type":"Var","name":"tex_r"}}},
              {"type":"Assign","targets":[{"var":"u","indices":[],"sigil":null}],"expr":{"type":"Var","name":"uv_x","sigil":null}},
              {"type":"Expr","expr":{"type":"Call","func":"putb","purity":"Emulable","args":[{"type":"Str","value":"255","style":"DoubleQuoted"}]}}
            ],"subs":[],"var_types":[],"var_lengths":[],"var_const":[],"var_lifetimes":[],"var_nospace":[]}"#,
            ShGlslOptions { es100: true, color_out: true, vert_out: false, tex_size: 32, max_view: 0 },
        );
        assert!(shader.contains("varying highp vec2 vUv;"), "vUv missing for tex bridge");
        assert!(shader.contains("uniform sampler2D uTex;"), "uTex missing for tex bridge");
        assert!(shader.contains("int g_uv_x;"), "uv_x not declared (the tex seeds write it)");
        assert!(shader.contains("int g_uv_y;"), "uv_y not declared (the tex seeds write it)");
        // the grid is tex_size (the test opts use 32 — ac19fa4 moved the
        // MIME name textures to 32×32; the assert tracks the option).
        assert!(shader.contains("g_uv_x = int(vUv.x * 32.0);"), "uv seed missing");
        assert!(shader.contains("vec4 _tex = texture2D(uTex"), "uTex sample missing");
        assert!(shader.contains("g_tex_r = int(_tex.r * 255.0);"), "tex_r seed missing");
        assert!(!shader.contains("g_tex_g"), "tex_g seeded unused");
        assert!(!shader.contains("uCrack"), "uCrack declared unused");
        assert!(!shader.contains("uDamage"), "uDamage declared unused");
    }

    // ── render vertex mode (vert_out) ──────────────────────────────

    /// render a BASH program with options (the same pipeline as the
    /// `sh2glsl` binary: parse_commands_from_text → ast_to_ir_raw).
    fn render_bash_opts(src: &str, opts: ShGlslOptions) -> String {
        let cmds = crate::parser::commands::parse_commands_from_text(src).expect("parse");
        let prog = crate::shir::ast_to_ir_raw(&cmds);
        shir_to_glsl_opts(&prog, &opts)
    }

    #[test]
    fn vertex_mode_emits_program_and_bridges() {
        // the real MIMEcroft vertex program: object→world position
        // (float bc), camera-relative delta, bc-trig rotation, and the
        // vp_*/vc_*/vu_* outputs.
        let shader = render_bash_opts(
            r#"
wx=$(echo "scale=4; $ap_x * $usc_x / 1000000.0 + $uop_x / 1000.0" | bc)
rad=$(echo "scale=8; $ucy_m * 3.14159265 / 180000.0" | bc)
c=$(echo "scale=6; c($rad) + 0.0" | bc)
s=$(echo "scale=6; s($rad) + 0.0" | bc)
dx=$(echo "scale=4; $wx - $cx + 0.0" | bc)
dz=$(echo "scale=4; $wz - $cz + 0.0" | bc)
relx=$(echo "scale=4; $dx * $c + $dz * $s + 0.0" | bc)
relz=$(echo "scale=4; 0 - $dx * $s + $dz * $c + 0.0" | bc)
w=$(echo "scale=4; 0 - $relz + 0.0" | bc)
vp_x=$(echo "scale=4; $relx * 0.9" | bc)
vp_z=$(echo "scale=4; $w * $w / 64.0" | bc)
vp_w=$w
vc_r=$((ash_r * ublk_r / 1000))
vu_u=$auv_u
"#,
            ShGlslOptions { es100: true, color_out: false, vert_out: true, tex_size: 32, max_view: 0 },
        );
        // header: attributes/uniforms/varyings
        assert!(shader.contains("attribute vec3 aPosition;"), "aPosition missing");
        assert!(shader.contains("attribute vec3 aShade;"), "aShade missing");
        assert!(shader.contains("attribute vec2 aUv;"), "aUv missing");
        assert!(shader.contains("uniform float uCamYaw;"), "uCamYaw missing");
        assert!(shader.contains("uniform vec3 uObjPos;"), "uObjPos missing");
        assert!(shader.contains("uniform vec3 uScale;"), "uScale missing");
        assert!(shader.contains("uniform vec3 uBlockColor;"), "uBlockColor missing");
        assert!(shader.contains("varying highp vec4 vColor;"), "vColor missing");
        assert!(shader.contains("varying highp vec2 vUv;"), "vUv missing");
        // seeds ×1000
        assert!(shader.contains("g_ap_x = int(aPosition.x * 1000.0);"), "ap_x seed");
        assert!(shader.contains("g_auv_u = int(aUv.x * 1000.0);"), "auv_u seed");
        assert!(shader.contains("g_ucy_m = int(uCamYaw * 1000.0);"), "ucy_m seed");
        assert!(shader.contains("g_ublk_r = int(uBlockColor.r * 1000.0);"), "ublk_r seed");
        // the float bc captures: locals are floats (the pass-1 verdict)
        assert!(shader.contains("float g_vp_x;"), "vp_x not a float local");
        assert!(shader.contains("float g_vp_w;"), "vp_w not a float local");
        assert!(shader.contains("float g_c;"), "cos var not a float local");
        assert!(shader.contains("float g_relx;"), "relx not a float local");
        // the trig: c($rad) → cos(g_rad) — and CHAINED float math
        // reads the float var directly (no float(int()) truncation)
        assert!(shader.contains("cos(g_rad)"), "bc cos emit");
        assert!(shader.contains("sin(g_rad)"), "bc sin emit");
        assert!(!shader.contains("int(g_rad)"), "float var truncated through int()");
        assert!(shader.contains("g_dx * g_c") && shader.contains("g_dz * g_s"), "chained float math");
        // float-var copies: vp_x=$relx / vp_w=$w stay direct float copies
        assert!(shader.contains("g_vp_x = (g_relx * (0.9));"), "vp_x from relx");
        assert!(shader.contains("g_vp_w = g_w;"), "float copy vp_w");
        // int outputs stay int (no cat/itos string machinery)
        assert!(shader.contains("g_ash_r * g_ublk_r") && shader.contains("g_vc_r"), "vc_r int math");
        assert!(shader.contains("g_vu_u = g_auv_u;"), "vu_u int copy");
        // the end-of-main emission
        assert!(shader.contains("gl_Position = vec4(g_vp_x, g_vp_y, g_vp_z, g_vp_w);"), "gl_Position");
        assert!(shader.contains("vColor = vec4(float(g_vc_r) / 1000.0, float(g_vc_g) / 1000.0, float(g_vc_b) / 1000.0, float(g_vc_a) / 1000.0);"), "vColor");
        assert!(shader.contains("vUv = vec2(float(g_vu_u) / 1000.0, float(g_vu_v) / 1000.0);"), "vUv");
        // no fragment machinery
        assert!(!shader.contains("gl_FragColor"), "fragment output in vertex mode");
        assert!(!shader.contains("out_buf"), "out_buf in vertex mode");
        assert!(!shader.contains("u_mode"), "u_mode in vertex mode");
        assert!(!shader.contains("texture2D"), "texture2D in vertex mode");
        assert!(!shader.contains("cat("), "string machinery in vertex mode");
        assert!(!shader.contains("itos"), "itos in vertex mode");
        // highp is REQUIRED in ES 1.00 vertex shaders (mediump gate is
        // a fragment-only concern — gl_Position math keeps precision)
        assert!(shader.contains("precision highp float;"), "vertex not highp float");
        assert!(shader.contains("precision highp int;"), "vertex not highp int");
        assert!(!shader.contains("precision mediump float"), "vertex downgraded to mediump");
        // clean compile marker
        assert!(shader.contains("TODO(unsupported): 0 construct(s)"), "unsupported constructs");
    }

    #[test]
    fn vertex_mode_bridges_use_gated() {
        // a program referencing ONLY ucy_m + the outputs gets no
        // attribute declarations and no unused seeds/uniforms.
        let shader = render_bash_opts(
            "vp_x=$ucy_m\n",
            ShGlslOptions { es100: true, color_out: false, vert_out: true, tex_size: 0, max_view: 0 },
        );
        assert!(shader.contains("uniform float uCamYaw;"), "uCamYaw missing");
        assert!(shader.contains("g_ucy_m = int(uCamYaw * 1000.0);"), "ucy_m seed");
        assert!(!shader.contains("attribute"), "attributes declared unused");
        assert!(!shader.contains("uObjPos"), "uObjPos declared unused");
        assert!(!shader.contains("uOverlay"), "uOverlay declared unused");
        assert!(!shader.contains("uCamPos"), "uCamPos declared unused");
    }
}
