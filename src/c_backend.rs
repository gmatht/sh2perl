//! C backend renderer — LIBRARY interface (worktree-local, branch
//! `backend/c`). Consumes the ShIR directly in-process, bypassing the
//! `--shir` JSON contract (ask B of docs/backend-c-core-needs.md §1):
//! `shir_to_c(&IrProgram) -> String`.
//!
//! Uses the core's A2 type verdicts (`IrProgram.var_types`): `Int` vars →
//! C `long long`, `Str` vars → `char*`, anything else → runtime store
//! (`char*` + sh2.* stubs in this draft). Identifiers are mangled against
//! C keywords (A6-consistent). Everything outside the lowable subset
//! (numeric arith, echo/printf, if/else, simple assignment) emits a
//! compile-able `sh2.*` stub or a `/* TODO(unsupported) */` marker, so
//! the draft always compiles.
//!
//! The naive string/number coercion here is exactly the "C needs type
//! inference" gap PLAN.md v2 flagged; the design doc surfaces it as the
//! next work item.

use crate::ir::{ArithAst, IrExpr, IrProgram, IrStmt, IrType, InterpPart};
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
    need_upper: bool,
    need_lower: bool,
    need_includes: bool,
    need_slice: bool,
    todo: usize,
}

/// Render an `IrProgram` to C source (main() body).
pub fn shir_to_c(prog: &IrProgram) -> String {
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
        self.emit(&format!("/* TODO(unsupported): {what} */"));
    }

    /// A6-consistent C-keyword mangling (mirrors the emitter's safe_ident,
    /// which only covers loop vars — renderers mangle the rest).
    fn c_ident(&self, name: &str) -> String {
        const C_KEYWORDS: &[&str] = &[
            "auto", "break", "case", "char", "const", "continue", "default", "do", "double",
            "else", "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long",
            "register", "restrict", "return", "short", "signed", "sizeof", "static", "struct",
            "switch", "typedef", "union", "unsigned", "void", "volatile", "while", "_Bool",
            "_Complex", "true", "false",
        ];
        if C_KEYWORDS.contains(&name) {
            format!("{name}_")
        } else {
            name.to_string()
        }
    }

    fn cstr(s: &str) -> String {
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

    fn is_num(&self, name: &str) -> bool {
        self.var_types.get(name).copied() == Some(IrType::Int)
    }

    // ── expressions ──────────────────────────────────────────────────

    fn expr(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Int(i) => i.to_string(),
            IrExpr::Str(s, _) => Self::cstr(s),
            IrExpr::Var(name, _) => self.c_ident(name),
            IrExpr::Ident(name) => self.c_ident(name),
            IrExpr::Bool(b) => {
                if *b { "1".into() } else { "0".into() }
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                let c_op = match op {
                    crate::ir::BinOpKind::Add => "+",
                    crate::ir::BinOpKind::Sub => "-",
                    crate::ir::BinOpKind::Mul => "*",
                    crate::ir::BinOpKind::Div => "/",
                    crate::ir::BinOpKind::Mod => "%",
                    crate::ir::BinOpKind::Eq => "==",
                    crate::ir::BinOpKind::Ne => "!=",
                    crate::ir::BinOpKind::Lt => "<",
                    crate::ir::BinOpKind::Gt => ">",
                    crate::ir::BinOpKind::Le => "<=",
                    crate::ir::BinOpKind::Ge => ">=",
                    crate::ir::BinOpKind::And => "&&",
                    crate::ir::BinOpKind::Or => "||",
                    crate::ir::BinOpKind::Not => "!",
                    crate::ir::BinOpKind::Pow => {
                        return format!("pow({l},{r})");
                    }
                    _ => {
                        self.mark_todo(&format!("BinOp {:?}", op));
                        "0".into()
                    }
                };
                format!("({l} {c_op} {r})")
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Interpolate(_) => {
                // interpolation only lowers via echo parts_of; standalone → TODO
                self.mark_todo("Interpolate expr");
                "0".into()
            }
            IrExpr::Array(items) => {
                let elems: Vec<String> = items.iter().map(|e| self.expr(e)).collect();
                format!("[{}]", elems.join(", "))
            }
            IrExpr::Call { func, args } => self.call(func, args),
            IrExpr::Json(v) => match v {
                serde_json::Value::String(s) => Self::cstr(s),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => {
                    if *b { "1".into() } else { "0".into() }
                }
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

    /// Native C arithmetic from ArithAst (the numeric path).
    fn arith(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Num(n) => n.to_string(),
            ArithAst::Var(name) => self.c_ident(name),
            ArithAst::Index { .. } => {
                self.mark_todo("arith Index");
                "0".into()
            }
            ArithAst::Bin { op, lhs, rhs } => {
                let l = self.arith(lhs);
                let r = self.arith(rhs);
                if *op == "**" {
                    format!("pow({l},{r})")
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
        }
    }



    fn sh2_stub(&mut self, name: &str, _args: &[IrExpr], note: &str) -> String {
        self.sh2_calls.insert(name.to_string());
        self.mark_todo(&format!("{note} → sh2.{name}"));
        format!("sh2_{name}()")
    }

    fn call(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            // exec("echo", [args...]) → native printf (the draft's echo path)
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
                        return self.printf_from_parts(parts);
                    }
                    if cmd == "printf" {
                        return self.sh2_stub("builtin", args, "builtin printf");
                    }
                }
                self.sh2_stub("exec", args, "exec")
            }
            // getVar("y") — the ShIR's form of a `$y` read; the estree
            // lowering rewrites lifted vars to bare identifiers, so a C
            // renderer must do the same for Int/Str-typed vars.
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.var_types.contains_key(name) {
                        return self.c_ident(name);
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
                let c_op = match *op {
                    "-gt" => ">",
                    "-lt" => "<",
                    "-ge" => ">=",
                    "-le" => "<=",
                    "-eq" | "=" | "==" => "==",
                    "-ne" | "!=" => "!=",
                    _ => return None,
                };
                Some(format!("({} {c_op} {})", self.test_value(a), self.test_value(b)))
            }
            [flag, v] if *flag == "-n" => Some(format!("({})", self.test_value(v))),
            [flag, v] if *flag == "-z" => Some(format!("(!{})", self.test_value(v))),
            [v] => Some(format!("({})", self.test_value(v))),
            _ => None,
        }
    }

    /// A test operand: `"$y"`/`$y`/`y` (typed var) → ident; number →
    /// literal; otherwise a quoted string.
    fn test_value(&self, t: &str) -> String {
        let t = t.trim().trim_matches('"').strip_prefix('$').unwrap_or(t.trim().trim_matches('"'));
        if self.var_types.contains_key(t) {
            self.c_ident(t)
        } else if let Ok(n) = t.parse::<i64>() {
            n.to_string()
        } else {
            Self::cstr(t)
        }
    }

    /// Split an expression into printf parts: Lit(text) | Arg(cexpr, is_num).
    fn parts_of(&mut self, e: &IrExpr) -> Vec<Part> {
        match e {
            IrExpr::Str(s, _) => vec![Part::Lit(s.clone())],
            IrExpr::Int(i) => vec![Part::Arg(i.to_string(), true)],
            IrExpr::Var(name, _) => {
                vec![Part::Arg(self.c_ident(name), self.is_num(name))]
            }
            IrExpr::Ident(name) => vec![Part::Arg(self.c_ident(name), false)],
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
            IrExpr::Call { func, args } => {
                let args = args.clone();
                // getVar("x") → ident if x is typed, else stub
                if func == "getVar" {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        if self.var_types.contains_key(name) {
                            return vec![Part::Arg(self.c_ident(name), self.is_num(name))];
                        }
                    }
                    return vec![Part::Arg(self.call(func, &args), false)];
                }
                // other calls: render the call expression; default to non-num
                vec![Part::Arg(self.call(func, &args), self.expr_is_num(e))]
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

    fn printf_from_parts(&mut self, parts: Vec<Part>) -> String {
        let mut fmt = String::new();
        let mut cargs = Vec::new();
        for p in parts {
            match p {
                Part::Lit(t) => fmt.push_str(&t),
                Part::Arg(v, is_num) => {
                    if is_num {
                        fmt.push_str("%lld");
                        cargs.push(format!("(long long)({v})"));
                    } else {
                        fmt.push_str("%s");
                        cargs.push(format!("({v})"));
                    }
                }
            }
        }
        if cargs.is_empty() {
            format!("fputs({}, stdout)", Self::cstr(&fmt))
        } else {
            format!("printf({}, {})", Self::cstr(&fmt), cargs.join(", "))
        }
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
                let name = self.c_ident(&t.var);
                let is_num = self.is_num(&t.var);
                let rhs = if is_num {
                    self.expr_as_num(expr)
                } else {
                    self.expr(expr)
                };
                // A stub call (sh2_*) returns long long; cast for char*
                // targets so the draft always compiles (the stub exits 2
                // before returning, so the value never matters).
                if !is_num && rhs.starts_with("sh2_") {
                    self.emit(&format!("{name} = (char*)({rhs});"));
                } else {
                    self.emit(&format!("{name} = {rhs};"));
                }
            }
            IrStmt::Declare { vars, init, .. } => {
                let init_expr = init.as_ref().map(|e| self.expr(e));
                for d in vars {
                    let name = self.c_ident(&d.name);
                    if self.is_num(&d.name) {
                        let v = init_expr.clone().unwrap_or_else(|| "0".into());
                        self.emit(&format!("long long {name} = {v};"));
                    } else {
                        let v = init_expr.clone().unwrap_or_else(|| "NULL".into());
                        self.emit(&format!("char* {name} = {v};"));
                    }
                }
            }
            IrStmt::Output { value, newline, .. } => {
                let v = self.expr(value);
                if *newline {
                    self.emit(&format!("printf(\"%s\\n\", ({v}));"));
                } else {
                    self.emit(&format!("fputs({v}, stdout);"));
                }
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
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
            IrStmt::Exit(e) => {
                let code = e.as_ref().map(|x| self.expr(x)).unwrap_or_else(|| "0".into());
                self.emit(&format!("return {code};"));
            }
            IrStmt::For { var, iter, body } => {
                // Emit a C for loop over an index variable; each iteration
                // assigns the loop var from a static items array. Supports
                // Int vars with numeric items and string vars with string
                // items; anything else → TODO inside the loop body.
                let items = match iter {
                    IrExpr::Array(items) => items.clone(),
                    _ => {
                        self.mark_todo("for iter not Array");
                        return;
                    }
                };
                let n = items.len();
                if n == 0 {
                    return;
                }
                let var_name = self.c_ident(var);
                let is_num = self.is_num(var);
                let arr_id = format!("_for_{var_name}");
                if is_num {
                    let mut values = Vec::new();
                    let mut ok = true;
                    for item in &items {
                        match item {
                            IrExpr::Int(i) => values.push(i.to_string()),
                            IrExpr::Str(s, _) => match s.trim().parse::<i64>() {
                                Ok(n) => values.push(n.to_string()),
                                Err(_) => {
                                    self.mark_todo("for item not numeric");
                                    ok = false;
                                    break;
                                }
                            },
                            _ => {
                                self.mark_todo("for item type");
                                ok = false;
                                break;
                            }
                        }
                    }
                    if !ok { return; }
                    self.emit(&format!(
                        "static const long long {arr_id}[] = {{{}}};",
                        values.join(", ")
                    ));
                    self.emit(&format!(
                        "for (size_t _i_{var_name} = 0; _i_{var_name} < {n}; _i_{var_name}++) {{"
                    ));
                    self.depth += 1;
                    self.emit(&format!("long long {var_name} = {arr_id}[_i_{var_name}];"));
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                } else {
                    let mut values = Vec::new();
                    let mut ok = true;
                    for item in &items {
                        match item {
                            IrExpr::Str(s, _) => values.push(Self::cstr(s)),
                            _ => {
                                self.mark_todo("for item type");
                                ok = false;
                                break;
                            }
                        }
                    }
                    if !ok { return; }
                    self.emit(&format!(
                        "static const char* {arr_id}[] = {{{}}};",
                        values.join(", ")
                    ));
                    self.emit(&format!(
                        "for (size_t _i_{var_name} = 0; _i_{var_name} < {n}; _i_{var_name}++) {{"
                    ));
                    self.depth += 1;
                    self.emit(&format!("char* {var_name} = (char*){arr_id}[_i_{var_name}];"));
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                }
            }
            other => self.mark_todo(&format!("stmt {:?}", other)),
        }
    }

    /// Render an expression as a C integer (Int-typed assignment target).
    fn expr_as_num(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Str(s, _) => {
                // numeric literal in the ShIR ("5" for x=5)
                if let Ok(n) = s.trim().parse::<i64>() {
                    n.to_string()
                } else {
                    self.mark_todo(&format!("string→int coercion of {s:?}"));
                    "0".into()
                }
            }
            IrExpr::Int(i) => i.to_string(),
            _ => self.expr(e),
        }
    }

    // ── program ──────────────────────────────────────────────────────

    fn program(&mut self, prog: &IrProgram) {
        // Pass 1: collect declared vars (assign targets, declare lists,
        // Var reads) so declarations can be hoisted before use. Also
        // collect for-loop variables so we can exclude them from the
        // top-level pre-declaration (they are declared inside the loop).
        let mut vars: BTreeSet<String> = BTreeSet::new();
        let mut for_vars: BTreeSet<String> = BTreeSet::new();
        collect_vars_full(&prog.stmts, &mut vars, &mut for_vars);
        for (n, _) in &prog.var_types {
            vars.insert(n.clone());
        }
        for v in &for_vars {
            vars.remove(v);
        }

        // Pass 2: render the body first (helper flags known before preamble).
        let mut body_out = Vec::new();
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 1;
        for v in &vars {
            let name = self.c_ident(v);
            if self.is_num(v) {
                self.emit(&format!("long long {name} = 0;"));
            } else {
                self.emit(&format!("char* {name} = NULL;"));
            }
        }
        if !vars.is_empty() {
            self.emit("");
        }
        for s in &prog.stmts {
            self.stmt(s);
        }
        self.emit("return 0;");
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 0;

        // Preamble: includes, then the sh2.* stubs (definition-before-use,
        // so main's calls link), then main with the rendered body.
        self.emit("#include <stdio.h>");
        self.emit("#include <stdlib.h>");
        self.emit("#include <string.h>");
        self.emit("#include <math.h>");
        self.emit("");
        if !self.sh2_calls.is_empty() {
            self.emit("/* sh2.* runtime stubs — TODO: implement (harness/sh2-namespace.json) */");
            let names: Vec<String> = self.sh2_calls.iter().cloned().collect();
            for name in names {
                self.emit(&format!("static long long sh2_{name}(void) {{"));
                self.emit(&format!("  fprintf(stderr, \"TODO sh2.{name}\\n\");"));
                self.emit("  exit(2);");
                self.emit("  return 0;");
                self.emit("}");
            }
            self.emit("");
        }
        self.emit("int main(void) {");
        self.out.extend(body_out.iter().cloned());
        self.emit("}");
        if self.todo > 0 {
            self.emit(&format!("/* {} construct(s) lowered to TODO markers */", self.todo));
        }
    }
}

