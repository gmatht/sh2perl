//! Variable lifetime analysis — per-variable live spans + the escape set.
//!
//! The sibling of `shir::analyze_string_lengths` (how BIG a buffer must
//! be) and `shir::analyze_var_types` (what a value IS): this analysis
//! answers WHERE a value's storage may live and HOW LONG it must
//! survive. The C backend's fixed-buffer transform (`char v[N+1]`) is
//! only sound when the buffer outlives every read of `v`; the escape
//! set decides whether a value may be retained beyond the scope where it
//! was produced (and therefore needs heap/copy semantics rather than a
//! reused stack local).
//!
//! # What it computes
//!
//! For every variable the program touches, in a pre-order walk of the
//! statement tree (each statement gets an increasing position):
//!
//! - **live span** `(first, last)`: the positions of the first and last
//!   access (def or use). A span is the C generator's per-point buffer
//!   sizing input: a bounded var whose whole span fits inside one loop
//!   iteration can reuse a buffer across iterations; a var whose span
//!   ends before a later def can be freed/moved at `last`.
//! - **escape bit**: set when the value's storage may be retained beyond
//!   the scope where it was produced. Conservative (over-approximation
//!   is the safe direction — a missed escape aliases two buffers, a
//!   wrong escape only wastes a copy).
//!
//! # Escape rules (conservative)
//!
//! 1. Array-element stores (`arr[i]=v`, `DeclareArray`, `setArray`,
//!    `ArithAst::Index` writes): the array is heap storage, so the
//!    target array escapes, and the RHS value escapes (the store may
//!    retain it).
//! 2. Closure captures: any variable accessed inside an `Arrow` body
//!    escapes (the closure may outlive the current scope — `fnCall`,
//!    `forLoop`/`whileLoop` callbacks, `define`).
//! 3. Function returns: a variable flowing into `Return`/`Exit`... (only
//!    `Return` — the value is handed to the caller, which may retain
//!    it; `Exit` copies the code into the process status).
//! 4. Subprocess boundaries (`exec`, `pipeline`, `capture`,
//!    `captureWords`, `redirect`, `background`, `subshell`) are NOT
//!    escapes: the kernel copies argv/pipe bytes at the boundary, so
//!    the buffer only needs to be alive AT the call (a use). This
//!    matches bash value semantics and keeps the C path zero-copy.
//! 5. `WriteFile`/`Output` are not escapes (bytes are copied).
//! 6. Reads inside `Subshell`/`Background` are uses (the fork's child
//!    observes the value at fork time); writes inside them do not
//!    propagate back, but recording them anyway only over-approximates
//!    the span (safe direction).
//!
//! # Determinism
//!
//! The verdicts are sorted by variable name (like `var_types` /
//! `var_lengths` / `var_const`), so `--shir` JSON is byte-identical for
//! the same input.

use std::collections::{HashMap, HashSet};

use crate::ir::{ArithAst, IrExpr, IrProgram, IrStmt, VarLifetime};
use crate::shir_passes::PassContext;

/// The pipeline analysis: populates `ctx.var_live_ranges` (per-variable
/// `(first, last)` access spans in statement positions) and
/// `ctx.var_escapes` (variables whose storage may be retained beyond the
/// current scope) from [`analyze_var_lifetimes`].
pub struct VarLifetimes;

impl super::Analysis for VarLifetimes {
    fn name(&self) -> &'static str {
        "var_lifetimes"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        for (n, l) in analyze_var_lifetimes(prog) {
            ctx.var_live_ranges.insert(n.clone(), (l.first, l.last));
            if l.escapes {
                ctx.var_escapes.insert(n);
            }
        }
    }
}

/// A single verdict per variable: the live span and the escape bit.
/// Missing from the list = never accessed (dead).
pub fn analyze_var_lifetimes(prog: &IrProgram) -> Vec<(String, VarLifetime)> {
    let mut first: HashMap<String, usize> = HashMap::new();
    let mut last: HashMap<String, usize> = HashMap::new();
    let mut escapes: HashSet<String> = HashSet::new();
    let mut pos = 0usize;

    // Top-level statements. The subs (Perl subroutines) are a separate
    // scope; walk them too so their vars get verdicts (conservative:
    // the same name in a sub and at top level shares a verdict).
    walk_stmts(
        &prog.stmts,
        &mut pos,
        &mut first,
        &mut last,
        &mut escapes,
        false,
        false,
    );
    for sub in &prog.subs {
        walk_stmts(
            &sub.body,
            &mut pos,
            &mut first,
            &mut last,
            &mut escapes,
            false,
            false,
        );
    }

    let mut names: Vec<String> = first.keys().cloned().collect();
    names.sort();
    names
        .into_iter()
        .map(|n| {
            (
                n.clone(),
                VarLifetime {
                    first: first[&n],
                    last: last[&n],
                    escapes: escapes.contains(&n),
                },
            )
        })
        .collect()
}

