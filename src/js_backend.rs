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
    /// distinct sh2.* callee names that need stubs
    sh2_calls: BTreeSet<String>,
    todo: usize,
}

/// Render an `IrProgram` to a Node-style JS script.
pub fn shir_to_js(prog: &IrProgram) -> String {
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
            IrExpr::Call { func, args } => self.call(func, args),
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

    /// String interpolation: `"a$xb"` → `("a" + x + "b")` — plain `+`
    /// concatenation (JS coerces numbers to strings mid-concat).
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
                    exprs.push(self.expr(x));
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
                    IrType::Int64 => format!("BigInt.asIntN(64, BigInt({inner}))"),
                    IrType::UInt64 => format!("BigInt.asUintN(64, BigInt({inner}))"),
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
                }
                self.sh2_stub("exec", args, "exec")
            }
            // getVar("y") — the ShIR's form of a `$y` read; typed vars
            // lower to bare identifiers, mirroring the C renderer.
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.var_types.contains_key(name) {
                        return self.js_ident(name);
                    }
                }
                self.sh2_stub("getVar", args, "getVar")
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
                Some(format!(
                    "({} {js_op} {})",
                    self.test_value(a),
                    self.test_value(b)
                ))
            }
            [flag, v] if *flag == "-n" => Some(format!("({} !== \"\")", self.test_value(v))),
            [flag, v] if *flag == "-z" => Some(format!("({} === \"\")", self.test_value(v))),
            [v] => Some(format!("({})", self.test_value(v))),
            _ => None,
        }
    }

    /// A test operand: `"$y"`/`$y`/`y` (typed var) → ident; number →
    /// literal; otherwise a quoted string.
    fn test_value(&self, t: &str) -> String {
        let t = t
            .trim()
            .trim_matches('"')
            .strip_prefix('$')
            .unwrap_or(t.trim().trim_matches('"'));
        if self.var_types.contains_key(t) {
            self.js_ident(t)
        } else if let Ok(n) = t.parse::<i64>() {
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
            // `$y` reads arrive as getVar("y"); typed vars lower to the
            // bare identifier, anything else → runtime stub.
            IrExpr::Call { func, args } if func == "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.var_types.contains_key(name) {
                        return vec![Part::Arg(self.js_ident(name), self.is_num(name))];
                    }
                }
                self.sh2_calls.insert("getVar".into());
                self.mark_todo("echo arg getVar");
                vec![Part::Arg("sh2_getVar()".into(), false)]
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
    /// unnecessary — plain `+` concatenation coerces numbers.
    fn write_from_parts(&mut self, parts: Vec<Part>) -> String {
        let mut exprs: Vec<String> = Vec::new();
        let mut lit = String::new();
        for p in parts {
            match p {
                Part::Lit(t) => lit.push_str(&t),
                Part::Arg(v, _is_num) => {
                    if !lit.is_empty() {
                        exprs.push(Self::js_str(&lit));
                        lit.clear();
                    }
                    exprs.push(v);
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

    // ── statements ───────────────────────────────────────────────────

    fn stmt(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Expr(e) => {
                let x = self.expr(e);
                self.emit(&format!("{x};"));
            }
            IrStmt::Assign { targets, expr } => {
                let Some(t) = targets.first() else {
                    self.mark_todo("multi-target assign");
                    return;
                };
                if !t.indices.is_empty() {
                    self.mark_todo("array-index assign");
                    return;
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
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
            }
            IrStmt::While { cond, body } => {
                let c = self.expr(cond);
                self.emit(&format!("while ({c}) {{"));
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
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
            other => self.mark_todo(&format!("stmt {:?}", other)),
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
        for (n, _) in &prog.var_types {
            vars.insert(n.clone());
        }

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

/// Collect every variable name referenced by statements (assign targets,
/// declare lists, Var reads, loop vars).
fn collect_vars(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr } => {
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
        IrExpr::Call { args, .. } => {
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
