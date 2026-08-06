//! C backend renderer — LIBRARY interface (worktree-local, branch
//! `backend/c`). Consumes the ShIR directly in-process, bypassing the
//! `--shir` JSON contract (ask B of docs/backend-c-core-needs.md §1):
//! `shir_to_c(&IrProgram) -> String`.
//!
//! Uses the core's A2 type verdicts (`IrProgram.var_types`): `Int` vars →
//! C `long long` narrowed by the range analysis (`analyze_var_ranges` +
//! `range_width_name`, M8 spike) to `unsigned int`/`int` when the
//! conservative [lo, hi] provably fits — the var AND every arith expr
//! mentioning it must stay in width (a var's width covers its arithmetic
//! RESULTS, not just its own values). `Str` vars → `char*`, anything
//! else → runtime store (`char*` + sh2.* stubs in this draft).
//! Identifiers are mangled against C keywords (A6-consistent). Everything
//! outside the lowable subset (numeric arith, echo/printf, if/else,
//! simple assignment) emits a compile-able `sh2.*` stub or a
//! `/* TODO(unsupported) */` marker, so the draft always compiles.
//!
//! Also consumes the core's conservative string-length analysis
//! (`IrProgram.var_lengths`, fbedac4): a Str var with a known bound N
//! gets a FIXED buffer `char v[N+1]` (the fixed-buffer transform the
//! analysis was built for), with DEBUG-ONLY length asserts (`assert()`,
//! compiled out under NDEBUG) at the function boundary and BEFORE every
//! copy into the buffer — the write that would overflow is UB, and the
//! assert is the debug-mode tripwire; under NDEBUG `strncpy` truncates.
//! Unbounded (None) vars stay `char*`.
//!
//! The naive string/number coercion here is the residual "C needs type
//! inference" gap PLAN.md v2 flagged (the numeric width side is now
//! covered by the range analysis; the string side stays open).

use crate::ir::{ArithAst, IrExpr, IrProgram, IrStmt, IrType, InterpPart, VarKind};
use std::collections::{BTreeSet, HashMap};

enum Part {
    Lit(String),
    /// Arg(cexpr, spec) — the printf specifier for the operand,
    /// precomputed at construction where the IrExpr is in scope.
    Arg(String, NumSpec),
}

/// How to print a printf/snprintf argument.
///
/// `Num(spec, cast)`: the operand is numeric. `spec` matches the
/// operand's PROVEN width (`%u`/`%d`/`%lld` for u32/i32/i64) and `cast`
/// says whether a `(long long)` wrap is still required. The pair is
/// always consistent: cast == true implies spec == "%lld" (the cast pins
/// the vararg type to long long), and cast == false implies the operand's
/// C type is provably exactly the spec's expected type.
/// `Str`: non-numeric — `%s` + a `(char*)` cast (stub calls return
/// `long long`; printf("%s", long long) is UB).
#[derive(Debug)]
enum NumSpec {
    Num(&'static str, bool),
    Str,
}

impl PartialEq for NumSpec {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (NumSpec::Num(a, ca), NumSpec::Num(b, cb)) => a == b && ca == cb,
            (NumSpec::Str, NumSpec::Str) => true,
            _ => false,
        }
    }
}

/// C width from the core's range analysis (`range_width_name`): an
/// Int-typed var whose conservative [lo, hi] value range provably fits
/// a narrower type than `long long` is declared at that width. Ordering
/// for widening: I64 > I32 > U32 (by signed capacity — the analysis
/// stays consistent, so a var's own range and the ranges of the arith
/// exprs mentioning it always share a common width).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Width {
    U32,
    I32,
    I64,
}

impl Width {
    fn c_type(self) -> &'static str {
        match self {
            Width::U32 => "unsigned int",
            Width::I32 => "int",
            Width::I64 => "long long",
        }
    }

    /// printf-family format for the width's C type. The format matches
    /// the DECLARED type exactly, so a cast is only needed when the
    /// operand's actual C type can't be proven to be it (see
    /// [`Render::expr_type_matches`] / [`Render::num_spec`]).
    fn format(self) -> &'static str {
        match self {
            Width::U32 => "%u",
            Width::I32 => "%d",
            Width::I64 => "%lld",
        }
    }

    fn from_range_name(name: &str) -> Width {
        match name {
            "u32" => Width::U32,
            "i32" => Width::I32,
            _ => Width::I64,
        }
    }

    fn widen(self, other: Width) -> Width {
        match (self, other) {
            (Width::I64, _) | (_, Width::I64) => Width::I64,
            (Width::I32, _) | (_, Width::I32) => Width::I32,
            _ => Width::U32,
        }
    }
}

#[derive(Default)]
pub struct Render {
    out: Vec<String>,
    depth: usize,
    /// var name -> type verdict (A2); missing = Any (runtime store)
    var_types: HashMap<String, IrType>,
    /// var name -> conservative max string length (fbedac4's
    /// analyze_string_lengths); None = unbounded. Only vars in the
    /// analysis' assign set appear.
    var_lengths: HashMap<String, Option<u64>>,
    /// var name -> conservative [lo, hi] (analyze_var_ranges + the
    /// Range/seq for-iter seeds the analysis doesn't track).
    var_ranges: HashMap<String, (i128, i128)>,
    /// var name -> const/var verdict (the const-markup analysis): `Const`
    /// vars with a single literal top-level assignment render as C
    /// `const` declarations initialized from that literal, and the
    /// assignment statement is dropped.
    const_vars: HashMap<String, VarKind>,
    /// name -> the single top-level `Assign` RHS of a `Const` var (the
    /// hoisted `const` declaration's initializer).
    const_rhs: HashMap<String, IrExpr>,
    /// names already emitted as `const` (the matching Assign stmt is
    /// skipped at emission time).
    const_lifted: BTreeSet<String>,
    /// var name -> effective C width: the widest of the var's own range
    /// and every arith-expr result range mentioning it (a var's width
    /// must cover its arithmetic results, not just its own values).
    var_widths: HashMap<String, Width>,
    /// shell function names defined in the program (Function stmts) —
    /// calls to these render as `name();` instead of a sh2.* stub.
    functions: BTreeSet<String>,
    /// the definitions themselves (name, body) — emitted in the
    /// preamble (BEFORE main: C has no nested function definitions).
    fn_defs: Vec<(String, Vec<IrStmt>)>,
    /// distinct sh2.* callee names that need stubs
    sh2_calls: BTreeSet<String>,
    need_upper: bool,
    need_lower: bool,
    need_slice: bool,
    /// named-temp counter for statement-level snprintf buffers
    /// (`char _sN[cap]; snprintf(_sN, ...)` before the enclosing stmt)
    temp_seq: usize,
    todo: usize,
}

/// Bounded Str vars get a fixed buffer of bound+1 bytes; unbounded or
/// over-cap vars stay `char*`. Aligned with the analysis' own CAP.
const FIXED_BUF_CAP: u64 = 1024;