/// Record one access (def or use) of a variable at the current position.
fn access(
    name: &str,
    pos: usize,
    first: &mut HashMap<String, usize>,
    last: &mut HashMap<String, usize>,
    escapes: &mut HashSet<String>,
    in_closure: bool,
) {
    first.entry(name.to_string()).or_insert(pos);
    last.insert(name.to_string(), pos);
    if in_closure {
        // a closure may outlive the scope where the var's buffer lives
        escapes.insert(name.to_string());
    }
}

/// Walk a statement list. `pos` advances per statement (pre-order, so a
/// body's statements get positions after their enclosing statement).
/// `in_closure` marks Arrow bodies (every access inside escapes).
/// `copied` marks Subshell/Background bodies (bash forks: writes don't
/// propagate back — accesses still count, conservatively).
fn walk_stmts(
    stmts: &[IrStmt],
    pos: &mut usize,
    first: &mut HashMap<String, usize>,
    last: &mut HashMap<String, usize>,
    escapes: &mut HashSet<String>,
    in_closure: bool,
    copied: bool,
) {
    for st in stmts {
        *pos += 1;
        walk_stmt(st, pos, first, last, escapes, in_closure, copied);
    }
}

fn walk_stmt(
    st: &IrStmt,
    pos: &mut usize,
    first: &mut HashMap<String, usize>,
    last: &mut HashMap<String, usize>,
    escapes: &mut HashSet<String>,
    in_closure: bool,
    copied: bool,
) {
    let p = *pos;
    match st {
        IrStmt::Label(_) | IrStmt::Goto(_) => {}
        IrStmt::Assign { targets, expr, asm, .. } => {
            for t in targets {
                if t.indices.is_empty() {
                    // plain scalar def
                    access(&t.var, p, first, last, escapes, in_closure);
                } else {
                    // array-element write: the ARRAY's storage is heap,
                    // so it escapes; the RHS value stored into it may be
                    // retained → also escapes.
                    access(&t.var, p, first, last, escapes, in_closure);
                    escapes.insert(t.var.clone());
                    for k in &t.indices {
                        walk_expr(k, p, first, last, escapes, in_closure);
                    }
                    mark_vars_escape(expr, first, escapes);
                }
            }
            walk_expr(expr, p, first, last, escapes, in_closure);
            // declarator-position asm label: OUTPUT operand targets are
            // store writes; input operand exprs are plain reads (same
            // contract as the Asm statement).
            if let Some(spec) = asm {
                for (_, t) in &spec.outputs {
                    if let IrExpr::Var(name, _) = t {
                        access(name, p, first, last, escapes, in_closure);
                    }
                    walk_expr(t, p, first, last, escapes, in_closure);
                }
                for (_, e) in &spec.inputs {
                    walk_expr(e, p, first, last, escapes, in_closure);
                }
            }
        }
        IrStmt::Declare { vars, init, .. } => {
            for d in vars {
                access(&d.name, p, first, last, escapes, in_closure);
            }
            if let Some(e) = init {
                walk_expr(e, p, first, last, escapes, in_closure);
            }
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            // a declared array is heap storage by construction
            access(var, p, first, last, escapes, in_closure);
            escapes.insert(var.clone());
            for e in elements {
                walk_expr(e, p, first, last, escapes, in_closure);
            }
            // the element values are stored into the array — the store
            // may retain them
            for e in elements {
                mark_vars_escape(e, first, escapes);
            }
        }
        IrStmt::Output { value, .. } => walk_expr(value, p, first, last, escapes, in_closure),
        IrStmt::WriteFile { path, content, .. } => {
            walk_expr(path, p, first, last, escapes, in_closure);
            walk_expr(content, p, first, last, escapes, in_closure);
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            walk_expr(cond, p, first, last, escapes, in_closure);
            walk_stmts(then, pos, first, last, escapes, in_closure, copied);
            for (c, b) in elsifs {
                walk_expr(c, p, first, last, escapes, in_closure);
                walk_stmts(b, pos, first, last, escapes, in_closure, copied);
            }
            walk_stmts(else_, pos, first, last, escapes, in_closure, copied);
        }
        IrStmt::For { var, iter, body } => {
            // the loop var is defined at the loop head, then per iteration
            access(var, p, first, last, escapes, in_closure);
            walk_expr(iter, p, first, last, escapes, in_closure);
            walk_stmts(body, pos, first, last, escapes, in_closure, copied);
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            walk_expr(cond, p, first, last, escapes, in_closure);
            walk_stmts(body, pos, first, last, escapes, in_closure, copied);
        }
        IrStmt::ForInit { init, cond, step, body } => {
            walk_stmts(init, pos, first, last, escapes, in_closure, copied);
            walk_expr(cond, p, first, last, escapes, in_closure);
            walk_stmts(step, pos, first, last, escapes, in_closure, copied);
            walk_stmts(body, pos, first, last, escapes, in_closure, copied);
        }
        IrStmt::Continue | IrStmt::Break => {}
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
            walk_expr(expr, p, first, last, escapes, in_closure);
        }
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            // subprocess boundary: values are uses (kernel copies); the
            // buffers must be alive at the call, nothing is retained.
            walk_expr(cmd, p, first, last, escapes, in_closure);
            for a in args {
                walk_expr(a, p, first, last, escapes, in_closure);
            }
            for r in redirects {
                walk_expr(r, p, first, last, escapes, in_closure);
            }
            for (_, v) in env {
                walk_expr(v, p, first, last, escapes, in_closure);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages {
                walk_stmts(s, pos, first, last, escapes, in_closure, copied);
            }
        }
        IrStmt::Return(e) => {
            // the value is handed to the caller, which may retain it
            if let Some(e) = e {
                walk_expr(e, p, first, last, escapes, in_closure);
                mark_vars_escape(e, first, escapes);
            }
        }
        IrStmt::Exit(e) => {
            if let Some(e) = e {
                walk_expr(e, p, first, last, escapes, in_closure);
            }
        }
        IrStmt::SetChildError(e) => walk_expr(e, p, first, last, escapes, in_closure),
        IrStmt::Require(_) | IrStmt::RawText(_) => {}
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            walk_expr(discriminant, p, first, last, escapes, in_closure);
            for c in clauses {
                walk_stmts(&c.body, pos, first, last, escapes, in_closure, copied);
            }
        }
        IrStmt::Redirect { inner, redirects } => {
            walk_stmts(inner, pos, first, last, escapes, in_closure, copied);
            for r in redirects {
                walk_expr(&r.target, p, first, last, escapes, in_closure);
            }
        }
        IrStmt::Function { name, body, .. } => {
            // the function name is defined (callable)
            access(name, p, first, last, escapes, in_closure);
            // body: a function may run 0..N times; accesses count
            // (conservative over-approx of the span is safe)
            walk_stmts(body, pos, first, last, escapes, in_closure, copied);
        }
        IrStmt::Subshell(body) | IrStmt::Background(body) => {
            // bash forks: the child observes the parent's values at fork
            // time (uses); writes don't propagate back. Record accesses
            // (over-approx span is the safe direction).
            walk_stmts(body, pos, first, last, escapes, in_closure, true);
        }
        IrStmt::Block(body) => walk_stmts(body, pos, first, last, escapes, in_closure, copied),
        // Select comm clauses: bodies may run when a clause is ready
        // (over-approx like Block); channel/value exprs are plain reads.
        IrStmt::Select { clauses } => {
            for c in clauses {
                walk_stmts(&c.body, pos, first, last, escapes, in_closure, copied);
                if let Some(ch) = &c.ch {
                    walk_expr(ch, *pos, first, last, escapes, in_closure);
                }
                if let Some(v) = &c.value {
                    walk_expr(v, *pos, first, last, escapes, in_closure);
                }
            }
        }
        // Inline asm (core requests c-sh-go-asm family): OUTPUT operand
        // targets are store writes (access them like assignment targets);
        // input operand exprs are plain reads.
        IrStmt::Asm { outputs, inputs, .. } => {
            for (_, t) in outputs {
                if let IrExpr::Var(name, _) = t {
                    access(name, p, first, last, escapes, in_closure);
                }
                walk_expr(t, p, first, last, escapes, in_closure);
            }
            for (_, e) in inputs {
                walk_expr(e, p, first, last, escapes, in_closure);
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            walk_stmts(body, pos, first, last, escapes, in_closure, copied);
            for e in excepts {
                if let Some(m) = &e.match_expr {
                    walk_expr(m, p, first, last, escapes, in_closure);
                }
                walk_stmts(&e.body, pos, first, last, escapes, in_closure, copied);
            }
            walk_stmts(else_body, pos, first, last, escapes, in_closure, copied);
            walk_stmts(finally_body, pos, first, last, escapes, in_closure, copied);
        }
        IrStmt::Expr(e) => walk_expr(e, p, first, last, escapes, in_closure),
    }
}

