//! Rust backend renderer — LIBRARY interface (worktree-local, branch
//! `backend/rust`). Consumes the ShIR directly in-process, bypassing the
//! `--shir` JSON contract (ask B of docs/backend-c-core-needs.md §1):
//! `shir_to_rust(&IrProgram) -> String`.
//!
//! Uses the core's A2 type verdicts (`IrProgram.var_types`): `Int` vars →
//! Rust `i64`, `Str` vars → `String`, anything else → the runtime store
//! (`String` + sh2.* stubs in this draft). Identifiers are sanitized to
//! Rust identifier syntax and mangled against Rust keywords (A6-consistent),
//! with the `sh2_*`/`_sh2*` helper prefixes reserved. Everything outside
//! the lowable subset (numeric arith, echo-style output, if/else,
//! while/do-while, for over Array/Range, simple assignment, write-file)
//! emits a compile-able `sh2.*` stub or a `// TODO(unsupported)` marker,
//! so the draft always compiles.
//!
//! Rust's type system makes the C draft's `(char*)(sh2_arith())` cast
//! pattern unnecessary: every string-typed value is a `String` expression
//! (literals render as `"…".to_string()`), every number a `i64` expression,
//! and `sh2.*` stubs return `i64` (exiting 2 before returning, so the
//! value never matters — `!` coerces to any return type). Native arithmetic
//! renders to real Rust operators (integer division truncates toward zero
//! like bash); `**` goes through a small `sh2_pow` helper so the emitted
//! code stays typed and compiles.

use crate::ir::{ArithAst, BinOpKind, InterpPart, IrExpr, IrProgram, IrStmt, IrType};
use std::collections::{BTreeSet, HashMap};

enum Part {
    Lit(String),
    Arg(String),
}

#[derive(Default)]
pub struct Render {
    out: Vec<String>,
    depth: usize,
    /// var name -> type verdict (A2); missing = Any (runtime store)
    var_types: HashMap<String, IrType>,
    /// distinct sh2.* callee names that need stubs
    sh2_calls: BTreeSet<String>,
    /// vars written anywhere (declared at the top of main)
    written: BTreeSet<String>,
    /// vars actually read (skip the dead-var comment)
    read: BTreeSet<String>,
    /// Rust identifier per shell var name (sanitize + de-dup)
    mangle: HashMap<String, String>,
    need_pow: bool,
    loop_depth: usize,
    /// gensym counter for loop temporaries (_sh2_items0, _sh2_i0, …)
    gensym: usize,
    todo: usize,
}

/// Rust keywords (edition 2021) — identifiers mangled against these,
/// plus the `sh2_` / `_sh2` helper prefixes the renderer owns.
const RUST_RESERVED: &[&str] = &[
    "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn",
    "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
    "use", "where", "while", "async", "await",
];