/// Render an `IrProgram` to C source (main() body).
pub fn shir_to_c(prog: &IrProgram) -> String {
    let mut prog = prog.clone();
    // A2 + var_lengths: the analyses run at serialization time in the
    // JSON path; the library path must run the same ones.
    prog.var_types = crate::shir::analyze_var_types(&prog);
    prog.var_lengths = crate::shir::analyze_string_lengths(&prog);
    prog.var_const = crate::shir::analyze_var_const(&prog);
    // Range analysis (M8 spike): conservative [lo, hi] per assigned var,
    // + the Range/seq for-iter seeds the analysis doesn't track (loop
    // vars are excluded from its assign set).
    let mut ranges = crate::shir::analyze_var_ranges(&prog);
    seed_loop_var_ranges(&prog.stmts, &mut ranges);
    // Effective widths: a var's width must cover every arith-expr result
    // mentioning it (e.g. i in [1, 70000] is u32, but i*i needs i64), so
    // narrow only when the var AND all its arithmetic stay in width.
    let widths = effective_widths(&prog, &ranges);
    let mut r = Render::default();
    r.var_types = prog.var_types.iter().cloned().collect();
    r.var_lengths = prog.var_lengths.iter().cloned().collect();
    r.const_vars = prog.var_const.iter().cloned().collect();
    r.const_rhs = const_assign_rhs(&prog.stmts, &r.const_vars);
    r.var_ranges = ranges;
    r.var_widths = widths;
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

    /// The fixed-buffer bound for a Str var (Some(N) -> `char v[N+1]`),
    /// or None (stay `char*`). INT vars are excluded — the length
    /// analysis also bounds numeric RHS (`i=$((i+1))` -> 20), and
    /// `strlen`/`strncpy` on a `long long` is itself UB.
    fn buf_bound(&self, name: &str) -> Option<u64> {
        if self.is_num(name) {
            return None;
        }
        self.var_lengths
            .get(name)
            .copied()
            .flatten()
            .filter(|&b| b <= FIXED_BUF_CAP)
    }

    /// `name = rhs` into a fixed buffer of size b+1: the DEBUG-ONLY
    /// length assert fires BEFORE the copy (the UB-triggering write);
    /// NDEBUG compiles it out and strncpy truncates (null-terminated).
    /// Non-string RHS exprs (the "0" placeholder for unlowered
    /// Interpolate etc.) lower to the empty string — copying a bogus
    /// pointer would itself be UB.
    fn emit_guarded_copy(&mut self, name: &str, b: u64, rhs: &str) {
        let rhs_c = format!("(char*)({rhs})");
        let stringy = rhs.starts_with('"')
            || rhs.starts_with("(char*)")
            || rhs.starts_with("sh2_")
            || is_ident(rhs);
        if stringy {
            self.emit(&format!("assert(strlen({rhs_c}) <= {b});"));
            self.emit(&format!("strncpy({name}, {rhs_c}, {b} + 1);"));
            self.emit(&format!("{name}[{b}] = '\\0';"));
        } else {
            self.emit(&format!("{name}[0] = '\\0';"));
        }
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
                    // `!` is UNARY — the ShIR duplicates the operand
                    // (until loops: BinOp{Not, test, test}), so render
                    // the negation of the lhs and ignore the rhs copy.
                    crate::ir::BinOpKind::Not => return format!("(!({l}))"),
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
            IrExpr::Interpolate(parts) => {
                // a pure-literal interpolation lowers to the concatenated
                // string literal; parts with vars stay TODO (no runtime
                // store in this draft — the "0" placeholder is caught by
                // emit_guarded_copy's non-string path, never copied).
                let mut lit = String::new();
                let mut all_lit = true;
                for p in parts {
                    match p {
                        InterpPart::Lit(t) => lit.push_str(t),
                        InterpPart::Expr(_) => {
                            all_lit = false;
                            break;
                        }
                    }
                }
                if all_lit {
                    Self::cstr(&lit)
                } else {
                    self.mark_todo("Interpolate expr");
                    "0".into()
                }
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
            ArithAst::Assign { var, op, rhs } => {
                // `x op= rhs` — native (the zero-divisor /%= cases are
                // kept on the runtime by the core, so op is safe here)
                format!("{} {op}= {}", self.c_ident(var), self.arith(rhs))
            }
            ArithAst::IncDec { var, delta, prefix } => {
                // `++x` / `x++` / `--x` / `x--` (delta ±1)
                let name = self.c_ident(var);
                if *prefix {
                    format!("{}{}", if *delta >= 0 { "++" } else { "--" }, name)
                } else {
                    format!("{}{}", name, if *delta >= 0 { "++" } else { "--" })
                }
            }
        }
    }



    fn sh2_stub(&mut self, name: &str, _args: &[IrExpr], note: &str) -> String {
        self.sh2_calls.insert(name.to_string());
        self.mark_todo(&format!("{note} → sh2.{name}"));
        format!("sh2_{name}()")
    }

    /// A sh2.*-free no-op call: `exec ":"` / `exec "true"` (and the
    /// always-false `exec "false"`). Setup/cleanup wrappers in the
    /// shellbench runners are exactly these — skipping them (instead of
    /// a sh2_exec stub) is what makes the loop body render natively.
    fn noop_value(&self, func: &str, args: &[IrExpr]) -> Option<&'static str> {
        if func == "exec" {
            if let Some(IrExpr::Str(cmd, _)) = args.first() {
                return match cmd.as_str() {
                    ":" | "true" => Some("1"),
                    "false" => Some("0"),
                    // declaration builtins — the hoist already declares
                    // the vars (`local x` / `typeset x` / `declare x`
                    // with no initializer are pure declarations).
                    "local" | "declare" | "typeset" | "export" | "readonly" => Some("1"),
                    _ => None,
                };
            }
        }
        None
    }

    fn noop_value_call(&self, e: &IrExpr) -> bool {
        matches!(
            e,
            IrExpr::Call { func, args } if self.noop_value(func, args).is_some()
        )
    }

    /// Render `var`'s declaration (Int -> the narrowed width, bounded Str
    /// -> the fixed buffer, else char*). Shared by the main hoist and the
    /// per-function hoists.
    fn emit_var_decl(&mut self, v: &str) {
        let name = self.c_ident(v);
        // const-markup lift: a Const var whose single top-level
        // assignment is a literal renders as a const declaration
        // initialized from that literal; the Assign stmt is dropped
        // (see the Assign arm). Only literal RHSs are lifted — a
        // non-literal init would need a runtime write (and possibly a
        // var reference declared later in the hoist order).
        if let Some(rhs) = self.const_rhs.get(v).cloned() {
            // numeric vars need a numeric initializer: the Str RHS parses
            // as an integer (that is exactly the numeric lift's criterion)
            let init = if self.is_num(v) {
                match &rhs {
                    IrExpr::Int(i) => Some(i.to_string()),
                    IrExpr::Str(s, _) => s.trim().parse::<i128>().ok().map(|n| n.to_string()),
                    _ => None,
                }
            } else {
                self.literal_init(&rhs)
            };
            if let Some(init) = init {
                self.const_lifted.insert(v.to_string());
                if self.is_num(v) {
                    self.emit(&format!("const {} {name} = {init};", self.width_of_var(v).c_type()));
                } else if let Some(b) = self.buf_bound(v) {
                    self.emit(&format!("const char {name}[{}] = {init};", b + 1));
                } else {
                    self.emit(&format!("const char* {name} = {init};"));
                }
                return;
            }
        }
        if self.is_num(v) {
            self.emit(&format!("{} {name} = 0;", self.width_of_var(v).c_type()));
        } else if let Some(b) = self.buf_bound(v) {
            // the fixed-buffer transform: the var_lengths analysis
            // proves len(v) <= b, so the buffer is b+1 bytes
            self.emit(&format!("char {name}[{}] = \"\";", b + 1));
        } else {
            self.emit(&format!("char* {name} = NULL;"));
        }
    }

    /// Render an expression as a C compile-time constant initializer, or
    /// None when it isn't one (var refs, calls, captures, interpolation
    /// with expression parts). Ints and string literals (incl. pure-
    /// literal interpolations) qualify.
    fn literal_init(&mut self, e: &IrExpr) -> Option<String> {
        match e {
            IrExpr::Int(i) => Some(i.to_string()),
            IrExpr::Str(s, _) => Some(Self::cstr(s)),
            IrExpr::Interpolate(parts) => {
                let mut s = String::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(l) => s.push_str(l),
                        InterpPart::Expr(_) => return None,
                    }
                }
                Some(Self::cstr(&s))
            }
            _ => None,
        }
    }

    /// Emit the DEBUG-ONLY length invariants (assert, NDEBUG-out) for the
    /// bounded vars among `vars` — at function boundaries.
    fn emit_bound_asserts(&mut self, vars: &BTreeSet<String>) {
        for v in vars {
            if let Some(b) = self.buf_bound(v) {
                let name = self.c_ident(v);
                self.emit(&format!("assert(strlen({name}) <= {b});"));
            }
        }
    }

    /// Emit one shell function as `static void NAME(void) { ... }`
    /// (preamble position — C has no nested function definitions).
    /// Always emitted: a `:`-body function may be CALLED (shellbench
    /// func:func wraps the call in @begin/@end) — dropping the
    /// definition would make the call an undefined symbol.
    fn emit_function(&mut self, name: &str, body: &[IrStmt]) {
        let fname = self.c_ident(name);
        self.emit(&format!("static void {fname}(void) {{"));
        self.depth += 1;
        // per-function hoist: vars ASSIGNED inside the function are
        // declared locally (a var only READ is the caller's — main's
        // hoist owns it; redeclaring would shadow it and break sharing).
        let mut fvars: BTreeSet<String> = BTreeSet::new();
        collect_assigned_vars(body, &mut fvars);
        for n in &fvars {
            self.emit_var_decl(n);
        }
        if !fvars.is_empty() {
            self.emit("");
            self.emit_bound_asserts(&fvars);
            self.emit("");
        }
        for st in body {
            self.stmt(st);
        }
        self.depth -= 1;
        self.emit("}");
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
                    // `:` / `true` / `false` — no-op builtins (setup()/
                    // cleanup() wrappers, `while true` conditions).
                    if let Some(v) = self.noop_value("exec", args) {
                        return v.to_string();
                    }
                    // `let "i++"` / `let "x+=1"` — the ((...)) builtin's
                    // string form (the core emits it when the var is
                    // typeset -i / let-declared). Parse the common shapes.
                    if cmd == "let" {
                        if let Some(IrExpr::Array(items)) = args.get(1) {
                            if let Some(IrExpr::Str(expr, _)) = items.first() {
                                if let Some(c) = self.let_render(expr) {
                                    return c;
                                }
                            }
                        }
                        return self.sh2_stub("let", args, "let");
                    }
                    // a defined shell function's call (the Expr
                    // stmt arm appends the ';')
                    if self.functions.contains(cmd) {
                        return format!("{}()", self.c_ident(cmd));
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
            // contains(needle, pattern) — PureCpu (the grep -q / case *P*)
            // lift). The core only emits contains for provably-literal
            // patterns, so strstr is exact-substring == the semantic.
            // Inlined directly — no wrapper (static tiny fns are inlined
            // at -O anyway; `inline` is an ODR/header mechanism).
            "contains" => {
                if let (Some(needle), Some(pattern)) = (args.first(), args.get(1)) {
                    let needle_c = if self.expr_is_num(needle) {
                        let t = format!("_s{}", self.temp_seq);
                        self.temp_seq += 1;
                        // size the temp to the value's proven width:
                        // u32 11 ("4294967295"), i32 12 ("-2147483648"),
                        // i64 21 ("-9223372036854775808") — each + NUL.
                        // snprintf truncates, so this is headroom, not
                        // correctness; the range analysis proves the value
                        // fits (its width is what we sized against).
                        let width = self.expr_width(needle);
                        let cap = width_buf_len(width);
                        self.emit(&format!("char {t}[{cap}];"));
                        let e = self.expr(needle);
                        // format by the PROVEN width; the (long long) cast
                        // only when the operand's C type can't be proven to
                        // match the specifier (a var read or arith over
                        // vars at that width is provable — `%u`/`%d` need
                        // no cast; literals/stubs keep the %lld pair)
                        let NumSpec::Num(spec, cast) = self.num_spec(needle) else {
                            unreachable!("numeric needle → numeric spec")
                        };
                        let arg = if cast {
                            format!("(long long)({e})")
                        } else {
                            e
                        };
                        self.emit(&format!(
                            "snprintf({t}, sizeof {t}, \"{spec}\", {arg});"
                        ));
                        t
                    } else {
                        self.expr(needle)
                    };
                    // no (char*) casts: arrays/literals decay to const
                    // char* implicitly
                    return format!(
                        "strstr({needle_c}, {}) != NULL",
                        self.expr(pattern)
                    );
                }
                self.sh2_stub("contains", args, "contains")
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
            // everything else → a defined shell function's call, or a
            // compile-able sh2.* stub
            _ if self.functions.contains(func) => format!("{}();", self.c_ident(func)),
            _ => self.sh2_stub(func, args, func),
        }
    }

    /// `let` — the ((...)) builtin arrives as a STRING ("i++", "x+=1").
    /// Parse the common single-assignment shapes natively; anything
    /// else → None (the caller stubs). No trailing ';' — the Expr
    /// stmt arm appends it.
    fn let_render(&self, s: &str) -> Option<String> {
        let s = s.trim();
        if let Some(rest) = s.strip_suffix("++") {
            let n = rest.trim();
            if is_ident(n) {
                return Some(format!("{}++", self.c_ident(n)));
            }
        }
        if let Some(rest) = s.strip_suffix("--") {
            let n = rest.trim();
            if is_ident(n) {
                return Some(format!("{}--", self.c_ident(n)));
            }
        }
        for op in ["+=", "-=", "*=", "/=", "%="] {
            if let Some((l, r)) = s.split_once(op) {
                let l = l.trim();
                if is_ident(l) && r.trim().parse::<i64>().is_ok() {
                    return Some(format!("{} {op} {}", self.c_ident(l), r.trim()));
                }
            }
        }
        None
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
    /// literal; a plain quoted string → the literal. Anything else
    /// (unresolved `$name`, positional `$#`/`$1`, compounds) → a sh2.*
    /// stub — NEVER a bare string: `$#` used to render `"#"`, and
    /// `("#" < 2)` is a pointer-vs-int comparison (UB — 900_if2echo
    /// silently skipped its if-body).
    fn test_value(&mut self, t: &str) -> String {
        let raw = t.trim();
        let dequoted = raw.trim_matches('"');
        let stripped = dequoted.strip_prefix('$').unwrap_or(dequoted);
        if self.var_types.contains_key(stripped) {
            self.c_ident(stripped)
        } else if let Ok(n) = stripped.parse::<i64>() {
            n.to_string()
        } else if dequoted.starts_with('$')
            || !stripped
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            // unresolved $-name / positional / compound operand
            self.sh2_stub("test", &[], "test operand")
        } else {
            Self::cstr(stripped)
        }
    }

    /// Split an expression into printf parts: Lit(text) | Arg(cexpr, is_num).
    fn parts_of(&mut self, e: &IrExpr) -> Vec<Part> {
        match e {
            IrExpr::Str(s, _) => vec![Part::Lit(s.clone())],
            IrExpr::Int(i) => vec![Part::Arg(i.to_string(), self.num_spec(e))],
            IrExpr::Var(name, _) => {
                if self.is_num(name) {
                    vec![Part::Arg(self.c_ident(name), self.num_spec(e))]
                } else {
                    vec![Part::Arg(self.c_ident(name), NumSpec::Str)]
                }
            }
            IrExpr::Ident(name) => vec![Part::Arg(self.c_ident(name), NumSpec::Str)],
            IrExpr::Interpolate(parts) => {
                let mut out = Vec::new();
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => out.push(Part::Lit(s.clone())),
                        InterpPart::Expr(x) => {
                            let spec = if self.expr_is_num(x) {
                                self.num_spec(x)
                            } else {
                                NumSpec::Str
                            };
                            out.push(Part::Arg(self.expr(x), spec))
                        }
                    }
                }
                out
            }
            IrExpr::Arith(a) => vec![Part::Arg(self.arith(a), self.num_spec(e))],
            IrExpr::BinOp { .. } => vec![Part::Arg(self.expr(e), self.num_spec(e))],
            IrExpr::Call { func, args } => {
                let args = args.clone();
                // getVar("x") → ident if x is typed, else stub
                if func == "getVar" {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        if self.var_types.contains_key(name) {
                            let spec = if self.is_num(name) {
                                self.num_spec(e)
                            } else {
                                NumSpec::Str
                            };
                            return vec![Part::Arg(self.c_ident(name), spec)];
                        }
                    }
                    return vec![Part::Arg(self.call(func, &args), NumSpec::Str)];
                }
                // other calls: render the call expression; numeric iff the
                // expression is numeric (stubs return long long → %lld+cast)
                let spec = if self.expr_is_num(e) {
                    self.num_spec(e)
                } else {
                    NumSpec::Str
                };
                vec![Part::Arg(self.call(func, &args), spec)]
            }
            other => {
                self.mark_todo(&format!("echo arg {:?}", other));
                vec![Part::Arg("0".into(), NumSpec::Num("%lld", true))]
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
                Part::Arg(v, spec) => match spec {
                    // numeric: the spec already matches the operand's
                    // proven width — cast only when the type is unproven
                    // (num_spec's invariant: cast ⟺ spec == "%lld")
                    NumSpec::Num(spec, cast) => {
                        fmt.push_str(spec);
                        if cast {
                            cargs.push(format!("(long long)({v})"));
                        } else {
                            cargs.push(v);
                        }
                    }
                    NumSpec::Str => {
                        fmt.push_str("%s");
                        // cast: the arg may be a stub call returning
                        // long long — printf("%s", long long) is UB.
                        cargs.push(format!("(char*)({v})"));
                    }
                },
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
                // const-markup lift: the declaration already carries the
                // literal initializer (emit_var_decl) — the assignment
                // statement is redundant (the verdict guarantees this is
                // the var's only write).
                if self.const_lifted.contains(&t.var) {
                    return;
                }
                let name = self.c_ident(&t.var);
                if let Some(b) = self.buf_bound(&t.var) {
                    // a bounded string var: the debug-only length assert
                    // fires BEFORE the write that would overflow the
                    // fixed buffer (see emit_guarded_copy).
                    let rhs = self.expr(expr);
                    self.emit_guarded_copy(&name, b, &rhs);
                    return;
                }
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
                        self.emit(&format!("{} {name} = {v};", self.width_of_var(&d.name).c_type()));
                    } else if let Some(b) = self.buf_bound(&d.name) {
                        self.emit(&format!("char {name}[{}] = \"\";", b + 1));
                        if let Some(v) = init_expr.clone() {
                            self.emit_guarded_copy(&name, b, &v);
                        }
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
                // seq-range lift: `for x in $(seq a b)` (captureWords →
                // arrow → exec "seq") OR a core-lowered IrExpr::Range →
                // traditional numeric C loop. The A2 verdict for the loop
                // var is usually Str (captureWords returns strings), so the
                // lift overrides it to Int for the loop scope and restores
                // it afterwards.
                if let Some((first, last, step)) = seq_iter_range(iter) {
                    let name = self.c_ident(var);
                    let prev_type = self.var_types.get(var).copied();
                    self.var_types.insert(var.clone(), IrType::Int);
                    let cmp = if step > 0 { "<=" } else { ">=" };
                    let upd = match step {
                        1 => format!("{name}++"),
                        -1 => format!("{name}--"),
                        s => format!("{name} += {s}"),
                    };
                    // the loop var's width comes from the range analysis
                    // (seed + every arith expr in the body that mentions
                    // it) — `for (int i = 1; ...)` when [lo, hi] fits
                    let ty = self.width_of_var(var).c_type();
                    self.emit(&format!(
                        "for ({ty} {name} = {first}; {name} {cmp} {last}; {upd}) {{"
                    ));
                    self.depth += 1;
                    for s in body {
                        self.stmt(s);
                    }
                    self.depth -= 1;
                    self.emit("}");
                    match prev_type {
                        Some(t) => {
                            self.var_types.insert(var.clone(), t);
                        }
                        None => {
                            self.var_types.remove(var);
                        }
                    }
                    return;
                }
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
            IrStmt::While { cond, body } => {
                // the cond is an IrExpr (the ShIR's `[ ... ]` is a
                // Call("test") -> test_render, `while true` -> "1")
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
                if *until {
                    self.emit(&format!("}} while (!({c}));"));
                } else {
                    self.emit(&format!("}} while ({c});"));
                }
            }
            IrStmt::Function { .. } => {
                // definitions are emitted in the preamble (before main);
                // calls arrive as exec("<name>") and render `name();`.
            }
            other => self.mark_todo(&format!("stmt {:?}", other)),
        }
    }

    /// The effective C width of an Int-typed var (from the range
    /// analysis; missing = no proof → long long).
    fn width_of_var(&self, name: &str) -> Width {
        self.var_widths.get(name).copied().unwrap_or(Width::I64)
    }

    /// The width of a numeric expression: a typed var's width, or the
    /// range-derived width of an arith result (None range → i64). Used to
    /// size stringification temps exactly.
    fn expr_width(&self, e: &IrExpr) -> Width {
        match e {
            IrExpr::Var(name, _) | IrExpr::Ident(name) => self.width_of_var(name),
            IrExpr::Arith(a) => {
                let state: HashMap<String, Option<(i128, i128)>> = self
                    .var_ranges
                    .iter()
                    .map(|(k, v)| (k.clone(), Some(*v)))
                    .collect();
                match arith_range_local(a, &state) {
                    Some((lo, hi)) => {
                        Width::from_range_name(crate::shir::range_width_name(lo, hi))
                    }
                    None => Width::I64,
                }
            }
            // `$y` read of a typed var renders as the declared ident —
            // its width is the var's declared width, not the I64 fallback
            // (without this, `echo $i` would keep the %lld cast)
            IrExpr::Call { func, args } if func == "getVar" => {
                match args.first() {
                    Some(IrExpr::Str(name, _)) if self.var_types.contains_key(name) => {
                        self.width_of_var(name)
                    }
                    _ => Width::I64,
                }
            }
            _ => Width::I64,
        }
    }

    /// The printf spec for a numeric operand: format by the PROVEN width,
    /// cast only when the operand's C type can't be proven to match.
    ///
    /// Invariant: cast == true ⟺ spec == "%lld". When the type is known
    /// (a var read / arith over vars at that width), the spec matches the
    /// actual C type — `%u` on an `unsigned int`, `%d` on an `int`,
    /// `%lld` on a `long long` — and no cast is emitted. When it is not
    /// (int literals, stub calls, unproven arith), the `(long long)` cast
    /// pins the vararg type to match `%lld` — the pair is always
    /// consistent, so a casted operand never meets a `%u`/`%d`.
    fn num_spec(&self, e: &IrExpr) -> NumSpec {
        let w = self.expr_width(e);
        if self.expr_type_matches(e, w) {
            NumSpec::Num(w.format(), false)
        } else {
            NumSpec::Num("%lld", true)
        }
    }

    /// Can the rendered C expression of `e` be proven to have exactly the
    /// C type of width `w` — so the width's printf format matches without
    /// a cast? True for a read of a variable declared at `w` (Var/Ident,
    /// or getVar of a typed var, which renders as the declared ident), and
    /// for arithmetic whose variable leaves are all at `w` (C's usual
    /// arithmetic conversions keep the result at the leaf type; int
    /// literals convert up). Everything else — int literals, stubs,
    /// BinOp — is conservative (cast kept).
    fn expr_type_matches(&self, e: &IrExpr, w: Width) -> bool {
        match e {
            IrExpr::Var(name, _) | IrExpr::Ident(name) => {
                // the var must be genuinely numeric (a string var's width
                // defaults to I64 but its C type is `char*` — never match)
                self.is_num(name) && self.width_of_var(name) == w
            }
            IrExpr::Arith(a) => {
                let mut has_var = false;
                arith_leaves_at_width(a, self, w, &mut has_var)
                    // a pure-Num arith renders as `int` — only matches I32
                    && (has_var || w == Width::I32)
            }
            IrExpr::Call { func, args } if func == "getVar" => {
                // `$y` read of a typed var renders as the declared ident
                matches!(
                    args.first(),
                    Some(IrExpr::Str(name, _))
                        if self.is_num(name) && self.width_of_var(name) == w
                )
            }
            _ => false,
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
        // top-level pre-declaration (they are declared inside the loop),
        // and the shell function names (their calls render `name();`).
        let mut vars: BTreeSet<String> = BTreeSet::new();
        let mut for_vars: BTreeSet<String> = BTreeSet::new();
        collect_vars_full(&prog.stmts, &mut vars, &mut for_vars);
        // collect function definitions at ANY depth (a function may be
        // defined inside a block/loop — the shellbench eval benches do).
        collect_fn_defs(&prog.stmts, &mut self.functions, &mut self.fn_defs);
        // vars that appear ONLY inside function bodies (var_types covers
        // them too, but they must NOT be hoisted into main — the
        // function declares its own copy).
        let mut fn_only: BTreeSet<String> = BTreeSet::new();
        for (_, body) in &self.fn_defs {
            let mut fv = BTreeSet::new();
            collect_vars(body, &mut fv);
            for v in &fv {
                if !vars.contains(v) {
                    fn_only.insert(v.clone());
                }
            }
        }
        for (n, _) in &prog.var_types {
            vars.insert(n.clone());
        }
        for v in &for_vars {
            vars.remove(v);
        }
        for v in &fn_only {
            vars.remove(v);
        }

        // Pass 2: render the body first (helper flags known before preamble).
        let mut body_out = Vec::new();
        std::mem::swap(&mut self.out, &mut body_out);
        self.depth = 1;
        for v in &vars {
            self.emit_var_decl(v);
        }
        if !vars.is_empty() {
            self.emit("");
            // DEBUG-ONLY length invariants at the function boundary
            // (assert() compiles out under NDEBUG): every bounded var
            // must still fit its analysis bound.
            self.emit_bound_asserts(&vars);
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
        self.emit("#include <assert.h>"); // debug-only length asserts (NDEBUG compiles out)
        self.emit("");
        // shell functions rendered FIRST (into a side buffer) so the
        // sh2.* stub set is complete before the stubs are emitted —
        // definition-before-use: a function body calling a stub must
        // see its definition (an implicit declaration then the real
        // definition is a conflicting-types error).
        let fn_defs = std::mem::take(&mut self.fn_defs);
        let mut fn_out = Vec::new();
        let saved_out = std::mem::replace(&mut self.out, Vec::new());
        for (name, body) in &fn_defs {
            self.emit_function(name, body);
        }
        fn_out = std::mem::replace(&mut self.out, saved_out);
        self.fn_defs = fn_defs;
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
        self.out.extend(fn_out.iter().cloned());
        if !fn_out.is_empty() {
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

/// Collect Function definitions at any depth (names + bodies).
fn collect_fn_defs(
    stmts: &[IrStmt],
    names: &mut BTreeSet<String>,
    defs: &mut Vec<(String, Vec<IrStmt>)>,
) {
    for s in stmts {
        match s {
            IrStmt::Function { name, body } => {
                names.insert(name.clone());
                defs.push((name.clone(), body.clone()));
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_fn_defs(then, names, defs);
                for (_, b) in elsifs {
                    collect_fn_defs(b, names, defs);
                }
                collect_fn_defs(else_, names, defs);
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => collect_fn_defs(body, names, defs),
            _ => {}
        }
    }
}

/// For every `Const`-verdict var: the single TOP-LEVEL `Assign` targeting
/// it (straight-line, no indices). The const markup alone allows
/// conditional single sites; the C backend lifts only the unconditional
/// top-level ones (a hoisted initializer must always run). The verdict
/// guarantees at most one site, so the first match is the only one.
fn const_assign_rhs(stmts: &[IrStmt], const_vars: &HashMap<String, VarKind>) -> HashMap<String, IrExpr> {
    let mut out = HashMap::new();
    for s in stmts {
        if let IrStmt::Assign { targets, expr } = s {
            for t in targets {
                if t.indices.is_empty()
                    && const_vars.get(&t.var) == Some(&VarKind::Const)
                    && !out.contains_key(&t.var)
                {
                    out.insert(t.var.clone(), expr.clone());
                }
            }
        }
    }
    out
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
            // loop bodies assign/read vars — hoist them (they are
            // ordinary top-level vars, unlike for-loop counters).
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                collect_vars(body, out)
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => collect_vars(b, out),
            _ => {}
        }
    }
}

/// Collect vars ASSIGNED in a statement list (Assign/Declare targets,
/// arith x=/x++/x--), not mere reads — the per-function hoist declares
/// exactly these (a read-only var is the caller's).
fn collect_assigned_vars(stmts: &[IrStmt], out: &mut BTreeSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    out.insert(t.var.clone());
                }
                collect_assigned_expr(expr, out);
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    out.insert(d.name.clone());
                }
                if let Some(e) = init {
                    collect_assigned_expr(e, out);
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                collect_assigned_vars(then, out);
                for (_, b) in elsifs {
                    collect_assigned_vars(b, out);
                }
                collect_assigned_vars(else_, out);
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body) => collect_assigned_vars(body, out),
            IrStmt::Expr(e) => collect_assigned_expr(e, out),
            _ => {}
        }
    }
}

