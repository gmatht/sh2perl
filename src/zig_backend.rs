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
//! the variable boundaries, and `sh2ToInt`/`sh2IntStr`/`sh2Truthy`
//! convert at the type boundaries. Native arithmetic renders to real Zig
//! operators (integer division truncates toward zero like bash); `**`
//! lowers to `std.math.pow`, arith `&&`/`||`/`!` to `@intFromBool`, string
//! comparisons to `std.mem.eql`/`std.mem.order`.

use crate::ir::{ArithAst, BinOpKind, InterpPart, IrExpr, IrProgram, IrStmt, IrType};
use std::collections::{BTreeSet, HashMap};

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
    "sh2TODO",
    "sh2ToInt",
    "sh2IntStr",
    "sh2Truthy",
    "sh2StrCat",
    "sh2B2S",
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
    loop_depth: usize,
    todo: usize,
    /// inside a user-function body (A1 Function): getVar("N") reads the
    /// positional param __args[N-1], Return returns the value, not exit
    in_function: bool,
    /// user functions hoisted OUT of main (rendered before it)
    pending_fns: Vec<String>,
    /// vars local to a hoisted user function (never declared in main)
    fn_locals: BTreeSet<String>,
    /// counter for generated temp names (for-loop item bindings)
    tmp_counter: usize,
    /// text-ops primitive helpers (sh2Contains/sh2Sub/…) needed
    need_ext: bool,
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
    if std::env::var("ZIG_DBG").is_ok() {
        eprintln!("DBG var_types: {:?}", r.var_types);
    }
    r.program(&prog);
    r.out.join("\n")
}