/// Render an `IrProgram` to Rust source (fn main()).
pub fn shir_to_rust(prog: &IrProgram) -> String {
    let mut prog = prog.clone();
    // A2: the type verdicts are computed at serialization time in the JSON
    // path; the library path must run the same analysis.
    prog.var_types = crate::shir::analyze_var_types(&prog);
    let mut r = Render::default();
    r.var_types = prog.var_types.iter().cloned().collect();
    r.program(&prog);
    r.out.join("\n")
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

    // ── identifiers ──────────────────────────────────────────────────

    /// Sanitize a shell var name to a valid Rust identifier and mangle
    /// reserved names (A6-consistent). De-duplicates collisions and keeps
    /// the renderer's helper prefixes (`sh2_*`, `_sh2*`) out of user vars.
    fn rust_ident(&mut self, name: &str) -> String {
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
        if RUST_RESERVED.contains(&m.as_str()) || m.starts_with("sh2_") || m.starts_with("_sh2") {
            m.push('_');
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

    /// A Rust string literal (value context — callers append
    /// `.to_string()` where a `String` value is required).
    fn rust_str(s: &str) -> String {
        let mut out = String::new();
        out.push('"');
        for c in s.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                c if (c as u32) < 32 => out.push_str(&format!("\\x{:02x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    /// A String-typed literal expression.
    fn rust_str_expr(s: &str) -> String {
        format!("{}.to_string()", Self::rust_str(s))
    }

    /// A format-string literal for println!/print!/format!: braces are
    /// escaped at PUSH time (print_from_parts/interpolate/call escape
    /// literal text; the `{}` markers we add for args stay raw), so this
    /// only quotes the text.
    fn rust_fmt(s: &str) -> String {
        Self::rust_str(s)
    }

    fn is_num(&self, name: &str) -> bool {
        self.var_types.get(name).copied() == Some(IrType::Int)
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

    fn gensym(&mut self, base: &str) -> String {
        let n = self.gensym;
        self.gensym += 1;
        format!("{base}{n}")
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
                    format!(
                        "{}.trim().parse::<i64>().unwrap_or(0)",
                        Self::rust_str_expr(s)
                    )
                }
            }
            IrExpr::Var(name, _) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return "sh2_getvar()".to_string();
                }
                let m = self.rust_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    m
                } else {
                    format!("{m}.trim().parse::<i64>().unwrap_or(0)")
                }
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return "sh2_getvar()".to_string();
                }
                let m = self.rust_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    m
                } else {
                    format!("{m}.trim().parse::<i64>().unwrap_or(0)")
                }
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Bool(b) => {
                if *b {
                    "1".into()
                } else {
                    "0".into()
                }
            }
            IrExpr::BinOp { op, .. } if matches!(op, BinOpKind::Concat) => {
                format!("{}.trim().parse::<i64>().unwrap_or(0)", self.expr_str(e))
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
                // comparisons/logicals render as bool; bash needs 1/0
                format!("({} as i64)", self.expr_bool(e))
            }
            IrExpr::BinOp { lhs, op, rhs } if *op == BinOpKind::Pow => {
                self.need_pow = true;
                format!("sh2_pow({}, {})", self.expr_num(lhs), self.expr_num(rhs))
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                format!(
                    "({} {} {})",
                    self.expr_num(lhs),
                    self.arith_op(op),
                    self.expr_num(rhs)
                )
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.declared(name) {
                        let m = self.rust_ident(name);
                        self.mark_read(name);
                        if self.is_num(name) {
                            return m;
                        }
                        return format!("{m}.trim().parse::<i64>().unwrap_or(0)");
                    }
                }
                self.sh2_calls.insert("getVar".into());
                "sh2_getvar()".to_string()
            }
            other => {
                format!(
                    "{}.trim().parse::<i64>().unwrap_or(0)",
                    self.expr_any(other)
                )
            }
        }
    }

    /// Render as a String-typed expression.
    fn expr_str(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => Self::rust_str_expr(s),
            IrExpr::Int(i) => format!("({i}).to_string()"),
            IrExpr::Var(name, _) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return "sh2_getvar().to_string()".to_string();
                }
                let m = self.rust_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    format!("{m}.to_string()")
                } else {
                    m
                }
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return "sh2_getvar().to_string()".to_string();
                }
                let m = self.rust_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    format!("{m}.to_string()")
                } else {
                    m
                }
            }
            IrExpr::Interpolate(parts) => self.interpolate(parts),
            IrExpr::Arith(a) => format!("({}).to_string()", self.arith(a)),
            IrExpr::Bool(b) => {
                if *b {
                    "(true).to_string()".into()
                } else {
                    "(false).to_string()".into()
                }
            }
            IrExpr::BinOp { lhs, op, rhs } if *op == BinOpKind::Concat => {
                format!(
                    "(format!(\"{{}}{{}}\", {}, {}))",
                    self.expr_str(lhs),
                    self.expr_str(rhs)
                )
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.declared(name) {
                        let m = self.rust_ident(name);
                        self.mark_read(name);
                        if self.is_num(name) {
                            return format!("{m}.to_string()");
                        }
                        return m;
                    }
                }
                self.sh2_calls.insert("getVar".into());
                "sh2_getvar().to_string()".to_string()
            }
            other => self.expr_any(other),
        }
    }

    /// Render as a bool-typed expression (conditions).
    fn expr_bool(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Bool(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            IrExpr::Int(i) => format!("({i} != 0)"),
            IrExpr::Str(s, _) => format!("(!{}.is_empty())", Self::rust_str_expr(s)),
            IrExpr::Var(name, _) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return "(sh2_getvar() != 0)".to_string();
                }
                let m = self.rust_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    format!("({m} != 0)")
                } else {
                    format!("(!{m}.is_empty())")
                }
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return "(sh2_getvar() != 0)".to_string();
                }
                let m = self.rust_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    format!("({m} != 0)")
                } else {
                    format!("(!{m}.is_empty())")
                }
            }
            IrExpr::BinOp { lhs, op, rhs } => match op {
                BinOpKind::And => format!("({} && {})", self.expr_bool(lhs), self.expr_bool(rhs)),
                BinOpKind::Or => format!("({} || {})", self.expr_bool(lhs), self.expr_bool(rhs)),
                BinOpKind::Not => format!("(!{})", self.expr_bool(lhs)),
                BinOpKind::Eq
                | BinOpKind::Ne
                | BinOpKind::Lt
                | BinOpKind::Gt
                | BinOpKind::Le
                | BinOpKind::Ge => {
                    let rs_op = self.cmp_op(op);
                    if self.static_num(lhs) && self.static_num(rhs) {
                        let (l, r) = (self.expr_num(lhs), self.expr_num(rhs));
                        format!("({l} {rs_op} {r})")
                    } else {
                        let (l, r) = (self.expr_str(lhs), self.expr_str(rhs));
                        format!("({l} {rs_op} {r})")
                    }
                }
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
                "sh2_test()".to_string()
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.declared(name) {
                        let m = self.rust_ident(name);
                        self.mark_read(name);
                        if self.is_num(name) {
                            return format!("({m} != 0)");
                        }
                        return format!("(!{m}.is_empty())");
                    }
                }
                self.sh2_calls.insert("getVar".into());
                "(sh2_getvar() != 0)".to_string()
            }
            other => format!("(!{}.is_empty())", self.expr_any(other)),
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

    /// Render as a String-typed expression (the general form — the
    /// runtime store is a String in this draft, so "any" == String).
    fn expr_any(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => Self::rust_str_expr(s),
            IrExpr::Int(i) => format!("({i}).to_string()"),
            IrExpr::Var(name, _) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return "sh2_getvar().to_string()".to_string();
                }
                let m = self.rust_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    format!("{m}.to_string()")
                } else {
                    m
                }
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return "sh2_getvar().to_string()".to_string();
                }
                let m = self.rust_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    format!("{m}.to_string()")
                } else {
                    m
                }
            }
            IrExpr::Bool(b) => {
                if *b {
                    "(true).to_string()".into()
                } else {
                    "(false).to_string()".into()
                }
            }
            IrExpr::Json(v) => match v {
                serde_json::Value::String(s) => Self::rust_str_expr(s),
                serde_json::Value::Number(n) => format!("({n}).to_string()"),
                serde_json::Value::Bool(b) => {
                    if *b {
                        "(true).to_string()".into()
                    } else {
                        "(false).to_string()".into()
                    }
                }
                _ => {
                    self.mark_todo("Json expr");
                    "String::new()".into()
                }
            },
            IrExpr::BinOp { lhs, op, rhs } if *op == BinOpKind::Concat => {
                format!(
                    "(format!(\"{{}}{{}}\", {}, {}))",
                    self.expr_str(lhs),
                    self.expr_str(rhs)
                )
            }
            IrExpr::BinOp { op, .. }
                if matches!(op, BinOpKind::And | BinOpKind::Or | BinOpKind::Not)
                    || matches!(
                        op,
                        BinOpKind::Eq
                            | BinOpKind::Ne
                            | BinOpKind::Lt
                            | BinOpKind::Gt
                            | BinOpKind::Le
                            | BinOpKind::Ge
                    ) =>
            {
                format!("({} as i64).to_string()", self.expr_bool(e))
            }
            IrExpr::BinOp { lhs, op, rhs } if *op == BinOpKind::Pow => {
                self.need_pow = true;
                format!(
                    "sh2_pow({}, {}).to_string()",
                    self.expr_num(lhs),
                    self.expr_num(rhs)
                )
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                format!(
                    "({} {} {}).to_string()",
                    self.expr_num(lhs),
                    self.arith_op(op),
                    self.expr_num(rhs)
                )
            }
            IrExpr::Arith(a) => format!("({}).to_string()", self.arith(a)),
            IrExpr::Interpolate(parts) => self.interpolate(parts),
            IrExpr::Ternary { cond, then, else_ } => format!(
                "(if {} {{ {} }} else {{ {} }})",
                self.expr_bool(cond),
                self.expr_any(then),
                self.expr_any(else_)
            ),
            IrExpr::DefinedOr { .. } => {
                self.sh2_calls.insert("definedOr".into());
                "sh2_definedor().to_string()".to_string()
            }
            IrExpr::Index { var, .. } => {
                self.mark_read(var);
                self.sh2_calls.insert("arrayIndex".into());
                "sh2_arrayindex().to_string()".to_string()
            }
            IrExpr::Capture { .. } => {
                self.sh2_calls.insert("capture".into());
                "sh2_capture().to_string()".to_string()
            }
            IrExpr::Regex { .. } => {
                self.sh2_calls.insert("regex".into());
                "sh2_regex().to_string()".to_string()
            }
            IrExpr::Range { .. } => {
                self.mark_todo("Range expr");
                "String::new()".into()
            }
            IrExpr::RawExpr(_) => {
                self.mark_todo("RawExpr");
                "String::new()".into()
            }
            IrExpr::Arrow(_) => {
                self.mark_todo("Arrow");
                "String::new()".into()
            }
            IrExpr::Array(_) => {
                self.mark_todo("Array expr");
                "String::new()".into()
            }
            IrExpr::Object(_) => {
                self.mark_todo("Object");
                "String::new()".into()
            }
            IrExpr::Call { func, args } => self.call(func, args),
            IrExpr::MethodCall { .. } => {
                self.sh2_calls.insert("methodCall".into());
                "sh2_methodcall().to_string()".to_string()
            }
        }
    }

    /// String interpolation: "hello $name" → format!(...) (String).
    /// With no Expr parts the raw text is a plain String literal (braces
    /// must NOT be format-escaped — the value is the literal text).
    fn interpolate(&mut self, parts: &[InterpPart]) -> String {
        let mut fmt = String::new();
        let mut raw = String::new();
        let mut args: Vec<String> = Vec::new();
        for p in parts {
            match p {
                InterpPart::Lit(s) => {
                    // escape braces at push time; {} markers stay raw
                    fmt.push_str(&s.replace('{', "{{").replace('}', "}}"));
                    raw.push_str(s);
                }
                InterpPart::Expr(x) => {
                    fmt.push_str("{}");
                    args.push(self.expr_any(x));
                }
            }
        }
        if args.is_empty() {
            Self::rust_str_expr(&raw)
        } else {
            format!("format!({}, {})", Self::rust_fmt(&fmt), args.join(", "))
        }
    }

    // ── arithmetic (native i64) ──────────────────────────────────────

    fn arith(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Num(n) => n.to_string(),
            ArithAst::Var(name) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return "sh2_getvar()".to_string();
                }
                let m = self.rust_ident(name);
                self.mark_read(name);
                if self.is_num(name) {
                    m
                } else {
                    format!("{m}.trim().parse::<i64>().unwrap_or(0)")
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
                        self.need_pow = true;
                        format!("sh2_pow({l},{r})")
                    }
                    "&&" => format!("(({l} != 0 && {r} != 0) as i64)"),
                    "||" => format!("(({l} != 0 || {r} != 0) as i64)"),
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                        format!("(({l} {op} {r}) as i64)")
                    }
                    _ => format!("({l} {op} {r})"),
                }
            }
            ArithAst::Un { op, arg } => {
                let a = self.arith(arg);
                match op.as_str() {
                    "!" => format!("(({a} == 0) as i64)"),
                    "~" => format!("(!{a})"),
                    _ => format!("({op}{a})"),
                }
            }
            ArithAst::Cond { test, then, else_ } => format!(
                "(if ({} != 0) {{ {} }} else {{ {} }})",
                self.arith(test),
                self.arith(then),
                self.arith(else_)
            ),
            ArithAst::Assign { .. } | ArithAst::IncDec { .. } => {
                // runtime setVar semantics (x=, x+=, x++) — sh2.arith stub
                self.sh2_calls.insert("arith".into());
                "sh2_arith()".to_string()
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
            BinOpKind::Pow => {
                // handled by the caller (needs the helper, not an operator)
                "**"
            }
            _ => "+",
        }
    }

    // ── sh2.* calls ──────────────────────────────────────────────────

    /// Render a Call as a String-typed expression. Stub callees take no
    /// args so the generated call always compiles.
    fn call(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            "exec" => {
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    if cmd == "echo" {
                        // argv = the Array of words; join with spaces + "\n"
                        let mut parts = Vec::new();
                        if let Some(IrExpr::Array(items)) = args.get(1) {
                            for (i, item) in items.iter().enumerate() {
                                if i > 0 {
                                    parts.push(Part::Lit(" ".to_string()));
                                }
                                parts.extend(self.parts_of(item));
                            }
                        } else {
                            self.sh2_calls.insert("exec".into());
                            return "sh2_exec().to_string()".to_string();
                        }
                        parts.push(Part::Lit("\n".to_string()));
                        // expression context: the format! form (single value)
                        let mut fmt = String::new();
                        let mut raw = String::new();
                        let mut cargs: Vec<String> = Vec::new();
                        for p in parts {
                            match p {
                                Part::Lit(t) => {
                                    fmt.push_str(&t.replace('{', "{{").replace('}', "}}"));
                                    raw.push_str(&t);
                                }
                                Part::Arg(v) => {
                                    fmt.push_str("{}");
                                    cargs.push(v);
                                }
                            }
                        }
                        if cargs.is_empty() {
                            return Self::rust_str_expr(&raw);
                        }
                        return format!("format!({}, {})", Self::rust_fmt(&fmt), cargs.join(", "));
                    }
                    if cmd == "printf" {
                        return self.sh2_stub("builtin", "builtin printf");
                    }
                }
                self.sh2_stub("exec", "exec")
            }
            // getVar("y") — the ShIR's form of a `$y` read; typed vars
            // lower to bare identifiers, untyped ones to the runtime store.
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.declared(name) {
                        let m = self.rust_ident(name);
                        self.mark_read(name);
                        if self.is_num(name) {
                            return format!("{m}.to_string()");
                        }
                        return m;
                    }
                }
                self.sh2_stub("getVar", "getVar")
            }
            "test" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some(c) = self.test_render(s) {
                        return format!("({c} as i64).to_string()");
                    }
                }
                self.sh2_stub("test", "test")
            }
            _ => self.sh2_stub(func, func),
        }
    }

    fn sh2_stub(&mut self, name: &str, note: &str) -> String {
        self.sh2_calls.insert(name.to_string());
        self.mark_todo(&format!("{note} → sh2.{name}"));
        format!("{}().to_string()", stub_name(name))
    }

    /// Split an expression into output parts: Lit(text) | Arg(rexpr).
    fn parts_of(&mut self, e: &IrExpr) -> Vec<Part> {
        match e {
            IrExpr::Str(s, _) => vec![Part::Lit(s.clone())],
            IrExpr::Int(i) => vec![Part::Arg(format!("({i}).to_string()"))],
            IrExpr::Var(name, _) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return vec![Part::Arg("sh2_getvar().to_string()".into())];
                }
                let m = self.rust_ident(name);
                self.mark_read(name);
                vec![Part::Arg(m)]
            }
            IrExpr::Ident(name) => {
                if !self.declared(name) {
                    self.sh2_calls.insert("getVar".into());
                    return vec![Part::Arg("sh2_getvar().to_string()".into())];
                }
                let m = self.rust_ident(name);
                self.mark_read(name);
                vec![Part::Arg(m)]
            }
            IrExpr::Interpolate(parts) => {
                let mut out = Vec::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => out.push(Part::Lit(s.clone())),
                        InterpPart::Expr(x) => out.push(Part::Arg(self.expr_any(x))),
                    }
                }
                out
            }
            IrExpr::Arith(a) => vec![Part::Arg(format!("({}).to_string()", self.arith(a)))],
            IrExpr::Bool(b) => {
                if *b {
                    vec![Part::Arg("(true).to_string()".into())]
                } else {
                    vec![Part::Arg("(false).to_string()".into())]
                }
            }
            IrExpr::BinOp { .. } => vec![Part::Arg(self.expr_any(e))],
            IrExpr::Call { .. } => vec![Part::Arg(self.expr_any(e))],
            other => {
                self.mark_todo(&format!("echo arg {:?}", other));
                vec![Part::Arg("String::new()".into())]
            }
        }
    }

    /// Parts → one println!(...) / print!(...) call (newline per flag).
    fn print_from_parts(&mut self, parts: Vec<Part>, newline: bool) -> String {
        let mut fmt = String::new();
        let mut args: Vec<String> = Vec::new();
        for p in parts {
            match p {
                Part::Lit(t) => fmt.push_str(&t.replace('{', "{{").replace('}', "}}")),
                Part::Arg(v) => {
                    fmt.push_str("{}");
                    args.push(v);
                }
            }
        }
        if newline {
            if args.is_empty() {
                format!("println!({});", Self::rust_fmt(&fmt))
            } else {
                format!("println!({}, {});", Self::rust_fmt(&fmt), args.join(", "))
            }
        } else if args.is_empty() {
            format!("print!({});", Self::rust_fmt(&fmt))
        } else {
            format!("print!({}, {});", Self::rust_fmt(&fmt), args.join(", "))
        }
    }

    /// Mini `[ ... ]` evaluator for the common patterns; None → stub.
    fn test_render(&mut self, s: &str) -> Option<String> {
        let toks: Vec<&str> = s.split_whitespace().collect();
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
                        format!("{va}.trim().parse::<i64>().unwrap_or(0)")
                    };
                    let vb = if nb {
                        vb
                    } else {
                        format!("{vb}.trim().parse::<i64>().unwrap_or(0)")
                    };
                    Some(format!("({va} {o} {vb})"))
                } else if let Some(o) = str_op {
                    let (va, na) = self.test_operand(a);
                    let (vb, nb) = self.test_operand(b);
                    let va = if na { format!("{va}.to_string()") } else { va };
                    let vb = if nb { format!("{vb}.to_string()") } else { vb };
                    Some(format!("({va} {o} {vb})"))
                } else {
                    None
                }
            }
            [flag, v] if *flag == "-n" => {
                let (vv, nv) = self.test_operand(v);
                let vv = if nv { format!("{vv}.to_string()") } else { vv };
                Some(format!("(!{vv}.is_empty())"))
            }
            [flag, v] if *flag == "-z" => {
                let (vv, nv) = self.test_operand(v);
                let vv = if nv { format!("{vv}.to_string()") } else { vv };
                Some(format!("({vv}.is_empty())"))
            }
            [v] => {
                let (vv, nv) = self.test_operand(v);
                let vv = if nv { format!("{vv}.to_string()") } else { vv };
                Some(format!("(!{vv}.is_empty())"))
            }
            _ => None,
        }
    }

    /// A test operand: `"$y"`/`$y`/`y` (typed var) → ident; number →
    /// literal; `$var` not hoisted → the runtime store; otherwise a quoted
    /// Rust string. Returns (expr, is_num).
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
            let m = self.rust_ident(inner);
            self.mark_read(inner);
            (m, self.is_num(inner))
        } else if let Ok(n) = inner.parse::<i64>() {
            (n.to_string(), true)
        } else if has_dollar {
            // a `$var` reference whose var is never hoisted (env / param):
            // the runtime store — loud TODO rather than a silent literal
            self.sh2_calls.insert("getVar".into());
            ("sh2_getvar()".to_string(), true)
        } else {
            (Self::rust_str_expr(inner), false)
        }
    }

    // ── statements ───────────────────────────────────────────────────

    fn stmt(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Expr(e) => {
                if let IrExpr::Call { func, args } = e {
                    if func == "exec" {
                        if let Some(IrExpr::Str(cmd, _)) = args.first() {
                            if cmd == "exit" {
                                // bash `exit N` / bare `exit` → process::exit
                                let code = match args.get(1) {
                                    Some(IrExpr::Array(items)) if !items.is_empty() => {
                                        self.expr_num(&items[0])
                                    }
                                    _ => "0".to_string(),
                                };
                                self.emit(&format!("std::process::exit(({code}) as i32);"));
                                return;
                            }
                            if cmd == "echo" {
                                let mut parts = Vec::new();
                                if let Some(IrExpr::Array(items)) = args.get(1) {
                                    for (i, item) in items.iter().enumerate() {
                                        if i > 0 {
                                            parts.push(Part::Lit(" ".to_string()));
                                        }
                                        parts.extend(self.parts_of(item));
                                    }
                                    // println! supplies the trailing newline
                                    let call = self.print_from_parts(parts, true);
                                    self.emit(&call);
                                    return;
                                }
                            }
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
                self.emit(&format!("let _ = {x};"));
            }
            IrStmt::Assign { targets, expr } => {
                let Some(t) = targets.first() else {
                    self.mark_todo("multi-target assign");
                    return;
                };
                let m = self.rust_ident(&t.var);
                self.mark_written(&t.var);
                if !t.indices.is_empty() {
                    self.mark_todo("array-index assign");
                    return;
                }
                let rhs = if self.is_num(&t.var) {
                    self.expr_num(expr)
                } else if self.is_str(&t.var) {
                    self.expr_str(expr)
                } else {
                    self.expr_any(expr)
                };
                self.emit(&format!("{m} = {rhs};"));
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    self.mark_written(&d.name);
                    let m = self.rust_ident(&d.name);
                    if let Some(e) = init {
                        let rhs = if self.is_num(&d.name) {
                            self.expr_num(e)
                        } else if self.is_str(&d.name) {
                            self.expr_str(e)
                        } else {
                            self.expr_any(e)
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
                self.emit(&format!("let _ = std::fs::write({p}, {c});"));
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                let c = self.expr_bool(cond);
                self.emit(&format!("if {c} {{"));
                self.depth += 1;
                for s in then {
                    self.stmt(s);
                }
                self.depth -= 1;
                for (ec, body) in elsifs {
                    let ec = self.expr_bool(ec);
                    self.emit(&format!("}} else if {ec} {{"));
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
            IrStmt::While { cond, body } => {
                let c = self.expr_bool(cond);
                self.emit(&format!("while {c} {{"));
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
                self.emit("loop {");
                self.loop_depth += 1;
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.loop_depth -= 1;
                let c = self.expr_bool(cond);
                if *until {
                    self.emit(&format!("if {c} {{ break; }}"));
                } else {
                    self.emit(&format!("if !{c} {{ break; }}"));
                }
                self.emit("}");
            }
            IrStmt::For { var, iter, body } => {
                let m = self.rust_ident(var);
                self.mark_written(var);
                match iter {
                    IrExpr::Array(items) => {
                        // an indexed while over a local Vec: `for m in …`
                        // would shadow the hoisted mutable var, and moving
                        // out of the array is illegal — clone per element
                        let items_t = if self.is_num(var) { "i64" } else { "String" };
                        let items_v: Vec<String> = if self.is_num(var) {
                            items.iter().map(|i| self.expr_num(i)).collect()
                        } else {
                            items.iter().map(|i| self.expr_str(i)).collect()
                        };
                        let items_g = self.gensym("_sh2_items");
                        let idx_g = self.gensym("_sh2_i");
                        self.emit(&format!(
                            "let {items_g}: Vec<{items_t}> = vec![{}];",
                            items_v.join(", ")
                        ));
                        self.emit(&format!("let mut {idx_g}: usize = 0;"));
                        self.emit(&format!("while {idx_g} < {items_g}.len() {{"));
                        self.loop_depth += 1;
                        self.depth += 1;
                        self.emit(&format!("{m} = {items_g}[{idx_g}].clone();"));
                        for s in body {
                            self.stmt(s);
                        }
                        self.depth -= 1;
                        self.loop_depth -= 1;
                        self.emit(&format!("{idx_g} += 1;"));
                        self.emit("}");
                    }
                    IrExpr::Range { start, end } if self.is_num(var) => {
                        // C-style counter loop: `for m in start..=end`
                        // would shadow the hoisted var and bind an
                        // immutable copy
                        self.emit(&format!("{m} = {start};"));
                        self.emit(&format!("loop {{"));
                        self.loop_depth += 1;
                        self.depth += 1;
                        self.emit(&format!("if !({m} <= {end}) {{ break; }}"));
                        for s in body {
                            self.stmt(s);
                        }
                        self.depth -= 1;
                        self.loop_depth -= 1;
                        self.emit(&format!("{m} += 1;"));
                        self.emit("}");
                    }
                    other => {
                        self.mark_todo(&format!("For iter {:?}", other));
                        self.mark_todo("For body skipped");
                        // body vars are still hoisted via collect_written
                    }
                }
            }
            IrStmt::Exit(e) => {
                let code = e
                    .as_ref()
                    .map(|x| self.expr_num(x))
                    .unwrap_or_else(|| "0".into());
                self.emit(&format!("std::process::exit(({code}) as i32);"));
            }
            IrStmt::Block(b) => {
                for s in b {
                    self.stmt(s);
                }
            }
            IrStmt::Pipeline { .. }
            | IrStmt::Die { .. }
            | IrStmt::Warn { .. }
            | IrStmt::Return(_)
            | IrStmt::SetChildError(_)
            | IrStmt::Require(_)
            | IrStmt::RawText(_)
            | IrStmt::Case { .. }
            | IrStmt::Redirect { .. }
            | IrStmt::Function { .. }
            | IrStmt::Subshell(_)
            | IrStmt::Background(_)
            | IrStmt::Exec { .. }
            | IrStmt::Label(_)
            | IrStmt::Goto(_) => {
                self.mark_todo(&format!("stmt {:?}", s));
            }
            IrStmt::ForInit { .. } => self.mark_todo("ForInit (strip_cfor should have lowered it)"),
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

        // Pass 2: render the body first (helper flags known before preamble).
        let mut body_out = Vec::new();
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 1;
        for v in &written {
            let m = self.rust_ident(v);
            if self.is_num(v) {
                self.emit(&format!("let mut {m}: i64 = 0;"));
            } else {
                self.emit(&format!("let mut {m}: String = String::new();"));
            }
        }
        if !written.is_empty() {
            self.emit("");
        }
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
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 0;

        // Preamble: sh2.* stubs, then main with the rendered body.
        if !self.sh2_calls.is_empty() || self.need_pow {
            self.emit("// sh2.* runtime stubs — TODO: implement (harness/sh2-namespace.json)");
            self.emit("");
        }
        if self.need_pow {
            self.emit("fn sh2_pow(a: i64, b: i64) -> i64 {");
            self.emit("    let mut r: i64 = 1;");
            self.emit("    let mut i: i64 = 0;");
            self.emit("    while i < b { r *= a; i += 1; }");
            self.emit("    r");
            self.emit("}");
            self.emit("");
        }
        if !self.sh2_calls.is_empty() {
            // per-callee stubs: compile-able, exit(2) at runtime
            let mut seen: BTreeSet<String> = BTreeSet::new();
            let names: Vec<String> = self.sh2_calls.iter().cloned().collect();
            for name in names {
                let fname = stub_name(&name);
                if !seen.insert(fname.clone()) {
                    continue; // two callees snaked to the same stub name
                }
                let ret = if name == "test" { "bool" } else { "i64" };
                self.emit(&format!("fn {fname}() -> {ret} {{"));
                self.emit(&format!("    eprintln!(\"TODO sh2.{}\");", name));
                self.emit("    std::process::exit(2)");
                self.emit("}");
                self.emit("");
            }
        }
        self.emit("fn main() {");
        self.out.extend(body_out.iter().cloned());
        self.emit("}");
        if self.todo > 0 {
            self.emit(&format!(
                "// {} construct(s) lowered to TODO markers",
                self.todo
            ));
        }
    }
}

impl Render {
    fn is_str(&self, name: &str) -> bool {
        self.var_types.get(name).copied() == Some(IrType::Str)
    }
}

/// Snake_case a sh2.* callee name for the stub function name
/// (matches the hardcoded sh2_getvar/sh2_capture/… call sites).
fn snake(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        "x".to_string()
    } else {
        out
    }
}

/// The stub fn name for a sh2.* callee; `sh2_pow` is the native pow
/// helper, so a (hypothetical) pow callee gets a distinct stub name.
fn stub_name(name: &str) -> String {
    let f = format!("sh2_{}", snake(name));
    if f == "sh2_pow" {
        f + "_stub"
    } else {
        f
    }
}

/// Collect every variable written by statements (assign targets, declare
/// lists, For loop vars) — the hoisted declaration set.
fn collect_written(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr } => {
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
            IrStmt::Function { body, .. } => collect_written(body, out),
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_written(st, out);
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
        _ => {}
    }
}

fn collect_written_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(name) => {
            out.insert(name.clone());
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
        shir_to_rust(&prog)
    }

    #[test]
    fn assigns_and_echo() {
        let out = render("x=5\necho \"x is $x\"\n");
        assert!(out.contains("fn main() {"), "{out}");
        assert!(out.contains("let mut x: i64 = 0;"), "{out}");
        assert!(out.contains("x = 5;"), "{out}");
        assert!(
            out.contains("println!(\"x is {}\", x.to_string());"),
            "{out}"
        );
    }

    #[test]
    fn if_arith_test() {
        let out = render("x=3\nif [ \"$x\" -gt 3 ]; then\ny=$((x+1))\necho \"$y\"\nfi\n");
        assert!(out.contains("if (x > 3) {"), "{out}");
        assert!(out.contains("y = (x + 1);"), "{out}");
    }

    #[test]
    fn untyped_var_uses_string() {
        let out = render("y=$(ls)\necho \"$y\"\n");
        // capture assigns are A2-typed Str by the core; the stub call
        // returns i64, .to_string() keeps it assignable to the String var
        assert!(out.contains("let mut y: String = String::new();"), "{out}");
        assert!(out.contains("y = sh2_capture().to_string();"), "{out}");
        assert!(out.contains("println!(\"{}\", y);"), "{out}");
    }

    #[test]
    fn rust_keyword_mangled() {
        let out = render("type=1\necho \"$type\"\n");
        assert!(out.contains("let mut type_: i64 = 0;"), "{out}");
        assert!(!out.contains("let mut type: i64"), "{out}");
    }
}