fn walk_expr(
    e: &IrExpr,
    pos: usize,
    first: &mut HashMap<String, usize>,
    last: &mut HashMap<String, usize>,
    escapes: &mut HashSet<String>,
    in_closure: bool,
) {
    match e {
        IrExpr::Var(name, _) | IrExpr::Ident(name) => {
            access(name, pos, first, last, escapes, in_closure);
        }
        IrExpr::Index { var, key } => {
            // reading an array element: the array is read (its storage
            // already escapes by construction)
            access(var, pos, first, last, escapes, in_closure);
            walk_expr(key, pos, first, last, escapes, in_closure);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            walk_expr(lhs, pos, first, last, escapes, in_closure);
            walk_expr(rhs, pos, first, last, escapes, in_closure);
        }
        IrExpr::Call { func, args } => {
            match func.as_str() {
                // getVar("x") — a $x read
                "getVar" => {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        access(name, pos, first, last, escapes, in_closure);
                        return;
                    }
                }
                // setVar("x", v) — a $x write
                "setVar" => {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        access(name, pos, first, last, escapes, in_closure);
                    }
                    if let Some(v) = args.get(1) {
                        walk_expr(v, pos, first, last, escapes, in_closure);
                    }
                    return;
                }
                // setArray("a", [...]) — array write; the array is heap
                "setArray" | "setArrayAppend" => {
                    if let Some(IrExpr::Str(name, _)) = args.first() {
                        access(name, pos, first, last, escapes, in_closure);
                        escapes.insert(name.clone());
                    }
                    for a in args.iter().skip(1) {
                        walk_expr(a, pos, first, last, escapes, in_closure);
                    }
                    return;
                }
                // define(name, Arrow) — the arrow is a closure: vars
                // accessed inside it escape
                "define" | "fnCall" => {
                    for a in args {
                        walk_expr(a, pos, first, last, escapes, in_closure);
                    }
                    if func == "define" {
                        if let Some(IrExpr::Arrow(body)) = args.get(1) {
                            let mut p = pos;
                            walk_stmts(body, &mut p, first, last, escapes, true, false);
                        }
                    }
                    return;
                }
                // subprocess boundaries: uses only (kernel copies)
                "exec"
                | "pipeline"
                | "capture"
                | "captureWords"
                | "redirect"
                | "subshell"
                | "background"
                | "forLoop"
                | "whileLoop"
                | "whileLoopSync"
                | "cstyleFor"
                | "cstyleForSync"
                | "forIn"
                | "forOf"
                | "commandSubstitution" => {
                    for a in args {
                        walk_expr(a, pos, first, last, escapes, in_closure);
                    }
                    return;
                }
                // everything else (param/test/contains/arith/brace/...):
                // walk args as ordinary expressions
                _ => {}
            }
            for a in args {
                walk_expr(a, pos, first, last, escapes, in_closure);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            walk_expr(obj, pos, first, last, escapes, in_closure);
            for a in args {
                walk_expr(a, pos, first, last, escapes, in_closure);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            walk_expr(cond, pos, first, last, escapes, in_closure);
            walk_expr(then, pos, first, last, escapes, in_closure);
            walk_expr(else_, pos, first, last, escapes, in_closure);
        }
        IrExpr::DefinedOr { expr, default } => {
            walk_expr(expr, pos, first, last, escapes, in_closure);
            walk_expr(default, pos, first, last, escapes, in_closure);
        }
        IrExpr::Interpolate(parts) => {
            for part in parts {
                if let crate::ir::InterpPart::Expr(x) = part {
                    walk_expr(x, pos, first, last, escapes, in_closure);
                }
            }
        }
        IrExpr::Capture { expr, .. } => {
            // the captured command runs in a child process; its arg vars
            // are uses (alive at the call), not escapes
            walk_expr(expr, pos, first, last, escapes, in_closure);
        }
        IrExpr::Arrow(body) => {
            // a closure: every access inside escapes
            let mut p = pos;
            walk_stmts(body, &mut p, first, last, escapes, true, false);
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            walk_expr(iter, pos, first, last, escapes, in_closure);
            walk_expr(elem, pos, first, last, escapes, in_closure);
            if let Some(c) = cond {
                walk_expr(c, pos, first, last, escapes, in_closure);
            }
        }
        IrExpr::Splice(e) => walk_expr(e, pos, first, last, escapes, in_closure),
        IrExpr::Lambda { body, .. } => {
            // a closure: every access inside escapes (like Arrow)
            let mut p = pos;
            walk_stmts(body, &mut p, first, last, escapes, true, false);
        }
        IrExpr::Array(items) => {
            for i in items {
                walk_expr(i, pos, first, last, escapes, in_closure);
            }
        }
        IrExpr::Arith(a) => walk_arith(a, pos, first, last, escapes, in_closure),
        IrExpr::Object(props) => {
            for (_, v) in props {
                walk_expr(v, pos, first, last, escapes, in_closure);
            }
        }
        IrExpr::Str(_, _)
        | IrExpr::Int(_)
        | IrExpr::Bool(_)
        | IrExpr::Json(_)
        | IrExpr::Regex { .. }
        | IrExpr::Range { .. }
        | IrExpr::RawExpr(_) => {}
    }
}