fn collect_assigned_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Arith(a) => collect_assigned_arith(a, out),
        _ => {}
    }
}

fn collect_assigned_arith(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        _ => {}
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
        IrExpr::Call { func, args } if func == "exec" => {
            // `let "i++"` hides its var inside a STRING arg — the hoist
            // must see it or the loop var is undeclared in C.
            if let Some(IrExpr::Str(cmd, _)) = args.first() {
                if cmd == "let" {
                    if let Some(IrExpr::Array(items)) = args.get(1) {
                        if let Some(IrExpr::Str(expr, _)) = items.first() {
                            if let Some(n) = let_var_name(expr) {
                                out.insert(n);
                            }
                        }
                    }
                }
            }
            for a in args {
                collect_vars_expr(a, out);
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

/// The variable a `let` string operates on ("i++", "++i", "x+=1").
fn let_var_name(s: &str) -> Option<String> {
    let s = s.trim();
    let s = s
        .strip_prefix("++")
        .or_else(|| s.strip_prefix("--"))
        .unwrap_or(s)
        .trim();
    let mut end = 0;
    for (i, c) in s.char_indices() {
        if c.is_ascii_alphanumeric() || c == '_' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if end > 0 {
        Some(s[..end].to_string())
    } else {
        None
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

/// A plain C identifier (a mangled var name or a string-literal-less
/// expression is NOT — used to decide whether an RHS is a string value
/// that may be length-asserted before a guarded copy).
fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().unwrap().is_ascii_alphabetic()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ── numeric-range wiring (core's analyze_var_ranges / range_width_name) ──

/// Byte size (chars + '\0') of a decimal string for the given width:
///   u32 `"4294967295"`        → 10 + 1 = 11
///   i32 `"-2147483648"`       → 11 + 1 = 12
///   i64 `"-9223372036854775808"` → 20 + 1 = 21 (u64's 20 digits too,
///      so 21 is the universal 64-bit bound)
/// All variable leaves of an arith tree must be declared at width `w`
/// (the rendered C expression then has `w`'s C type: `unsigned int ×
/// unsigned int → unsigned int`, `int × int → int`, `long long × long
/// long → long long`, and an `int` literal operand converts up). `has_var`
/// records whether any Var/Assign/IncDec leaf was seen (a pure-Num tree
/// renders as `int`, which only matches I32). Index leaves are stubbed to
/// `0` (`int`) — unprovable, returns false.
fn arith_leaves_at_width(a: &ArithAst, r: &Render, w: Width, has_var: &mut bool) -> bool {
    match a {
        ArithAst::Num(_) => true,
        ArithAst::Var(name) => {
            *has_var = true;
            // genuinely numeric (a string var's width default I64 must not
            // match; its rendered type is `char*`)
            r.is_num(name) && r.width_of_var(name) == w
        }
        ArithAst::Index { .. } => false,
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_leaves_at_width(lhs, r, w, has_var)
                && arith_leaves_at_width(rhs, r, w, has_var)
        }
        ArithAst::Un { arg, .. } => arith_leaves_at_width(arg, r, w, has_var),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            arith_leaves_at_width(test, r, w, has_var)
                && arith_leaves_at_width(then, r, w, has_var)
                && arith_leaves_at_width(else_, r, w, has_var)
        }
        ArithAst::Assign { var, rhs, .. } => {
            *has_var = true;
            r.is_num(var) && r.width_of_var(var) == w
                && arith_leaves_at_width(rhs, r, w, has_var)
        }
        ArithAst::IncDec { var, .. } => {
            *has_var = true;
            r.is_num(var) && r.width_of_var(var) == w
        }
    }
}

fn width_buf_len(w: Width) -> usize {
    match w {
        Width::U32 => 11,
        Width::I32 => 12,
        Width::I64 => 21,
    }
}

/// Detect a `Range` iterable and the shell `for x in $(seq a b)` shape
/// (core-lowered `Array([Range])` or pre-lift captureWords → arrow →
/// exec "seq"); returns (first, last, step). Anything else → None.
fn seq_iter_range(iter: &IrExpr) -> Option<(i128, i128, i128)> {
    match iter {
        IrExpr::Range { start, end } => Some((*start as i128, *end as i128, 1i128)),
        IrExpr::Array(items) if items.len() == 1 => match items.first() {
            Some(IrExpr::Range { start, end }) => Some((*start as i128, *end as i128, 1i128)),
            Some(cap) => seq_capture_words(cap),
            None => None,
        },
        _ => None,
    }
}

/// Parse the pre-lift `captureWords → arrow → exec "seq"` iterable
/// (seq [FIRST [INCREMENT]] LAST); None → not a numeric seq.
fn seq_capture_words(cap: &IrExpr) -> Option<(i128, i128, i128)> {
    let IrExpr::Call { func, args } = cap else {
        return None;
    };
    if func != "captureWords" {
        return None;
    }
    let arrow = args.first()?;
    let IrExpr::Arrow(body) = arrow else {
        return None;
    };
    if body.len() != 1 {
        return None;
    }
    let stmt = body.first()?;
    let exec_call = match stmt {
        IrStmt::Expr(e) => e,
        _ => return None,
    };
    let IrExpr::Call { func, args } = exec_call else {
        return None;
    };
    if func != "exec" {
        return None;
    }
    let IrExpr::Str(cmd, _) = args.first()? else {
        return None;
    };
    if cmd != "seq" {
        return None;
    }
    let IrExpr::Array(seqargs) = args.get(1)? else {
        return None;
    };
    if seqargs.is_empty() || seqargs.len() > 3 {
        return None;
    }
    let num = |e: &IrExpr| -> Option<i128> {
        match e {
            IrExpr::Str(s, _) => s.trim().parse::<i128>().ok(),
            IrExpr::Int(n) => Some(*n as i128),
            _ => None,
        }
    };
    let last = num(seqargs.last()?)?;
    let (first, step) = match seqargs.len() {
        1 => (1, 1),
        2 => (num(&seqargs[0])?, 1),
        _ => (num(&seqargs[0])?, num(&seqargs[1])?),
    };
    if step == 0 {
        return None;
    }
    Some((first, last, step))
}

/// Seed loop-var ranges from Range/seq For iters — `analyze_var_ranges`
/// doesn't track for-loop bindings (its For arm marks body-assigned vars
/// unbounded). Nested loops and branches are walked; an existing range
/// joins (widens) with the seed.
fn seed_loop_var_ranges(stmts: &[IrStmt], ranges: &mut HashMap<String, (i128, i128)>) {
    for s in stmts {
        match s {
            IrStmt::For { var, iter, body } => {
                if let Some((first, last, _)) = seq_iter_range(iter) {
                    let (lo, hi) = (first.min(last), first.max(last));
                    match ranges.get(var) {
                        Some((l0, h0)) => {
                            ranges.insert(var.clone(), ((*l0).min(lo), (*h0).max(hi)));
                        }
                        None => {
                            ranges.insert(var.clone(), (lo, hi));
                        }
                    }
                }
                seed_loop_var_ranges(body, ranges);
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                seed_loop_var_ranges(b, ranges);
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                seed_loop_var_ranges(then, ranges);
                for (_, b) in elsifs {
                    seed_loop_var_ranges(b, ranges);
                }
                seed_loop_var_ranges(else_, ranges);
            }
            IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
                seed_loop_var_ranges(body, ranges);
            }
            IrStmt::Redirect { inner, .. } => seed_loop_var_ranges(inner, ranges),
            _ => {}
        }
    }
}

/// Effective C width per Int-typed var: the widest of the var's own
/// [lo, hi] (range_width_name) and every arith-expr result range that
/// mentions it. Sound: a var's width must cover the RESULTS of the
/// arithmetic computed on it, not just its own values — `i` in
/// [1, 70000] is u32, but `(i * i)` needs i64. An arith expr whose range
/// is unknown (None) forces i64 — no proof, no narrowing.
fn effective_widths(
    prog: &IrProgram,
    ranges: &HashMap<String, (i128, i128)>,
) -> HashMap<String, Width> {
    let state: HashMap<String, Option<(i128, i128)>> =
        ranges.iter().map(|(k, v)| (k.clone(), Some(*v))).collect();
    let mut widths: HashMap<String, Width> = HashMap::new();
    for (name, (lo, hi)) in ranges {
        widths.insert(
            name.clone(),
            Width::from_range_name(crate::shir::range_width_name(*lo, *hi)),
        );
    }
    walk_widths_stmts(&prog.stmts, &state, &mut widths);
    widths
}

fn walk_widths_stmts(
    stmts: &[IrStmt],
    state: &HashMap<String, Option<(i128, i128)>>,
    widths: &mut HashMap<String, Width>,
) {
    for s in stmts {
        match s {
            IrStmt::Assign { expr, .. } => walk_widths_expr(expr, state, widths),
            IrStmt::Declare { init, .. } => {
                if let Some(e) = init {
                    walk_widths_expr(e, state, widths);
                }
            }
            IrStmt::DeclareArray { elements, .. } => {
                for e in elements {
                    walk_widths_expr(e, state, widths);
                }
            }
            IrStmt::Output { value, .. } => walk_widths_expr(value, state, widths),
            IrStmt::WriteFile { path, content, .. } => {
                walk_widths_expr(path, state, widths);
                walk_widths_expr(content, state, widths);
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                walk_widths_expr(cond, state, widths);
                walk_widths_stmts(then, state, widths);
                for (c, b) in elsifs {
                    walk_widths_expr(c, state, widths);
                    walk_widths_stmts(b, state, widths);
                }
                walk_widths_stmts(else_, state, widths);
            }
            IrStmt::For { iter, body, .. } => {
                walk_widths_expr(iter, state, widths);
                walk_widths_stmts(body, state, widths);
            }
            IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
                walk_widths_expr(cond, state, widths);
                walk_widths_stmts(body, state, widths);
            }
            IrStmt::Exit(e) | IrStmt::Return(e) => {
                if let Some(x) = e {
                    walk_widths_expr(x, state, widths);
                }
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
                walk_widths_expr(expr, state, widths);
            }
            IrStmt::SetChildError(e) => walk_widths_expr(e, state, widths),
            IrStmt::Expr(e) => walk_widths_expr(e, state, widths),
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                walk_widths_stmts(b, state, widths);
            }
            IrStmt::Redirect { inner, redirects } => {
                walk_widths_stmts(inner, state, widths);
                for r in redirects {
                    walk_widths_expr(&r.target, state, widths);
                }
            }
            IrStmt::Function { body, .. } => walk_widths_stmts(body, state, widths),
            IrStmt::Case { discriminant, clauses } => {
                walk_widths_expr(discriminant, state, widths);
                for c in clauses {
                    walk_widths_stmts(&c.body, state, widths);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    walk_widths_stmts(st, state, widths);
                }
            }
            _ => {}
        }
    }
}

fn walk_widths_expr(
    e: &IrExpr,
    state: &HashMap<String, Option<(i128, i128)>>,
    widths: &mut HashMap<String, Width>,
) {
    match e {
        IrExpr::Arith(a) => {
            let rng = arith_range_local(a, state);
            let mut vs = Vec::new();
            arith_vars(a, &mut vs);
            for v in vs {
                match rng {
                    Some((lo, hi)) => {
                        let w = Width::from_range_name(crate::shir::range_width_name(lo, hi));
                        let cur = widths.get(&v).copied().unwrap_or(Width::I64);
                        widths.insert(v, cur.widen(w));
                    }
                    None => {
                        // no proof the expr stays in width → no narrowing
                        widths.insert(v, Width::I64);
                    }
                }
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            walk_widths_expr(lhs, state, widths);
            walk_widths_expr(rhs, state, widths);
        }
        IrExpr::Index { key, .. } => walk_widths_expr(key, state, widths),
        IrExpr::Call { args, .. } => {
            for a in args {
                walk_widths_expr(a, state, widths);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            walk_widths_expr(obj, state, widths);
            for a in args {
                walk_widths_expr(a, state, widths);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            walk_widths_expr(cond, state, widths);
            walk_widths_expr(then, state, widths);
            walk_widths_expr(else_, state, widths);
        }
        IrExpr::DefinedOr { expr, default } => {
            walk_widths_expr(expr, state, widths);
            walk_widths_expr(default, state, widths);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    walk_widths_expr(x, state, widths);
                }
            }
        }
        IrExpr::Capture { expr, .. } => walk_widths_expr(expr, state, widths),
        IrExpr::Array(items) => {
            for i in items {
                walk_widths_expr(i, state, widths);
            }
        }
        IrExpr::Arrow(body) => walk_widths_stmts(body, state, widths),
        IrExpr::Object(props) => {
            for (_, v) in props {
                walk_widths_expr(v, state, widths);
            }
        }
        _ => {}
    }
}

/// Local copy of the core's (private) `arith_range`: the conservative
/// [lo, hi] of an ArithAst over the per-var ranges. The renderer needs
/// it to prove an arith expr's RESULT fits its operands' widths before
/// narrowing; the core keeps it private (single-owner), so the copy
/// lives renderer-side. Mirrors shir.rs arith_range exactly.
fn arith_range_local(
    a: &ArithAst,
    state: &HashMap<String, Option<(i128, i128)>>,
) -> Option<(i128, i128)> {
    match a {
        ArithAst::Num(i) => Some((*i as i128, *i as i128)),
        ArithAst::Var(n) => state.get(n).copied().flatten(),
        ArithAst::Bin { op, lhs, rhs } => {
            let (l, r) = (arith_range_local(lhs, state)?, arith_range_local(rhs, state)?);
            let (l0, l1, r0, r1) = (l.0, l.1, r.0, r.1);
            match op.as_str() {
                "+" => Some((l0.checked_add(r0)?, l1.checked_add(r1)?)),
                "-" => Some((l0.checked_sub(r1)?, l1.checked_sub(r0)?)),
                "*" => {
                    let ps = [
                        l0.checked_mul(r0)?,
                        l0.checked_mul(r1)?,
                        l1.checked_mul(r0)?,
                        l1.checked_mul(r1)?,
                    ];
                    Some((*ps.iter().min()?, *ps.iter().max()?))
                }
                "/" => {
                    if r0 <= 0 && r1 >= 0 {
                        return None; // possible division by zero
                    }
                    let qs = [
                        l0.checked_div(r0)?,
                        l0.checked_div(r1)?,
                        l1.checked_div(r0)?,
                        l1.checked_div(r1)?,
                    ];
                    Some((*qs.iter().min()?, *qs.iter().max()?))
                }
                _ => None, // %, ^, comparisons, ... conservative
            }
        }
        ArithAst::Un { op, arg } => {
            let (lo, hi) = arith_range_local(arg, state)?;
            match op.as_str() {
                "-" => Some((-hi, -lo)),
                "+" => Some((lo, hi)),
                _ => None,
            }
        }
        _ => None, // Index / Cond / Assign / IncDec
    }
}

/// Every variable name an ArithAst mentions (reads; a bare `var =` write
/// target is excluded — its RHS vars are included).
fn arith_vars(a: &ArithAst, out: &mut Vec<String>) {
    match a {
        ArithAst::Var(n) => out.push(n.clone()),
        ArithAst::Index { var, key } => {
            out.push(var.clone());
            arith_vars(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_vars(lhs, out);
            arith_vars(rhs, out);
        }
        ArithAst::Un { arg, .. } => arith_vars(arg, out),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_vars(test, out);
            arith_vars(then, out);
            arith_vars(else_, out);
        }
        ArithAst::Assign { rhs, .. } => arith_vars(rhs, out),
        ArithAst::IncDec { var, .. } => out.push(var.clone()),
        ArithAst::Num(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A renderer with `name` declared as an Int var at width `w` (both
    /// the range the width analysis derives from, and the width itself —
    /// the shir_to_c flow's invariant).
    fn int_render(name: &str, lo: i64, hi: i64, w: Width) -> Render {
        let mut r = Render::default();
        r.var_types.insert(name.to_string(), IrType::Int);
        r.var_ranges.insert(name.to_string(), (lo, hi));
        r.var_widths.insert(name.to_string(), w);
        r
    }

    #[test]
    fn width_format_matches_c_type() {
        // the format must read exactly what the width's C type declares
        assert_eq!(Width::U32.format(), "%u");
        assert_eq!(Width::I32.format(), "%d");
        assert_eq!(Width::I64.format(), "%lld");
        assert_eq!(Width::U32.c_type(), "unsigned int");
        assert_eq!(Width::I32.c_type(), "int");
        assert_eq!(Width::I64.c_type(), "long long");
    }

    #[test]
    fn typed_var_reads_drop_the_cast() {
        // `echo $i` where i is proven u32 → `printf("%u\n", i)` — no cast
        // (the format matches the declared `unsigned int` exactly)
        for (name, lo, hi, w, fmt) in [
            ("i", 1, 10000, Width::U32, "%u"),
            ("x", -100, -100, Width::I32, "%d"),
            ("n", 1_000_000_000_000, 1_000_000_000_000, Width::I64, "%lld"),
        ] {
            let r = int_render(name, lo, hi, w);
            let e = IrExpr::Var(name.to_string(), None);
            assert_eq!(r.num_spec(&e), NumSpec::Num(fmt, false), "{name}");
        }
    }

    #[test]
    fn getvar_of_typed_var_matches_var() {
        // `$y` reads arrive as getVar("y"); the read renders as the
        // declared ident, so it gets the same cast-free spec
        let r = int_render("i", 1, 10000, Width::U32);
        let e = IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![IrExpr::Str("i".to_string(), crate::ir::StrStyle::DoubleQuoted)],
        };
        assert_eq!(r.num_spec(&e), NumSpec::Num("%u", false));
    }

    #[test]
    fn arith_over_same_width_leaves_drops_the_cast() {
        // `$((i * i))` — every var leaf at u32 → `snprintf(..., "%u",
        // (i * i))` — the usual arithmetic conversions keep `unsigned int`
        let r = int_render("i", 1, 10000, Width::U32);
        let e = IrExpr::Arith(Box::new(ArithAst::Bin {
            op: "*".to_string(),
            lhs: Box::new(ArithAst::Var("i".to_string())),
            rhs: Box::new(ArithAst::Var("i".to_string())),
        }));
        assert_eq!(r.num_spec(&e), NumSpec::Num("%u", false));
    }

    #[test]
    fn mixed_width_arith_keeps_the_cast() {
        // a long long leaf in the tree → the result type is long long,
        // not the u32 the range might suggest → %lld + cast (the safe pair)
        let mut r = int_render("i", 1, 10000, Width::U32);
        r.var_types.insert("n".to_string(), IrType::Int);
        r.var_ranges.insert("n".to_string(), (1_000_000_000_000, 1_000_000_000_000));
        r.var_widths.insert("n".to_string(), Width::I64);
        let e = IrExpr::Arith(Box::new(ArithAst::Bin {
            op: "*".to_string(),
            lhs: Box::new(ArithAst::Var("i".to_string())),
            rhs: Box::new(ArithAst::Var("n".to_string())),
        }));
        assert_eq!(r.num_spec(&e), NumSpec::Num("%lld", true));
    }

    #[test]
    fn string_var_and_literal_keep_the_cast() {
        // a Str var's width DEFAULTS to I64 but its C type is `char*` —
        // never type-matched; an int literal renders as `int` — never
        // long long. Both keep the %lld + (long long) pair.
        let mut r = Render::default();
        r.var_types.insert("s".to_string(), IrType::Str);
        assert_eq!(
            r.num_spec(&IrExpr::Var("s".to_string(), None)),
            NumSpec::Num("%lld", true)
        );
        assert_eq!(
            r.num_spec(&IrExpr::Int(42)),
            NumSpec::Num("%lld", true)
        );
    }
}
