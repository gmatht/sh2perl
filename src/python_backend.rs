//! Python backend renderer — LIBRARY interface (worktree-local, branch
//! `backend/python`). Consumes the ShIR directly in-process, bypassing the
//! `--shir` JSON contract (ask B of docs/backend-python-core-needs.md §1):
//! `shir_to_python(&IrProgram) -> String`.
//!
//! Uses the core's A2 type verdicts (`IrProgram.var_types`): `Int` vars →
//! python `int`, everything else → python `str` (shell vars are strings).
//! Identifiers are mangled against Python keywords (A6-consistent).
//! Everything outside the lowable subset (numeric arith, echo/printf,
//! if/elif/else, while/for loops, simple assignment, subprocess exec)
//! emits a compile-able `sh2.*` stub or a `# TODO(unsupported)` marker,
//! so the draft always compiles (the stubs exit 2, mirroring the C
//! backend's runtime-store convention).

use crate::ir::{ArithAst, InterpPart, IrExpr, IrProgram, IrStmt, IrType};
use std::collections::{BTreeSet, HashMap};

#[derive(Default)]
pub struct Render {
    out: Vec<String>,
    depth: usize,
    /// var name -> type verdict (A2); missing = Any (runtime store)
    var_types: HashMap<String, IrType>,
    /// distinct sh2.* callee names that need stubs
    sh2_calls: BTreeSet<String>,
    /// >0 while rendering a function body (top-level `return` is a python
    /// syntax error, so Return outside a function lowers to a TODO)
    in_function: usize,
    todo: usize,
    /// needs `import re` (the grepMatches lift)
    need_re: bool,
}