fn walk_arith(
    a: &ArithAst,
    pos: usize,
    first: &mut HashMap<String, usize>,
    last: &mut HashMap<String, usize>,
    escapes: &mut HashSet<String>,
    in_closure: bool,
) {
    match a {
        ArithAst::Var(name) | ArithAst::Ident(name) => access(name, pos, first, last, escapes, in_closure),
        ArithAst::Index { var, key } => {
            access(var, pos, first, last, escapes, in_closure);
            walk_arith(key, pos, first, last, escapes, in_closure);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            walk_arith(lhs, pos, first, last, escapes, in_closure);
            walk_arith(rhs, pos, first, last, escapes, in_closure);
        }
        ArithAst::Un { arg, .. } => walk_arith(arg, pos, first, last, escapes, in_closure),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            walk_arith(test, pos, first, last, escapes, in_closure);
            walk_arith(then, pos, first, last, escapes, in_closure);
            walk_arith(else_, pos, first, last, escapes, in_closure);
        }
        ArithAst::Assign { var, rhs, .. } => {
            // x=... in $(( )) — a def (read-modify-write)
            access(var, pos, first, last, escapes, in_closure);
            walk_arith(rhs, pos, first, last, escapes, in_closure);
        }
        ArithAst::IncDec { var, .. } => {
            // x++ / x-- — reads and writes
            access(var, pos, first, last, escapes, in_closure);
        }
        ArithAst::Num(_) => {}
        ArithAst::Sizeof(_) => {}
        ArithAst::Cast { arg, .. } => {
            walk_arith(arg, pos, first, last, escapes, in_closure);
        }
    }
}

