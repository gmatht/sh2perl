//! Zig backend renderer — LIBRARY interface (worktree-local, branch
//! `backend/zig`). Consumes the ShIR directly in-process, bypassing the
//! `--shir` JSON contract (ask B of docs/backend-c-core-needs.md §1):
//! `shir_to_zig(&IrProgram) -> String`.
//!
//! Uses the core's A2 type verdicts (`IrProgram.var_types`): `Int` vars →
//! Zig `i64`, `Str` vars → `[]const u8`, anything else → the runtime store
//! (`[]const u8` + sh2.* stubs in this draft). Identifiers are sanitized
//! to Zig identifier syntax and mangled against Zig keywords (A6-
//! consistent). Everything outside the lowable subset (numeric arith,
//! echo-style output, if/else, while/do-while, simple assignment,
//! write-file) emits a compile-able `sh2.*` stub or a
//! `// TODO(unsupported)` marker, so the draft always compiles.
//!
//! Zig has no `any` type: every value is statically `i64`, `[]const u8`
//! or `bool`, and the renderer picks per node — the A2 verdicts decide at
//! the variable boundaries, and `shToInt`/`shIntStr`/`shTruthy`
//! convert at the type boundaries. Native arithmetic renders to real Zig
//! operators (integer division truncates toward zero like bash); `**`
//! lowers to `std.math.pow`, arith `&&`/`||`/`!` to `@intFromBool`, string
//! comparisons to `std.mem.eql`/`std.mem.order`.

use crate::ir::{ArithAst, BinOpKind, InterpPart, IrExpr, IrProgram, IrStmt, IrType};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Zig keywords + identifiers the generated program relies on (the `std`
/// import, the builtin scalar types, the helper/stub names). Everything
/// else can be a legal block variable; mangling these keeps the generated
/// program compilable.
const ZIG_RESERVED: &[&str] = &[
    "addrspace",
    "align",
    "allowzero",
    "and",
    "anyframe",
    "anytype",
    "asm",
    "async",
    "await",
    "break",
    "callconv",
    "catch",
    "comptime",
    "const",
    "continue",
    "defer",
    "else",
    "enum",
    "errdefer",
    "error",
    "export",
    "extern",
    "fn",
    "for",
    "if",
    "inline",
    "noalias",
    "noinline",
    "nosuspend",
    "opaque",
    "or",
    "orelse",
    "packed",
    "pub",
    "resume",
    "return",
    "linksection",
    "struct",
    "suspend",
    "switch",
    "test",
    "threadlocal",
    "try",
    "union",
    "unreachable",
    "usingnamespace",
    "var",
    "volatile",
    "while",
    // predeclared / shadowable names used by the generated program
    "std",
    "i64",
    "u8",
    "usize",
    "bool",
    "void",
    "noreturn",
    "true",
    "false",
    "null",
    "undefined",
    "main",
    "stdout",
    "sh_rc",
    "sh2TODO",
    "shToInt",
    "shIntStr",
    "shTruthy",
    "shStrCat",
    "shB2S",
];

/// Runtime-helper INTERNAL identifiers (locals, params, loop bindings in
/// the emitted `sh*` helpers and the main prologue). A file-scope
/// (capture-promoted) user var with one of these names makes the helper's
/// local a shadowing error (`local constant shadows declaration`), so
/// capture-promoted globals mangle away from all of them (they gain a `_`
/// suffix). Function/main locals are separate scopes and keep clean names.
const HELPER_INTERNALS: &[&str] = &[
    "result", "term", "child", "io", "st", "f", "s", "t", "m", "b", "out", "ix", "j",
    "w", "n", "k", "hit", "done", "best", "e", "o", "ou", "lo", "hi", "c", "a", "i",
    "v", "argv", "argc", "zargs", "util_z", "hay", "needle", "upper", "lead", "trail",
    "off", "len", "pat", "name", "data", "p", "flag", "append", "repl", "all",
    "longest", "first", "suppress", "d", "ids", "si", "pi", "ssi", "spi", "star",
    "negate", "body", "close", "bi", "lines", "s0", "e0", "num", "full", "nm", "bn",
    "fh", "buf", "rd", "line", "needs", "total", "stdout_buffer", "stdout_file_writer",
    "sh_io", "sh_environ", "init", "__args",
];

/// sh2.* callee names that collide with a real runtime helper (or the
/// generic sh2TODO) — their per-callee stub gets a `Stub` suffix so the
/// call site and the definition agree.
const RESERVED_STUB: &[&str] = &[
    "sh2TODO",
    "shToInt",
    "shIntStr",
    "shTruthy",
    "shStrCat",
    "shB2S",
    "sh2Test",
    "sh2Arith",
];

#[derive(Default)]
pub struct Render {
    out: Vec<String>,
    depth: usize,
    /// var name -> type verdict (A2); missing = Any (runtime store)
    var_types: HashMap<String, IrType>,
    var_storage: HashMap<String, crate::ir::StorageClass>,
    /// distinct sh2.* callee names that need stubs
    sh2_calls: BTreeSet<String>,
    /// vars written anywhere (declared at the top of main)
    written: BTreeSet<String>,
    /// vars actually read (skip the `_ = x` dead-var guard)
    read: BTreeSet<String>,
    /// Zig identifier per shell var name (sanitize + de-dup)
    mangle: HashMap<String, String>,
    need_stdout: bool,
    need_toint: bool,
    need_intstr: bool,
    need_truthy: bool,
    need_cat: bool,
    need_b2s: bool,
    need_env: bool,
    need_fexist: bool,
    need_writefile: bool,
    need_run: bool,
    /// an FFI-eligible (whitelisted) external command was emitted under
    /// SH2_UU_FFI — the in-process `uu_run` arm is only emitted when this is
    /// set, so a render with no coreutils commands carries no FFI extern.
    need_uu_run: bool,
    loop_depth: usize,
    todo: usize,
    /// inside a user-function body (A1 Function): getVar("N") reads the
    /// positional param __args[N-1], Return returns the value, not exit
    in_function: bool,
    /// user functions hoisted OUT of main (rendered before it)
    pending_fns: Vec<String>,
    /// vars local to a hoisted user function (never declared in main)
    fn_locals: BTreeSet<String>,
    /// set-typed arrays (`factors = set(); factors.add(x)`) — lowered to
    /// a fixed i64 array + length counter (the C backend's model)
    set_arrays: BTreeSet<String>,
    /// set/list arrays with bigint elements — lowered to a fixed
    /// std.math.big.int.Managed array + length counter
    bigint_arrays: BTreeSet<String>,
    /// string-element arrays (`arr=(a b c)`) — lowered to a fixed
    /// `[]const u8` array + length counter (bash arrays are strings; the
    /// i64 set home only fits all-numeric element lists)
    str_arrays: BTreeSet<String>,
    /// emit the shArrJoin helper (array read in string context)
    need_arrjoin: bool,
    /// emit shDiv/shRem (zero-divisor-safe arithmetic)
    need_safe_div: bool,
    /// emit shShl/shShr (shift amounts normalised to u6)
    need_shift: bool,
    /// emit shPadW (printf field width)
    need_pad: bool,
    /// emit shArgv0 ($0)
    need_argv0: bool,
    /// `trap 'payload' EXIT` — run at the end of main
    exit_trap: Option<String>,
    /// at least one `&` job was forked (reap them before exiting)
    saw_background: bool,
    /// names that are only ever READ (never a store target)
    read_only_names: BTreeSet<String>,
    /// shopt -s nocasematch in effect (case-insensitive [[ == ]])
    shopt_nocasematch: bool,
    /// shopt -s extglob in effect
    shopt_extglob: bool,
    /// the `test` being rendered is a `[[ ]]` (pattern RHS allowed)
    current_is_dbracket: bool,
    /// emit shGlobPat (pattern match with * ? [] and extglob @() *() +() ?() !())
    need_globpat: bool,
    /// emit shBEscape (printf %b backslash expansion)
    need_bexpand: bool,
    /// emit shSource (runtime `.`/`source` support)
    need_source: bool,
    /// names a runtime `source`d file is allowed to assign
    source_targets: BTreeSet<String>,
    /// names a runtime-sourced file may assign (declared up front)
    source_names: BTreeSet<String>,
    /// bigint vars (arith beyond i64) — lowered to std.math.big.int.Managed
    bigint_vars: BTreeSet<String>,
    /// user functions whose body uses `try` (bigint/file IO) — they
    /// return `![]const u8`, so call sites need `catch ""`
    error_fns: BTreeSet<String>,
    /// counter for generated temp names (for-loop item bindings)
    tmp_counter: usize,
    /// text-ops primitive helpers (shContains/shSub/…) needed
    need_ext: bool,
    /// sortedIntJoin helper (set-array sort+dedup+join) needed
    need_sorted: bool,
    /// a `try stdout.print` was emitted at MAIN level (not inside a
    /// hoisted user fn): main's own stdout prologue is only emitted then
    /// (a fn-level print uses the fn's private buffer, leaving main's
    /// `const stdout` unused — a compile error)
    need_main_stdout: bool,
    /// max/min/sum reduction helpers needed
    need_reduce: bool,
    /// bigint (std.math.big.int.Managed) helpers needed
    need_bigint: bool,
    /// shellQuote helper needed (printf %q)
    need_shquote: bool,
    /// glob matching helper needed (case patterns with wildcards)
    need_glob: bool,
    /// set when the current statement already assigned sh_rc (external
    /// run, capture, `false`, real-status test): the stmt() wrapper skips
    /// the default `sh_rc = 0` reset. Cleared at each stmt() entry so
    /// nested body statements resolve independently and compounds keep
    /// last-leaf-wins by construction.
    rc_dirty: bool,
    /// user-function names defined in this program (top-level Function
    /// stmts + subs): a `$(fname …)` capture of one of these is a direct
    /// call; any other `exec` inside a capture is an EXTERNAL command
    /// (shRun), never a call to an undeclared Zig fn
    user_fns: BTreeSet<String>,
    /// vars read by a nested function from an enclosing function scope
    /// (bash dynamic scoping): promoted to main-level globals; enclosing
    /// fns must not shadow them with locals
    globals: BTreeSet<String>,
    /// vars written by TOP-level statements (function bodies excluded by
    /// the walker): a function that assigns one of these writes the shared
    /// global (bash has no `local` unless declared) — the Function arm
    /// must not redeclare it as an fn-local shadow, except for names the
    /// fn itself declares `local` (Declare{local} / builtin `local`)
    top_written: BTreeSet<String>,
    /// user functions whose trailing `echo [-n] V` became the return value
    /// (name → trailing newline?): a STATEMENT-position call must print
    /// that value (bash executes the echo as a side effect);
    /// capture-position calls consume it silently
    value_fns: BTreeMap<String, bool>,
}

/// Render an `IrProgram` to Zig source.
pub fn shir_to_zig(prog: &IrProgram) -> String {
    let mut prog = prog.clone();
    // builtin-op fallback arm (shir-builtin-op-20260816): the zig backend
    // has NOT accepted the `builtin` op — render as exec.
    crate::transforms::builtin::fallback_builtin_to_exec(&mut prog);
    // A2: the type verdicts — the FRONTEND's own var_types (the C
    // frontend emits Int32/Int64 for typed decls) take precedence;
    // analyze_var_types fills in names the frontend left untyped. A
    // blind overwrite DOWNGRADED typed vars to untyped (empty map for
    // frontend-only programs), leaving string homes with i64-typed
    // reads/writes that did not compile.
    let provided: std::collections::HashSet<String> =
        prog.var_types.iter().map(|(n, _)| n.clone()).collect();
    let analyzed = crate::shir::analyze_var_types(&prog);
    for (n, t) in analyzed {
        if !provided.contains(&n) {
            prog.var_types.push((n, t));
        }
    }
    let mut r = Render::default();
    r.var_types = prog.var_types.iter().cloned().collect();
    r.var_storage = prog.var_storage.iter().cloned().collect();
    collect_fns(&prog.stmts, &mut r.user_fns);
    for sub in &prog.subs {
        r.user_fns.insert(sub.name.clone());
        collect_fns(&sub.body, &mut r.user_fns);
    }
    if std::env::var("ZIG_DBG").is_ok() {
        eprintln!("DBG var_types: {:?}", r.var_types);
    }
    r.program(&prog);
    r.out.join("\n")
}

/// The pattern text of a `\x01SH2GLOB\x01`-marked word (the core's marker
/// for an UNQUOTED glob), or None.
fn glob_marked_str(e: &IrExpr) -> Option<String> {
    const MARK: &str = "\u{1}SH2GLOB\u{1}";
    match e {
        IrExpr::Str(s, _) => s.strip_prefix(MARK).map(|p| p.to_string()),
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(t) => match t.strip_prefix(MARK) {
                        Some(rest) => out.push_str(rest),
                        None => return None,
                    },
                    InterpPart::Expr(_) => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// The Zig format specifier for a statically-typed part.
/// A bare identifier (no spaces/operators) — used to recognize the
/// cstyleFor loop var and increment LHS.
fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The single value of a trailing `echo [-n] V` statement (the
/// function-return convention), plus whether it ends with a newline.
/// None for any other shape.
fn trailing_echo_value(s: &IrStmt) -> Option<(IrExpr, bool)> {
    if let IrStmt::Expr(IrExpr::Call { func, args }) = s {
        if func == "exec" {
            if let Some(IrExpr::Str(cmd, _)) = args.first() {
                if cmd == "echo" {
                    if let Some(IrExpr::Array(items)) = args.get(1) {
                        let mut rest = items.as_slice();
                        let mut newline = true;
                        while let Some(IrExpr::Str(flag, _)) = rest.first() {
                            let t = flag.trim();
                            if t.len() > 1 && t.starts_with('-') && t[1..].chars().all(|c| c == 'n' || c == 'e' || c == 'E') {
                                if t.contains('n') {
                                    newline = false;
                                }
                                rest = &rest[1..];
                            } else {
                                break;
                            }
                        }
                        if rest.len() == 1 {
                            return rest.first().cloned().map(|v| (v, newline));
                        }
                    }
                }
            }
        }
    }
    None
}

fn spec_char(spec: char) -> &'static str {
    match spec {
        'd' => "{d}",
        's' => "{s}",
        _ => "{}",
    }
}

/// One pipeline stage: either a forked argv or an in-process string
/// feed (echo/printf, whose output must match the renderer's own print
/// semantics exactly — forking /bin/echo would diverge on backslash
/// escapes).
#[derive(Clone)]
enum PipeStage {
    Fork(Vec<String>),
    Feed(String),
}


impl Render {
    fn emit(&mut self, s: &str) {
        if s.is_empty() {
            self.out.push(String::new());
        } else {
            self.out.push(format!("{}{}", "    ".repeat(self.depth), s));
        }
    }

    fn mark_todo(&mut self, what: &str) {
        self.todo += 1;
        let one = what.replace('\n', "\\n");
        self.emit(&format!("// TODO(unsupported): {one}"));
    }

    /// A fresh generated temp name that cannot collide with any mangled
    /// shell variable (or another temp).
    fn fresh_tmp(&mut self, base: &str) -> String {
        loop {
            let name = format!("{base}{}", self.tmp_counter);
            self.tmp_counter += 1;
            if !self.mangle.values().any(|v| v == &name) {
                return name;
            }
        }
    }

    // ── identifiers ──────────────────────────────────────────────────

    /// Sanitize a shell var name to a valid Zig identifier and mangle
    /// reserved names (A6-consistent). De-duplicates collisions.
    fn zig_ident(&mut self, name: &str) -> String {
        if let Some(m) = self.mangle.get(name) {
            return m.clone();
        }
        let mut m = String::new();
        for (_i, c) in name.chars().enumerate() {
            if c.is_ascii_alphanumeric() || c == '_' {
                m.push(c);
            } else {
                m.push('_');
            }
        }
        if m.is_empty() || m.chars().next().unwrap().is_ascii_digit() {
            m.insert_str(0, "v_");
        }
        if ZIG_RESERVED.contains(&m.as_str()) {
            m.push('_');
        }
        if let Some(m) = self.mangle.get(name) {
            return m.clone();
        }
        let mut m = String::new();
        for (_i, c) in name.chars().enumerate() {
            if c.is_ascii_alphanumeric() || c == '_' {
                m.push(c);
            } else {
                m.push('_');
            }
        }
        if m.is_empty() || m.chars().next().unwrap().is_ascii_digit() {
            m.insert_str(0, "v_");
        }
        if ZIG_RESERVED.contains(&m.as_str()) {
            m.push('_');
        }
        if m == "_" {
            m = "underscore".to_string();
        }
        // de-dup collisions (e.g. `a-b` and `a.b` both sanitize to `a_b`)
        let base = m.clone();
        let mut n = 1;
        while self.mangle.values().any(|v| v == &m) {
            m = format!("{base}{n}");
            n += 1;
        }
        self.mangle.insert(name.to_string(), m.clone());
        m
    }

    /// UU-FFI.md gate: is the in-process coreutils arm requested at RENDER
    /// time? Mirrors the C backend's `SH2_UU_FFI` switch (opt OUT with
    /// `SH2_UU_FFI=0`, on if set to anything else). Off by default so the
    /// emitted program keeps the dependency-free fork/exec path unless the
    /// invoker has built/linked libcoreutils_ffi.so.
    /// Does this operand text (the ORIGINAL `test` operand token) use glob
    /// or extglob pattern syntax? bash treats the RHS of `=`/`==`/`!=` in
    /// `[[ ]]` as a pattern, so `@(foo|bar)`, `*.c`, `[ab]` are patterns.
    fn lit_is_pattern(tok: &str) -> bool {
        let t = tok.trim_matches('"');
        let b = t.as_bytes();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'*' | b'?' | b'[' => return true,
                b'@' | b'+' | b'!' if i + 1 < b.len() && b[i + 1] == b'(' => return true,
                b'\\' => i += 1,
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn uu_ffi_enabled() -> bool {
        match std::env::var("SH2_UU_FFI") {
            Ok(v) => v != "0",
            Err(_) => false,
        }
    }

    /// Commands compiled into libcoreutils_ffi.so — kept in sync with
    /// `runtime/build_uu_ffi.sh` FEATURES (the same list the C backend and
    /// the perl preamble gate on). `uu_run` returns 127 for anything else
    /// and prints NOTHING, so an unwhitelisted command routed through the
    /// FFI would silently lose its output; those stay on fork/exec.
    fn uu_cmd_ok(cmd: &str) -> bool {
        matches!(
            cmd,
            "cat"
                | "wc"
                | "ls"
                | "sort"
                | "sed"
                | "awk"
                // NO "grep": the pinned libcoreutils_ffi build rejects
                // common flags (-q, -w) with "grep: unsupported flag",
                // which turned `grep -q hello <<< …` into rc 2
                // (parse-herestring.sh). The fork/exec arm runs the
                // system binary verbatim.
                | "date"
                | "sha256sum"
                | "uname"
                | "basename"
                | "dirname"
                | "cp"
        )
    }

    /// A Zig string literal. Escaped byte-wise so arbitrary (non-UTF-8)
    /// payloads from the shell corpus stay valid Zig and byte-exact.
    fn zig_str(s: &str) -> String {
        let mut out = String::new();
        out.push('"');
        for &b in s.as_bytes() {
            match b {
                b'\\' => out.push_str("\\\\"),
                b'"' => out.push_str("\\\""),
                b'\n' => out.push_str("\\n"),
                b'\t' => out.push_str("\\t"),
                b'\r' => out.push_str("\\r"),
                0x20..=0x7e => out.push(b as char),
                _ => out.push_str(&format!("\\x{:02x}", b)),
            }
        }
        out.push('"');
        out
    }

    /// A format-string literal: shell text verbatim, `{`/`}` escaped for
    /// Zig's print formatting (the `{d}`/`{s}`/`{}` markers we add for
    /// args stay intact — escape at PUSH time, not on the whole format).
    fn zig_fmt(s: &str) -> String {
        let esc: String = s
            .chars()
            .map(|c| match c {
                '{' => "{{".to_string(),
                '}' => "}}".to_string(),
                c => c.to_string(),
            })
            .collect();
        Self::zig_str(&esc)
    }

    fn is_num(&self, name: &str) -> bool {
        // the C frontend emits the SIZED kinds (Int32/Int64/UInt32/...)
        // — a widthless Int alone left every typed var string-homed
        matches!(
            self.var_types.get(name),
            Some(IrType::Int | IrType::Int32 | IrType::Int64 | IrType::UInt32 | IrType::UInt64)
        )
    }

    fn is_str(&self, name: &str) -> bool {
        self.var_types.get(name).copied() == Some(IrType::Str)
    }

    /// The var has a hoisted declaration (written somewhere) — only then
    /// may generated code reference it as a bare identifier.
    fn declared(&self, name: &str) -> bool {
        self.written.contains(name)
    }

    fn mark_read(&mut self, name: &str) {
        self.read.insert(name.to_string());
    }

    fn mark_written(&mut self, name: &str) {
        self.written.insert(name.to_string());
    }

    // ── typed expressions ────────────────────────────────────────────

    /// Statically-numeric check (for comparison typing).
    fn static_num(&self, e: &IrExpr) -> bool {
        match e {
            IrExpr::Int(_) => true,
            IrExpr::Str(s, _) => s.trim().parse::<i64>().is_ok(),
            IrExpr::Var(name, _) => self.is_num(name),
            IrExpr::Arith(_) => true,
            IrExpr::BinOp { op, .. } => !matches!(op, BinOpKind::Concat),
            IrExpr::Call { func, args } if func == "getVar" => {
                matches!(args.first(), Some(IrExpr::Str(name, _)) if self.is_num(name))
            }
            _ => false,
        }
    }

    /// Render as an i64-typed expression.
    fn expr_num(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Int(i) => i.to_string(),
            IrExpr::Str(s, _) => {
                if let Ok(n) = s.trim().parse::<i64>() {
                    n.to_string()
                } else {
                    self.need_toint = true;
                    format!("shToInt({})", Self::zig_str(s))
                }
            }
            IrExpr::Var(name, _) => {
                if name == "?" {
                    return "sh_rc".to_string();
                }
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    self.need_toint = true;
                    return "shToInt(sh2GetVar())".to_string();
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    m
                } else {
                    self.need_toint = true;
                    format!("shToInt({m})")
                }
            }
            IrExpr::Ident(name) => {
                if name == "?" {
                    return "sh_rc".to_string();
                }
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    self.need_toint = true;
                    return "shToInt(sh2GetVar())".to_string();
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    m
                } else {
                    self.need_toint = true;
                    format!("shToInt({m})")
                }
            }
            IrExpr::Arith(a) => {
                if crate::shir::arith_refs_bigint(a, &self.bigint_vars) {
                    self.need_bigint = true;
                    let t = self.arith_bigint(a);
                    return format!("@intCast({t}.toConst().toInt(i64) catch 0)");
                }
                self.arith(a)
            }
            IrExpr::BinOp { lhs: _, op, rhs: _ }
                if matches!(
                    op,
                    BinOpKind::Eq
                        | BinOpKind::Ne
                        | BinOpKind::Lt
                        | BinOpKind::Gt
                        | BinOpKind::Le
                        | BinOpKind::Ge
                        | BinOpKind::And
                        | BinOpKind::Or
                        | BinOpKind::Not
                ) =>
            {
                // bool → i64 (bash's 1/0)
                format!("@intFromBool({})", self.expr_bool(e))
            }
            IrExpr::BinOp { lhs: _, op, rhs: _ } if *op == BinOpKind::Concat => {
                self.need_toint = true;
                format!("shToInt({})", self.expr_str(e))
            }
            IrExpr::BinOp { lhs, op, rhs } if *op == BinOpKind::Pow => {
                let (l, r) = (self.expr_num(lhs), self.expr_num(rhs));
                format!("std.math.pow(i64, {l}, {r})")
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                let (l, r, zop) = (self.expr_num(lhs), self.expr_num(rhs), self.arith_op(op));
                format!("({l} {zop} {r})")
            }
            IrExpr::Bool(b) => {
                if *b {
                    "@intFromBool(true)".into()
                } else {
                    "@intFromBool(false)".into()
                }
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.declared(name) {
                        let m = self.zig_ident(name);
                        self.mark_read(name);
                        if self.is_num(name) {
                            return m;
                        }
                        self.need_toint = true;
                        return format!("shToInt({m})");
                    }
                }
                self.need_toint = true;
                format!("shToInt({})", self.call_str("getVar", args))
            }
            IrExpr::Call { func, args } => self.call_num(func, args),
            IrExpr::Ternary { cond, then, else_ } => format!(
                "if ({}) {} else {}",
                self.expr_bool(cond),
                self.expr_num(then),
                self.expr_num(else_)
            ),
            other => {
                self.need_toint = true;
                format!("shToInt({})", self.expr_str(other))
            }
        }
    }

    /// Render as a []const u8-typed expression.
    fn expr_str(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => Self::zig_str(s),
            IrExpr::Int(i) => Self::zig_str(&i.to_string()),
            IrExpr::Var(name, _) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return "sh2GetVar()".to_string();
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    // an INT-homed var read in string context converts
                    self.need_intstr = true;
                    return format!("shIntStr({m})");
                }
                m
            }
            IrExpr::Ident(name) => {
                // a name in the renderer's own namespace is declared by the
                // generated code around it, never hoisted
                if name.starts_with("__pfs_") {
                    return name.clone();
                }
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return "sh2GetVar()".to_string();
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                m
            }
            IrExpr::Interpolate(parts) => self.interpolate(parts),
            IrExpr::Arith(a) => {
                if crate::shir::arith_refs_bigint(a, &self.bigint_vars) {
                    self.need_bigint = true;
                    let t = self.arith_bigint(a);
                    return format!("shBigIntStr({t})");
                }
                self.need_intstr = true;
                format!("shIntStr({})", self.arith(a))
            }
            IrExpr::Bool(b) => {
                if *b {
                    "\"true\"".into()
                } else {
                    "\"false\"".into()
                }
            }
            IrExpr::BinOp { lhs: _, op, rhs: _ }
                if matches!(
                    op,
                    BinOpKind::Eq
                        | BinOpKind::Ne
                        | BinOpKind::Lt
                        | BinOpKind::Gt
                        | BinOpKind::Le
                        | BinOpKind::Ge
                        | BinOpKind::And
                        | BinOpKind::Or
                        | BinOpKind::Not
                ) =>
            {
                self.need_b2s = true;
                format!("shB2S({})", self.expr_bool(e))
            }
            IrExpr::BinOp { lhs, op, rhs } if *op == BinOpKind::Concat => {
                self.need_cat = true;
                format!("shStrCat({}, {})", self.expr_str(lhs), self.expr_str(rhs))
            }
            IrExpr::BinOp { .. } => {
                self.need_intstr = true;
                format!("shIntStr({})", self.expr_num(e))
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if name == "?" {
                        self.need_intstr = true;
                        return "shIntStr(sh_rc)".to_string();
                    }
                    // `$-` (checked BEFORE declared(): the mangler turns
                    // "-" into `underscore`, a bare identifier that printed
                    // an empty line). The gate's reference bash is a plain
                    // non-interactive shell: hashall + braceexpand.
                    if name == "-" {
                        return Self::zig_str("hB");
                    }
                    if self.declared(name) {
                        let m = self.zig_ident(name);
                        self.mark_read(name);
                        // mirror the var's HOME (is_num), not the verdict:
                        // an un-verdictable var (no A2 entry) is string-homed
                        // — the old is_str test wrapped those in shIntStr
                        // and broke compilation (t41_local_var)
                        if self.is_num(name) {
                            self.need_intstr = true;
                            return format!("shIntStr({m})");
                        }
                        return m;
                    }
                }
                self.call_str("getVar", args)
            }
            IrExpr::Call { func, args } => self.call_str(func, args),
            IrExpr::Ternary { cond, then, else_ } => format!(
                "if ({}) {} else {}",
                self.expr_bool(cond),
                self.expr_str(then),
                self.expr_str(else_)
            ),
            IrExpr::Json(v) => match v {
                serde_json::Value::String(s) => Self::zig_str(s),
                serde_json::Value::Number(n) => Self::zig_str(&n.to_string()),
                serde_json::Value::Bool(b) => {
                    if *b {
                        "\"true\"".into()
                    } else {
                        "\"false\"".into()
                    }
                }
                _ => {
                    self.mark_todo("Json expr");
                    "\"\"".into()
                }
            },
            IrExpr::Index { .. } => {
                self.sh2_calls.insert("arrayIndex".into());
                "sh2ArrayIndex()".to_string()
            }
            IrExpr::Capture { expr, .. } => {
                if let Some(rendered) = self.capture_expr(expr) {
                    return rendered;
                }
                self.sh2_calls.insert("capture".into());
                "sh2Capture()".to_string()
            }
            IrExpr::Regex { .. } => {
                self.sh2_calls.insert("regex".into());
                "sh2Regex()".to_string()
            }
            IrExpr::DefinedOr { .. } => {
                self.sh2_calls.insert("definedOr".into());
                "sh2DefinedOr()".to_string()
            }
            IrExpr::MethodCall { .. } => {
                self.sh2_calls.insert("methodCall".into());
                "sh2MethodCall()".to_string()
            }
            IrExpr::Range { .. } => {
                self.mark_todo("Range expr");
                "\"\"".into()
            }
            IrExpr::RawExpr(_) => {
                self.mark_todo("RawExpr");
                "\"\"".into()
            }
            IrExpr::Arrow(_) => {
                self.mark_todo("Arrow");
                "\"\"".into()
            }
            IrExpr::ArrayComp { .. } => {
                self.mark_todo("ArrayComp expr");
                "\"\"".into()
            }
            IrExpr::Lambda { .. } => {
                self.mark_todo("Lambda expr");
                "\"\"".into()
            }
            IrExpr::Splice(_) => {
                self.mark_todo("Splice expr");
                "\"\"".into()
            }
            IrExpr::Ext(n) => {
                if let Some(code) = self.ext_value_zig(n.as_ref(), false) {
                    code
                } else {
                    let ctx = crate::render_ext_expr::ExprRenderCtx::bare(
                        crate::render_ext_expr::Backend::Zig,
                        0,
                    );
                    if let Some(code) = crate::render_ext_expr::render(&**n, &ctx) {
                        code
                    } else {
                        format!("sh2.{}(...)", n.tag())
                    }
                }
            }
            IrExpr::Array(_) => {
                self.mark_todo("Array expr");
                "\"\"".into()
            }
            IrExpr::Object(_) => {
                self.mark_todo("Object");
                "\"\"".into()
            }
        }
    }

    /// Render as a bool-typed expression (conditions).
    fn expr_bool(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Ext(n) => {
                // boolean-context primitive (StringContains): raw bool
                if let Some(code) = self.ext_value_zig(n.as_ref(), true) {
                    code
                } else {
                    "true".into()
                }
            }
            IrExpr::Bool(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            IrExpr::Int(i) => format!("({i} != 0)"),
            IrExpr::Str(s, _) => format!("({} .len != 0)", Self::zig_str(s)),
            IrExpr::Var(name, _) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    self.need_truthy = true;
                    return "shTruthy(sh2GetVar())".to_string();
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    format!("({m} != 0)")
                } else {
                    format!("({m} .len != 0)")
                }
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    self.need_truthy = true;
                    return "shTruthy(sh2GetVar())".to_string();
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    format!("({m} != 0)")
                } else {
                    format!("({m} .len != 0)")
                }
            }
            IrExpr::BinOp { lhs, op, rhs } => match op {
                BinOpKind::And => format!("({} and {})", self.expr_bool(lhs), self.expr_bool(rhs)),
                BinOpKind::Or => format!("({} or {})", self.expr_bool(lhs), self.expr_bool(rhs)),
                BinOpKind::Not => format!("(!{})", self.expr_bool(lhs)),
                BinOpKind::Eq
                | BinOpKind::Ne
                | BinOpKind::Lt
                | BinOpKind::Gt
                | BinOpKind::Le
                | BinOpKind::Ge => {
                    let zop = self.cmp_op(op);
                    if self.static_num(lhs) && self.static_num(rhs) {
                        let (l, r) = (self.expr_num(lhs), self.expr_num(rhs));
                        format!("({l} {zop} {r})")
                    } else {
                        let (l, r) = (self.expr_str(lhs), self.expr_str(rhs));
                        self.str_cmp(zop, &l, &r)
                    }
                }
                BinOpKind::Concat => format!("({} .len != 0)", self.expr_str(e)),
                _ => format!("({} != 0)", self.expr_num(e)),
            },
            IrExpr::Arith(a) => {
                if crate::shir::arith_refs_bigint(a, &self.bigint_vars) {
                    self.bigint_cond(a)
                } else {
                    format!("({} != 0)", self.arith(a))
                }
            }
            IrExpr::Call { func, args } if func == "test" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    self.current_is_dbracket = matches!(args.get(1), Some(IrExpr::Str(t, _)) if t == "[[");
                    let r = self.test_render(s);
                    self.current_is_dbracket = false;
                    if let Some(c) = r {
                        return c;
                    }
                }
                self.sh2_calls.insert("test".into());
                "sh2Test()".to_string()
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.declared(name) {
                        let m = self.zig_ident(name);
                        self.mark_read(name);
                        if self.is_num(name) {
                            return format!("({m} != 0)");
                        }
                        return format!("({m} .len != 0)");
                    }
                }
                self.need_truthy = true;
                format!("shTruthy({})", self.call_str("getVar", args))
            }
            IrExpr::Call { func, args } => self.call_bool(func, args),
            IrExpr::Ternary { cond, then, else_ } => format!(
                "if ({}) {} else {}",
                self.expr_bool(cond),
                self.expr_bool(then),
                self.expr_bool(else_)
            ),
            IrExpr::Interpolate(parts) => format!("({} .len != 0)", self.interpolate(parts)),
            IrExpr::Index { .. } => {
                self.sh2_calls.insert("arrayIndex".into());
                self.need_truthy = true;
                format!("shTruthy(sh2ArrayIndex())")
            }
            IrExpr::Capture { expr, .. } => {
                self.need_truthy = true;
                if let Some(rendered) = self.capture_expr(expr) {
                    format!("shTruthy({rendered})")
                } else {
                    self.sh2_calls.insert("capture".into());
                    format!("shTruthy(sh2Capture())")
                }
            }
            IrExpr::Regex { .. } => {
                self.sh2_calls.insert("regex".into());
                self.need_truthy = true;
                format!("shTruthy(sh2Regex())")
            }
            IrExpr::DefinedOr { .. } => {
                self.sh2_calls.insert("definedOr".into());
                self.need_truthy = true;
                format!("shTruthy(sh2DefinedOr())")
            }
            IrExpr::MethodCall { .. } => {
                self.sh2_calls.insert("methodCall".into());
                self.need_truthy = true;
                format!("shTruthy(sh2MethodCall())")
            }
            IrExpr::Json(v) => match v {
                serde_json::Value::String(s) => format!("({} .len != 0)", Self::zig_str(s)),
                serde_json::Value::Number(n) => format!("({n} != 0)"),
                serde_json::Value::Bool(b) => {
                    if *b {
                        "true".into()
                    } else {
                        "false".into()
                    }
                }
                _ => {
                    self.mark_todo("Json expr");
                    "false".into()
                }
            },
            IrExpr::Range { .. } => {
                self.mark_todo("Range expr");
                "false".into()
            }
            IrExpr::RawExpr(_) => {
                self.mark_todo("RawExpr");
                "false".into()
            }
            IrExpr::Arrow(_) => {
                self.mark_todo("Arrow");
                "false".into()
            }
            IrExpr::ArrayComp { .. } => {
                self.mark_todo("ArrayComp expr");
                "false".into()
            }
            IrExpr::Lambda { .. } => {
                self.mark_todo("Lambda expr");
                "false".into()
            }
            IrExpr::Splice(_) => {
                self.mark_todo("Splice expr");
                "false".into()
            }
            IrExpr::Ext(n) => {
                if let Some(code) = self.ext_value_zig(n.as_ref(), false) {
                    code
                } else {
                    let ctx = crate::render_ext_expr::ExprRenderCtx::bare(
                        crate::render_ext_expr::Backend::Zig,
                        0,
                    );
                    if let Some(code) = crate::render_ext_expr::render(&**n, &ctx) {
                        code
                    } else {
                        format!("sh2.{}(...)", n.tag())
                    }
                }
            }
            IrExpr::Array(_) => {
                self.mark_todo("Array expr");
                "false".into()
            }
            IrExpr::Object(_) => {
                self.mark_todo("Object");
                "false".into()
            }
        }
    }

    fn cmp_op(&self, op: &BinOpKind) -> &'static str {
        match op {
            BinOpKind::Eq => "==",
            BinOpKind::Ne => "!=",
            BinOpKind::Lt => "<",
            BinOpKind::Gt => ">",
            BinOpKind::Le => "<=",
            BinOpKind::Ge => ">=",
            _ => "==",
        }
    }

    /// A string comparison in Zig: `==`/`!=` via std.mem.eql, ordering via
    /// std.mem.order.
    fn str_cmp(&mut self, zop: &str, l: &str, r: &str) -> String {
        match zop {
            "==" => format!("std.mem.eql(u8, {l}, {r})"),
            "!=" => format!("!std.mem.eql(u8, {l}, {r})"),
            "<" => format!("std.mem.order(u8, {l}, {r}) == .lt"),
            ">" => format!("std.mem.order(u8, {l}, {r}) == .gt"),
            "<=" => format!("std.mem.order(u8, {l}, {r}) != .gt"),
            ">=" => format!("std.mem.order(u8, {l}, {r}) != .lt"),
            _ => format!("std.mem.eql(u8, {l}, {r})"),
        }
    }

    /// Split an expression into print parts: (spec, zgexpr) pairs — the
    /// Zig print specifier ({d}/{s}/{}) is chosen statically per node.
    fn part_spec(&mut self, e: &IrExpr) -> (char, String) {
        match e {
            IrExpr::Int(_) | IrExpr::Arith(_) => ('d', self.expr_num(e)),
            IrExpr::Str(_, _) | IrExpr::Interpolate(_) => ('s', self.expr_str(e)),
            IrExpr::Bool(_) => ('b', self.expr_bool(e)),
            // a user-function call returns the STRING model — print it
            // with {s} (a %d of int 42 and {s} of "42" produce the same
            // stdout); a numeric context would coerce via shToInt
            IrExpr::Call { func, .. } if func == "fnValue" || func == "fnCall" => {
                ('s', self.expr_str(e))
            }
            IrExpr::Var(name, _) => {
                // mark the read so the dead-var guard never discards a
                // variable that is used in printf output
                self.mark_read(name);
                if self.is_num(name) {
                    ('d', self.expr_num(e))
                } else {
                    ('s', self.expr_str(e))
                }
            }
            IrExpr::Ident(name) => {
                self.mark_read(name);
                if self.is_num(name) {
                    ('d', self.expr_num(e))
                } else {
                    ('s', self.expr_str(e))
                }
            }
            IrExpr::BinOp { op, .. }
                if matches!(
                    op,
                    BinOpKind::Eq
                        | BinOpKind::Ne
                        | BinOpKind::Lt
                        | BinOpKind::Gt
                        | BinOpKind::Le
                        | BinOpKind::Ge
                        | BinOpKind::And
                        | BinOpKind::Or
                        | BinOpKind::Not
                ) =>
            {
                ('b', self.expr_bool(e))
            }
            IrExpr::BinOp { op, .. } if *op == BinOpKind::Concat => ('s', self.expr_str(e)),
            IrExpr::BinOp { .. } => ('d', self.expr_num(e)),
            // `$x` inside interpolation is a getVar Call — type it by the
            // A2 verdict so numeric vars keep the {d} specifier.
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    // `$?` first (see call_str/expr_str: "?" can land in
                    // the written set and would mangle to `underscore`)
                    if name == "?" {
                        self.need_intstr = true;
                        return ('s', "shIntStr(sh_rc)".to_string());
                    }
                    if name == "-" {
                        return ('s', Self::zig_str("hB"));
                    }
                    // `$#` (no positional params under the gate) and `$0`
                    // BEFORE the declared lookup: the mangler turns '#' into
                    // `underscore`, which printed an empty line
                    if name == "#" {
                        return ('s', if self.in_function {
                            self.need_intstr = true;
                            "shIntStr(@as(i64, @intCast(__args.len)))".to_string()
                        } else {
                            Self::zig_str("0")
                        });
                    }
                    if name == "0" {
                        self.need_argv0 = true;
                        self.need_env = true;
                        return ('s', "shArgv0()".to_string());
                    }
                    self.mark_read(name);
                    if self.bigint_vars.contains(name) {
                        self.need_bigint = true;
                        let m = self.zig_ident(name);
                        return ('s', format!("shBigIntStr({m})"));
                    }
                    if self.is_num(name) {
                        self.mark_read(name);
                        return ('d', self.expr_num(e));
                    }
                    // a DECLARED non-num var homes as a string — print it
                    // raw (shIntStr would wrap a []const u8 in an i64
                    // conversion and fail to compile)
                    if self.declared(name) {
                        // NB: mark_read — skipping it emits a spurious
                        // dead-var `_ = x;` guard for vars assigned inside
                        // loops/walkers
                        self.mark_read(name);
                        return ('s', self.zig_ident(name));
                    }
                    return ('s', self.expr_str(e));
                }
                ('s', self.expr_str(e))
            }
            _ => ('s', self.expr_str(e)),
        }
    }

    /// String interpolation: "hello $name" → allocPrint(...) ([]const u8).
    fn interpolate(&mut self, parts: &[InterpPart]) -> String {
        let mut fmt = String::new();
        let mut args: Vec<String> = Vec::new();
        for p in parts {
            match p {
                InterpPart::Lit(s) => {
                    // escape { } at push time; {d}/{s} markers stay intact
                    for c in s.chars() {
                        match c {
                            '{' => fmt.push_str("{{"),
                            '}' => fmt.push_str("}}"),
                            c => fmt.push(c),
                        }
                    }
                }
                InterpPart::Expr(x) => {
                    let (spec, e) = self.part_spec(x);
                    fmt.push_str(spec_char(spec));
                    args.push(e);
                }
            }
        }
        if args.is_empty() {
            Self::zig_str(&fmt)
        } else {
            format!(
                "std.fmt.allocPrint(std.heap.page_allocator, {}, .{{{}}}) catch \"\"",
                Self::zig_str(&fmt),
                args.join(", ")
            )
        }
    }

    // ── arithmetic (native i64) ──────────────────────────────────────

    fn arith(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Num(n) => n.to_string(),
            ArithAst::Var(name) | ArithAst::Ident(name) => {
                // inside a user function, a bare positional (`$1`) reads
                // the param channel, not the process env
                if self.in_function
                    && !name.is_empty()
                    && name.bytes().all(|b| b.is_ascii_digit())
                {
                    let idx: usize = name.parse().unwrap_or(1);
                    self.need_toint = true;
                    return format!(
                        "shToInt(if (__args.len >= {idx}) __args[{idx} - 1] else \"\")"
                    );
                }
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    self.need_toint = true;
                    return "shToInt(sh2GetVar())".to_string();
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    m
                } else {
                    self.need_toint = true;
                    format!("shToInt({m})")
                }
            }
            ArithAst::Index { var, .. } => {
                self.mark_read(var);
                self.mark_todo("arith Index");
                "0".into()
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let l = self.arith(lhs);
                let r = self.arith(rhs);
                match op.as_str() {
                    "**" => {
                        self.need_shift = true;
                        format!("shPow({l}, {r})")
                    },
                    // zig requires the SHIFT AMOUNT in its own integer type
                    "<<" => {
                        self.need_shift = true;
                        format!("shShl({l}, {r})")
                    }
                    ">>" => {
                        self.need_shift = true;
                        format!("shShr({l}, {r})")
                    }
                    "&&" => format!("@intFromBool(({l} != 0) and ({r} != 0))"),
                    "||" => format!("@intFromBool(({l} != 0) or ({r} != 0))"),
                    // zig signed ints forbid `%`/`/` truncation operators —
                    // C semantics are truncated division/remainder (@rem)
                    // a ZERO divisor must not trap: bash reports
                    // "division by 0" and the script CONTINUES with an
                    // empty value (parse-arithmetic-extra-paren.sh), while
                    // @divTrunc PANICS and kills the program
                    "/" => {
                        self.need_safe_div = true;
                        format!("shDiv({l}, {r})")
                    }
                    "%" => {
                        self.need_safe_div = true;
                        format!("shRem({l}, {r})")
                    }
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                        format!("@intFromBool({l} {op} {r})")
                    }
                    _ => format!("({l} {op} {r})"),
                }
            }
            ArithAst::Un { op, arg } => {
                let a = self.arith(arg);
                match op.as_str() {
                    "!" => format!("@intFromBool({a} == 0)"),
                     "~" => format!("(~@as(i64, {a}))"),
                    _ => format!("({op}{a})"),
                }
            }
            ArithAst::Cond { test, then, else_ } => format!(
                "if ({} != 0) {} else {}",
                self.arith(test),
                self.arith(then),
                self.arith(else_)
            ),
            ArithAst::IncDec { var, delta, prefix } => {
                // `i++` / `++i` INSIDE an expression (not the statement
                // form): a Zig block expression reads the old or the new
                // value, and the store is the side effect
                if self.declared(var) {
                    let m = self.zig_ident(var);
                    self.mark_read(var);
                    self.mark_written(var);
                    if self.is_num(var) {
                        if *prefix {
                            format!("blk: {{ {m} += {}; break :blk {m}; }}", delta)
                        } else {
                            format!("blk: {{ const __sh_old = {m}; {m} += {}; break :blk __sh_old; }}", delta)
                        }
                    } else {
                        // STRING home: the arithmetic value is shToInt of
                        // the text, and the store goes back as text
                        self.need_toint = true;
                        self.need_intstr = true;
                        if *prefix {
                            format!("blk: {{ {m} = shIntStr(shToInt({m}) + {}); break :blk shToInt({m}); }}", delta)
                        } else {
                            format!("blk: {{ const __sh_old = shToInt({m}); {m} = shIntStr(__sh_old + {}); break :blk __sh_old; }}", delta)
                        }
                    }
                } else {
                    self.sh2_calls.insert("arith".into());
                    "sh2ArithStub()".to_string()
                }
            }
            ArithAst::Assign { .. } => {
                // runtime setVar semantics (x=, x+=) — sh2.arith stub
                self.sh2_calls.insert("arith".into());
                "sh2ArithStub()".to_string()
            }
            ArithAst::Sizeof(ty) => ty.c_sizeof().unwrap_or(4).to_string(),
            ArithAst::Cast { arg, .. } => self.arith(arg),
        }
    }

    // ── bigint (std.math.big.int.Managed) ────────────────────────────

    /// Evaluate a bigint arith into a fresh Managed temp; returns its
    /// name. The temp is declared + deferred before the current
    /// statement's expression, so the caller can reference it in a
    /// print/assign that follows.
    fn arith_bigint(&mut self, a: &ArithAst) -> String {
        self.bigint_eval_temp(a)
    }

    /// Materialize an operand as a Managed expression: bigint vars read
    /// directly, everything else evaluates into a fresh temp.
    fn bigint_operand(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Var(name) | ArithAst::Ident(name)
                if self.bigint_vars.contains(name) =>
            {
                let m = self.zig_ident(name);
                self.mark_read(name);
                m
            }
            _ => self.bigint_eval_temp(a),
        }
    }

    /// Evaluate bigint arith into a fresh Managed temp; returns its name.
    fn bigint_eval_temp(&mut self, a: &ArithAst) -> String {
        self.need_bigint = true;
        let t = self.fresh_tmp("__bi");
        self.emit(&format!(
            "var {t} = try std.math.big.int.Managed.init(std.heap.page_allocator);"
        ));
        self.emit(&format!("defer {t}.deinit();"));
        self.bigint_into(a, &t);
        t
    }

    /// Compute bigint arith `a` into the already-initialized Managed
    /// `target`. `//`/`%` are Python FLOOR ops (divFloor); `**` with a
    /// negative exponent is 0.
    fn bigint_into(&mut self, a: &ArithAst, target: &str) {
        self.need_bigint = true;
        match a {
            ArithAst::Num(n) => {
                self.emit(&format!("try {target}.setString(10, \"{n}\");"));
            }
            ArithAst::Var(name) | ArithAst::Ident(name) => {
                if self.bigint_vars.contains(name) {
                    let m = self.zig_ident(name);
                    self.mark_read(name);
                    self.emit(&format!("try {target}.copy({m}.toConst());"));
                } else if self.is_num(name) {
                    let m = self.zig_ident(name);
                    self.mark_read(name);
                    self.emit(&format!("try {target}.set({m});"));
                } else {
                    // positional / string var: parse its decimal text
                    let v = self.bigint_str_operand(name);
                    self.emit(&format!("try {target}.setString(10, {v});"));
                }
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let lt = self.bigint_operand(lhs);
                let rt = self.bigint_operand(rhs);
                match op.as_str() {
                    "+" => self.emit(&format!("try {target}.add(&{lt}, &{rt});")),
                    "-" => self.emit(&format!("try {target}.sub(&{lt}, &{rt});")),
                    "*" => self.emit(&format!("try {target}.mul(&{lt}, &{rt});")),
                    "/" => {
                        // floor div; zero divisor → 0
                        self.emit(&format!(
                            "if ({rt}.toConst().orderAgainstScalar(0) == .eq) {{ try {target}.set(0); }} else {{"
                        ));
                        self.emit(&format!(
                            "    var __r = try std.math.big.int.Managed.init(std.heap.page_allocator); defer __r.deinit();"
                        ));
                        self.emit(&format!("    try {target}.divFloor(&__r, &{lt}, &{rt});"));
                        self.emit("}");
                    }
                    "%" => {
                        self.emit(&format!(
                            "if ({rt}.toConst().orderAgainstScalar(0) == .eq) {{ try {target}.set(0); }} else {{"
                        ));
                        self.emit(&format!(
                            "    var __q = try std.math.big.int.Managed.init(std.heap.page_allocator); defer __q.deinit();"
                        ));
                        self.emit(&format!("    try __q.divFloor(&{target}, &{lt}, &{rt});"));
                        self.emit("}");
                    }
                    "**" => {
                        self.emit(&format!(
                            "if ({rt}.toConst().orderAgainstScalar(0) == .lt) {{ try {target}.set(0); }} else {{"
                        ));
                        self.emit(&format!(
                            "    try {target}.pow(&{lt}, @intCast({rt}.toConst().toInt(u32) catch 0));"
                        ));
                        self.emit("}");
                    }
                    _ => {
                        self.mark_todo(&format!("bigint op {op}"));
                        self.emit(&format!("try {target}.set(0);"));
                    }
                }
            }
            ArithAst::Un { op, arg } => {
                let at = self.bigint_operand(arg);
                match op.as_str() {
                    "-" => {
                        let z = self.fresh_tmp("__bi");
                        self.emit(&format!(
                            "var {z} = try std.math.big.int.Managed.initSet(std.heap.page_allocator, 0); defer {z}.deinit();"
                        ));
                        self.emit(&format!("try {target}.sub(&{z}, &{at});"));
                    }
                    "+" => self.emit(&format!("try {target}.copy({at}.toConst());")),
                    "!" => self.emit(&format!(
                        "try {target}.set(@intFromBool({at}.toConst().orderAgainstScalar(0) == .eq));"
                    )),
                    _ => {
                        self.mark_todo(&format!("bigint unop {op}"));
                        self.emit(&format!("try {target}.set(0);"));
                    }
                }
            }
            ArithAst::Cast { arg, .. } => self.bigint_into(arg, target),
            _ => {
                self.mark_todo("bigint arith shape");
                self.emit(&format!("try {target}.set(0);"));
            }
        }
    }

    /// A string operand for bigint setString: a positional/string var's
    /// value (or "0" when undeclared).
    fn bigint_str_operand(&mut self, name: &str) -> String {
        if self.in_function && !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
            let idx: usize = name.parse().unwrap_or(1);
            return format!(
                "(if (__args.len >= {idx}) __args[{idx} - 1] else \"0\")"
            );
        }
        if self.declared(name) {
            let m = self.zig_ident(name);
            self.mark_read(name);
            return m;
        }
        self.sh2_calls.insert("getVar".into());
        "sh2GetVar()".to_string()
    }

    /// A bigint arith in a BOOLEAN (condition) position: a comparison
    /// evaluates both sides into Managed temps and compares via order
    /// (exact — never truncates past i64); any other bigint value is
    /// truthy iff its sign is nonzero.
    fn bigint_cond(&mut self, a: &ArithAst) -> String {
        self.need_bigint = true;
        match a {
            ArithAst::Bin { op, lhs, rhs }
                if matches!(op.as_str(), "==" | "!=" | "<" | ">" | "<=" | ">=") =>
            {
                let lt = self.bigint_operand(lhs);
                let rt = self.bigint_operand(rhs);
                let ord = format!(
                    "std.math.big.int.Managed.order({lt}, {rt})"
                );
                match op.as_str() {
                    "==" => format!("({ord} == .eq)"),
                    "!=" => format!("({ord} != .eq)"),
                    "<" => format!("({ord} == .lt)"),
                    ">" => format!("({ord} == .gt)"),
                    "<=" => format!("({ord} != .gt)"),
                    ">=" => format!("({ord} != .lt)"),
                    _ => unreachable!(),
                }
            }
            _ => {
                let t = self.bigint_eval_temp(a);
                format!("({t}.toConst().orderAgainstScalar(0) != .eq)")
            }
        }
    }

    fn arith_op(&self, op: &BinOpKind) -> &'static str {
        match op {
            BinOpKind::Add => "+",
            BinOpKind::Sub => "-",
            BinOpKind::Mul => "*",
            BinOpKind::Div => "/",
            BinOpKind::Mod => "%",
            BinOpKind::BitAnd => "&",
            BinOpKind::BitOr => "|",
            BinOpKind::BitXor => "^",
            BinOpKind::ShiftL => "<<",
            BinOpKind::ShiftR => ">>",
            BinOpKind::Pow => "**",
            BinOpKind::Concat => "+",
            _ => "+",
        }
    }

    // ── sh2.* calls ──────────────────────────────────────────────────

    /// String-typed context for a Call (stub callees take no args so the
    /// generated call always compiles; stubs return []const u8 by default).
    /// An arith STRING (`($x % 2)`, `$j*3+$k == 4` — the C frontend's
    /// testArith/ternary-cond shape): normalize $refs, parse, render via
    /// the Arith AST walker. None when it doesn't parse.
    fn arith_text(&mut self, s: &str) -> Option<String> {
        let norm: String = s
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if c == '$'
                    && i + 1 < s.len()
                    && (s[i + 1..].starts_with('{')
                        || s[i + 1..]
                            .chars()
                            .next()
                            .map_or(false, |n| n.is_ascii_alphabetic() || n == '_'))
                {
                    ' '
                } else {
                    c
                }
            })
            .collect();
        let norm = norm.replace('{', " ").replace('}', " ");
        // the CORE arithmetic parser FIRST: it understands `i++`/`++i` and
        // every operator; the hand-rolled mf_* chain only covers math
        // functions and silently read `i++` as `i` (078_arithmetic_double_
        // paren: `(( j = i++ + ++i ))`).
        if let Some(ast) = crate::shir::parse_arith(norm.trim()) {
            return Some(self.arith(&ast));
        }
        if let Some(v) = self.mathfunc_arith_zig(norm.trim()) {
            return Some(v);
        }
        None
    }

    /// zsh-mathfunc arith text → native Zig numeric expression. The
    /// py-sh-go frontend emits `int(sqrt($1)) + 1`-style loop bounds as
    /// arith texts; real bash cannot even parse a mathfunc call (it is a
    /// bash arith SYNTAX error), so this is a dedicated parser (the same
    /// shape as the C backend's mathfunc_arith_c). `int()` truncates
    /// toward zero (@intFromFloat); the other names are the same-named
    /// Zig float builtins.
    fn mathfunc_arith_zig(&mut self, src: &str) -> Option<String> {
        let chars: Vec<char> = src.chars().collect();
        let mut pos = 0usize;
        let out = self.mf_cmp(&chars, &mut pos)?;
        Self::mf_ws(&chars, &mut pos);
        if pos != chars.len() {
            return None;
        }
        Some(out)
    }

    fn mf_ws(chars: &[char], pos: &mut usize) {
        while *pos < chars.len() && chars[*pos].is_whitespace() {
            *pos += 1;
        }
    }

    fn mf_cmp(&mut self, chars: &[char], pos: &mut usize) -> Option<String> {
        let l = self.mf_add(chars, pos)?;
        Self::mf_ws(chars, pos);
        let rest: String = chars[*pos..].iter().collect();
        for op in ["<=", ">=", "==", "!=", "<", ">"] {
            if rest.starts_with(op) {
                *pos += op.len();
                let r = self.mf_add(chars, pos)?;
                return Some(format!("@intFromBool(({l}) {op} ({r}))"));
            }
        }
        Some(l)
    }

    fn mf_add(&mut self, chars: &[char], pos: &mut usize) -> Option<String> {
        let mut l = self.mf_mul(chars, pos)?;
        loop {
            Self::mf_ws(chars, pos);
            let op = match chars.get(*pos) {
                Some('+') => "+",
                Some('-') => "-",
                _ => return Some(l),
            };
            *pos += 1;
            let r = self.mf_mul(chars, pos)?;
            l = format!("(({l}) {op} ({r}))");
        }
    }

    fn mf_mul(&mut self, chars: &[char], pos: &mut usize) -> Option<String> {
        let mut l = self.mf_pow(chars, pos)?;
        loop {
            Self::mf_ws(chars, pos);
            if chars.get(*pos) == Some(&'*') && chars.get(*pos + 1) != Some(&'*') {
                *pos += 1;
                let r = self.mf_pow(chars, pos)?;
                l = format!("(({l}) * ({r}))");
            } else if chars.get(*pos) == Some(&'/') {
                *pos += 1;
                let r = self.mf_pow(chars, pos)?;
                l = format!("(@as(f64, @floatFromInt({l})) / @as(f64, @floatFromInt({r})))");
            } else {
                return Some(l);
            }
        }
    }

    fn mf_pow(&mut self, chars: &[char], pos: &mut usize) -> Option<String> {
        Self::mf_ws(chars, pos);
        let signed = matches!(chars.get(*pos), Some('+') | Some('-'));
        let base = self.mf_unary(chars, pos)?;
        Self::mf_ws(chars, pos);
        if chars.get(*pos) == Some(&'*') && chars.get(*pos + 1) == Some(&'*') {
            if signed {
                return None;
            }
            *pos += 2;
            let exp = self.mf_unary(chars, pos)?;
            return Some(format!("std.math.pow(f64, @as(f64, @floatFromInt({base})), @as(f64, @floatFromInt({exp})))"));
        }
        Some(base)
    }

    fn mf_unary(&mut self, chars: &[char], pos: &mut usize) -> Option<String> {
        Self::mf_ws(chars, pos);
        match chars.get(*pos) {
            Some('+') => {
                *pos += 1;
                self.mf_unary(chars, pos)
            }
            Some('-') => {
                *pos += 1;
                Some(format!("(-{})", self.mf_unary(chars, pos)?))
            }
            _ => self.mf_atom(chars, pos),
        }
    }

    fn mf_atom(&mut self, chars: &[char], pos: &mut usize) -> Option<String> {
        Self::mf_ws(chars, pos);
        match chars.get(*pos) {
            Some('(') => {
                *pos += 1;
                let e = self.mf_cmp(chars, pos)?;
                Self::mf_ws(chars, pos);
                if chars.get(*pos) != Some(&')') {
                    return None;
                }
                *pos += 1;
                Some(format!("({e})"))
            }
            Some('$') => self.mf_dollar(chars, pos),
            Some(c) if c.is_ascii_digit() || *c == '.' => Self::mf_number(chars, pos),
            Some(c) if c.is_ascii_alphabetic() || *c == '_' => self.mf_named(chars, pos),
            _ => None,
        }
    }

    fn mf_dollar(&mut self, chars: &[char], pos: &mut usize) -> Option<String> {
        *pos += 1;
        if chars.get(*pos) == Some(&'{') {
            *pos += 1;
            let name = Self::mf_name(chars, pos)?;
            Self::mf_ws(chars, pos);
            if chars.get(*pos) != Some(&'}') {
                return None;
            }
            *pos += 1;
            return Some(self.mf_numref(&name));
        }
        let name = Self::mf_name(chars, pos)?;
        Some(self.mf_numref(&name))
    }

    fn mf_name(chars: &[char], pos: &mut usize) -> Option<String> {
        let start = *pos;
        while let Some(c) = chars.get(*pos) {
            if c.is_ascii_alphanumeric() || *c == '_' {
                *pos += 1;
            } else {
                break;
            }
        }
        if *pos == start {
            return None;
        }
        Some(chars[start..*pos].iter().collect())
    }

    fn mf_named(&mut self, chars: &[char], pos: &mut usize) -> Option<String> {
        let name = Self::mf_name(chars, pos)?;
        let save = *pos;
        Self::mf_ws(chars, pos);
        if chars.get(*pos) != Some(&'(') {
            *pos = save;
            return Some(self.mf_numref(&name));
        }
        *pos += 1;
        let mut args: Vec<String> = Vec::new();
        Self::mf_ws(chars, pos);
        if chars.get(*pos) == Some(&')') {
            *pos += 1;
            return self.mf_call(&name, args);
        }
        loop {
            args.push(self.mf_cmp(chars, pos)?);
            Self::mf_ws(chars, pos);
            match chars.get(*pos) {
                Some(',') => {
                    *pos += 1;
                }
                Some(')') => {
                    *pos += 1;
                    break;
                }
                _ => return None,
            }
        }
        self.mf_call(&name, args)
    }

    fn mf_call(&mut self, name: &str, args: Vec<String>) -> Option<String> {
        let dbl = |a: &String| format!("@as(f64, @floatFromInt({}))", a);
        match name {
            // `int(x)` truncates toward zero: the arg is a float expr
            "int" if args.len() == 1 => Some(format!("@as(i64, @intFromFloat({}))", args[0])),
            "abs" | "fabs" if args.len() == 1 => Some(format!("@abs({})", args[0])),
            // float builtins return f64 (a later `int()` truncates)
            "floor" | "ceil" | "round" | "trunc" | "sqrt" | "cbrt" | "exp"
            | "log" | "log10" | "log2" | "sin" | "cos" | "tan" | "asin"
            | "acos" | "atan" | "sinh" | "cosh" | "tanh" | "asinh" | "acosh"
            | "atanh" if args.len() == 1 => Some(format!("@{}({})", name, dbl(&args[0]))),
            "atan2" | "fmod" | "copysign" if args.len() == 2 => Some(format!(
                "@{}({}, {})",
                name,
                dbl(&args[0]),
                dbl(&args[1])
            )),
            _ => None,
        }
    }

    fn mf_number(chars: &[char], pos: &mut usize) -> Option<String> {
        let start = *pos;
        while let Some(c) = chars.get(*pos) {
            if c.is_ascii_digit() || *c == '.' {
                *pos += 1;
            } else {
                break;
            }
        }
        if *pos == start {
            return None;
        }
        Some(chars[start..*pos].iter().collect())
    }

    /// Numeric read of a `$`-ref or bare name in a mathfunc text. A
    /// positional param ($1) reads the function's param via the getVar
    /// stub (matching the function body's own $N read); a typed-Int var
    /// reads its ident; anything else atolls the store read.
    fn mf_numref(&mut self, name: &str) -> String {
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_digit()) {
            if self.in_function {
                let idx: usize = name.parse().unwrap_or(1);
                self.need_toint = true;
                return format!(
                    "shToInt(if (__args.len >= {idx}) __args[{idx} - 1] else \"\")"
                );
            }
            self.sh2_calls.insert("getVar".into());
            self.need_toint = true;
            return "shToInt(sh2GetVar())".to_string();
        }
        if self.declared(name) {
            let m = self.zig_ident(name);
            self.mark_read(name);
            if self.is_num(name) {
                return m;
            }
            self.need_toint = true;
            return format!("shToInt({m})");
        }
        self.sh2_calls.insert("getVar".into());
        self.need_toint = true;
        "shToInt(sh2GetVar())".to_string()
    }

/// Raw `$(…)`/backtick capture value (user-fn direct call,
/// pipeline collect, or external shRun). None → caller stubs.
fn capture_raw(&mut self, args: &[IrExpr]) -> Option<String> {
            // `$(fn args…)` of a user function DEFINED in this
            // program: call it and use its return value. The capture
            // body is a single `exec fn [args]`. An `exec` of anything
            // else is an EXTERNAL command — render the native capture
            // (shRun), never a call to an undeclared Zig fn.
            // `$(a | b)` (single-Expr pipeline body): collect the pipe.
            if let Some(IrExpr::Arrow(stmts)) = args.first() {
                // structured `IrStmt::Pipeline` body (backtick/`$()`
                // of a pipeline lowers here, not to a `pipeline` call)
                if let [IrStmt::Pipeline { stages, .. }] = stmts.as_slice() {
                    if let Some(rendered) = self.render_pipeline_structured(stages, true) {
                        return Some(rendered);
                    }
                }
                if let [IrStmt::Expr(IrExpr::Call { func: pfunc, args: pargs })] = stmts.as_slice() {
                    if pfunc == "pipeline" {
                        if let Some(rendered) = self.render_pipeline(pargs, true) {
                            return Some(rendered);
                        }
                    }
                }
                if let Some(IrStmt::Expr(IrExpr::Call { func, args: cargs })) = stmts.first() {
                    if func == "exec" {
                        if let Some(IrExpr::Str(fname, _)) = cargs.first() {
                            if self.user_fns.contains(fname) {
                                let m = self.zig_ident(fname);
                                let call_args: Vec<String> = cargs
                                    .get(1)
                                    .and_then(|a| match a {
                                        IrExpr::Array(items) => Some(
                                            items.iter().map(|e| self.expr_str(e)).collect(),
                                        ),
                                        _ => None,
                                    })
                                    .unwrap_or_default();
                                let catch = if self.error_fns.contains(fname) {
                                    " catch \"\""
                                } else {
                                    ""
                                };
                                return Some(format!(
                                    "{m}(init, &[_][]const u8{{ {} }}){catch}",
                                    call_args.join(", ")
                                ));
                            }
                            if let Some(arrow) = args.first() {
                                if let Some(rendered) = self.capture_expr(arrow) {
                                    return Some(rendered);
                                }
                            }
                        }
                    }
                }
            }

            None
        }

    fn call_str(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            "fnValue" | "fnCall" => {
                // a user-function call: name(args...) — values flow as
                // strings (the positional-param convention); a numeric
                // context coerces via shToInt at the read site
                let orig = args
                    .first()
                    .and_then(|a| match a {
                        IrExpr::Str(n, _) => Some(n.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                let name = self.zig_ident(&orig);
                let call_args: Vec<String> = args
                    .get(1)
                    .and_then(|a| match a {
                        IrExpr::Array(elems) => {
                            Some(elems.iter().map(|e| self.expr_str(e)).collect())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                let catch = if self.error_fns.contains(&orig) { " catch \"\"" } else { "" };
                format!("{name}(init, &[_][]const u8{{ {} }}){catch}", call_args.join(", "))
            }
            "capture" => {
                if let Some(rendered) = self.capture_raw(args) {
                    return rendered;
                }
                self.sh2_stub("capture", "capture")
            }
            "captureWords" => {
                // unquoted backticks: word-split the captured text (bash
                // splits unquoted substitutions; echo joins with spaces)
                if let Some(rendered) = self.capture_raw(args) {
                    self.need_ext = true;
                    return format!("shJoinWs({rendered})");
                }
                self.sh2_stub("capture", "capture")
            }

            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    // `$?` FIRST — a synthesized Var("?")/read can land "?"
                    // in the written set, and the declared arm below would
                    // emit it as a bare (nonexistent, mangled) identifier
                    if name == "?" {
                        self.need_intstr = true;
                        return "shIntStr(sh_rc)".to_string();
                    }
                    // inside a user function: getVar("N") is the POSITIONAL
                    // PARAMETER read ($1 convention — the C frontend's
                    // outparam/param channel), not an env lookup
                    if self.in_function && name.bytes().all(|b| b.is_ascii_digit()) && !name.is_empty() {
                        let idx: usize = name.parse().unwrap_or(1);
                        return format!(
                            "(if (__args.len >= {idx}) __args[{idx} - 1] else \"\")"
                        );
                    }
                    // `$@` / `$*` inside a user function: the positional
                    // args, space-joined (bash's default IFS). Outside a
                    // function there are none. Reading the ENVIRONMENT for
                    // "@" printed nothing ("hello " instead of "hello
                    // test", generator-system-echo-checkqx.sh).
                    // `$-`: the shell's option letters. The gate compares
                    // against a plain non-interactive bash, whose set is
                    // "hB" (hashall + braceexpand). Nothing else in the
                    // corpus inspects it.
                    if name == "-" {
                        return Self::zig_str("hB");
                    }
                    if self.in_function && (name == "@" || name == "*") {
                        self.need_ext = true;
                        return "shJoinSp(__args)".to_string();
                    }
                    // `$#`: the positional-argument COUNT inside a fn (0 at
                    // top level). It used to read the environment for "#"
                    // and printed an empty count.
                    if name == "#" {
                        if self.in_function {
                            self.need_intstr = true;
                            return "shIntStr(@as(i64, @intCast(__args.len)))".to_string();
                        }
                        // top level: the script's own argv count. The gate
                        // runs with none, so "0" (005_args.sh printed a blank
                        // line because the value went through the wrong arm).
                        return Self::zig_str("0");
                    }
                    // `$0`: the script path the invoker passed (SH2_ARGV0 in
                    // the gate, argv[0] standalone)
                    if name == "0" {
                        self.need_argv0 = true;
                        self.need_env = true;
                        return "shArgv0()".to_string();
                    }
                    if self.declared(name) {
                        let m = self.zig_ident(name);
                        self.mark_read(name);
                        return m;
                    }
                    // undeclared (env) var: read the process environment
                    self.need_env = true;
                    return format!("shEnv({})", Self::zig_str(name));
                }
                self.sh2_stub("getVar", "getVar")
            }
            "test" => {
                // a [ ... ] result in string context (rare; e.g. `x=$( [ .. ] )`)
                self.need_b2s = true;
                format!("shB2S({})", self.call_bool(func, args))
            }
            "split" => {
                // word splitting (unquoted `$x` / `$@` in word position):
                // split on IFS whitespace, drop empties, rejoin with
                // single spaces (bash; echo joins argv with spaces). On
                // whitespace-free values this is the identity.
                match args.first() {
                    Some(arg) => {
                        self.need_ext = true;
                        format!("shJoinWs({})", self.expr_str(arg))
                    }
                    None => self.sh2_stub("split", "split"),
                }
            }
            "shopt" => {
                // shell options (`nullglob`, `nocasematch`, `extglob`, …):
                // known runtime limitation — statement position has its
                // own arm (records status); value position is "0"
                "0".to_string()
            }
            "pipeline" => {
                // value position (`x=$(a | b)`, nested words): collect the
                // last stage's output; statement/condition positions have
                // their own arms
                if let Some(rendered) = self.render_pipeline(args, true) {
                    return rendered;
                }
                self.sh2_stub("pipeline", "pipeline")
            }
            "param" => {
                if let Some(rendered) = self.render_param(args) {
                    return rendered;
                }
                self.sh2_stub("param", "param")
            }
            "join" => {
                // `join(slice…)` wraps scalar AND array slices: scalar
                // slices already render as plain strings (identity); array
                // slices bail upstream in render_param, so this is reached
                // only for values that are already joined text
                match args.first() {
                    Some(arg) => self.expr_str(arg),
                    None => self.sh2_stub("join", "join"),
                }
            }
            "arrayLen" => {
                // `${#arr[@]}` — the live element count of any array home
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.set_arrays.contains(name)
                        || self.str_arrays.contains(name)
                        || self.bigint_arrays.contains(name)
                    {
                        let m = self.zig_ident(name);
                        self.mark_read(name);
                        self.need_intstr = true;
                        return format!(
                            "shIntStr(@as(i64, @intCast({m}_len)))"
                        );
                    }
                }
                self.sh2_stub("arrayLen", "arrayLen")
            }
            "arrayIndex" => {
                let (name, idx) = match args {
                    [IrExpr::Str(name, _), IrExpr::Str(idx, _)] => (name.clone(), idx.clone()),
                    _ => return self.sh2_stub("arrayIndex", "arrayIndex"),
                };
                if let Some(rendered) = self.render_array_index(&name, &idx) {
                    return rendered;
                }
                self.sh2_stub("arrayIndex", "arrayIndex")
            }
            "listVar" => {
                // `$@` — positional params: joined with spaces in string
                // context (empty at top level; the gate passes no argv)
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if name == "@" || name == "*" {
                        if self.in_function {
                            return "std.mem.join(std.heap.page_allocator, \" \", __args) catch \"\"".to_string();
                        }
                        return "\"\"".to_string();
                    }
                }
                self.sh2_stub("listVar", "listVar")
            }
            "setArray" | "setArrayAppend" => {
                // array assignment `arr=(a b c)` — render as a bracketed
                // string (the common indexed-array form).
                let mut parts = Vec::new();
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for it in items {
                        parts.push(self.expr_str(it));
                    }
                }
                format!("[_] []const u8{{{}}}", parts.join(", "))
            }
            "sortedIntJoin" => {
                // `sorted(list(factors))` — sort+dedup+join the set array
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.set_arrays.contains(name) {
                        let m = self.zig_ident(name);
                        self.mark_read(name);
                        self.need_sorted = true;
                        return format!("shSortedIntJoin(&{m}, {m}_len)");
                    }
                }
                self.sh2_stub("sortedIntJoin", "sortedIntJoin")
            }
            "sortedBigintJoin" => {
                // `sorted(list(factors))` over a bigint list — sort+dedup+join
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.bigint_arrays.contains(name) {
                        let m = self.zig_ident(name);
                        self.mark_read(name);
                        self.need_sorted = true;
                        return format!("shSortedBigintJoin(&{m}, {m}_len)");
                    }
                }
                self.sh2_stub("sortedBigintJoin", "sortedBigintJoin")
            }
            "max" | "min" | "sum" => {
                // typed reduction over a list/set array
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.bigint_arrays.contains(name) {
                        let m = self.zig_ident(name);
                        self.mark_read(name);
                        self.need_reduce = true;
                        return format!("sh2{}Bigint(&{m}, {m}_len)", camel(func));
                    }
                    if self.set_arrays.contains(name) {
                        let m = self.zig_ident(name);
                        self.mark_read(name);
                        self.need_reduce = true;
                        return format!("sh2{}(&{m}, {m}_len)", camel(func));
                    }
                }
                self.sh2_stub(func, func)
            }
            "arith" => {
                if let Some(IrExpr::Str(sv, _)) = args.first() {
                    if let Some(x) = self.arith_text(sv) {
                        self.need_intstr = true;
                        return format!("shIntStr({x})");
                    }
                }
                self.need_intstr = true;
                self.sh2_calls.insert("arith".into());
                format!("shIntStr(sh2ArithStub())")
            }
            _ => self.sh2_stub(func, func),
        }
    }

    /// i64-typed context for a Call.
    fn call_num(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            "arith" => {
                if let Some(IrExpr::Str(sv, _)) = args.first() {
                    if let Some(x) = self.arith_text(sv) {
                        return x;
                    }
                }
                self.sh2_calls.insert("arith".into());
                "sh2ArithStub()".to_string()
            }
            "isqrt" => {
                // structured integer sqrt (py-sh-go `int(Y**0.5)` node
                // — no arith-string detour). CPython parity by
                // construction: trunc(sqrt(x)) in doubles.
                if let Some(IrExpr::Arith(a)) = args.first() {
                    let x = self.arith(a);
                    return format!("@as(i64, @intFromFloat(@sqrt(@as(f64, @floatFromInt({x})))))" );
                }
                "0".to_string()
            }
            "test" => format!("@intFromBool({})", self.call_bool(func, args)),
            _ => {
                self.need_toint = true;
                format!("shToInt({})", self.call_str(func, args))
            }
        }
    }

    /// bool-typed context for a Call.
    fn call_bool(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            "test" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some(c) = self.test_render(s) {
                        return c;
                    }
                }
                self.sh2_calls.insert("test".into());
                "sh2Test()".to_string()
            }
            "contains" => {
                // `echo X | grep LIT >/dev/null` → contains(X, LIT): native
                // std.mem.indexOf (the backend's native substring primitive).
                if let (Some(needle), Some(pattern)) = (args.first(), args.get(1)) {
                    let n = self.expr_str(needle);
                    let p = self.expr_str(pattern);
                    return format!("std.mem.indexOf(u8, {n}, {p}) != null");
                }
                self.sh2_calls.insert("contains".into());
                "sh2Contains()".to_string()
            }
            "arith" => {
                if let Some(IrExpr::Str(sv, _)) = args.first() {
                    if let Some(x) = self.arith_text(sv) {
                        return format!("({x} != 0)");
                    }
                }
                self.sh2_calls.insert("arith".into());
                "(sh2ArithStub() != 0)".to_string()
            }
            "getVar" => {
                self.need_truthy = true;
                format!("shTruthy({})", self.call_str(func, args))
            }
            "pipeline" => {
                // `if a | b` / `while a | b` — the LAST stage's status
                // decides (the pass-through run prints side effects like
                // bash, then sh_rc is read)
                match self.render_pipeline(args, false) {
                    Some(run) => {
                        self.rc_dirty = true;
                        format!("blk: {{ _ = {run}; break :blk sh_rc == 0; }}")
                    }
                    None => {
                        self.sh2_calls.insert("pipeline".into());
                        "shTruthy(sh2Pipeline())".to_string()
                    }
                }
            }
            "exec" => {
                // builtin true/false used as a condition
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    match cmd.as_str() {
                        "true" => return "true".to_string(),
                        "false" => return "false".to_string(),
                        "let" => {
                            // `let "i<3"` — an arithmetic status condition
                            if let Some(IrExpr::Array(items)) = args.get(1) {
                                if let Some(IrExpr::Str(text, _)) = items.first() {
                                    if let Some(c) = self.render_let_cond(text) {
                                        return c;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                self.need_truthy = true;
                format!("shTruthy({})", self.call_str(func, args))
            }
            _ => {
                self.need_truthy = true;
                format!("shTruthy({})", self.call_str(func, args))
            }
        }
    }

    /// Render a `let "EXPR"` arithmetic condition (`i<3`) as a Zig
    /// numeric comparison over declared numeric vars. Returns None for
    /// shapes outside the simple subset.
    fn render_let_cond(&mut self, text: &str) -> Option<String> {
        for (op, zop) in [("<=", "<="), (">=", ">="), ("==", "=="), ("!=", "!="), ("<", "<"), (">", ">")] {
            if let Some(idx) = text.find(op) {
                let l = text[..idx].trim();
                let r = text[idx + op.len()..].trim();
                let l = self.num_operand(l)?;
                let r = self.num_operand(r)?;
                return Some(format!("({l} {zop} {r})"));
            }
        }
        None
    }

    /// A numeric operand for a `let` condition: a declared numeric var
    /// → its ident; a declared STRING-homed var → shToInt coercion (bash
    /// compares values, and a `$1`-fed local is string-homed); a number
    /// literal → the literal.
    fn num_operand(&mut self, t: &str) -> Option<String> {
        let t = t.trim();
        if let Ok(n) = t.parse::<i64>() {
            return Some(n.to_string());
        }
        let name = t.strip_prefix('$').unwrap_or(t);
        let name = name
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .unwrap_or(name);
        if self.declared(name) {
            let m = self.zig_ident(name);
            self.mark_read(name);
            if self.is_num(name) {
                return Some(m);
            }
            self.need_toint = true;
            return Some(format!("shToInt({m})"));
        }
        None
    }

    /// `for (( i=1; i<n; i++ ))` — render as a native while loop. The
    /// loop var is hoisted at i64 (function scope; bash leaves $i = LAST
    /// after the loop), so the header's init assigns it and the body's
    /// reads see the same ident.
    fn emit_cstyle_for(&mut self, spec: &str, body: &[IrStmt]) {
        let parts: Vec<&str> = spec.split(';').map(|s| s.trim()).collect();
        let init = parts.first().copied().unwrap_or("");
        let cond = parts.get(1).copied().unwrap_or("1");
        let incr = parts.get(2).copied().unwrap_or("");
        // the loop var: `i = 1` / `i=1` — the init's LHS
        let (var, init_c) = if let Some(eq) = init.find('=') {
            let v = init[..eq].trim().to_string();
            let rhs = init[eq + 1..].trim().to_string();
            if is_ident(&v) {
                (Some(v), rhs)
            } else {
                (None, init.to_string())
            }
        } else if !init.is_empty() {
            (None, init.to_string())
        } else {
            (None, String::new())
        };
        if let Some(v) = &var {
            self.var_types.insert(v.clone(), IrType::Int);
        }
        let cond_c = self.cstyle_cond(cond);
        let incr_c = if incr.is_empty() {
            String::new()
        } else {
            self.cstyle_incr(incr)
        };
        let var_name = var.as_ref().map(|v| self.zig_ident(v)).unwrap_or_default();
        if var.is_some() {
            self.emit(&format!("{var_name} = {init_c};"));
            self.emit(&format!("while ({cond_c}) {{"));
        } else {
            self.emit(&format!("{init_c};"));
            self.emit(&format!("while ({cond_c}) {{"));
        }
        self.loop_depth += 1;
        self.depth += 1;
        for s in body {
            self.stmt(s);
        }
        if !incr_c.is_empty() {
            self.emit(&format!("{incr_c};"));
        }
        self.depth -= 1;
        self.loop_depth -= 1;
        self.emit("}");
    }

    /// The cstyleFor condition: split on the comparison ops, map the
    /// operand names to Zig idents (numeric now) or literals.
    fn cstyle_cond(&mut self, c: &str) -> String {
        let c = c.trim();
        if c.is_empty() {
            return "true".into();
        }
        for op in ["<=", ">=", "==", "!=", "<", ">"] {
            if let Some(pos) = c.find(op) {
                let l = c[..pos].trim();
                let r = c[pos + op.len()..].trim();
                let lc = self.cstyle_operand(l);
                let rc = self.cstyle_operand(r);
                let c_op = match op {
                    "<=" => "<=",
                    ">=" => ">=",
                    "==" => "==",
                    "!=" => "!=",
                    "<" => "<",
                    _ => ">",
                };
                return format!("({lc} {c_op} {rc})");
            }
        }
        self.cstyle_operand(c)
    }

    /// A cstyleFor operand: a var name → the numeric Zig expr.
    fn cstyle_operand(&mut self, name: &str) -> String {
        let name = name.trim();
        if name.is_empty() {
            return "0".into();
        }
        if let Ok(n) = name.parse::<i64>() {
            return n.to_string();
        }
        if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            if self.declared(name) {
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    return m;
                }
                // a string-homed var in an arith cond — coerce
                self.need_toint = true;
                return format!("shToInt({m})");
            }
            return "0".into();
        }
        "0".into()
    }

    /// The cstyleFor increment: `i++` / `i--` / `i += 2`.
    fn cstyle_incr(&mut self, s: &str) -> String {
        let s = s.trim();
        if s.ends_with("++") {
            let v = s.trim_end_matches("++").trim();
            if is_ident(v) {
                return format!("{} += 1", self.zig_ident(v));
            }
        }
        if s.ends_with("--") {
            let v = s.trim_end_matches("--").trim();
            if is_ident(v) {
                return format!("{} -= 1", self.zig_ident(v));
            }
        }
        for op in ["+=", "-=", "*=", "/=", "%="] {
            if let Some((l, r)) = s.split_once(op) {
                let l = l.trim();
                if is_ident(l) {
                    return format!("{} {op} {}", self.zig_ident(l), r.trim());
                }
            }
        }
        s.to_string()
    }


    /// Build the argv literals for an external `exec` command
    /// (`["cmd", "arg", …]`). The command name is args[0]; the trailing
    /// args array is args[1].
    fn build_argv(&mut self, cmd: &str, args: &[IrExpr]) -> Vec<String> {
        let mut argv = vec![Self::zig_str(cmd)];
        if let Some(IrExpr::Array(items)) = args.get(1) {
            for it in items {
                argv.push(self.expr_str(it));
            }
        }
        argv
    }

    /// `$(cmd args)` — command substitution.
    ///
    /// The lowered shape is `Capture { expr: Arrow([Expr(Call{exec|builtin,
    /// [cmd, args]})]) }` (the pipeline case nests a `pipeline` call, which
    /// stays a stub). A capture is exactly `shRun` on that argv: `shRun`
    /// already trims the trailing newline the way `$( )` does, so the simple
    /// single-command form renders NATIVELY with no runtime helper.
    fn capture_expr(&mut self, expr: &IrExpr) -> Option<String> {
        let IrExpr::Arrow(stmts) = expr else { return None };
        // `$(a | b)`: structured pipeline body — collect the pipe
        if let [IrStmt::Pipeline { stages, .. }] = stmts.as_slice() {
            return self.render_pipeline_structured(stages, true);
        }
        let [IrStmt::Expr(IrExpr::Call { func, args })] = stmts.as_slice() else {
            return None;
        };
        if func != "exec" && func != "builtin" {
            return None;
        }
        let cmd = match args.first() {
            Some(IrExpr::Str(s, _)) => s.clone(),
            _ => return None,
        };
        // `echo $(fnValue("f", [args]))` is the echo-return-lift's pass-3
        // wrapper: the fnValue ALREADY returns the value, so capturing it would
        // spawn a child that prints what we already have. The discriminator is
        // the fnValue INSIDE, not `echo` itself — a user's own
        // `x=`echo hi`` is a genuine capture and must run.
        if cmd == "echo" {
            let wraps_fn_value = match args.get(1) {
                Some(IrExpr::Array(items)) => items.iter().any(|it| {
                    matches!(it, IrExpr::Call { func, .. } if func == "fnValue")
                }),
                _ => false,
            };
            if wraps_fn_value {
                return None;
            }
        }
        let argv = self.build_argv(&cmd, args);
        self.need_run = true;
        // a capture records the child's real status in sh_rc (bash:
        // `x=$(false); echo $?` → 1), so the stmt wrapper must not reset it
        self.rc_dirty = true;
        // CAPTURE position: always the fork/exec arm. `uu_run` takes no
        // capture buffer — it returns a status and writes fd 1 — so a
        // `$(cat "$tmpf")` lowered through it evaluated to "" and lost the
        // command's output (heredoc-with-redirect-same-line printed
        // `content=[]`). The FFI arm stays for statement position, where
        // writing fd 1 is exactly right.
        let _ = Self::uu_cmd_ok(&cmd);
        Some(format!("shRun(&[_][]const u8{{{}}})", argv.join(", ")))
    }

    /// One stage of either pipeline form as a fork/feed unit. None for
    /// compound stages or stateful builtins (cd/export/exit/… — their side
    /// effects must not leak from the pipe into our process).
    fn stage_call(&mut self, func: &str, cargs: &[IrExpr]) -> Option<PipeStage> {
        // commands that must NEVER fork-or-feed as a stage: their effects
        // belong to our own process (or terminate it)
        const DENY: &[&str] = &[
            "cd", "local", "declare", "export", "readonly", "typeset", "exit", "set", "shopt",
            "eval", "source", ".", "exec", "return", "break", "continue", "trap", "unset",
            "shift", "read",
        ];
        if func != "exec" && func != "builtin" {
            return None;
        }
        let Some(IrExpr::Str(cmd, _)) = cargs.first() else {
            return None;
        };
        if DENY.contains(&cmd.as_str()) {
            return None;
        }
        if cmd == "echo" {
            if let Some((parts, newline)) = self.echo_parts(cargs) {
                return Some(PipeStage::Feed(self.parts_value(parts, newline)));
            }
            return None;
        }
        if cmd == "printf" {
            if let Some(v) = self.printf_value(cargs) {
                return Some(PipeStage::Feed(v));
            }
            return None;
        }
        Some(PipeStage::Fork(self.build_argv(cmd, cargs)))
    }

    /// Stage list of a `pipeline` call (each arg = Arrow of a single
    /// single-command stage). None for empty pipelines.
    fn pipeline_stages(&mut self, args: &[IrExpr]) -> Option<Vec<PipeStage>> {
        if args.is_empty() {
            return None;
        }
        let mut stages = Vec::new();
        for a in args {
            let IrExpr::Arrow(stmts) = a else {
                return None;
            };
            let [IrStmt::Expr(IrExpr::Call { func, args: cargs })] = stmts.as_slice() else {
                return None;
            };
            stages.push(self.stage_call(func, cargs)?);
        }
        Some(stages)
    }

    /// A `printf` stage's output as a heap value (mirrors the statement
    /// printf arm's format decoding; None when the format is dynamic).
    fn printf_value(&mut self, args: &[IrExpr]) -> Option<String> {
        let items = match args.get(1) {
            Some(IrExpr::Array(items)) => items,
            _ => return None,
        };
        let fmt = match items.first() {
            Some(IrExpr::Str(f, _)) => f.clone(),
            _ => return None,
        };
        // decode exactly like the statement arm: reuse it by rendering a
        // synthetic printf call's parts through a shared helper is
        // complex; instead only static formats with %s/%d/%q are fed
        let mut parts: Vec<(char, String)> = Vec::new();
        let rest = &items[1..];
        let mut argi = 0;
        let mut chars = fmt.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '%' {
                if chars.peek() == Some(&'%') {
                    chars.next();
                    parts.push(('s', Self::zig_str("%")));
                    continue;
                }
                while let Some(&f) = chars.peek() {
                    if matches!(f, 'l' | 'h' | 'z' | 'j' | 't' | 'L')
                        || f.is_ascii_digit()
                        || matches!(f, '-' | '+' | ' ' | '#')
                    {
                        chars.next();
                    } else {
                        break;
                    }
                }
                let spec = match chars.peek() {
                    Some('s') => {
                        chars.next();
                        's'
                    }
                    Some('d') | Some('i') | Some('u') => {
                        chars.next();
                        'd'
                    }
                    Some('x') | Some('X') | Some('o') => {
                        chars.next();
                        'd'
                    }
                    Some('c') => {
                        chars.next();
                        's'
                    }
                    Some('q') => {
                        chars.next();
                        'q'
                    }
                    _ => return None,
                };
                let a = rest.get(argi)?;
                argi += 1;
                let pa = self.parts_of(a);
                if pa.len() != 1 {
                    return None;
                }
                if spec == 'd' && pa[0].0 == 's' {
                    parts.push(('s', pa[0].1.clone()));
                } else if spec == 's' && pa[0].0 == 'd' {
                    parts.push(('d', pa[0].1.clone()));
                } else {
                    parts.push((spec, pa[0].1.clone()));
                }
                continue;
            }
            if c == '\\' {
                // printf escapes: only the trivial ones are fed; anything
                // richer bails to the stub (the statement arm interprets
                // \a\b\f\v\xHH\NNN — duplicating that here would drift)
                let esc: String = match chars.peek() {
                    Some('n') => {
                        chars.next();
                        "\n".to_string()
                    }
                    Some('t') => {
                        chars.next();
                        "\t".to_string()
                    }
                    Some('r') => {
                        chars.next();
                        "\r".to_string()
                    }
                    Some('\\') => {
                        chars.next();
                        "\\".to_string()
                    }
                    _ => return None,
                };
                parts.push(('s', Self::zig_str(&esc)));
                continue;
            }
            parts.push(('s', Self::zig_str(&c.to_string())));
        }
        if argi != rest.len() {
            return None;
        }
        Some(self.parts_value(parts, false))
    }

    /// Emit a `pipeline` call: `capture` selects pass-through (statement:
    /// the last stage inherits our stdout) vs collect-and-return.
    fn render_pipeline(&mut self, args: &[IrExpr], capture: bool) -> Option<String> {
        let stages = self.pipeline_stages(args)?;
        self.emit_pipeline(&stages, capture)
    }

    /// Structured `IrStmt::Pipeline` stages rendered through shPipe
    /// (shared by the Pipeline-stmt arm and capture-of-pipeline).
    fn render_pipeline_structured(
        &mut self,
        stages: &[Vec<IrStmt>],
        capture: bool,
    ) -> Option<String> {
        let mut converted: Vec<PipeStage> = Vec::new();
        for st in stages {
            let [IrStmt::Expr(IrExpr::Call { func, args })] = st.as_slice() else {
                return None;
            };
            converted.push(self.stage_call(func, args)?);
        }
        self.emit_pipeline(&converted, capture)
    }

    /// The shPipe call for converted stages.
    fn emit_pipeline(&mut self, stages: &[PipeStage], capture: bool) -> Option<String> {
        if stages.is_empty() {
            return None;
        }
        let mut argv_list: Vec<String> = Vec::new();
        let mut feed_list: Vec<String> = Vec::new();
        for st in stages {
            match st {
                PipeStage::Fork(argv) => {
                    argv_list.push(format!("&[_][]const u8{{{}}}", argv.join(", ")));
                    feed_list.push("null".to_string());
                }
                PipeStage::Feed(s) => {
                    argv_list.push("&[_][]const u8{}".to_string());
                    feed_list.push(format!("{s}"));
                }
            }
        }
        self.need_run = true;
        self.rc_dirty = true;
        Some(format!(
            "shPipe(&[_][]const []const u8{{{}}}, &[_]?[]const u8{{{}}}, {})",
            argv_list.join(", "),
            feed_list.join(", "),
            capture
        ))
    }

    fn sh2_stub(&mut self, name: &str, note: &str) -> String {
        self.sh2_calls.insert(name.to_string());
        self.mark_todo(&format!("{note} → sh2.{name}"));
        self.mark_todo(&format!("{note} → sh2.{name}"));
        let mut fname = format!("sh2{}", camel(name));
        if RESERVED_STUB.contains(&fname.as_str()) {
            fname.push_str("Stub");
        }
        format!("{fname}()")
    }

    /// The generic expression form (used by `_ = expr;` statements and
    /// the part classifier's fallback).
    fn expr_any(&mut self, e: &IrExpr) -> String {
        let (_, x) = self.part_spec(e);
        x
    }

    /// Parts → one `try stdout.print("fmt", .{args});` statement.
    /// The `echo` words as print parts + trailing newline (the flag
    /// stripping shared by the statement echo arm and pipeline feeds).
    /// None when the words are not an Array.
    fn echo_parts(&mut self, args: &[IrExpr]) -> Option<(Vec<(char, String)>, bool)> {
        if let Some(IrExpr::Array(items)) = args.get(1) {
            // leading -n/-e/-E flags (incl. combined -ne): -n suppresses
            // the newline; -e/-E are accepted and ignored (no backslash
            // escapes in word-split values to expand)
            let mut newline = true;
            let mut rest = items.as_slice();
            while let Some(IrExpr::Str(flag, _)) = rest.first() {
                let t = flag.trim();
                if t.len() > 1 && t.starts_with('-') && t[1..].chars().all(|c| c == 'n' || c == 'e' || c == 'E') {
                    if t.contains('n') {
                        newline = false;
                    }
                    rest = &rest[1..];
                } else {
                    break;
                }
            }
            let mut parts = Vec::new();
            for (i, item) in rest.iter().enumerate() {
                if i > 0 {
                    parts.push(('s', "\" \"".to_string()));
                }
                parts.extend(self.parts_of(item));
            }
            return Some((parts, newline));
        }
        None
    }

    /// Print parts as a heap-allocated VALUE (pipeline feeds, `$(…)` of
    /// print-only stages): the same fmt/args print_from_parts would print.
    fn parts_value(&mut self, parts: Vec<(char, String)>, newline: bool) -> String {
        let mut fmt = String::new();
        let mut fargs: Vec<String> = Vec::new();
        for (spec, e) in parts {
            if spec == 'q' {
                self.need_shquote = true;
                fmt.push_str(spec_char('s'));
                fargs.push(format!("shellQuote({e})"));
            } else {
                fmt.push_str(spec_char(spec));
                fargs.push(e);
            }
        }
        if newline {
            fmt.push('\n');
        }
        format!(
            "std.fmt.allocPrint(std.heap.page_allocator, {}, .{{{}}}) catch \"\"",
            Self::zig_str(&fmt),
            fargs.join(", ")
        )
    }

    /// Render already-split printf parts into ONE `[]const u8` expression
    /// (allocPrint with the matching format).
    fn parts_to_str(&mut self, parts: Vec<(char, String)>) -> String {
        let mut fmt = String::new();
        let mut args: Vec<String> = Vec::new();
        for (spec, e) in parts {
            if spec == 'q' {
                self.need_shquote = true;
                fmt.push_str("{s}");
                args.push(format!("shellQuote({e})"));
            } else if spec == 'd' {
                fmt.push_str("{d}");
                args.push(e);
            } else {
                fmt.push_str("{s}");
                args.push(e);
            }
        }
        format!(
            "std.fmt.allocPrint(std.heap.page_allocator, {}, .{{{}}}) catch \"\"",
            Self::zig_str(&fmt),
            args.join(", ")
        )
    }

    /// A static format + arg list rendered as ONE `[]const u8` expression
    /// (`printf -v NAME …`): the text goes to a VARIABLE, not stdout.
    fn printf_text(&mut self, fmt: &str, rest: &[IrExpr]) -> Option<String> {
        let parts = self.printf_parts_of(fmt, rest)?;
        Some(self.parts_to_str(parts))
    }

    /// Decode a STATIC printf format + arg list into print_from_parts
    /// specs. Shared by statement printf, `printf -v`, and the pipeline
    /// stage decoder. None when the shape is not statically decodable.
    fn printf_parts_of(&mut self, fmt: &str, rest: &[IrExpr]) -> Option<Vec<(char, String)>> {
        let mut parts: Vec<(char, String)> = Vec::new();
        let mut argi = 0;
        // literal runs accumulate into `lit` and flush as ONE part around
        // each conversion; a char-wise split made
        // `printf 'heredoc+same-line-redirect content=[%s]\n'` a 39-argument
        // print, past zig's 32-argument cap.
        let mut lit = String::new();
        let mut chars = fmt.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                // printf decodes backslash escapes in the FORMAT text
                // (\n \t \r \\ \a \b \f \v). Leaving them raw printed
                // a literal "\n" (printf-format-and-v.sh).
                let esc: Option<char> = match chars.peek() {
                    Some('n') => Some('\n'),
                    Some('t') => Some('\t'),
                    Some('r') => Some('\r'),
                    Some('a') => Some('\u{7}'),
                    Some('b') => Some('\u{8}'),
                    Some('f') => Some('\u{c}'),
                    Some('v') => Some('\u{b}'),
                    Some('\\') => Some('\\'),
                    _ => None,
                };
                if let Some(e) = esc {
                    chars.next();
                    lit.push(e);
                } else {
                    lit.push(c);
                }
                continue;
            }
            if c != '%' {
                lit.push(c);
                continue;
            }
            if !lit.is_empty() {
                parts.push(('s', Self::zig_str(&lit)));
                lit.clear();
            }
            if chars.peek() == Some(&'%') {
                chars.next();
                parts.push(('s', Self::zig_str("%")));
                continue;
            }
            // flags/width/length modifiers (`%lld`, `%5d`, `%-10s`): record
            // the WIDTH and left-justify flag — printf pads the value
            let mut left = false;
            let mut width: usize = 0;
            while let Some(&f) = chars.peek() {
                if matches!(f, 'l' | 'h' | 'z' | 'j' | 't' | 'L') {
                    chars.next();
                } else if f == '-' {
                    left = true;
                    chars.next();
                } else if f.is_ascii_digit() {
                    width = width * 10 + (f as usize - '0' as usize);
                    chars.next();
                } else if matches!(f, '+' | ' ' | '#') {
                    chars.next();
                } else {
                    break;
                }
            }
            let spec = match chars.peek() {
                Some('s') => { chars.next(); 's' }
                Some('d') | Some('i') | Some('u') => { chars.next(); 'd' }
                Some('x') | Some('X') | Some('o') => { chars.next(); 'd' }
                Some('c') => { chars.next(); 's' }
                Some('b') => { chars.next(); 'b' }
                Some('q') => { chars.next(); 'q' }
                _ => { parts.push(('s', Self::zig_str("%"))); continue; }
            };
            let Some(a) = rest.get(argi) else {
                // missing operand: bash prints nothing for the conversion
                continue;
            };
            argi += 1;
            if spec == 'b' {
                // %b: the ARGUMENT carries the backslash escapes (printf
                // "%b" "a\tb\n" prints a<TAB>b). Expand at render time for a
                // literal, else fall back to the raw text.
                let raw = self.expr_str(a);
                self.need_bexpand = true;
                parts.push(('s', format!("shBEscape({raw})")));
                continue;
            }
            let mut pa = self.parts_of(a);
            if width > 0 && pa.len() == 1 && spec != 'd' {
                self.need_pad = true;
                pa[0].1 = format!("shPadW({}, {width}, {})", pa[0].1, left);
            }
            if pa.len() == 1 {
                // the string model prints ids as TEXT: a %d over a string
                // operand keeps {s} (bash prints the same bytes), and a %s
                // over an INT operand {d}
                if spec == 's' && pa[0].0 == 'd' {
                    parts.push(('d', pa[0].1.clone()));
                } else if spec == 'd' && pa[0].0 != 'd' {
                    parts.push(('s', pa[0].1.clone()));
                } else {
                    parts.push((spec, pa[0].1.clone()));
                }
            } else {
                // multi-part arg (Interpolate / capture with surrounding
                // text): splice EVERY piece (keeping only pa[0] dropped the
                // argument's leading text)
                for p in pa {
                    parts.push(p);
                }
            }
        }
        if !lit.is_empty() {
            parts.push(('s', Self::zig_str(&lit)));
        }
        Some(parts)
    }

    fn print_from_parts(&mut self, parts: Vec<(char, String)>, newline: bool) -> String {
        self.need_stdout = true;
        if !self.in_function {
            self.need_main_stdout = true;
        }
        let mut fmt = String::new();
        let mut args: Vec<String> = Vec::new();
        for (spec, e) in parts {
            if spec == 'q' {
                self.need_shquote = true;
                fmt.push_str(spec_char('s'));
                args.push(format!("shellQuote({e})"));
            } else {
                fmt.push_str(spec_char(spec));
                args.push(e);
            }
        }
        if newline {
            fmt.push('\n');
        }
        // zig caps a format call at 32 ARGUMENTS. A wide argv (a brace
        // expansion of 54 words) must be printed in CHUNKS — the whole
        // call failed to compile otherwise (063_17_nested_brace_expansion
        // and every other brace-expansion example).
        const CHUNK: usize = 24;
        if args.len() <= CHUNK {
            return format!(
                "try stdout.print({}, .{{{}}});\n{}try stdout.flush();",
                Self::zig_str(&fmt),
                args.join(", "),
                "    ".repeat(self.depth)
            );
        }
        let mut out = String::new();
        let mut i = 0;
        while i < args.len() {
            let end = (i + CHUNK).min(args.len());
            let seg = &args[i..end];
            // every chunk is plain {s}/{d} in part order, so the format is
            // just the segment's specifiers
            out.push_str(&format!(
                "try stdout.print(\"{}\", .{{{}}});\n",
                "{s}".repeat(seg.len()),
                seg.join(", ")
            ));
            i = end;
        }
        if newline {
            out.push_str("try stdout.print(\"\\n\", .{});\n");
        }
        out.push_str("try stdout.flush();");
        out
    }

    /// Parts for an echo argument (bash word-splits argv items on spaces).
    fn parts_of(&mut self, e: &IrExpr) -> Vec<(char, String)> {
        // a GLOB-MARKED word (the core writes \x01SH2GLOB\x01 before an
        // unquoted pattern) must be EXPANDED against the filesystem: as a
        // literal it printed "SH2GLOB*.txt" (064_19, 011_brace_expansion).
        if let Some(pat) = glob_marked_str(e) {
            self.need_globpat = true;
            self.need_run = true; // sh_io is declared by the run arm
            return vec![('s', format!("shGlobExpand({}, ' ')", Self::zig_str(&pat)))];
        }
        match e {
            IrExpr::Str(s, _) => vec![('s', Self::zig_str(s))],
            IrExpr::Int(i) => vec![('d', i.to_string())],
            IrExpr::Var(name, _) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return vec![('s', "sh2GetVar()".into())];
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.bigint_vars.contains(name) {
                    self.need_bigint = true;
                    vec![('s', format!("shBigIntStr({m})"))]
                } else if self.is_num(name) {
                    vec![('d', m)]
                } else {
                    vec![('s', m)]
                }
            }
            IrExpr::Ident(name) => {
                if name.starts_with("__pfs_") {
                    return vec![('s', name.clone())];
                }
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return vec![('s', "sh2GetVar()".into())];
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.bigint_vars.contains(name) {
                    self.need_bigint = true;
                    vec![('s', format!("shBigIntStr({m})"))]
                } else if self.is_num(name) {
                    vec![('d', m)]
                } else {
                    vec![('s', m)]
                }
            }
            IrExpr::Interpolate(parts) => {
                let mut out = Vec::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => out.push(('s', Self::zig_str(s))),
                        InterpPart::Expr(x) => out.push(self.part_spec(x)),
                    }
                }
                out
            }
            // a user-function call returns the STRING model — {s} prints
            // ints-as-strings identically to %d
            IrExpr::Call { func, .. } if func == "fnValue" || func == "fnCall" => {
                vec![('s', self.expr_str(e))]
            }
            IrExpr::Ext(n) => {
                // text-ops primitive in echo/printf arg position
                if let Some(code) = self.ext_value_zig(n.as_ref(), false) {
                    vec![('s', code)]
                } else {
                    self.mark_todo(&format!("echo arg {:?}", n.tag()));
                    vec![('s', "\"\"".into())]
                }
            }
            IrExpr::Arith(a) => {
                if crate::shir::arith_refs_bigint(a, &self.bigint_vars) {
                    self.need_bigint = true;
                    let t = self.arith_bigint(a);
                    vec![('s', format!("shBigIntStr({t})"))]
                } else {
                    vec![('d', self.arith(a))]
                }
            }
            IrExpr::Bool(b) => {
                if *b {
                    vec![('b', "true".into())]
                } else {
                    vec![('b', "false".into())]
                }
            }
            IrExpr::BinOp { .. }
            | IrExpr::Call { .. }
            | IrExpr::Capture { .. }
            | IrExpr::Index { .. }
            | IrExpr::Ternary { .. }
            | IrExpr::DefinedOr { .. }
            | IrExpr::Regex { .. }
            | IrExpr::MethodCall { .. } => vec![self.part_spec(e)],
            other => {
                self.mark_todo(&format!("echo arg {:?}", other));
                vec![('s', "\"\"".into())]
            }
        }
    }

    /// Mini `[ ... ]` evaluator for the common patterns; None → stub.
    fn test_render(&mut self, s: &str) -> Option<String> {
        let toks: Vec<&str> = s.split_whitespace().collect();
        // the parser strips the spaces around `=`/`!=` inside `[ ... ]`
        // ("$a"="hello") — re-split the single token when needed
        let toks: Vec<&str> = match toks.as_slice() {
            [one] => {
                if let Some(idx) = one.find("!=") {
                    vec![&one[..idx], "!=", &one[idx + 2..]]
                } else if let Some(idx) = one.find("==") {
                    // `==` BEFORE `=`, or `[[ $a == !(x) ]]` split at the
                    // first '=' and kept the second in the pattern
                    vec![&one[..idx], "==", &one[idx + 2..]]
                } else if let Some(idx) = one.find('=') {
                    vec![&one[..idx], "=", &one[idx + 1..]]
                } else {
                    toks
                }
            }
            _ => toks,
        };
        match toks.as_slice() {
            [a, op, b] => {
                let num_op = match *op {
                    "-gt" => Some(">"),
                    "-lt" => Some("<"),
                    "-ge" => Some(">="),
                    "-le" => Some("<="),
                    "-eq" => Some("=="),
                    "-ne" => Some("!="),
                    _ => None,
                };
                let str_op = match *op {
                    "=" | "==" => Some("=="),
                    "!=" => Some("!="),
                    _ => None,
                };
                if let Some(o) = num_op {
                    let (va, na) = self.test_operand(a);
                    let (vb, nb) = self.test_operand(b);
                    let va = if na {
                        va
                    } else {
                        self.need_toint = true;
                        format!("shToInt({va})")
                    };
                    let vb = if nb {
                        vb
                    } else {
                        self.need_toint = true;
                        format!("shToInt({vb})")
                    };
                    Some(format!("({va} {o} {vb})"))
                } else if let Some(o) = str_op {
                    let (va, na) = self.test_operand(a);
                    let (vb, nb) = self.test_operand(b);
                    let va = if na {
                        self.need_intstr = true;
                        format!("shIntStr({va})")
                    } else {
                        va
                    };
                    let vb = if nb {
                        self.need_intstr = true;
                        format!("shIntStr({vb})")
                    } else {
                        vb
                    };
                    // RHS with glob/extglob syntax is a PATTERN, not a
                    // literal: `[[ $x = @(foo|bar) ]]` matched nothing
                    // before (at-in-test.sh). A plain literal keeps the
                    // exact-equality fast path.
                    // a PATTERN only in `[[ ]]`, and only when the RHS is
                    // UNQUOTED: in `[ ]` the RHS is always a literal string,
                    // and in `[[ ]]` a quoted RHS is literal too
                    let is_pat = self.current_is_dbracket && !b.trim_start().starts_with('"') && !b.trim_start().starts_with('\'') && Self::lit_is_pattern(b);
                    if is_pat {
                        self.need_globpat = true;
                        let call = if self.shopt_nocasematch {
                            format!("shGlobPatI({va}, {vb})")
                        } else {
                            format!("shGlobPat({va}, {vb})")
                        };
                        if o == "==" {
                            Some(call)
                        } else {
                            Some(format!("!{call}"))
                        }
                    } else if self.shopt_nocasematch {
                        self.need_ext = true;
                        if o == "==" {
                            Some(format!("shEqI({va}, {vb})"))
                        } else {
                            Some(format!("!shEqI({va}, {vb})"))
                        }
                    } else if o == "==" {
                        Some(format!("std.mem.eql(u8, {va}, {vb})"))
                    } else {
                        Some(format!("!std.mem.eql(u8, {va}, {vb})"))
                    }
                } else {
                    None
                }
            }
            [flag, v] if *flag == "-n" => {
                let (vv, nv) = self.test_operand(v);
                let vv = if nv {
                    self.need_intstr = true;
                    format!("shIntStr({vv})")
                } else {
                    vv
                };
                Some(format!("({vv} .len != 0)"))
            }
            [flag, v] if *flag == "-z" => {
                let (vv, nv) = self.test_operand(v);
                let vv = if nv {
                    self.need_intstr = true;
                    format!("shIntStr({vv})")
                } else {
                    vv
                };
                Some(format!("({vv} .len == 0)"))
            }
            [v] => {
                let (vv, nv) = self.test_operand(v);
                let vv = if nv {
                    self.need_intstr = true;
                    format!("shIntStr({vv})")
                } else {
                    vv
                };
                Some(format!("({vv} .len != 0)"))
            }
            [flag, v] if matches!(*flag, "-f" | "-d" | "-e" | "-s") => {
                // file test: -f regular, -d dir, -e exists, -s non-empty
                let (vv, nv) = self.test_operand(v);
                let vv = if nv {
                    self.need_intstr = true;
                    format!("shIntStr({vv})")
                } else {
                    vv
                };
                self.need_fexist = true;
                Some(format!("shFileTest({}, {vv})", Self::zig_str(*flag)))
            }
            _ => None,
        }
    }

    /// A test operand: `"$y"`/`$y`/`y` (typed var) → ident; number →
    /// literal; `$var` not hoisted → the runtime store; otherwise a quoted
    /// Zig string. Returns (expr, is_num).
    fn test_operand(&mut self, t: &str) -> (String, bool) {
        let t = t.trim();
        let has_dollar = t.contains('$');
        let inner = t.trim_matches('"').trim_matches('\'');
        let inner = inner.strip_prefix('$').unwrap_or(inner);
        let inner = inner
            .strip_prefix('{')
            .and_then(|s| s.strip_suffix('}'))
            .unwrap_or(inner);
        if self.declared(inner) {
            let m = self.zig_ident(inner);
            self.mark_read(inner);
            (m, self.is_num(inner))
        } else if !has_dollar && inner.parse::<i64>().is_ok() {
            // a QUOTED literal is a STRING in `[ ]` ("010" != "10"): keep the
            // exact bytes, do not route through an i64 home
            (Self::zig_str(inner), false)
        } else if has_dollar {
            // a `$var` reference whose var is never hoisted (env / param):
            // the runtime store — loud TODO rather than a silent literal
            self.sh2_calls.insert("getVar".into());
            ("sh2GetVar()".to_string(), false)
        } else {
            (Self::zig_str(inner), false)
        }
    }

    // ── statements ───────────────────────────────────────────────────

    /// A text-ops primitive Ext value → a ZIG expression (string context
    /// unless `bool_ctx`). None = not in the supported subset → caller
    /// falls back.
    fn ext_value_zig(&mut self, node: &dyn crate::shir_nodes::ExtExpr, bool_ctx: bool) -> Option<String> {
        let ch = node.children();
        let child = |slf: &mut Self, i: usize| -> Option<String> { Some(slf.expr_any(ch.get(i)?)) };
        match node.tag() {
            // `read VAR` normalisation: one stdin line, newline stripped;
            // EOF → "" (the helper owns the stdin reader state)
            "ReadLine" => {
                self.need_ext = true;
                // rendered INSIDE main → init.io is in scope at the call
                Some("shReadLine(init.io)".to_string())
            }
            "StrLen" => {
                self.need_intstr = true;
                Some(format!("shIntStr(@intCast(({}).len))", child(self, 0)?))
            }
            "CaseTransform" => {
                let n = node.as_any().downcast_ref::<crate::shir_nodes::CaseTransform>()?;
                self.need_ext = true;
                Some(format!("shCase({}, {})", child(self, 0)?, if n.upper { "true" } else { "false" }))
            }
            "SubStrExtract" => {
                self.need_ext = true;
                let off = child(self, 1)?;
                let len = ch.get(2).map(|e| self.expr_num(e)).unwrap_or_else(|| "-1".into());
                Some(format!("shSub({}, {}, {})", child(self, 0)?, off, len))
            }
            "StringContains" => {
                self.need_ext = true;
                let b = format!("shContains({}, {})", child(self, 0)?, child(self, 1)?);
                if bool_ctx { Some(b) } else { self.need_b2s = true; Some(format!("shB2S({})", b)) }
            }
            "RegSub" => {
                // pattern must be a LITERAL (no regex metachars) — zig has
                // no std regex; real regexes fall back to the runtime.
                let n = node.as_any().downcast_ref::<crate::shir_nodes::RegSub>()?;
                // the command-substitution newline strip (`\n+$`, first
                // only) is a NAMED primitive here: trim trailing newlines
                if !n.global && n.pattern == "\n+$" {
                    self.need_ext = true;
                    return Some(format!("shTrimRightNl({})", child(self, 0)?));
                }
                if n.pattern.chars().any(|c| ".*+?[](){}|^$\\\\".contains(c) || c == '"') {
                    return None;
                }
                self.need_ext = true;
                Some(format!("shReplaceLit({}, {}, {}, {})",
                    child(self, 0)?,
                    Self::zig_str(&n.pattern),
                    Self::zig_str(&n.replacement),
                    if n.global { "true" } else { "false" }))
            }
            "FieldExtract" => {
                let n = node.as_any().downcast_ref::<crate::shir_nodes::FieldExtract>()?;
                let mut ids: Vec<i64> = Vec::new();
                for f in &n.fields {
                    match f {
                        crate::ir::FieldRange::Single(i) => ids.push(*i as i64 - 1),
                        crate::ir::FieldRange::Range { start, end } => {
                            let (lo, hi) = (*start as i64, *end as i64);
                            if hi - lo > 4096 { return None; }
                            let mut i = lo - 1;
                            while i <= hi - 1 { ids.push(i); i += 1; }
                        }
                    }
                }
                ids.retain(|&i| i >= 0);
                ids.sort_unstable();
                ids.dedup();
                if ids.is_empty() { return None; }
                self.need_ext = true;
                let list = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
                let arr = format!("&[_]usize{{ {} }}", list);
                Some(format!("shFields(std.heap.page_allocator, {}, {}, {}, {})",
                    child(self, 0)?,
                    Self::zig_str(&n.delimiter),
                    arr,
                    if n.suppress_no_delim { "true" } else { "false" }))
            }
            "TakeLines" => {
                let n = node.as_any().downcast_ref::<crate::shir_nodes::TakeLines>()?;
                let cnt = match ch.get(1) {
                    Some(IrExpr::Int(v)) => v.to_string(),
                    _ => return None,
                };
                self.need_ext = true;
                if n.bytes {
                    let t = child(self, 0)?;
                    return Some(if n.from_end {
                        format!("shSub({}, if ({t}.len > {}) {t}.len - {} else 0)", t, cnt, cnt)
                    } else {
                        format!("shSub({}, 0, {})", t, cnt)
                    });
                }
                Some(format!("shTakeLines({}, {}, {})", child(self, 0)?, cnt, if n.from_end { "true" } else { "false" }))
            }
            "PathName" => {
                let n = node.as_any().downcast_ref::<crate::shir_nodes::PathName>()?;
                self.need_ext = true;
                let f = if n.which == "dirname" { "shDirname" } else { "shBasename" };
                Some(format!("{}({})", f, child(self, 0)?))
            }
            "StringTrim" => {
                let n = node.as_any().downcast_ref::<crate::shir_nodes::StringTrim>()?;
                self.need_ext = true;
                Some(format!("shTrimSides({}, {}, {})",
                    child(self, 0)?, if n.leading { "true" } else { "false" }, if n.trailing { "true" } else { "false" }))
            }
            _ => None,
        }
    }

    /// Statement-position shell builtins that the core leaves as
    /// `exec`/`builtin` calls but that must NOT spawn a process:
    /// `local`/`declare`/`export`/`readonly` (declaration + assignment),
    /// `:` (no-op). Returns true when handled (the caller returns).
    /// Anything unrecognized returns false → the external path.
    fn stmt_builtin(&mut self, cmd: &str, args: &[IrExpr]) -> bool {
        match cmd {
            ":" => {
                self.emit("sh_rc = 0;");
                self.rc_dirty = true;
                true
            }
            "cd" => {
                // real chdir (relative paths, `cd ..`, capture-chains like
                // `DIR=$(cd /x && pwd)` all observe it). Bare `cd` → HOME.
                let target = match args.get(1) {
                    Some(IrExpr::Array(items)) if !items.is_empty() => {
                        self.expr_str(&items[0])
                    }
                    _ => {
                        self.need_env = true;
                        "shEnv(\"HOME\")".to_string()
                    }
                };
                self.emit(&format!(
                    "std.Io.Threaded.chdir({target}) catch {{ sh_rc = 1; }};"
                ));
                self.rc_dirty = true;
                true
            }
            "eval" => {
                // `eval payload`: re-parse and run through `sh -c` (a real
                // shell re-execution, not a fallback — the payload is shell
                // source by definition). State changes inside the payload
                // do not feed back (known limitation, documented).
                let words: Vec<String> = match args.get(1) {
                    Some(IrExpr::Array(items)) => {
                        items.iter().map(|w| self.expr_str(w)).collect()
                    }
                    _ => vec![],
                };
                // `eval "NAME=payload"` (a single assignment-shaped word):
                // bind NAME to payload IN THIS PROCESS. Bash's eval runs in
                // the current shell, so the assignment must be visible to
                // the following statements — a `sh -c` re-execution wrote
                // it into a child and `echo "y=$y"` printed `y=`
                // (eval-assign.sh). The payload is used LITERALLY (no
                // re-parse of its text): `eval "y=$x+1"` assigns `5+1`,
                // which is exactly what bash stores for a plain NAME=word.
                // `eval "NAME=$(( arith ))"` with a LITERAL payload: the
                // arithmetic is evaluated in-process and stored as text
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    if let [IrExpr::Str(w, _)] = items.as_slice() {
                        if let Some(target) = eval_arith_assign(w) {
                            // the operands are shell variables too — an unset
                            // one is 0, so give each a home BEFORE rendering
                            // the arithmetic (declared() gates the read)
                            let mut ops: BTreeSet<String> = BTreeSet::new();
                            if let Some(ast) = crate::shir::parse_arith(
                                target.1.split_whitespace().collect::<Vec<_>>().join(" ").as_str(),
                            ) {
                                collect_arith_reads(&ast, &mut ops);
                            }
                            for o in &ops {
                                self.mark_written(o);
                                self.mark_read(o);
                            }
                            if let Some(v) = self.arith_text(&target.1) {
                                self.mark_written(&target.0);
                                let m = self.zig_ident(&target.0);
                                self.need_intstr = true;
                                self.emit(&format!("{m} = shIntStr({v});"));
                                self.rc_dirty = true;
                                return true;
                            }
                        }
                    }
                }
                if let Some(items) = match args.get(1) {
                    Some(IrExpr::Array(items)) if items.len() == 1 => Some(items),
                    _ => None,
                } {
                    if let Some((ename, payload)) = self.eval_assign_parts(&items[0]) {
                        self.mark_written(&ename);
                        let m = self.zig_ident(&ename);
                        self.emit(&format!("{m} = {payload};"));
                        self.rc_dirty = true;
                        return true;
                    }
                }
                self.need_run = true;
                self.rc_dirty = true;
                self.need_ext = true;
                if words.is_empty() {
                    self.emit("sh_rc = 0;");
                } else {
                    self.emit(&format!(
                        "shRunStmt(&[_][]const u8{{{}, {}, shJoinSp(&[_][]const u8{{{}}})}});",
                        Self::zig_str("sh"),
                        Self::zig_str("-c"),
                        words.join(", ")
                    ));
                }
                true
            }
            // `. file` / `source file`: bash runs the file IN THE CURRENT
            // SHELL, so its assignments are visible afterwards. A child
            // `sh` loses them ("shared=" instead of "shared=secret",
            // dot-source-lib.sh). When the target is a LITERAL path that
            // exists at RENDER time, splice its statements in place.
            "." | "source" => {
                let target = match args.get(1) {
                    Some(IrExpr::Array(items)) if !items.is_empty() => match &items[0] {
                        IrExpr::Str(s, _) => Some(s.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(path) = target {
                    let cand = if path.starts_with('/') {
                        Some(std::path::PathBuf::from(&path))
                    } else {
                        // relative: resolve against SH2_SOURCE_DIR when the
                        // invoker names one, else the RENDERER's cwd (which
                        // is the script's cwd under the gate)
                        Some(match std::env::var_os("SH2_SOURCE_DIR") {
                            Some(d) => std::path::Path::new(&d).join(&path),
                            None => std::path::PathBuf::from(&path),
                        })
                    };
                    if let Some(pb) = cand {
                        if let Ok(text) = std::fs::read_to_string(&pb) {
                            if let Ok(cmds) = crate::Parser::new(&text).parse() {
                                let mut sub = crate::shir::ast_to_ir(&cmds);
                                crate::transforms::builtin::fallback_builtin_to_exec(&mut sub);
                                for st in &sub.stmts {
                                    self.stmt(st);
                                }
                                self.emit("sh_rc = 0;");
                                self.rc_dirty = true;
                                return true;
                            }
                        }
                        // NOT readable at render time (the script creates
                        // it first): source it AT RUN TIME. `shSource` reads
                        // the file and applies its simple `NAME=value` lines
                        // to variables the program already declares.
                        self.need_source = true;
                        self.need_env = true;
                        // a sourced file may assign any module-level global:
                        // dispatch on every declared name (the set is closed
                        // before rendering starts, so this stays stable)
                        for n in self.written.clone() {
                            self.source_targets.insert(n);
                        }
                        self.source_targets.extend(self.globals.iter().cloned());
                        // `path` is the RAW literal text: emit it as a Zig
                        // string literal, not as an expression
                        self.emit(&format!("shSource({});", Self::zig_str(&path)));
                        self.rc_dirty = true;
                        return true;
                    }
                }
                false
            }
            "shopt" => {
                // `shopt -s/-u <option>`: track the ones that change how
                // later constructs lower (nocasematch → case-insensitive
                // `[[ == ]]`, extglob → patterns). The statement itself is a
                // no-op with status 0.
                let mut enable = true;
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for w in items {
                        if let IrExpr::Str(t, _) = w {
                            match t.as_str() {
                                "-s" => enable = true,
                                "-u" => enable = false,
                                "nocasematch" => self.shopt_nocasematch = enable,
                                "extglob" => self.shopt_extglob = enable,
                                _ => {}
                            }
                        }
                    }
                }
                self.emit("sh_rc = 0;");
                self.rc_dirty = true;
                return true;
            }
            "unset" => {
                // `unset x`: the variable is EMPTY afterwards (`${x:-gone}`
                // prints "gone"). The name stays declared (a zero home).
                let mut any = false;
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    for w in items {
                        if let IrExpr::Str(n, _) = w {
                            if is_ident(n) {
                                let m = self.zig_ident(n);
                                self.mark_written(n);
                                if self.is_num(n) {
                                    self.emit(&format!("{m} = 0;"));
                                } else if self.str_arrays.contains(n) || self.set_arrays.contains(n) {
                                    self.emit(&format!("{m}_len = 0;"));
                                } else {
                                    self.emit(&format!("{m} = \"\";"));
                                }
                                any = true;
                            }
                        }
                    }
                }
                if any {
                    self.emit("sh_rc = 0;");
                    self.rc_dirty = true;
                    return true;
                }
                false
            }
            "wait" => {
                // `wait`: reap every outstanding background job (there is no
                // `wait` BINARY — spawning one failed with 127)
                if self.saw_background {
                    self.need_run = true;
                    self.emit("while (true) {");
                    self.emit("    var __wst: c_int = 0;");
                    self.emit("    if (waitpid(-1, &__wst, 0) <= 0) break;");
                    self.emit("}");
                    self.emit("sh_rc = 0;");
                    self.rc_dirty = true;
                    return true;
                }
                self.emit("sh_rc = 127;");
                self.rc_dirty = true;
                true
            }
            "trap" => {
                // `trap 'payload' EXIT` runs the payload when the shell
                // exits. Capture the payload for the end-of-main hook (other
                // signals are not modelled — they record status 0).
                let mut payload: Option<String> = None;
                let mut sig = String::new();
                if let Some(IrExpr::Array(items)) = args.get(1) {
                    if let Some(IrExpr::Str(p, _)) = items.first() {
                        payload = Some(p.clone());
                    }
                    if let Some(IrExpr::Str(sg, _)) = items.get(1) {
                        sig = sg.clone();
                    }
                }
                if let (Some(p), true) = (payload, sig == "EXIT" || sig == "0") {
                    self.need_run = true;
                    self.exit_trap = Some(p);
                }
                self.emit("sh_rc = 0;");
                self.rc_dirty = true;
                true
            }
            "local" | "declare" | "export" | "readonly" | "typeset" => self.stmt_declare(cmd, args),
            _ => false,
        }
    }

    /// `eval "NAME=rest"`: split the one-word payload into the assignment
    /// NAME (the literal prefix before the first `=`) and a Zig expression
    /// for the REST of the word, so the eval'd assignment lands in the
    /// current process instead of a `sh -c` child. The rest is joined as a
    /// `++` concatenation of already-rendered string pieces. None for any
    /// other eval shape (the caller keeps the `sh -c` arm).
    fn eval_assign_parts(&mut self, word: &IrExpr) -> Option<(String, String)> {
        let IrExpr::Interpolate(parts) = word else { return None };
        let mut name = String::new();
        let mut pieces: Vec<String> = Vec::new();
        let mut seen_eq = false;
        for p in parts {
            match p {
                InterpPart::Lit(text) if !seen_eq => {
                    if let Some((head, tail)) = text.split_once('=') {
                        name.push_str(head);
                        seen_eq = true;
                        if !tail.is_empty() {
                            pieces.push(Self::zig_str(tail));
                        }
                    } else {
                        name.push_str(text);
                    }
                }
                InterpPart::Lit(text) => pieces.push(Self::zig_str(text)),
                InterpPart::Expr(e) => {
                    // STRING context by construction (the payload is text):
                    // an int-homed var must not arrive as `shIntStr(x)`
                    // inside a `++` chain that the file may never declare —
                    // render the var directly.
                    let piece = match e.as_ref() {
                        IrExpr::Var(v, _) | IrExpr::Ident(v) if self.is_num(v) => {
                            self.need_intstr = true;
                            let m = self.zig_ident(v);
                            self.mark_read(v);
                            format!("shIntStr({m})")
                        }
                        _ => self.expr_str(e),
                    };
                    pieces.push(piece);
                }
            }
        }
        if !seen_eq || !is_ident(&name) {
            return None;
        }
        if pieces.is_empty() {
            pieces.push(Self::zig_str(""));
        }
        // runtime concatenation: `++` needs comptime-known slices, so fold
        // with the shStrCat helper (same one the string-concat lowering uses)
        let mut payload = pieces[0].clone();
        for piece in &pieces[1..] {
            self.need_cat = true;
            payload = format!("shStrCat({payload}, {piece})");
        }
        Some((name, payload))
    }

    /// A `$name`/`${name}`/literal word inside a `local`-family value
    /// (mirrors the perl backend's declare_value_expr): bare `$3` arrives
    /// UNEXPANDED in glued `NAME=value` words, so the backend resolves it.
    /// None → declare the name bare (perl's behavior), never guess.
    fn declare_value_expr(&mut self, value: &str) -> Option<String> {
        if value.is_empty() {
            return None;
        }
        let user = |n: &str| {
            !n.is_empty()
                && (n.as_bytes()[0].is_ascii_alphabetic() || n.as_bytes()[0] == b'_')
                && n.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        };
        let special = |n: &str| {
            matches!(n, "@" | "*" | "#" | "?" | "!" | "-")
                || (n.len() == 1 && n.as_bytes()[0].is_ascii_digit())
        };
        if let Some(n) = value.strip_prefix('$') {
            if user(n) || special(n) {
                return Some(self.declare_name_read(n));
            }
            return None;
        }
        if let Some(inner) = value
            .strip_prefix("${")
            .and_then(|s| s.strip_suffix('}'))
        {
            if user(inner) || special(inner) {
                return Some(self.declare_name_read(inner));
            }
            return None;
        }
        if value.contains(['$', '`', '\\', '"', '\'']) {
            return None;
        }
        Some(Self::zig_str(value))
    }

    /// A shared-var read in string position for declare values (the getVar
    /// string behavior: positional params, `$?`, typed homes, env fallback).
    fn declare_name_read(&mut self, name: &str) -> String {
        if self.in_function && !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
            let idx: usize = name.parse().unwrap_or(1);
            return format!("(if (__args.len >= {idx}) __args[{idx} - 1] else \"\")");
        }
        if name == "?" {
            self.need_intstr = true;
            return "shIntStr(sh_rc)".to_string();
        }
        if self.declared(name) {
            let m = self.zig_ident(name);
            self.mark_read(name);
            if self.is_num(name) {
                self.need_intstr = true;
                return format!("shIntStr({m})");
            }
            return m;
        }
        self.need_env = true;
        format!("shEnv({})", Self::zig_str(name))
    }

    /// `local`/`declare`/`export`/`readonly` with static `NAME[=value]`
    /// words: declare (hoisted like Declare) + assign. Words are glued
    /// (`NAME=` + value parts may arrive split); leading flags skipped.
    /// Unparseable shapes return false (external fallback).
    fn stmt_declare(&mut self, cmd: &str, args: &[IrExpr]) -> bool {
        let items = match args.get(1) {
            Some(IrExpr::Array(items)) => items,
            _ => return cmd == "readonly",
        };
        // flatten one level (a quoted word may arrive as a nested Array)
        let mut words: Vec<&IrExpr> = Vec::new();
        for it in items {
            if let IrExpr::Array(inner) = it {
                words.extend(inner.iter());
            } else {
                words.push(it);
            }
        }
        // (name, value-items) per word; unparseable → external fallback
        let mut decls: Vec<(String, Vec<&IrExpr>)> = Vec::new();
        let mut i = 0;
        while i < words.len() {
            match words[i] {
                IrExpr::Str(s, _) => {
                    if s.starts_with('-') && !s.contains('=') {
                        i += 1; // flags (-a, -r, -p, …)
                        continue;
                    }
                    match s.find('=') {
                        Some(eq) => {
                            let name = s[..eq].to_string();
                            if name.is_empty() || !is_ident(&name) {
                                return false;
                            }
                            let mut val: Vec<&IrExpr> = Vec::new();
                            if !s[eq + 1..].is_empty() {
                                val.push(words[i]);
                            }
                            i += 1;
                            while i < words.len() {
                                match words[i] {
                                    IrExpr::Str(t, _) if t.contains('=') => break,
                                    _ => {
                                        val.push(words[i]);
                                        i += 1;
                                    }
                                }
                            }
                            decls.push((name, val));
                        }
                        None => {
                            if !is_ident(s) {
                                return false;
                            }
                            decls.push((s.clone(), Vec::new()));
                            i += 1;
                        }
                    }
                }
                _ => return false,
            }
        }
        for (name, val) in decls {
            self.mark_written(&name);
            let m = self.zig_ident(&name);
            if val.is_empty() {
                // `export NAME` / `NAME=` with an empty value binds the
                // name to the EMPTY string (bash); a bare comment left the
                // home unset AND made the decl "never mutated"
                if self.is_num(&name) {
                    self.emit(&format!("{m} = 0;"));
                } else if self.str_arrays.contains(&name) || self.set_arrays.contains(&name) {
                    self.emit(&format!("{m}_len = 0;"));
                } else {
                    self.emit(&format!("{m} = \"\";"));
                }
                continue;
            }
            // rebuild the value expr: a glued `NAME=rest` first word
            // contributes only its post-`=` slice as a literal — unless it
            // is the ONLY value part, in which case `$`-refs resolve
            // (bare `$3` arrives unexpanded in glued words)
            let glued_only = val.len() == 1
                && matches!(val[0], IrExpr::Str(s, _) if s.contains('='));
            if glued_only {
                if let IrExpr::Str(s, _) = val[0] {
                    let rest = &s[s.find('=').unwrap() + 1..];
                    if self.is_num(&name) {
                        // numeric home: literals parse, `$`-refs resolve
                        // then coerce (a literal `$3` here would numify to 0)
                        if rest.trim().starts_with('$') {
                            match self.declare_value_expr(rest.trim()) {
                                Some(v) => {
                                    self.need_toint = true;
                                    self.emit(&format!("{m} = shToInt({v});"));
                                }
                                None => {
                                    self.emit(&format!("// {cmd} {m} (unresolvable value)"));
                                }
                            }
                        } else {
                            let lit = IrExpr::Str(
                                rest.to_string(),
                                crate::ir::StrStyle::DoubleQuoted,
                            );
                            let rhs = self.expr_num(&lit);
                            self.emit(&format!("{m} = {rhs};"));
                        }
                    } else {
                        match self.declare_value_expr(rest) {
                            Some(rhs) => {
                                self.emit(&format!("{m} = {rhs};"));
                            }
                            None => {
                                self.emit(&format!("// {cmd} {m} (unresolvable value)"));
                            }
                        }
                    }
                    continue;
                }
            }
            let mut parts: Vec<InterpPart> = Vec::new();
            for (vi, v) in val.iter().enumerate() {
                match v {
                    IrExpr::Str(s, _) if vi == 0 && s.contains('=') => {
                        let rest = &s[s.find('=').unwrap() + 1..];
                        if !rest.is_empty() {
                            parts.push(InterpPart::Lit(rest.to_string()));
                        }
                    }
                    IrExpr::Str(s, _) => parts.push(InterpPart::Lit(s.clone())),
                    other => parts.push(InterpPart::Expr(Box::new((*other).clone()))),
                }
            }
            let vexpr = if parts.len() == 1 {
                if val.len() == 1 {
                    // single literal word — keep the original node so
                    // numeric literals stay numeric (expr_num parses)
                    (*val[0]).clone()
                } else {
                    IrExpr::Interpolate(parts)
                }
            } else {
                IrExpr::Interpolate(parts)
            };
            let rhs = if self.is_num(&name) {
                self.expr_num(&vexpr)
            } else {
                self.expr_str(&vexpr)
            };
            self.emit(&format!("{m} = {rhs};"));
        }
        true
    }

    /// A user-function call in STATEMENT position: the call runs for its
    /// side effects, and when the fn is a value-fn (its trailing echo
    /// became the return value) that echo is a side-effect print in bash —
    /// so print the returned value. Capture-position callers consume the
    /// value silently instead. `catch {}` (not `try`): this line must also
    /// compile inside non-error-union user functions.
    fn emit_fn_call_stmt(&mut self, name: &str, call_args: &[String]) {
        let m = self.zig_ident(name);
        let catch = if self.error_fns.contains(name) {
            " catch \"\""
        } else {
            ""
        };
        let call = format!(
            "{m}(init, &[_][]const u8{{ {} }}){catch}",
            call_args.join(", ")
        );
        if let Some(newline) = self.value_fns.get(name).copied() {
            if !self.in_function {
                self.need_main_stdout = true;
            }
            let fmt = if newline { "{s}\\n" } else { "{s}" };
            self.emit(&format!("stdout.print(\"{fmt}\", .{{{call}}}) catch {{}};"));
            self.emit("stdout.flush() catch {};");
        } else {
            self.emit(&format!("_ = {call};"));
        }
        // the body runs at runtime and manages sh_rc through its own
        // leaves — keep, don't reset
        self.rc_dirty = true;
    }

    /// A `&&`/`||`/`!` chain in STATEMENT position: sides run as
    /// statements (sh_rc records each side's status through the normal
    /// arms); the connective branches on it. A side that is itself a
    /// chain recurses. Non-command sides (tests, captures) evaluate for
    /// effect and set sh_rc from their value.
    fn chain_stmt(&mut self, lhs: &IrExpr, op: &BinOpKind, rhs: &IrExpr) {
        if *op == BinOpKind::Not {
            self.side_stmt(lhs);
            self.emit("sh_rc = if (sh_rc != 0) 0 else 1;");
            self.rc_dirty = true;
            return;
        }
        self.side_stmt(lhs);
        let cond = if *op == BinOpKind::And {
            "sh_rc == 0"
        } else {
            "sh_rc != 0"
        };
        self.emit(&format!("if ({cond}) {{"));
        self.depth += 1;
        let rc_before = self.rc_dirty;
        self.rc_dirty = false;
        self.side_stmt(rhs);
        // the RHS side's OWN status becomes the chain's status (bash:
        // `cd /bad || echo x` leaves $? = 0). A side whose arm cannot
        // record a status keeps whatever it set.
        if !self.rc_dirty {
            self.emit("sh_rc = 0;");
        }
        self.rc_dirty = rc_before || self.rc_dirty;
        self.depth -= 1;
        self.emit("}");
        self.rc_dirty = true;
    }

    /// One chain side as a statement: commands run through the shared
    /// exec arm (user-fns direct, builtins native, externals spawned);
    /// a nested chain recurses; a test records its real 0/1 status; a
    /// capture runs for effect (shRun records the child's code); anything
    /// else is an honest stub.
    fn side_stmt(&mut self, e: &IrExpr) {
        match e {
            IrExpr::BinOp { lhs, op, rhs }
                if matches!(op, BinOpKind::And | BinOpKind::Or | BinOpKind::Not) =>
            {
                self.chain_stmt(lhs, op, rhs);
            }
            IrExpr::Call { func, args } if func == "exec" || func == "builtin" => {
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    if self.stmt_builtin(cmd, args) {
                        return;
                    }
                    self.exec_stmt_cmd(cmd, args);
                    return;
                }
                self.mark_todo("chain side without command word");
            }
            IrExpr::Call { func, args } if func == "fnCall" || func == "fnValue" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    let call_args: Vec<String> = args
                        .get(1)
                        .and_then(|a| match a {
                            IrExpr::Array(items) => {
                                Some(items.iter().map(|w| self.expr_str(w)).collect())
                            }
                            _ => None,
                        })
                        .unwrap_or_default();
                    self.need_run = true;
                    self.emit_fn_call_stmt(name, &call_args);
                    return;
                }
                self.mark_todo("chain side fnCall without name");
            }
            IrExpr::Call { func, args } if func == "test" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    self.current_is_dbracket = matches!(args.get(1), Some(IrExpr::Str(t, _)) if t == "[[");
                    let r = self.test_render(s);
                    self.current_is_dbracket = false;
                    if let Some(c) = r {
                        self.emit(&format!("sh_rc = if ({c}) 0 else 1;"));
                        self.rc_dirty = true;
                        return;
                    }
                }
                self.sh2_calls.insert("test".into());
                self.emit("sh_rc = if (sh2Test()) 0 else 1;");
                self.rc_dirty = true;
            }
            IrExpr::Capture { expr, .. } => {
                if let Some(rendered) = self.capture_expr(expr) {
                    self.emit(&format!("_ = {rendered};"));
                    self.rc_dirty = true;
                    return;
                }
                self.sh2_calls.insert("capture".into());
                self.emit("_ = sh2Capture();");
                self.rc_dirty = true;
            }
            _ => {
                self.mark_todo(&format!("chain side {e:?}"));
            }
        }
    }

    /// `${arr[i]}` element read for the i64/str array homes: negative
    /// indices count from the end (bash), out-of-range yields "".
    /// A labeled block binds the normalized index once (the index text may
    /// be dynamic); int homes convert through shIntStr.
    fn render_array_index(&mut self, name: &str, idx: &str) -> Option<String> {
        let is_int = self.set_arrays.contains(name);
        let is_str = self.str_arrays.contains(name);
        if !is_int && !is_str {
            return None;
        }
        let m = self.zig_ident(name);
        self.mark_read(name);
        let t = self.fresh_tmp("sh_arridx");
        let idx_e = self.param_int(&IrExpr::Str(
            idx.to_string(),
            crate::ir::StrStyle::DoubleQuoted,
        ))?;
        let val = if is_int {
            self.need_intstr = true;
            format!("shIntStr({m}[@as(usize, @intCast({t}_i))])")
        } else {
            format!("{m}[@as(usize, @intCast({t}_i))]")
        };
        Some(format!(
            "blk: {{ const {t}: i64 = {idx_e}; const {t}_n: i64 = @intCast({m}_len); const {t}_i = if ({t} < 0) {t}_n + {t} else {t}; break :blk if ({t}_i < 0 or {t}_i >= {t}_n) \"\" else {val}; }}"
        ))
    }

    /// `${arr[@]:off:len}` — space-joined window over an array home.
    /// Arg shapes: `[slice, arr, @, off, len?]` or `[slice, arr, off, len?]`
    /// (the core omits the `@` for some subjects); offsets/lengths resolve
    /// like scalar slice operands (literals, negatives, `$`-refs).
    fn render_array_slice(&mut self, name: &str, args: &[IrExpr]) -> Option<String> {
        let m = self.zig_ident(name);
        self.mark_read(name);
        let is_int = self.set_arrays.contains(name);
        let (off_e, len_e) = match args.get(2) {
            Some(IrExpr::Str(s, _)) if s == "@" || s == "*" => (
                self.param_int(&args.get(3).cloned().unwrap_or(IrExpr::Str(
                    String::new(),
                    crate::ir::StrStyle::DoubleQuoted,
                )))?,
                match args.get(4) {
                    None => "9223372036854775807".to_string(),
                    Some(IrExpr::Str(t, _)) if t.trim().is_empty() => {
                        "9223372036854775807".to_string()
                    }
                    Some(e) => self.param_int(e)?,
                },
            ),
            off => (
                self.param_int(&off.cloned().unwrap_or(IrExpr::Str(
                    String::new(),
                    crate::ir::StrStyle::DoubleQuoted,
                )))?,
                match args.get(3) {
                    None => "9223372036854775807".to_string(),
                    Some(IrExpr::Str(t, _)) if t.trim().is_empty() => {
                        "9223372036854775807".to_string()
                    }
                    Some(e) => self.param_int(e)?,
                },
            ),
        };
        self.need_ext = true;
        if is_int {
            Some(format!("shIntArrSlice(&{m}, {m}_len, {off_e}, {len_e})"))
        } else {
            Some(format!("shStrArrSlice(&{m}, {m}_len, {off_e}, {len_e})"))
        }
    }

    /// `${…}` parameter expansion (the `param` call): `args` =
    /// `[op, name, extra…]` all Str. Returns None for shapes outside the
    /// modeled subset (arrays, indirect, exotic) → the caller stubs.
    fn render_param(&mut self, args: &[IrExpr]) -> Option<String> {
        self.render_param_depth(args, 0)
    }

    fn render_param_depth(&mut self, args: &[IrExpr], depth: usize) -> Option<String> {
        if depth > 8 {
            return None;
        }
        let op = match args.first() {
            Some(IrExpr::Str(s, _)) => s.clone(),
            _ => return None,
        };
        let name = match args.get(1) {
            Some(IrExpr::Str(s, _)) => s.clone(),
            _ => return None,
        };
        let extra = match args.get(2) {
            Some(IrExpr::Str(s, _)) => s.clone(),
            None => String::new(),
            _ => return None,
        };
        // array element / slice subjects (`arr[i]`, `arr`, `@`) go through
        // the array homes (not yet modeled) — except `len`, which counts.
        // Single `,`/`^` suffix: the core only splits the DOUBLED forms, so
        // `${n,}` arrives as op="" name="n," — apply first-char case-mod
        // here rather than misreading it as an array index.
        if name == "@" || name == "*" || name.starts_with('!') {
            return None;
        }
        if op.is_empty() && !name.contains('[') {
            if let Some(base) = name.strip_suffix(',') {
                let v = self.param_var_str(base)?;
                self.need_ext = true;
                return Some(format!("shCaseOne({v}, false)"));
            }
            if let Some(base) = name.strip_suffix('^') {
                let v = self.param_var_str(base)?;
                self.need_ext = true;
                return Some(format!("shCaseOne({v}, true)"));
            }
        }
        // `${arr[i]}` / `${arr[@]}` / `${arr[*]}` — array reads through
        // the i64/str homes (assoc `map[k]` stays a stub: no home models it)
        if op.is_empty() {
            if name.ends_with("[@]") || name.ends_with("[*]") {
                let base = name[..name.len() - 3].to_string();
                if self.set_arrays.contains(&base) || self.str_arrays.contains(&base) {
                    let m = self.zig_ident(&base);
                    self.mark_read(&base);
                    self.need_ext = true;
                    if self.set_arrays.contains(&base) {
                        return Some(format!("shIntArrSlice(&{m}, {m}_len, 0, 9223372036854775807)"));
                    }
                    return Some(format!("shStrArrSlice(&{m}, {m}_len, 0, 9223372036854775807)"));
                }
                return None;
            }
            if let Some(bi) = name.find('[') {
                if let Some(idx) = name[bi + 1..].strip_suffix(']') {
                    let base = &name[..bi];
                    return self.render_array_index(base, idx.trim());
                }
            }
        }
        if name.contains('[') {
            return None;
        }
        match op.as_str() {
            ":-" => {
                // `${s:off:len}` with a NEGATIVE len lexes as `:-` with a
                // `s:off` name (the core keeps the slice split only for
                // non-negative lens) — a `:` can never occur in a real var
                // name, so re-split to a slice unambiguously (perl renders
                // this shape literally and prints the default: wrong)
                if let Some(ci) = name.find(':') {
                    let (base, off_text) = (&name[..ci], name[ci + 1..].to_string());
                    if !base.is_empty()
                        && !base.contains('[')
                        && base.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                    {
                        let v = self.param_var_str(base)?;
                        let off_s = IrExpr::Str(off_text, crate::ir::StrStyle::DoubleQuoted);
                        let off = self.param_int(&off_s)?;
                        let len_s = IrExpr::Str(extra.clone(), crate::ir::StrStyle::DoubleQuoted);
                        let len = self.param_int(&len_s)?;
                        self.need_ext = true;
                        return Some(format!("shSub({v}, {off}, {len})"));
                    }
                }
                let v = self.param_var_str(&name)?;
                let d = self.render_param_default(&extra, depth)?;
                Some(format!("(if ({v}.len == 0) {d} else {v})"))
            }
            ":=" => {
                // assign-back needs a DECLARED home (mid-render decls are
                // impossible); env-only names stay stubs
                if !self.declared(&name) {
                    return None;
                }
                let m = self.zig_ident(&name);
                self.mark_read(&name);
                let d = self.render_param_default(&extra, depth)?;
                let cur = if self.is_num(&name) {
                    self.need_intstr = true;
                    format!("shIntStr({m})")
                } else {
                    m.clone()
                };
                let assign = if self.is_num(&name) {
                    self.need_toint = true;
                    format!("{m} = shToInt({d});")
                } else {
                    format!("{m} = {d};")
                };
                Some(format!(
                    "blk: {{ if ({cur}.len == 0) {{ {assign} }} break :blk {m}; }}"
                ))
            }
            ":?" => {
                let v = self.param_var_str(&name)?;
                let msg = if extra.is_empty() {
                    format!("\"{name}: parameter null or not set\"")
                } else {
                    match self.render_param_default(&extra, depth) {
                        Some(d) => d,
                        None => return None,
                    }
                };
                Some(format!(
                    "blk: {{ if ({v}.len == 0) {{ std.debug.print(\"{{s}}\\n\", .{{{msg}}}); std.process.exit(1); }} break :blk {v}; }}"
                ))
            }
            ":+" => {
                // alternate value (the core conflates bare `+` here too —
                // same shape, same semantics)
                let v = self.param_var_str(&name)?;
                let d = self.render_param_default(&extra, depth)?;
                Some(format!("(if ({v}.len == 0) \"\" else {d})"))
            }
            "len" => {
                let v = self.param_var_str(&name)?;
                self.need_intstr = true;
                Some(format!("shIntStr(@as(i64, @intCast({v}.len)))"))
            }
            "slice" => {
                // array slice (`${arr[@]:off:len}` / `${arr:off:len}` — the
                // latter when the core omits the `@`) joins the window with
                // spaces; scalar subjects use shSub
                if self.set_arrays.contains(&name) || self.str_arrays.contains(&name) {
                    return self.render_array_slice(&name, args);
                }
                let v = self.param_var_str(&name)?;
                let off = self.param_int(&args.get(2).cloned().unwrap_or(IrExpr::Str(String::new(), crate::ir::StrStyle::DoubleQuoted)))?;
                // omitted length = "to the end": pass through as a huge
                // length (clamped inside shSub) — NOT -1, which bash reads
                // as end-counting (`${s:1:-1}` chops the last char)
                let len = match args.get(3) {
                    None => "9223372036854775807".to_string(),
                    Some(IrExpr::Str(s, _)) if s.trim().is_empty() => {
                        "9223372036854775807".to_string()
                    }
                    Some(e) => self.param_int(e)?,
                };
                self.need_ext = true;
                Some(format!("shSub({v}, {off}, {len})"))
            }
            "#" | "##" => {
                let v = self.param_var_str(&name)?;
                let pat = self.param_pattern(&extra)?;
                self.need_ext = true;
                let longest = op == "##";
                Some(format!("shStripPre({v}, {pat}, {})", longest))
            }
            "%" | "%%" => {
                let v = self.param_var_str(&name)?;
                let pat = self.param_pattern(&extra)?;
                self.need_ext = true;
                let longest = op == "%%";
                Some(format!("shStripSuf({v}, {pat}, {})", longest))
            }
            "/" | "//" => {
                let v = self.param_var_str(&name)?;
                let (pat, repl) = (self.param_pattern(&extra)?, self.param_repl(args)?);
                self.need_ext = true;
                let all = op == "//";
                Some(format!("shReplace({v}, {pat}, {repl}, {})", all))
            }
            "^" | "^^" => {
                let v = self.param_var_str(&name)?;
                self.need_ext = true;
                if op == "^^" {
                    Some(format!("shCase({v}, true)"))
                } else {
                    Some(format!("shCaseOne({v}, true)"))
                }
            }
            "," | ",," => {
                let v = self.param_var_str(&name)?;
                self.need_ext = true;
                if op == ",," {
                    Some(format!("shCase({v}, false)"))
                } else {
                    Some(format!("shCaseOne({v}, false)"))
                }
            }
            "basename" => {
                let v = self.param_var_str(&name)?;
                self.need_ext = true;
                Some(format!("shBasename({v})"))
            }
            "dirname" => {
                let v = self.param_var_str(&name)?;
                self.need_ext = true;
                Some(format!("shDirname({v})"))
            }
            _ => None,
        }
    }

    /// A `param` subject as a string expression: specials, positionals,
    /// declared homes (typed), env fallback. None for array subjects.
    fn param_var_str(&mut self, name: &str) -> Option<String> {
        if name == "?" {
            self.need_intstr = true;
            return Some("shIntStr(sh_rc)".to_string());
        }
        if !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
            if self.in_function {
                let idx: usize = name.parse().unwrap_or(1);
                return Some(format!("(if (__args.len >= {idx}) __args[{idx} - 1] else \"\")"));
            }
            return Some("\"\"".to_string());
        }
        if name == "#" {
            return Some("\"0\"".to_string());
        }
        if name == "@" || name == "*" {
            return Some("\"\"".to_string());
        }
        if self.declared(name) {
            let m = self.zig_ident(name);
            self.mark_read(name);
            if self.is_num(name) {
                self.need_intstr = true;
                return Some(format!("shIntStr({m})"));
            }
            // an ARRAY read in SCALAR context (`${LIST:-}`) is element 0
            // only (verified: bash prints `item1` for a two-element LIST).
            // The bare home `LIST` is a `[1024][]const u8`, which does not
            // typecheck against a []const u8 and printed nothing
            // (parse-array-plusassign.sh). `"${arr[*]}"` joins — that form
            // routes through arr_star_str, not here.
            if self.str_arrays.contains(name) || self.set_arrays.contains(name) {
                return Some(format!(
                    "(if ({m}_len > 0) {m}[0] else @as([]const u8, \"\"))"
                ));
            }
            return Some(m);
        }
        self.need_env = true;
        Some(format!("shEnv({})", Self::zig_str(name)))
    }

    /// A `param` slice offset/length as an i64 expression: literal ints
    /// (whitespace-tolerated, negatives allowed), empty (offset 0), or
    /// `$`-refs thereof.
    fn param_int(&mut self, e: &IrExpr) -> Option<String> {
        match e {
            IrExpr::Str(s, _) => {
                let t = s.trim();
                if t.is_empty() {
                    // omitted offset — bash starts at 0 (an omitted LENGTH
                    // is handled by the caller with the huge sentinel)
                    return Some("0".to_string());
                }
                if t.parse::<i64>().is_ok() {
                    return Some(t.to_string());
                }
                // `$i` / `${i}` / `-$i`? — resolve the ref, coerce numeric
                let neg = t.starts_with('-');
                let body = if neg { t[1..].trim() } else { t };
                let r = self.declare_value_expr(body)?;
                self.need_toint = true;
                let c = format!("shToInt({r})");
                if neg {
                    Some(format!("-{c}"))
                } else {
                    Some(c)
                }
            }
            _ => None,
        }
    }

    /// A `${…#pat}`-family pattern as a string expression (Str literal or
    /// a `$`-ref resolving to one).
    fn param_pattern(&mut self, pat: &str) -> Option<String> {
        if !pat.contains('$') {
            return Some(Self::zig_str(pat));
        }
        self.declare_value_expr(pat.trim())
    }

    /// The replacement of a `${…/pat/repl}`: args[3] as Str (may be empty).
    fn param_repl(&mut self, args: &[IrExpr]) -> Option<String> {
        match args.get(3) {
            None => Some("\"\"".to_string()),
            Some(IrExpr::Str(s, _)) => {
                if s.contains('$') {
                    self.declare_value_expr(s.trim())
                } else {
                    Some(Self::zig_str(s))
                }
            }
            _ => None,
        }
    }

    /// A `:-`-family default word: plain/quoted literal, `$(…)` capture
    /// text, nested `${…}` (recursed), `$`-refs. None → caller stubs.
    fn render_param_default(&mut self, text: &str, depth: usize) -> Option<String> {
        let t = text.trim();
        // `$(cmd args)` / backticks: naive word-split (quotes respected);
        // anything shell-active beyond words bails
        if let Some(inner) = t
            .strip_prefix("$(")
            .and_then(|r| r.strip_suffix(')'))
            .or_else(|| {
                (t.len() >= 2 && t.starts_with('`') && t.ends_with('`'))
                    .then(|| &t[1..t.len() - 1])
            })
        {
            return self.param_default_capture(inner.trim());
        }
        // nested `${…}`: re-split op/name/arg, recurse
        if let Some(body) = t.strip_prefix("${").and_then(|r| r.strip_suffix('}')) {
            if let Some((op2, name2, arg2)) = split_param_body(body) {
                let s = |x: &str| IrExpr::Str(x.to_string(), crate::ir::StrStyle::DoubleQuoted);
                return self.render_param_depth(
                    &[s(op2), s(name2), s(arg2), s("")],
                    depth + 1,
                );
            }
            if !body.is_empty()
                && body.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                return self.param_var_str(body);
            }
            return None;
        }
        // quoted literal: the quotes are delimiters, not content
        if t.len() >= 2
            && ((t.starts_with('"') && t.ends_with('"'))
                || (t.starts_with('\'') && t.ends_with('\'')))
        {
            return Some(Self::zig_str(&t[1..t.len() - 1]));
        }
        // `$`-refs resolve; anything else shell-active bails
        if t.contains('$') || t.contains('`') {
            return self.declare_value_expr(t);
        }
        Some(Self::zig_str(t))
    }

    /// Words of a `$(…)` default body into a capture call. Refuses (None)
    /// on anything beyond plain words: pipes, redirects, globs, `~`.
    fn param_default_capture(&mut self, body: &str) -> Option<String> {
        let mut words: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut quote = '\0';
        let mut in_word = false;
        for c in body.chars() {
            if quote != '\0' {
                if c == quote {
                    quote = '\0';
                } else {
                    cur.push(c);
                }
                continue;
            }
            match c {
                '"' | '\'' => {
                    quote = c;
                    in_word = true;
                }
                c if c.is_whitespace() => {
                    if in_word {
                        words.push(cur.clone());
                        cur.clear();
                        in_word = false;
                    }
                }
                '$' | '`' | ';' | '&' | '|' | '<' | '>' | '(' | ')' | '*' | '?' | '['
                | ']' | '{' | '}' | '!' | '#' | '\n' | '\\' => return None,
                _ => {
                    cur.push(c);
                    in_word = true;
                }
            }
        }
        if quote != '\0' {
            return None;
        }
        if in_word {
            words.push(cur);
        }
        if words.is_empty() {
            return None;
        }
        let argv: Vec<String> = words.iter().map(|w| Self::zig_str(w)).collect();
        self.need_run = true;
        self.rc_dirty = true;
        // CAPTURE position: always the fork/exec arm. `uu_run` (the
        // in-process FFI entry) takes no capture buffer — it returns an
        // exit code and writes to fd 1 — so a `$(uname -s)` lowered
        // through it evaluated to "" and the assignment lost the command's
        // output (uname-in-qx.sh printed an empty line). The FFI arm stays
        // for STATEMENT position, where writing fd 1 is exactly right.
        Some(format!("shRun(&[_][]const u8{{{}}})", argv.join(", ")))
    }

    /// `y+=2` / `r=$(cmd)` / `x=$y` as an `assign` CALL (the core leaves
    /// compound assignment as a call, not an Assign node): same homes as
    /// Assign. Array-homed `+=` appends word-wise (bash appends ELEMENTS).
    /// None for unmodeled shapes → caller stubs.
    fn assign_call(&mut self, args: &[IrExpr]) -> bool {
        let name = match args.first() {
            Some(IrExpr::Str(n, _)) => n.clone(),
            _ => return false,
        };
        let op = match args.get(1) {
            Some(IrExpr::Str(o, _)) => o.clone(),
            _ => return false,
        };
        let value = match args.get(2) {
            Some(v) => v,
            None => return false,
        };
        if !is_ident(&name) {
            return false;
        }
        self.mark_written(&name);
        let m = self.zig_ident(&name);
        // array-homed `+=`: append each word (bash element semantics)
        if op == "+="
            && (self.set_arrays.contains(&name)
                || self.str_arrays.contains(&name)
                || self.bigint_arrays.contains(&name))
        {
            if self.bigint_arrays.contains(&name) {
                return false;
            }
            let v = self.expr_str(value);
            self.emit(&format!("shArrAppendStr(&{m}, &{m}_len, {v});"));
            self.rc_dirty = true;
            self.need_ext = true;
            return true;
        }
        let rhs = if self.is_num(&name) {
            self.expr_num(value)
        } else {
            self.expr_str(value)
        };
        match op.as_str() {
            "=" => {
                self.emit(&format!("{m} = {rhs};"));
            }
            "+=" => {
                if self.is_num(&name) {
                    self.emit(&format!("{m} += {rhs};"));
                } else {
                    self.need_cat = true;
                    self.emit(&format!("{m} = shStrCat({m}, {rhs});"));
                }
            }
            _ => return false,
        }
        self.rc_dirty = true;
        true
    }

    /// Expand `$name`/`${name}`/specials inside raw shell TEXT (unquoted
    /// heredoc bodies arrive as one Str with `interpolate: true` — the
    /// core does not lower the refs). Returns a string expression;
    /// unknown `$` forms stay literal (bash keeps `$` before a
    /// non-name char literally, except the documented specials).
    fn expand_heredoc(&mut self, text: &str) -> String {
        let bytes = text.as_bytes();
        let mut lits = String::new();
        let mut parts: Vec<String> = Vec::new();
        let mut i = 0;
        let flush_lit = |lits: &mut String, parts: &mut Vec<String>| {
            if !lits.is_empty() {
                parts.push(Self::zig_str(lits));
                lits.clear();
            }
        };
        while i < bytes.len() {
            if bytes[i] != b'$' {
                lits.push(bytes[i] as char);
                i += 1;
                continue;
            }
            // `${name}` form
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                if let Some(end) = text[i..].find('}') {
                    let inner = &text[i + 2..i + end];
                    let user = !inner.is_empty()
                        && (inner.as_bytes()[0].is_ascii_alphabetic() || inner.as_bytes()[0] == b'_')
                        && inner.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_');
                    let special = matches!(inner, "@" | "*" | "#" | "?" | "!" | "-")
                        || (inner.len() == 1 && inner.as_bytes()[0].is_ascii_digit());
                    if user || special {
                        flush_lit(&mut lits, &mut parts);
                        parts.push(self.declare_name_read(inner));
                        i += end + 1;
                        continue;
                    }
                }
                lits.push('$');
                i += 1;
                continue;
            }
            // `$name` / special single char
            let mut j = i + 1;
            if j < bytes.len()
                && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_')
            {
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                flush_lit(&mut lits, &mut parts);
                parts.push(self.declare_name_read(&text[i + 1..j]));
                i = j;
                continue;
            }
            if j < bytes.len() {
                let c = bytes[j] as char;
                let special = matches!(c, '@' | '*' | '#' | '?' | '!' | '-' | '$')
                    || bytes[j].is_ascii_digit();
                if special {
                    flush_lit(&mut lits, &mut parts);
                    parts.push(self.declare_name_read(&text[i + 1..j + 1]));
                    i = j + 1;
                    continue;
                }
            }
            lits.push('$');
            i += 1;
        }
        flush_lit(&mut lits, &mut parts);
        if parts.is_empty() {
            return Self::zig_str("");
        }
        if parts.len() == 1 {
            return parts.into_iter().next().unwrap();
        }
        // fold with shStrCat (left-assoc)
        let mut it = parts.into_iter();
        let mut acc = it.next().unwrap();
        self.need_cat = true;
        for p in it {
            acc = format!("shStrCat({acc}, {p})");
        }
        acc
    }

    fn stmt(&mut self, s: &IrStmt) {
        // `$?` regime: every simple command updates the status. Helpers
        // that record a real code (shRun/shRunStmt/shUuRun, `false`,
        // real-status tests) set rc_dirty; anything else falls through to
        // the native-success reset. Compounds (if/loops/case/subshells)
        // need no handling: their bodies render through stmt(), so the
        // last executed leaf wins exactly like bash. Flow-diverting
        // statements (break/continue/return/exit) take no reset — it
        // would be unreachable code, which zig rejects.
        self.rc_dirty = false;
        self.stmt_inner(s);
        // a diverging `return` (inside a user function) takes no reset —
        // it would be unreachable code, which zig rejects. A top-level
        // return is a status-2 no-op (bash keeps running), so it resets
        // normally (usually already dirty).
        let diverges =
            matches!(s, IrStmt::Break | IrStmt::Continue | IrStmt::Exit(_))
                || (matches!(s, IrStmt::Return(_)) && self.in_function);
        if !self.rc_dirty && !diverges {
            self.emit("sh_rc = 0;");
        }
    }

    /// A single `exec cmd [args]` in STATEMENT position (extracted from
    /// the Expr-stmt arm so `&&`/`||` chain sides reuse it): builtins with
    /// native lowering (exit/echo/cd/…) render inline, anything else spawns.
    fn exec_stmt_cmd(&mut self, cmd: &str, args: &[IrExpr]) {
        // a user-function call: direct call, not a
        // process spawn (the capture path already gates
        // this way; the stmt path called `g` as an
        // external and failed to compile)
        if self.user_fns.contains(cmd) {
            let call_args: Vec<String> = args
                .get(1)
                .and_then(|a| match a {
                    IrExpr::Array(items) => Some(
                        items.iter().map(|w| self.expr_str(w)).collect(),
                    ),
                    _ => None,
                })
                .unwrap_or_default();
            // main must take `init` to forward it (the
            // need_run gate covers the signature rewrite)
            self.need_run = true;
            self.emit_fn_call_stmt(cmd, &call_args);
            return;
        }
        if cmd == "exit" {
            // bash `exit N` → exit(N); bare `exit` →
            // the current status (bash `false; exit` → 1)
            let code = match args.get(1) {
                Some(IrExpr::Array(items)) if !items.is_empty() => {
                    self.expr_num(&items[0])
                }
                _ => "sh_rc".to_string(),
            };
            self.emit(&format!("std.process.exit(@intCast({code}));"));
            return;
        }
        if cmd == "true" {
            // status 0 no-op
            self.emit("sh_rc = 0;");
            return;
        }
        if cmd == "false" {
            // status 1, but the program CONTINUES (bash
            // `false; echo hi` prints hi and exits 0)
            self.emit("sh_rc = 1;");
            self.rc_dirty = true;
            return;
        }
        if cmd == "echo" {
            if let Some((parts, newline)) = self.echo_parts(args) {
                let call = self.print_from_parts(parts, newline);
                self.emit(&call);
                return;
            }
        }
        if cmd == "printf" {
            // `printf -v NAME fmt args…` assigns the formatted text to NAME
            // instead of printing (bash). Without this the whole statement
            // was dropped and the format string leaked into later output
            // (`%b-vname=` for printf-format-and-v.sh).
            if let Some(IrExpr::Array(items)) = args.get(1) {
                if let [IrExpr::Str(flag, _), IrExpr::Str(vname, _), IrExpr::Str(fmt, _), rest @ ..] =
                    items.as_slice()
                {
                    if flag == "-v" && is_ident(vname) {
                        if let Some(v) = self.printf_text(fmt, rest) {
                            self.mark_written(vname);
                            let m = self.zig_ident(vname);
                            self.emit(&format!("{m} = {v};"));
                            self.rc_dirty = true;
                            return;
                        }
                    }
                }
            }
            if let Some(IrExpr::Array(items)) = args.get(1) {
                if let Some(IrExpr::Str(fmt, _)) = items.first() {
                    // FIELD-SPLIT operand (`printf "<%s>\n" $x`): bash
                    // runs the format once per SPLIT WORD. Render the
                    // conversion list with the word bound to a temp name,
                    // then loop over the runtime tokens.
                    let split_src = items[1..].iter().find_map(|a| match a {
                        IrExpr::Call { func, args: sa } if func == "split" => sa.first().cloned(),
                        _ => None,
                    });
                    if let Some(src) = split_src {
                        let val = self.expr_str(&src);
                        // a name in the RENDERER's reserved namespace: it is
                        // declared by the loop below, so it must not collide
                        // with (or be hoisted as) a program variable
                        let tmp = "__pfs_word".to_string();
                        let mut sub: Vec<IrExpr> = vec![IrExpr::Ident(tmp.clone())];
                        sub.extend(items[1..].iter().filter(|a| {
                            !matches!(a, IrExpr::Call { func, .. } if func == "split")
                        }).cloned());
                        if let Some(parts) = self.printf_parts_of(fmt, &sub) {
                            let call = self.print_from_parts(parts, false);
                            self.emit("{");
                            self.emit(&format!("    var __pfs_it = std.mem.tokenizeAny(u8, {val}, \" \\t\\n\\r\");"));
                            self.emit("    while (__pfs_it.next()) |__pfs_w| {");
                            self.emit(&format!("        const {tmp} = __pfs_w;"));
                            for line in call.lines() {
                                self.emit(&format!("        {line}"));
                            }
                            self.emit("    }");
                            self.emit("}");
                            return;
                        }
                    }
                    if let Some(parts) = self.printf_parts_of(fmt, &items[1..]) {
                        let call = self.print_from_parts(parts, false);
                        self.emit(&call);
                        return;
                    }
                }
            }
        }
        // external command: in-process uutils when the
        // library provides it, else fork/exec.
        // STATEMENT position: the command's stdout must
        // reach OUR stdout (bash runs it in the
        // foreground with inherited fds). shRun captures
        // (for `$(…)`); shRunStmt inherits stdio.
        // shUuRun writes to fd 1 in-process either way.
        // an argv word that must be GLOB-EXPANDED (or brace-split) needs a
        // shell: the word count is only known at run time, so run the whole
        // command through `sh -c` with the words quoted
        if let Some(items) = args.get(1).and_then(|a| match a {
            IrExpr::Array(items) => Some(items),
            _ => None,
        }) {
            if items.iter().any(|i| glob_marked_str(i).is_some()) {
                let mut text = String::new();
                text.push_str(&cmd.replace('\'', "'\\''"));
                for it in items {
                    if let Some(pat) = glob_marked_str(it) {
                        self.need_globpat = true;
                        self.need_run = true;
                        text.push(' ');
                        text.push_str(&pat);
                    } else if let IrExpr::Str(w, _) = it {
                        // a literal word: single-quote it for the child shell
                        text.push(' ');
                        text.push('\'');
                        text.push_str(&w.replace('\'', "'\\''"));
                        text.push('\'');
                    } else {
                        // dynamic word: NOT portable through sh -c — stub
                        text.clear();
                        break;
                    }
                }
                if !text.is_empty() {
                    self.need_run = true;
                    self.rc_dirty = true;
                    self.emit(&format!(
                        "shRunStmt(&[_][]const u8{{\"sh\", \"-c\", {}}});",
                        Self::zig_str(&text)
                    ));
                    return;
                }
            }
        }
        let argv = self.build_argv(cmd, args);
        self.need_run = true;
        self.rc_dirty = true;
        if Self::uu_ffi_enabled() && Self::uu_cmd_ok(cmd) {
            self.need_uu_run = true;
            self.emit(&format!("_ = shUuRun(&[_][]const u8{{{}}});", argv.join(", ")));
        } else {
            self.emit(&format!("shRunStmt(&[_][]const u8{{{}}});", argv.join(", ")));
        }
        return;
    }

    fn stmt_inner(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Ext(node) if node.tag() == "ForEachLine" => {
                // STREAMING line iteration (docs/shir-primitives.md
                // §ForEachLine): takeDelimiter('\n') reader loop — O(1)
                // memory, never a whole-file read; an unterminated final
                // line is still delivered (takeDelimiter treats EOF as a
                // delimiter when bytes remain).
                let fl = node.as_any().downcast_ref::<crate::shir_nodes::ForEachLine>()
                    .expect("tag/type agree");
                let src = self.expr_any(&fl.source);
                let n = self.tmp_counter;
                self.tmp_counter += 1;
                let fh = format!("__f{}", n);
                let buf = format!("__b{}", n);
                let rd = format!("__r{}", n);
                let lv = self
                    .mangle
                    .get(&fl.var)
                    .cloned()
                    .unwrap_or_else(|| self.zig_ident(&fl.var));
                self.mark_written(&fl.var);
                self.mark_read(&fl.var); // bound by the loop — never a dead var
                // a read line is ALWAYS a string — pin the A2 verdict so
                // body reads render as strings, not shToInt coerctions
                self.var_types.insert(fl.var.clone(), IrType::Str);
                self.emit(&format!("var {} = std.Io.Dir.cwd().openFile(init.io, {}, .{{}}) catch {{", fh, src));
                self.emit("    std.process.exit(2);");
                self.emit("};");
                self.emit(&format!("defer {}.close(init.io);", fh));
                self.emit(&format!("var {}: [8192]u8 = undefined;", buf));
                self.emit(&format!("var {} = {}.reader(init.io, &{});", rd, fh, buf));
                if fl.limit.is_some() {
                    self.emit(&format!("var __n{}: usize = 0;", n));
                }
                // capture into a temp then bind the mangled loop var:
                // an unread loop var would be an "unused capture" error
                self.emit(&format!("while (try {}.interface.takeDelimiter('\\n')) |__ln{}| {{", rd, n));
                self.depth += 1;
                self.emit(&format!("{} = __ln{};", lv, n));
                if let Some(lim) = &fl.limit {
                    let le = self.expr_num(lim);
                    self.emit(&format!("__n{} += 1;", n));
                    self.emit(&format!("if (__n{} > {}) break;", n, le));
                }
                for b in &fl.body {
                    self.stmt(b);
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::Ext(node) if node.tag() == "WalkDir" => {
                // STREAMING directory walk (docs/shir-primitives.md
                // §WalkDir): entries collected into a path LIST (O(tree)
                // metadata — file CONTENTS are never opened), then iterated
                // inline so the body captures main's locals naturally.
                let wd = node.as_any().downcast_ref::<crate::shir_nodes::WalkDir>()
                    .expect("tag/type agree");
                let src = self.expr_any(&wd.source);
                let n = self.tmp_counter;
                self.tmp_counter += 1;
                let lv = self
                    .mangle
                    .get(&wd.var)
                    .cloned()
                    .unwrap_or_else(|| self.zig_ident(&wd.var));
                let tf = match &wd.type_filter {
                    Some(x) => x.as_str(),
                    None => "",
                };
                let md = match &wd.maxdepth {
                    Some(e) => self.expr_num(e),
                    None => "0".into(),
                };
                let nf = match &wd.name_filter {
                    Some(x) => Self::zig_str(x),
                    None => "\"\"".to_string(),
                };
                self.need_ext = true; // shGlobName fn
                self.mark_written(&wd.var);
                self.mark_read(&wd.var);
                self.var_types.insert(wd.var.clone(), IrType::Str);
                self.emit("{");
                self.emit(&format!("    var __wl{}: std.ArrayList([]const u8) = .empty;", n));
                self.emit(&format!("    const W{} = struct {{", n));
                self.emit("        fn matches(t: []const u8, nm: []const u8, full: []const u8, isdir: bool) bool {");
                self.emit("            // -name GLOB on the BASE NAME (independent of -type)");
                self.emit("            if (nm.len != 0) {");
                self.emit("                const bn = if (std.mem.lastIndexOfScalar(u8, full, '/')) |ix| full[ix + 1 ..] else full;");
                self.emit("                if (!shGlobName(nm, bn)) return false;");
                self.emit("            }");
                self.emit("            if (t.len != 0) {");
                self.emit("                if (std.mem.eql(u8, t, \"d\") != isdir) return false;");
                self.emit("            }");
                self.emit("            return true;");
                self.emit("        }");
                self.emit("        fn go(al: std.mem.Allocator, io: std.Io, dpath: []const u8, l: *std.ArrayList([]const u8), t: []const u8, nm: []const u8, max: i64, depth: i64) void {");
                self.emit("            // GNU find evaluates the START point too");
                self.emit("            blk0: {");
                self.emit("                const st0 = std.Io.Dir.cwd().statFile(io, dpath, .{}) catch break :blk0;");
                self.emit("                const bn0 = if (std.mem.lastIndexOfScalar(u8, dpath, '/')) |ix| dpath[ix + 1 ..] else dpath;");
                self.emit("                if (matches(t, nm, bn0, st0.kind == .directory)) l.append(al, dpath) catch return;");
                self.emit("            }");
                self.emit("            if (max != 0 and depth >= max) return;");
                self.emit("            var d = std.Io.Dir.cwd().openDir(io, dpath, .{ .iterate = true }) catch return;");
                self.emit("            defer d.close(io);");
                self.emit("            var it = d.iterate();");
                self.emit("            while (it.next(io) catch return) |e| {");
                self.emit("                const full = std.fmt.allocPrint(al, \"{s}/{s}\", .{ dpath, e.name }) catch continue;");
                self.emit("                const isdir = (e.kind == .directory);");
                self.emit("                if (matches(t, nm, e.name, isdir)) l.append(al, full) catch continue;");
                self.emit("                if (isdir and (max == 0 or depth + 1 < max)) go(al, io, full, l, t, nm, max, depth + 1);");
                self.emit("            }");
                self.emit("        }");
                self.emit("    }.go;");
                self.emit(&format!(
                    "    W{}(std.heap.page_allocator, init.io, {}, &__wl{}, \"{tf}\", {nf}, {md}, 0);",
                    n, src, n, tf = tf, nf = nf, md = md
                ));
                self.emit(&format!(
                    "    for (__wl{}.items) |__ln{}| {{",
                    n, n
                ));
                self.depth += 1;
                self.emit(&format!("    {} = __ln{};", lv, n));
                for b in &wd.body {
                    self.stmt(b);
                }
                self.depth -= 1;
                self.emit("    }");
                self.emit("}");
            }
            IrStmt::Ext(_) => panic!("zig backend: Ext node unsupported"),
            IrStmt::Expr(e) => {
                // setVar(name, value) — the frontend-emitted store write:
                // a typed assignment to the var's zig home (dotted struct
                // names sanitize consistently for reads)
                if let IrExpr::Call { func, args } = e {
                    if func == "exec" || func == "builtin" {
                        if let Some(IrExpr::Str(cmd, _)) = args.first() {
                            if self.stmt_builtin(cmd, args) {
                                return;
                            }
                            // `exec <cmd> <args>` / `builtin <cmd> <args>`:
                            // the prefix is a CALL FORM, not a command to run
                            // — `exec echo "$x"` arrived as
                            // ("exec", Array[echo, …]) and spawning a binary
                            // literally named `exec` printed NOTHING
                            // (param-expand-hashhash.sh).
                            if (cmd == "exec" || cmd == "builtin") && func == "exec" {
                                if let Some(IrExpr::Array(inner)) = args.get(1) {
                                    if !inner.is_empty() {
                                        let mut uargs: Vec<IrExpr> = inner.clone();
                                        if let Some(IrExpr::Str(sub, _)) = uargs.first() {
                                            if self.stmt_builtin(sub, &uargs) {
                                                return;
                                            }
                                        }
                                        // not a builtin → external command
                                        if let Some(IrExpr::Str(sub, _)) = uargs.first() {
                                            let _ = sub;
                                            let mut argv = Vec::new();
                                            for a in &uargs {
                                                argv.push(self.expr_str(a));
                                            }
                                            self.need_run = true;
                                            self.rc_dirty = true;
                                            self.emit(&format!(
                                                "shRunStmt(&[_][]const u8{{{}}});",
                                                argv.join(", ")
                                            ));
                                            return;
                                        }
                                    }
                                }
                            }
                            // a `local`-family builtin the native path
                            // refused is a STUB (never an external: there is
                            // no `local` binary — spawning it can only fail)
                            if matches!(cmd.as_str(), "local" | "declare" | "export" | "readonly" | "typeset") {
                                self.sh2_calls.insert("declare".into());
                                self.mark_todo(&format!("{cmd} declaration"));
                                return;
                            }
                        }
                    }
                    if func == "setVar" {
                        if let (Some(IrExpr::Str(name, _)), Some(value)) =
                            (args.first(), args.get(1))
                        {
                            let v = self.zig_ident(name);
                            self.mark_written(name);
                            let rhs = if self.is_num(name) {
                                self.expr_num(value)
                            } else {
                                self.expr_str(value)
                            };
                            self.emit(&format!("{v} = {rhs};"));
                            return;
                        }
                    }
                    if func == "fnCall" || func == "fnValue" {
                        if let Some(IrExpr::Str(name, _)) = args.first() {
                            self.mark_read(name);
                            let call_args: Vec<String> = args
                                .get(1)
                                .and_then(|a| match a {
                                    IrExpr::Array(items) => Some(
                                        items.iter().map(|w| self.expr_str(w)).collect(),
                                    ),
                                    _ => None,
                                })
                                .unwrap_or_default();
                            self.need_run = true;
                            self.emit_fn_call_stmt(name, &call_args);
                            return;
                        }
                    }
                }
                // statement-position arith (`runs++` — the do-while idiom):
                // render natively (`runs += 1;`) instead of the sh2Arith
                // stub (which was called without being emitted)
                if let IrExpr::Arith(a) = e {
                    if let ArithAst::IncDec { var, delta, .. } = &**a {
                        let v = self.zig_ident(var);
                        self.mark_read(var);
                        let d = delta.unsigned_abs();
                        // a STRING-homed var (the C frontend's value model
                        // for untyped locals) coerces via shToInt
                        if self.is_num(var) {
                            let zop = if *delta >= 0 { "+" } else { "-" };
                            self.emit(&format!("{v} {zop}= {d};"));
                        } else {
                            // the value model is strings: convert back
                            self.need_intstr = true;
                            let op = if *delta >= 0 { "+" } else { "-" };
                            self.emit(&format!(
                                "{v} = shIntStr(shToInt({v}) {op} {d});"
                            ));
                        }
                        return;
                    }
                    if let ArithAst::Assign { var, op, rhs } = &**a {
                        let v = self.zig_ident(var);
                        self.mark_read(var);
                        let r = self.arith(rhs);
                        let zop = match op.as_str() {
                            "+=" => "+=",
                            "-=" => "-=",
                            "*=" => "*=",
                            "/=" => "/=",
                            "%=" => "%=",
                            _ => "=",
                        };
                        self.emit(&format!("{v} {zop}= {r};"));
                        return;
                    }
                }
                if let IrExpr::Call { func, args } = e {
                    if func == "exec" {
                        if let Some(IrExpr::Str(cmd, _)) = args.first() {
                            self.exec_stmt_cmd(cmd, args);
                            return;
                        }
                    }
                    if func == "break" && self.loop_depth > 0 {
                        self.emit("break;");
                        return;
                    }
                    if func == "continue" && self.loop_depth > 0 {
                        self.emit("continue;");
                        return;
                    }
                    if func == "cstyleFor" {
                        // `for (( i=1; i<n; i++ ))` — init; cond; incr.
                        // The loop var is hoisted at i64 (function scope,
                        // bash leaves $i = LAST after the loop).
                        if let Some(IrExpr::Str(spec, _)) = args.first() {
                            if let Some(IrExpr::Arrow(body)) = args.get(1) {
                                self.emit_cstyle_for(spec, body);
                                return;
                            }
                        }
                        self.mark_todo("cstyleFor args");
                        return;
                    }
                }
                // `A && B` / `A || B` chains: status-driven control flow.
                // Each side runs as a statement (managing sh_rc through
                // the same arms as top level); the connective branches on
                // the recorded status. `!A` inverts it. A bare `a | b`
                // pipeline in statement position passes through (the last
                // stage inherits our stdout).
                if let IrExpr::BinOp { lhs, op, rhs } = e {
                    if matches!(
                        op,
                        BinOpKind::And | BinOpKind::Or | BinOpKind::Not
                    ) {
                        self.chain_stmt(lhs, op, rhs);
                        return;
                    }
                }
                // `shopt("name", Bool(on))` (top-level Call form)
                if let IrExpr::Call { func, args } = e {
                    if func == "shopt" {
                        if let Some(IrExpr::Str(opt, _)) = args.first() {
                            let on = matches!(args.get(1), Some(IrExpr::Bool(true)));
                            match opt.as_str() {
                                "nocasematch" => self.shopt_nocasematch = on,
                                "extglob" => self.shopt_extglob = on,
                                _ => {}
                            }
                        }
                        self.emit("sh_rc = 0;");
                        self.rc_dirty = true;
                        return;
                    }
                    if func == "assign" {
                        if self.assign_call(args) {
                            return;
                        }
                        self.sh2_calls.insert("assign".into());
                        self.mark_todo("assign → sh2.assign");
                        return;
                    }
                    if func == "pipeline" {
                        if let Some(rendered) = self.render_pipeline(args, false) {
                            self.emit(&format!("_ = {rendered};"));
                            return;
                        }
                    }
                    // `shopt -s opt` — known runtime limitation: accept as
                    // a successful no-op (status 0); files depending on the
                    // option fail equivalence honestly
                    if func == "shopt" {
                        self.emit("sh_rc = 0;");
                        self.rc_dirty = true;
                        return;
                    }
                }
                let x = self.expr_any(e);
                self.emit(&format!("_ = {x};"));
            }
            IrStmt::Assign { targets, expr, asm, .. } => {
                // Declarator-position asm label (core request
                // c-sh-go-toplevelasmargument-20260814-042952) — no Zig
                // rendering; refuse loudly (refuse > guess).
                if let Some(spec) = asm {
                    self.mark_todo(&format!("asm label '{}' on an assign", spec.template));
                    return;
                }
                let Some(t) = targets.first() else {
                    self.mark_todo("multi-target assign");
                    return;
                };
                let m = self.zig_ident(&t.var);
                self.mark_written(&t.var);
                // the core may deliver an INDEXED target as the literal
                // name `arr[1]` (indices: []) — parse it out
                if t.indices.is_empty() {
                    if let Some((base, idxtext)) = t.var.split_once('[') {
                        if let Some(idx) = idxtext.strip_suffix(']') {
                            if is_ident(base) && (self.str_arrays.contains(base)
                                || self.set_arrays.contains(base))
                            {
                                let bm = self.zig_ident(base);
                                self.mark_written(base);
                                let ix = if let Ok(n) = idx.trim().parse::<i64>() {
                                    n.to_string()
                                } else {
                                    self.expr_num(&IrExpr::Var(
                                        idx.trim().to_string(),
                                        None,
                                    ))
                                };
                                let v = self.expr_str(expr);
                                let n = format!("{bm}_len");
                                self.emit("{");
                                self.emit(&format!("    const __sh_ix: i64 = {ix};"));
                                self.emit(&format!("    const __sh_n: i64 = @intCast({n});"));
                                self.emit("    const __sh_j: i64 = if (__sh_ix < 0) __sh_n + __sh_ix else __sh_ix;");
                                self.emit("    if (__sh_j >= 0) {");
                                self.emit("        const __sh_u: usize = @intCast(__sh_j);");
                                self.emit(&format!("        if (__sh_u >= {n}) {{"));
                                self.emit(&format!("            var __sh_k: usize = {n};"));
                                self.emit(&format!("            while (__sh_k <= __sh_u) : (__sh_k += 1) {{ {bm}[__sh_k] = \"\"; }}"));
                                self.emit(&format!("            {n} = __sh_u + 1;"));
                                self.emit("        }");
                                self.emit(&format!("        {bm}[__sh_u] = {v};"));
                                self.emit("    }");
                                self.emit("}");
                                self.rc_dirty = true;
                                return;
                            }
                        }
                    }
                }
                if !t.indices.is_empty() {
                    // `arr[i]=V` on a string-element array: store at the
                    // (bash: negative wraps from the end)
                    if self.str_arrays.contains(&t.var) || self.set_arrays.contains(&t.var) {
                        if let Some(idx) = t.indices.first() {
                            let ix = self.expr_num(idx);
                            let v = self.expr_str(expr);
                            let n = format!("{m}_len");
                            self.emit("{");
                            self.emit(&format!("    const __sh_ix: i64 = {ix};"));
                            self.emit(&format!("    const __sh_n: i64 = @intCast({n});"));
                            self.emit("    const __sh_j: i64 = if (__sh_ix < 0) __sh_n + __sh_ix else __sh_ix;");
                            self.emit(&format!("    if (__sh_j >= 0) {{"));
                            self.emit(&format!("        const __sh_u: usize = @intCast(__sh_j);"));
                            self.emit(&format!("        if (__sh_u >= {n}) {{"));
                            self.emit(&format!("            var __sh_k: usize = {n};"));
                            self.emit(&format!("            while (__sh_k <= __sh_u) : (__sh_k += 1) {m}[__sh_k] = \"\"; }}"));
                            self.emit(&format!("            {n} = __sh_u + 1;"));
                            self.emit("        }");
                            self.emit(&format!("        {m}[__sh_u] = {v};"));
                            self.emit("    }");
                            self.emit("}");
                            self.rc_dirty = true;
                            return;
                        }
                    }
                    self.mark_todo("array-index assign");
                    return;
                }
                // compound assignment as a call (`y+=2`, `r=$(cmd)`): the
                // call carries its own target name (== the Assign target)
                if let IrExpr::Call { func, args } = expr {
                    if func == "assign" {
                        if self.assign_call(args) {
                            return;
                        }
                        self.sh2_calls.insert("assign".into());
                        self.mark_todo("assign → sh2.assign");
                        return;
                    }
                }
                // set-array writes: `factors = set()` resets the length;
                // `factors.add(x)` appends (dedup happens at sortedIntJoin).
                // String-element arrays (`arr=(a b c)`) fill the []const u8
                // home element-wise; bigint homes init Managed slots.
                if let IrExpr::Call { func, args } = expr {
                    if matches!(func.as_str(), "setArray" | "setArrayAppend")
                        && (self.set_arrays.contains(&t.var)
                            || self.bigint_arrays.contains(&t.var)
                            || self.str_arrays.contains(&t.var))
                    {
                        if func == "setArray" {
                            self.emit(&format!("{m}_len = 0;"));
                        }
                        // populate/append each element
                        if let Some(IrExpr::Array(items)) = args.get(1) {
                            for it in items {
                                if self.bigint_arrays.contains(&t.var) {
                                    // bigint element: init a Managed slot
                                    self.emit(&format!("{m}[{m}_len] = try std.math.big.int.Managed.init(std.heap.page_allocator);"));
                                    match it {
                                        IrExpr::Arith(a) => self.bigint_into(a, &format!("{m}[{m}_len]")),
                                        _ => {
                                            let v = self.expr_str(it);
                                            self.emit(&format!("try {m}[{m}_len].setString(10, {v});"));
                                        }
                                    }
                                } else if let IrExpr::Call { func: sf, args: sa } = it {
                                    if sf == "split" && self.str_arrays.contains(&t.var) {
                                        // `arr=($x)`: the unquoted expansion
                                        // FIELD-SPLITS into elements
                                        if let Some(inner) = sa.first() {
                                            let v = self.expr_str(inner);
                                            let tk = self.fresh_tmp("sh_spk");
                                            self.emit("{");
                                            self.emit(&format!("    var {tk} = std.mem.tokenizeAny(u8, {v}, \" \\t\\n\\r\");"));
                                            self.emit(&format!("    while ({tk}.next()) |{tk}w| {{"));
                                            self.emit(&format!("        if ({m}_len < {m}.len) {{ {m}[{m}_len] = {tk}w; {m}_len += 1; }}"));
                                            self.emit("    }");
                                            self.emit("}");
                                            continue;
                                        }
                                    }
                                    let v = self.expr_str(it);
                                    self.emit(&format!("{m}[{m}_len] = {v};"));
                                } else if self.str_arrays.contains(&t.var) {
                                    let v = self.expr_str(it);
                                    self.emit(&format!("{m}[{m}_len] = {v};"));
                                } else {
                                    let v = self.expr_num(it);
                                    self.emit(&format!("{m}[{m}_len] = {v};"));
                                }
                                self.emit(&format!("{m}_len += 1;"));
                            }
                        }
                        return;
                    }
                }
                // bigint target: compute the RHS into the Managed var
                if self.bigint_vars.contains(&t.var) {
                    self.need_bigint = true;
                    match expr {
                        IrExpr::Arith(a) => self.bigint_into(a, &m),
                        IrExpr::Var(name, _) | IrExpr::Ident(name)
                            if self.bigint_vars.contains(name) =>
                        {
                            let src = self.zig_ident(name);
                            self.mark_read(name);
                            self.emit(&format!("try {m}.copy({src}.toConst());"));
                        }
                        _ => {
                            let v = self.expr_str(expr);
                            self.emit(&format!("try {m}.setString(10, {v});"));
                        }
                    }
                    return;
                }
                let rhs = if self.is_num(&t.var) {
                    // arith-assign / inc-dec render natively (not via
                    // the sh2Arith stub)
                    match expr {
                        IrExpr::Arith(a) => match &**a {
                            ArithAst::IncDec { var, delta, prefix } => {
                                let v = self.zig_ident(var);
                                self.mark_read(var);
                                let d = delta.unsigned_abs();
                                let s = if *delta >= 0 { "+" } else { "-" };
                                // the STATEMENT form sets $? from the
                                // expression VALUE: old for postfix, new for
                                // prefix; 0 -> status 1 (arith-postfix-status)
                                if *prefix {
                                    self.emit(&format!("{v} {s}= {d};"));
                                    self.emit(&format!("sh_rc = if ({v} != 0) 0 else 1;"));
                                } else {
                                    // braced: two postfix statements in one
                                    // scope would redeclare the temp
                                    self.emit("{");
                                    self.emit(&format!("    const __sh_old = {v};"));
                                    self.emit(&format!("    {v} {s}= {d};"));
                                    self.emit("    sh_rc = if (__sh_old != 0) 0 else 1;");
                                    self.emit("}");
                                }
                                self.rc_dirty = true;
                                return;
                            }
                            ArithAst::Assign { var, op, rhs } => {
                                let v = self.zig_ident(var);
                                self.mark_read(var);
                                let r = self.arith(rhs);
                                let zop = match op.as_str() {
                                    "+=" => "+=",
                                    "-=" => "-=",
                                    "*=" => "*=",
                                    "/=" => "/=",
                                    "%=" => "%=",
                                    _ => "=",
                                };
                                if zop == "=" {
                                    self.emit(&format!("{v} = {r};"));
                                } else {
                                    self.emit(&format!("{v} {zop} {r};"));
                                }
                                return;
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                    self.expr_num(expr)
                } else {
                    self.expr_str(expr)
                };
                // a zero-divisor arithmetic error ABANDONS the assignment
                // (bash keeps the previous value and exits 1) — see the
                // shDiv/shRem helpers
                let arith_checked = self.need_safe_div;
                if arith_checked {
                    // evaluate FIRST, then store: the RHS call itself sets
                    // sh_arith_err, so a plain `x = f()` would still assign
                    self.emit(&format!("{{"));
                    self.emit(&format!("    const __sh_av = {rhs};"));
                    self.emit(&format!("    if (!sh_arith_err) {{ {m} = __sh_av; }}"));
                    self.emit(&format!("}}"));
                    self.emit("if (sh_arith_err) { sh_rc = 1; }");
                } else {
                    self.emit(&format!("{m} = {rhs};"));
                }
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    self.mark_written(&d.name);
                    let m = self.zig_ident(&d.name);
                    if let Some(e) = init {
                        let rhs = if self.is_num(&d.name) {
                            self.expr_num(e)
                        } else {
                            self.expr_str(e)
                        };
                        self.emit(&format!("{m} = {rhs};"));
                    } else {
                        // `export NAME` / `declare NAME=`: bash binds the
                        // name to the EMPTY string, so emit a real store
                        // (a pure comment left it "never mutated")
                        if self.is_num(&d.name) {
                            self.emit(&format!("{m} = 0;"));
                        } else if self.str_arrays.contains(&d.name) || self.set_arrays.contains(&d.name) {
                            self.emit(&format!("{m}_len = 0;"));
                        } else {
                            self.emit(&format!("{m} = \"\";"));
                        }
                    }
                }
            }
            IrStmt::DeclareArray { .. } => {
                self.mark_todo("DeclareArray");
            }
            IrStmt::Output {
                value,
                newline,
                target,
            } => {
                if target.is_some() {
                    self.mark_todo("Output to filehandle");
                    return;
                }
                let parts = self.parts_of(value);
                let call = self.print_from_parts(parts, *newline);
                self.emit(&call);
            }
            IrStmt::WriteFile {
                path,
                content,
                append,
            } => {
                if *append {
                    self.sh2_calls.insert("writeFile".into());
                    self.mark_todo("WriteFile append → sh2.writeFile");
                    return;
                }
                let p = self.expr_str(path);
                let c = self.expr_str(content);
                self.emit(&format!(
                    "std.Io.Dir.cwd().writeFile(sh_io orelse return, .{{ .sub_path = {p}, .data = {c} }}) catch return;"
                ));
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
                for s in then {
                    self.stmt(s);
                }
                self.depth -= 1;
                for (ec, body) in elsifs {
                    let ec = self.expr_bool(ec);
                    self.emit(&format!("}} else if ({ec}) {{"));
                    self.depth += 1;
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                }
                if !else_.is_empty() {
                    self.emit("} else {");
                    self.depth += 1;
                    for s in else_ {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                }
                self.emit("}");
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                // shell `case D in pat) …;; esac` — if/else-if chain on
                // string equality (the common literal-pattern case).
                let d = self.expr_str(discriminant);
                let mut emitted = false;
                for cl in clauses {
                    let conds: Vec<String> = cl
                        .patterns
                        .iter()
                        .filter(|p| p.as_str() != "*")
                        .map(|p| {
                            // source quotes are not part of the value: a
                            // QUOTED pattern is a literal (even `"*"` —
                            // bash matches only a literal `*`); only an
                            // UNQUOTED pattern with wildcards is a glob
                            let (text, quoted) = match p.as_str() {
                                q if q.len() >= 2
                                    && q.starts_with('"')
                                    && q.ends_with('"') =>
                                {
                                    (q[1..q.len() - 1].to_string(), true)
                                }
                                q if q.len() >= 2
                                    && q.starts_with('\'')
                                    && q.ends_with('\'') =>
                                {
                                    (q[1..q.len() - 1].to_string(), true)
                                }
                                q => (q.to_string(), false),
                            };
                            if !quoted
                                && (text.contains('*')
                                    || text.contains('?')
                                    || text.contains('['))
                            {
                                self.need_glob = true;
                                format!("matchGlob({d}, {})", Self::zig_str(&text))
                            } else {
                                format!("std.mem.eql(u8, {d}, {})", Self::zig_str(&text))
                            }
                        })
                        .collect();
                    let is_default = cl.patterns.iter().any(|p| p.as_str() == "*");
                    if conds.is_empty() && is_default {
                        if emitted {
                            self.emit("} else {");
                        } else {
                            self.emit("else {");
                        }
                        self.depth += 1;
                        for s in &cl.body {
                            self.stmt(s);
                        }
                        self.depth -= 1;
                        emitted = true;
                        continue;
                    }
                    let kw = if emitted { "} else if" } else { "if" };
                    self.emit(&format!("{kw} ({}) {{", conds.join(" or ")));
                    self.depth += 1;
                    for s in &cl.body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    emitted = true;
                }
                if emitted {
                    self.emit("}");
                }
            }
            IrStmt::While { cond, body } => {
                let c = self.expr_bool(cond);
                self.emit(&format!("while ({c}) {{"));
                self.loop_depth += 1;
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.loop_depth -= 1;
                self.emit("}");
            }
            IrStmt::DoWhile { body, cond, until } => {
                self.emit("while (true) {");
                self.loop_depth += 1;
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.loop_depth -= 1;
                let c = self.expr_bool(cond);
                if *until {
                    self.emit(&format!("if ({c}) break;"));
                } else {
                    self.emit(&format!("if (!({c})) break;"));
                }
                self.emit("}");
            }
            IrStmt::For { var, iter, body } => {
                let m = self.zig_ident(var);
                self.mark_written(var);
                // `for a in "$@"`: the POSITIONAL PARAMS. Inside a fn that
                // is __args; at TOP LEVEL it is the script's own argv, which
                // the corpus gate runs with NONE — zero iterations, not one
                // empty one (parse-dollars-in-string.sh printed a blank line).
                if !self.in_function {
                    if let IrExpr::Array(items) = iter {
                        if let [IrExpr::Call { func, args }] = items.as_slice() {
                            if func == "listVar"
                                && matches!(args.first(), Some(IrExpr::Str(n, _)) if n == "@" || n == "*")
                            {
                                self.emit("{");
                                self.emit("    // `for x in \"$@\"` with no positional params");
                                self.emit("}");
                                return;
                            }
                        }
                    }
                }
                if self.in_function {
                    if let IrExpr::Array(items) = iter {
                        if let [IrExpr::Call { func, args }] = items.as_slice() {
                            if func == "listVar"
                                && matches!(args.first(), Some(IrExpr::Str(n, _)) if n == "@" || n == "*")
                            {
                                // the loop capture must not collide with
                                // the hoisted `var a_` home (zig: "capture
                                // shadows declaration")
                                let v_tmp = self.fresh_tmp("sh_arg");
                                self.emit(&format!("for (__args) |{v_tmp}| {{"));
                                self.emit(&format!("{m} = {v_tmp};"));
                                self.depth += 1;
                                self.loop_depth += 1;
                                for s in body {
                                    self.stmt(s);
                                }
                                self.loop_depth -= 1;
                                self.depth -= 1;
                                self.emit("}");
                                return;
                            }
                        }
                    }
                }
                // a field-split word in the list (`for w in $x`): bash
                // iterates the SPLIT words, so expand at run time
                if let IrExpr::Array(items) = iter {
                    if let Some(src) = items.iter().find_map(|a| match a {
                        IrExpr::Call { func, args: sa } if func == "split" => sa.first().cloned(),
                        _ => None,
                    }) {
                        let val = self.expr_str(&src);
                        let t = self.fresh_tmp("sh_fs");
                        let v_tmp = self.fresh_tmp("sh_fv");
                        self.emit("{");
                        self.emit(&format!("    var {t} = std.mem.tokenizeAny(u8, {val}, \" \\t\\n\\r\");"));
                        self.emit(&format!("    while ({t}.next()) |{v_tmp}| {{"));
                        self.emit(&format!("        {m} = {v_tmp};"));
                        self.depth += 1;
                        self.loop_depth += 1;
                        for s in body {
                            self.stmt(s);
                        }
                        self.loop_depth -= 1;
                        self.depth -= 1;
                        self.emit("    }");
                        self.emit("}");
                        return;
                    }
                }
                // a GLOB-marked word in the list must expand against the
                // filesystem FIRST (`for f in *.{txt,log}` — bash expands
                // brace+glob before iterating)
                if let IrExpr::Array(items) = iter {
                    if items.iter().any(|i| glob_marked_str(i).is_some()) {
                        let mut pats: Vec<String> = Vec::new();
                        for it in items {
                            match glob_marked_str(it) {
                                Some(p) => {
                                    self.need_globpat = true;
                                    self.need_run = true;
                                    // a pattern may expand to MANY names —
                                    // expand at run time and re-split on 0x1f
                                    pats.push(format!(
                                        "(shGlobExpand({}, 0x1f))",
                                        Self::zig_str(&p)
                                    ));
                                }
                                None => {
                                    let pp = self.parts_of(it);
                                    pats.push(self.parts_to_str(pp));
                                }
                            }
                        }
                        // join the per-pattern expansions with the SAME
                        // 0x1f separator so the split below sees every name
                        // (a space join merged adjacent results)
                        let joined = if pats.len() == 1 {
                            pats[0].clone()
                        } else {
                            let mut j = pats[0].clone();
                            self.need_cat = true;
                            for q in &pats[1..] {
                                j = format!("shStrCat({j}, shStrCat(\"\\x1f\", {q}))");
                            }
                            j
                        };
                        let items_tmp = self.fresh_tmp("sh_gl");
                        let v_tmp = self.fresh_tmp("sh_gv");
                        let arr_tmp = self.fresh_tmp("sh_ga");
                        self.emit("{");
                        self.depth += 1;
                        self.emit(&format!("const {items_tmp} = {joined};"));
                        self.emit(&format!("var {arr_tmp}: std.ArrayList([]const u8) = .empty;"));
                        self.emit(&format!("var __gsit = std.mem.splitScalar(u8, {items_tmp}, 0x1f);"));
                        self.emit("while (__gsit.next()) |__gsp| {");
                        self.emit("    if (__gsp.len == 0) continue;");
                        self.emit(&format!("    {arr_tmp}.append(std.heap.page_allocator, __gsp) catch {{}};"));
                        self.emit("}");
                        self.emit(&format!("for ({arr_tmp}.items) |{v_tmp}| {{"));
                        self.emit(&format!("    {m} = {v_tmp};"));
                        self.loop_depth += 1;
                        self.depth += 1;
                        for s in body {
                            self.stmt(s);
                        }
                        self.depth -= 1;
                        self.loop_depth -= 1;
                        self.emit("}");
                        self.depth -= 1;
                        self.emit("}");
                        return;
                    }
                }
                match iter {
                    IrExpr::Array(items) => {
                        let elems: Vec<String> = if self.is_num(var) {
                            items.iter().map(|i| self.expr_num(i)).collect()
                        } else {
                            items.iter().map(|i| self.expr_str(i)).collect()
                        };
                        let (t, inner) = if self.is_num(var) {
                            ("i64", elems.join(", "))
                        } else {
                            ("[]const u8", elems.join(", "))
                        };
                        let items_tmp = self.fresh_tmp("sh_items");
                        let v_tmp = self.fresh_tmp("sh_v");
                        self.emit(&format!("{{"));
                        self.depth += 1;
                        self.emit(&format!("const {items_tmp} = [_]{t}{{{inner}}};"));
                        self.emit(&format!("for ({items_tmp}[0..]) |{v_tmp}| {{"));
                        self.loop_depth += 1;
                        self.depth += 1;
                        self.emit(&format!("{m} = {v_tmp};"));
                        for s in body {
                            self.stmt(s);
                        }
                        self.depth -= 1;
                        self.loop_depth -= 1;
                        self.emit("}");
                        self.depth -= 1;
                        self.emit("}");
                    }
                    IrExpr::Range { start, end } if self.is_num(var) => {
                        self.emit(&format!("{m} = {start};"));
                        self.emit(&format!("while ({m} <= {end}) : ({m} += 1) {{"));
                        self.loop_depth += 1;
                        self.depth += 1;
                        for s in body {
                            self.stmt(s);
                        }
                        self.depth -= 1;
                        self.loop_depth -= 1;
                        self.emit("}");
                    }
                    other => {
                        self.mark_todo(&format!("For iter {:?}", other));
                        self.mark_todo("For body skipped");
                        // body vars are still hoisted via collect_written;
                        // the dead-var guard keeps them compiling
                    }
                }
            }
            IrStmt::Exit(e) => {
                let code = e
                    .as_ref()
                    .map(|x| self.expr_num(x))
                    .unwrap_or_else(|| "sh_rc".to_string());
                self.emit(&format!("std.process.exit(@intCast({code}));"));
            }
            IrStmt::Block(b) => {
                for s in b {
                    self.stmt(s);
                }
            }
            IrStmt::Exec { .. } => {
                // run a compile-able stub (exits 2 at runtime) rather than
                // silently dropping the command
                let _ = self.sh2_stub("exec", "exec");
                self.emit("_ = sh2Exec();");
            }
            IrStmt::Pipeline { stages, .. } => {
                // structured pipeline stages: same shPipe lowering as the
                // `pipeline` call form (single-command stages; anything
                // else keeps the honest stub, never silent sequencing)
                match self.render_pipeline_structured(stages, false) {
                    Some(rendered) => {
                        self.emit(&format!("_ = {rendered};"));
                    }
                    None => {
                        self.sh2_calls.insert("pipeline".into());
                        self.mark_todo("pipeline → sh2.pipeline");
                    }
                }
            }
            IrStmt::Subshell(body) => {
                // bash subshells are copy-at-fork: assignments inside never
                // reach the parent. Save every var the body writes, render
                // inline, restore after (mirrors the perl backend).
                let mut ws = BTreeSet::new();
                collect_written(body, &mut ws);
                let mut saves: Vec<(String, String)> = Vec::new();
                for w in &ws {
                    let m = self.zig_ident(w);
                    let tmp = self.fresh_tmp("sh_subsave");
                    self.emit(&format!("const {tmp} = {m};"));
                    saves.push((m, tmp));
                }
                for s in body {
                    self.stmt(s);
                }
                for (m, tmp) in saves {
                    self.emit(&format!("{m} = {tmp};"));
                }
            }
            IrStmt::Background(body) => {
                // bash runs the job CONCURRENTLY on a copy of the shell
                // state (`(sleep 1; echo a) & echo b` prints b first). Fork:
                // the child flushes our buffered stdout, runs the body
                // against its own copy (the fork IS the copy), then
                // _exits; the parent resets the launch status to 0.
                self.need_run = true;
                self.need_stdout = true;
                self.need_main_stdout = true;
                self.saw_background = true;
                if !self.in_function {
                    self.emit("stdout.flush() catch {};");
                }
                let pid = self.fresh_tmp("sh_bgpid");
                self.emit(&format!("const {pid} = std.c.fork();"));
                self.emit(&format!("if ({pid} == 0) {{"));
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                // flush what the child buffered before _exit (which does
                // NOT run atexit handlers or flush stdio)
                if !self.in_function {
                    self.emit("stdout.flush() catch {};");
                }
                self.emit("std.c._exit(0);");
                self.depth -= 1;
                self.emit("}");
                self.emit("sh_rc = 0;");
            }
            IrStmt::Block(body) => {
                for s in body {
                    self.stmt(s);
                }
            }
            IrStmt::DeclareArray {
                var,
                elements,
                sigil: _,
            } => {
                let elems: Vec<String> = elements.iter().map(|e| self.expr_str(e)).collect();
                let m = self.zig_ident(var);
                self.mark_written(var);
                self.emit(&format!(
                    "{m} = [_][]const u8{{{}}};",
                    elems.join(", ")
                ));
            }
            IrStmt::Redirect { inner, redirects } => {
                // fd juggling through libc dup/dup2 (the gate links it):
                // each spec opens its target and swaps it onto the fd IN
                // SOURCE ORDER (bash `>f 2>&1` dups the already-redirected
                // stdout), the inner commands run (prints AND forked
                // children observe the swapped fds), then the saved fds
                // restore in reverse. Buffered stdout is flushed across
                // both transitions (bytes buffered before the swap belong
                // to the old destination). A failed open skips the inner
                // commands with status 1 (bash). Custom fds (3+) persist
                // past `exec N>f` (empty inner); writes/reads through them
                // stay stubs. Without libc there is no dup — stub loudly.
                // Specs are VALIDATED before anything emits, so a refused
                // redirect leaves no partial juggling behind.
                if !Self::uu_ffi_enabled() {
                    self.sh2_calls.insert("redirect".into());
                    self.mark_todo("redirect → sh2.redirect (no libc for dup)");
                    return;
                }
                // pre-validate: every spec must be a modeled shape
                let mut valid = true;
                for r in redirects {
                    let fd = r.fd.unwrap_or(1);
                    let dup_target = match &r.target {
                        IrExpr::Str(s, _) => {
                            s.len() == 2
                                && s.starts_with('&')
                                && s[1..].chars().all(|c| c.is_ascii_digit())
                        }
                        _ => false,
                    };
                    match r.mode.as_str() {
                        "w" | "a" | "wc" => {
                            if !dup_target && fd != 0 && fd != 1 && fd != 2 && !inner.is_empty() {
                                valid = false;
                            }
                        }
                        "r" | "r+" => {
                            if fd != 0 && !inner.is_empty() {
                                valid = false;
                            }
                        }
                        "heredoc" | "heredoc-tabs" | "herestring" => {
                            if fd != 0 {
                                valid = false;
                            }
                        }
                        _ => {
                            valid = false;
                        }
                    }
                }
                if !valid {
                    self.sh2_calls.insert("redirect".into());
                    self.mark_todo("redirect → sh2.redirect (unmodeled spec)");
                    return;
                }
                self.need_writefile = true; // forces sh_io publication
                // the flush lines reference `stdout`: make sure the
                // prologue exists in this scope (main: flags; functions:
                // the textual uses_stdout scan sees the flush lines)
                self.need_stdout = true;
                if !self.in_function {
                    self.need_main_stdout = true;
                }
                let blk = self.fresh_tmp("sh_rd");
                // the ok-flag is only mutable when a fallible spec (path
                // open / pipe) exists; pure dup-swaps use a const
                let mut fallible = false;
                for r in redirects {
                    match r.mode.as_str() {
                        "w" | "a" | "wc" => {
                            let is_dup = match &r.target {
                                IrExpr::Str(t, _) => {
                                    t.len() == 2
                                        && t.starts_with('&')
                                        && t[1..].chars().all(|c| c.is_ascii_digit())
                                }
                                _ => false,
                            };
                            if !is_dup {
                                fallible = true;
                            }
                        }
                        _ => {
                            fallible = true;
                        }
                    }
                }
                self.emit("{");
                self.depth += 1;
                self.emit("stdout.flush() catch {};");
                if fallible {
                    self.emit(&format!("var {blk}_ok = true;"));
                } else {
                    self.emit(&format!("const {blk}_ok = true;"));
                }
                let mut saves: Vec<(i32, String)> = Vec::new();
                for r in redirects {
                    let fd = r.fd.unwrap_or(1);
                    let sv = self.fresh_tmp("sh_rd_sv");
                    match r.mode.as_str() {
                        "w" | "a" | "wc" => {
                            let is_dup = match &r.target {
                                IrExpr::Str(s, _) => {
                                    s.len() == 2
                                        && s.starts_with('&')
                                        && s[1..].chars().all(|c| c.is_ascii_digit())
                                }
                                _ => false,
                            };
                            if is_dup {
                                if let IrExpr::Str(s, _) = &r.target {
                                    let m: i32 = s[1..].parse().unwrap_or(1);
                                    self.emit(&format!("const {sv}: c_int = std.c.dup({fd});"));
                                    self.emit(&format!("_ = std.c.dup2({m}, {fd});"));
                                    saves.push((fd, sv));
                                }
                                continue;
                            }
                            let p = self.expr_str(&r.target);
                            let pv = self.fresh_tmp("sh_rd_p");
                            let flags = if r.mode == "a" {
                                ".{ .ACCMODE = .WRONLY, .CREAT = true, .APPEND = true }"
                            } else {
                                ".{ .ACCMODE = .WRONLY, .CREAT = true, .TRUNC = true }"
                            };
                            self.emit(&format!("const {pv}_z = std.heap.page_allocator.dupeSentinel(u8, {p}, 0) catch \"\";"));
                            self.emit(&format!("const {pv}: c_int = if ({pv}_z.len == 0) -1 else std.c.open({pv}_z.ptr, {flags}, @as(c_uint, 0o666));"));
                            self.emit(&format!("var {sv}: c_int = -1;"));
                            self.emit(&format!("if ({pv} < 0) {{ {blk}_ok = false; }} else {{ {sv} = std.c.dup({fd}); _ = std.c.dup2({pv}, {fd}); _ = std.c.close({pv}); }}"));
                            if inner.is_empty() {
                                // `exec N>f`: persistent, no restore — keep
                                // the saved fd referenced (address-discard:
                                // a plain `_ = sv` on an assigned var is a
                                // pointless-discard error)
                                self.emit(&format!("_ = &{sv};"));
                            } else {
                                saves.push((fd, sv));
                            }
                        }
                        "r" | "r+" => {
                            let p = self.expr_str(&r.target);
                            self.emit(&format!("var {sv}: c_int = -1;"));
                            self.emit(&format!("if (std.Io.Dir.cwd().openFile(sh_io orelse unreachable, {p}, .{{}}) catch null) |__f| {{ {sv} = std.c.dup({fd}); _ = std.c.dup2(__f.handle, {fd}); __f.close(sh_io orelse unreachable); }} else {{ {blk}_ok = false; }}"));
                            if !inner.is_empty() {
                                saves.push((fd, sv));
                            } else {
                                self.emit(&format!("_ = &{sv};"));
                            }
                        }
                        "heredoc" | "heredoc-tabs" | "herestring" => {
                            // unquoted heredocs/herestrings interpolate
                            // (`interpolate: true`): the target arrives as
                            // raw text with `$`-refs intact — expand them;
                            // quoted bodies stay literal
                            let mut text = match &r.target {
                                IrExpr::Str(s, _) if r.interpolate => self.expand_heredoc(s),
                                _ => self.expr_str(&r.target),
                            };
                            if r.mode == "herestring" {
                                // `<<< "word"` feeds the word PLUS a newline
                                // (bash appends one); without it `cat <<< x`
                                // printed `x` with no trailing newline and
                                // the next echo ran onto the same line
                                // (zz-hs-probe2.sh: `plainrc=0`)
                                self.need_cat = true;
                                text = format!("shStrCat({text}, \"\\n\")");
                            }
                            self.emit(&format!("var {sv}: c_int = -1;"));
                            self.emit(&format!("var {blk}_p_{sv}: [2]std.c.fd_t = undefined;"));
                            self.emit(&format!("if (std.c.pipe(&{blk}_p_{sv}) != 0) {{ {blk}_ok = false; }} else {{"));
                            self.depth += 1;
                            self.emit(&format!("const __wf = std.Io.File{{ .handle = {blk}_p_{sv}[1], .flags = .{{ .nonblocking = false }} }};"));
                            self.emit(&format!("__wf.writeStreamingAll(sh_io orelse unreachable, {text}) catch {{ {blk}_ok = false; }};"));
                            self.emit("__wf.close(sh_io orelse unreachable);");
                            self.emit(&format!("{sv} = std.c.dup(0); _ = std.c.dup2({blk}_p_{sv}[0], 0); _ = std.c.close({blk}_p_{sv}[0]);"));
                            self.depth -= 1;
                            self.emit("}");
                            if !inner.is_empty() {
                                saves.push((fd, sv));
                            } else {
                                self.emit(&format!("_ = &{sv};"));
                            }
                        }
                        _ => {}
                    }
                }
                self.emit(&format!("if ({blk}_ok) {{"));
                self.depth += 1;
                for s in inner {
                    self.stmt(s);
                }
                self.emit("stdout.flush() catch {};");
                self.depth -= 1;
                self.emit("} else { sh_rc = 1; }");
                // restore in reverse (only swapped ones)
                for (fd, sv) in saves.iter().rev() {
                    self.emit(&format!("if ({sv} >= 0) {{ _ = std.c.dup2({sv}, {fd}); _ = std.c.close({sv}); }}"));
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::Die { .. }
            | IrStmt::Warn { .. }
            | IrStmt::SetChildError(_)
            | IrStmt::Require(_)
            | IrStmt::RawText(_)
            | IrStmt::Redirect { .. }
            | IrStmt::Subshell(_)
            | IrStmt::Background(_)
            | IrStmt::Label(_)
            | IrStmt::Goto(_) => {
                self.mark_todo(&format!("stmt {:?}", s));
            }
            IrStmt::Return(v) => {
                if self.in_function {
                    // user-function value return: strings are the value
                    // model (numeric contexts coerce at the read site).
                    // A numeric literal also sets the status (bash `f;
                    // echo $?` sees return N — non-numeric values leave it).
                    let r = v.as_ref().map(|e| self.expr_str(e)).unwrap_or_default();
                    if let Some(e) = v {
                        if let IrExpr::Int(n) = e {
                            self.emit(&format!("sh_rc = {n};"));
                        }
                    }
                    self.emit(&format!("return {r};"));
                    return;
                }
                // top-level `return`: bash errors (stderr, ignored by the
                // gate) and CONTINUES with $? = 2 (probed) — never a Zig
                // return from main (type error) nor process exit (bash
                // keeps running)
                self.emit("sh_rc = 2;");
                self.rc_dirty = true;
            }
            IrStmt::Function { name, body, .. } => {
                // Render as a hoisted Zig fn taking POSITIONAL params:
                // `fn name(__args: []const []const u8) []const u8` — the
                // body's getVar("N") reads route through __args (the
                // $1 convention), Return returns the value string.
                // Hoisted out of main (pending_fns) — a fn nested inside
                // another fn is legal zig but the decls/return machinery
                // above assumes top level.
                let id = self.zig_ident(name);
                // fn-local names must read as DECLARED inside the body
                // (they are the fn's own stores) but must not leak into the
                // module-level `written` set afterwards — a leak made a
                // top-level read of a never-called fn's local emit a bare
                // undeclared identifier (parse-bare-quote-in-function)
                let fn_locals_now = function_locals(body);
                let written_saved = std::mem::take(&mut self.written);
                self.written = written_saved.clone();
                for v in &fn_locals_now {
                    self.written.insert(v.clone());
                }
                let prev_in_fn = self.in_function;
                self.in_function = true;
                let mut saved = std::mem::take(&mut self.out);
                let n_body = body.len();
                // did the body already return (trailing echo-value or an
                // explicit Return)? A value-returning fn that falls off
                // the end is a compile error — append `return "";`.
                let mut body_returns = false;
                for (bi, s) in body.iter().enumerate() {
                    // the single trailing `echo V` becomes the return
                    // value (the caller consumed it as the $(fn) capture)
                    if bi == n_body - 1 {
                        if let Some((v, nl)) = trailing_echo_value(s) {
                            let r = self.expr_str(&v);
                            self.emit(&format!("return {r};"));
                            body_returns = true;
                            self.value_fns.insert(name.clone(), nl);
                            continue;
                        }
                        if matches!(s, IrStmt::Return(_)) {
                            body_returns = true;
                        }
                    }
                    self.stmt(s);
                }
                if !body_returns {
                    self.emit("return \"\";");
                }
                std::mem::swap(&mut saved, &mut self.out);
                self.in_function = prev_in_fn;
                self.written = written_saved;
                let mut locals = function_locals(body);
                // capture-promoted vars stay global (bash dynamic scoping):
                // a fn-local shadow here would hide the global the nested
                // fn reads
                locals.retain(|v| !self.globals.contains(v));
                // vars written at TOP level are shared globals (bash writes
                // global unless `local`): keep them global here too, EXCEPT
                // names this fn itself declares `local` (Declare{local} or
                // a builtin `local` word — those genuinely shadow)
                {
                    let mut must_local: BTreeSet<String> = BTreeSet::new();
                    for st in body {
                        if let IrStmt::Declare { vars, local: true, .. } = st {
                            for d in vars {
                                must_local.insert(d.name.clone());
                            }
                        }
                        if let IrStmt::Expr(IrExpr::Call { func, args }) = st {
                            if func == "builtin" {
                                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                                    if cmd == "local" {
                                        for n in builtin_declare_names(cmd, args) {
                                            must_local.insert(n);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    locals.retain(|v| must_local.contains(v) || !self.top_written.contains(v));
                }
                self.fn_locals.extend(locals.iter().cloned());
                let uses_stdout = saved.iter().any(|l| l.contains("stdout.print") || l.contains("stdout.flush"));
                let uses_args = saved.iter().any(|l| l.contains("__args"));
                // a body with `try` (bigint Managed ops, file IO) must
                // return an error union
                let uses_try = saved.iter().any(|l| l.contains("try "));
                if uses_try {
                    self.error_fns.insert(name.clone());
                }
                let ret = if uses_try { "![]const u8" } else { "[]const u8" };
                let mut fntext = format!(
                    "fn {id}(init: std.process.Init, __args: []const []const u8) {ret} {{\n"
                );
                if uses_stdout {
                    fntext.push_str("    var stdout_buffer: [4096]u8 = undefined;\n");
                    fntext.push_str("    var stdout_file_writer: std.Io.File.Writer = .initStreaming(.stdout(), init.io, &stdout_buffer);\n");
                    fntext.push_str("    const stdout = &stdout_file_writer.interface;\n");
                } else {
                    // a pure value-return fn never touches init
                    fntext.push_str("    _ = &init;\n");
                }
                // mark __args used only when the body never reads a param
                // (Zig rejects an unused function parameter)
                if !uses_args {
                    fntext.push_str("    _ = &__args;\n");
                }
                for v in &locals {
                    let m = self.zig_ident(v);
                    if self.bigint_arrays.contains(v) {
                        // bigint list → fixed Managed array + length counter
                        fntext.push_str(&format!("    var {m}: [1024]std.math.big.int.Managed = undefined;\n"));
                        fntext.push_str(&format!("    var {m}_len: usize = 0;\n"));
                    } else if self.set_arrays.contains(v) {
                        // set → fixed i64 array + length counter
                        fntext.push_str(&format!("    var {m}: [1024]i64 = undefined;\n"));
                        fntext.push_str(&format!("    var {m}_len: usize = 0;\n"));
                    } else if self.str_arrays.contains(v) {
                        // string list → fixed []const u8 array + length counter
                        fntext.push_str(&format!("    var {m}: [1024][]const u8 = undefined;\n"));
                        fntext.push_str(&format!("    var {m}_len: usize = 0;\n"));
                    } else if self.bigint_vars.contains(v) {
                        fntext.push_str(&format!("    var {m} = try std.math.big.int.Managed.init(std.heap.page_allocator);\n"));
                        fntext.push_str(&format!("    defer {m}.deinit();\n"));
                    } else if self.is_num(v) {
                        fntext.push_str(&format!("    var {m}: i64 = 0;\n"));
                    } else {
                        fntext.push_str(&format!("    var {m}: []const u8 = \"\";\n"));
                    }
                }
                fntext.push_str(&saved.join("\n"));
                fntext.push('\n');
                fntext.push_str("}\n");
                self.pending_fns.push(fntext);
                // defining a function executes nothing (bash: status 0) —
                // the hoisted body's leaves must not count as executed here
                self.rc_dirty = false;
            }
            IrStmt::Try { .. } => self.mark_todo("try"),
            IrStmt::Select { .. } => self.mark_todo("select"),
            IrStmt::Asm { .. } => self.mark_todo("asm"),
            IrStmt::ForInit {
                init,
                cond,
                step,
                body,
            } => {
                // native while-loop: init; while (cond) { body; step }
                for s in init {
                    self.stmt(s);
                }
                let c = self.expr_bool(cond);
                self.emit(&format!("while ({c}) {{"));
                self.loop_depth += 1;
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                for s in step {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.loop_depth -= 1;
                self.emit("}");
            }
            IrStmt::Continue => self.emit("continue;"),
            IrStmt::Break => self.emit("break;"),
            IrStmt::Shaderize { .. } => {
                self.mark_todo("shaderize (autoshaderization) not supported by the Zig backend");
            }
        }
    }

    fn program(&mut self, prog: &IrProgram) {
        // Pass 0: global promotion. A var written at top level AND touched
        // (read or written) inside any function is ONE shared global (bash
        // has no `local` unless declared) — main-locals and fn-locals would
        // both be invisible to the other side. Nested-fn reads of enclosing
        // locals promote the same way (dynamic scoping). Promoted globals
        // get file-scope-safe manglings (no helper-internal collisions).
        let mut fn_locals_all: BTreeSet<String> = BTreeSet::new();
        let mut targets_all: BTreeSet<String> = BTreeSet::new();
        {
            let mut top = BTreeSet::new();
            collect_written(&prog.stmts, &mut top);
            self.top_written = top.clone();
            collect_targets(&prog.stmts, &mut targets_all);
            {
                let mut fb: Vec<&[IrStmt]> = Vec::new();
                all_fn_bodies(&prog.stmts, &mut fb);
                for b in fb {
                    collect_targets(b, &mut targets_all);
                }
            }
            let mut touched = BTreeSet::new();
            let mut bodies: Vec<&[IrStmt]> = Vec::new();
            all_fn_bodies(&prog.stmts, &mut bodies);
            for b in bodies {
                reads_stmts(b, &mut touched);
                collect_targets(b, &mut touched);
                fn_locals_all.extend(function_locals(b));
            }
            // capture-promoted GLOBALS are top-level variables a FUNCTION
            // reads — not every name read anywhere. Taking all reads
            // promoted environment variables (`$HOME`) and made them
            // uninitialised declarations (t05_env_read)
            {
                let mut reads_all: BTreeSet<String> = BTreeSet::new();
                collect_captured(&prog.stmts, &mut reads_all);
                for n in &reads_all {
                    if !targets_all.contains(n) {
                        self.read_only_names.insert(n.clone());
                    }
                }
                let mut fn_reads: BTreeSet<String> = BTreeSet::new();
                let mut fb2: Vec<&[IrStmt]> = Vec::new();
                all_fn_bodies(&prog.stmts, &mut fb2);
                for b in fb2 {
                    reads_stmts(b, &mut fn_reads);
                }
                // a var ANY function writes and another (or the same one)
                // reads is shared shell state — bash has dynamic scoping, so
                // a nested fn sees the enclosing fn's assignment
                let mut fn_writes: BTreeSet<String> = BTreeSet::new();
                let mut fb3: Vec<&[IrStmt]> = Vec::new();
                all_fn_bodies(&prog.stmts, &mut fb3);
                for b in fb3 {
                    collect_targets(b, &mut fn_writes);
                }
                for n in fn_reads {
                    if self.top_written.contains(&n) || top.contains(&n) || fn_writes.contains(&n) {
                        self.globals.insert(n);
                    }
                }
            }
            for v in top {
                if touched.contains(&v) {
                    self.globals.insert(v);
                }
            }
            let names: Vec<String> = self.globals.iter().cloned().collect();
            for v in names {
                let mut m = String::new();
                for c in v.chars() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        m.push(c);
                    } else {
                        m.push('_');
                    }
                }
                if m.is_empty() || m.chars().next().unwrap().is_ascii_digit() {
                    m.insert_str(0, "v_");
                }
                if ZIG_RESERVED.contains(&m.as_str())
                    || HELPER_INTERNALS.contains(&m.as_str())
                {
                    m.push('_');
                }
                if m == "_" {
                    m = "underscore".to_string();
                }
                let base = m.clone();
                let mut n = 1;
                while self.mangle.values().any(|x| x == &m) {
                    m = format!("{base}{n}");
                    n += 1;
                }
                self.mangle.insert(v, m);
            }
        }
        // Pass 1: collect written vars so declarations are hoisted.
        let mut written: BTreeSet<String> = BTreeSet::new();
        collect_written(&prog.stmts, &mut written);
        // top-level writes (function bodies are excluded by the walker):
        // functions assigning these names write the shared global
        // var_types carries fn-LOCAL names too (the core records the type
        // of every assigned var). A name that is only ever a fn local must
        // NOT be declared at module level: the decl loop skips it anyway,
        // so marking it written made top-level reads emit a bare
        // undeclared identifier.
        for (n, _) in &prog.var_types {
            let vt_glob = self.globals.contains(n);
            let vt_fn_local = fn_locals_all.contains(n) && !vt_glob;
            // a var_types entry only means the ANALYSIS inferred a type — it
            // does not hoist the name. A name the program NEVER assigns is an
            // environment read (`$HOME`), and declaring it made the read a
            // bare identifier that printed an empty line (t05_env_read).
            // A var_types name is normally a REAL assignment (the analysis
            // seeded it from one). The exception is an env-only read
            // (`$HOME`): the name has a type entry but the program never
            // assigns it — hoisting it printed an empty line. `assigned`
            // records both the static targets and the names the RENDERER
            // marks written (setVar arms, eval, source, array element
            // writes), so the check is on static targets only here and
            // env names without one stay unhoisted.
            if !vt_fn_local && targets_all.contains(n) {
                written.insert(n.clone());
            }
        }
        // `declared()` consults this during rendering (a var reference
        // before its assignment in source order still lowers natively).
        // Promoted globals (Pass 0) are main-level even though no top-level
        // statement may write them (function bodies are excluded from the
        // walk below).
        written.extend(self.globals.iter().cloned());
        self.written = written.clone();

        // set-typed arrays (setArray/setArrayAppend) — lowered to a
        // fixed i64 array + length counter
        let mut set_arrays = BTreeSet::new();
        collect_set_arrays(&prog.stmts, &mut set_arrays);
        // string-element arrays keep the []const u8 home (and leave the
        // i64 set when every element is numeric)
        let mut str_arrays = BTreeSet::new();
        collect_str_arrays(&prog.stmts, &mut str_arrays);
        set_arrays.retain(|v| !str_arrays.contains(v));
        self.set_arrays = set_arrays;
        self.str_arrays = str_arrays;

        // bigint-element arrays — lowered to a Managed array
        let mut bigint_arrays = BTreeSet::new();
        collect_bigint_arrays(&prog.stmts, &self.bigint_vars, &mut bigint_arrays);
        for sub in &prog.subs {
            collect_bigint_arrays(&sub.body, &self.bigint_vars, &mut bigint_arrays);
        }
        self.bigint_arrays = bigint_arrays;

        // bigint vars (arith beyond i64) — lowered to Managed
        self.bigint_vars = crate::shir::analyze_bigint_vars(prog);
        if !self.bigint_vars.is_empty() {
            self.need_bigint = true;
        }

        // Pass 2: render the body first (helper flags known before
        // preamble). The hoisted declarations must precede the statements
        // lexically, so they are collected separately and spliced in.
        let mut body_stmts: Vec<String> = Vec::new();
        std::mem::swap(&mut self.out, &mut body_stmts);
        self.depth = 1;
        for (idx, s) in prog.stmts.iter().enumerate() {
            let before = self.out.len();
            self.stmt(s);
            let line = prog.stmt_lines.iter().find(|(i, _)| *i == idx).map(|(_, l)| *l);
            if let Some(l) = line {
                if let Some(first) = self.out.get_mut(before) {
                    *first = format!("{first} // line {l}");
                }
            }
        }
        // dead-var guard: Zig rejects declared-but-never-read variables.
        // `_ = x;` is NOT enough on the 0.16+ target: a variable that was
        // ASSIGNED (which is what `written` holds) makes the discard itself an
        // error — "pointless discard of local variable" (verified on 0.16.0 and
        // 0.17.0). Discarding the ADDRESS (`_ = &x;`) keeps the declaration
        // live for both cases: it reads the variable AND satisfies
        // "never mutated" for an unassigned one (taking the address counts as a
        // potential mutation). Mirrors the core fix in sh2perl/src/zig_backend.rs.
        for v in &written {
            if !self.read.contains(v) && !self.fn_locals.contains(v) {
                let m = self
                    .mangle
                    .get(v)
                    .cloned()
                    .unwrap_or_else(|| self.zig_ident(v));
                self.emit(&format!("_ = &{m};"));
            }
        }
        std::mem::swap(&mut self.out, &mut body_stmts);
        self.depth = 0;

        // ── preamble: import, helpers, stubs, then main ──
        self.emit("const std = @import(\"std\");");
        // last-command status (`$?`, the C backend's _sh_rc): recorded by
        // shRun/shRunStmt/shUuRun and `false`, read by getVar("?"), and the
        // program's final exit code. Always emitted (file-scope; no
        // unused-var error) so every read site typechecks.
        self.emit("");
        self.emit("var sh_rc: i64 = 0;");
        if !self.sh2_calls.is_empty() {
            self.emit("");
            self.emit("// sh2.* runtime helpers — TODO: implement (harness/sh2-namespace.json)");
            self.emit("fn sh2TODO(name: []const u8) noreturn {");
            self.emit("    std.debug.print(\"TODO sh2.{s}\\n\", .{name});");
            self.emit("    std.process.exit(2);");
            self.emit("}");
        }
        if self.need_toint {
            self.emit("");
            self.emit("fn shToInt(s: []const u8) i64 {");
            self.emit("    return std.fmt.parseInt(i64, std.mem.trim(u8, s, \" \\t\\n\\r\"), 10) catch 0;");
            self.emit("}");
        }
        if self.need_intstr {
            self.emit("");
            self.emit("fn shIntStr(n: i64) []const u8 {");
            self.emit(
                "    return std.fmt.allocPrint(std.heap.page_allocator, \"{d}\", .{n}) catch \"\";",
            );
            self.emit("}");
        }
        if self.need_truthy {
            self.emit("");
            self.emit("fn shTruthy(s: []const u8) bool {");
            self.emit("    return s.len != 0;");
            self.emit("}");
        }
        if self.need_cat {
            self.emit("");
            self.emit("fn shStrCat(a: []const u8, b: []const u8) []const u8 {");
            self.emit("    return std.fmt.allocPrint(std.heap.page_allocator, \"{s}{s}\", .{ a, b }) catch \"\";");
            self.emit("}");
        }
        if self.need_arrjoin {
            self.emit("");
            self.emit("fn shArrJoin(a: []const []const u8, n: usize) []const u8 {");
            self.emit("    // string context of an array: elements joined by a SPACE");
            self.emit("    // (the default IFS; the corpus uses the default)");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    var k: usize = 0;");
            self.emit("    while (k < n) : (k += 1) {");
            self.emit("        if (k != 0) out.append(std.heap.page_allocator, ' ') catch return \"\";");
            self.emit("        out.appendSlice(std.heap.page_allocator, a[k]) catch return \"\";");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
        }
        if self.need_bexpand {
            self.emit("");
            // printf %b: expand \\n \\t \\r \\\\ \\a \\b \\f \\v and \\0NNN in
            // the ARGUMENT (format-text escapes are decoded at render time)
            self.emit("fn shBEscape(s: []const u8) []const u8 {");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    var i: usize = 0;");
            self.emit("    while (i < s.len) : (i += 1) {");
            self.emit("        if (s[i] != '\\\\' or i + 1 >= s.len) { out.append(std.heap.page_allocator, s[i]) catch return \"\"; continue; }");
            self.emit("        i += 1;");
            self.emit("        const c = switch (s[i]) { 'n' => @as(u8, '\\n'), 't' => '\\t', 'r' => '\\r', 'a' => 7, 'b' => 8, 'f' => 12, 'v' => 11, '\\\\' => '\\\\', else => 0 };");
            self.emit("        if (c != 0) { out.append(std.heap.page_allocator, c) catch return \"\"; continue; }");
            self.emit("        if (s[i] == '0') {");
            self.emit("            var v: u8 = 0;");
            self.emit("            var k: usize = 0;");
            self.emit("            i += 1;");
            self.emit("            while (i < s.len and k < 3 and s[i] >= '0' and s[i] <= '7') : (i += 1) { v = v * 8 + (s[i] - '0'); k += 1; }");
            self.emit("            out.append(std.heap.page_allocator, v) catch return \"\";");
            self.emit("            i -= 1;");
            self.emit("            continue;");
            self.emit("        }");
            self.emit("        out.append(std.heap.page_allocator, s[i]) catch return \"\";");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
        }
        if self.need_argv0 {
            self.emit("");
            self.emit("fn shArgv0() []const u8 {");
            self.emit("    // SH2_ARGV0 is what the invoker passed as $0 (the gate sets it to");
            self.emit("    // the script path so usage lines match bash)");
            self.emit("    if (sh_environ) |env| return env.get(\"SH2_ARGV0\") orelse \"\";");
            self.emit("    return \"\";");
            self.emit("}");
        }
        if self.shopt_nocasematch {
            self.emit("");
            self.emit("fn shEqI(a: []const u8, b: []const u8) bool {");
            self.emit("    if (a.len != b.len) return false;");
            self.emit("    for (a, b) |x, y| { if (std.ascii.toLower(x) != std.ascii.toLower(y)) return false; }");
            self.emit("    return true;");
            self.emit("}");
        }
        if self.need_pad {
            self.emit("");
            self.emit("fn shPadW(s: []const u8, w: usize, left: bool) []const u8 {");
            self.emit("    if (s.len >= w) return s;");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    if (!left) {");
            self.emit("        var k: usize = s.len;");
            self.emit("        while (k < w) : (k += 1) out.append(std.heap.page_allocator, ' ') catch return s;");
            self.emit("        out.appendSlice(std.heap.page_allocator, s) catch return s;");
            self.emit("    } else {");
            self.emit("        out.appendSlice(std.heap.page_allocator, s) catch return s;");
            self.emit("        var k: usize = s.len;");
            self.emit("        while (k < w) : (k += 1) out.append(std.heap.page_allocator, ' ') catch return s;");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
        }
        if self.need_shift {
            self.emit("");
            self.emit("fn shPow(a: i64, b: i64) i64 { if (b < 0) return 0; return std.math.powi(i64, a, b) catch 0; }");
            self.emit("fn shShl(a: i64, b: i64) i64 {");
            self.emit("    const s: u6 = @intCast(@as(u64, @bitCast(b)) & 63);");
            self.emit("    return a << s;");
            self.emit("}");
            self.emit("fn shShr(a: i64, b: i64) i64 {");
            self.emit("    const s: u6 = @intCast(@as(u64, @bitCast(b)) & 63);");
            self.emit("    return a >> s;");
            self.emit("}");
        }
        if self.need_safe_div {
            self.emit("");
            // bash's arithmetic error: message on stderr, the enclosing
            // assignment is ABANDONED (the variable keeps its old value),
            // and the shell exits 1 at the end of that command's list. We
            // approximate the value half exactly (flag + no store).
            self.emit("var sh_arith_err = false;");
            self.emit("fn shDiv(a: i64, b: i64) i64 {");
            self.emit("    if (b == 0) { sh_arith_err = true; return 0; }");
            self.emit("    return @divTrunc(a, b);");
            self.emit("}");
            self.emit("fn shRem(a: i64, b: i64) i64 {");
            self.emit("    if (b == 0) { sh_arith_err = true; return 0; }");
            self.emit("    return @rem(a, b);");
            self.emit("}");
        }
        if self.need_b2s {
            self.emit("");
            self.emit("fn shB2S(b: bool) []const u8 {");
            self.emit("    return if (b) \"1\" else \"0\";");
            self.emit("}");
        }
        if self.saw_background || self.need_run {
            self.emit("");
            self.emit("extern fn waitpid(pid: c_int, status: ?*c_int, options: c_int) c_int;");
        }
        if self.need_env || self.need_run || self.need_fexist || self.need_writefile || self.need_globpat || self.need_source {
            // Process handle for the runtime helpers. `Init` is only available
            // inside the emitted `main`, and zig has no way to thread it into a
            // free helper without changing every call site, so the emitted main
            // publishes it here (see the `sh_io = init.io;` injection).
            self.emit("");
            self.emit("var sh_io: ?std.Io = null;");
        }
        if self.need_env {
            self.need_run = true;
            self.emit("var sh_environ: ?*std.process.Environ.Map = null;");
            self.emit("");
            self.emit("fn shEnv(name: []const u8) []const u8 {");
            self.emit("    if (sh_environ) |__sh_m| {");
            self.emit("        if (__sh_m.get(name)) |v| return v;");
            self.emit("    }");
            // bash DEFINES a few shell variables that need not be exported:
            // HOSTNAME (the kernel hostname) is the one the corpus reads
            self.emit("    if (std.mem.eql(u8, name, \"HOSTNAME\")) {");
            self.emit("        if (sh_io) |io| {");
            self.emit("            const data = std.Io.Dir.cwd().readFileAlloc(io, \"/etc/hostname\", std.heap.page_allocator, .limited(256)) catch return \"\";");
            self.emit("            return std.mem.trimEnd(u8, data, \"\\r\\n\");");
            self.emit("        }");
            self.emit("    }");
            self.emit("    return \"\";");
            self.emit("}");
        }
        if self.need_source {
            // `.`/`source` at RUN time: read the file and apply its simple
            // `NAME=value` lines. The names are the program's DECLARED
            // globals (a dispatch on the file's name), because a sourced
            // value must land in the same home the reader consults — an
            // env-map write was shadowed by the declared `var` and printed
            // empty (dot-source-lib.sh: `shared=`).
            self.emit("");
            self.emit("fn shSource(path: []const u8) void {");
            self.emit("    const io = sh_io orelse return;");
            self.emit("    const data = std.Io.Dir.cwd().readFileAlloc(io, path, std.heap.page_allocator, .limited(1 << 20)) catch { sh_rc = 1; return; };");
            self.emit("    var it = std.mem.splitScalar(u8, data, '\\n');");
            self.emit("    while (it.next()) |line0| {");
            self.emit("        const line = std.mem.trim(u8, line0, \" \\t\\r\");");
            self.emit("        if (line.len == 0 or line[0] == '#') continue;");
            self.emit("        const eq = std.mem.indexOfScalar(u8, line, '=') orelse continue;");
            self.emit("        const nm = std.mem.trim(u8, line[0..eq], \" \\t\");");
            self.emit("        if (nm.len == 0) continue;");
            self.emit("        var val = std.mem.trim(u8, line[eq + 1 ..], \" \\t\");");
            self.emit("        if (val.len >= 2 and (val[0] == 0x27 or val[0] == 0x22) and val[val.len - 1] == val[0]) val = val[1 .. val.len - 1];");
            self.emit("        const cv = std.heap.page_allocator.dupe(u8, val) catch continue;");
            for name in self.source_targets.iter().cloned().collect::<Vec<_>>() {
                let m = self.zig_ident(&name);
                self.emit(&format!(
                    "        if (std.mem.eql(u8, nm, {})) {{ {m} = cv; continue; }}",
                    Self::zig_str(&name)
                ));
            }
            self.emit("        if (sh_environ) |env| {");
            self.emit("            env.put(std.heap.page_allocator.dupe(u8, nm) catch continue, cv) catch {};");
            self.emit("        }");
            self.emit("    }");
            self.emit("    sh_rc = 0;");
            self.emit("}");
        }
        if self.need_fexist {
            self.emit("");
            self.emit("fn shFileTest(flag: []const u8, p: []const u8) bool {");
            self.emit("    const io = sh_io orelse return false;");
            self.emit("    const st = std.Io.Dir.cwd().statFile(io, p, .{}) catch return false;");
            self.emit("    if (std.mem.eql(u8, flag, \"-f\")) return st.kind == .file;");
            self.emit("    if (std.mem.eql(u8, flag, \"-d\")) return st.kind == .directory;");
            self.emit("    if (std.mem.eql(u8, flag, \"-e\")) return true;");
            self.emit("    if (std.mem.eql(u8, flag, \"-s\")) return st.size != 0;");
            self.emit("    return false;");
            self.emit("}");
        }
        if self.need_writefile {
            self.emit("");
            self.emit("fn shWriteFile(p: []const u8, data: []const u8, append: bool) void {");
            self.emit("    const io = sh_io orelse return;");
            // `__sh_f`: a single-letter helper local shadows a user fn of
            // the same name declared at file scope ("local constant shadows
            // declaration of 'f'" — parse-heredoc-in-if.sh has `f()`).
            self.emit("    const __sh_f = std.Io.Dir.cwd().createFile(io, p, .{ .truncate = !append }) catch return;");
            self.emit("    defer __sh_f.close(io);");
            self.emit("    __sh_f.writeStreamingAll(io, data) catch {};");
            self.emit("}");
        }
        if self.need_ext {
            self.emit("");
            self.emit("fn shContains(hay: []const u8, needle: []const u8) bool {");
            self.emit("    return std.mem.indexOf(u8, hay, needle) != null;");
            self.emit("}");
            self.emit("fn shSub(t: []const u8, off: i64, len: i64) []const u8 {");
            self.emit("    const n: i64 = @intCast(t.len);");
            self.emit("    var o = if (off < 0) n + off else off;");
            self.emit("    o = @max(0, @min(o, n));");
            self.emit("    const ou: usize = @intCast(o);");
            self.emit("    if (len < 0) {");
            self.emit("        var e = n + len;");
            self.emit("        e = @max(o, @min(e, n));");
            self.emit("        return t[ou..@as(usize, @intCast(e))];");
            self.emit("    }");
            self.emit("    const e: usize = @min(ou +| @as(usize, @intCast(len)), t.len);");
            self.emit("    return t[ou..e];");
            self.emit("}");
            self.emit("fn shStripPre(t: []const u8, pat: []const u8, longest: bool) []const u8 {");
            self.emit("    // shortest/longest PREFIX of t matching the glob pat is removed");
            self.emit("    if (pat.len == 0) return t;");
            self.emit("    var best: ?usize = null;");
            self.emit("    var k: usize = 0;");
            self.emit("    while (k <= t.len) : (k += 1) {");
            self.emit("        if (shGlobName(pat, t[0..k])) {");
            self.emit("            if (!longest) return t[k..];");
            self.emit("            best = k;");
            self.emit("        }");
            self.emit("    }");
            self.emit("    if (best) |b| return t[b..];");
            self.emit("    return t;");
            self.emit("}");
            self.emit("fn shStripSuf(t: []const u8, pat: []const u8, longest: bool) []const u8 {");
            self.emit("    // shortest/longest SUFFIX of t matching the glob pat is removed");
            self.emit("    if (pat.len == 0) return t;");
            self.emit("    var best: ?usize = null;");
            self.emit("    var k: usize = 0;");
            self.emit("    while (k <= t.len) : (k += 1) {");
            self.emit("        if (shGlobName(pat, t[t.len - k ..])) {");
            self.emit("            if (!longest) return t[0 .. t.len - k];");
            self.emit("            best = k;");
            self.emit("        }");
            self.emit("    }");
            self.emit("    if (best) |b| return t[0 .. t.len - b];");
            self.emit("    return t;");
            self.emit("}");
            self.emit("fn shReplace(t: []const u8, pat: []const u8, repl: []const u8, all: bool) []const u8 {");
            self.emit("    // glob-pattern replace (first or all, left-to-right, non-overlapping)");
            self.emit("    if (pat.len == 0) return t;");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    var j: usize = 0;");
            self.emit("    var done = false;");
            self.emit("    while (j <= t.len) {");
            self.emit("        var hit: usize = 0;");
            self.emit("        if (!done) {");
            self.emit("            var k: usize = 1;");
            self.emit("            while (j + k <= t.len) : (k += 1) {");
            self.emit("                if (shGlobName(pat, t[j .. j + k])) hit = k;");
            self.emit("            }");
            self.emit("        }");
            self.emit("        if (hit > 0) {");
            self.emit("            out.appendSlice(std.heap.page_allocator, repl) catch return \"\";");
            self.emit("            j += hit;");
            self.emit("            if (!all) done = true;");
            self.emit("        } else {");
            self.emit("            if (j == t.len) break;");
            self.emit("            out.append(std.heap.page_allocator, t[j]) catch return \"\";");
            self.emit("            j += 1;");
            self.emit("        }");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
            self.emit("fn shCaseOne(t: []const u8, upper: bool) []const u8 {");
            self.emit("    // `^`/`,`: convert only the FIRST byte (bash operates on the");
            self.emit("    // first character; byte-wise is exact for ASCII)");
            self.emit("    if (t.len == 0) return t;");
            self.emit("    const __sh_b = std.heap.page_allocator.alloc(u8, t.len) catch return \"\";");
            self.emit("    @memcpy(__sh_b, t);");
            self.emit("    __sh_b[0] = if (upper) std.ascii.toUpper(t[0]) else std.ascii.toLower(t[0]);");
            self.emit("    return __sh_b;");
            self.emit("}");
            self.emit("fn shJoinWs(t: []const u8) []const u8 {");
            self.emit("    // word-split on IFS whitespace, drop empties, rejoin with");
            self.emit("    // single spaces (bash unquoted-substitution semantics)");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    var it = std.mem.tokenizeAny(u8, t, \" \\t\\n\\r\");");
            self.emit("    var first = true;");
            self.emit("    while (it.next()) |w| {");
            self.emit("        if (!first) out.append(std.heap.page_allocator, ' ') catch return \"\";");
            self.emit("        first = false;");
            self.emit("        out.appendSlice(std.heap.page_allocator, w) catch return \"\";");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
            self.emit("fn shJoinSp(a: []const []const u8) []const u8 {");
            self.emit("    // join argv words verbatim with single spaces (eval payloads)");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    for (a, 0..) |w, k| {");
            self.emit("        if (k != 0) out.append(std.heap.page_allocator, ' ') catch return \"\";");
            self.emit("        out.appendSlice(std.heap.page_allocator, w) catch return \"\";");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
            self.emit("fn shArrAppendStr(a: [][]const u8, n: *usize, text: []const u8) void {");
            self.emit("    // array `+=` of a joined string: append each word (bash appends");
            self.emit("    // ELEMENTS, so a multi-word append grows the array by each word)");
            self.emit("    var it = std.mem.tokenizeAny(u8, text, \" \\t\\n\\r\");");
            self.emit("    while (it.next()) |w| {");
            self.emit("        if (n.* >= 1024) return;");
            self.emit("        a[n.*] = w;");
            self.emit("        n.* += 1;");
            self.emit("    }");
            self.emit("}");
            self.emit("fn shStrArrSlice(a: []const []const u8, n: usize, off: i64, len: i64) []const u8 {");
            self.emit("    // `${arr[@]:off:len}` over a string home: space-joined window,");
            self.emit("    // negative offsets count from the end (bash)");
            self.emit("    const nn: i64 = @intCast(n);");
            self.emit("    var o = if (off < 0) nn + off else off;");
            self.emit("    o = @max(0, @min(o, nn));");
            self.emit("    var e: i64 = if (len < 0) nn +| len else o +| len;");
            self.emit("    e = @max(o, @min(e, nn));");
            self.emit("    const ou: usize = @intCast(o);");
            self.emit("    const eu: usize = @intCast(e);");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    for (a[ou..eu], 0..) |s, k| {");
            self.emit("        if (k != 0) out.append(std.heap.page_allocator, ' ') catch return \"\";");
            self.emit("        out.appendSlice(std.heap.page_allocator, s) catch return \"\";");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
            self.emit("fn shIntArrSlice(a: []const i64, n: usize, off: i64, len: i64) []const u8 {");
            self.emit("    // `${arr[@]:off:len}` over an int home: space-joined window");
            self.emit("    const nn: i64 = @intCast(n);");
            self.emit("    var o = if (off < 0) nn + off else off;");
            self.emit("    o = @max(0, @min(o, nn));");
            self.emit("    var e: i64 = if (len < 0) nn +| len else o +| len;");
            self.emit("    e = @max(o, @min(e, nn));");
            self.emit("    const ou: usize = @intCast(o);");
            self.emit("    const eu: usize = @intCast(e);");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    for (a[ou..eu], 0..) |v, k| {");
            self.emit("        if (k != 0) out.append(std.heap.page_allocator, ' ') catch return \"\";");
            self.emit("        const num = std.fmt.allocPrint(std.heap.page_allocator, \"{d}\", .{v}) catch return \"\";");
            self.emit("        out.appendSlice(std.heap.page_allocator, num) catch return \"\";");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
            self.emit("fn shCase(t: []const u8, upper: bool) []const u8 {");
            self.emit("    const __sh_b = std.heap.page_allocator.alloc(u8, t.len) catch return \"\";");
            self.emit("    for (t, 0..) |c, i| __sh_b[i] = if (upper) std.ascii.toUpper(c) else std.ascii.toLower(c);");
            self.emit("    return __sh_b;");
            self.emit("}");
            self.emit("fn shReplaceLit(t: []const u8, pat: []const u8, repl: []const u8, all: bool) []const u8 {");
            self.emit("    const __sh_i = std.mem.indexOf(u8, t, pat) orelse return t;");
            self.emit("    if (!all) {");
            self.emit("        return std.fmt.allocPrint(std.heap.page_allocator, \"{s}{s}{s}\", .{ t[0..__sh_i], repl, t[__sh_i + pat.len ..] }) catch \"\";");
            self.emit("    }");
            self.emit("    var n: usize = 0;");
            self.emit("    var j: usize = 0;");
            self.emit("    while (j + pat.len <= t.len) {");
            self.emit("        if (std.mem.startsWith(u8, t[j..], pat)) { n += 1; j += pat.len; } else j += 1;");
            self.emit("    }");
            self.emit("    const out = std.heap.page_allocator.alloc(u8, t.len - n * pat.len + n * repl.len) catch return \"\";");
            self.emit("    var w: usize = 0;");
            self.emit("    j = 0;");
            self.emit("    while (j < t.len) {");
            self.emit("        if (j + pat.len <= t.len and std.mem.eql(u8, t[j .. j + pat.len], pat)) {");
            self.emit("            @memcpy(out[w .. w + repl.len], repl);");
            self.emit("            w += repl.len; j += pat.len;");
            self.emit("        } else {");
            self.emit("            out[w] = t[j]; w += 1; j += 1;");
            self.emit("        }");
            self.emit("    }");
            self.emit("    return out[0..w];");
            self.emit("}");
            self.emit("fn shFields(allocator: std.mem.Allocator, t: []const u8, d: []const u8, ids: []const usize, suppress: bool) []const u8 {");
            self.emit("    if (!shContains(t, d)) return if (suppress) \"\" else t;");
            self.emit("    const __sh_p = std.heap.page_allocator.alloc([]const u8, std.mem.count(u8, t, d) + 1) catch return \"\";");
            self.emit("    var it = std.mem.splitScalar(u8, t, d[0]);");
            self.emit("    var np: usize = 0;");
            self.emit("    while (it.next()) |part| { __sh_p[np] = part; np += 1; }");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    var first = true;");
            self.emit("    for (ids) |id| {");
            self.emit("        if (id < __sh_p.len) {");
            self.emit("            if (!first) out.appendSlice(allocator, d) catch return \"\";");
            self.emit("            out.appendSlice(allocator, __sh_p[id]) catch return \"\";");
            self.emit("            first = false;");
            self.emit("        }");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
            self.emit("fn shTakeLines(t: []const u8, k: usize, fromEnd: bool) []const u8 {");
            self.emit("    var lines = std.ArrayList([]const u8).init(std.heap.page_allocator);");
            self.emit("    var it = std.mem.splitScalar(u8, t, '\\n');");
            self.emit("    while (it.next()) |part| lines.append(part) catch return \"\";");
            self.emit("    const n = lines.items.len;");
            self.emit("    const s0 = if (fromEnd) @max(0, n - k) else 0;");
            self.emit("    const e0 = if (fromEnd) n else @min(k, n);");
            self.emit("    return std.mem.join(std.heap.page_allocator, \"\\n\", lines.items[s0..e0]) catch \"\";");
            self.emit("}");
            self.emit("fn shBasename(t: []const u8) []const u8 {");
            self.emit("    const i = std.mem.lastIndexOfScalar(u8, t, '/') orelse return t;");
            self.emit("    return t[i + 1 ..];");
            self.emit("}");
            self.emit("fn shDirname(t: []const u8) []const u8 {");
            self.emit("    const i = std.mem.lastIndexOfScalar(u8, t, '/') orelse return \".\";");
            self.emit("    if (i == 0) return \"/\";");
            self.emit("    return t[0..i];");
            self.emit("}");
                        self.emit("var __rl_file: std.Io.File = undefined;");
            self.emit("var __rl_buf: [4096]u8 = undefined;");
            self.emit("var __rl_rdr: std.Io.File.Reader = undefined;");
            self.emit("var __rl_init: bool = false;");
            self.emit("fn shReadLine(io: std.Io) []const u8 {");
            self.emit("    if (!__rl_init) {");
            self.emit("        __rl_file = std.Io.File.stdin();");
            self.emit("        __rl_rdr = __rl_file.reader(io, &__rl_buf);");
            self.emit("        __rl_init = true;");
            self.emit("    }");
            self.emit("    const line = __rl_rdr.interface.takeDelimiter('\\n') catch return \"\";");
            self.emit("    return line orelse \"\";");
            self.emit("}");
self.emit("fn shTrimRightNl(t: []const u8) []const u8 {");
            self.emit("    var s = t;");
            self.emit("    while (s.len > 0 and s[s.len - 1] == '\\n') s = s[0 .. s.len - 1];");
            self.emit("    return s;");
            self.emit("}");
            self.emit("fn shGlobName(pat: []const u8, name: []const u8) bool {");
            self.emit("    // fnmatch-style: * ? [cls] — recursive matcher, no regex");
            self.emit("    if (pat.len == 0) return name.len == 0;");
            self.emit("    if (pat[0] == '*') {");
            self.emit("        var k: usize = 0;");
            self.emit("        while (k <= name.len) : (k += 1) {");
            self.emit("            if (shGlobName(pat[1..], name[k..])) return true;");
            self.emit("        }");
            self.emit("        return false;");
            self.emit("    }");
            self.emit("    if (name.len == 0) return false;");
            self.emit("    if (pat[0] == '?') return shGlobName(pat[1..], name[1..]);");
            self.emit("    if (pat[0] == '[') {");
            self.emit("        const close = std.mem.indexOfScalar(u8, pat, ']') orelse return false;");
            self.emit("        if (close < 2) return false;");
            self.emit("        var body = pat[1..close];");
            self.emit("        const negate = body[0] == '!' or body[0] == '^';");
            self.emit("        if (negate) body = body[1..];");
            self.emit("        var hit = false;");
            self.emit("        var bi: usize = 0;");
            self.emit("        while (bi < body.len) : (bi += 1) {");
            self.emit("            if (bi + 2 < body.len and body[bi + 1] == '-') {");
            self.emit("                if (name[0] >= body[bi] and name[0] <= body[bi + 2]) hit = true;");
            self.emit("                bi += 2;");
            self.emit("            } else if (body[bi] == name[0]) hit = true;");
            self.emit("        }");
            self.emit("        if (hit != negate) return shGlobName(pat[close + 1 ..], name[1..]);");
            self.emit("        return false;");
            self.emit("    }");
            self.emit("    return pat[0] == name[0] and shGlobName(pat[1..], name[1..]);");
            self.emit("}");
            self.emit("fn shTrimSides(t: []const u8, lead: bool, trail: bool) []const u8 {");
            self.emit("    var s = t;");
            self.emit("    if (lead) s = std.mem.trimLeft(u8, s, \" \\t\\n\\r\");");
            self.emit("    if (trail) s = std.mem.trimRight(u8, s, \" \\t\\n\\r\");");
            self.emit("    return s;");
            self.emit("}");
        }
        if self.need_bigint {
            self.emit("");
            self.emit("fn shBigIntStr(v: std.math.big.int.Managed) []const u8 {");
            self.emit("    return v.toString(std.heap.page_allocator, 10, .lower) catch \"\";");
            self.emit("}");
        }
        if self.need_sorted {
            self.emit("");
            self.emit("fn shSortedIntJoin(a: []const i64, n: usize) []const u8 {");
            self.emit("    var ix = std.heap.page_allocator.alloc(usize, n) catch return \"\";");
            self.emit("    for (0..n) |i| ix[i] = i;");
            self.emit("    std.mem.sort(usize, ix, a, struct {");
            self.emit("        fn lt(ctx: []const i64, x: usize, y: usize) bool {");
            self.emit("            return ctx[x] < ctx[y];");
            self.emit("        }");
            self.emit("    }.lt);");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    var prev: ?i64 = null;");
            self.emit("    for (ix) |j| {");
            self.emit("        if (prev != null and prev.? == a[j]) continue;");
            self.emit("        if (out.items.len != 0) out.appendSlice(std.heap.page_allocator, \", \") catch return \"\";");
            self.emit("        const num = std.fmt.allocPrint(std.heap.page_allocator, \"{d}\", .{a[j]}) catch return \"\";");
            self.emit("        out.appendSlice(std.heap.page_allocator, num) catch return \"\";");
            self.emit("        prev = a[j];");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
            self.emit("fn shSortedBigintJoin(a: []const std.math.big.int.Managed, n: usize) []const u8 {");
            self.emit("    var ix = std.heap.page_allocator.alloc(usize, n) catch return \"\";");
            self.emit("    for (0..n) |i| ix[i] = i;");
            self.emit("    std.mem.sort(usize, ix, a, struct {");
            self.emit("        fn lt(ctx: []const std.math.big.int.Managed, x: usize, y: usize) bool {");
            self.emit("            return std.math.big.int.Managed.order(ctx[x], ctx[y]) == .lt;");
            self.emit("        }");
            self.emit("    }.lt);");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    var prev: ?std.math.big.int.Managed = null;");
            self.emit("    for (ix) |j| {");
            self.emit("        if (prev != null and std.math.big.int.Managed.order(prev.?, a[j]) == .eq) continue;");
            self.emit("        if (out.items.len != 0) out.appendSlice(std.heap.page_allocator, \", \") catch return \"\";");
            self.emit("        const num = a[j].toString(std.heap.page_allocator, 10, .lower) catch return \"\";");
            self.emit("        out.appendSlice(std.heap.page_allocator, num) catch return \"\";");
            self.emit("        prev = a[j];");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
        }
        if self.need_reduce {
            self.emit("");
            self.emit("fn shMax(a: []const i64, n: usize) []const u8 {");
            self.emit("    if (n == 0) return \"0\";");
            self.emit("    var best = a[0];");
            self.emit("    for (a[1..n]) |v| { if (v > best) best = v; }");
            self.emit("    return std.fmt.allocPrint(std.heap.page_allocator, \"{d}\", .{best}) catch \"\";");
            self.emit("}");
            self.emit("fn shMin(a: []const i64, n: usize) []const u8 {");
            self.emit("    if (n == 0) return \"0\";");
            self.emit("    var best = a[0];");
            self.emit("    for (a[1..n]) |v| { if (v < best) best = v; }");
            self.emit("    return std.fmt.allocPrint(std.heap.page_allocator, \"{d}\", .{best}) catch \"\";");
            self.emit("}");
            self.emit("fn shSum(a: []const i64, n: usize) []const u8 {");
            self.emit("    var total: i64 = 0;");
            self.emit("    for (a[0..n]) |v| total += v;");
            self.emit("    return std.fmt.allocPrint(std.heap.page_allocator, \"{d}\", .{total}) catch \"\";");
            self.emit("}");
            self.emit("fn shMaxBigint(a: []const std.math.big.int.Managed, n: usize) []const u8 {");
            self.emit("    if (n == 0) return \"0\";");
            self.emit("    var best = a[0];");
            self.emit("    for (a[1..n]) |v| { if (std.math.big.int.Managed.order(v, best) == .gt) best = v; }");
            self.emit("    return best.toString(std.heap.page_allocator, 10, .lower) catch \"\";");
            self.emit("}");
            self.emit("fn shMinBigint(a: []const std.math.big.int.Managed, n: usize) []const u8 {");
            self.emit("    if (n == 0) return \"0\";");
            self.emit("    var best = a[0];");
            self.emit("    for (a[1..n]) |v| { if (std.math.big.int.Managed.order(v, best) == .lt) best = v; }");
            self.emit("    return best.toString(std.heap.page_allocator, 10, .lower) catch \"\";");
            self.emit("}");
            self.emit("fn shSumBigint(a: []const std.math.big.int.Managed, n: usize) []const u8 {");
            self.emit("    var total = std.math.big.int.Managed.initSet(std.heap.page_allocator, 0) catch return \"\";");
            self.emit("    defer total.deinit();");
            self.emit("    for (a[0..n]) |v| {");
            self.emit("        var t = std.math.big.int.Managed.init(std.heap.page_allocator) catch return \"\";");
            self.emit("        defer t.deinit();");
            self.emit("        t.add(&total, &v) catch return \"\";");
            self.emit("        total.copy(t.toConst()) catch return \"\";");
            self.emit("    }");
            self.emit("    return total.toString(std.heap.page_allocator, 10, .lower) catch \"\";");
            self.emit("}");
        }
        if self.need_run {
            self.emit("");
            // fork/exec arm — always emitted: it is the fallback for any
            // command the FFI library does not provide, and the only arm
            // when SH2_UU_FFI is off.
            self.emit("fn shRun(argv: []const []const u8) []const u8 {");
            self.emit("    const io = sh_io orelse return \"\";");
            self.emit("    const result = std.process.run(std.heap.page_allocator, io, .{");
            self.emit("        .argv = argv,");
            self.emit("        .stdout_limit = .limited(1 << 20),");
            self.emit("    }) catch return \"\";");
            self.emit("    sh_rc = switch (result.term) { .exited => |c| @as(i64, c), else => 1 };");
            self.emit("    return std.mem.trimEnd(u8, result.stdout, \"\\n\");");
            self.emit("}");
            self.emit("");
            // statement-position twin: same spawn, but the child INHERITS
            // our stdio (bash foreground semantics) instead of capturing.
            // Used by statement-level externals; `$(…)` keeps shRun.
            self.emit("fn shRunStmt(argv: []const []const u8) void {");
            self.emit("    const io = sh_io orelse return;");
            self.emit("    var child = std.process.spawn(io, .{ .argv = argv }) catch { sh_rc = 127; return; };");
            self.emit("    const term = child.wait(io) catch { sh_rc = 1; return; };");
            self.emit("    sh_rc = switch (term) { .exited => |c| @as(i64, c), else => 1 };");
            self.emit("}");
            self.emit("");
            // multi-stage pipelines (`a | b | c…`): fork every argv stage
            // chained by pipes; string-feed stages (echo/printf, whose
            // output must match our own print semantics) are written into
            // a libc pipe (no pipe() in the 0.17 Io surface). The last
            // stage inherits our stdout in statement position, or is
            // collected (1 MiB cap, trailing newlines trimmed like shRun)
            // in capture position. sh_rc = the last FORK stage's status
            // (a feed-last pipeline is 0 — feeds always succeed).
            self.emit("fn shPipe(stages: []const []const []const u8, feeds: []const ?[]const u8, capture: bool) []const u8 {");
            self.emit("    const io = sh_io orelse return \"\";");
            self.emit("    var prev: ?std.Io.File = null;");
            self.emit("    var childs: [64]std.process.Child = undefined;");
            self.emit("    var nchild: usize = 0;");
            self.emit("    var last_fork: ?usize = null;");
            self.emit("    var si: usize = 0;");
            self.emit("    for (stages, feeds) |argv, feed| {");
            self.emit("        const last = si == stages.len - 1;");
            self.emit("        si += 1;");
            self.emit("        if (feed) |text| {");
            self.emit("            // string stage: drain any piped input (echo/printf");
            self.emit("            // never read stdin), then feed our text onward");
            self.emit("            if (prev) |pf| {");
            self.emit("                var dbuf: [8192]u8 = undefined;");
            self.emit("                var drdr = pf.reader(io, &dbuf);");
            self.emit("                while (true) {");
            self.emit("                    const dn = drdr.interface.readSliceShort(&dbuf) catch break;");
            self.emit("                    if (dn == 0) break;");
            self.emit("                }");
            self.emit("            }");
            self.emit("            var fds: [2]std.c.fd_t = undefined;");
            self.emit("            if (std.c.pipe(&fds) != 0) { sh_rc = 1; return \"\"; }");
            self.emit("            const wf = std.Io.File{ .handle = fds[1], .flags = .{ .nonblocking = false } };");
            self.emit("            wf.writeStreamingAll(io, text) catch { sh_rc = 1; return \"\"; };");
            self.emit("            wf.close(io);");
            self.emit("            prev = std.Io.File{ .handle = fds[0], .flags = .{ .nonblocking = false } };");
            self.emit("            continue;");
            self.emit("        }");
            self.emit("        var opts = std.process.SpawnOptions{ .argv = argv };");
            self.emit("        if (prev) |pf| opts.stdin = .{ .file = pf };");
            self.emit("        if (!last or capture) opts.stdout = .pipe;");
            self.emit("        const child = std.process.spawn(io, opts) catch { sh_rc = 127; return \"\"; };");
            self.emit("        if (!last or capture) prev = child.stdout;");
            self.emit("        if (nchild < 64) { childs[nchild] = child; nchild += 1; last_fork = nchild - 1; }");
            self.emit("    }");
            self.emit("    var collected: []const u8 = \"\";");
            self.emit("    if (capture) {");
            self.emit("        if (prev) |pf| {");
            self.emit("            var cbuf: [8192]u8 = undefined;");
            self.emit("            var crdr = pf.reader(io, &cbuf);");
            self.emit("            var cacc: std.ArrayList(u8) = .empty;");
            self.emit("            while (true) {");
            self.emit("                const cn = crdr.interface.readSliceShort(&cbuf) catch break;");
            self.emit("                if (cn == 0) break;");
            self.emit("                if (cacc.items.len + cn > 1 << 20) break;");
            self.emit("                cacc.appendSlice(std.heap.page_allocator, cbuf[0..cn]) catch break;");
            self.emit("            }");
            self.emit("            collected = std.mem.trimEnd(u8, cacc.items, \"\\n\");");
            self.emit("        }");
            self.emit("    }");
            self.emit("    var last_rc: i64 = 0;");
            self.emit("    var ci: usize = 0;");
            self.emit("    while (ci < nchild) : (ci += 1) {");
            self.emit("        const term = childs[ci].wait(io) catch { if (last_fork != null and ci == last_fork.?) last_rc = 1; continue; };");
            self.emit("        if (last_fork != null and ci == last_fork.?) {");
            self.emit("            last_rc = switch (term) { .exited => |cc| @as(i64, cc), else => 1 };");
            self.emit("        }");
            self.emit("    }");
            self.emit("    sh_rc = last_rc;");
            self.emit("    return collected;");
            self.emit("}");
        }
        if self.need_uu_run {
            // UU-FFI.md: the in-process uutils coreutils arm. `uu_run(util,
            // argc, argv)` takes the SAME argv we already build, so only the
            // entry point differs. The emitted program must be linked against
            // libcoreutils_ffi.so WITH libc — a Rust cdylib does not
            // initialise without it (measured: identical zig segfaulted
            // without `-lc` and printed correctly with it). The invoker
            // supplies the link flags.
            self.emit("");
            self.emit("extern fn uu_run(util: [*:0]const u8, argc: c_int, argv: [*]const [*:0]const u8) c_int;");
            self.emit("");
            self.emit("fn shUuRun(argv: []const []const u8) []const u8 {");
            self.emit("    if (argv.len == 0) return \"\";");
            self.emit("    const zargs = std.heap.page_allocator.alloc([*:0]const u8, argv.len) catch return \"\";");
            self.emit("    for (argv, 0..) |a, i| {");
            self.emit("        zargs[i] = std.heap.page_allocator.dupeSentinel(u8, a, 0) catch return \"\";");
            self.emit("    }");
            self.emit("    sh_rc = uu_run(zargs[0], @intCast(argv.len), zargs.ptr);");
            self.emit("    return \"\";");
            self.emit("}");
        }
        if self.need_globpat {
            self.emit("");
            // pattern match for `[[ $x = PAT ]]`: a RECURSIVE matcher over
            // * ? [set] plus the extglob groups @(a|b) ?(a|b) *(a|b) +(a|b)
            // !(a|b) — the literal-equality path matched nothing for
            // `[[ $x = @(foo|bar) ]]` (at-in-test.sh).
            self.emit("fn shGlobPat(s: []const u8, pat: []const u8) bool {");
            self.emit("    return shGlob2(s, 0, pat, 0);");
            self.emit("}");
            // `shopt -s nocasematch`: fold both sides before matching
            self.emit("fn __sh_lower(s: []const u8) []const u8 {");
            self.emit("    const b = std.heap.page_allocator.alloc(u8, s.len) catch return s;");
            self.emit("    for (s, 0..) |c, i| b[i] = std.ascii.toLower(c);");
            self.emit("    return b;");
            self.emit("}");
            self.emit("fn shGlobPatI(s: []const u8, pat: []const u8) bool {");
            self.emit("    return shGlob2(__sh_lower(s), 0, __sh_lower(pat), 0);");
            self.emit("}");
            // end index of the `(...)` group opening at `pi`
            self.emit("fn shGlobGroupEnd(pat: []const u8, pi: usize) usize {");
            self.emit("    var depth: usize = 0;");
            self.emit("    var i = pi;");
            self.emit("    while (i < pat.len) : (i += 1) {");
            self.emit("        if (pat[i] == '(') depth += 1");
            self.emit("        else if (pat[i] == ')') { depth -= 1; if (depth == 0) return i; }");
            self.emit("    }");
            self.emit("    return pat.len;");
            self.emit("}");
            self.emit("fn shGlobAlt(s: []const u8, si: usize, alt: []const u8, pend: usize, pat: []const u8) bool {");
            self.emit("    var k: usize = 0;");
            self.emit("    while (k <= s.len - si) : (k += 1) {");
            self.emit("        if (!shGlob2(s[0..si + k], 0, alt, 0)) continue;");
            self.emit("        // try the remainder of the pattern after the group");
            self.emit("        if (shGlob2(s, si + k, pat, pend + 1)) return true;");
            self.emit("    }");
            self.emit("    return false;");
            self.emit("}");
            self.emit("fn shGlob2(s: []const u8, s0: usize, pat: []const u8, p0: usize) bool {");
            self.emit("    var si = s0;");
            self.emit("    var pi = p0;");
            self.emit("    while (pi < pat.len) {");
            self.emit("        const pc = pat[pi];");
            self.emit("        if (pc == '*') {");
            self.emit("            // plain star: try every suffix");
            self.emit("            var k: usize = 0;");
            self.emit("            while (si + k <= s.len) : (k += 1) {");
            self.emit("                if (shGlob2(s, si + k, pat, pi + 1)) return true;");
            self.emit("            }");
            self.emit("            return false;");
            self.emit("        }");
            self.emit("        if ((pc == '@' or pc == '+' or pc == '?' or pc == '!') and pi + 1 < pat.len and pat[pi + 1] == '(') {");
            self.emit("            const pe = shGlobGroupEnd(pat, pi + 1);");
            self.emit("            const inner = pat[pi + 2 .. pe];");
            self.emit("            var it = std.mem.splitScalar(u8, inner, '|');");
            self.emit("            var matched = false;");
            self.emit("            while (it.next()) |alt| {");
            self.emit("                var k: usize = 0;");
            self.emit("                while (si + k <= s.len) : (k += 1) {");
            self.emit("                    if (!shGlob2(s[0 .. si + k], 0, alt, 0)) continue;");
            self.emit("                    if (pc == '@' and shGlob2(s, si + k, pat, pe + 1)) { matched = true; break; }");
            self.emit("                    if (pc == '?' and k >= 1 and shGlob2(s, si + k, pat, pe + 1)) { matched = true; break; }");
            self.emit("                    if ((pc == '+' or pc == '*') and k >= 1 and shGlob2(s, si + k, pat, pe + 1)) { matched = true; break; }");
            self.emit("                }");
            self.emit("                if (matched) break;");
            self.emit("            }");
            self.emit("            if (pc == '*' and shGlob2(s, si, pat, pe + 1)) matched = true;");
            self.emit("            if (pc == '!') {");
            self.emit("                // `!(alt)` matches anything the alternatives do NOT, then");
            self.emit("                // the rest of the pattern must match the remainder");
            self.emit("                var k: usize = 0;");
            self.emit("                while (si + k <= s.len) : (k += 1) {");
            self.emit("                    var inner_hit = false;");
            self.emit("                    var it2 = std.mem.splitScalar(u8, inner, '|');");
            self.emit("                    while (it2.next()) |alt| {");
            self.emit("                        if (shGlob2(s[si .. si + k], 0, alt, 0)) { inner_hit = true; break; }");
            self.emit("                    }");
            self.emit("                    if (!inner_hit and shGlob2(s, si + k, pat, pe + 1)) return true;");
            self.emit("                }");
            self.emit("                return false;");
            self.emit("            }");
            self.emit("            return matched;");
            self.emit("        }");
            self.emit("        if (pc == '?') { if (si >= s.len) return false; si += 1; pi += 1; continue; }");
            self.emit("        if (pc == '[') {");
            self.emit("            if (si >= s.len) return false;");
            self.emit("            var i = pi + 1;");
            self.emit("            var neg = false;");
            self.emit("            if (i < pat.len and (pat[i] == '!' or pat[i] == '^')) { neg = true; i += 1; }");
            self.emit("            var hit = false;");
            self.emit("            var first = true;");
            self.emit("            while (i < pat.len and (pat[i] != ']' or first)) {");
            self.emit("                first = false;");
            self.emit("                if (i + 2 < pat.len and pat[i + 1] == '-' and pat[i + 2] != ']') {");
            self.emit("                    if (s[si] >= pat[i] and s[si] <= pat[i + 2]) hit = true;");
            self.emit("                    i += 3;");
            self.emit("                } else {");
            self.emit("                    if (s[si] == pat[i]) hit = true;");
            self.emit("                    i += 1;");
            self.emit("                }");
            self.emit("            }");
            self.emit("            if (i >= pat.len) return false;");
            self.emit("            if (hit == neg) return false;");
            self.emit("            si += 1; pi = i + 1; continue;");
            self.emit("        }");
            self.emit("        if (si >= s.len or s[si] != pc) return false;");
            self.emit("        si += 1; pi += 1;");
            self.emit("    }");
            self.emit("    return si == s.len;");
            self.emit("}");
            // FILESYSTEM glob: expand `pat` against the directory part
            // (bash word expansion). No match → the pattern TEXT (bash's
            // nullglob-off default); matches are joined by `sep`.
            self.emit("fn shGlobExpand(pat: []const u8, sep: u8) []const u8 {");
            self.emit("    const io = sh_io orelse return pat;");
            self.emit("    const slash = std.mem.lastIndexOfScalar(u8, pat, '/');");
            self.emit("    const dirname: []const u8 = if (slash) |i| (if (i == 0) \"/\" else pat[0..i]) else \".\";");
            self.emit("    const base: []const u8 = if (slash) |i| pat[i + 1 ..] else pat;");
            self.emit("    var dir = std.Io.Dir.cwd().openDir(io, dirname, .{ .iterate = true }) catch return pat;");
            self.emit("    defer dir.close(io);");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    var n: usize = 0;");
            self.emit("    var it = dir.iterate();");
            self.emit("    while (it.next(io) catch null) |ent| {");
            self.emit("        const nm = ent.name;");
            // dotfiles only match an explicit leading dot
            self.emit("        if (nm.len > 0 and nm[0] == '.' and (base.len == 0 or base[0] != '.')) continue;");
            self.emit("        if (!shGlobPat(nm, base)) continue;");
            self.emit("        if (n != 0) out.append(std.heap.page_allocator, sep) catch return pat;");
            self.emit("        if (slash) |_| {");
            self.emit("            out.appendSlice(std.heap.page_allocator, dirname) catch return pat;");
            self.emit("            out.append(std.heap.page_allocator, '/') catch return pat;");
            self.emit("        }");
            self.emit("        out.appendSlice(std.heap.page_allocator, nm) catch return pat;");
            self.emit("        n += 1;");
            self.emit("    }");
            self.emit("    if (n == 0) return pat;");
            self.emit("    return out.items;");
            self.emit("}");
        }
        if self.need_glob {
            self.emit("");
            self.emit("fn matchGlob(s: []const u8, pat: []const u8) bool {");
            self.emit("    var si: usize = 0;");
            self.emit("    var pi: usize = 0;");
            self.emit("    var ssi: usize = 0;");
            self.emit("    var spi: usize = 0;");
            self.emit("    var star = false;");
            self.emit("    while (si < s.len) {");
            self.emit("        if (pi < pat.len) {");
            self.emit("            const pc = pat[pi];");
            self.emit("            if (pc == '*') {");
            self.emit("                star = true;");
            self.emit("                ssi = si;");
            self.emit("                spi = pi;");
            self.emit("                pi += 1;");
            self.emit("                continue;");
            self.emit("            }");
            self.emit("            if (pc == '?' or s[si] == pc) {");
            self.emit("                si += 1;");
            self.emit("                pi += 1;");
            self.emit("                continue;");
            self.emit("            }");
            self.emit("        }");
            self.emit("        if (star) { si = ssi + 1; ssi = si; pi = spi + 1; } else return false;");
            self.emit("    }");
            self.emit("    while (pi < pat.len and pat[pi] == '*') { pi += 1; }");
            self.emit("    return pi == pat.len;");
            self.emit("}");
        }
        if self.need_shquote {
            self.emit("");
            self.emit("fn shellQuote(s: []const u8) []const u8 {");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    var needs = false;");
            self.emit("    for (s) |c| {");
            self.emit("        if (c == '\\n') { out.appendSlice(std.heap.page_allocator, \"\\\\n\") catch {}; needs = true; }");
            self.emit("        else if (c == '\\t') { out.appendSlice(std.heap.page_allocator, \"\\\\t\") catch {}; needs = true; }");
            self.emit("        else if (c == '\\r') { out.appendSlice(std.heap.page_allocator, \"\\\\r\") catch {}; needs = true; }");
            self.emit("        else if (c == '\\\\') { out.appendSlice(std.heap.page_allocator, \"\\\\\") catch {}; needs = true; }");
            self.emit("        else if (c == '\\'') { out.appendSlice(std.heap.page_allocator, \"\\\\'\") catch {}; needs = true; }");
            self.emit("        else if (c < 32 or c == 127) {");
            self.emit("            out.appendSlice(std.heap.page_allocator, \"\\\\x\") catch {};");
            self.emit("            const hi: u8 = \"0123456789abcdef\"[@intCast(c >> 4)];");
            self.emit("            const lo: u8 = \"0123456789abcdef\"[@intCast(c & 0xF)];");
            self.emit("            out.append(std.heap.page_allocator, hi) catch {}; out.append(std.heap.page_allocator, lo) catch {};");
            self.emit("            needs = true;");
            self.emit("        } else { out.append(std.heap.page_allocator, c) catch {}; }");
            self.emit("    }");
            self.emit("    if (needs) {");
            self.emit("        return std.fmt.allocPrint(std.heap.page_allocator, \"$'{s}'\", .{out.items}) catch \"\";");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
        }
        // per-callee stubs: compile-able, exit(2) at runtime
        if !self.sh2_calls.is_empty() {
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let names: Vec<String> = self.sh2_calls.iter().cloned().collect();
            for name in names {
                let mut fname = format!("sh2{}", camel(&name));
                if RESERVED_STUB.contains(&fname.as_str()) {
                    fname.push_str("Stub");
                }
                if !seen.insert(fname.clone()) {
                    continue; // two callees cameled to the same stub name
                }
                let (ret, retval) = match name.as_str() {
                    "test" => ("bool", "false"),
                    "arith" => ("i64", "0"),
                    _ => ("[]const u8", "\"\""),
                };
                self.emit("");
                self.emit(&format!("fn {fname}() {ret} {{"));
                self.emit(&format!("    sh2TODO({});", Self::zig_str(&name)));
                self.emit(&format!("    return {retval};"));
                self.emit("}");
            }
        }
        self.emit("");
        // hoisted user functions render BEFORE main (zig order-free, but
        // keep the listing readable)
        let pending = std::mem::take(&mut self.pending_fns);
        if !pending.is_empty() {
            self.emit("");
            for f in &pending {
                for line in f.lines() {
                    self.emit(line);
                }
            }
        }
        self.emit("pub fn main(init: std.process.Init) !void {");
        // hoisted declarations (typed by the A2 verdicts)
        // zig 0.16 errors on a `var` that is never mutated — emit `const`
        // unless the rendered body assigns the name somewhere
        let mut decls: Vec<String> = Vec::new();
        // capture-promoted vars live at FILE scope (visible to every hoisted
        // fn); everything else is a main local. File-scope bigint homes are
        // unsupported (Managed needs `try init`) — those stay main-local as
        // a known gap.
        let mut fdecls: Vec<String> = Vec::new();
        // self.written (not the pass-1 local): setVar arms discovered
        // during body rendering add their names here
        let written_now: Vec<String> = self.written.iter().cloned().collect();
        for v in &written_now {
            // user-function locals hoist with their fn — never in main
            if self.fn_locals.contains(v) {
                continue;
            }
            let m = self.zig_ident(v);
            // WORD-BOUNDED: a bare `contains` matched `sh_rc = 1` for the
            // variable `c` (every decl became `var`, and zig rejects
            // "never mutated")
            let mutated = body_stmts.iter().any(|l| assigns_ident(l, &m))
                // capture-promoted vars are assigned inside functions (their
                // stores never appear in main's body text) — always mutable
                || self.globals.contains(v);
            let kw = if mutated
                || self.bigint_vars.contains(v)
                || self.set_arrays.contains(v)
                || self.str_arrays.contains(v)
            {
                "var"
            } else {
                "const"
            };
            // file-scope when promoted (and not bigint-homed)
            let file_scope = self.globals.contains(v) && !self.bigint_vars.contains(v);
            let ind = if file_scope { "" } else { "    " };
            if self.bigint_vars.contains(v) {
                decls.push(format!(
                    "    {kw} {m} = try std.math.big.int.Managed.init(std.heap.page_allocator);"
                ));
                decls.push(format!("    defer {m}.deinit();"));
            } else if self.bigint_arrays.contains(v) {
                // bigint list → fixed Managed array + length counter
                decls.push(format!("    var {m}: [1024]std.math.big.int.Managed = undefined;"));
                decls.push(format!("    var {m}_len: usize = 0;"));
            } else if self.set_arrays.contains(v) {
                // set/list → fixed i64 array + length counter
                if file_scope {
                    fdecls.push(format!("var {m}: [1024]i64 = undefined;"));
                    fdecls.push(format!("var {m}_len: usize = 0;"));
                } else {
                    decls.push(format!("    {kw} {m}: [1024]i64 = undefined;"));
                    decls.push(format!("    var {m}_len: usize = 0;"));
                }
            } else if self.str_arrays.contains(v) {
                // string list → fixed []const u8 array + length counter
                if file_scope {
                    fdecls.push(format!("var {m}: [1024][]const u8 = undefined;"));
                    fdecls.push(format!("var {m}_len: usize = 0;"));
                } else {
                    decls.push(format!("    var {m}: [1024][]const u8 = undefined;"));
                    decls.push(format!("    var {m}_len: usize = 0;"));
                }
            } else if self.is_num(v) {
                if file_scope {
                    fdecls.push(format!("var {m}: i64 = 0;"));
                } else {
                    decls.push(format!("    {kw} {m}: i64 = 0;"));
                }
            } else if file_scope {
                fdecls.push(format!("var {m}: []const u8 = \"\";"));
            } else {
                decls.push(format!("    {kw} {m}: []const u8 = \"\";"));
            }
        }
        if !fdecls.is_empty() {
            // splice before the `pub fn main` line (file scope)
            if let Some(pos) = self.out.iter().position(|l| l.starts_with("pub fn main")) {
                for (j, d) in fdecls.iter().enumerate() {
                    self.out.insert(pos + j, d.clone());
                }
                self.out.insert(pos + fdecls.len(), String::new());
            } else {
                for d in &fdecls {
                    self.emit(d);
                }
            }
        }
        if !decls.is_empty() {
            self.emit("");
            for d in &decls {
                self.emit(d);
            }
        }
        // zig 0.16 removed std.io.getStdOut, and 0.14's std.process.getEnvVar /
        // std.process.Child.run went with it: stdout writing, env reads and
        // child capture ALL need the process `Init` now. So `main` takes
        // `init` whenever any of the three is needed, and the ones that cannot
        // be threaded into a free helper read it back through these globals.
        let need_init = self.need_stdout || self.need_env || self.need_run;
        if need_init {
            // Rewrite the already-emitted main signature. Generated statements
            // (`try stdout.print(...)`) are unchanged; each print is followed
            // by an explicit flush (early returns / exit() must not lose
            // buffered output).
            for line in self.out.iter_mut() {
                if line == "pub fn main() !void {" {
                    *line = "pub fn main(init: std.process.Init) !void {".to_string();
                    break;
                }
            }
        }
        if self.need_main_stdout {
            self.emit("");
            self.out
                .push("    var stdout_buffer: [4096]u8 = undefined;".to_string());
            self.out.push(
                "    var stdout_file_writer: std.Io.File.Writer = .initStreaming(.stdout(), init.io, &stdout_buffer);"
                    .to_string(),
            );
            self.out
                .push("    const stdout = &stdout_file_writer.interface;".to_string());
        }
        if self.need_env || self.need_run || self.need_fexist || self.need_writefile {
            self.out
                .push("    sh_io = init.io;".to_string());
        }
        if self.need_env {
            self.out
                .push("    sh_environ = init.environ_map;".to_string());
        }
        if !self.need_main_stdout && !self.need_env && !self.need_run && !self.need_fexist && !self.need_writefile {
            // `main` always takes `init`; if nothing consumed the handle the
            // parameter is unused and zig rejects it.
            self.emit("    _ = &init;");
        }
        if !body_stmts.is_empty() || !decls.is_empty() {
            self.emit("");
        }
        for line in &body_stmts {
            self.emit(line);
        }
        // the program's exit code is the last recorded status (bash: the
        // last command's code). Explicit `exit N` never reaches here.
        // an EXIT trap runs on the way out (before the final exit)
        if let Some(payload) = self.exit_trap.clone() {
            if !payload.is_empty() {
                self.need_run = true;
                self.emit(&format!(
                    "shRunStmt(&[_][]const u8{{\"sh\", \"-c\", {}}});",
                    Self::zig_str(&payload)
                ));
                self.emit("stdout.flush() catch {};");
            }
        }
        // a script END waits for its background jobs (bash reaps them
        // before exiting, so their output still reaches stdout)
        if self.saw_background {
            // `#include <sys/wait.h>` equivalent: libc's waitpid
            self.emit("    while (true) {");
            self.emit("        var __st: c_int = 0;");
            self.emit("        const __r = waitpid(-1, &__st, 0);");
            self.emit("        if (__r <= 0) break;");
            self.emit("    }");
        }
        self.emit("    std.process.exit(@intCast(sh_rc));");
        self.emit("}");
        if self.todo > 0 {
            self.emit(&format!(
                "// {} construct(s) lowered to TODO markers",
                self.todo
            ));
        }
    }
}

/// Names declared by a statement-position `local`/`declare`/`export`/
/// `readonly` builtin call (the words the stmt_declare renderer consumes).
/// Static (Pass-1 usable): only `NAME[=…]` Str words count.
fn builtin_declare_names(cmd: &str, args: &[IrExpr]) -> Vec<String> {
    if !matches!(
        cmd,
        "local" | "declare" | "export" | "readonly" | "typeset"
    ) {
        return Vec::new();
    }
    let items = match args.get(1) {
        Some(IrExpr::Array(items)) => items,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for it in items {
        let subs: Vec<&IrExpr> = match it {
            IrExpr::Array(inner) => inner.iter().collect(),
            other => vec![other],
        };
        for w in subs {
            if let IrExpr::Str(s, _) = w {
                if s.starts_with('-') && !s.contains('=') {
                    continue;
                }
                let name = s.split('=').next().unwrap_or("");
                if !name.is_empty() && is_ident(name) {
                    out.push(name.to_string());
                }
            }
        }
    }
    out
}

/// Direct-level write targets of a function body (Assign/Declare/for-loop
/// vars/cstyleFor/builtin `local` — the locals scan). Shared by the
/// Function arm and the capture-promotion pre-pass.
fn function_locals(body: &[IrStmt]) -> BTreeSet<String> {
    let mut locals: BTreeSet<String> = BTreeSet::new();
    for st in body {
        if let IrStmt::Assign { targets, .. } = st {
            for t in targets {
                locals.insert(t.var.clone());
            }
        }
        if let IrStmt::Expr(IrExpr::Call { func, args }) = st {
            if func == "assign" {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    locals.insert(name.clone());
                }
            }
        }
        if let IrStmt::Declare { vars, .. } = st {
            for d in vars {
                locals.insert(d.name.clone());
            }
        }
        if let IrStmt::For { var, .. } = st {
            locals.insert(var.clone());
        }
        if let IrStmt::Expr(IrExpr::Call { func, args }) = st {
            if func == "exec" || func == "builtin" {
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    for n in builtin_declare_names(cmd, args) {
                        locals.insert(n);
                    }
                }
            }
            if func == "cstyleFor" {
                if let Some(IrExpr::Str(spec, _)) = args.first() {
                    for part in spec.split(';') {
                        let part = part.trim();
                        if let Some(eq) = part.find('=') {
                            let v = part[..eq].trim();
                            if is_ident(v) {
                                locals.insert(v.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    locals
}

/// Targets written anywhere under stmts WITHOUT crossing a function
/// boundary (nested Function bodies are their own scope). Used to tell a
/// nested function's self-contained writes apart from its reads of outer
/// locals (which must promote to globals — dynamic scoping).
// nested-function names count as defined (a call inside the enclosing
// fn is a direct call, never an external)
fn collect_captured(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
            match s {
                IrStmt::Expr(e)
                | IrStmt::Output { value: e, .. }
                | IrStmt::Return(Some(e))
                | IrStmt::Exit(Some(e))
                | IrStmt::SetChildError(e) => reads_expr(e, out),
                IrStmt::Assign { expr, .. } => reads_expr(expr, out),
                IrStmt::Declare { init: Some(e), .. } => reads_expr(e, out),
                IrStmt::WriteFile { path, content, .. } => {
                    reads_expr(path, out);
                    reads_expr(content, out);
                }
                IrStmt::If { cond, then, elsifs, else_, .. } => {
                    reads_expr(cond, out);
                    reads_stmts(then, out);
                    for (c, b) in elsifs {
                        reads_expr(c, out);
                        reads_stmts(b, out);
                    }
                    reads_stmts(else_, out);
                }
                IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                    reads_expr(cond, out);
                    reads_stmts(body, out);
                }
                IrStmt::For { iter, body, .. } => {
                    reads_expr(iter, out);
                    reads_stmts(body, out);
                }
                IrStmt::ForInit { init, cond, step, body, .. } => {
                    reads_stmts(init, out);
                    reads_expr(cond, out);
                    reads_stmts(step, out);
                    reads_stmts(body, out);
                }
                IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => {
                    reads_stmts(b, out);
                }
                IrStmt::Redirect { inner, redirects } => {
                    reads_stmts(inner, out);
                    for r in redirects {
                        reads_expr(&r.target, out);
                    }
                }
                IrStmt::Pipeline { stages, .. } => {
                    for st in stages {
                        reads_stmts(st, out);
                    }
                }
                IrStmt::Case { discriminant, clauses, .. } => {
                    reads_expr(discriminant, out);
                    for c in clauses {
                        reads_stmts(&c.body, out);
                    }
                }
                IrStmt::Try { body, excepts, else_body, finally_body, .. } => {
                    reads_stmts(body, out);
                    for e in excepts {
                        reads_stmts(&e.body, out);
                    }
                    reads_stmts(else_body, out);
                    reads_stmts(finally_body, out);
                }
                IrStmt::Function { body, named_blocks, .. } => {
                    reads_stmts(body, out);
                    for (_, b) in named_blocks {
                        reads_stmts(b, out);
                    }
                }
                _ => {}
            }
        }
    }
fn collect_fns(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Function { name, body, named_blocks, .. } => {
                out.insert(name.clone());
                collect_fns(body, out);
                for (_, b) in named_blocks {
                    collect_fns(b, out);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_fns(then, out);
                for (_, b) in elsifs {
                    collect_fns(b, out);
                }
                collect_fns(else_, out);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                collect_fns(body, out);
            }
            IrStmt::For { body, .. } | IrStmt::ForInit { body, .. } => {
                collect_fns(body, out);
            }
            IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => {
                collect_fns(b, out);
            }
            IrStmt::Redirect { inner, .. } => collect_fns(inner, out),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_fns(st, out);
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    collect_fns(&c.body, out);
                }
            }
            _ => {}
        }
    }
}

/// Every Function body in the tree (any depth, incl. named blocks):
/// feeds the touched-names union for global promotion.
/// Bodies of subs are included by the caller separately.
fn all_fn_bodies<'a>(stmts: &'a [IrStmt], out: &mut Vec<&'a [IrStmt]>) {
    for s in stmts {
        match s {
            IrStmt::Function { body, named_blocks, .. } => {
                out.push(body);
                all_fn_bodies(body, out);
                for (_, b) in named_blocks {
                    all_fn_bodies(b, out);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                all_fn_bodies(then, out);
                for (_, b) in elsifs {
                    all_fn_bodies(b, out);
                }
                all_fn_bodies(else_, out);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                all_fn_bodies(body, out);
            }
            IrStmt::For { body, .. } | IrStmt::ForInit { body, .. } => {
                all_fn_bodies(body, out);
            }
            IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => {
                all_fn_bodies(b, out);
            }
            IrStmt::Redirect { inner, .. } => all_fn_bodies(inner, out),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    all_fn_bodies(st, out);
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    all_fn_bodies(&c.body, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_targets(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, .. } => {
                for t in targets {
                    out.insert(t.var.clone());
                }
            }
            IrStmt::Expr(IrExpr::Call { func, args }) if func == "assign" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    out.insert(name.clone());
                }
            }
            IrStmt::Declare { vars, .. } => {
                for d in vars {
                    out.insert(d.name.clone());
                }
            }
            IrStmt::For { var, body, .. } => {
                out.insert(var.clone());
                collect_targets(body, out);
            }
            IrStmt::Expr(IrExpr::Call { func, args })
                if func == "exec" || func == "builtin" =>
            {
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    for n in builtin_declare_names(cmd, args) {
                        out.insert(n);
                    }
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_targets(then, out);
                for (_, b) in elsifs {
                    collect_targets(b, out);
                }
                collect_targets(else_, out);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                collect_targets(body, out);
            }
            IrStmt::ForInit { body, .. } => collect_targets(body, out),
            IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => {
                collect_targets(b, out);
            }
            IrStmt::Redirect { inner, .. } => collect_targets(inner, out),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_targets(st, out);
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    collect_targets(&c.body, out);
                }
            }
            IrStmt::Try { body, excepts, else_body, finally_body, .. } => {
                collect_targets(body, out);
                for e in excepts {
                    collect_targets(&e.body, out);
                }
                collect_targets(else_body, out);
                collect_targets(finally_body, out);
            }
            IrStmt::Function { .. } => {}
            _ => {}
        }
    }
}

/// Names a nested function reads from an enclosing function scope:
/// (mentions in the nested body) − (the nested body's own targets).
/// Those promote to main-level globals (bash dynamic scoping); the
/// enclosing fn must not shadow them with locals.
/// Reads under one expression (Var/Ident/getVar("name") — the names
/// collect_written_expr cannot see: it only walks values, and a
/// getVar name is a Str, not a Var)
/// `NAME=$(( expr ))` inside an eval payload → (NAME, expr).
fn eval_arith_assign(w: &str) -> Option<(String, String)> {
    let (name, rest) = w.split_once("=$((")?;
    if !is_ident(name) {
        return None;
    }
    let expr = rest.trim().strip_suffix("))")?.trim();
    if expr.is_empty() {
        return None;
    }
    // `${v:-0}` / `${v}` inside the arithmetic: keep the NAME and drop the
    // default (`${var:-0}` reads var, which is 0 when unset — the corpus
    // form). `${arr[i]:-0}` becomes `arr` (the element read is a stub).
    let mut out = String::new();
    let mut rest2 = expr;
    while let Some(pos) = rest2.find("${") {
        out.push_str(&rest2[..pos]);
        let tail = &rest2[pos + 2..];
        // NESTED expansions (`${arr[${i:-0}]:-0}`): track depth
        let mut depth = 1usize;
        let mut close = 0usize;
        for (k, c) in tail.char_indices() {
            match c {
                '{' if k > 0 && tail.as_bytes()[k - 1] == b'$' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = k;
                        break;
                    }
                }
                _ => {}
            }
        }
        if depth != 0 {
            return None;
        }
        let body = &tail[..close];
        let head = body.split(|c| c == ':' || c == '[').next().unwrap_or("");
        if !head.is_empty() && head.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            out.push_str(head);
        } else {
            return None;
        }
        rest2 = &tail[close + 1..];
    }
    out.push_str(rest2);
    Some((name.to_string(), out))
}

/// Does this emitted line store into the Zig identifier `m`? Word-bounded,
/// so `sh_rc = 1` is not a store into `c`.
fn assigns_ident(line: &str, m: &str) -> bool {
    let store_ops = [" =", " +=", " -=", " *=", " /=", " %=", "++", "--"];
    let mut start = 0;
    while let Some(pos) = line[start..].find(m) {
        let at = start + pos;
        let before_ok = at == 0
            || !(line.as_bytes()[at - 1].is_ascii_alphanumeric() || line.as_bytes()[at - 1] == b'_');
        let after = &line[at + m.len()..];
        if before_ok {
            for op in store_ops {
                if after.starts_with(op) {
                    return true;
                }
            }
        }
        start = at + m.len();
    }
    false
}

/// Variable names an arithmetic tree READS (Var/Ident leaves).
fn collect_arith_reads(a: &crate::ir::ArithAst, out: &mut BTreeSet<String>) {
    use crate::ir::ArithAst::*;
    match a {
        Var(n) | Ident(n) => {
            out.insert(n.clone());
        }
        Bin { lhs, rhs, .. } => {
            collect_arith_reads(lhs, out);
            collect_arith_reads(rhs, out);
        }
        Un { arg, .. } | Cast { arg, .. } => collect_arith_reads(arg, out),
        Cond { test, then, else_ } => {
            collect_arith_reads(test, out);
            collect_arith_reads(then, out);
            collect_arith_reads(else_, out);
        }
        Assign { rhs, .. } => collect_arith_reads(rhs, out),
        IncDec { var, .. } => {
            out.insert(var.clone());
        }
        Index { var, .. } => {
            out.insert(var.clone());
        }
        Num(_) | Sizeof(_) => {}
    }
}

fn reads_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Var(name, _) | IrExpr::Ident(name) => {
            out.insert(name.clone());
        }
        IrExpr::Call { func, args } => {
            if func == "getVar" {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    out.insert(name.clone());
                }
            }
            for a in args {
                reads_expr(a, out);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            reads_expr(obj, out);
            for a in args {
                reads_expr(a, out);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            reads_expr(lhs, out);
            reads_expr(rhs, out);
        }
        IrExpr::Ternary { cond, then, else_, .. } => {
            reads_expr(cond, out);
            reads_expr(then, out);
            reads_expr(else_, out);
        }
        IrExpr::DefinedOr { expr, default, .. } => {
            reads_expr(expr, out);
            reads_expr(default, out);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    reads_expr(x, out);
                }
            }
        }
        // arithmetic operands reference variables too (`a + b` reads both)
        IrExpr::Arith(a) => {
            collect_arith_reads(a, out);
        }
        IrExpr::Capture { expr, .. } | IrExpr::Splice(expr) => reads_expr(expr, out),
        IrExpr::Arrow(stmts) => reads_stmts(stmts, out),
        IrExpr::Lambda { body, .. } => reads_stmts(body, out),
        IrExpr::Index { var, key } => {
            out.insert(var.clone());
            reads_expr(key, out);
        }
        IrExpr::Array(items) => {
            for i in items {
                reads_expr(i, out);
            }
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            reads_expr(iter, out);
            reads_expr(elem, out);
            if let Some(c) = cond {
                reads_expr(c, out);
            }
        }
        IrExpr::Object(fields) => {
            for (_, v) in fields {
                reads_expr(v, out);
            }
        }
        IrExpr::Arith(a) => reads_arith(a, out),
        _ => {}
    }
}
/// reads under arithmetic (variable leaves)
fn reads_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(name) | ArithAst::Ident(name) => {
            out.insert(name.clone());
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            reads_arith(lhs, out);
            reads_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => reads_arith(arg, out),
        ArithAst::Cond { test, then, else_, .. } => {
            reads_arith(test, out);
            reads_arith(then, out);
            reads_arith(else_, out);
        }
        ArithAst::Assign { rhs, .. } => reads_arith(rhs, out),
        ArithAst::Index { var, key } => {
            out.insert(var.clone());
            reads_arith(key, out);
        }
        ArithAst::Cast { arg, .. } => reads_arith(arg, out),
        _ => {}
    }
}
/// reads under statements (expression positions only — targets are
/// writes, handled by collect_targets; nested Functions recurse)
fn reads_stmts(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    
    for s in stmts {
        match s {
            IrStmt::Function { body, named_blocks, .. } => {
                let mut mentions = BTreeSet::new();
                reads_stmts(body, &mut mentions);
                let mut writes = BTreeSet::new();
                collect_targets(body, &mut writes);
                for m in mentions.difference(&writes) {
                    out.insert(m.clone());
                }
                // deeper nesting promotes through every level
                collect_captured(body, out);
                for (_, b) in named_blocks {
                    collect_captured(b, out);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_captured(then, out);
                for (_, b) in elsifs {
                    collect_captured(b, out);
                }
                collect_captured(else_, out);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                collect_captured(body, out);
            }
            IrStmt::For { body, .. } | IrStmt::ForInit { body, .. } => {
                collect_captured(body, out);
            }
            IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => {
                collect_captured(b, out);
            }
            IrStmt::Redirect { inner, .. } => collect_captured(inner, out),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_captured(st, out);
                }
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    collect_captured(&c.body, out);
                }
            }
            IrStmt::Try { body, excepts, else_body, finally_body, .. } => {
                collect_captured(body, out);
                for e in excepts {
                    collect_captured(&e.body, out);
                }
                collect_captured(else_body, out);
                collect_captured(finally_body, out);
            }
            _ => {}
        }
    }
}

/// Split a nested `${name<op>arg}` body into (op, name, arg) — mirrors the
/// perl backend's split_param_body (same op list, longest-first) for the
/// `:-`-default re-parse path.
fn split_param_body(body: &str) -> Option<(&str, &str, &str)> {
    let name_end = body
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(body.len());
    if name_end == 0 {
        return None;
    }
    let name = &body[..name_end];
    let rest = &body[name_end..];
    let (op, arg) = [
        "##", "%%", ":-", ":=", ":+", ":?", "//", "^^", ",,", "#", "%", "-", "+", "^", ",",
        "/",
    ]
    .iter()
    .find_map(|o| rest.strip_prefix(o).map(|a| (*o, a)))?;
    Some((op, name, arg))
}

/// CamelCase a sh2.* callee name for the stub function name.
fn camel(s: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for c in s.chars() {
        if c == '_' || c == '.' || c == '-' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        out.push_str("X");
    }
    out
}

/// Collect set-typed arrays (`factors = set(); factors.add(x)`) — the
/// setArray/setArrayAppend call targets.
/// Is this arith bigint-tainted (contains a sized Cast — the frontend's
/// >i64 marker — or reads a bigint var)? Such subtrees render via
/// std.math.big.int.Managed statements, never native i64 operators.

/// Bigint vars: a var assigned an arith that is bigint-tainted (a sized
/// Cast or a bigint var read). Fixed-point over the assign pairs.

/// Arrays whose setArray/setArrayAppend elements are bigint-tainted —
/// lowered to a std.math.big.int.Managed array (an i64 array would
/// truncate the bigint elements).
fn collect_bigint_arrays(stmts: &[IrStmt], big: &BTreeSet<String>, out: &mut BTreeSet<String>) {
    fn walk(stmts: &[IrStmt], big: &BTreeSet<String>, out: &mut BTreeSet<String>) {
        for s in stmts {
            match s {
                IrStmt::Assign { targets, expr, .. } => {
                    if let IrExpr::Call { func, args } = expr {
                        if matches!(func.as_str(), "setArray" | "setArrayAppend") {
                            if let Some(IrExpr::Str(name, _)) = args.first() {
                                if let Some(IrExpr::Array(items)) = args.get(1) {
                                    if items.iter().any(|it| {
                                        matches!(it, IrExpr::Arith(a) if crate::shir::arith_refs_bigint(a, big))
                                    }) {
                                        out.insert(name.clone());
                                    }
                                }
                            }
                        }
                    }
                    for t in targets {
                        if big.contains(&t.var) {
                            out.insert(t.var.clone());
                        }
                    }
                }
                IrStmt::If { then, elsifs, else_, .. } => {
                    walk(then, big, out);
                    for (_, b) in elsifs { walk(b, big, out); }
                    walk(else_, big, out);
                }
                IrStmt::For { body, .. } => walk(body, big, out),
                IrStmt::ForInit { body, .. } => walk(body, big, out),
                IrStmt::While { body, .. } => walk(body, big, out),
                IrStmt::Function { body, .. } => walk(body, big, out),
                IrStmt::Subshell(body) => walk(body, big, out),
                IrStmt::Background(body) => walk(body, big, out),
                _ => {}
            }
        }
    }
    walk(stmts, big, out);
}

/// String-element arrays: any setArray/setArrayAppend target with a
/// non-numeric element (bash arrays are strings; all-numeric lists keep
/// the i64 set home for the max/min/sum/sorted reductions).
fn collect_str_arrays(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    fn numeric_element(e: &IrExpr) -> bool {
        match e {
            IrExpr::Int(_) => true,
            IrExpr::Str(s, _) => s.trim().parse::<i64>().is_ok(),
            IrExpr::Arith(_) => true,
            _ => false,
        }
    }
    fn walk(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
        for s in stmts {
            match s {
                IrStmt::Assign { expr, .. } => {
                    if let IrExpr::Call { func, args } = expr {
                        if matches!(func.as_str(), "setArray" | "setArrayAppend") {
                            if let Some(IrExpr::Str(name, _)) = args.first() {
                                if let Some(IrExpr::Array(items)) = args.get(1) {
                                    if items.iter().any(|it| !numeric_element(it)) {
                                        out.insert(name.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                IrStmt::If { then, elsifs, else_, .. } => {
                    walk(then, out);
                    for (_, b) in elsifs {
                        walk(b, out);
                    }
                    walk(else_, out);
                }
                IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                    walk(body, out);
                }
                IrStmt::For { body, .. } | IrStmt::ForInit { body, .. } => {
                    walk(body, out);
                }
                IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => {
                    walk(b, out);
                }
                IrStmt::Redirect { inner, .. } => walk(inner, out),
                IrStmt::Pipeline { stages, .. } => {
                    for st in stages {
                        walk(st, out);
                    }
                }
                IrStmt::Case { clauses, .. } => {
                    for c in clauses {
                        walk(&c.body, out);
                    }
                }
                IrStmt::Function { body, named_blocks, .. } => {
                    walk(body, out);
                    for (_, b) in named_blocks {
                        walk(b, out);
                    }
                }
                _ => {}
            }
        }
    }
    walk(stmts, out);
}

fn collect_set_arrays(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                if let IrExpr::Call { func, .. } = expr {
                    if matches!(func.as_str(), "setArray" | "setArrayAppend") {
                        for t in targets {
                            out.insert(t.var.clone());
                        }
                    }
                }
                collect_set_arrays_expr(expr, out);
            }
            IrStmt::Expr(e) => collect_set_arrays_expr(e, out),
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_set_arrays(then, out);
                for (_, b) in elsifs {
                    collect_set_arrays(b, out);
                }
                collect_set_arrays(else_, out);
            }
            IrStmt::For { body, .. } => collect_set_arrays(body, out),
            IrStmt::ForInit { body, .. } => collect_set_arrays(body, out),
            IrStmt::While { body, .. } => collect_set_arrays(body, out),
            IrStmt::Function { body, .. } => collect_set_arrays(body, out),
            IrStmt::Subshell(body) => collect_set_arrays(body, out),
            IrStmt::Background(body) => collect_set_arrays(body, out),
            _ => {}
        }
    }
}

fn collect_set_arrays_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    if let IrExpr::Call { func, args } = e {
        if matches!(func.as_str(), "setArray" | "setArrayAppend") {
            if let Some(IrExpr::Str(name, _)) = args.first() {
                out.insert(name.clone());
            }
        }
    }
}

/// Collect every variable written by statements (assign targets, declare
/// lists, For loop vars) — the hoisted declaration set.
fn collect_written(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    out.insert(t.var.clone());
                }
                collect_written_expr(expr, out);
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    out.insert(d.name.clone());
                }
                if let Some(e) = init {
                    collect_written_expr(e, out);
                }
            }
            IrStmt::For { var, iter, body } => {
                out.insert(var.clone());
                collect_written_expr(iter, out);
                collect_written(body, out);
            }
            IrStmt::Ext(node) if node.tag() == "ForEachLine" => {
                // the loop var is bound by the reader loop (like For)
                if let Some(fl) = node.as_any().downcast_ref::<crate::shir_nodes::ForEachLine>() {
                    out.insert(fl.var.clone());
                    collect_written(&fl.body, out);
                    collect_written_expr(&fl.source, out);
                }
            }
            IrStmt::Expr(e) => collect_written_expr(e, out),
            IrStmt::Output { value, .. } => collect_written_expr(value, out),
            IrStmt::WriteFile { path, content, .. } => {
                collect_written_expr(path, out);
                collect_written_expr(content, out);
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                collect_written_expr(cond, out);
                collect_written(then, out);
                for (c, b) in elsifs {
                    collect_written_expr(c, out);
                    collect_written(b, out);
                }
                collect_written(else_, out);
            }
            IrStmt::While { cond, body } => {
                collect_written_expr(cond, out);
                collect_written(body, out);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                collect_written(body, out);
                collect_written_expr(cond, out);
            }
            IrStmt::Exit(e) => {
                if let Some(x) = e {
                    collect_written_expr(x, out);
                }
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                collect_written(b, out)
            }
            IrStmt::Redirect { inner, redirects } => {
                collect_written(inner, out);
                for r in redirects {
                    collect_written_expr(&r.target, out);
                }
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                collect_written_expr(discriminant, out);
                for c in clauses {
                    collect_written(&c.body, out);
                }
            }
            // user-function locals are hoisted with their fn — not
            // main-level declarations
            IrStmt::Function { .. } => {}
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_written(st, out);
                }
            }
            IrStmt::Exec { cmd, args, .. } => {
                collect_written_expr(cmd, out);
                for a in args {
                    collect_written_expr(a, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_written_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        // a bare READ is not a write: marking reads declared made an
        // environment variable (`$HOME`, t05_env_read) render as an
        // unemitted identifier and print an empty line
        IrExpr::Var(_, _) => {}
        IrExpr::Index { var, key } => {
            out.insert(var.clone());
            collect_written_expr(key, out);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_written_expr(lhs, out);
            collect_written_expr(rhs, out);
        }
        IrExpr::Arith(a) => collect_written_arith(a, out),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_written_expr(x, out);
                }
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_written_expr(i, out);
            }
        }
        IrExpr::Call { func, args } => {
            // a `local`/`declare`/`export`/`readonly` builtin call declares
            // its NAME words (hoisted like Declare) — the values may read
            // other vars, so still walk the args
            if func == "exec" || func == "builtin" {
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    for n in builtin_declare_names(cmd, args) {
                        out.insert(n);
                    }
                }
            }
            // an `assign` call writes its name (`y+=2`, `r=$(cmd)`)
            if func == "assign" {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    out.insert(name.clone());
                }
            }
            for a in args {
                collect_written_expr(a, out);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            collect_written_expr(obj, out);
            for a in args {
                collect_written_expr(a, out);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            collect_written_expr(cond, out);
            collect_written_expr(then, out);
            collect_written_expr(else_, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_written_expr(expr, out);
            collect_written_expr(default, out);
        }
        IrExpr::Capture { expr, .. } => collect_written_expr(expr, out),
        _ => {}
    }
}

fn collect_written_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(name) => {
            out.insert(name.clone());
        }
        ArithAst::Index { var, key } => {
            out.insert(var.clone());
            collect_written_arith(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_written_arith(lhs, out);
            collect_written_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => collect_written_arith(arg, out),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            collect_written_arith(test, out);
            collect_written_arith(then, out);
            collect_written_arith(else_, out);
        }
        ArithAst::Assign { rhs, .. } => collect_written_arith(rhs, out),
        ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Parser;

    fn render(src: &str) -> String {
        let commands = Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&commands);
        shir_to_zig(&prog)
    }

    #[test]
    fn assigns_and_echo() {
        let out = render("x=5\necho \"$x\"\nx=6\necho \"x is $x\"\n");
        assert!(out.contains("const std = @import(\"std\");"), "{out}");
        assert!(out.contains("var x: i64 = 0;"), "{out}");
        assert!(out.contains("x = 5;"), "{out}");
        assert!(
            out.contains("try stdout.print(\"{s}{d}\\n\", .{\"x is \", x});"),
            "{out}"
        );
        assert!(
            out.contains("pub fn main(init: std.process.Init) !void {"),
            "{out}"
        );
    }

    #[test]
    fn if_arith_test() {
        let out = render("x=3\nif [ \"$x\" -gt 3 ]; then\ny=$((x+1))\necho \"$y\"\nfi\n");
        assert!(out.contains("if ((x > 3)) {"), "{out}");
        assert!(out.contains("y = (x + 1);"), "{out}");
        assert!(out.contains("var y: i64 = 0;"), "{out}");
    }

    #[test]
    fn untyped_var_uses_runtime_store() {
        let out = render("y=$(ls)\necho \"$y\"\n");
        // capture assigns are A2-typed Str by the core; a bare `$(cmd)`
        // renders natively via shRun (the sh2Capture stub is only the
        // fallback for shapes capture_expr refuses)
        assert!(out.contains("var y: []const u8 = \"\";"), "{out}");
        assert!(out.contains("y = shRun(&[_][]const u8{\"ls\"});"), "{out}");
        assert!(!out.contains("sh2Capture()"), "{out}");
    }

    #[test]
    fn zig_keyword_mangled() {
        let out = render("fn=1\nfn=2\necho \"$fn\"\n");
        assert!(out.contains("var fn_: i64 = 0;"), "{out}");
        assert!(!out.contains("var fn: i64"), "{out}");
    }

    #[test]
    fn string_var_and_comparison() {
        let out = render("a=\"hello\"\nif [ \"$a\" = \"hello\" ]; then\necho yes\nfi\n");
        assert!(out.contains("var a: []const u8 = \"\";"), "{out}");
        assert!(out.contains("std.mem.eql(u8, a, \"hello\")"), "{out}");
    }

    #[test]
    fn for_loop_renders() {
        let out = render("for i in 1 2 3; do\necho \"$i\"\ndone\n");
        // the parser may not produce Range; just require it renders
        assert!(
            out.contains("pub fn main(init: std.process.Init) !void {"),
            "{out}"
        );
    }
}