/// Collect every variable name referenced by statements (assign targets,
/// declare lists, Var reads).
fn collect_vars(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    collect_vars_full(stmts, out, &mut BTreeSet::new());
}

/// Like `collect_vars`, but also returns the set of for-loop variables
/// (which are declared inside the loop, not at function top).
fn collect_vars_full(
    stmts: &[IrStmt],
    out: &mut BTreeSet<String>,
    for_vars: &mut BTreeSet<String>,
) {
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
            IrStmt::Expr(e) => collect_vars_expr(e, out),
            IrStmt::Output { value, .. } => collect_vars_expr(value, out),
            IrStmt::If { cond, then, elsifs, else_ } => {
                collect_vars_expr(cond, out);
                collect_vars(then, out);
                for (c, b) in elsifs {
                    collect_vars_expr(c, out);
                    collect_vars(b, out);
                }
                collect_vars(else_, out);
            }
            IrStmt::Exit(e) => {
                if let Some(x) = e {
                    collect_vars_expr(x, out);
                }
            }
            IrStmt::For { var, iter, body } => {
                // The for-loop variable is declared inside the loop; don't
                // pre-declare it at function top.
                for_vars.insert(var.clone());
                collect_vars_expr(iter, out);
                collect_vars_full(body, out, for_vars);
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => collect_vars(b, out),
            _ => {}
        }
    }
}

fn collect_vars_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Var(name, _) => {
            out.insert(name.clone());
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
        IrExpr::Call { args, .. } => {
            for a in args {
                collect_vars_expr(a, out);
            }
        }
        _ => {}
    }
}

fn collect_vars_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(name) => {
            out.insert(name.clone());
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_vars_arith(lhs, out);
            collect_vars_arith(rhs, out);
        }
        ArithAst::Un { arg, .. } => collect_vars_arith(arg, out),
        ArithAst::Cond { test, then, else_, .. } => {
            collect_vars_arith(test, out);
            collect_vars_arith(then, out);
            collect_vars_arith(else_, out);
        }
        _ => {}
    }
}