/// Mark every variable appearing in an expression as escaping (used for
/// array-element stores and function returns: the storage may be
/// retained past the current scope).
fn mark_vars_escape(e: &IrExpr, first: &mut HashMap<String, usize>, escapes: &mut HashSet<String>) {
    match e {
        IrExpr::Var(name, _) | IrExpr::Ident(name) => {
            first.entry(name.to_string()).or_insert(0);
            escapes.insert(name.to_string());
        }
        IrExpr::Index { var, key } => {
            first.entry(var.clone()).or_insert(0);
            escapes.insert(var.clone());
            mark_vars_escape(key, first, escapes);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            mark_vars_escape(lhs, first, escapes);
            mark_vars_escape(rhs, first, escapes);
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                mark_vars_escape(a, first, escapes);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            mark_vars_escape(obj, first, escapes);
            for a in args {
                mark_vars_escape(a, first, escapes);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => {
            mark_vars_escape(cond, first, escapes);
            mark_vars_escape(then, first, escapes);
            mark_vars_escape(else_, first, escapes);
        }
        IrExpr::DefinedOr { expr, default } => {
            mark_vars_escape(expr, first, escapes);
            mark_vars_escape(default, first, escapes);
        }
        IrExpr::Interpolate(parts) => {
            for part in parts {
                if let crate::ir::InterpPart::Expr(x) = part {
                    mark_vars_escape(x, first, escapes);
                }
            }
        }
        IrExpr::Capture { expr, .. } => mark_vars_escape(expr, first, escapes),
        IrExpr::Arrow(body) => {
            // a closure stores its whole environment — everything it
            // accesses is retained
            for st in body {
                mark_stmt_vars_escape(st, first, escapes);
            }
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            mark_vars_escape(iter, first, escapes);
            mark_vars_escape(elem, first, escapes);
            if let Some(c) = cond {
                mark_vars_escape(c, first, escapes);
            }
        }
        IrExpr::Lambda { body, .. } => {
            for st in body {
                mark_stmt_vars_escape(st, first, escapes);
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                mark_vars_escape(i, first, escapes);
            }
        }
        IrExpr::Arith(a) => mark_arith_vars_escape(a, first, escapes),
        IrExpr::Object(props) => {
            for (_, v) in props {
                mark_vars_escape(v, first, escapes);
            }
        }
        _ => {}
    }
}

/// The statement-level twin of [`mark_vars_escape`] (closure bodies).
fn mark_stmt_vars_escape(
    st: &IrStmt,
    first: &mut HashMap<String, usize>,
    escapes: &mut HashSet<String>,
) {
    match st {
        IrStmt::Label(_) | IrStmt::Goto(_) => {}
        IrStmt::Assign { targets, expr, asm, .. } => {
            for t in targets {
                first.entry(t.var.clone()).or_insert(0);
                escapes.insert(t.var.clone());
            }
            mark_vars_escape(expr, first, escapes);
            // declarator-position asm label: output target vars escape
            // (the asm writes them); operand exprs may carry retained
            // values (same contract as the Asm statement).
            if let Some(spec) = asm {
                for (_, t) in &spec.outputs {
                    if let IrExpr::Var(name, _) = t {
                        first.entry(name.clone()).or_insert(0);
                        escapes.insert(name.clone());
                    }
                    mark_vars_escape(t, first, escapes);
                }
                for (_, e) in &spec.inputs {
                    mark_vars_escape(e, first, escapes);
                }
            }
        }
        IrStmt::Declare { vars, init, .. } => {
            for d in vars {
                first.entry(d.name.clone()).or_insert(0);
                escapes.insert(d.name.clone());
            }
            if let Some(e) = init {
                mark_vars_escape(e, first, escapes);
            }
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            first.entry(var.clone()).or_insert(0);
            escapes.insert(var.clone());
            for e in elements {
                mark_vars_escape(e, first, escapes);
            }
        }
        IrStmt::Output { value, .. } => mark_vars_escape(value, first, escapes),
        IrStmt::WriteFile { path, content, .. } => {
            mark_vars_escape(path, first, escapes);
            mark_vars_escape(content, first, escapes);
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            mark_vars_escape(cond, first, escapes);
            mark_stmts_vars_escape(then, first, escapes);
            for (c, b) in elsifs {
                mark_vars_escape(c, first, escapes);
                mark_stmts_vars_escape(b, first, escapes);
            }
            mark_stmts_vars_escape(else_, first, escapes);
        }
        IrStmt::For { var, iter, body } => {
            first.entry(var.clone()).or_insert(0);
            escapes.insert(var.clone());
            mark_vars_escape(iter, first, escapes);
            mark_stmts_vars_escape(body, first, escapes);
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            mark_vars_escape(cond, first, escapes);
            mark_stmts_vars_escape(body, first, escapes);
        }
        IrStmt::ForInit { init, cond, step, body } => {
            mark_stmts_vars_escape(init, first, escapes);
            mark_vars_escape(cond, first, escapes);
            mark_stmts_vars_escape(step, first, escapes);
            mark_stmts_vars_escape(body, first, escapes);
        }
        IrStmt::Continue | IrStmt::Break => {}
        IrStmt::Die { expr, .. }
        | IrStmt::Warn { expr, .. }
        | IrStmt::Exit(Some(expr))
        | IrStmt::SetChildError(expr)
        | IrStmt::Return(Some(expr)) => {
            mark_vars_escape(expr, first, escapes);
        }
        IrStmt::Exit(None) | IrStmt::Return(None) => {}
        IrStmt::Exec {
            cmd,
            args,
            redirects,
            env,
            ..
        } => {
            mark_vars_escape(cmd, first, escapes);
            for a in args {
                mark_vars_escape(a, first, escapes);
            }
            for r in redirects {
                mark_vars_escape(r, first, escapes);
            }
            for (_, v) in env {
                mark_vars_escape(v, first, escapes);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages {
                mark_stmts_vars_escape(s, first, escapes);
            }
        }
        IrStmt::Require(_) | IrStmt::RawText(_) => {}
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            mark_vars_escape(discriminant, first, escapes);
            for c in clauses {
                mark_stmts_vars_escape(&c.body, first, escapes);
            }
        }
        IrStmt::Redirect { inner, redirects } => {
            mark_stmts_vars_escape(inner, first, escapes);
            for r in redirects {
                mark_vars_escape(&r.target, first, escapes);
            }
        }
        IrStmt::Function { name, body, .. } => {
            first.entry(name.clone()).or_insert(0);
            escapes.insert(name.clone());
            mark_stmts_vars_escape(body, first, escapes);
        }
        IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
            mark_stmts_vars_escape(body, first, escapes);
        }
        IrStmt::Select { clauses } => {
            for c in clauses {
                mark_stmts_vars_escape(&c.body, first, escapes);
                if let Some(ch) = &c.ch {
                    mark_vars_escape(ch, first, escapes);
                }
                if let Some(v) = &c.value {
                    mark_vars_escape(v, first, escapes);
                }
            }
        }
        // Inline asm: output target vars escape (the asm writes them);
        // operand exprs may carry retained values.
        IrStmt::Asm { outputs, inputs, .. } => {
            for (_, t) in outputs {
                if let IrExpr::Var(name, _) = t {
                    first.entry(name.clone()).or_insert(0);
                    escapes.insert(name.clone());
                }
                mark_vars_escape(t, first, escapes);
            }
            for (_, e) in inputs {
                mark_vars_escape(e, first, escapes);
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            mark_stmts_vars_escape(body, first, escapes);
            for e in excepts {
                if let Some(m) = &e.match_expr {
                    mark_vars_escape(m, first, escapes);
                }
                mark_stmts_vars_escape(&e.body, first, escapes);
            }
            mark_stmts_vars_escape(else_body, first, escapes);
            mark_stmts_vars_escape(finally_body, first, escapes);
        }
        IrStmt::Expr(e) => mark_vars_escape(e, first, escapes),
    }
}

fn mark_stmts_vars_escape(
    stmts: &[IrStmt],
    first: &mut HashMap<String, usize>,
    escapes: &mut HashSet<String>,
) {
    for st in stmts {
        mark_stmt_vars_escape(st, first, escapes);
    }
}

fn mark_arith_vars_escape(
    a: &ArithAst,
    first: &mut HashMap<String, usize>,
    escapes: &mut HashSet<String>,
) {
    match a {
        ArithAst::Var(name) | ArithAst::Ident(name) => {
            first.entry(name.clone()).or_insert(0);
            escapes.insert(name.clone());
        }
        ArithAst::Index { var, key } => {
            first.entry(var.clone()).or_insert(0);
            escapes.insert(var.clone());
            mark_arith_vars_escape(key, first, escapes);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            mark_arith_vars_escape(lhs, first, escapes);
            mark_arith_vars_escape(rhs, first, escapes);
        }
        ArithAst::Un { arg, .. } => mark_arith_vars_escape(arg, first, escapes),
        ArithAst::Cond {
            test, then, else_, ..
        } => {
            mark_arith_vars_escape(test, first, escapes);
            mark_arith_vars_escape(then, first, escapes);
            mark_arith_vars_escape(else_, first, escapes);
        }
        ArithAst::Assign { var, rhs, .. } => {
            first.entry(var.clone()).or_insert(0);
            escapes.insert(var.clone());
            mark_arith_vars_escape(rhs, first, escapes);
        }
        ArithAst::IncDec { var, .. } => {
            first.entry(var.clone()).or_insert(0);
            escapes.insert(var.clone());
        }
        ArithAst::Num(_) => {}
        ArithAst::Sizeof(_) => {}
        ArithAst::Cast { arg, .. } => mark_arith_vars_escape(arg, first, escapes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, Decl, StrStyle};

    fn empty_prog() -> IrProgram {
        IrProgram {
            var_nospace: vec![],
            var_bash_env: vec![],
            imports: vec![],
            requires: vec![],
            stmts: vec![],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        }
    }

    fn assign(var: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: var.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        }
    }

    fn read(var: &str) -> IrExpr {
        IrExpr::Var(var.to_string(), None)
    }

    #[test]
    fn empty_program_no_verdicts() {
        let verdicts = analyze_var_lifetimes(&empty_prog());
        assert!(verdicts.is_empty());
    }

    #[test]
    fn live_span_covers_def_to_use() {
        // x=1; echo $x; echo $x;  →  first=1, last=3
        let prog = IrProgram {
            stmts: vec![
                assign("x", IrExpr::Int(1)),
                IrStmt::Output {
                    value: read("x"),
                    newline: true,
                    target: None,
                },
                IrStmt::Output {
                    value: read("x"),
                    newline: true,
                    target: None,
                },
            ],
            ..empty_prog()
        };
        let verdicts = analyze_var_lifetimes(&prog);
        let (name, l) = verdicts.iter().find(|(n, _)| n == "x").unwrap();
        assert_eq!(name, "x");
        assert_eq!(l.first, 1);
        assert_eq!(l.last, 3);
        assert!(!l.escapes);
    }

    #[test]
    fn later_def_extends_span() {
        // x=1; x=2; echo $x  →  first=1, last=3
        let prog = IrProgram {
            stmts: vec![
                assign("x", IrExpr::Int(1)),
                assign("x", IrExpr::Int(2)),
                IrStmt::Output {
                    value: read("x"),
                    newline: true,
                    target: None,
                },
            ],
            ..empty_prog()
        };
        let verdicts = analyze_var_lifetimes(&prog);
        let l = &verdicts[0].1;
        assert_eq!(l.first, 1);
        assert_eq!(l.last, 3);
    }

    #[test]
    fn array_element_write_escapes() {
        // a[i]=$x  →  a and x both escape (the array may retain x)
        let prog = IrProgram {
            stmts: vec![IrStmt::Assign {
                targets: vec![AssignTarget {
                    var: "a".to_string(),
                    sigil: None,
                    indices: vec![IrExpr::Int(0)],
                }],
                expr: read("x"),
                asm: None,
            }],
            ..empty_prog()
        };
        let verdicts = analyze_var_lifetimes(&prog);
        let by_name: HashMap<&str, &VarLifetime> =
            verdicts.iter().map(|(n, l)| (n.as_str(), l)).collect();
        assert!(by_name["a"].escapes);
        assert!(by_name["x"].escapes);
    }

    #[test]
    fn declare_array_escapes() {
        let prog = IrProgram {
            stmts: vec![IrStmt::DeclareArray {
                var: "a".to_string(),
                sigil: None,
                elements: vec![read("x")],
            }],
            ..empty_prog()
        };
        let verdicts = analyze_var_lifetimes(&prog);
        let by_name: HashMap<&str, &VarLifetime> =
            verdicts.iter().map(|(n, l)| (n.as_str(), l)).collect();
        assert!(by_name["a"].escapes);
        assert!(by_name["x"].escapes);
    }

    #[test]
    fn exec_args_are_uses_not_escapes() {
        // echo $x  →  x is used but does NOT escape (kernel copies argv)
        let prog = IrProgram {
            stmts: vec![IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![
                    IrExpr::Str("echo".to_string(), StrStyle::SingleQuoted),
                    IrExpr::Array(vec![read("x")]),
                ],
            })],
            ..empty_prog()
        };
        let verdicts = analyze_var_lifetimes(&prog);
        let l = &verdicts[0].1;
        assert_eq!(l.first, 1);
        assert_eq!(l.last, 1);
        assert!(!l.escapes);
    }

    #[test]
    fn closure_capture_escapes() {
        // define("f", Arrow([echo $x]))  →  x escapes (closure may outlive)
        let prog = IrProgram {
            stmts: vec![IrStmt::Expr(IrExpr::Call {
                func: "define".to_string(),
                args: vec![
                    IrExpr::Str("f".to_string(), StrStyle::SingleQuoted),
                    IrExpr::Arrow(vec![IrStmt::Output {
                        value: read("x"),
                        newline: true,
                        target: None,
                    }]),
                ],
            })],
            ..empty_prog()
        };
        let verdicts = analyze_var_lifetimes(&prog);
        let l = &verdicts[0].1;
        assert!(l.escapes);
    }

    #[test]
    fn return_escapes() {
        // return $x  →  x escapes (the caller may retain it)
        let prog = IrProgram {
            stmts: vec![IrStmt::Return(Some(read("x")))],
            ..empty_prog()
        };
        let verdicts = analyze_var_lifetimes(&prog);
        let l = &verdicts[0].1;
        assert!(l.escapes);
    }

    #[test]
    fn getvar_setvar_tracked() {
        // setVar("x", 1); getVar("x")  →  first=1, last=2, no escape
        let prog = IrProgram {
            stmts: vec![
                IrStmt::Expr(IrExpr::Call {
                    func: "setVar".to_string(),
                    args: vec![
                        IrExpr::Str("x".to_string(), StrStyle::SingleQuoted),
                        IrExpr::Int(1),
                    ],
                }),
                IrStmt::Expr(IrExpr::Call {
                    func: "getVar".to_string(),
                    args: vec![IrExpr::Str("x".to_string(), StrStyle::SingleQuoted)],
                }),
            ],
            ..empty_prog()
        };
        let verdicts = analyze_var_lifetimes(&prog);
        let l = &verdicts[0].1;
        assert_eq!(l.first, 1);
        assert_eq!(l.last, 2);
        assert!(!l.escapes);
    }

    #[test]
    fn deterministic_sorted_output() {
        // names come back sorted; two runs are identical
        let prog = IrProgram {
            stmts: vec![
                assign("zeta", IrExpr::Int(1)),
                assign("alpha", IrExpr::Int(1)),
            ],
            ..empty_prog()
        };
        let v1 = analyze_var_lifetimes(&prog);
        let v2 = analyze_var_lifetimes(&prog);
        assert_eq!(v1, v2);
        let names: Vec<&str> = v1.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["alpha", "zeta"]);
    }

    #[test]
    fn declare_counts_as_def() {
        // declare -r x="hi"; echo $x
        let prog = IrProgram {
            stmts: vec![
                IrStmt::Declare {
                    vars: vec![Decl {
                        name: "x".to_string(),
                        sigil: None,
                    }],
                    init: Some(IrExpr::Str("hi".to_string(), StrStyle::DoubleQuoted)),
                    local: false,
                },
                IrStmt::Output {
                    value: read("x"),
                    newline: true,
                    target: None,
                },
            ],
            ..empty_prog()
        };
        let verdicts = analyze_var_lifetimes(&prog);
        let l = &verdicts[0].1;
        assert_eq!(l.first, 1);
        assert_eq!(l.last, 2);
    }

    #[test]
    fn arith_assign_is_def() {
        // $((x += 1))  →  x first=1 last=1, no escape
        use crate::ir::ArithAst;
        let prog = IrProgram {
            stmts: vec![IrStmt::Expr(IrExpr::Arith(Box::new(ArithAst::Assign {
                var: "x".to_string(),
                op: "+=".to_string(),
                rhs: Box::new(ArithAst::Num(1)),
            })))],
            ..empty_prog()
        };
        let verdicts = analyze_var_lifetimes(&prog);
        let l = &verdicts[0].1;
        assert_eq!(l.first, 1);
        assert_eq!(l.last, 1);
        assert!(!l.escapes);
    }
}
