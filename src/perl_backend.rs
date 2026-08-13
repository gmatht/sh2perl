//! Perl backend renderer — LIBRARY interface (worktree-local, branch
//! `backend/perl`). Consumes the ShIR directly in-process, bypassing the
//! `--shir` JSON round-trip: `shir_to_perl(&IrProgram) -> String`.
//!
//! The renderer walks the FULL ShIR node vocabulary and NEVER panics:
//! anything outside the lowable subset emits a `# TODO(unsupported)`
//! marker plus a valid-Perl fallback (the sh2.*-stub pattern, mirroring
//! the C backend's `/* TODO */` convention), so the output ALWAYS
//! compiles and the corpus gate's render step always succeeds.
//!
//! Lowable subset (native, idiomatic Perl):
//!   - output: `echo`/`printf` (incl. `-n`/`-e`), IrStmt::Output/WriteFile
//!   - assignment: Assign/Declare/DeclareArray, `$x op= rhs` folding,
//!     index writes, setVar/assign calls, array store (setArray/append)
//!   - reads: getVar, param expansions (defaults, length, case mods,
//!     prefix/suffix removal, substitution, slice, basename/dirname),
//!     array reads (arrayIndex/listVar/arrayItems/arrayLen)
//!   - arithmetic: native Perl for IrExpr::Arith AND the `arith("...")`
//!     string form (shell arith syntax ≈ Perl arith syntax)
//!   - tests: a mini `[ ... ]` evaluator (numeric/string/file-test/glob
//!     pattern ops, && || !) lowered to native Perl booleans
//!   - control: if/elsif/else, while, do-while/until, for (list + range),
//!     case → if/elsif regex chain, break/continue/return signals
//!   - builtins: cd, mkdir, touch, rm/unlink, read, shift, exit
//!   - external commands: `system 'cmd', @args` (LIST form, no shell)
//!   - capture/pipeline: `qx{...}` from the reconstructed shell command
//!   - functions: `sub name { ... }`, positional `$1` → `$_[0]` inside
//!   - brace expansion `{a,b}{1..3}` evaluated at render time (the pure
//!     string work the sh2.brace runtime does), lowered to a literal list
//!
//! Variable conventions (mirroring the core's established Perl emitter,
//! `src/ir.rs`): ALL-CAPS names → `$ENV{NAME}`; `$1`..`$9` → `$ARGV[n-1]`
//! at top level / `$_[n-1]` inside subs; `$?` → `($? >> 8)`; `$$` → PID;
//! `$@`/`$*` → `@ARGV`; `$#` → `scalar(@ARGV)`. Every other scalar/array/
//! hash is pre-declared (`my $x;` / `my @a;` / `my %h;`) so the output is
//! strict-clean.

use crate::ir::*;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct Render {
    /// Rendered body lines (rendered before the preamble so the var
    /// declarations can be hoisted above first use).
    out: Vec<String>,
    depth: usize,
    /// Inside a `sub` body: positional params are `$_[n-1]`.
    in_func: usize,
    /// Scalar vars needing `my $x;` hoisting.
    scalars: BTreeSet<String>,
    /// Array vars needing `my @x;` hoisting.
    arrays: BTreeSet<String>,
    /// Hash vars needing `my %x;` hoisting.
    hashes: BTreeSet<String>,
    /// For-loop vars — declared by the loop itself, never hoisted.
    loop_vars: BTreeSet<String>,
    /// User-defined function names (exec("foo") with a known foo → sub call).
    funcs: BTreeSet<String>,
    need_say: bool,
    need_basename: bool,
    todo: usize,
}

/// Render an `IrProgram` to Perl source.
pub fn shir_to_perl(prog: &IrProgram) -> String {
    let mut r = Render::default();
    // A2 var_types are ignored: Perl scalars are dynamically typed, so the
    // type verdicts are only relevant for the static backends (C).
    r.collect_funcs(&prog.stmts);
    for s in &prog.subs {
        r.funcs.insert(s.name.clone());
    }
    // Pass 1: render the body (registers vars + helper flags).
    let mut body_out = Vec::new();
    std::mem::swap(&mut r.out, &mut body_out);
    for s in &prog.stmts {
        r.stmt(s);
    }
    for s in &prog.subs {
        r.sub(s);
    }
    std::mem::swap(&mut r.out, &mut body_out);
    r.depth = 0;

    // Pass 2: preamble.
    r.emit("#!/usr/bin/env perl");
    r.emit("use strict;");
    r.emit("use warnings;");
    if r.need_say {
        r.emit("use feature 'say';");
    }
    if r.need_basename {
        r.emit("use File::Basename qw(basename dirname);");
    }
    for import in &prog.imports {
        r.emit(&format!("use {};", import));
    }
    for req in &prog.requires {
        r.emit(&format!("require {};", req));
    }
    let scalars: Vec<String> = r
        .scalars
        .iter()
        .filter(|v| v.as_str() != "_")
        .cloned()
        .collect();
    let arrays: Vec<String> = r
        .arrays
        .iter()
        .filter(|v| v.as_str() != "_" && v.as_str() != "@" && v.as_str() != "*")
        .cloned()
        .collect();
    let hashes: Vec<String> = r.hashes.iter().cloned().collect();
    for v in &scalars {
        r.emit(&format!("my ${};", ident(v)));
    }
    for v in &arrays {
        r.emit(&format!("my @{};", ident(v)));
    }
    for v in &hashes {
        r.emit(&format!("my %{};", ident(v)));
    }
    if !scalars.is_empty() || !arrays.is_empty() || !hashes.is_empty() {
        r.emit("");
    }
    r.out.extend(body_out.iter().cloned());
    if r.todo > 0 {
        r.emit(&format!(
            "# {} construct(s) lowered to TODO markers",
            r.todo
        ));
    }
    let mut text = r.out.join("\n");
    text.push('\n');
    text
}