/// Render an `IrProgram` to python source (a runnable script).
pub fn shir_to_python(prog: &IrProgram) -> String {
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
        self.emit(&format!("# TODO(unsupported): {what}"));
    }

    /// A6-consistent Python-keyword mangling (renderers mangle the rest —
    /// the emitter's safe_ident only covers loop vars).
    fn py_ident(&self, name: &str) -> String {
        const PY_KEYWORDS: &[&str] = &[
            "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
            "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
            "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise",
            "return", "try", "while", "with", "yield",
        ];
        if PY_KEYWORDS.contains(&name) {
            format!("{name}_")
        } else {
            name.to_string()
        }
    }

    fn py_str(s: &str) -> String {
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
            IrExpr::Str(s, _) => Self::py_str(s),
            IrExpr::Var(name, _) => self.py_ident(name),
            IrExpr::Ident(name) => self.py_ident(name),
            IrExpr::Bool(b) => {
                if *b {
                    "True".into()
                } else {
                    "False".into()
                }
            }
            IrExpr::Index { var, key } => {
                format!("{}[{}]", self.py_ident(var), self.expr(key))
            }
            IrExpr::BinOp { lhs, op, rhs } => {
                let l = self.expr(lhs);
                // `not` is unary in python; the IR only ever pairs it with a
                // meaningful lhs (the rhs is ignored)
                if matches!(op, crate::ir::BinOpKind::Not) {
                    return format!("(not {l})");
                }
                let r = self.expr(rhs);
                let py_op = match op {
                    crate::ir::BinOpKind::Add => "+",
                    crate::ir::BinOpKind::Sub => "-",
                    crate::ir::BinOpKind::Mul => "*",
                    crate::ir::BinOpKind::Div => "/",
                    crate::ir::BinOpKind::Mod => "%",
                    crate::ir::BinOpKind::Pow => "**",
                    crate::ir::BinOpKind::Concat => "+",
                    crate::ir::BinOpKind::Eq => "==",
                    crate::ir::BinOpKind::Ne => "!=",
                    crate::ir::BinOpKind::Lt => "<",
                    crate::ir::BinOpKind::Gt => ">",
                    crate::ir::BinOpKind::Le => "<=",
                    crate::ir::BinOpKind::Ge => ">=",
                    crate::ir::BinOpKind::And => "and",
                    crate::ir::BinOpKind::Or => "or",
                    crate::ir::BinOpKind::BitAnd => "&",
                    crate::ir::BinOpKind::BitOr => "|",
                    crate::ir::BinOpKind::BitXor => "^",
                    crate::ir::BinOpKind::ShiftL => "<<",
                    crate::ir::BinOpKind::ShiftR => ">>",
                    _ => {
                        self.mark_todo(&format!("BinOp {:?}", op));
                        "?".into()
                    }
                };
                format!("({l} {py_op} {r})")
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Call { func, args } => self.call(func, args),
            IrExpr::MethodCall { obj, method, args } => {
                let o = self.expr(obj);
                let a: Vec<String> = args.iter().map(|x| self.expr(x)).collect();
                format!("{o}.{method}({})", a.join(", "))
            }
            IrExpr::Ternary { cond, then, else_ } => format!(
                "({} if {} else {})",
                self.expr(then),
                self.expr(cond),
                self.expr(else_)
            ),
            IrExpr::DefinedOr { expr, default } => {
                format!("({} or {})", self.expr(expr), self.expr(default))
            }
            IrExpr::Interpolate(parts) => self.interp(parts),
            IrExpr::Capture { .. } => self.sh2_stub("capture", &[], "capture"),
            IrExpr::Regex { .. } => self.sh2_stub("regex", &[], "regex"),
            IrExpr::Range { start, end } => format!("range({}, {})", start, end + 1),
            IrExpr::RawExpr(s) => {
                self.mark_todo(&format!("RawExpr {s:?}"));
                "None".into()
            }
            IrExpr::Arrow(_) => self.sh2_stub("arrow", &[], "arrow"),
            IrExpr::Array(items) => {
                let elems: Vec<String> = items.iter().map(|e| self.expr(e)).collect();
                format!("[{}]", elems.join(", "))
            }
            IrExpr::Object(kv) => {
                let entries: Vec<String> = kv
                    .iter()
                    .map(|(k, v)| format!("{}: {}", Self::py_str(k), self.expr(v)))
                    .collect();
                format!("{{{}}}", entries.join(", "))
            }
            IrExpr::Json(v) => match v {
                serde_json::Value::String(s) => Self::py_str(s),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => {
                    if *b {
                        "True".into()
                    } else {
                        "False".into()
                    }
                }
                serde_json::Value::Null => "None".into(),
                _ => {
                    self.mark_todo("Json expr");
                    "None".into()
                }
            },
            other => {
                self.mark_todo(&format!("expr {:?}", other));
                "None".into()
            }
        }
    }

    /// Native python arithmetic from ArithAst (the numeric path).
    fn arith(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Num(n) => n.to_string(),
            ArithAst::Var(name) | ArithAst::Ident(name) => self.py_ident(name),
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
                "({} if {} else {})",
                self.arith(then),
                self.arith(test),
                self.arith(else_)
            ),
            ArithAst::Assign { .. } | ArithAst::IncDec { .. } => {
                // runtime setVar semantics (x+=, x++) — sh2.arith stub
                self.sh2_calls.insert("arith".into());
                format!("sh2_arith()")
            }
            ArithAst::Sizeof(ty) => ty.c_sizeof().unwrap_or(4).to_string(),
            ArithAst::Cast { arg, .. } => self.arith(arg),
        }
    }

    /// String interpolation: f-string when every expression part renders to
    /// a quote-free atom (the common `"a$x"` case); otherwise a safe
    /// `str()` concatenation.
    fn interp(&mut self, parts: &[InterpPart]) -> String {
        if parts.iter().all(|p| match p {
            InterpPart::Lit(_) => true,
            InterpPart::Expr(x) => self.fstring_safe(x),
        }) {
            let mut s = String::from("f\"");
            for p in parts {
                match p {
                    InterpPart::Lit(t) => s.push_str(&Self::py_fstr_lit(t)),
                    InterpPart::Expr(x) => {
                        s.push('{');
                        s.push_str(&self.expr(x));
                        s.push('}');
                    }
                }
            }
            s.push('"');
            s
        } else {
            let mut bits = Vec::new();
            for p in parts {
                match p {
                    InterpPart::Lit(t) => bits.push(Self::py_str(t)),
                    InterpPart::Expr(x) => bits.push(format!("str({})", self.expr(x))),
                }
            }
            format!("({})", bits.join(" + "))
        }
    }

    /// Can this expression be embedded in an f-string `{...}` (i.e. its
    /// rendering contains no `"` or backslash)?
    fn fstring_safe(&self, e: &IrExpr) -> bool {
        match e {
            IrExpr::Var(_, _) | IrExpr::Ident(_) | IrExpr::Int(_) | IrExpr::Bool(_) => true,
            IrExpr::Arith(a) => self.arith_safe(a),
            IrExpr::Call { func, args } if func == "getVar" => {
                // known vars render to bare idents; unknown → sh2_getVar("..")
                matches!(args.first(), Some(IrExpr::Str(name, _)) if self.var_types.contains_key(name))
            }
            _ => false,
        }
    }

    fn arith_safe(&self, a: &ArithAst) -> bool {
        match a {
            ArithAst::Num(_) | ArithAst::Var(_) => true,
            ArithAst::Bin { lhs, rhs, .. } => self.arith_safe(lhs) && self.arith_safe(rhs),
            ArithAst::Un { arg, .. } => self.arith_safe(arg),
            ArithAst::Cond { test, then, else_ } => {
                self.arith_safe(test) && self.arith_safe(then) && self.arith_safe(else_)
            }
            _ => false,
        }
    }

    /// Escape a literal for an f-string body: py_str plus brace doubling.
    fn py_fstr_lit(s: &str) -> String {
        let mut out = String::new();
        for c in s.chars() {
            match c {
                '{' => out.push_str("{{"),
                '}' => out.push_str("}}"),
                c => out.push(c),
            }
        }
        let inner = Self::py_str(&out);
        inner[1..inner.len() - 1].to_string()
    }

    fn sh2_stub(&mut self, name: &str, _args: &[IrExpr], note: &str) -> String {
        let safe = name.replace('.', "_");
        self.sh2_calls.insert(safe.clone());
        self.mark_todo(&format!("{note} → sh2.{name}"));
        format!("sh2_{safe}()")
    }

    fn call(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            // exec("echo", [args...]) → native print (python's print IS echo
            // semantics: space-separated args + trailing newline)
            "exec" => {
                if let Some(IrExpr::Str(cmd, _)) = args.first() {
                    if cmd == "echo" {
                        if let Some(IrExpr::Array(items)) = args.get(1) {
                            let rendered: Vec<String> =
                                items.iter().map(|i| self.expr(i)).collect();
                            if rendered.is_empty() {
                                return "print()".into();
                            }
                            return format!("print({})", rendered.join(", "));
                        }
                    }
                }
                self.sh2_stub("exec", args, "exec")
            }
            // getVar("y") — the ShIR's form of a `$y` read; typed vars are
            // plain python names, anything else → runtime stub
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    if self.var_types.contains_key(name) {
                        return self.py_ident(name);
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
            // `grepMatches(text, pattern, flags)` — the `grep -o` lift:
            // native re.findall (one match per line, grep -o's output).
            // flags: E (ERE as-is), F (fixed), i (case-insensitive).
            "grepMatches" => {
                let text = args.first().map(|a| self.expr(a)).unwrap_or_else(|| "\"\"".into());
                let pat = match args.get(1) {
                    Some(IrExpr::Str(s, _)) => s.clone(),
                    _ => return self.sh2_stub("grepMatches", args, "grepMatches"),
                };
                let flags = match args.get(2) {
                    Some(IrExpr::Str(s, _)) => s.clone(),
                    _ => String::new(),
                };
                self.need_re = true;
                let mut body = pat;
                if flags.contains('F') {
                    let mut lit = String::new();
                    for c in body.chars() {
                        if matches!(c, '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\') {
                            lit.push('\\');
                        }
                        lit.push(c);
                    }
                    body = lit;
                } else if !flags.contains('E') {
                    body = body
                        .replace("\\\\+", "+").replace("\\\\?", "?")
                        .replace("\\(", "(").replace("\\)", ")")
                        .replace("\\\\|", "|").replace("\\\\{", "{").replace("\\\\}", "}");
                }
                let rc = format!("\"\\n\".join(re.findall({}, {text}))", Self::py_str(&body));
                rc
            }
            _ => self.sh2_stub(func, args, func),
        }
    }

    /// Mini `[ ... ]` evaluator for the common patterns; None → stub.
    fn test_render(&mut self, s: &str) -> Option<String> {
        let toks: Vec<&str> = s.split_whitespace().collect();
        match toks.as_slice() {
            [a, op, b] => {
                let py_op = match *op {
                    "-gt" => ">",
                    "-lt" => "<",
                    "-ge" => ">=",
                    "-le" => "<=",
                    "-eq" | "=" | "==" => "==",
                    "-ne" | "!=" => "!=",
                    _ => return None,
                };
                Some(format!(
                    "({} {py_op} {})",
                    self.test_value(a),
                    self.test_value(b)
                ))
            }
            [flag, v] if *flag == "-n" => Some(format!("({})", self.test_value(v))),
            [flag, v] if *flag == "-z" => Some(format!("(not {})", self.test_value(v))),
            [v] => Some(format!("({})", self.test_value(v))),
            _ => None,
        }
    }

    /// A test operand: `"$y"`/`$y`/`y` (typed var) → ident; number →
    /// literal; otherwise a quoted string.
    fn test_value(&self, t: &str) -> String {
        let t = t.trim().trim_matches('"');
        let t = t.strip_prefix('$').unwrap_or(t);
        if self.var_types.contains_key(t) {
            self.py_ident(t)
        } else if let Ok(n) = t.parse::<i64>() {
            n.to_string()
        } else {
            Self::py_str(t)
        }
    }

    /// Render an expression as a python int (Int-typed assignment target).
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

    // ── statements ───────────────────────────────────────────────────

    /// Render a statement list as an indented block, emitting `pass` when
    /// the body would otherwise be empty (python requires at least one
    /// statement after a `:`).
    fn block(&mut self, stmts: &[IrStmt]) {
        let mut scratch = Vec::new();
        std::mem::swap(&mut self.out, &mut scratch);
        self.depth += 1;
        for s in stmts {
            self.stmt(s);
        }
        self.depth -= 1;
        std::mem::swap(&mut self.out, &mut scratch);
        let has_code = scratch.iter().any(|l| {
            let t = l.trim_start();
            !t.is_empty() && !t.starts_with('#')
        });
        if !has_code {
            self.out.extend(scratch);
            self.emit("pass");
        } else {
            self.out.extend(scratch);
        }
    }

    fn stmt(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Expr(e) => {
                if let IrExpr::Call { func, .. } = e {
                    if func == "grepMatches" {
                        // statement position: the matches are the output
                        let v = self.expr(e);
                        self.emit(&format!("print({v})"));
                        return;
                    }
                }
                let x = self.expr(e);
                self.emit(&format!("{x}"));
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
                let name = self.py_ident(&t.var);
                let rhs = if self.is_num(&t.var) {
                    self.expr_as_num(expr)
                } else {
                    self.expr(expr)
                };
                self.emit(&format!("{name} = {rhs}"));
            }
            IrStmt::Declare { vars, init, .. } => {
                let init_expr = init.as_ref().map(|e| self.expr(e));
                if vars.len() > 1 && init_expr.is_some() {
                    let names: Vec<String> = vars.iter().map(|d| self.py_ident(&d.name)).collect();
                    self.emit(&format!("{} = {}", names.join(" = "), init_expr.unwrap()));
                } else {
                    for d in vars {
                        let name = self.py_ident(&d.name);
                        let v = init_expr.clone().unwrap_or_else(|| {
                            if self.is_num(&d.name) {
                                "0".into()
                            } else {
                                "\"\"".into()
                            }
                        });
                        self.emit(&format!("{name} = {v}"));
                    }
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                let name = self.py_ident(var);
                let elems: Vec<String> = elements.iter().map(|e| self.expr(e)).collect();
                self.emit(&format!("{name} = [{}]", elems.join(", ")));
            }
            IrStmt::Output {
                value,
                newline,
                target,
            } => {
                let v = self.expr(value);
                if let Some(t) = target {
                    self.sh2_calls.insert("output".into());
                    self.mark_todo("output to filehandle");
                    self.emit(&format!("sh2_output({}, {v})", Self::py_str(t)));
                } else if *newline {
                    self.emit(&format!("print({v})"));
                } else {
                    self.emit(&format!("print({v}, end=\"\")"));
                }
            }
            IrStmt::WriteFile {
                path,
                content,
                append,
            } => {
                let p = self.expr(path);
                let c = self.expr(content);
                let mode = if *append { "\"a\"" } else { "\"w\"" };
                self.emit(&format!("with open({p}, {mode}) as _f:"));
                self.depth += 1;
                self.emit(&format!("_f.write(str({c}))"));
                self.depth -= 1;
            }
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => {
                let c = self.expr(cond);
                self.emit(&format!("if {c}:"));
                self.block(then);
                for (ec, body) in elsifs {
                    let ec = self.expr(ec);
                    self.emit(&format!("elif {ec}:"));
                    self.block(body);
                }
                if !else_.is_empty() {
                    self.emit("else:");
                    self.block(else_);
                }
            }
            IrStmt::For { var, iter, body } => {
                let v = self.py_ident(var);
                let it = self.expr(iter);
                self.emit(&format!("for {v} in {it}:"));
                self.block(body);
            }
            IrStmt::While { cond, body } => {
                let c = self.expr(cond);
                self.emit(&format!("while {c}:"));
                self.block(body);
            }
            IrStmt::DoWhile { body, cond, until } => {
                self.emit("while True:");
                self.block(body);
                let c = self.expr(cond);
                if *until {
                    self.emit(&format!("if {c}:"));
                } else {
                    self.emit(&format!("if not {c}:"));
                }
                self.depth += 1;
                self.emit("break");
                self.depth -= 1;
            }
            IrStmt::Exit(e) => {
                let code = e
                    .as_ref()
                    .map(|x| self.expr(x))
                    .unwrap_or_else(|| "0".into());
                self.emit(&format!("sys.exit({code})"));
            }
            IrStmt::Function { name, body, .. } => {
                let n = self.py_ident(name);
                self.emit(&format!("def {n}():"));
                self.in_function += 1;
                self.block(body);
                self.in_function -= 1;
            }
            IrStmt::Return(e) => {
                if self.in_function > 0 {
                    let x = e
                        .as_ref()
                        .map(|x| self.expr(x))
                        .unwrap_or_else(|| "None".into());
                    self.emit(&format!("return {x}"));
                } else {
                    self.mark_todo("top-level return");
                }
            }
            IrStmt::Exec {
                cmd, args, capture, ..
            } => {
                let c = self.expr(cmd);
                let mut argv = vec![c];
                for a in args {
                    argv.push(self.expr(a));
                }
                if let Some(var) = capture {
                    let v = self.py_ident(var);
                    self.emit(&format!(
                        "{v} = subprocess.check_output([{}]).decode()",
                        argv.join(", ")
                    ));
                } else {
                    self.emit(&format!("subprocess.run([{}])", argv.join(", ")));
                }
            }
            IrStmt::Block(b) => {
                for s in b {
                    self.stmt(s);
                }
            }
            other => self.mark_todo(&format!("stmt {:?}", other)),
        }
    }

    // ── program ──────────────────────────────────────────────────────

    fn program(&mut self, prog: &IrProgram) {
        // Pass 1: collect declared vars (assign targets, declare lists,
        // Var reads) so defaults can be hoisted before use (python vars
        // are dynamic, but unset reads should be "" / 0 like shell).
        let mut vars: BTreeSet<String> = BTreeSet::new();
        collect_vars(&prog.stmts, &mut vars);
        for (n, _) in &prog.var_types {
            vars.insert(n.clone());
        }

        // Pass 2: render the body first (helper flags known before preamble).
        let mut body_out = Vec::new();
        std::mem::swap(&mut self.out, &mut body_out);
        for v in &vars {
            let name = self.py_ident(v);
            if self.is_num(v) {
                self.emit(&format!("{name} = 0"));
            } else {
                self.emit(&format!("{name} = \"\""));
            }
        }
        if !vars.is_empty() {
            self.emit("");
        }
        for (idx, s) in prog.stmts.iter().enumerate() {
            let before = self.out.len();
            self.stmt(s);
            let line = prog.stmt_lines.iter().find(|(i, _)| *i == idx).map(|(_, l)| *l);
            if let Some(l) = line {
                if let Some(first) = self.out.get_mut(before) {
                    *first = format!("{first} # line {l}");
                }
            }
        }
        std::mem::swap(&mut self.out, &mut body_out);

        // Preamble: shebang, imports, then the sh2.* stubs
        // (definition-before-use, so the body's calls link).
        self.emit("#!/usr/bin/env python3");
        self.emit("# Generated by sh2perl's python backend (debashl::python_backend).");
        self.emit("import os");
        self.emit("import subprocess");
        self.emit("import sys");
        if self.need_re {
            self.emit("import re");
        }
        self.emit("");
        if !self.sh2_calls.is_empty() {
            self.emit("# sh2.* runtime stubs — TODO: implement (harness/sh2-namespace.json)");
            let names: Vec<String> = self.sh2_calls.iter().cloned().collect();
            for name in names {
                self.emit(&format!("def sh2_{name}(*args):"));
                self.emit(&format!("    print(\"TODO sh2.{name}\", file=sys.stderr)"));
                self.emit("    sys.exit(2)");
                self.emit("");
            }
        }
        // Subroutine definitions (before the body that calls them).
        for sub in &prog.subs {
            let params: Vec<String> = sub.params.iter().map(|p| self.py_ident(p)).collect();
            self.emit(&format!(
                "def {}({}):",
                self.py_ident(&sub.name),
                params.join(", ")
            ));
            self.in_function += 1;
            self.block(&sub.body);
            self.in_function -= 1;
            self.emit("");
        }
        self.out.extend(body_out.iter().cloned());
        if self.todo > 0 {
            self.emit(&format!(
                "# {} construct(s) lowered to TODO markers",
                self.todo
            ));
        }
    }
}