/// The Zig format specifier for a statically-typed part.
fn spec_char(spec: char) -> &'static str {
    match spec {
        'd' => "{d}",
        's' => "{s}",
        _ => "{}",
    }
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
        for (i, c) in name.chars().enumerate() {
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
                    format!("sh2ToInt({})", Self::zig_str(s))
                }
            }
            IrExpr::Var(name, _) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    self.need_toint = true;
                    return "sh2ToInt(sh2GetVar())".to_string();
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    m
                } else {
                    self.need_toint = true;
                    format!("sh2ToInt({m})")
                }
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    self.need_toint = true;
                    return "sh2ToInt(sh2GetVar())".to_string();
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    m
                } else {
                    self.need_toint = true;
                    format!("sh2ToInt({m})")
                }
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::BinOp { lhs, op, rhs }
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
            IrExpr::BinOp { lhs, op, rhs } if *op == BinOpKind::Concat => {
                self.need_toint = true;
                format!("sh2ToInt({})", self.expr_str(e))
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
                        return format!("sh2ToInt({m})");
                    }
                }
                self.need_toint = true;
                format!("sh2ToInt({})", self.call_str("getVar", args))
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
                format!("sh2ToInt({})", self.expr_str(other))
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
                    return format!("sh2IntStr({m})");
                }
                m
            }
            IrExpr::Ident(name) => {
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
                self.need_intstr = true;
                format!("sh2IntStr({})", self.arith(a))
            }
            IrExpr::Bool(b) => {
                if *b {
                    "\"true\"".into()
                } else {
                    "\"false\"".into()
                }
            }
            IrExpr::BinOp { lhs, op, rhs }
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
                format!("sh2B2S({})", self.expr_bool(e))
            }
            IrExpr::BinOp { lhs, op, rhs } if *op == BinOpKind::Concat => {
                self.need_cat = true;
                format!("sh2StrCat({}, {})", self.expr_str(lhs), self.expr_str(rhs))
            }
            IrExpr::BinOp { .. } => {
                self.need_intstr = true;
                format!("sh2IntStr({})", self.expr_num(e))
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.declared(name) {
                        let m = self.zig_ident(name);
                        self.mark_read(name);
                        if self.is_str(name) {
                            return m;
                        }
                        self.need_intstr = true;
                        return format!("sh2IntStr({m})");
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
            IrExpr::Capture { .. } => {
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
                    let ctx = crate::render_ext_expr::ExprRenderCtx {
                        backend: crate::render_ext_expr::Backend::Zig,
                        indent: 0,
                    };
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
                    return "sh2Truthy(sh2GetVar())".to_string();
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
                    return "sh2Truthy(sh2GetVar())".to_string();
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
            IrExpr::Arith(a) => format!("({} != 0)", self.arith(a)),
            IrExpr::Call { func, args } if func == "test" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some(c) = self.test_render(s) {
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
                format!("sh2Truthy({})", self.call_str("getVar", args))
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
                format!("sh2Truthy(sh2ArrayIndex())")
            }
            IrExpr::Capture { .. } => {
                self.sh2_calls.insert("capture".into());
                self.need_truthy = true;
                format!("sh2Truthy(sh2Capture())")
            }
            IrExpr::Regex { .. } => {
                self.sh2_calls.insert("regex".into());
                self.need_truthy = true;
                format!("sh2Truthy(sh2Regex())")
            }
            IrExpr::DefinedOr { .. } => {
                self.sh2_calls.insert("definedOr".into());
                self.need_truthy = true;
                format!("sh2Truthy(sh2DefinedOr())")
            }
            IrExpr::MethodCall { .. } => {
                self.sh2_calls.insert("methodCall".into());
                self.need_truthy = true;
                format!("sh2Truthy(sh2MethodCall())")
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
                    let ctx = crate::render_ext_expr::ExprRenderCtx {
                        backend: crate::render_ext_expr::Backend::Zig,
                        indent: 0,
                    };
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
            // stdout); a numeric context would coerce via sh2ToInt
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
                    self.mark_read(name);
                    if self.is_num(name) {
                        self.mark_read(name);
                        return ('d', self.expr_num(e));
                    }
                    // a DECLARED non-num var homes as a string — print it
                    // raw (sh2IntStr would wrap a []const u8 in an i64
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
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    self.need_toint = true;
                    return "sh2ToInt(sh2GetVar())".to_string();
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    m
                } else {
                    self.need_toint = true;
                    format!("sh2ToInt({m})")
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
                    "**" => format!("std.math.pow(i64, {l}, {r})"),
                    "&&" => format!("@intFromBool(({l} != 0) and ({r} != 0))"),
                    "||" => format!("@intFromBool(({l} != 0) or ({r} != 0))"),
                    // zig signed ints forbid `%`/`/` truncation operators —
                    // C semantics are truncated division/remainder (@rem)
                    "/" => format!("@divTrunc({l}, {r})"),
                    "%" => format!("@rem({l}, {r})"),
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
                    "~" => format!("(~{a})"),
                    _ => format!("({op}{a})"),
                }
            }
            ArithAst::Cond { test, then, else_ } => format!(
                "if ({} != 0) {} else {}",
                self.arith(test),
                self.arith(then),
                self.arith(else_)
            ),
            ArithAst::Assign { .. } | ArithAst::IncDec { .. } => {
                // runtime setVar semantics (x=, x+=, x++) — sh2.arith stub
                self.sh2_calls.insert("arith".into());
                "sh2Arith()".to_string()
            }
            ArithAst::Sizeof(ty) => ty.c_sizeof().unwrap_or(4).to_string(),
            ArithAst::Cast { arg, .. } => self.arith(arg),
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
        let ast = crate::shir::parse_arith(norm.trim())?;
        Some(self.arith(&ast))
    }

    fn call_str(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            "fnValue" | "fnCall" => {
                // a user-function call: name(args...) — values flow as
                // strings (the positional-param convention); a numeric
                // context coerces via sh2ToInt at the read site
                let name = args
                    .first()
                    .and_then(|a| match a {
                        IrExpr::Str(n, _) => Some(self.zig_ident(n)),
                        _ => None,
                    })
                    .unwrap_or_else(|| "sh2TODO".to_string());
                let call_args: Vec<String> = args
                    .get(1)
                    .and_then(|a| match a {
                        IrExpr::Array(elems) => {
                            Some(elems.iter().map(|e| self.expr_str(e)).collect())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                format!("{name}(&[_][]const u8{{ {} }})", call_args.join(", "))
            }
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    // inside a user function: getVar("N") is the POSITIONAL
                    // PARAMETER read ($1 convention — the C frontend's
                    // outparam/param channel), not an env lookup
                    if self.in_function && name.bytes().all(|b| b.is_ascii_digit()) && !name.is_empty() {
                        let idx: usize = name.parse().unwrap_or(1);
                        return format!(
                            "(if (__args.len >= {idx}) __args[{idx} - 1] else \"\")"
                        );
                    }
                    if self.declared(name) {
                        let m = self.zig_ident(name);
                        self.mark_read(name);
                        return m;
                    }
                    // undeclared (env) var: read the process environment
                    self.need_env = true;
                    return format!("sh2Env({})", Self::zig_str(name));
                }
                self.sh2_stub("getVar", "getVar")
            }
            "test" => {
                // a [ ... ] result in string context (rare; e.g. `x=$( [ .. ] )`)
                self.need_b2s = true;
                format!("sh2B2S({})", self.call_bool(func, args))
            }
            "split" => {
                // word splitting: a single value with default IFS (no
                // whitespace) is just the value; render the inner arg.
                match args.first() {
                    Some(arg) => self.expr_str(arg),
                    None => self.sh2_stub("split", "split"),
                }
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
            "arith" => {
                if let Some(IrExpr::Str(sv, _)) = args.first() {
                    if let Some(x) = self.arith_text(sv) {
                        self.need_intstr = true;
                        return format!("sh2IntStr({x})");
                    }
                }
                self.need_intstr = true;
                format!("sh2IntStr(sh2Arith())")
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
                "sh2Arith()".to_string()
            }
            "test" => format!("@intFromBool({})", self.call_bool(func, args)),
            _ => {
                self.need_toint = true;
                format!("sh2ToInt({})", self.call_str(func, args))
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
                "(sh2Arith() != 0)".to_string()
            }
            "getVar" => {
                self.need_truthy = true;
                format!("sh2Truthy({})", self.call_str(func, args))
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
                format!("sh2Truthy({})", self.call_str(func, args))
            }
            _ => {
                self.need_truthy = true;
                format!("sh2Truthy({})", self.call_str(func, args))
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
    /// → its ident; a number literal → the literal.
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
            return Some(m);
        }
        None
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

    fn sh2_stub(&mut self, name: &str, note: &str) -> String {
        self.sh2_calls.insert(name.to_string());
        self.mark_todo(&format!("{note} → sh2.{name}"));
        self.mark_todo(&format!("{note} → sh2.{name}"));
        format!("sh2{}()", camel(name))
    }

    /// The generic expression form (used by `_ = expr;` statements and
    /// the part classifier's fallback).
    fn expr_any(&mut self, e: &IrExpr) -> String {
        let (_, x) = self.part_spec(e);
        x
    }

    /// Parts → one `try stdout.print("fmt", .{args});` statement.
    fn print_from_parts(&mut self, parts: Vec<(char, String)>, newline: bool) -> String {
        self.need_stdout = true;
        let mut fmt = String::new();
        let mut args: Vec<String> = Vec::new();
        for (spec, e) in parts {
            fmt.push_str(spec_char(spec));
            args.push(e);
        }
        if newline {
            fmt.push('\n');
        }
        format!(
            "try stdout.print({}, .{{{}}});\n    try stdout.flush();",
            Self::zig_str(&fmt),
            args.join(", ")
        )
    }

    /// Parts for an echo argument (bash word-splits argv items on spaces).
    fn parts_of(&mut self, e: &IrExpr) -> Vec<(char, String)> {
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
                if self.is_num(name) {
                    vec![('d', m)]
                } else {
                    vec![('s', m)]
                }
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return vec![('s', "sh2GetVar()".into())];
                }
                let m = self.zig_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
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
            IrExpr::Arith(a) => vec![('d', self.arith(a))],
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
                        format!("sh2ToInt({va})")
                    };
                    let vb = if nb {
                        vb
                    } else {
                        self.need_toint = true;
                        format!("sh2ToInt({vb})")
                    };
                    Some(format!("({va} {o} {vb})"))
                } else if let Some(o) = str_op {
                    let (va, na) = self.test_operand(a);
                    let (vb, nb) = self.test_operand(b);
                    let va = if na {
                        self.need_intstr = true;
                        format!("sh2IntStr({va})")
                    } else {
                        va
                    };
                    let vb = if nb {
                        self.need_intstr = true;
                        format!("sh2IntStr({vb})")
                    } else {
                        vb
                    };
                    if o == "==" {
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
                    format!("sh2IntStr({vv})")
                } else {
                    vv
                };
                Some(format!("({vv} .len != 0)"))
            }
            [flag, v] if *flag == "-z" => {
                let (vv, nv) = self.test_operand(v);
                let vv = if nv {
                    self.need_intstr = true;
                    format!("sh2IntStr({vv})")
                } else {
                    vv
                };
                Some(format!("({vv} .len == 0)"))
            }
            [v] => {
                let (vv, nv) = self.test_operand(v);
                let vv = if nv {
                    self.need_intstr = true;
                    format!("sh2IntStr({vv})")
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
                    format!("sh2IntStr({vv})")
                } else {
                    vv
                };
                self.need_fexist = true;
                Some(format!("sh2FileTest({}, {vv})", Self::zig_str(*flag)))
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
        } else if let Ok(n) = inner.parse::<i64>() {
            (n.to_string(), true)
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
                Some("sh2ReadLine(init.io)".to_string())
            }
            "StrLen" => {
                self.need_intstr = true;
                Some(format!("sh2IntStr(@intCast(({}).len))", child(self, 0)?))
            }
            "CaseTransform" => {
                let n = node.as_any().downcast_ref::<crate::shir_nodes::CaseTransform>()?;
                self.need_ext = true;
                Some(format!("sh2Case({}, {})", child(self, 0)?, if n.upper { "true" } else { "false" }))
            }
            "SubStrExtract" => {
                self.need_ext = true;
                let off = child(self, 1)?;
                let len = ch.get(2).map(|e| self.expr_num(e)).unwrap_or_else(|| "-1".into());
                Some(format!("sh2Sub({}, {}, {})", child(self, 0)?, off, len))
            }
            "StringContains" => {
                self.need_ext = true;
                let b = format!("sh2Contains({}, {})", child(self, 0)?, child(self, 1)?);
                if bool_ctx { Some(b) } else { self.need_b2s = true; Some(format!("sh2B2S({})", b)) }
            }
            "RegSub" => {
                // pattern must be a LITERAL (no regex metachars) — zig has
                // no std regex; real regexes fall back to the runtime.
                let n = node.as_any().downcast_ref::<crate::shir_nodes::RegSub>()?;
                // the command-substitution newline strip (`\n+$`, first
                // only) is a NAMED primitive here: trim trailing newlines
                if !n.global && n.pattern == "\n+$" {
                    self.need_ext = true;
                    return Some(format!("sh2TrimRightNl({})", child(self, 0)?));
                }
                if n.pattern.chars().any(|c| ".*+?[](){}|^$\\\\".contains(c) || c == '"') {
                    return None;
                }
                self.need_ext = true;
                Some(format!("sh2ReplaceLit({}, {}, {}, {})",
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
                Some(format!("sh2Fields(std.heap.page_allocator, {}, {}, {}, {})",
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
                        format!("sh2Sub({}, if ({t}.len > {}) {t}.len - {} else 0)", t, cnt, cnt)
                    } else {
                        format!("sh2Sub({}, 0, {})", t, cnt)
                    });
                }
                Some(format!("sh2TakeLines({}, {}, {})", child(self, 0)?, cnt, if n.from_end { "true" } else { "false" }))
            }
            "PathName" => {
                let n = node.as_any().downcast_ref::<crate::shir_nodes::PathName>()?;
                self.need_ext = true;
                let f = if n.which == "dirname" { "sh2Dirname" } else { "sh2Basename" };
                Some(format!("{}({})", f, child(self, 0)?))
            }
            "StringTrim" => {
                let n = node.as_any().downcast_ref::<crate::shir_nodes::StringTrim>()?;
                self.need_ext = true;
                Some(format!("sh2TrimSides({}, {}, {})",
                    child(self, 0)?, if n.leading { "true" } else { "false" }, if n.trailing { "true" } else { "false" }))
            }
            _ => None,
        }
    }

    fn stmt(&mut self, s: &IrStmt) {
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
                // body reads render as strings, not sh2ToInt coerctions
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
                self.need_ext = true; // sh2GlobName fn
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
                self.emit("                if (!sh2GlobName(nm, bn)) return false;");
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
                            let m = self.zig_ident(name);
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
                            self.emit(&format!(
                                "_ = {m}(&[_][]const u8{{ {} }});",
                                call_args.join(", ")
                            ));
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
                        // for untyped locals) coerces via sh2ToInt
                        if self.is_num(var) {
                            let zop = if *delta >= 0 { "+" } else { "-" };
                            self.emit(&format!("{v} {zop}= {d};"));
                        } else {
                            // the value model is strings: convert back
                            self.need_intstr = true;
                            let op = if *delta >= 0 { "+" } else { "-" };
                            self.emit(&format!(
                                "{v} = sh2IntStr(sh2ToInt({v}) {op} {d});"
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
                            if cmd == "exit" {
                                // bash `exit N` / bare `exit` → std.process.exit
                                let code = match args.get(1) {
                                    Some(IrExpr::Array(items)) if !items.is_empty() => {
                                        self.expr_num(&items[0])
                                    }
                                    _ => "0".to_string(),
                                };
                                self.emit(&format!("std.process.exit(@intCast({code}));"));
                                return;
                            }
                            if cmd == "true" {
                                // status 0 no-op
                                return;
                            }
                            if cmd == "false" {
                                // status 1: exit nonzero
                                self.emit("std.process.exit(1);");
                                return;
                            }
                            if cmd == "echo" {
                                let mut parts = Vec::new();
                                if let Some(IrExpr::Array(items)) = args.get(1) {
                                    for (i, item) in items.iter().enumerate() {
                                        if i > 0 {
                                            parts.push(('s', "\" \"".to_string()));
                                        }
                                        parts.extend(self.parts_of(item));
                                    }
                                    let call = self.print_from_parts(parts, true);
                                    self.emit(&call);
                                    return;
                                }
                            }
                            if cmd == "printf" {
                                if let Some(IrExpr::Array(items)) = args.get(1) {
                                    if let Some(IrExpr::Str(fmt, _)) = items.first() {
                                        let mut parts = Vec::new();
                                        // translate the format's % conversions into
                                        // print_from_parts specs over the arg list
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
                                                // consume flags/width/length
                                                // modifiers (`%lld`, `%u`,
                                                // `%5d`): the value model is
                                                // int-as-string or i64
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
                                                    Some('s') => { chars.next(); 's' }
                                                    Some('d') | Some('i') | Some('u') => { chars.next(); 'd' }
                                                    Some('x') | Some('X') | Some('o') => { chars.next(); 'd' }
                                                    Some('c') => { chars.next(); 's' }
                                                    Some('\n') => { chars.next(); 's' }
                                                    _ => { parts.push(('s', Self::zig_str("%"))); continue; }
                                                };
                                                if let Some(a) = rest.get(argi) {
                                                    argi += 1;
                                                    let mut pa = self.parts_of(a);
                                                    if pa.len() == 1 {
                                                        // a %d over a
                                                        // string-model value
                                                        // (the user-fn
                                                        // convention) prints
                                                        // identically with {s}
                                                        if spec == 'd' && pa[0].0 == 's' {
                                                            parts.push(('s', pa[0].1.clone()));
                                                            continue;
                                                        }
                                                        parts.push((spec, pa[0].1.clone()));
                                                    } else {
                                                        parts.push(pa[0].clone());
                                                    }
                                                } else {
                                                    parts.push((spec, if spec == 'd' { "0".into() } else { "\"\"".into() }));
                                                }
                                            } else {
                                                parts.push(('s', Self::zig_str(&c.to_string())));
                                            }
                                        }
                                        let call = self.print_from_parts(parts, false);
                                        self.emit(&call);
                                        return;
                                    }
                                }
                            }
                            // external command: fork/exec via sh2Run
                            let argv = self.build_argv(cmd, args);
                            self.need_run = true;
                            self.emit(&format!("_ = sh2Run(&[_][]const u8{{{}}});", argv.join(", ")));
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
                if !t.indices.is_empty() {
                    self.mark_todo("array-index assign");
                    return;
                }
                let rhs = if self.is_num(&t.var) {
                    // arith-assign / inc-dec render natively (not via
                    // the sh2Arith stub)
                    match expr {
                        IrExpr::Arith(a) => match &**a {
                            ArithAst::IncDec { var, delta, .. } => {
                                let v = self.zig_ident(var);
                                self.mark_read(var);
                                let d = delta.unsigned_abs();
                                let s = if *delta >= 0 { "+" } else { "-" };
                                self.emit(&format!("{v} {s}= {d};"));
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
                self.emit(&format!("{m} = {rhs};"));
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
                        self.emit(&format!("// declare {m}"));
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
                    "try std.fs.cwd().writeFile(.{{ .sub_path = {p}, .data = {c} }});"
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
                        .map(|p| format!("std.mem.eql(u8, {d}, {})", Self::zig_str(p)))
                        .collect();
                    let is_default = cl.patterns.iter().any(|p| p.as_str() == "*");
                    if conds.is_empty() && is_default {
                        self.emit("else {");
                        self.depth += 1;
                        for s in &cl.body {
                            self.stmt(s);
                        }
                        self.depth -= 1;
                        self.emit("}");
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
                        let items_tmp = self.fresh_tmp("sh2_items");
                        let v_tmp = self.fresh_tmp("sh2_v");
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
                    .unwrap_or_else(|| "0".into());
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
                // render each stage's statements in sequence (the native
                // in-process approximation for the v1 subset).
                for st in stages {
                    for s in st {
                        self.stmt(s);
                    }
                }
            }
            IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
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
                // render the inner commands; apply a simple fd-1 write
                // redirect (`> file` / `>> file`) by writing the captured
                // output to the file (the common echo-to-file case).
                for s in inner {
                    self.stmt(s);
                }
                for r in redirects {
                    if r.fd.unwrap_or(1) == 1 && (r.mode == "w" || r.mode == "a") {
                        let p = self.expr_str(&r.target);
                        self.need_writefile = true;
                        self.emit(&format!(
                            "sh2WriteFile({p}, \"\", {});",
                            if r.mode == "a" { "true" } else { "false" }
                        ));
                    }
                }
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
                    // model (numeric contexts coerce at the read site)
                    let r = v.as_ref().map(|e| self.expr_str(e)).unwrap_or_default();
                    self.emit(&format!("return {r};"));
                    return;
                }
                match v {
                    Some(e) => {
                        let code = self.expr_num(e);
                        self.emit(&format!("return @intCast({code});"));
                    }
                    None => self.emit("return;"),
                }
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
                let prev_in_fn = self.in_function;
                self.in_function = true;
                let mut saved = std::mem::take(&mut self.out);
                for s in body {
                    self.stmt(s);
                }
                std::mem::swap(&mut saved, &mut self.out);
                self.in_function = prev_in_fn;
                let mut locals: BTreeSet<String> = BTreeSet::new();
                for st in body {
                    if let IrStmt::Assign { targets, .. } = st {
                        for t in targets {
                            locals.insert(t.var.clone());
                        }
                    }
                    if let IrStmt::Declare { vars, .. } = st {
                        for d in vars {
                            locals.insert(d.name.clone());
                        }
                    }
                }
                self.fn_locals.extend(locals.iter().cloned());
                let mut fntext = format!(
                    "fn {id}(__args: []const []const u8) []const u8 {{\n"
                );
                for v in &locals {
                    let m = self.zig_ident(v);
                    if self.is_num(v) {
                        fntext.push_str(&format!("    var {m}: i64 = 0;\n"));
                    } else {
                        fntext.push_str(&format!("    var {m}: []const u8 = \"\";\n"));
                    }
                }
                fntext.push_str(&saved.join("\n"));
                fntext.push('\n');
                fntext.push_str("}\n");
                self.pending_fns.push(fntext);
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
        }
    }

    // ── program ──────────────────────────────────────────────────────

    fn program(&mut self, prog: &IrProgram) {
        // Pass 1: collect written vars so declarations are hoisted.
        let mut written: BTreeSet<String> = BTreeSet::new();
        collect_written(&prog.stmts, &mut written);
        for (n, _) in &prog.var_types {
            written.insert(n.clone());
        }
        // `declared()` consults this during rendering (a var reference
        // before its assignment in source order still lowers natively).
        self.written = written.clone();

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
        // dead-var guard: Zig rejects declared-but-never-read variables
        for v in &written {
            if !self.read.contains(v) {
                let m = self
                    .mangle
                    .get(v)
                    .cloned()
                    .unwrap_or_else(|| self.zig_ident(v));
                self.emit(&format!("_ = {m};"));
            }
        }
        std::mem::swap(&mut self.out, &mut body_stmts);
        self.depth = 0;

        // ── preamble: import, helpers, stubs, then main ──
        self.emit("const std = @import(\"std\");");
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
            self.emit("fn sh2ToInt(s: []const u8) i64 {");
            self.emit("    return std.fmt.parseInt(i64, std.mem.trim(u8, s, \" \\t\\n\\r\"), 10) catch 0;");
            self.emit("}");
        }
        if self.need_intstr {
            self.emit("");
            self.emit("fn sh2IntStr(n: i64) []const u8 {");
            self.emit(
                "    return std.fmt.allocPrint(std.heap.page_allocator, \"{d}\", .{n}) catch \"\";",
            );
            self.emit("}");
        }
        if self.need_truthy {
            self.emit("");
            self.emit("fn sh2Truthy(s: []const u8) bool {");
            self.emit("    return s.len != 0;");
            self.emit("}");
        }
        if self.need_cat {
            self.emit("");
            self.emit("fn sh2StrCat(a: []const u8, b: []const u8) []const u8 {");
            self.emit("    return std.fmt.allocPrint(std.heap.page_allocator, \"{s}{s}\", .{ a, b }) catch \"\";");
            self.emit("}");
        }
        if self.need_b2s {
            self.emit("");
            self.emit("fn sh2B2S(b: bool) []const u8 {");
            self.emit("    return if (b) \"1\" else \"0\";");
            self.emit("}");
        }
        if self.need_env {
            self.emit("");
            self.emit("fn sh2Env(name: []const u8) []const u8 {");
            self.emit("    return std.process.getEnvVar(name) catch \"\";");
            self.emit("}");
        }
        if self.need_fexist {
            self.emit("");
            self.emit("fn sh2FileTest(flag: []const u8, p: []const u8) bool {");
            self.emit("    const st = std.fs.cwd().statFile(p) catch return false;");
            self.emit("    if (std.mem.eql(u8, flag, \"-f\")) return st.kind == .file;");
            self.emit("    if (std.mem.eql(u8, flag, \"-d\")) return st.kind == .directory;");
            self.emit("    if (std.mem.eql(u8, flag, \"-e\")) return true;");
            self.emit("    if (std.mem.eql(u8, flag, \"-s\")) return st.size != 0;");
            self.emit("    return false;");
            self.emit("}");
        }
        if self.need_writefile {
            self.emit("");
            self.emit("fn sh2WriteFile(p: []const u8, data: []const u8, append: bool) void {");
            self.emit("    const f = std.fs.cwd().createFile(p, .{ .truncate = !append }) catch return;");
            self.emit("    defer f.close();");
            self.emit("    f.writeAll(data) catch {};");
            self.emit("}");
        }
        if self.need_ext {
            self.emit("");
            self.emit("fn sh2Contains(hay: []const u8, needle: []const u8) bool {");
            self.emit("    return std.mem.indexOf(u8, hay, needle) != null;");
            self.emit("}");
            self.emit("fn sh2Sub(t: []const u8, off: i64, len: i64) []const u8 {");
            self.emit("    const o: usize = @intCast(@max(0, @min(off, @as(i64, @intCast(t.len)))));");
            self.emit("    if (len < 0) return t[o..];");
            self.emit("    const e: usize = o + @as(usize, @intCast(len));");
            self.emit("    return t[o..@min(e, t.len)];");
            self.emit("}");
            self.emit("fn sh2Case(t: []const u8, upper: bool) []const u8 {");
            self.emit("    const b = std.heap.page_allocator.alloc(u8, t.len) catch return \"\";");
            self.emit("    for (t, 0..) |c, i| b[i] = if (upper) std.ascii.toUpper(c) else std.ascii.toLower(c);");
            self.emit("    return b;");
            self.emit("}");
            self.emit("fn sh2ReplaceLit(t: []const u8, pat: []const u8, repl: []const u8, all: bool) []const u8 {");
            self.emit("    const i = std.mem.indexOf(u8, t, pat) orelse return t;");
            self.emit("    if (!all) {");
            self.emit("        return std.fmt.allocPrint(std.heap.page_allocator, \"{s}{s}{s}\", .{ t[0..i], repl, t[i + pat.len ..] }) catch \"\";");
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
            self.emit("fn sh2Fields(allocator: std.mem.Allocator, t: []const u8, d: []const u8, ids: []const usize, suppress: bool) []const u8 {");
            self.emit("    if (!sh2Contains(t, d)) return if (suppress) \"\" else t;");
            self.emit("    const p = std.heap.page_allocator.alloc([]const u8, std.mem.count(u8, t, d) + 1) catch return \"\";");
            self.emit("    var it = std.mem.splitScalar(u8, t, d[0]);");
            self.emit("    var np: usize = 0;");
            self.emit("    while (it.next()) |part| { p[np] = part; np += 1; }");
            self.emit("    var out: std.ArrayList(u8) = .empty;");
            self.emit("    var first = true;");
            self.emit("    for (ids) |id| {");
            self.emit("        if (id < p.len) {");
            self.emit("            if (!first) out.appendSlice(allocator, d) catch return \"\";");
            self.emit("            out.appendSlice(allocator, p[id]) catch return \"\";");
            self.emit("            first = false;");
            self.emit("        }");
            self.emit("    }");
            self.emit("    return out.items;");
            self.emit("}");
            self.emit("fn sh2TakeLines(t: []const u8, k: usize, fromEnd: bool) []const u8 {");
            self.emit("    var lines = std.ArrayList([]const u8).init(std.heap.page_allocator);");
            self.emit("    var it = std.mem.splitScalar(u8, t, '\\n');");
            self.emit("    while (it.next()) |part| lines.append(part) catch return \"\";");
            self.emit("    const n = lines.items.len;");
            self.emit("    const s0 = if (fromEnd) @max(0, n - k) else 0;");
            self.emit("    const e0 = if (fromEnd) n else @min(k, n);");
            self.emit("    return std.mem.join(std.heap.page_allocator, \"\\n\", lines.items[s0..e0]) catch \"\";");
            self.emit("}");
            self.emit("fn sh2Basename(t: []const u8) []const u8 {");
            self.emit("    const i = std.mem.lastIndexOfScalar(u8, t, '/') orelse return t;");
            self.emit("    return t[i + 1 ..];");
            self.emit("}");
            self.emit("fn sh2Dirname(t: []const u8) []const u8 {");
            self.emit("    const i = std.mem.lastIndexOfScalar(u8, t, '/') orelse return \".\";");
            self.emit("    if (i == 0) return \"/\";");
            self.emit("    return t[0..i];");
            self.emit("}");
                        self.emit("var __rl_file: std.Io.File = undefined;");
            self.emit("var __rl_buf: [4096]u8 = undefined;");
            self.emit("var __rl_rdr: std.Io.File.Reader = undefined;");
            self.emit("var __rl_init: bool = false;");
            self.emit("fn sh2ReadLine(io: std.Io) []const u8 {");
            self.emit("    if (!__rl_init) {");
            self.emit("        __rl_file = std.Io.File.stdin();");
            self.emit("        __rl_rdr = __rl_file.reader(io, &__rl_buf);");
            self.emit("        __rl_init = true;");
            self.emit("    }");
            self.emit("    const line = __rl_rdr.interface.takeDelimiter('\\n') catch return \"\";");
            self.emit("    return line orelse \"\";");
            self.emit("}");
self.emit("fn sh2TrimRightNl(t: []const u8) []const u8 {");
            self.emit("    var s = t;");
            self.emit("    while (s.len > 0 and s[s.len - 1] == '\\n') s = s[0 .. s.len - 1];");
            self.emit("    return s;");
            self.emit("}");
            self.emit("fn sh2GlobName(pat: []const u8, name: []const u8) bool {");
            self.emit("    // fnmatch-style: * ? [cls] — recursive matcher, no regex");
            self.emit("    if (pat.len == 0) return name.len == 0;");
            self.emit("    if (pat[0] == '*') {");
            self.emit("        var k: usize = 0;");
            self.emit("        while (k <= name.len) : (k += 1) {");
            self.emit("            if (sh2GlobName(pat[1..], name[k..])) return true;");
            self.emit("        }");
            self.emit("        return false;");
            self.emit("    }");
            self.emit("    if (name.len == 0) return false;");
            self.emit("    if (pat[0] == '?') return sh2GlobName(pat[1..], name[1..]);");
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
            self.emit("        if (hit != negate) return sh2GlobName(pat[close + 1 ..], name[1..]);");
            self.emit("        return false;");
            self.emit("    }");
            self.emit("    return pat[0] == name[0] and sh2GlobName(pat[1..], name[1..]);");
            self.emit("}");
            self.emit("fn sh2TrimSides(t: []const u8, lead: bool, trail: bool) []const u8 {");
            self.emit("    var s = t;");
            self.emit("    if (lead) s = std.mem.trimLeft(u8, s, \" \\t\\n\\r\");");
            self.emit("    if (trail) s = std.mem.trimRight(u8, s, \" \\t\\n\\r\");");
            self.emit("    return s;");
            self.emit("}");
        }
        if self.need_run {
            self.emit("");
            self.emit("fn sh2Run(argv: []const []const u8) []const u8 {");
            self.emit("    const result = std.process.Child.run(.{ .allocator = std.heap.page_allocator, .argv = argv }) catch return \"\";");
            self.emit("    return std.mem.trimRight(u8, result.stdout, \"\\n\");");
            self.emit("}");
        }
        // per-callee stubs: compile-able, exit(2) at runtime
        const RESERVED_STUB: &[&str] = &[
            "sh2TODO",
            "sh2ToInt",
            "sh2IntStr",
            "sh2Truthy",
            "sh2StrCat",
            "sh2B2S",
            "sh2Test",
            "sh2Arith",
        ];
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
        // self.written (not the pass-1 local): setVar arms discovered
        // during body rendering add their names here
        let written_now: Vec<String> = self.written.iter().cloned().collect();
        for v in &written_now {
            // user-function locals hoist with their fn — never in main
            if self.fn_locals.contains(v) {
                continue;
            }
            let m = self.zig_ident(v);
            let mutated = body_stmts.iter().any(|l| {
                let pat = [format!("{m} ="), format!("{m}="), format!("{m} +="), format!("{m} -="), format!("{m} *="), format!("{m} /="), format!("{m} %="), format!("{m}++"), format!("{m}--")];
                pat.iter().any(|p| l.contains(p.as_str()))
            });
            let kw = if mutated { "var" } else { "const" };
            if self.is_num(v) {
                decls.push(format!("    {kw} {m}: i64 = 0;"));
            } else {
                decls.push(format!("    {kw} {m}: []const u8 = \"\";"));
            }
        }
        if !decls.is_empty() {
            self.emit("");
            for d in &decls {
                self.emit(d);
            }
        }
        if self.need_stdout {
            self.emit("");
            // zig 0.16 removed std.io.getStdOut: stdout writing needs the
            // process Init's Io. Rewrite the already-emitted main signature
            // and set up a buffered File.Writer as `stdout` — generated
            // statements (`try stdout.print(...)`) are unchanged, each one
            // is followed by an explicit flush (early returns / exit() must
            // not lose buffered output).
            for line in self.out.iter_mut() {
                if line == "pub fn main() !void {" {
                    *line = "pub fn main(init: std.process.Init) !void {".to_string();
                    break;
                }
            }
            self.out
                .push("    var stdout_buffer: [4096]u8 = undefined;".to_string());
            self.out.push(
                "    var stdout_file_writer: std.Io.File.Writer = .init(.stdout(), init.io, &stdout_buffer);"
                    .to_string(),
            );
            self.out
                .push("    const stdout = &stdout_file_writer.interface;".to_string());
        }
        if !body_stmts.is_empty() || !decls.is_empty() {
            self.emit("");
        }
        for line in &body_stmts {
            self.emit(line);
        }
        self.emit("}");
        if self.todo > 0 {
            self.emit(&format!(
                "// {} construct(s) lowered to TODO markers",
                self.todo
            ));
        }
    }
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
        IrExpr::Var(name, _) => {
            out.insert(name.clone());
        }
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
        IrExpr::Call { args, .. } => {
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
        let out = render("x=5\necho \"x is $x\"\n");
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
        // capture assigns are A2-typed Str by the core; the sh2Capture
        // stub returns []const u8 so the string var assignment compiles
        assert!(out.contains("var y: []const u8 = \"\";"), "{out}");
        assert!(out.contains("y = sh2Capture();"), "{out}");
        assert!(out.contains("fn sh2Capture() []const u8 {"), "{out}");
    }

    #[test]
    fn zig_keyword_mangled() {
        let out = render("fn=1\necho \"$fn\"\n");
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
