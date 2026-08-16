//! JS backend renderer — LIBRARY interface (worktree-local, branch
//! `backend/js`). Consumes the ShIR directly in-process, bypassing the
//! `--shir` JSON contract (ask B of backends/c/docs/backend-c-core-needs.md
//! §1): `shir_to_js(&IrProgram) -> String`.
//!
//! Uses the core's A2 type verdicts (`IrProgram.var_types`): `Int` vars →
//! JS numbers, `Str` vars → JS strings, anything else → runtime store
//! (`let` + sh2.* stubs in this draft). Identifiers are mangled against
//! JS reserved words. Everything outside the lowable subset (numeric
//! arith, echo/printf, if/else/loops, simple assignment) emits a
//! compile-able `sh2.*` stub or a `/* TODO(unsupported) */` marker, so
//! the draft is always valid JS.
//!
//! Output shape (mirrors the C renderer's main() pattern): sh2.* stubs
//! first, then `function main() { … }` with hoisted `let` declarations,
//! then `main();`. Everything runs inside `main` so `return` (from a
//! top-level shell `return`) stays valid JS.

use crate::ir::{ArithAst, InterpPart, IrExpr, IrProgram, IrStmt, IrType};
use std::collections::{BTreeSet, HashMap};

enum Part {
    Lit(String),
    Arg(String, bool),
}

#[derive(Default)]
pub struct Render {
    out: Vec<String>,
    depth: usize,
    /// var name -> type verdict (A2); missing = Any (runtime store)
    var_types: HashMap<String, IrType>,
    /// var names that may hold shell-env values (var_bash_env) — getVar
    /// of these stays on the runtime stub (the native `let` binding does
    /// not carry the env value)
    env_vars: BTreeSet<String>,
    /// every var name the program references (assign targets, declare
    /// lists, reads, typed verdicts) — the getVar fold's "known" set: a
    /// name never referenced reads as the constant "" (bash unset read,
    /// mirroring the estree ref's SH2_ASSUME_NO_ENV fold)
    known_vars: BTreeSet<String>,
    /// distinct sh2.* callee names that need stubs
    sh2_calls: BTreeSet<String>,
    /// $1..$9 positional reads / fnCall present — emit the positional
    /// array + call protocol
    uses_positional: bool,
    /// `$?` reads / exec statements present — emit the `sh2_lastExit`
    /// status binding + the writes after each exec (the estree ref's
    /// `sh2.lastExit` protocol; bash `$?` is the last command's status)
    uses_status: bool,
    /// C memory arena (mem*) calls present — emit the mem helpers
    uses_mem: bool,
    todo: usize,
    loop_depth: usize,
}