// ── helpers ──────────────────────────────────────────────────────────

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

    /// Sanitize a shell variable name into a Perl identifier.
    fn perl_str(s: &str) -> String {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '$' => out.push_str("\\$"),
                '@' => out.push_str("\\@"),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                c if (c as u32) < 32 => out.push_str(&format!("\\x{{{:x}}}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }

    fn str_arg(args: &[IrExpr], i: usize) -> Option<String> {
        match args.get(i) {
            Some(IrExpr::Str(s, _)) => Some(s.clone()),
            _ => None,
        }
    }

    /// Read reference for a shell variable (getVar/Var/arith contexts).
    fn var_ref(&mut self, name: &str) -> String {
        match name {
            "?" => "(($? >> 8))".to_string(),
            "$" => "$$".to_string(),
            "@" | "*" => "@ARGV".to_string(),
            "#" => "scalar(@ARGV)".to_string(),
            "!" => "0".to_string(),
            "-" => "''".to_string(),
            "0" => "$0".to_string(),
            n if n.len() == 1 && n.as_bytes()[0].is_ascii_digit() => {
                let idx: usize = n.parse().unwrap_or(1);
                if self.in_func > 0 {
                    format!("$_[{}]", idx - 1)
                } else {
                    format!("$ARGV[{}]", idx - 1)
                }
            }
            _ => {
                if is_env_style_var_name(name) {
                    format!("$ENV{{{}}}", name)
                } else {
                    self.scalars.insert(name.to_string());
                    format!("${}", ident(name))
                }
            }
        }
    }

    fn array_ref(&mut self, name: &str) -> String {
        match name {
            "@" | "*" => "@ARGV".to_string(),
            "_" => "@_".to_string(),
            _ => {
                self.arrays.insert(name.to_string());
                format!("@{}", ident(name))
            }
        }
    }

    /// `$name[key]` — array index read/write (registers the right container).
    fn index_ref(&mut self, name: &str, key: &IrExpr) -> String {
        let k = self.expr(key);
        match key {
            IrExpr::Int(_) => {
                self.arrays.insert(name.to_string());
                format!("${}[{k}]", ident(name))
            }
            _ => {
                self.hashes.insert(name.to_string());
                format!("${}{{{k}}}", ident(name))
            }
        }
    }

    /// A user-facing scalar write target.
    fn scalar_target(&mut self, name: &str) -> String {
        if is_env_style_var_name(name) {
            format!("$ENV{{{}}}", name)
        } else {
            self.scalars.insert(name.to_string());
            format!("${}", ident(name))
        }
    }

    /// Shell-quote one word of a reconstructed command for `qx{...}`.
    /// Literal words are single-quoted (no Perl interpolation); words with
    /// `$var` parts keep the reference so Perl interpolates the value.
    fn shell_word(&mut self, w: &IrExpr) -> String {
        match w {
            IrExpr::Str(s, _) => shell_squote(s),
            IrExpr::Int(n) => n.to_string(),
            IrExpr::Interpolate(parts) => {
                let mut out = String::new();
                let mut lit = String::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => lit.push_str(s),
                        InterpPart::Expr(x) => {
                            if let IrExpr::Call { func, args } = x.as_ref() {
                                if func == "getVar" {
                                    if let Some(name) = Self::str_arg(args, 0) {
                                        if !lit.is_empty() {
                                            out.push_str(&shell_squote(&lit));
                                            lit.clear();
                                        }
                                        out.push_str(&self.var_ref(&name));
                                        continue;
                                    }
                                }
                            }
                            if !lit.is_empty() {
                                out.push_str(&shell_squote(&lit));
                                lit.clear();
                            }
                            out.push_str("$(");
                            out.push_str(&self.expr(x));
                            out.push(')');
                        }
                    }
                }
                if !lit.is_empty() {
                    out.push_str(&shell_squote(&lit));
                }
                if out.is_empty() {
                    "''".to_string()
                } else {
                    out
                }
            }
            IrExpr::Var(name, sigil) => match sigil {
                Some(Sigil::Array) => format!("@{}", ident(name)),
                _ => self.var_ref(name),
            },
            other => {
                // complex words: interpolate the rendered value
                format!("$({})", self.expr(other))
            }
        }
    }

    /// Reconstruct a shell command from a statement list (for qx capture).
    fn shell_cmd(&mut self, stmts: &[IrStmt], sep: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        for s in stmts {
            match s {
                IrStmt::Expr(IrExpr::Call { func, args }) if func == "exec" => {
                    let mut words: Vec<String> = Vec::new();
                    if let Some(cmd) = Self::str_arg(args, 0) {
                        words.push(shell_squote(&cmd));
                    }
                    if let Some(IrExpr::Array(items)) = args.get(1) {
                        for w in items {
                            words.push(self.shell_word(w));
                        }
                    }
                    parts.push(words.join(" "));
                }
                IrStmt::Expr(IrExpr::Call { func, args }) if func == "test" => {
                    if let Some(t) = Self::str_arg(args, 0) {
                        parts.push(format!("test {}", t));
                    }
                }
                _ => {
                    self.mark_todo("capture body stmt");
                }
            }
        }
        parts.join(sep)
    }

    fn qx(&mut self, cmd: &str) -> String {
        // Escape what must stay literal; `$var` refs interpolate.
        let mut out = String::new();
        for c in cmd.chars() {
            match c {
                '$' => out.push_str("\\$"),
                '@' => out.push_str("\\@"),
                '\\' => out.push_str("\\\\"),
                '{' => out.push_str("\\{"),
                '}' => out.push_str("\\}"),
                c => out.push(c),
            }
        }
        format!("qx{{{out}}}")
    }

    fn emit_qx_stmt(&mut self, cmd: &str) {
        let q = self.qx(cmd);
        self.emit(&format!("{q};"));
    }

    // ── expressions ──────────────────────────────────────────────────

    fn expr(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Int(n) => n.to_string(),
            IrExpr::Str(s, _) => Self::perl_str(s),
            IrExpr::Var(name, sigil) => match sigil {
                Some(Sigil::Array) => self.array_ref(name),
                Some(Sigil::Hash) => {
                    self.hashes.insert(name.to_string());
                    format!("%{}", ident(name))
                }
                _ => self.var_ref(name),
            },
            IrExpr::Index { var, key } => self.index_ref(var, key),
            IrExpr::BinOp { lhs, op, rhs } => {
                let (l, r) = (self.expr(lhs), self.expr(rhs));
                match op {
                    BinOpKind::Add => format!("({l} + {r})"),
                    BinOpKind::Sub => format!("({l} - {r})"),
                    BinOpKind::Mul => format!("({l} * {r})"),
                    BinOpKind::Div => format!("int(({l}) / ({r}))"),
                    BinOpKind::Mod => format!("({l} % {r})"),
                    BinOpKind::Pow => format!("({l} ** {r})"),
                    BinOpKind::Concat => format!("({l} . {r})"),
                    BinOpKind::Eq => format!("({l} == {r})"),
                    BinOpKind::Ne => format!("({l} != {r})"),
                    BinOpKind::Lt => format!("({l} < {r})"),
                    BinOpKind::Gt => format!("({l} > {r})"),
                    BinOpKind::Le => format!("({l} <= {r})"),
                    BinOpKind::Ge => format!("({l} >= {r})"),
                    BinOpKind::And => format!("({} && {})", self.boolify(lhs), self.boolify(rhs)),
                    BinOpKind::Or => format!("({} || {})", self.boolify(lhs), self.boolify(rhs)),
                    BinOpKind::Not => format!("(!{})", self.boolify(lhs)),
                    BinOpKind::BitAnd => format!("({l} & {r})"),
                    BinOpKind::BitOr => format!("({l} | {r})"),
                    BinOpKind::BitXor => format!("({l} ^ {r})"),
                    BinOpKind::ShiftL => format!("({l} << {r})"),
                    BinOpKind::ShiftR => format!("({l} >> {r})"),
                }
            }
            IrExpr::Call { func, args } => self.call(func, args),
            IrExpr::MethodCall { .. } => {
                self.mark_todo("MethodCall expr");
                "0".to_string()
            }
            IrExpr::Ternary { cond, then, else_ } => format!(
                "({} ? {} : {})",
                self.boolify(cond),
                self.expr(then),
                self.expr(else_)
            ),
            IrExpr::DefinedOr { expr, default } => format!(
                "((({} // \"\") ne \"\") ? {} : {})",
                self.expr(expr),
                self.expr(expr),
                self.expr(default)
            ),
            IrExpr::Interpolate(parts) => self.interp(parts),
            IrExpr::Capture { expr, native } => {
                let _ = native;
                self.capture_from_expr(expr)
            }
            IrExpr::Regex { pattern, flags } => {
                let mut p = String::new();
                for c in pattern.chars() {
                    if c == '/' || c == '\\' {
                        p.push('\\');
                    }
                    p.push(c);
                }
                format!("/{p}/{flags}")
            }
            IrExpr::Range { start, end } => format!("{start}..{end}"),
            IrExpr::RawExpr(t) => t.clone(),
            IrExpr::Arrow(stmts) => {
                // expression-position block: do { ... }
                let mut inner = Vec::new();
                std::mem::swap(&mut self.out, &mut inner);
                let saved = self.depth;
                self.depth = 0;
                for s in stmts {
                    self.stmt(s);
                }
                let body = self.out.join("\n");
                self.out = inner;
                self.depth = saved;
                format!("do {{\n{}\n}}", indent_block(&body, 1))
            }
            IrExpr::Array(items) => {
                let elems: Vec<String> = items.iter().map(|i| self.expr(i)).collect();
                format!("({})", elems.join(", "))
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Bool(b) => {
                if *b {
                    "1".into()
                } else {
                    "0".into()
                }
            }
            IrExpr::Json(v) => match v {
                serde_json::Value::String(s) => Self::perl_str(s),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => {
                    if *b {
                        "1".into()
                    } else {
                        "0".into()
                    }
                }
                _ => {
                    self.mark_todo("Json expr");
                    "0".into()
                }
            },
            IrExpr::Ident(name) => Self::perl_str(name),
            IrExpr::Object(pairs) => {
                let elems: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("{} => {}", Self::perl_str(k), self.expr(v)))
                    .collect();
                format!("{{ {} }}", elems.join(", "))
            }
        }
    }

    /// In boolean contexts (&& || ! ternary test), commands are true on
    /// exit status 0.
    fn boolify(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Call { func, args } if func == "exec" || func == "let" => {
                format!("(({}) == 0)", self.expr(e))
            }
            _ => self.expr(e),
        }
    }

    /// String interpolation: `"lit" . $x . "lit2"` (single-expression when
    /// only literals).
    fn interp(&mut self, parts: &[InterpPart]) -> String {
        let mut out = String::new();
        let mut lit = String::new();
        for p in parts {
            match p {
                InterpPart::Lit(s) => lit.push_str(s),
                InterpPart::Expr(x) => {
                    if !lit.is_empty() {
                        out.push_str(&Self::perl_str(&lit));
                        out.push_str(" . ");
                        lit.clear();
                    }
                    out.push_str(&self.expr(x));
                    out.push_str(" . ");
                }
            }
        }
        if !lit.is_empty() || out.is_empty() {
            out.push_str(&Self::perl_str(&lit));
            out
        } else {
            out.truncate(out.len() - 3);
            out
        }
    }

    fn capture_from_expr(&mut self, e: &IrExpr) -> String {
        match e {
            IrExpr::Arrow(stmts) => {
                let cmd = self.shell_cmd(stmts, "; ");
                self.qx(&cmd)
            }
            other => {
                self.mark_todo("capture expr");
                let e = self.expr(other);
                self.qx(&e)
            }
        }
    }

    /// Native arithmetic from ArithAst.
    fn arith(&mut self, a: &ArithAst) -> String {
        match a {
            ArithAst::Num(n) => n.to_string(),
            ArithAst::Var(name) => self.var_ref(name),
            ArithAst::Index { var, key } => {
                let k = self.arith(key);
                self.arrays.insert(var.clone());
                format!("${}[{}]", ident(var), k)
            }
            ArithAst::Bin { op, lhs, rhs } => {
                if op == "/" {
                    // shell arithmetic is INTEGER division
                    format!("int(({}) / ({}))", self.arith(lhs), self.arith(rhs))
                } else {
                    format!("({} {op} {})", self.arith(lhs), self.arith(rhs))
                }
            }
            ArithAst::Un { op, arg } => format!("({op}{})", self.arith(arg)),
            ArithAst::Cond { test, then, else_ } => format!(
                "({} ? {} : {})",
                self.arith(test),
                self.arith(then),
                self.arith(else_)
            ),
            ArithAst::Assign { var, op, rhs } => {
                let v = self.scalar_target(var);
                format!("({v} {op} {})", self.arith(rhs))
            }
            ArithAst::IncDec { var, delta, prefix } => {
                let v = self.scalar_target(var);
                let op = if *delta >= 0 { "++" } else { "--" };
                if *prefix {
                    format!("({op}{v})")
                } else {
                    format!("({v}{op})")
                }
            }
            // C-frontend nodes (never emitted by the shell path): sizeof
            // is a compile-time constant; casts are identity (Perl IV).
            ArithAst::Sizeof(ty) => ty.c_sizeof().unwrap_or(4).to_string(),
            ArithAst::Cast { arg, .. } => self.arith(arg),
        }
    }

    /// `arith("$j*$i")` — shell arith syntax ≈ Perl arith syntax; prefix
    /// `$var` refs (and bare identifiers), pass everything else through.
    fn arith_str(&mut self, s: &str) -> String {
        // `$((echo "test"))` ambiguity: a quote inside the string means the
        // parser resolved it as command substitution — run it instead.
        if s.contains('"') || s.contains('\'') {
            return self.qx(s);
        }
        // `$(cmd)` inside arithmetic: lower each substitution to `(qx{...})`
        // (numeric coercion handles the trailing newline)
        let mut s = s.to_string();
        if s.contains("$(") {
            let mut out = String::new();
            let chars: Vec<char> = s.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '(' {
                    let mut j = i + 2;
                    let mut depth = 1;
                    let mut cmd = String::new();
                    while j < chars.len() && depth > 0 {
                        if chars[j] == '(' {
                            depth += 1;
                        } else if chars[j] == ')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        cmd.push(chars[j]);
                        j += 1;
                    }
                    let q = self.qx(&cmd);
                    out.push_str(&q);
                    i = j + 1;
                    continue;
                }
                out.push(chars[i]);
                i += 1;
            }
            s = out;
            return format!("({s})");
        }
        let mut out = String::new();
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '$' {
                if i + 1 < chars.len() && chars[i + 1] == '{' {
                    // ${name} / ${#arr[@]} / ${arr[i]} — parse to the closing brace
                    let mut j = i + 2;
                    let mut name = String::new();
                    while j < chars.len() && chars[j] != '}' {
                        name.push(chars[j]);
                        j += 1;
                    }
                    if j < chars.len() {
                        out.push_str(&self.arith_braced(&name));
                        i = j + 1;
                        continue;
                    }
                } else if i + 1 < chars.len()
                    && (chars[i + 1].is_ascii_alphanumeric() || chars[i + 1] == '_')
                {
                    let mut j = i + 1;
                    let mut name = String::new();
                    while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                        name.push(chars[j]);
                        j += 1;
                    }
                    out.push_str(&self.var_ref(&name));
                    i = j;
                    continue;
                }
            } else if c.is_ascii_alphabetic() || c == '_' {
                // bare identifier → variable read (`i++ + ++i`)
                let mut j = i;
                let mut name = String::new();
                while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                    name.push(chars[j]);
                    j += 1;
                }
                out.push_str(&self.var_ref(&name));
                i = j;
                continue;
            } else if c == '#' && i > 0 {
                // base#number notation: `10#2`, `16#ff` → evaluate when both
                // sides are literal, else drop the base prefix
                let mut b = i;
                while b > 0 && chars[b - 1].is_ascii_digit() {
                    b -= 1;
                }
                let base_str: String = chars[b..i].iter().collect();
                if !base_str.is_empty() {
                    let mut j = i + 1;
                    let mut val = String::new();
                    while j < chars.len()
                        && (chars[j].is_ascii_alphanumeric() || chars[j] == '_' || chars[j] == '@')
                    {
                        val.push(chars[j]);
                        j += 1;
                    }
                    if let (Ok(base), Ok(v)) = (base_str.parse::<u32>(), val.parse::<i64>()) {
                        if (2..=36).contains(&base) {
                            if let Ok(n) = i64::from_str_radix(&val, base) {
                                let _ = v;
                                // remove the already-emitted base digits
                                let cut = out.len() - base_str.len();
                                out.truncate(cut);
                                out.push_str(&n.to_string());
                                i = j;
                                continue;
                            }
                        }
                    }
                    // not literal-evaluable: keep the value, drop the base
                    // (and the base digits already emitted); a bareword
                    // value is a variable read (`10#x` == `x`)
                    let cut = out.len() - base_str.len();
                    out.truncate(cut);
                    if !val.is_empty()
                        && (val.chars().next().unwrap().is_ascii_alphabetic()
                            || val.chars().next().unwrap() == '_')
                        && val.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    {
                        out.push_str(&self.var_ref(&val));
                    } else {
                        out.push_str(&val);
                    }
                    i = j;
                    continue;
                }
            }
            out.push(c);
            i += 1;
        }
        // shell arithmetic is integer: wrap when a `/` division is present
        // (division inside larger expressions is rare in the string form;
        // the ArithAST form wraps each `/` node exactly)
        if s.contains('/') {
            format!("int(({out}))")
        } else {
            format!("({out})")
        }
    }

    /// Inside `${{...}}` in an arith string: `#arr[@]` → length, `arr[i]` →
    /// index read, plain name → var read.
    fn arith_braced(&mut self, name: &str) -> String {
        if let Some(rest0) = name.strip_prefix('#') {
            let rest = rest0
                .strip_suffix("[@]")
                .or_else(|| rest0.strip_suffix("[*]"));
            if let Some(rest) = rest {
                self.arrays.insert(rest.to_string());
                return format!("scalar(@{})", ident(rest));
            }
            return format!("length({})", self.var_ref(rest0));
        }
        if let Some(open) = name.find('[') {
            if name.ends_with(']') {
                let var = &name[..open];
                let key = &name[open + 1..name.len() - 1];
                let inner = self.arith_str(key);
                self.arrays.insert(var.to_string());
                return format!(
                    "${}[{}]",
                    ident(var),
                    inner.trim_matches(|c| c == '(' || c == ')')
                );
            }
        }
        self.var_ref(name)
    }

    // ── call dispatch ────────────────────────────────────────────────

    fn call(&mut self, func: &str, args: &[IrExpr]) -> String {
        match func {
            "getVar" => match Self::str_arg(args, 0) {
                Some(name) => self.var_ref(&name),
                None => {
                    self.mark_todo("getVar arg");
                    "0".into()
                }
            },
            "param" => self.param(args),
            "setVar" => {
                let (Some(name), Some(value)) = (Self::str_arg(args, 0), args.get(1)) else {
                    self.mark_todo("setVar args");
                    return "0".into();
                };
                format!("({} = {})", self.scalar_target(&name), self.expr(value))
            }
            "assign" => {
                // arith assignment x+= / x++ via the arith evaluator
                let (Some(name), Some(op)) = (Self::str_arg(args, 0), Self::str_arg(args, 1))
                else {
                    self.mark_todo("assign args");
                    return "0".into();
                };
                let v = self.scalar_target(&name);
                if op == "++" || op == "--" {
                    format!("({v}{op})")
                } else if let Some(value) = args.get(2) {
                    format!("({v} {op} {})", self.expr(value))
                } else {
                    format!("({v} {op} 1)")
                }
            }
            "arith" => match Self::str_arg(args, 0) {
                Some(s) => self.arith_str(&s),
                None => {
                    self.mark_todo("arith arg");
                    "0".into()
                }
            },
            "test" => match Self::str_arg(args, 0) {
                Some(s) => self.test(&s),
                None => {
                    self.mark_todo("test arg");
                    "0".into()
                }
            },
            "exec" => self.exec_expr(args),
            "capture" | "captureWords" => match args.first() {
                Some(IrExpr::Arrow(stmts)) => {
                    let cmd = self.shell_cmd(stmts, "; ");
                    self.qx(&cmd)
                }
                other => {
                    self.mark_todo(&format!("{func} arg"));
                    self.capture_from_expr(
                        other.unwrap_or(&IrExpr::Str(String::new(), StrStyle::DoubleQuoted)),
                    )
                }
            },
            "pipeline" => {
                let mut stages: Vec<String> = Vec::new();
                if let Some(IrExpr::Array(items)) = args.first() {
                    for it in items {
                        if let IrExpr::Arrow(stmts) = it {
                            stages.push(self.shell_cmd(stmts, "; "));
                        }
                    }
                }
                if stages.is_empty() {
                    self.mark_todo("pipeline stages");
                    return "0".into();
                }
                self.qx(&stages.join(" | "))
            }
            "brace" => self.brace(args),
            "join" => match args.first() {
                Some(IrExpr::Array(items)) => {
                    let elems: Vec<String> = items.iter().map(|i| self.expr(i)).collect();
                    format!("join(' ', {})", elems.join(", "))
                }
                Some(other) => format!("join(' ', {})", self.expr(other)),
                None => {
                    self.mark_todo("join arg");
                    "0".into()
                }
            },
            "setArray" => {
                let (Some(name), Some(items)) = (Self::str_arg(args, 0), args.get(1)) else {
                    self.mark_todo("setArray args");
                    return "0".into();
                };
                let is_assoc = matches!(args.get(2), Some(IrExpr::Bool(true)));
                if is_assoc {
                    self.hashes.insert(name.clone());
                    let pairs: Vec<String> = match items {
                        IrExpr::Array(els) => els.iter().map(|e| self.expr(e)).collect(),
                        _ => vec!["0".into()],
                    };
                    format!("(%{} = ({}))", ident(&name), pairs.join(", "))
                } else {
                    self.arrays.insert(name.clone());
                    let elems: Vec<String> = match items {
                        IrExpr::Array(els) => els.iter().map(|e| self.expr(e)).collect(),
                        _ => vec!["0".into()],
                    };
                    format!("(@{} = ({}))", ident(&name), elems.join(", "))
                }
            }
            "setArrayAppend" => {
                let (Some(name), Some(items)) = (Self::str_arg(args, 0), args.get(1)) else {
                    self.mark_todo("setArrayAppend args");
                    return "0".into();
                };
                self.arrays.insert(name.clone());
                let elems: Vec<String> = match items {
                    IrExpr::Array(els) => els.iter().map(|e| self.expr(e)).collect(),
                    _ => vec!["0".into()],
                };
                format!("(push @{}, {})", ident(&name), elems.join(", "))
            }
            "arrayIndex" => {
                let (Some(name), Some(key)) = (Self::str_arg(args, 0), args.get(1)) else {
                    self.mark_todo("arrayIndex args");
                    return "0".into();
                };
                // key is a rendered string expression of the index
                self.arrays.insert(name.clone());
                format!("${}[{}]", ident(&name), self.expr(key))
            }
            "listVar" | "arrayItems" => match Self::str_arg(args, 0) {
                Some(name) => self.array_ref(&name),
                None => {
                    self.mark_todo(&format!("{func} arg"));
                    "0".into()
                }
            },
            "arrayLen" => match Self::str_arg(args, 0) {
                Some(name) => {
                    self.arrays.insert(name.clone());
                    format!("scalar(@{})", ident(&name))
                }
                None => {
                    self.mark_todo("arrayLen arg");
                    "0".into()
                }
            },
            "redirect" => {
                if let Some(IrExpr::Arrow(stmts)) = args.first() {
                    self.mark_todo("redirect specs");
                    self.arrow_expr(stmts)
                } else {
                    self.mark_todo("redirect arg");
                    "0".into()
                }
            }
            "block" | "subshell" => match args.first() {
                Some(IrExpr::Arrow(stmts)) => self.arrow_expr(stmts),
                _ => {
                    self.mark_todo(&format!("{func} arg"));
                    "0".into()
                }
            },
            "cstyleFor" => self.cstyle_for_expr(args),
            "whileLoop" => {
                self.mark_todo("whileLoop expr");
                "0".into()
            }
            "shopt" => {
                self.mark_todo("shopt");
                "1".into()
            }
            "break" => "do { last; 0 }".to_string(),
            "continue" => "do { next; 0 }".to_string(),
            "return" => match args.first() {
                Some(v) => format!("do {{ return {}; }}", self.expr(v)),
                None => "do { return; 0 }".to_string(),
            },
            "unsupported" => {
                self.mark_todo("unsupported");
                "0".into()
            }
            _ => {
                if self.funcs.contains(func) {
                    let a: Vec<String> = args.iter().map(|x| self.expr(x)).collect();
                    format!("{}({})", ident(func), a.join(", "))
                } else {
                    self.mark_todo(&format!("call {func}"));
                    "0".into()
                }
            }
        }
    }

    fn arrow_expr(&mut self, stmts: &[IrStmt]) -> String {
        let mut inner = Vec::new();
        std::mem::swap(&mut self.out, &mut inner);
        let saved = self.depth;
        self.depth = 0;
        for s in stmts {
            self.stmt(s);
        }
        let body = self.out.join("\n");
        self.out = inner;
        self.depth = saved;
        format!("do {{\n{}\n}}", indent_block(&body, 1))
    }

    /// cstyleFor(header, condArrow, bodyArrow) — header is "init; cond; incr".
    fn cstyle_for_expr(&mut self, args: &[IrExpr]) -> String {
        let Some(hdr) = Self::str_arg(args, 0) else {
            self.mark_todo("cstyleFor header");
            return "0".into();
        };
        let parts: Vec<&str> = hdr.split(';').map(|s| s.trim()).collect();
        if parts.len() != 3 {
            self.mark_todo("cstyleFor header shape");
            return "0".into();
        }
        let init = self.arith_str(parts[0]);
        let cond = self.arith_str(parts[1]);
        let incr = self.arith_str(parts[2]);
        let body = match args.get(2) {
            Some(IrExpr::Arrow(stmts)) => {
                let mut inner = Vec::new();
                std::mem::swap(&mut self.out, &mut inner);
                let saved = self.depth;
                self.depth = 0;
                for s in stmts {
                    self.stmt(s);
                }
                let b = self.out.join("\n");
                self.out = inner;
                self.depth = saved;
                indent_block(&b, 1)
            }
            _ => String::new(),
        };
        format!("for ({init}; {cond}; {incr}) {{\n{body}\n}}")
    }

    /// exec in EXPRESSION position — the exit status of a spawned command.
    fn exec_expr(&mut self, args: &[IrExpr]) -> String {
        let cmd = Self::str_arg(args, 0).unwrap_or_default();
        let words = match args.get(1) {
            Some(IrExpr::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        if self.funcs.contains(&cmd) {
            let a: Vec<String> = words.iter().map(|w| self.expr(w)).collect();
            return format!("{}({})", ident(&cmd), a.join(", "));
        }
        let mut a: Vec<String> = vec![Self::perl_str(&cmd)];
        for w in &words {
            a.push(self.expr(w));
        }
        format!("system({})", a.join(", "))
    }

    /// exec as a STATEMENT — builtins lower natively, externals → system.
    fn exec_stmt(&mut self, args: &[IrExpr]) {
        let Some(cmd) = Self::str_arg(args, 0) else {
            self.mark_todo("exec cmd");
            return;
        };
        let words = match args.get(1) {
            Some(IrExpr::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        match cmd.as_str() {
            "echo" => self.echo_stmt(&words),
            "printf" => self.printf_stmt(&words),
            "cd" => {
                if let Some(dir) = words.first() {
                    let d = self.expr(dir);
                    self.emit(&format!("chdir({d}) or die \"cd: $!\\n\";"));
                } else {
                    self.emit("chdir($ENV{HOME} // '.') or die \"cd: $!\\n\";");
                }
            }
            "exit" => match words.first() {
                Some(code) => {
                    let e = self.expr(code);
                    self.emit(&format!("exit {e};"));
                }
                None => self.emit("exit 0;"),
            },
            "mkdir" => {
                for w in words.iter().filter(|w| match w {
                    IrExpr::Str(s, _) => s != "-p" && s != "-m" && !s.starts_with("-m"),
                    _ => true,
                }) {
                    for d in self.word_items(w) {
                        let d = Self::perl_str(&d);
                        self.emit(&format!("mkdir({d}) unless -d {d};"));
                    }
                }
            }
            "touch" => {
                for w in &words {
                    for f in self.word_items(w) {
                        let f = Self::perl_str(&f);
                        self.emit(&format!("open my $__fh, '>>', {f};"));
                        self.emit("close $__fh;");
                    }
                }
            }
            "rm" => {
                let mut files: Vec<String> = Vec::new();
                for w in words.iter().filter(|w| match w {
                    IrExpr::Str(s, _) => !s.starts_with('-'),
                    _ => true,
                }) {
                    for f in self.word_items(w) {
                        files.push(Self::perl_str(&f));
                    }
                }
                if files.is_empty() {
                    return;
                }
                let recursive = words.iter().any(|w| {
                    matches!(w, IrExpr::Str(s, _) if s == "-r" || s == "-R" || s == "-rf" || s == "-fr")
                });
                if recursive {
                    self.mark_todo("rm -r");
                }
                self.emit(&format!("unlink {};", files.join(", ")));
            }
            "read" => {
                let vars: Vec<String> = words
                    .iter()
                    .filter(|w| match w {
                        IrExpr::Str(s, _) => !s.starts_with('-'),
                        _ => true,
                    })
                    .map(|w| match w {
                        IrExpr::Str(s, _) => self.scalar_target(s),
                        _ => self.expr(w),
                    })
                    .collect();
                if vars.is_empty() {
                    self.emit("$_ = <STDIN>;");
                    self.emit("chomp;");
                } else if vars.len() == 1 {
                    self.emit(&format!("{} = <STDIN>;", vars[0]));
                    self.emit(&format!("chomp {};", vars[0]));
                } else {
                    self.emit(&format!(
                        "({}) = split /\\s+/, scalar(<STDIN>);",
                        vars.join(", ")
                    ));
                }
            }
            "shift" => {
                if self.in_func > 0 {
                    self.emit("shift;");
                } else {
                    self.emit("shift @ARGV;");
                }
            }
            "let" => {
                // `let expr` — arithmetic eval, exit 0 on nonzero result
                if let Some(IrExpr::Str(s, _)) = words.first() {
                    let a = self.arith_str(s);
                    self.emit(&format!("my $__r = {a};"));
                    self.emit("$? = ($__r != 0 ? 0 : 1);");
                }
            }
            "true" => self.emit("$? = 0;"),
            "false" => self.emit("$? = 1;"),
            "local" => {
                self.mark_todo("local");
            }
            "declare" => {
                // `declare -A map` / `declare -x NAME` — the hoisted
                // `my %map;`/`$ENV{NAME}` declarations cover the storage.
            }
            "set" => {
                self.mark_todo("set options");
            }
            "export" | "unset" | "source" | "." | "eval" | "trap" | "return" | "umask" | "type"
            | "hash" | "builtin" | "enable" | "help" | "logout" | "alias" | "unalias" | "times"
            | "ulimit" | "wait" | "getopts" | "shopt" => {
                self.mark_todo(&format!("builtin {cmd}"));
            }
            _ => {
                if self.funcs.contains(&cmd) {
                    let a: Vec<String> = words.iter().map(|w| self.expr(w)).collect();
                    self.emit(&format!("{}({});", ident(&cmd), a.join(", ")));
                    return;
                }
                let mut a: Vec<String> = vec![Self::perl_str(&cmd)];
                for w in &words {
                    a.push(self.expr(w));
                }
                self.emit(&format!("system({});", a.join(", ")));
            }
        }
    }

    fn echo_stmt(&mut self, words: &[IrExpr]) {
        let mut ws: Vec<IrExpr> = words.to_vec();
        let mut newline = true;
        if let Some(IrExpr::Str(s, _)) = ws.first() {
            if s == "-n" {
                newline = false;
                ws.remove(0);
            } else if s == "-e" {
                // interpret backslash escapes: keep simple, \n \t \\ \c
                ws.remove(0);
                let mut parts: Vec<String> = Vec::new();
                for w in &ws {
                    let mut p = self.expr(w);
                    if let IrExpr::Str(s, _) = w {
                        if s.contains('\\') {
                            p = Self::perl_str(
                                &s.replace("\\n", "\n")
                                    .replace("\\t", "\t")
                                    .replace("\\\\", "\\"),
                            );
                        }
                    }
                    parts.push(p);
                }
                if parts.is_empty() {
                    self.emit("print \"\\n\";");
                } else {
                    self.emit(&format!("print join(' ', {});", parts.join(", ")));
                }
                self.emit("print \"\\n\";");
                return;
            }
        }
        self.need_say = true;
        if ws.is_empty() {
            if newline {
                self.emit("say \"\";");
            } else {
                self.emit("print \"\";");
            }
            return;
        }
        let parts: Vec<String> = ws.iter().map(|w| self.expr(w)).collect();
        if newline {
            self.emit(&format!("say join(' ', {});", parts.join(", ")));
        } else {
            self.emit(&format!("print join(' ', {});", parts.join(", ")));
        }
    }

    fn printf_stmt(&mut self, words: &[IrExpr]) {
        let Some(fmt) = words.first() else {
            self.emit("print \"\";");
            return;
        };
        let fmt_str = self.expr(fmt);
        if words.len() == 1 {
            self.emit(&format!("print {fmt_str};"));
            return;
        }
        let args: Vec<String> = words[1..].iter().map(|w| self.expr(w)).collect();
        self.emit(&format!("printf({fmt_str}, {});", args.join(", ")));
    }

    // ── test expressions ─────────────────────────────────────────────

    /// Mini `[ ... ]` evaluator: `[ a -gt b ]`, `[ -n "$x" ]`, `[ -f f ]`,
    /// glob patterns for `==`/`!=`, `&&`/`||`/`!` combinators.
    fn test(&mut self, s: &str) -> String {
        // single-token forms with an embedded operator: `$(uname -r)==5.4.*`
        let trimmed = s.trim();
        if !trimmed.contains(' ') {
            for op in ["==", "!=", "="] {
                if let Some(pos) = trimmed.find(op) {
                    let (a, b) = (
                        trimmed[..pos].trim().to_string(),
                        trimmed[pos + op.len()..].trim().to_string(),
                    );
                    let lhs = self.test_value(&a);
                    let rhs = self.test_value(&b);
                    return self.test_compare(op, &lhs, &rhs, &a, &b);
                }
            }
            let v = self.test_value(trimmed);
            return format!("({v})");
        }
        let toks = self.test_tokens(trimmed);
        self.test_tokens_parse(&toks)
    }

    fn test_tokens(&mut self, s: &str) -> Vec<String> {
        let mut toks = Vec::new();
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace() {
                i += 1;
                continue;
            }
            if c == '"' || c == '\'' {
                let quote = c;
                let mut j = i + 1;
                let mut t = String::new();
                t.push(c);
                while j < chars.len() && chars[j] != quote {
                    t.push(chars[j]);
                    j += 1;
                }
                if j < chars.len() {
                    t.push(chars[j]);
                    i = j + 1;
                } else {
                    i = j;
                }
                toks.push(t);
                continue;
            }
            let mut j = i;
            while j < chars.len() && !chars[j].is_whitespace() {
                j += 1;
            }
            toks.push(chars[i..j].iter().collect());
            i = j;
        }
        toks
    }

    fn test_tokens_parse(&mut self, toks: &[String]) -> String {
        if toks.is_empty() {
            return "0".to_string();
        }
        // `!` negation
        if toks[0] == "!" {
            let inner = self.test_tokens_parse(&toks[1..]);
            return format!("(!{inner})");
        }
        // `&&` / `||` at top level
        for (i, t) in toks.iter().enumerate() {
            if t == "&&" || t == "||" {
                let l = self.test_tokens_parse(&toks[..i]);
                let r = self.test_tokens_parse(&toks[i + 1..]);
                return format!("({l} {} {r})", if t == "&&" { "&&" } else { "||" });
            }
        }
        match toks.len() {
            1 => {
                let v = self.test_value(&toks[0]);
                format!("({v})")
            }
            2 => {
                // -n/-z/file tests
                let (flag, v) = (&toks[0], self.test_value(&toks[1]));
                match flag.as_str() {
                    "-n" => format!("(({v}) ne \"\")"),
                    "-z" => format!("(({v}) eq \"\")"),
                    "-f" | "-d" | "-e" | "-s" | "-r" | "-w" | "-x" | "-L" | "-S" | "-p" | "-b"
                    | "-c" | "-g" | "-k" | "-t" | "-u" | "-G" | "-O" | "-N" => {
                        let pf = if flag == "-a" { "-e" } else { flag };
                        format!("({pf} ({v}))")
                    }
                    "-a" => format!("(-e ({v}))"),
                    _ => {
                        self.mark_todo(&format!("test flag {flag}"));
                        "0".into()
                    }
                }
            }
            3 => {
                let (a, op, b) = (&toks[0], &toks[1], &toks[2]);
                let raw_a = a.clone();
                let raw_b = b.clone();
                let (l, r) = (self.test_value(a), self.test_value(b));
                if op == "-nt" || op == "-ot" || op == "-ef" {
                    self.mark_todo(&format!("test {op}"));
                    return "0".into();
                }
                self.test_compare(op, &l, &r, &raw_a, &raw_b)
            }
            _ => {
                self.mark_todo(&format!("test shape {:?}", toks));
                "0".into()
            }
        }
    }

    fn test_compare(&mut self, op: &str, l: &str, r: &str, raw_l: &str, raw_r: &str) -> String {
        match op {
            "-gt" => format!("({l} > {r})"),
            "-lt" => format!("({l} < {r})"),
            "-ge" => format!("({l} >= {r})"),
            "-le" => format!("({l} <= {r})"),
            "-eq" => format!("({l} == {r})"),
            "-ne" => format!("({l} != {r})"),
            "=" | "==" | "!=" => {
                let has_glob = raw_l.contains('*')
                    || raw_l.contains('?')
                    || raw_r.contains('*')
                    || raw_r.contains('?');
                if has_glob {
                    // `[[ x == pattern ]]` — glob match
                    let re_l = glob_to_regex(raw_l, true);
                    let re_r = glob_to_regex(raw_r, true);
                    if op == "!=" {
                        let re = brace_escape(&re_r);
                        format!("(({l}) !~ m{{^{re}$}})")
                    } else {
                        let re = brace_escape(&re_r);
                        format!("(({l}) =~ m{{^{re}$}})")
                    }
                } else if op == "!=" {
                    format!("(({l}) ne ({r}))")
                } else {
                    format!("(({l}) eq ({r}))")
                }
            }
            _ => {
                self.mark_todo(&format!("test op {op}"));
                "0".into()
            }
        }
    }

    /// A test operand: `"$x"`/`'$x'`/`$x` → var ref; quoted text → string;
    /// number → literal; bareword → string.
    fn test_value(&mut self, t: &str) -> String {
        let t = t.trim();
        let inner = t
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| t.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(t);
        if let Some(name) = inner.strip_prefix('$') {
            if !name.is_empty() {
                return self.var_ref(name);
            }
        }
        if t.starts_with('$') && t.len() > 1 {
            return self.var_ref(&t[1..]);
        }
        if inner.parse::<i64>().is_ok() {
            return inner.to_string();
        }
        Self::perl_str(inner)
    }

    // ── param expansions ─────────────────────────────────────────────

    fn param(&mut self, args: &[IrExpr]) -> String {
        let Some(op) = Self::str_arg(args, 0) else {
            self.mark_todo("param op");
            return "0".into();
        };
        let Some(name) = Self::str_arg(args, 1) else {
            self.mark_todo("param name");
            return "0".into();
        };
        // `${arr[1]}` arrives with the index inside the name
        if let Some(open) = name.find('[') {
            if name.ends_with(']') && op.is_empty() {
                let var = &name[..open];
                let key = &name[open + 1..name.len() - 1];
                if key == "@" || key == "*" {
                    // `${arr[@]}` — all elements as a list
                    return self.array_ref(var);
                }
                let key_expr = if let Ok(n) = key.parse::<i64>() {
                    IrExpr::Int(n)
                } else if let Some(kname) = key.strip_prefix('$') {
                    IrExpr::Var(kname.to_string(), None)
                } else {
                    IrExpr::Str(key.to_string(), StrStyle::DoubleQuoted)
                };
                return self.index_ref(var, &key_expr);
            }
        }
        // `${#arr[@]}` — the serializer spells it slice("#arr", "@", "")
        if let Some(rest) = name.strip_prefix('#') {
            let rest = rest
                .strip_suffix("[@]")
                .or_else(|| rest.strip_suffix("[*]"))
                .unwrap_or(rest);
            if !rest.is_empty() && (op == "slice" || op == "len") {
                self.arrays.insert(rest.to_string());
                return format!("scalar(@{})", ident(rest));
            }
        }
        // `${!map[@]}` — keys of an associative array
        if let Some(rest) = name.strip_prefix('!') {
            let rest = rest
                .strip_suffix("[@]")
                .or_else(|| rest.strip_suffix("[*]"))
                .unwrap_or(rest);
            if !rest.is_empty() && (op == "slice" || op == "len") {
                self.hashes.insert(rest.to_string());
                return format!("keys %{}", ident(rest));
            }
        }
        // `${#arr[@]}` / `${arr[@]:off:len}` — array length / slice
        let is_array_name = name.ends_with("[@]") || name.ends_with("[*]");
        if is_array_name {
            let var = &name[..name.len() - 3];
            match op.as_str() {
                "len" | "#" => {
                    self.arrays.insert(var.to_string());
                    return format!("scalar(@{})", ident(var));
                }
                "slice" => {
                    self.arrays.insert(var.to_string());
                    let off_raw = args.get(2).and_then(|a| Self::str_arg(args, 2));
                    let len_raw = args.get(3).and_then(|a| Self::str_arg(args, 3));
                    let off = args
                        .get(2)
                        .map(|a| self.expr(a))
                        .unwrap_or_else(|| "0".into());
                    // `${arr[@]}` — the whole array as a list
                    if matches!(off_raw.as_deref(), Some("@") | Some("*")) {
                        return format!("@{}", ident(var));
                    }
                    if len_raw.as_deref() == Some("@") {
                        return format!("@{}[{off}..$#{}]", ident(var), ident(var));
                    }
                    let len = args
                        .get(3)
                        .map(|a| self.expr(a))
                        .unwrap_or_else(|| "0".into());
                    if len == "0" {
                        return format!("@{}[{off}..$#{}]", ident(var), ident(var));
                    }
                    return format!("@{}[{off}..({off})+({len})-1]", ident(var));
                }
                _ => {}
            }
        }
        let v = self.var_ref(&name);
        match op.as_str() {
            "" => v,
            ":-" => {
                let d = self.expr(&args[2]);
                format!("((({v} // \"\") ne \"\") ? {v} : {d})")
            }
            "-" => {
                let d = self.expr(&args[2]);
                format!("(defined({v}) ? {v} : {d})")
            }
            ":=" => {
                let d = self.expr(&args[2]);
                format!("((({v} // \"\") ne \"\") ? {v} : ({v} = {d}))")
            }
            ":+" => {
                let a = self.expr(&args[2]);
                format!("((({v} // \"\") ne \"\") ? {a} : \"\")")
            }
            "+" => {
                let a = self.expr(&args[2]);
                format!("(defined({v}) ? {a} : \"\")")
            }
            ":?" => {
                let m = self.expr(&args[2]);
                format!("((({v} // \"\") ne \"\") ? {v} : die {m})")
            }
            "len" => format!("length({v})"),
            "^" => format!("ucfirst({v})"),
            "^^" => format!("uc({v})"),
            "," => format!("lcfirst({v})"),
            ",," => format!("lc({v})"),
            "basename" => {
                self.need_basename = true;
                format!("basename({v})")
            }
            "dirname" => {
                self.need_basename = true;
                format!("dirname({v})")
            }
            "slice" => {
                let off_raw = Self::str_arg(args, 2);
                // `${arr[@]}` / `${arr[@]:off:}` — whole-array slices
                if matches!(off_raw.as_deref(), Some("@") | Some("*")) {
                    self.arrays.insert(name.clone());
                    return format!("@{}", ident(&name));
                }
                let off = args
                    .get(2)
                    .map(|a| self.expr(a))
                    .unwrap_or_else(|| "0".into());
                match args.get(3) {
                    Some(IrExpr::Str(s, _)) if s.is_empty() => format!("substr({v}, {off})"),
                    Some(a) => format!("substr({v}, {off}, {})", self.expr(a)),
                    None => format!("substr({v}, {off})"),
                }
            }
            "#" | "##" => {
                // shortest/longest prefix removal
                let pat = Self::str_arg(args, 2).unwrap_or_default();
                let re = glob_to_regex(&pat, op == "#");
                let re = brace_escape(&re);
                format!("do {{ my $__t = {v}; $__t =~ s{{^{re}}}//; $__t }}")
            }
            "%" | "%%" => {
                let pat = Self::str_arg(args, 2).unwrap_or_default();
                let re = glob_to_regex(&pat, op == "%");
                let re = brace_escape(&re);
                format!("do {{ my $__t = {v}; $__t =~ s{{{re}$}}//; $__t }}")
            }
            "//" | "/" => {
                let pat = Self::str_arg(args, 2).unwrap_or_default();
                let repl = Self::str_arg(args, 3).unwrap_or_default();
                let re = glob_to_regex(&pat, true);
                let g = if op == "//" { "g" } else { "" };
                let re = brace_escape(&re);
                format!(
                    "do {{ my $__t = {v}; $__t =~ s{{{re}}}{{{}}}{g}; $__t }}",
                    brace_escape(&repl)
                )
            }
            _ => {
                self.mark_todo(&format!("param op {op}"));
                "0".into()
            }
        }
    }

    // ── brace expansion (render-time evaluation) ─────────────────────

    fn brace(&mut self, args: &[IrExpr]) -> String {
        let out = self.brace_list(args);
        let lits: Vec<String> = out.iter().map(|s| Self::perl_str(s)).collect();
        format!("({})", lits.join(", "))
    }

    /// The brace expansion as raw strings (render-time evaluation).
    fn brace_list(&mut self, args: &[IrExpr]) -> Vec<String> {
        let prefix = Self::str_arg(args, 0).unwrap_or_default();
        let suffix = Self::str_arg(args, 3).unwrap_or_default();
        let mut groups: Vec<Vec<String>> = Vec::new();
        if let Some(IrExpr::Json(serde_json::Value::Array(gs))) = args.get(1) {
            for g in gs {
                groups.push(brace_group(g));
            }
        }
        let mut middles: Vec<String> = Vec::new();
        if let Some(IrExpr::Json(serde_json::Value::Array(ms))) = args.get(2) {
            for m in ms {
                if let serde_json::Value::String(s) = m {
                    middles.push(s.clone());
                }
            }
        }
        let mut combos: Vec<Vec<String>> = vec![Vec::new()];
        for g in &groups {
            let mut next: Vec<Vec<String>> = Vec::new();
            for c in &combos {
                for item in g {
                    let mut nc = c.clone();
                    nc.push(item.clone());
                    next.push(nc);
                }
            }
            combos = next;
        }
        combos
            .iter()
            .map(|c| {
                let mut s = prefix.clone();
                for (i, item) in c.iter().enumerate() {
                    s.push_str(item);
                    if let Some(m) = middles.get(i) {
                        s.push_str(m);
                    }
                }
                s.push_str(&suffix);
                s
            })
            .collect()
    }

    /// Render one exec word; brace-call words expand to their item list.
    fn word_items(&mut self, w: &IrExpr) -> Vec<String> {
        match w {
            IrExpr::Call { func, args } if func == "brace" => self.brace_list(args),
            other => vec![self.expr(other)],
        }
    }

    // ── statements ───────────────────────────────────────────────────

    fn stmt(&mut self, s: &IrStmt) {
        match s {
            IrStmt::Expr(e) => match e {
                IrExpr::Call { func, args } => match func.as_str() {
                    "exec" => self.exec_stmt(args),
                    "pipeline" => {
                        let mut stages: Vec<String> = Vec::new();
                        if let Some(IrExpr::Array(items)) = args.first() {
                            for it in items {
                                if let IrExpr::Arrow(stmts) = it {
                                    stages.push(self.shell_cmd(stmts, "; "));
                                }
                            }
                        }
                        if stages.is_empty() {
                            self.mark_todo("pipeline stages");
                        } else {
                            let cmd = self.qx(&stages.join(" | "));
                            self.emit(&format!("{cmd};"));
                        }
                    }
                    "break" => self.emit("last;"),
                    "continue" => self.emit("next;"),
                    "return" => {
                        if let Some(v) = args.first() {
                            let e = self.expr(v);
                            self.emit(&format!("return {e};"));
                        } else {
                            self.emit("return;");
                        }
                    }
                    "setVar" => {
                        if let (Some(name), Some(value)) = (Self::str_arg(args, 0), args.get(1)) {
                            let t = self.scalar_target(&name);
                            let e = self.expr(value);
                            self.emit(&format!("{t} = {e};"));
                        } else {
                            self.mark_todo("setVar stmt args");
                        }
                    }
                    "assign" => {
                        if let (Some(name), Some(op)) =
                            (Self::str_arg(args, 0), Self::str_arg(args, 1))
                        {
                            let t = self.scalar_target(&name);
                            if op == "++" || op == "--" {
                                self.emit(&format!("{t}{op};"));
                            } else if let Some(value) = args.get(2) {
                                let e = self.expr(value);
                                self.emit(&format!("{t} {op} {e};"));
                            }
                        } else {
                            self.mark_todo("assign stmt args");
                        }
                    }
                    "test" => {
                        let x = self.expr(e);
                        self.emit(&format!("{x};"));
                    }
                    "let" => {
                        let x = self.expr(e);
                        self.emit(&format!("{x};"));
                    }
                    _ => {
                        let x = self.expr(e);
                        self.emit(&format!("{x};"));
                    }
                },
                _ => {
                    let x = self.expr(e);
                    self.emit(&format!("{x};"));
                }
            },
            IrStmt::Output {
                value,
                newline,
                target,
            } => {
                let v = self.expr(value);
                match target {
                    Some(fh) => {
                        if *newline {
                            self.need_say = true;
                            self.emit(&format!("say {{${fh}}} {v};"));
                        } else {
                            self.emit(&format!("print {{${fh}}} {v};"));
                        }
                    }
                    None => {
                        if *newline {
                            self.need_say = true;
                            self.emit(&format!("say {v};"));
                        } else {
                            self.emit(&format!("print {v};"));
                        }
                    }
                }
            }
            IrStmt::WriteFile {
                path,
                content,
                append,
            } => {
                let p = self.expr(path);
                let c = self.expr(content);
                let mode = if *append { ">>" } else { ">" };
                self.emit(&format!(
                    "open my $__fh, {mode:?}, {p} or die \"Cannot open {p}: $!\\n\";"
                ));
                self.emit(&format!("print {{$__fh}} {c};"));
                self.emit("close $__fh;");
            }
            IrStmt::Assign { targets, expr } => {
                let Some(t) = targets.first() else {
                    self.mark_todo("multi-target assign");
                    return;
                };
                // `map[foo]=bar` arrives with the index inside the var name
                if let Some(open) = t.var.find('[') {
                    if t.var.ends_with(']') && t.indices.is_empty() {
                        let var = &t.var[..open];
                        let key = &t.var[open + 1..t.var.len() - 1];
                        let key_expr = if let Ok(n) = key.parse::<i64>() {
                            IrExpr::Int(n)
                        } else {
                            IrExpr::Str(key.to_string(), StrStyle::DoubleQuoted)
                        };
                        let target = self.index_ref(var, &key_expr);
                        let e = self.expr(expr);
                        self.emit(&format!("{target} = {e};"));
                        return;
                    }
                }
                if !t.indices.is_empty() {
                    let key = &t.indices[0];
                    let target = self.index_ref(&t.var, key);
                    let e = self.expr(expr);
                    self.emit(&format!("{target} = {e};"));
                    return;
                }
                // `arr=(...)` / `arr+=(...)` arrive as Assign over a
                // setArray/setArrayAppend call — emit the store directly
                if let IrExpr::Call { func, args } = expr {
                    if func == "setArray" || func == "setArrayAppend" {
                        let x = self.expr(expr);
                        self.emit(&format!("{x};"));
                        return;
                    }
                }
                let lhs = self.scalar_target(&t.var);
                // compound folding: $x = $x op $y → $x op= $y
                if let IrExpr::BinOp {
                    lhs: inner,
                    op,
                    rhs,
                } = expr
                {
                    if let IrExpr::Var(name, _) = inner.as_ref() {
                        if *name == t.var {
                            let cop = match op {
                                BinOpKind::Add => Some("+="),
                                BinOpKind::Sub => Some("-="),
                                BinOpKind::Mul => Some("*="),
                                BinOpKind::Div => Some("/="),
                                BinOpKind::Concat => Some(".="),
                                _ => None,
                            };
                            if let Some(op) = cop {
                                let e = self.expr(rhs);
                                self.emit(&format!("{lhs} {op} {e};"));
                                return;
                            }
                        }
                    }
                }
                let e = self.expr(expr);
                self.emit(&format!("{lhs} = {e};"));
            }
            IrStmt::Declare { vars, init, .. } => {
                let init_expr = init.as_ref().map(|e| self.expr(e));
                for (i, d) in vars.iter().enumerate() {
                    let t = self.scalar_target(&d.name);
                    if i == 0 {
                        match &init_expr {
                            Some(v) => self.emit(&format!("{t} = {v};")),
                            None => self.emit(&format!("{t} = undef;")),
                        }
                    } else {
                        self.emit(&format!("{t} = undef;"));
                    }
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                self.arrays.insert(var.clone());
                let elems: Vec<String> = elements.iter().map(|e| self.expr(e)).collect();
                self.emit(&format!("@{} = ({});", ident(var), elems.join(", ")));
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
                    self.emit(&format!("}} elsif ({ec}) {{"));
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
                self.loop_vars.insert(var.clone());
                // the loop var aliases the hoisted `my $var` (NOT a fresh
                // loop-lexical): shell keeps the final value after the loop
                if !is_env_style_var_name(var)
                    && !var.is_empty()
                    && var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    self.scalars.insert(var.clone());
                }
                let items = match iter {
                    IrExpr::Array(items) => {
                        let l: Vec<String> = items.iter().map(|i| self.expr(i)).collect();
                        l.join(", ")
                    }
                    IrExpr::Range { start, end } => format!("{start}..{end}"),
                    IrExpr::Call { func, args } if func == "brace" => self.brace(args),
                    IrExpr::Call { func, args } if func == "seq" => {
                        let a: Vec<String> = args.iter().map(|x| self.expr(x)).collect();
                        format!("1..{}", a.join(".."))
                    }
                    other => {
                        self.mark_todo("for iter");
                        self.expr(other)
                    }
                };
                let v = ident(var);
                // shell keeps the loop var's final value after the loop, so
                // the loop runs over a fresh iterator variable and copies it
                // into the hoisted `$var` each iteration
                self.emit(&format!("for my $__loop_{v} ({items}) {{"));
                self.depth += 1;
                self.emit(&format!("${v} = $__loop_{v};"));
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
                self.emit("do {");
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                let kw = if *until { "until" } else { "while" };
                self.emit(&format!("}} {kw} ({c});"));
            }
            IrStmt::Case {
                discriminant,
                clauses,
            } => {
                let disc = self.expr(discriminant);
                let mut first = true;
                for clause in clauses {
                    let alts: Vec<String> = clause
                        .patterns
                        .iter()
                        .map(|p| {
                            // `*` default clause → match everything
                            glob_to_regex(p, true)
                        })
                        .collect();
                    let re = brace_escape(&alts.join("|"));
                    if first {
                        self.emit(&format!("if (({disc}) =~ m{{^(?:{re})$}}) {{"));
                        first = false;
                    } else {
                        self.emit(&format!("}} elsif (({disc}) =~ m{{^(?:{re})$}}) {{"));
                    }
                    self.depth += 1;
                    for s in &clause.body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                }
                if !clauses.is_empty() {
                    self.emit("}");
                } else {
                    self.mark_todo("case clauses");
                }
            }
            IrStmt::Function { name, body, .. } => {
                self.funcs.insert(name.clone());
                let mut saved = self.in_func;
                self.in_func += 1;
                self.emit(&format!("sub {} {{", ident(name)));
                self.depth += 1;
                for s in body {
                    self.stmt(s);
                }
                self.depth -= 1;
                self.emit("}");
                self.in_func = saved;
            }
            IrStmt::Subshell(body) => {
                self.mark_todo("subshell");
                self.block_stmt(body);
            }
            IrStmt::Background(body) => {
                self.mark_todo("background");
                self.block_stmt(body);
            }
            IrStmt::Block(body) => self.block_stmt(body),
            IrStmt::Redirect { inner, .. } => {
                self.mark_todo("redirect stmt");
                for s in inner {
                    self.stmt(s);
                }
            }
            IrStmt::Exec {
                cmd, args, capture, ..
            } => {
                let mut words: Vec<IrExpr> = Vec::new();
                if let Some(IrExpr::Array(items)) = args.first() {
                    words.extend(items.iter().cloned());
                }
                if let Some(var) = capture {
                    let t = self.scalar_target(var);
                    let c = match cmd {
                        IrExpr::Str(cmd_s, _) => {
                            let mut a = vec![shell_squote(cmd_s)];
                            for w in &words {
                                a.push(self.shell_word(w));
                            }
                            a.join(" ")
                        }
                        _ => String::new(),
                    };
                    let q = self.qx(&c);
                    self.emit(&format!("{t} = {q};"));
                    self.emit(&format!("chomp {t};"));
                } else {
                    self.exec_stmt(&{
                        let mut a = vec![cmd.clone()];
                        a.push(IrExpr::Array(words));
                        a
                    });
                }
            }
            IrStmt::Pipeline {
                capture, cmd_str, ..
            } => match capture {
                Some(var) => {
                    let t = self.scalar_target(var);
                    if let Some(cs) = cmd_str {
                        let q = self.qx(cs);
                        self.emit(&format!("{t} = {q};"));
                    } else {
                        self.mark_todo("pipeline capture cmd_str");
                    }
                }
                None => {
                    if let Some(cs) = cmd_str {
                        self.emit_qx_stmt(cs);
                    } else {
                        self.mark_todo("pipeline stmt");
                    }
                }
            },
            IrStmt::Return(e) => match e {
                Some(v) => {
                    let e = self.expr(v);
                    self.emit(&format!("return {e};"))
                }
                None => self.emit("return;"),
            },
            IrStmt::Exit(e) => match e {
                Some(v) => {
                    let e = self.expr(v);
                    self.emit(&format!("exit {e};"))
                }
                None => self.emit("exit 0;"),
            },
            IrStmt::SetChildError(e) => {
                let e = self.expr(e);
                self.emit(&format!("$? = {e};"));
            }
            IrStmt::Die { expr, carp } => {
                let v = self.expr(expr);
                if *carp {
                    self.emit(&format!("croak {v};"));
                } else {
                    self.emit(&format!("die {v};"));
                }
            }
            IrStmt::Warn { expr, carp } => {
                let v = self.expr(expr);
                if *carp {
                    self.emit(&format!("carp {v};"));
                } else {
                    self.emit(&format!("warn {v};"));
                }
            }
            IrStmt::Require(m) => self.emit(&format!("require \"{m}\";")),
            IrStmt::RawText(t) => self.emit(t),
            IrStmt::Label(name) | IrStmt::Goto(name) => {
                let kind = if matches!(s, IrStmt::Label(_)) {
                    "label"
                } else {
                    "goto"
                };
                self.mark_todo(&format!(
                    "{kind} {name} not restructured by restructure_goto"
                ));
            }
            IrStmt::ForInit { .. } => self.mark_todo("ForInit (strip_cfor should have lowered it)"),
            IrStmt::Continue => self.emit("next;"),
            IrStmt::Break => self.emit("last;"),
            IrStmt::Try { .. } => self.mark_todo("try"),
        }
    }

    fn block_stmt(&mut self, body: &[IrStmt]) {
        self.emit("{");
        self.depth += 1;
        for s in body {
            self.stmt(s);
        }
        self.depth -= 1;
        // the trailing `;` keeps an empty bare block a valid statement
        // (`{ }` followed by another statement is a Perl syntax error)
        self.emit("};");
    }

    fn sub(&mut self, s: &IrSub) {
        self.funcs.insert(s.name.clone());
        let mut saved = self.in_func;
        self.in_func += 1;
        self.emit(&format!("sub {} {{", ident(&s.name)));
        self.depth += 1;
        for st in &s.body {
            self.stmt(st);
        }
        self.depth -= 1;
        self.emit("}");
        self.in_func = saved;
    }

    fn collect_funcs(&mut self, stmts: &[IrStmt]) {
        for s in stmts {
            match s {
                IrStmt::Function { name, body, .. } => {
                    self.funcs.insert(name.clone());
                    self.collect_funcs(body);
                }
                IrStmt::Block(b)
                | IrStmt::Subshell(b)
                | IrStmt::Background(b)
                | IrStmt::Redirect { inner: b, .. } => self.collect_funcs(b),
                IrStmt::If {
                    then,
                    elsifs,
                    else_,
                    ..
                } => {
                    self.collect_funcs(then);
                    for (_, b) in elsifs {
                        self.collect_funcs(b);
                    }
                    self.collect_funcs(else_);
                }
                IrStmt::For { body, .. }
                | IrStmt::While { body, .. }
                | IrStmt::DoWhile { body, .. } => self.collect_funcs(body),
                IrStmt::Case { clauses, .. } => {
                    for c in clauses {
                        self.collect_funcs(&c.body);
                    }
                }
                _ => {}
            }
        }
    }
}

// ── free helpers ─────────────────────────────────────────────────────

/// Sanitize a shell variable name into a Perl identifier.
fn ident(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push('_');
    }
    let first = out.chars().next().unwrap();
    if first.is_ascii_digit() {
        out.insert(0, '_');
    }
    out
}

/// Shell single-quote a literal (no Perl interpolation in the qx string).
fn shell_squote(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        match c {
            '\'' => out.push_str("'\\''"),
            '$' => out.push_str("\\$"),
            '@' => out.push_str("\\@"),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// Indent every line of a rendered block by `n` levels.
fn indent_block(s: &str, n: usize) -> String {
    let pad = "    ".repeat(n);
    s.lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("{pad}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Escape `{`/`}` so a regex body survives `m{...}` / `s{...}{...}`
/// delimiters.
fn brace_escape(re: &str) -> String {
    let mut out = String::new();
    for c in re.chars() {
        match c {
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            c => out.push(c),
        }
    }
    out
}

/// Shell glob → Perl regex (anchored fragments). `greedy` selects `.*` vs
/// `.*?` for `*` (shortest-prefix semantics like `${x#pat}`).
fn glob_to_regex(pat: &str, greedy: bool) -> String {
    let star = if greedy { ".*" } else { ".*?" };
    let mut out = String::new();
    let chars: Vec<char> = pat.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '*' => out.push_str(star),
            '?' => out.push('.'),
            '[' => {
                // character class: pass through; a lone `[` (no closing
                // bracket) is a literal
                let mut j = i + 1;
                let mut cls = String::from("[");
                let mut first = true;
                let mut closed = false;
                while j < chars.len() {
                    let cc = chars[j];
                    if cc == ']' && !first {
                        cls.push(']');
                        closed = true;
                        break;
                    }
                    if cc == '\\' {
                        cls.push('\\');
                    }
                    cls.push(cc);
                    first = false;
                    j += 1;
                }
                if closed {
                    out.push_str(&cls);
                    i = j;
                } else {
                    out.push_str("\\[");
                }
            }
            '\\' => {
                if i + 1 < chars.len() {
                    out.push('\\');
                    out.push(chars[i + 1]);
                    i += 1;
                }
            }
            '.' | '+' | '(' | ')' | '^' | '$' | '|' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
        i += 1;
    }
    out
}

// ── brace expansion (mirror of harness/sh2-namespace.mjs) ────────────

fn brace_group(g: &serde_json::Value) -> Vec<String> {
    let items = match g {
        serde_json::Value::Array(items) => items,
        _ => return Vec::new(),
    };
    let has_range = items
        .iter()
        .any(|it| it.is_object() && it.get("range").map(|r| r.is_array()).unwrap_or(false));
    let mut out = Vec::new();
    for it in items {
        match it {
            serde_json::Value::String(s) => out.push(s.clone()),
            o if o.is_object() && o.get("range").map(|r| r.is_array()).unwrap_or(false) => {
                if items.len() == 1 {
                    out.extend(brace_range(o.get("range").unwrap()));
                } else {
                    // bash: a range inside a comma group stays LITERAL
                    let r = o.get("range").unwrap();
                    out.push(format!(
                        "{}..{}",
                        r[0].as_str().unwrap_or(""),
                        r[1].as_str().unwrap_or("")
                    ));
                }
            }
            o if o.is_object() && o.get("nested").is_some() => {
                out.extend(brace_nested(o.get("nested").unwrap()));
            }
            serde_json::Value::Array(sub) => {
                out.extend(brace_nested(&serde_json::Value::Array(sub.clone())))
            }
            _ => {}
        }
    }
    out
}

fn brace_nested(items: &serde_json::Value) -> Vec<String> {
    let items = match items {
        serde_json::Value::Array(items) => items,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for it in items {
        match it {
            serde_json::Value::String(s) => out.push(s.clone()),
            o if o.is_object() && o.get("range").map(|r| r.is_array()).unwrap_or(false) => {
                out.extend(brace_range(o.get("range").unwrap()));
            }
            o if o.is_object() && o.get("nested").is_some() => {
                out.extend(brace_nested(o.get("nested").unwrap()));
            }
            serde_json::Value::Array(sub) => {
                out.extend(brace_nested(&serde_json::Value::Array(sub.clone())))
            }
            _ => {}
        }
    }
    out
}

fn brace_range(r: &serde_json::Value) -> Vec<String> {
    let arr = match r.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    let get = |i: usize| -> String {
        arr.get(i)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let start = get(0);
    let end = get(1);
    let step: i64 = arr
        .get(2)
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0)
        .abs()
        .max(1);
    let is_num = |s: &str| -> bool {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_digit() || (c == '-' && s.len() > 1))
    };
    let fmt = |n: i64, width: usize| -> String {
        let s = n.abs().to_string();
        let padded = if width > 0 {
            format!("{:0>width$}", s, width = width)
        } else {
            s
        };
        if n < 0 {
            format!("-{padded}")
        } else {
            padded
        }
    };
    if is_num(&start) && is_num(&end) {
        let (a, b): (i64, i64) = (start.parse().unwrap(), end.parse().unwrap());
        let width = if start.starts_with('0') || end.starts_with('0') {
            start.len().max(end.len())
        } else {
            0
        };
        let mut out = Vec::new();
        if a <= b {
            let mut n = a;
            while n <= b {
                out.push(fmt(n, width));
                n += step;
            }
        } else {
            let mut n = a;
            while n >= b {
                out.push(fmt(n, width));
                n -= step;
            }
        }
        return out;
    }
    // alpha runs
    let single = |s: &str| s.len() == 1 && s.chars().next().unwrap().is_ascii_alphabetic();
    if single(&start) && single(&end) {
        let (ca, cb) = (
            start.chars().next().unwrap() as u8,
            end.chars().next().unwrap() as u8,
        );
        let mut out = Vec::new();
        if ca <= cb {
            let mut c = ca;
            while c <= cb {
                out.push((c as char).to_string());
                c = c.saturating_add(step as u8).min(cb + 1);
            }
        } else {
            let mut c = ca;
            while c >= cb {
                out.push((c as char).to_string());
                c = c.saturating_sub(step as u8).max(cb.saturating_sub(1));
            }
        }
        return out;
    }
    vec![format!("{start}..{end}")]
}