/// Collect every variable name referenced by statements (assign targets,
/// declare lists, Var reads).
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
            IrStmt::Function { name, body, .. } => {
                out.insert(name.clone());
                collect_vars(body, out);
            }
            IrStmt::Return(e) => {
                if let Some(x) = e {
                    collect_vars_expr(x, out);
                }
            }
            IrStmt::Exec {
                cmd, args, capture, ..
            } => {
                collect_vars_expr(cmd, out);
                for a in args {
                    collect_vars_expr(a, out);
                }
                if let Some(v) = capture {
                    out.insert(v.clone());
                }
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
        IrExpr::Object(kv) => {
            for (_, v) in kv {
                collect_vars_expr(v, out);
            }
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
        IrExpr::Ternary { cond, then, else_ } => {
            collect_vars_expr(cond, out);
            collect_vars_expr(then, out);
            collect_vars_expr(else_, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_vars_expr(expr, out);
            collect_vars_expr(default, out);
        }
        IrExpr::Capture { expr, .. } => collect_vars_expr(expr, out),
        IrExpr::Arrow(b) => collect_vars(b, out),
        _ => {}
    }
}

fn collect_vars_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(name) => {
            out.insert(name.clone());
        }
        ArithAst::Index { var, key, .. } => {
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
        ArithAst::Assign { rhs, .. } => collect_vars_arith(rhs, out),
        _ => {}
    }
}