/// A plain shell/JS identifier (no `[idx]` baked names, no specials).
fn is_plain_name(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Names bash may answer from the CALLER's environment (the estree
/// ref's `env_resident` set): a never-written read of one of these
/// falls back to `process.env.<name>` instead of the unset "".
const ENV_RESIDENT: &[&str] = &[
    "HOME", "USER", "PATH", "LOGNAME", "SHELL", "TERM", "LANG", "LC_ALL", "LC_CTYPE",
    "OLDPWD", "SHLVL", "TMPDIR", "EDITOR", "PAGER", "HOSTNAME", "BASH_VERSION", "BASH",
];

/// Render an `IrProgram` to a Node-style JS script.
pub fn shir_to_js(prog: &IrProgram) -> String {
    let mut prog = prog.clone();
    // builtin-op fallback arm (shir-builtin-op-20260816): the js backend
    // has NOT accepted the `builtin` op — render as exec.
    crate::transforms::builtin::fallback_builtin_to_exec(&mut prog);
    // A2: the type verdicts are computed at serialization time in the JSON
    // path; the library path must run the same analysis.
    prog.var_types = crate::shir::analyze_var_types(&prog);
    let mut r = Render::default();
    r.var_types = prog.var_types.iter().cloned().collect();
    r.env_vars = prog.var_bash_env.iter().cloned().collect();
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
        // debug text can contain `*/` (e.g. glob patterns) — sanitize so
        // the emitted comment can never terminate early.
        let safe = what.replace("*/", "* /");
        self.emit(&format!("/* TODO(unsupported): {safe} */"));
    }

    /// JS reserved words (incl. strict-mode + future reserved) mangled
    /// with a trailing `_`, mirroring the C renderer's keyword mangling.
    fn js_ident(&self, name: &str) -> String {
        const JS_KEYWORDS: &[&str] = &[
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "debugger",
            "default",
            "delete",
            "do",
            "else",
            "enum",
            "export",
            "extends",
            "false",
            "finally",
            "for",
            "function",
            "if",
            "implements",
            "import",
            "in",
            "instanceof",
            "interface",
            "let",
            "new",
            "null",
            "package",
            "private",
            "protected",
            "public",
            "return",
            "static",
            "super",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "typeof",
            "var",
            "void",
            "while",
            "with",
            "yield",
            "arguments",
            "eval",
        ];
        if name.is_empty() || JS_KEYWORDS.contains(&name) {
            format!("{name}_")
        } else {
            name.to_string()
        }
    }

    fn js_str(s: &str) -> String {
        let mut out = String::new();
        out.push('"');
        for c in s.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                '\u{08}' => out.push_str("\\b"),
                '\u{0c}' => out.push_str("\\f"),
                c if (c as u32) < 32 => out.push_str(&format!("\\x{:02x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    fn is_num(&self, name: &str) -> bool {
        self.var_types.get(name).copied() == Some(IrType::Int)
    }

    // ── expressions ──────────────────────────────────────────────────

    fn expr(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Int(i) => i.to_string(),
            IrExpr::Str(s, _) => Self::js_str(s),
            IrExpr::Var(name, _) => self.js_ident(name),
            IrExpr::Ident(name) => self.js_ident(name),
            IrExpr::Index { var, key } => {
                format!("{}[{}]", self.js_ident(var), self.expr(key))
            }
            IrExpr::Bool(b) => {
                if *b {
                    "true".into()
                } else {
                    "false".into()
                }
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                // `Not` is unary in shell semantics: the parser duplicates
                // the operand into rhs; the core renderers use lhs only.
                if matches!(op, crate::ir::BinOpKind::Not) {
                    return format!("(!{})", self.expr(lhs));
                }
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                let js_op = match op {
                    crate::ir::BinOpKind::Add => "+",
                    crate::ir::BinOpKind::Sub => "-",
                    crate::ir::BinOpKind::Mul => "*",
                    crate::ir::BinOpKind::Div => "/",
                    crate::ir::BinOpKind::Mod => "%",
                    crate::ir::BinOpKind::Concat => "+",
                    crate::ir::BinOpKind::Eq => "==",
                    crate::ir::BinOpKind::Ne => "!=",
                    crate::ir::BinOpKind::Lt => "<",
                    crate::ir::BinOpKind::Gt => ">",
                    crate::ir::BinOpKind::Le => "<=",
                    crate::ir::BinOpKind::Ge => ">=",
                    crate::ir::BinOpKind::And => "&&",
                    crate::ir::BinOpKind::Or => "||",
                    crate::ir::BinOpKind::Not => "!", // unreachable (early return above)
                    crate::ir::BinOpKind::BitAnd => "&",
                    crate::ir::BinOpKind::BitOr => "|",
                    crate::ir::BinOpKind::BitXor => "^",
                    crate::ir::BinOpKind::ShiftL => "<<",
                    crate::ir::BinOpKind::ShiftR => ">>",
                    crate::ir::BinOpKind::Pow => {
                        return format!("Math.pow({l},{r})");
                    }
                };
                format!("({l} {js_op} {r})")
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Interpolate(parts) => self.interpolate(parts),
            IrExpr::Array(items) => {
                let elems: Vec<String> = items.iter().map(|e| self.expr(e)).collect();
                format!("[{}]", elems.join(", "))
            }
            IrExpr::Ternary { cond, then, else_ } => format!(
                "({} ? {} : {})",
                self.expr(cond),
                self.expr(then),
                self.expr(else_)
            ),
            IrExpr::DefinedOr { expr, default } => {
                format!("({} ?? {})", self.expr(expr), self.expr(default))
            }
            IrExpr::Call { func, args } if func == "split" => {
                if let Some(x) = args.first() {
                    return self.expr(x);
                }
                self.sh2_stub("split", args, "split")
            }
            IrExpr::Call { func, args } => self.call(func, args),
            // a numeric-range iterable (`For.iter` Range — the
            // rust-frontend's `0..3` / `1..=2`): the JS surface has no
            // range literal, so materialize the string list (the estree
            // ref's bounded fallback — the For-of yields strings; the
            // For arm's numeric-lift coerces each item)
            IrExpr::Range { start, end } => {
                let items: Vec<String> = (*start..=*end)
                    .map(|v| Self::js_str(&v.to_string()))
                    .collect();
                format!("[{}]", items.join(", "))
            }
            IrExpr::Json(v) => match v {
                serde_json::Value::String(s) => Self::js_str(s),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => {
                    if *b {
                        "true".into()
                    } else {
                        "false".into()
                    }
                }
                serde_json::Value::Null => "null".into(),
                _ => {
                    self.mark_todo("Json expr");
                    "0".into()
                }
            },
            other => {
                self.mark_todo(&format!("expr {:?}", other));
                "0".into()
            }
        }
    }

    /// String interpolation: `"a$xb"` → `("a" + String(x) + "b")` —
    /// plain `+` concatenation, with NUMERIC parts String()-coerced so
    /// an all-numeric interpolation (`"$i$j"`) concatenates instead of
    /// adding (bash: string context; the estree ref renders
    /// interpolations as template literals, which stringify each part).
    fn interpolate(&mut self, parts: &[InterpPart]) -> String {
        let mut exprs: Vec<String> = Vec::new();
        let mut lit = String::new();
        for p in parts {
            match p {
                InterpPart::Lit(s) => lit.push_str(s),
                InterpPart::Expr(x) => {
                    if !lit.is_empty() {
                        exprs.push(Self::js_str(&lit));
                        lit.clear();
                    }
                    let e = self.expr(x);
                    if self.expr_is_num(x) {
                        exprs.push(format!("String({e})"));
                    } else {
                        exprs.push(e);
                    }
                }
            }
        }
        if !lit.is_empty() {
            exprs.push(Self::js_str(&lit));
        }
        match exprs.len() {
            0 => "(\"\")".to_string(),
            1 => format!("({})", exprs[0]),
            _ => format!("({})", exprs.join(" + ")),
        }
    }

    /// Native JS arithmetic from ArithAst (the numeric path).
    fn arith(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Num(n) => n.to_string(),
            // arith is a numeric context: bash vars are strings, so a
            // non-Int-typed var must be coerced (mirrors the core's
            // `Number(x) || 0` lowering); Int-typed vars are already
            // native JS numbers.
            ArithAst::Var(name) | ArithAst::Ident(name) => {
                if self.is_num(name) {
                    self.js_ident(name)
                } else {
                    format!("(Number({}) || 0)", self.js_ident(name))
                }
            }
            ArithAst::Index { .. } => {
                self.mark_todo("arith Index");
                "0".into()
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let l = self.arith(lhs);
                let r = self.arith(rhs);
                if *op == "**" {
                    format!("Math.pow({l},{r})")
                } else {
                    format!("({l} {op} {r})")
                }
            }
            ArithAst::Un { op, arg } => format!("({op}{})", self.arith(arg)),
            ArithAst::Cond { test, then, else_ } => format!(
                "({} ? {} : {})",
                self.arith(test),
                self.arith(then),
                self.arith(else_)
            ),
            ArithAst::Assign { .. } | ArithAst::IncDec { .. } => {
                // runtime setVar semantics (x+=, x++) — sh2.arith stub
                self.sh2_calls.insert("arith".into());
                format!("sh2_arith()")
            }
            // C-frontend nodes (never emitted by the shell path): sizeof is
            // a compile-time constant; casts lower to the JS width idioms
            // (`| 0` for int, `BigInt.asIntN(64, …)` for long long).
            ArithAst::Sizeof(ty) => ty.c_sizeof().unwrap_or(4).to_string(),
            ArithAst::Cast { ty, arg } => {
                let inner = self.arith(arg);
                match ty {
                    // Number(...) first: the arg may already be a BigInt
                    // (an i64 sub-expression cast down to int)
                    IrType::Int32 => format!("(Number({inner}) | 0)"),
                    IrType::UInt32 => format!("(Number({inner}) >>> 0)"),
                    // exact i64 literal: the STRING form keeps the value
                    // exact past 2^53 (a JS number literal rounds —
                    // t44_huge_cond.c's 2^63-1 came out even); mirrors
                    // the estree ref's bigint_lit_expr. The value already
                    // fits, so no asIntN wrap.
                    IrType::Int64 => match &**arg {
                        ArithAst::Num(n) => format!("BigInt({})", Self::js_str(&n.to_string())),
                        _ => format!("BigInt.asIntN(64, BigInt({inner}))"),
                    },
                    // u64: the i64 value is the two's-complement bit
                    // pattern (the frontend emits negative i64 for u64
                    // values >= 2^63); the STRING form keeps the literal
                    // exact past 2^53 (estree parity)
                    IrType::UInt64 => match &**arg {
                        ArithAst::Num(n) => format!(
                            "BigInt.asUintN(64, BigInt({}))",
                            Self::js_str(&n.to_string())
                        ),
                        _ => format!("BigInt.asUintN(64, BigInt({inner}))"),
                    },
                    IrType::Float(32) => format!("Math.fround({inner})"),
                    _ => inner,
                }
            }
        }
    }

    fn sh2_stub(&mut self, name: &str, _args: &[IrExpr], note: &str) -> String {
        self.sh2_calls.insert(name.to_string());
        self.mark_todo(&format!("{note} → sh2.{name}"));
        format!("sh2_{}()", name.replace('.', "_"))
    }

    fn call(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            // exec("echo", [words...]) → native process.stdout.write
            "exec" => {
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    // exec("true") / exec("false") — the restructure
                    // pass's While(true) cond (a backward-goto loop)
                    if cmd == "true" {
                        return "true".into();
                    }
                    if cmd == "false" {
                        return "false".into();
                    }
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
                        }
                        parts.push(Part::Lit("\n".to_string()));
                        return self.write_from_parts(parts);
                    }
                    if cmd == "printf" {
                        return self.printf_call(args);
                    }
                }
                self.sh2_stub("exec", args, "exec")
            }
            // getVar("y") — the ShIR's form of a `$y` read; known vars
            // lower to the native binding, unset reads to "" (the estree
            // ref's SH2_ASSUME_NO_ENV fold).
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if let Some(x) = self.getvar_expr(name) {
                        return x;
                    }
                }
                self.sh2_stub("getVar", args, "getVar")
            }
            // `arith("$i")` — the C frontend's runtime-arith operand
            // (dynamic array index / cond); plain reads fold natively.
            "arith" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some(x) = self.arith_str(s) {
                        return x;
                    }
                }
                self.sh2_stub("arith", args, "arith")
            }
            // `param(op, name[, a[, b]])` — `${x}` parameter expansions
            // (the C/zsh/go frontends' `${#x}` strlen / `${x:-d}`
            // default / strip / replace / slice lowerings). Mirrors the
            // estree ref's `try_native_param` native forms for literal
            // operands; anything else keeps the runtime stub.
            "param" => {
                if let (Some(IrExpr::Str(op, _)), Some(IrExpr::Str(name, _))) =
                    (args.first(), args.get(1))
                {
                    if let Some(x) = self.param_native(op, name, args) {
                        return x;
                    }
                }
                self.sh2_stub("param", args, "param")
            }
            // C array literal (`int a[3] = {..}` → setArray("a", [...]))
            "setArray" => {
                if let (Some(IrExpr::Str(name, _)), Some(IrExpr::Array(items))) =
                    (args.first(), args.get(1))
                {
                    if is_plain_name(name) {
                        let elems: Vec<String> = items.iter().map(|e| self.expr(e)).collect();
                        return format!("({} = [{}])", self.js_ident(name), elems.join(", "));
                    }
                }
                self.sh2_stub("setArray", args, "setArray")
            }
            // C array element write (`a[i] = v` → arrayStore("a", i, v))
            "arrayStore" => {
                if let (Some(IrExpr::Str(name, _)), Some(idx), Some(val)) =
                    (args.first(), args.get(1), args.get(2))
                {
                    if is_plain_name(name) {
                        return format!(
                            "({}[{}] = {})",
                            self.js_ident(name),
                            self.expr(idx),
                            self.expr(val)
                        );
                    }
                }
                self.sh2_stub("arrayStore", args, "arrayStore")
            }
            // C array element read (`v = a[i]` → arrayIndex("a", i))
            "arrayIndex" => {
                if let (Some(IrExpr::Str(name, _)), Some(idx)) = (args.first(), args.get(1)) {
                    if is_plain_name(name) {
                        return format!("{}[{}]", self.js_ident(name), self.expr(idx));
                    }
                }
                self.sh2_stub("arrayIndex", args, "arrayIndex")
            }
            // out-param protocol: capture a function call's stdout
            // (literal-echo bodies fold to the text; anything else runs
            // with fd-1 swapped into a buffer)
            "capture" => {
                if let Some(IrExpr::Arrow(body)) = args.first() {
                    if let Some(text) = self.capture_echo_fold(body) {
                        return Self::js_str(&text);
                    }
                    return self.capture_iife(body);
                }
                self.sh2_stub("capture", args, "capture")
            }
            // user function call with positional setup ($1..$9)
            "fnCall" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if is_plain_name(name) {
                        let fname = self.js_ident(name);
                        let arg_exprs: Vec<String> = match args.get(1) {
                            Some(IrExpr::Array(items)) => {
                                items.iter().map(|a| self.expr(a)).collect()
                            }
                            _ => Vec::new(),
                        };
                        self.uses_positional = true;
                        return format!(
                            "(() => {{ const __saved = sh2_positional; sh2_positional = [{}]; try {{ return {}({}); }} finally {{ sh2_positional = __saved; }} }})()",
                            arg_exprs.join(", "),
                            fname,
                            arg_exprs.join(", ")
                        );
                    }
                }
                self.sh2_stub("fnCall", args, "fnCall")
            }
            // user function call in a VALUE position — the C frontend's
            // value-returning dispatch (t58_func_runtime.c: printf args,
            // assign RHSes): same positional save/restore as fnCall, but
            // the function's native `return e` value comes back to the
            // caller (the estree ref's sh2.fnValue; caller-side
            // consumers like printf %d coerce).
            "fnValue" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if is_plain_name(name) {
                        let fname = self.js_ident(name);
                        let arg_exprs: Vec<String> = match args.get(1) {
                            Some(IrExpr::Array(items)) => {
                                items.iter().map(|a| self.expr(a)).collect()
                            }
                            _ => Vec::new(),
                        };
                        self.uses_positional = true;
                        return format!(
                            "(() => {{ const __saved = sh2_positional; sh2_positional = [{}]; try {{ return {}({}); }} finally {{ sh2_positional = __saved; }} }})()",
                            arg_exprs.join(", "),
                            fname,
                            arg_exprs.join(", ")
                        );
                    }
                }
                self.sh2_stub("fnValue", args, "fnValue")
            }
            // multi-return line read (`line(cap, N)` → split("\n")[N])
            "line" => {
                if let (Some(text), Some(IrExpr::Str(i, _))) = (args.first(), args.get(1)) {
                    let t = self.expr(text);
                    let n = i.parse::<usize>().unwrap_or(0);
                    return format!("({}.split(\"\\n\")[{}] ?? \"\")", t, n);
                }
                self.sh2_stub("line", args, "line")
            }
            // C memory arena (malloc/pointer arithmetic) — helper
            // preamble mirrors the estree ref runtime's mem* slice-2
            "memAlloc" | "memStore" | "memLoad" | "memAdvance" | "memFree" | "memTest"
            | "memElemSize" => {
                self.uses_mem = true;
                let arg_exprs: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                format!("sh2_{}({})", func, arg_exprs.join(", "))
            }
            // test("...") — mini evaluator for the common numeric/string
            // patterns; anything else → runtime stub.
            "test" => {
                if let Some(IrExpr::Str(s, _)) = args.first() {
                    if let Some(c) = self.test_render(s) {
                        return c;
                    }
                }
                self.sh2_stub("test", args, "test")
            }
            // everything else → compile-able sh2.* stub
            _ => self.sh2_stub(func, args, func),
        }
    }

    /// A `$name` read (getVar("name")) → the native JS expression, or
    /// None when the read must stay on the sh2.* stub (env/special vars).
    /// Mirrors the estree ref's SH2_ASSUME_NO_ENV fold: a name never
    /// written in the program is the constant "" (bash reads an unset
    /// var as empty). `$1..$9` read the positional array the fnCall
    /// protocol establishes (0-based, like the estree ref runtime).
    fn getvar_expr(&mut self, name: &str) -> Option<String> {
        if self.env_vars.contains(name) {
            return None;
        }
        // `$?` — the last command's exit status (the estree ref's
        // `sh2.lastExit`; exec statements write it)
        if name == "?" {
            self.uses_status = true;
            return Some("sh2_lastExit".into());
        }
        if matches!(
            name,
            "$"
                | "#"
                | "@"
                | "*"
                | "0"
                | "-"
                | "PWD"
                | "HOSTNAME"
                | "BASH_VERSION"
                | "BASH"
                | "SHELL"
                | "EPOCHREALTIME"
                | "EPOCHSECONDS"
        ) {
            return None;
        }
        if let Ok(n) = name.parse::<usize>() {
            if (1..=9).contains(&n) {
                self.uses_positional = true;
                return Some(format!("(sh2_positional[{}] ?? \"\")", n - 1));
            }
            return None; // $0 → argv0 (no standalone equivalent)
        }
        if is_plain_name(name) {
            if self.known_vars.contains(name) {
                return Some(self.js_ident(name));
            }
            // ENV-RESIDENT name (the estree ref's env_resident set —
            // the runtime's env fallback): a name never written in the
            // program answers the caller's environment (zsh-sh-go
            // t05_env_read.zsh — `echo $HOME`)
            if ENV_RESIDENT.contains(&name) {
                return Some(format!("(process.env.{name} ?? \"\")"));
            }
            // never written (and not env): bash reads unset → ""
            return Some("\"\"".into());
        }
        None
    }

    /// `arith("$i")` — the C frontend's runtime-arith operand (array
    /// index/cond); a plain `$name` (or `$a op $b`) folds to the native
    /// bindings. Anything else → None (runtime stub).
    fn arith_str(&mut self, s: &str) -> Option<String> {
        let toks: Vec<&str> = s.split_whitespace().collect();
        match toks.as_slice() {
            [t] => {
                let rest = t.strip_prefix('$')?;
                if is_plain_name(rest) {
                    Some(self.js_ident(rest))
                } else {
                    None
                }
            }
            [l, op, r] if matches!(*op, "+" | "-" | "*" | "/" | "%") => {
                let l = l.strip_prefix('$').filter(|x| is_plain_name(x))?;
                let rr = if let Some(n) = r.strip_prefix('$') {
                    if is_plain_name(n) {
                        self.js_ident(n)
                    } else {
                        return None;
                    }
                } else if let Ok(n) = r.parse::<i64>() {
                    n.to_string()
                } else {
                    return None;
                };
                Some(format!("({} {} {})", self.js_ident(l), op, rr))
            }
            _ => None,
        }
    }

    /// The native read of a `param` target: the binding for a
    /// referenced name, the env fallback for an env-resident
    /// never-written name, "" — exactly the getVar fold (param reads
    /// route through the same value paths; `?` is the last-exit status).
    fn read_var(&mut self, name: &str) -> String {
        if name == "?" {
            self.uses_status = true;
            return "sh2_lastExit".into();
        }
        if self.known_vars.contains(name) {
            return self.js_ident(name);
        }
        if ENV_RESIDENT.contains(&name) {
            return format!("(process.env.{name} ?? \"\")");
        }
        "\"\"".into()
    }

    /// A literal glob-strip/substitute pattern (no metachars) that
    /// embeds cleanly in a JS string literal.
    fn is_literal_pattern(p: &str) -> bool {
        !p.is_empty() && p.is_ascii() && !p.chars().any(|c| matches!(c, '*' | '?' | '['))
    }

    /// `param(op, name[, a[, b]])` with literal operands — the native
    /// forms mirroring the estree ref's `try_native_param` (the runtime
    /// param dispatch's literal fast paths) over the native bindings.
    /// None → the runtime stub.
    fn param_native(&mut self, op: &str, name: &str, args: &[IrExpr]) -> Option<String> {
        if !is_plain_name(name) {
            return None;
        }
        let val = self.read_var(name);
        // `${x}` — a plain read of the binding
        if op.is_empty() {
            return Some(val);
        }
        // `${#x}` — string length
        if op == "len" {
            return Some(format!("String({val}).length"));
        }
        // `${x^^}` / `${x,,}` — case conversion
        if op == "^^" {
            return Some(format!("({val}).toUpperCase()"));
        }
        if op == ",," {
            return Some(format!("({val}).toLowerCase()"));
        }
        // `${x^}` / `${x,}` — first character only (empty → "", like
        // the runtime's `v.length ? ... : v`)
        if op == "^" || op == "," {
            let up = if op == "^" { "toUpperCase" } else { "toLowerCase" };
            return Some(format!(
                "(({val}).charAt(0).{up}() + ({val}).slice(1))"
            ));
        }
        // `${x#p}` / `${x##p}` / `${x%p}` / `${x%%p}` — literal
        // prefix/suffix removal (shortest == longest for literal
        // patterns, exactly like the runtime's literal fast paths);
        // single-star globs (`*P` / `P*`) strip through the first/last
        // occurrence of the literal core.
        if matches!(op, "#" | "##" | "%" | "%%") {
            let [_, _, IrExpr::Str(p, _), ..] = args else {
                return None;
            };
            if Self::is_literal_pattern(p) {
                let len = p.chars().count() as i64;
                return Some(if op.starts_with('#') {
                    format!(
                        "(({val}).startsWith({p}) ? ({val}).slice({len}) : ({val}))",
                        p = Self::js_str(p)
                    )
                } else {
                    format!(
                        "(({val}).endsWith({p}) ? ({val}).slice(0, -{len}) : ({val}))",
                        p = Self::js_str(p)
                    )
                });
            }
            let core = if op.starts_with('#') {
                p.strip_prefix('*')
            } else {
                p.strip_suffix('*')
            };
            let core = core?;
            if core.is_empty() || !Self::is_literal_pattern(core) {
                return None;
            }
            let clen = core.chars().count() as i64;
            // `#` (shortest prefix) / `%%` (longest suffix) are the
            // FIRST occurrence; `##` (longest prefix) / `%` (shortest
            // suffix) are the LAST (literal: first == last, but the
            // runtime's indexOf/lastIndexOf mirror keeps it exact).
            let first = op == "#" || op == "%%";
            let ix = if first {
                format!("({val}).indexOf({})", Self::js_str(core))
            } else {
                format!("({val}).lastIndexOf({})", Self::js_str(core))
            };
            return Some(if op.starts_with('#') {
                format!("(({ix}) >= 0 ? ({val}).slice(({ix}) + {clen}) : ({val}))")
            } else {
                format!("(({ix}) >= 0 ? ({val}).slice(0, ({ix})) : ({val}))")
            });
        }
        // `${x/p/r}` — replace the first occurrence; `${x//p/r}` — all
        // occurrences. A `$` in the replacement stays on the runtime
        // (JS replace would interpret `$&`/`$1` sequences); the `//`
        // form then takes the split/join path instead.
        if op == "/" || op == "//" {
            let [_, _, IrExpr::Str(p, _), IrExpr::Str(r, _), ..] = args else {
                return None;
            };
            if !Self::is_literal_pattern(p) {
                return None;
            }
            if r.contains('$') {
                if op == "/" {
                    return None;
                }
                return Some(format!(
                    "(({val}).split({}).join({}))",
                    Self::js_str(p),
                    Self::js_str(r)
                ));
            }
            let m = if op == "/" { "replace" } else { "replaceAll" };
            return Some(format!(
                "({val}).{m}({}, {})",
                Self::js_str(p),
                Self::js_str(r)
            ));
        }
        // `${x:-d}` — default when empty. `${x:=d}` also WRITES the
        // binding (a JS assignment expression; only for a native
        // binding — an undeclared write would throw in strict mode).
        if op == ":-" || op == ":=" {
            let [_, _, d] = args else {
                return None;
            };
            let dflt = self.expr(d);
            if op == ":-" {
                return Some(format!("(({val}) !== \"\" ? ({val}) : {dflt})"));
            }
            if !self.known_vars.contains(name) {
                return None;
            }
            let nm = self.js_ident(name);
            return Some(format!(
                "(({val}) !== \"\" ? ({val}) : ({nm} = {dflt}, {nm}))"
            ));
        }
        // `${x:?msg}` — error when empty: bash prints to stderr and
        // exits 1 (a literal message only).
        if op == ":?" {
            let [_, _, IrExpr::Str(m, _)] = args else {
                return None;
            };
            if m.contains('$') {
                return None;
            }
            let msg = if m.is_empty() {
                format!("{name}: parameter null or not set")
            } else {
                m.clone()
            };
            return Some(format!(
                "(({val}) !== \"\" ? ({val}) : (process.stderr.write({}), process.exit(1)))",
                Self::js_str(&format!("bash: {name}: {msg}\n"))
            ));
        }
        // `${x:off:len}` — substring slice with LITERAL integer
        // offsets (negative counts from the end, like the runtime's
        // v.slice(off, off + len)); non-integer offsets stay on the
        // runtime.
        if op == "slice" {
            let int_of = |t: &str| -> Option<i64> {
                let t = t.trim();
                if t.is_empty() {
                    Some(0)
                } else if t.starts_with('-') {
                    t[1..].parse::<i64>().ok().map(|v| -v)
                } else {
                    t.parse::<i64>().ok()
                }
            };
            let [_, _, IrExpr::Str(off, _), IrExpr::Str(len, _)] = args else {
                return None;
            };
            let o = int_of(off)?;
            if len.trim().is_empty() {
                return Some(format!("({val}).slice({o})"));
            }
            let l = int_of(len)?;
            return Some(format!("({val}).slice({o}, {})", o + l));
        }
        // `${x##*/}` — the parser's basename/dirname ops: trailing-
        // slash strip + last-component split (a missing slash yields
        // the whole path / ".").
        if op == "basename" || op == "dirname" {
            let strip = format!("({val}).replace(/\\/+$/, \"\")");
            return Some(if op == "basename" {
                format!(
                    "(() => {{ const s = {strip}; const i = s.lastIndexOf(\"/\"); return i >= 0 ? s.slice(i + 1) : s; }})()"
                )
            } else {
                format!(
                    "(() => {{ const s = {strip}; const i = s.lastIndexOf(\"/\"); return i >= 0 ? s.slice(0, i) : \".\"; }})()"
                )
            });
        }
        None
    }

    /// `$(echo <literal words>)` — a capture of a literal echo folds to
    /// the captured text (mirrors the estree ref's echo-capture fold;
    /// the capture strips the trailing newline the echo emits).
    fn capture_echo_fold(&mut self, body: &[IrStmt]) -> Option<String> {
        let [IrStmt::Expr(e)] = body else {
            return None;
        };
        let IrExpr::Call { func, args } = e else {
            return None;
        };
        if func != "exec" {
            return None;
        }
        let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else {
            return None;
        };
        if cmd != "echo" {
            return None;
        }
        let mut text = String::new();
        for (i, w) in words.iter().enumerate() {
            if i > 0 {
                text.push(' ');
            }
            match w {
                IrExpr::Str(s, _) => text.push_str(s),
                IrExpr::Interpolate(parts) => {
                    for p in parts {
                        match p {
                            InterpPart::Lit(s) => text.push_str(s),
                            InterpPart::Expr(_) => return None,
                        }
                    }
                }
                _ => return None,
            }
        }
        Some(text)
    }

    /// General capture: run the arrow body with fd-1 redirected into a
    /// buffer (the estree ref runtime's captureSync semantics — NUL
    /// strip + trailing-newline strip), as an IIFE.
    fn capture_iife(&mut self, body: &[IrStmt]) -> String {
        let mut saved = Vec::new();
        std::mem::swap(&mut self.out, &mut saved);
        let saved_depth = self.depth;
        self.depth += 1;
        for s in body {
            self.stmt(s);
        }
        let body_src = std::mem::replace(&mut self.out, saved).join("\n");
        self.depth = saved_depth;
        format!(
            "(() => {{ const __out = []; const __saved = process.stdout.write.bind(process.stdout); process.stdout.write = (s) => {{ __out.push(String(s)); }}; try {{ {} }} finally {{ process.stdout.write = __saved; }} return __out.join(\"\").replace(/\\u0000/g, \"\").replace(/\\n+$/, \"\"); }})()",
            body_src
        )
    }

    /// Mini `[ ... ]` evaluator for the common patterns; None → stub.
    fn test_render(&mut self, s: &str) -> Option<String> {
        let toks: Vec<&str> = s.split_whitespace().collect();
        match toks.as_slice() {
            [a, op, b] => {
                let js_op = match *op {
                    "-gt" => ">",
                    "-lt" => "<",
                    "-ge" => ">=",
                    "-le" => "<=",
                    "-eq" | "=" | "==" => "==",
                    "-ne" | "!=" => "!=",
                    _ => return None,
                };
                // numeric operators coerce both operands (bash: a
                // non-numeric operand is 0)
                if matches!(*op, "-gt" | "-lt" | "-ge" | "-le" | "-eq" | "-ne") {
                    Some(format!(
                        "((Number({}) || 0) {js_op} (Number({}) || 0))",
                        self.test_operand(a),
                        self.test_operand(b)
                    ))
                } else {
                    Some(format!(
                        "({} {js_op} {})",
                        self.test_operand(a),
                        self.test_operand(b)
                    ))
                }
            }
            [flag, v] if *flag == "-n" => Some(format!("({} !== \"\")", self.test_operand(v))),
            [flag, v] if *flag == "-z" => Some(format!("({} === \"\")", self.test_operand(v))),
            // `[ 0 ]` / `[ "" ]` — bash tests the non-emptiness
            [v] => Some(format!("({} !== \"\")", self.test_operand(v))),
            _ => None,
        }
    }

    /// A test operand: `$y` (optionally quoted) → the native var
    /// binding; `$?` → the last-exit status; a number → a literal;
    /// anything else → a quoted string (bash only expands
    /// `$`-prefixed operands).
    fn test_operand(&mut self, t: &str) -> String {
        let t = t.trim().trim_matches('"');
        if let Some(rest) = t.strip_prefix('$') {
            if rest == "?" {
                self.uses_status = true;
                return "sh2_lastExit".into();
            }
            if is_plain_name(rest) {
                return self.js_ident(rest);
            }
            return Self::js_str(t);
        }
        if let Ok(n) = t.parse::<i64>() {
            n.to_string()
        } else {
            Self::js_str(t)
        }
    }

    /// Split an expression into output parts: Lit(text) | Arg(jsexpr, is_num).
    fn parts_of(&mut self, e: &IrExpr) -> Vec<Part> {
        match e {
            IrExpr::Str(s, _) => vec![Part::Lit(s.clone())],
            IrExpr::Int(i) => vec![Part::Arg(i.to_string(), true)],
            IrExpr::Var(name, _) => {
                vec![Part::Arg(self.js_ident(name), self.is_num(name))]
            }
            IrExpr::Ident(name) => vec![Part::Arg(self.js_ident(name), false)],
            IrExpr::Interpolate(parts) => {
                let mut out = Vec::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => out.push(Part::Lit(s.clone())),
                        InterpPart::Expr(x) => {
                            out.push(Part::Arg(self.expr(x), self.expr_is_num(x)))
                        }
                    }
                }
                out
            }
            IrExpr::Arith(a) => vec![Part::Arg(self.arith(a), true)],
            IrExpr::BinOp { .. } => vec![Part::Arg(self.expr(e), true)],
            // `$y` reads arrive as getVar("y"); known vars lower to the
            // native binding, unset reads to "" (the estree ref's
            // SH2_ASSUME_NO_ENV fold)
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if let Some(x) = self.getvar_expr(name) {
                        return vec![Part::Arg(x, self.expr_is_num(e))];
                    }
                }
                self.sh2_calls.insert("getVar".into());
                self.mark_todo("echo arg getVar");
                vec![Part::Arg("sh2_getVar()".into(), false)]
            }
            // `split(...)` — the A1's word-split marker on a read in
            // echo position; the estree ref folds it to the read itself
            IrExpr::Call { func, args } if func == "split" => {
                if let Some(x) = args.first() {
                    return self.parts_of(x);
                }
                self.mark_todo("echo arg split");
                vec![Part::Arg("0".into(), true)]
            }
            other => {
                self.mark_todo(&format!("echo arg {:?}", other));
                vec![Part::Arg("0".into(), true)]
            }
        }
    }

    fn expr_is_num(&mut self, e: &IrExpr) -> bool {
        match e {
            IrExpr::Var(name, _) => self.is_num(name),
            IrExpr::Int(_) | IrExpr::Arith(_) | IrExpr::BinOp { .. } => true,
            // `$y` reads arrive as getVar("y"); a typed-Int var is numeric
            IrExpr::Call { func, args } if func == "getVar" => {
                matches!(args.first(), Some(IrExpr::Str(name, _)) if self.is_num(name))
            }
            _ => false,
        }
    }

    /// `parts` → a single `process.stdout.write(<concat>)` expression.
    /// JS's dynamic typing makes the C renderer's %lld/%s split
    /// unnecessary — plain `+` concatenation coerces strings — but a
    /// NUMERIC part must be String()-coerced first: `$i$j` with two
    /// Int-typed vars would otherwise ADD (bash: string concatenation;
    /// the estree ref String()s every non-literal echo arg).
    fn write_from_parts(&mut self, parts: Vec<Part>) -> String {
        let mut exprs: Vec<String> = Vec::new();
        let mut lit = String::new();
        for p in parts {
            match p {
                Part::Lit(t) => lit.push_str(&t),
                Part::Arg(v, is_num) => {
                    if !lit.is_empty() {
                        exprs.push(Self::js_str(&lit));
                        lit.clear();
                    }
                    if is_num {
                        exprs.push(format!("String({v})"));
                    } else {
                        exprs.push(v);
                    }
                }
            }
        }
        if !lit.is_empty() {
            exprs.push(Self::js_str(&lit));
        }
        let joined = if exprs.is_empty() {
            "".to_string()
        } else if exprs.len() == 1 {
            exprs[0].clone()
        } else {
            format!("({})", exprs.join(" + "))
        };
        format!("process.stdout.write({joined})")
    }

    /// exec("printf", [fmt, args...]) → native process.stdout.write with
    /// the %s/%d/%i/%u conversions (the C-family frontends' stdout —
    /// cpp-sh-go t30_static_assert.cc, t29_goto.cc). Complex specs
    /// (flags/width/precision) and unknown conversions fall back to the
    /// sh2.exec stub.
    fn printf_call(&mut self, args: &[IrExpr]) -> String {
        let Some(IrExpr::Array(items)) = args.get(1) else {
            return self.sh2_stub("exec", args, "exec");
        };
        let Some(IrExpr::Str(fmt, _)) = items.first() else {
            return self.sh2_stub("exec", args, "exec");
        };
        let parsed = match Self::printf_parse(fmt) {
            Some(p) => p,
            None => return self.sh2_stub("exec", args, "exec"),
        };
        let (els, n_specs) = parsed;
        let fmt_args: Vec<&IrExpr> = items[1..].iter().collect();
        if fmt_args.iter().any(|a| matches!(a, IrExpr::Array(_))) {
            return self.sh2_stub("exec", args, "exec");
        }
        let arg_exprs: Vec<String> = fmt_args.iter().map(|a| self.expr(a)).collect();
        // a spec with flags/width/prec must keep the runtime builtin
        let complex = els.iter().any(|(_, s)| match s {
            Some((flags, width, prec, _)) => {
                !flags.is_empty() || *width > 0 || prec.is_some()
            }
            None => false,
        });
        if complex {
            return self.sh2_stub("exec", args, "exec");
        }
        let passes = if n_specs == 0 {
            arg_exprs.len().max(1)
        } else if arg_exprs.is_empty() {
            1
        } else {
            (arg_exprs.len() + n_specs - 1) / n_specs
        };
        let mut pieces: Vec<String> = Vec::new();
        if n_specs == 0 {
            // no specs: the format text repeats once per arg
            let text = Self::js_str(&Self::printf_unescape(fmt));
            if passes > 1 {
                pieces.push(format!("({text}).repeat({passes})"));
            } else {
                pieces.push(text);
            }
        } else {
            let mut ai = 0usize;
            for _pass in 0..passes {
                for (text, spec) in &els {
                    if let Some((_, _, _, conv)) = spec {
                        let arg = arg_exprs.get(ai).cloned().unwrap_or_else(|| "\"\"".into());
                        ai += 1;
                        match conv {
                            's' => pieces.push(format!("String({arg})")),
                            'd' | 'i' | 'u' => {
                                // parseInt(s, 10) || 0 — bash coerces a
                                // non-numeric arg to 0
                                pieces.push(format!("String(parseInt({arg}, 10) || 0)"));
                            }
                            _ => unreachable!("printf_parse gates the conversions"),
                        }
                    } else {
                        pieces.push(Self::js_str(&Self::printf_unescape(text)));
                    }
                }
            }
        }
        format!("process.stdout.write({})", pieces.join(" + "))
    }

    /// printf(1) format parsing — mirrors python_backend::printf_parse
    /// (the flags/width/precision scan; %s/%d/%i/%u conversions).
    fn printf_parse(fmt: &str) -> Option<(Vec<(String, Option<(String, usize, Option<usize>, char)>)>, usize)> {
        let chars: Vec<char> = fmt.chars().collect();
        let mut els: Vec<(String, Option<(String, usize, Option<usize>, char)>)> = Vec::new();
        let mut text = String::new();
        let mut pos = 0usize;
        let mut n_specs = 0usize;
        while pos < chars.len() {
            if chars[pos] == '%' {
                let mut i = pos + 1;
                while i < chars.len() && matches!(chars[i], '-' | '+' | ' ' | '0' | '#') {
                    i += 1;
                }
                let flags: String = chars[pos + 1..i].iter().collect();
                let wstart = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let width: usize = if i > wstart {
                    chars[wstart..i].iter().collect::<String>().parse().ok()?
                } else {
                    0
                };
                let mut prec = None;
                if i < chars.len() && chars[i] == '.' {
                    i += 1;
                    let pstart = i;
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                    if i > pstart {
                        prec = Some(chars[pstart..i].iter().collect::<String>().parse().ok()?);
                    } else {
                        return None;
                    }
                }
                while i < chars.len() && (chars[i] == 'l' || chars[i] == 'L') {
                    i += 1;
                }
                if i >= chars.len() {
                    return None;
                }
                let conv = chars[i];
                if conv == '%' {
                    text.push('%');
                } else if matches!(conv, 's' | 'd' | 'i' | 'u') {
                    if !text.is_empty() {
                        els.push((std::mem::take(&mut text), None));
                    }
                    els.push((String::new(), Some((flags, width, prec, conv))));
                    n_specs += 1;
                } else {
                    return None;
                }
                pos = i + 1;
                continue;
            }
            text.push(chars[pos]);
            pos += 1;
        }
        if !text.is_empty() {
            els.push((text, None));
        }
        Some((els, n_specs))
    }

    /// Text-run backslash escapes (\n \t \r \a \b \f \v and octal).
    fn printf_unescape(s: &str) -> String {
        let mut out = s.to_string();
        for (from, to) in [
            ("\\n", "\n"),
            ("\\t", "\t"),
            ("\\r", "\r"),
            ("\\a", "\x07"),
            ("\\b", "\x08"),
            ("\\f", "\x0c"),
            ("\\v", "\x0b"),
            ("\\\\", "\\"),
        ] {
            out = out.replace(from, to);
        }
        let chars: Vec<char> = out.chars().collect();
        let mut res = String::with_capacity(out.len());
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '\\' && i + 1 < chars.len() {
                let mut oct = String::new();
                let mut j = i + 1;
                while j < chars.len() && oct.len() < 3 && matches!(chars[j], '0'..='7') {
                    oct.push(chars[j]);
                    j += 1;
                }
                if !oct.is_empty() {
                    if let Some(c) = u32::from_str_radix(&oct, 8).ok().and_then(char::from_u32) {
                        res.push(c);
                        i = j;
                        continue;
                    }
                }
            }
            res.push(chars[i]);
            i += 1;
        }
        res
    }

    // ── statements ───────────────────────────────────────────────────

    fn stmt(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Expr(e) => {
                // statement-position arith: `i++` / `x += n` (the
                // C-style frontends' loop steps) render natively — the
                // arith() expression path can only stub them (no JS
                // expression-assignment) — cpp-sh-go t29_goto.cc's
                // backward-goto loop step.
                if let IrExpr::Arith(a) = e {
                    if let ArithAst::IncDec { var, delta, .. } = &**a {
                        let name = self.js_ident(var);
                        let d = delta.unsigned_abs();
                        let sign = if *delta >= 0 { "+" } else { "-" };
                        if self.is_num(var) {
                            self.emit(&format!("{name} {sign}= {d};"));
                        } else {
                            // untyped (shell) vars hold strings: coerce
                            // like the arith() Var arm (Number(x) || 0)
                            self.emit(&format!(
                                "{name} = (Number({name}) || 0) {sign} {d};"
                            ));
                        }
                        return;
                    }
                    if let ArithAst::Assign { var, op, rhs } = &**a {
                        let name = self.js_ident(var);
                        let r = self.arith(rhs);
                        let js_op = match op.as_str() {
                            "+=" => "+=",
                            "-=" => "-=",
                            "*=" => "*=",
                            "/=" => "/=",
                            "%=" => "%=",
                            _ => "=",
                        };
                        if self.is_num(var) || js_op == "=" {
                            self.emit(&format!("{name} {js_op} {r};"));
                        } else {
                            self.emit(&format!(
                                "{name} = (Number({name}) || 0) {} {r};",
                                js_op.trim_end_matches('=')
                            ));
                        }
                        return;
                    }
                }
                // setVar("name", value) — the A1's store write; a plain
                // name lowers to a native assignment (typed targets keep
                // their numeric binding)
                if let IrExpr::Call { func, args } = e {
                    if func == "setVar" {
                        if let (Some(IrExpr::Str(name, _)), Some(value)) =
                            (args.first(), args.get(1))
                        {
                            if is_plain_name(name) {
                                let nm = self.js_ident(name);
                                let v = if self.is_num(name) {
                                    self.expr_as_num(value)
                                } else {
                                    self.expr(value)
                                };
                                self.emit(&format!("{nm} = {v};"));
                                return;
                            }
                        }
                    }
                    // setArray("name", [...]) — a C array literal
                    if func == "setArray" {
                        if let (Some(IrExpr::Str(name, _)), Some(IrExpr::Array(items))) =
                            (args.first(), args.get(1))
                        {
                            if is_plain_name(name) {
                                let nm = self.js_ident(name);
                                let elems: Vec<String> =
                                    items.iter().map(|e2| self.expr(e2)).collect();
                                self.emit(&format!("{nm} = [{}];", elems.join(", ")));
                                return;
                            }
                        }
                    }
                }
                // break()/continue() calls inside a loop lower natively
                // (bash status verbs — the goto-restructure pass emits
                // them as the loop exit); outside a loop they keep the stub
                if let IrExpr::Call { func, .. } = e {
                    if self.loop_depth > 0 {
                        if func == "break" {
                            self.emit("break;");
                            return;
                        }
                        if func == "continue" {
                            self.emit("continue;");
                            return;
                        }
                    }
                }
                // exec statements set the exit status (`$?` — the estree
                // ref's sh2.lastExit protocol): the known natives fold to
                // a constant; an unknown external keeps the previous
                // status (its sh2.* stub aborts anyway).
                if let IrExpr::Call { func, args } = e {
                    if func == "exec" {
                        if let Some(IrExpr::Str(cmd, _)) = args.first() {
                            let status = match cmd.as_str() {
                                "false" => Some("1"),
                                "true" | "echo" | "printf" => Some("0"),
                                _ => None,
                            };
                            if let Some(st) = status {
                                let x = self.expr(e);
                                self.emit(&format!("{x};"));
                                self.uses_status = true;
                                self.emit(&format!("sh2_lastExit = {st};"));
                                return;
                            }
                        }
                    }
                }
                let x = self.expr(e);
                self.emit(&format!("{x};"));
            }
            IrStmt::Assign { targets, expr, asm, .. } => {
                // Declarator-position asm label (core request
                // c-sh-go-toplevelasmargument-20260814-042952) — no JS
                // rendering here (the estree path no-ops it); refuse
                // loudly (refuse > guess).
                if let Some(spec) = asm {
                    self.mark_todo(&format!("asm label '{}' on an assign", spec.template));
                    return;
                }
                let Some(t) = targets.first() else {
                    self.mark_todo("multi-target assign");
                    return;
                };
                if !t.indices.is_empty() {
                    self.mark_todo("array-index assign");
                    return;
                }
                // `s += n` arriving as a plain Assign (the C/zsh
                // frontends' arith-step shape) — mirror the IrStmt::Expr
                // arm's native lowering instead of the sh2.arith stub
                if let IrExpr::Arith(a) = expr {
                    if let ArithAst::Assign { var, op, rhs } = &**a {
                        if t.var == *var {
                            let name = self.js_ident(var);
                            let r = self.arith(rhs);
                            let js_op = match op.as_str() {
                                "+=" => "+=",
                                "-=" => "-=",
                                "*=" => "*=",
                                "/=" => "/=",
                                "%=" => "%=",
                                _ => "=",
                            };
                            if self.is_num(var) || js_op == "=" {
                                self.emit(&format!("{name} {js_op} {r};"));
                            } else {
                                self.emit(&format!(
                                    "{name} = (Number({name}) || 0) {} {r};",
                                    js_op.trim_end_matches('=')
                                ));
                            }
                            return;
                        }
                    }
                }
                let name = self.js_ident(&t.var);
                let is_num = self.is_num(&t.var);
                let rhs = if is_num {
                    self.expr_as_num(expr)
                } else {
                    self.expr(expr)
                };
                self.emit(&format!("{name} = {rhs};"));
            }
            IrStmt::Declare { vars, init, .. } => {
                // vars are hoisted as `let` in main(); Declare lowers to
                // an assignment (or nothing when there is no init).
                if let Some(init_expr) = init {
                    for d in vars {
                        let name = self.js_ident(&d.name);
                        let v = if self.is_num(&d.name) {
                            self.expr_as_num(init_expr)
                        } else {
                            self.expr(init_expr)
                        };
                        self.emit(&format!("{name} = {v};"));
                    }
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                let name = self.js_ident(var);
                let elems: Vec<String> = elements.iter().map(|e| self.expr(e)).collect();
                self.emit(&format!("{name} = [{}];", elems.join(", ")));
            }
            IrStmt::Output {
                value,
                newline,
                target,
            } => {
                let v = self.expr(value);
                if target.is_some() {
                    self.mark_todo("filehandle output");
                    return;
                }
                if *newline {
                    self.emit(&format!("console.log({v});"));
                } else {
                    self.emit(&format!("process.stdout.write(String({v}));"));
                }
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                let c = self.expr(cond);
                self.emit(&format!("if ({c}) {{"));
                self.depth += 1;
                for s in then {
                    self.stmt(s);
                }
                self.depth -= 1;
                for (ec, body) in elsifs {
                    let ec = self.expr(ec);
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
            IrStmt::For { var, iter, body } => {
                let name = self.js_ident(var);
                let it = self.expr(iter);
                self.emit(&format!("for (let {name} of {it}) {{"));
                self.depth += 1;
                if self.is_num(var) {
                    // the for-of yields STRINGS; a numeric-lifted loop
                    // var must be coerced before arith uses it (the
                    // estree ref emits the same `n = Number(n)` step)
                    self.emit(&format!("{name} = Number({name});"));
                }
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::While { cond, body } => {
                let c = self.expr(cond);
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
                let c = self.expr(cond);
                if *until {
                    // until → while (!cond)
                    self.emit(&format!("while (!({c})) {{"));
                    self.depth += 1;
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                } else {
                    self.emit("do {");
                    self.depth += 1;
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit(&format!("}} while ({c});"));
                }
            }
            IrStmt::Block(b) => {
                self.emit("{");
                self.depth += 1;
                for s in b {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::Exit(e) => {
                let code = e
                    .as_ref()
                    .map(|x| self.expr(x))
                    .unwrap_or_else(|| "0".into());
                self.emit(&format!("process.exit({code});"));
            }
            IrStmt::Return(e) => match e {
                Some(x) => {
                    let v = self.expr(x);
                    self.emit(&format!("return {v};"));
                }
                None => self.emit("return;"),
            },
            IrStmt::Function { name, body, .. } => {
                let fname = self.js_ident(name);
                self.emit(&format!("function {fname}(..._args) {{"));
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::Break => {
                // the restructure pass's inverted guarded goto
                // (`while (true) { … if (c) {} else { break } }`) lowers
                // to a plain break — cpp-sh-go t29_goto.cc
                if self.loop_depth > 0 {
                    self.emit("break;");
                } else {
                    self.mark_todo("top-level break");
                }
            }
            IrStmt::Continue => {
                if self.loop_depth > 0 {
                    self.emit("continue;");
                } else {
                    self.mark_todo("top-level continue");
                }
            }
            // try/except/else/finally (py-sh-go try_stmt) → JS
            // try/catch/finally, mirroring the estree ref's TryStatement
            // lowering: the except arms are an if/else-if ladder over
            // `e instanceof <match>` INSIDE a single catch (a match-less
            // arm is the ladder's terminal else; a ladder that matches
            // nothing rethrows `e`), preceded by the signal guard — the
            // runtime's BREAK/CONTINUE/RETURN control signals are NOT
            // Error objects and must pass through a Try untouched (a
            // bare `except:` must not swallow a `return`). `as` binds
            // the caught value to the native var (the arm body's getVar
            // reads see it). Python `else` runs only when the try body
            // completed WITHOUT raising — even a HANDLED exception
            // skips it — and else-body exceptions are NOT caught by
            // this statement's arms, so the else suite sits behind a
            // completion flag (`__sh2else`); `finally` is the JS
            // finalizer.
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                if excepts.is_empty() && finally_body.is_empty() {
                    // no handler: the try adds nothing — emit the suites
                    // plainly (else just follows the body), like estree
                    for s in body {
                        self.stmt(s);
                    }
                    for s in else_body {
                        self.stmt(s);
                    }
                    return;
                }
                if excepts.is_empty() {
                    // no arms to protect the else from: else-body
                    // exceptions propagate anyway (nothing catches
                    // them), so the plain sequential form is exact
                    self.emit("try {");
                    self.depth += 1;
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    if finally_body.is_empty() {
                        self.emit("}");
                    } else {
                        self.emit("} finally {");
                        self.depth += 1;
                        for s in finally_body {
                            self.stmt(s);
                        }
                        self.depth -= 1;
                        self.emit("}");
                    }
                    for s in else_body {
                        self.stmt(s);
                    }
                    return;
                }
                // arms present: the try/catch (+ possibly the flag-gated
                // else, + the outer finalizer when both else and finally
                // exist — python runs the else BEFORE the finally, and a
                // JS finalizer fires when the whole statement ends).
                let flag = "__sh2else";
                let need_flag = !else_body.is_empty();
                let outer = need_flag && !finally_body.is_empty();
                if outer {
                    self.emit("try {");
                    self.depth += 1;
                }
                if need_flag {
                    self.emit(&format!("let {flag} = false;"));
                }
                self.emit("try {");
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                if need_flag {
                    self.emit(&format!("{flag} = true;"));
                }
                self.depth -= 1;
                self.emit("} catch (e) {");
                self.depth += 1;
                self.emit("if (!(e instanceof Error)) throw e;");
                self.try_ladder(excepts);
                self.depth -= 1;
                if !finally_body.is_empty() && !outer {
                    // the finalizer attaches to the try/catch: the
                    // catch-close `}` doubles as the `finally` opener
                    self.emit("} finally {");
                    self.depth += 1;
                    for s in finally_body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                } else {
                    self.emit("}");
                }
                if need_flag {
                    self.emit(&format!("if ({flag}) {{"));
                    self.depth += 1;
                    for s in else_body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                }
                if outer {
                    // close the OUTER try block (the inner try/catch
                    // is already closed); the finalizer runs after
                    // the flag-gated else — python's order
                    self.depth -= 1;
                    self.emit("} finally {");
                    self.depth += 1;
                    for s in finally_body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                }
            }
            other => self.mark_todo(&format!("stmt {:?}", other)),
        }
    }

    /// The except-arm ladder inside a catch (the IrStmt::Try arm):
    /// `e instanceof <match>` ifs in source order; a match-less (bare)
    /// arm is the ladder's terminal else — later arms are dead in
    /// Python, nested inside its block (unreachable but still emitted,
    /// estree parity); a match-ladder that matches nothing rethrows
    /// `e`. `as` bindings write the caught value first.
    fn try_ladder(&mut self, excepts: &[crate::ir::TryExcept]) {
        // the bare arm index (scan from the end: the LAST bare arm; the
        // source arms after it are the dead tail nested in its block)
        let bare = excepts.iter().rposition(|e| e.match_expr.is_none());
        for (i, e) in excepts.iter().enumerate() {
            let last_live = match bare {
                Some(b) => i + 1 == b,
                None => i + 1 == excepts.len(),
            };
            if let Some(m) = &e.match_expr {
                let m = self.expr(m);
                self.emit(&format!("if (e instanceof {m}) {{"));
            } else {
                self.emit("else {");
            }
            self.depth += 1;
            if let Some(asn) = &e.as_name {
                let nm = self.js_ident(asn);
                self.emit(&format!("{nm} = e;"));
            }
            for s in &e.body {
                self.stmt(s);
            }
            if Some(i) == bare {
                // dead arms after the bare one (unreachable python
                // syntax) — nested blocks keep the code compile-able
                for dead in &excepts[i + 1..] {
                    self.emit("{");
                    self.depth += 1;
                    for s in &dead.body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                }
            }
            self.depth -= 1;
            if let Some(m) = &e.match_expr {
                if last_live {
                    // the last live arm: a non-matching exception has
                    // nowhere else to go — rethrow (or the bare arm
                    // follows as the terminal else)
                    if bare.is_some() {
                        self.emit("}");
                    } else {
                        self.emit("} else {");
                        self.depth += 1;
                        self.emit("throw e;");
                        self.depth -= 1;
                        self.emit("}");
                    }
                } else {
                    self.emit("} else {");
                }
            } else {
                self.emit("}");
            }
        }
    }

    /// Render an expression as a JS number (Int-typed assignment target).
    /// Numeric literals arrive as `Str("5")` in the ShIR (x=5); parse them
    /// so the Int var stays a native number.
    fn expr_as_num(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => {
                if let Ok(n) = s.trim().parse::<i64>() {
                    n.to_string()
                } else {
                    format!("(Number({}) || 0)", Self::js_str(s))
                }
            }
            IrExpr::Int(i) => i.to_string(),
            _ => self.expr(e),
        }
    }

    // ── program ──────────────────────────────────────────────────────

    fn program(&mut self, prog: &IrProgram) {
        // Pass 1: collect declared vars (assign targets, declare lists,
        // Var reads, for-loop vars) so declarations can be hoisted.
        let mut vars: BTreeSet<String> = BTreeSet::new();
        collect_vars(&prog.stmts, &mut vars);
        for sub in &prog.subs {
            collect_vars(&sub.body, &mut vars);
        }
        for (n, _) in &prog.var_types {
            vars.insert(n.clone());
        }
        self.known_vars = vars.clone();

        // Pass 2: render the body first (helper flags known before preamble).
        let mut body_out = Vec::new();
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 1;
        // Indexed assign targets arrive as `map[foo]` whole names — hoist
        // the base name as an object so `map[foo] = …` index writes work.
        // A name used both ways keeps the object (JS objects accept both).
        let mut obj_vars: BTreeSet<String> = BTreeSet::new();
        for v in &vars {
            if let Some(base) = v.split('[').next() {
                if base != v {
                    obj_vars.insert(base.to_string());
                }
            }
        }
        let plain_vars: BTreeSet<String> = vars
            .iter()
            .filter(|v| !v.contains('[') && !obj_vars.contains(*v))
            .cloned()
            .collect();
        for v in &obj_vars {
            let name = self.js_ident(v);
            self.emit(&format!("let {name} = {{}};"));
        }
        for v in &plain_vars {
            let name = self.js_ident(v);
            if self.is_num(v) {
                self.emit(&format!("let {name} = 0;"));
            } else {
                self.emit(&format!("let {name} = \"\";"));
            }
        }
        if !vars.is_empty() {
            self.emit("");
        }
        for s in &prog.stmts {
            self.stmt(s);
        }
        if !prog.subs.is_empty() {
            self.emit("");
        }
        for sub in &prog.subs {
            let fname = self.js_ident(&sub.name);
            let params: Vec<String> = sub.params.iter().map(|p| self.js_ident(p)).collect();
            self.emit(&format!("function {fname}({}) {{", params.join(", ")));
            self.depth += 1;
            for s in &sub.body {
                self.stmt(s);
            }
            self.depth -= 1;
            self.emit("}");
        }
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 0;

        // Preamble: shebang + strict, then the sh2.* stubs, then main()
        // with the rendered body, then the call.
        self.emit("#!/usr/bin/env node");
        self.emit("\"use strict\";");
        self.emit("");
        if self.uses_status {
            self.emit("let sh2_lastExit = 0;");
            self.emit("");
        }
        if !self.sh2_calls.is_empty() {
            self.emit("/* sh2.* runtime stubs — TODO: implement (harness/sh2-namespace.json) */");
            let names: Vec<String> = self.sh2_calls.iter().cloned().collect();
            for name in names {
                let fname = format!("sh2_{}", name.replace('.', "_"));
                self.emit(&format!("function {fname}(..._args) {{"));
                self.emit(&format!("  console.error(\"TODO sh2.{name}\");"));
                self.emit("  process.exit(2);");
                self.emit("}");
            }
            self.emit("");
        }
        if self.uses_positional {
            self.emit("let sh2_positional = [];");
        }
        if self.uses_mem {
            self.emit(MEM_PREAMBLE);
        }
        self.emit("function main() {");
        self.out.extend(body_out.iter().cloned());
        self.emit("}");
        self.emit("");
        self.emit("main();");
        if self.todo > 0 {
            self.emit(&format!(
                "/* {} construct(s) lowered to TODO markers */",
                self.todo
            ));
        }
    }
}

/// C memory arena helpers — the estree ref runtime's mem* slice-2
/// (harness/sh2-namespace.mjs), standalone: a flat byte-slot arena keyed
/// by allocation id, handles as `\u0001mem:<id>:<offset>` tagged strings,
/// element offsets scaled by the type's size at load/store.
const MEM_PREAMBLE: &str = r#"/* C memory arena (the estree ref runtime's mem* slice-2, standalone) */
let sh2_memSeq = 0;
let sh2_memArena = {};
function sh2_memPos(h) {
    const m = /^\u0001mem:([^:]+):(-?\d+)$/.exec(String(h ?? ""));
    if (m) return Number(m[2]) || 0;
    return Number(h) || 0;
}
function sh2_memElemSize(type) {
    if (typeof type === "number") return Math.max(1, Math.floor(type));
    const t = String(type ?? "int");
    const sizes = { char: 1, "signed char": 1, "unsigned char": 1, short: 2, "short int": 2, int: 4, "unsigned int": 4, unsigned: 4, long: 8, "long int": 8, "long long": 8, "unsigned long": 8, "unsigned long long": 8, float: 4, double: 8, "void*": 8, ptr: 8, pointer: 8, int8: 1, int16: 2, int32: 4, int64: 8, u32: 4, u64: 8 };
    return sizes[t] ?? 1;
}
function sh2_memArenaOf(h) {
    const m = /^\u0001mem:([^:]+):(-?\d+)$/.exec(String(h ?? ""));
    if (!m) return null;
    const id = m[1];
    if (!/^\d+$/.test(id)) return null;
    return (sh2_memArena ?? {})[Number(id)] ?? null;
}
function sh2_memAlloc(size) {
    const id = (sh2_memSeq += 1);
    const n = Math.max(0, Math.floor(Number(size) || 0));
    sh2_memArena[id] = new Array(n).fill(0);
    return "\u0001mem:" + id + ":0";
}
function sh2_memLoad(h, offset, type) {
    const a = sh2_memArenaOf(h);
    if (!a) return "";
    const i = (sh2_memPos(h) + (Number(offset) || 0)) * sh2_memElemSize(type);
    return i >= 0 && i < a.length ? String(a[i]) : "";
}
function sh2_memStore(h, offset, type, v) {
    const a = sh2_memArenaOf(h);
    if (!a) return;
    const i = (sh2_memPos(h) + (Number(offset) || 0)) * sh2_memElemSize(type);
    if (i >= 0 && i < a.length) a[i] = String(v ?? "");
}
function sh2_memAdvance(h, n) {
    const m = /^(\u0001mem:[^:]+):(-?\d+)$/.exec(String(h ?? ""));
    if (!m) return h;
    return m[1] + ":" + (sh2_memPos(h) + (Number(n) || 0));
}
function sh2_memFree(h) {
    const m = /^\u0001mem:([^:]+):(-?\d+)$/.exec(String(h ?? ""));
    if (!m || !/^\d+$/.test(m[1])) return;
    delete sh2_memArena[Number(m[1])];
}
function sh2_memTest(op, a, b) {
    const pa = sh2_memPos(a);
    const pb = sh2_memPos(b);
    switch (op) { case "<": return pa < pb; case "<=": return pa <= pb; case ">": return pa > pb; case ">=": return pa >= pb; case "==": return pa === pb; case "!=": return pa !== pb; default: return false; }
}"#;

fn collect_vars(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                for t in targets {
                    out.insert(t.var.clone());
                }
                collect_vars_expr(expr, out);
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    out.insert(d.name.clone());
                }
                if let Some(e) = init {
                    collect_vars_expr(e, out);
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                out.insert(var.clone());
                for e in elements {
                    collect_vars_expr(e, out);
                }
            }
            IrStmt::Expr(e) => collect_vars_expr(e, out),
            IrStmt::Output { value, .. } => collect_vars_expr(value, out),
            IrStmt::WriteFile { path, content, .. } => {
                collect_vars_expr(path, out);
                collect_vars_expr(content, out);
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                collect_vars_expr(cond, out);
                collect_vars(then, out);
                for (c, b) in elsifs {
                    collect_vars_expr(c, out);
                    collect_vars(b, out);
                }
                collect_vars(else_, out);
            }
            IrStmt::For { var, iter, body } => {
                out.insert(var.clone());
                collect_vars_expr(iter, out);
                collect_vars(body, out);
            }
            IrStmt::While { cond, body } => {
                collect_vars_expr(cond, out);
                collect_vars(body, out);
            }
            IrStmt::DoWhile { body, cond, .. } => {
                collect_vars(body, out);
                collect_vars_expr(cond, out);
            }
            IrStmt::Exit(e) => {
                if let Some(x) = e {
                    collect_vars_expr(x, out);
                }
            }
            IrStmt::Return(e) => {
                if let Some(x) = e {
                    collect_vars_expr(x, out);
                }
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
                collect_vars_expr(expr, out);
            }
            IrStmt::Exec { cmd, args, .. } => {
                collect_vars_expr(cmd, out);
                for a in args {
                    collect_vars_expr(a, out);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    collect_vars(stage, out);
                }
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                collect_vars_expr(discriminant, out);
                for c in clauses {
                    collect_vars(&c.body, out);
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                collect_vars(inner, out);
                for r in redirects {
                    collect_vars_expr(&r.target, out);
                }
            }
            IrStmt::Function { body, .. } => collect_vars(body, out),
            IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => collect_vars(b, out),
            // try/except: the `as` binding writes the caught value into
            // the native var (the arm body's getVar reads must see it)
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                collect_vars(body, out);
                for e in excepts {
                    if let Some(asn) = &e.as_name {
                        out.insert(asn.clone());
                    }
                    collect_vars(&e.body, out);
                }
                collect_vars(else_body, out);
                collect_vars(finally_body, out);
            }
            _ => {}
        }
    }
}

fn collect_vars_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Var(name, _) => {
            out.insert(name.clone());
        }
        IrExpr::Index { var, key } => {
            out.insert(var.clone());
            collect_vars_expr(key, out);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_vars_expr(lhs, out);
            collect_vars_expr(rhs, out);
        }
        IrExpr::Arith(a) => collect_vars_arith(a, out),
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_vars_expr(x, out);
                }
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_vars_expr(i, out);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            collect_vars_expr(cond, out);
            collect_vars_expr(then, out);
            collect_vars_expr(else_, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_vars_expr(expr, out);
            collect_vars_expr(default, out);
        }
        IrExpr::Call { func, args } => {
            // setVar/setArray carry the STORE name as their first Str
            // arg — collect it so the var is declared and getVar reads
            // of it fold to the native binding
            if matches!(func.as_str(), "setVar" | "setArray") {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    out.insert(name.clone());
                }
            }
            for a in args {
                collect_vars_expr(a, out);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            collect_vars_expr(obj, out);
            for a in args {
                collect_vars_expr(a, out);
            }
        }
        IrExpr::Capture { expr, .. } => collect_vars_expr(expr, out),
        IrExpr::Arrow(body) => collect_vars(body, out),
        IrExpr::Object(pairs) => {
            for (_, v) in pairs {
                collect_vars_expr(v, out);
            }
        }
        _ => {}
    }
}

fn collect_vars_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Num(_) => {}
        ArithAst::Var(name) | ArithAst::Ident(name) => {
            out.insert(name.clone());
        }
        ArithAst::Index { var, key } => {
            out.insert(var.clone());
            collect_vars_arith(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_vars_arith(lhs, out);
            collect_vars_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => collect_vars_arith(arg, out),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            collect_vars_arith(test, out);
            collect_vars_arith(then, out);
            collect_vars_arith(else_, out);
        }
        ArithAst::Assign { var, rhs, .. } => {
            out.insert(var.clone());
            collect_vars_arith(rhs, out);
        }
        ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        ArithAst::Num(_) => {}
        ArithAst::Sizeof(_) => {}
        ArithAst::Cast { arg, .. } => collect_vars_arith(arg, out),
    }
}
