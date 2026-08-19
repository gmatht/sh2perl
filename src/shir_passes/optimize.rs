//! The shared A1 optimizer — `const_prop` (const/copy propagation +
//! dead-store elimination for the A1's `Const` verdicts),
//! `const_fold_arith` (const-pool arith folding, THE SIBLING of
//! shir.rs's fold — this module's version is pass-shaped and walks
//! subprograms too), and `dead_store_elim` (never-read stores drop
//! through the shared read/write walkers).
//!
//! Core requests:
//! - estree-20260813-183713-a1-ssa-const-copy-prop — the umbrella:
//!   `shir_passes::const_prop(&mut IrProgram) -> bool`, modeled on the
//!   `analyze_var_const` verdict machinery (Const vars are the only
//!   candidates), with the request's HONESTY rules (dominance/order,
//!   bare-copy only, numeric-context const substitution, dead-store
//!   elimination, never-touch list).
//! - estree-20260813-182434-const-fold-arith — the const-pool arith
//!   fold (the shir.rs sibling already wires the top-level fold at A1
//!   ingress; this module's pass runs the same fold over every arith
//!   node reachable from a statement list, including function bodies).
//! - estree-20260813-182435-dce-dead-vars + go-sh-20260816-164300 — the
//!   read-set walk marks `param(op, name, …)` as a READ of `name`
//!   (EVERY param op; over-marking is the safe direction), so a store
//!   whose only consumer is a param call is never classified dead (the
//!   `ReferenceError: s is not defined` regression).
//!
//! The passes are wired at the A1-ingress points (frontend JSON →
//! render), NOT the bash→IR corpus channel — every renderer consumes the
//! optimized A1 byte-identically (the folds are value-preserving reads;
//! the drops are provably-unobserved stores).

use crate::ir::{ArithAst, InterpPart, IrExpr, IrProgram, IrStmt, StrStyle, VarKind};
use std::collections::{HashMap, HashSet};

// ────────────────────────────────────────────────────────────────────────
// shared expression walkers
// ────────────────────────────────────────────────────────────────────────

/// Does the expression READ the variable `name`? The go-sh-20260816-164300
/// rule: `param(op, name, …)` reads `name` (all ops — over-marking).
pub fn expr_reads(name: &str, e: &IrExpr) -> bool {
    match e {
        IrExpr::Var(n, _) | IrExpr::Ident(n) => n == name,
        IrExpr::Index { var, key } => var == name || expr_reads(name, key),
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Bool(_) | IrExpr::Json(_)
        | IrExpr::RawExpr(_) | IrExpr::Regex { .. } | IrExpr::Range { .. } => false,
        IrExpr::BinOp { lhs, rhs, .. } => expr_reads(name, lhs) || expr_reads(name, rhs),
        IrExpr::Ternary { cond, then, else_ } => {
            expr_reads(name, cond) || expr_reads(name, then) || expr_reads(name, else_)
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_reads(name, expr) || expr_reads(name, default)
        }
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Lit(_) => false,
            InterpPart::Expr(e) => expr_reads(name, e),
        }),
        IrExpr::Capture { expr, .. } => expr_reads(name, expr),
        IrExpr::Arrow(stmts) | IrExpr::Lambda { body: stmts, .. } => stmts_read(name, stmts),
        IrExpr::Array(items) => items.iter().any(|i| expr_reads(name, i)),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            expr_reads(name, iter) || expr_reads(name, elem) || cond.as_ref().is_some_and(|c| expr_reads(name, c))
        }
        IrExpr::Splice(inner) => expr_reads(name, inner),
        IrExpr::Arith(ast) => arith_reads(name, ast),
        IrExpr::Object(pairs) => pairs.iter().any(|(_, v)| expr_reads(name, v)),
        IrExpr::MethodCall { obj, args, .. } => {
            expr_reads(name, obj) || args.iter().any(|a| expr_reads(name, a))
        }
        IrExpr::Call { func, args } => {
            // literal-name ops: getVar/listVar/arrayItems/arrayLen/
            // arrayIndex name ARGS read the NAME (the array/var whose
            // elements are read). param(op, name, …) reads name.
            if func == "param" {
                if let Some(IrExpr::Str(n, _)) = args.get(1) {
                    if n == name {
                        return true;
                    }
                }
            } else if matches!(func.as_str(), "getVar" | "listVar" | "arrayItems" | "arrayLen") {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    if n == name {
                        return true;
                    }
                }
            } else if func == "arrayIndex" {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    if n == name {
                        return true;
                    }
                }
            } else if func == "test" {
                // `[ "$x" -eq 5 ]` string operands carry $names
                for a in args {
                    if let IrExpr::Str(s, _) = a {
                        if test_string_reads(name, s) {
                            return true;
                        }
                    }
                }
            }
            // recurse the remaining arg expressions (the name args are
            // literal Str — no recursion needed, but be thorough)
            args.iter().any(|a| expr_reads(name, a))
        }
    }
}

/// Bare `$name` / `${name}` tokens inside a `test`/arith string operand.
fn test_string_reads(name: &str, s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '$' && i + 1 < bytes.len() {
            let start = i + 1;
            let mut j = start;
            if bytes[j] == b'{' {
                j += 1;
                let w = j;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'[')
                {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'}' {
                    if &bytes[w..j] == name.as_bytes() {
                        return true;
                    }
                    i = j + 1;
                    continue;
                }
                // unbalanced — just try bare
            }
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if &bytes[start..j] == name.as_bytes() {
                return true;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    false
}

fn arith_reads(name: &str, a: &ArithAst) -> bool {
    match a {
        ArithAst::Num(_) => false,
        ArithAst::Var(v) | ArithAst::Ident(v) => v == name,
        ArithAst::Index { var, key } => var == name || arith_reads(name, key),
        ArithAst::Bin { lhs, rhs, .. } => arith_reads(name, lhs) || arith_reads(name, rhs),
        ArithAst::Un { arg, .. } => arith_reads(name, arg),
        ArithAst::Cond { test, then, else_ } => {
            arith_reads(name, test) || arith_reads(name, then) || arith_reads(name, else_)
        }
        ArithAst::Assign { var, rhs, .. } => arith_reads(name, rhs),
        ArithAst::IncDec { var, .. } => false,
        ArithAst::Sizeof(_) => false,
        ArithAst::Cast { arg, .. } => arith_reads(name, arg),
    }
}

/// Does a statement list READ `name`?
pub fn stmts_read(name: &str, stmts: &[IrStmt]) -> bool {
    stmts.iter().any(|s| stmt_reads(name, s))
}

pub fn stmt_reads(name: &str, st: &IrStmt) -> bool {
    match st {
        IrStmt::Ext(n) => crate::shir_nodes::ExtNode::children(&**n).into_iter().any(|c| stmt_reads(name, c)),
        IrStmt::Expr(e) => expr_reads(name, e),
        IrStmt::Assign { targets, expr, .. } => {
            expr_reads(name, expr)
                || targets.iter().any(|t| t.indices.iter().any(|i| expr_reads(name, i)))
        }
        IrStmt::Declare { init, .. } => init.as_ref().is_some_and(|i| expr_reads(name, i)),
        IrStmt::DeclareArray { elements, .. } => elements.iter().any(|e| expr_reads(name, e)),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            expr_reads(name, cond)
                || stmts_read(name, then)
                || elsifs.iter().any(|(c, b)| expr_reads(name, c) || stmts_read(name, b))
                || stmts_read(name, else_)
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            stmts_read(name, body)
                || excepts
                    .iter()
                    .any(|e| stmts_read(name, &e.body))
                || stmts_read(name, else_body)
                || stmts_read(name, finally_body)
        }
        IrStmt::For { iter, body, .. } => expr_reads(name, iter) || stmts_read(name, body),
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            stmts_read(name, init)
                || expr_reads(name, cond)
                || stmts_read(name, step)
                || stmts_read(name, body)
        }
        IrStmt::While { cond, body, .. } | IrStmt::DoWhile { body, cond, .. } => {
            expr_reads(name, cond) || stmts_read(name, body)
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            expr_reads(name, discriminant)
                || clauses.iter().any(|c| stmts_read(name, &c.body))
        }
        IrStmt::Pipeline { stages, .. } => stages.iter().any(|s| stmts_read(name, s)),
        IrStmt::Redirect { inner, redirects } => {
            // redirect TARGETS are reads too (`echo hi > "$f"` — the
            // getVar target must keep the store alive; missing it made
            // dead_store_elim drop the write and the read fold to "" —
            // the bat-sh-go t36_redirect_var cluster)
            stmts_read(name, inner)
                || redirects.iter().any(|r| expr_reads(name, &r.target))
        }
        IrStmt::Exec { cmd, args, .. } => {
            expr_reads(name, cmd)
                || args.iter().any(|a| expr_reads(name, a))
                || args.iter().any(|a| {
                    // exec arg strings with $name interpolations
                    if let IrExpr::Str(s, style) = a {
                        if matches!(
                            style,
                            StrStyle::DoubleQuoted | StrStyle::Heredoc
                        ) {
                            return test_string_reads(name, s);
                        }
                    }
                    false
                })
        }
        IrStmt::Function { body, .. } => stmts_read(name, body),
        IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
            stmts_read(name, body)
        }
        IrStmt::Output { value, .. } => expr_reads(name, value),
        IrStmt::WriteFile { content, .. } => expr_reads(name, content),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e)
        | IrStmt::Die { expr: e, .. } | IrStmt::Warn { expr: e, .. } => expr_reads(name, e),
        IrStmt::Select { clauses } => clauses.iter().any(|c| stmts_read(name, &c.body)),
        IrStmt::Asm {
            template, inputs, outputs, ..
        } => {
            test_string_reads(name, template)
                || inputs.iter().any(|(_, e)| expr_reads(name, e))
                || outputs.iter().any(|(_, e)| expr_reads(name, e))
        }
        IrStmt::Return(None) | IrStmt::Exit(None) | IrStmt::Continue | IrStmt::Break
        | IrStmt::Require(_) | IrStmt::RawText(_) | IrStmt::Label(_) | IrStmt::Goto(_) => false,
    }
}

/// Does the expression WRITE `name` (a bare-name store: assignment
/// target, setVar name, param ":=", arith compound write)?
fn expr_writes(name: &str, e: &IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } => {
            if func == "setVar" {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    return n == name;
                }
            }
            if func == "param" {
                // ":=" assigns the name (READ AND WRITE)
                if let Some(IrExpr::Str(op, _)) = args.first() {
                    if op == ":=" {
                        if let Some(IrExpr::Str(n, _)) = args.get(1) {
                            return n == name;
                        }
                    }
                }
            }
            if func == "let" {
                // `let x=5` / `let x++` — bare identifiers in the arith
                // strings are potential writes
                for a in args {
                    if let IrExpr::Str(s, _) = a {
                        let mut names = Vec::new();
                        let bytes = s.as_bytes();
                        let mut i = 0;
                        while i < bytes.len() {
                            let c = bytes[i] as char;
                            if c.is_ascii_alphabetic() || c == '_' {
                                let start = i;
                                while i < bytes.len()
                                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
                                {
                                    i += 1;
                                }
                                names.push(s[start..i].to_string());
                            } else {
                                i += 1;
                            }
                        }
                        if names.iter().any(|n| n == name) {
                            return true;
                        }
                    }
                }
            }
            false
        }
        IrExpr::Arith(ast) => arith_writes(name, ast),
        _ => false,
    }
}

fn arith_writes(name: &str, a: &ArithAst) -> bool {
    match a {
        ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => var == name,
        _ => false,
    }
}

/// Collect the bare-name write targets of a statement (for DSE).
fn stmt_writes(st: &IrStmt, out: &mut Vec<String>) {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                if t.indices.is_empty() {
                    out.push(t.var.clone());
                }
                // indexed writes: the ARRAY is written — arrays are out
                // of DCE scope (never dropped), so record nothing more
            }
            // compound arith writes on the expr side (x+=1, x++)
            if let IrExpr::Arith(ast) = expr {
                collect_arith_writes(ast, out);
            }
            if let IrExpr::Call { func, args } = expr {
                if func == "param" {
                    if let Some(IrExpr::Str(op, _)) = args.first() {
                        if op == ":=" {
                            if let Some(IrExpr::Str(n, _)) = args.get(1) {
                                out.push(n.clone());
                            }
                        }
                    }
                }
            }
        }
        IrStmt::Declare { vars, .. } => {
            for d in vars {
                out.push(d.name.clone());
            }
        }
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "setVar" => {
            if let Some(IrExpr::Str(n, _)) = args.first() {
                out.push(n.clone());
            }
        }
        _ => {}
    }
}

fn collect_arith_writes(a: &ArithAst, out: &mut Vec<String>) {
    match a {
        ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => out.push(var.clone()),
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_arith_writes(lhs, out);
            collect_arith_writes(rhs, out);
        }
        ArithAst::Cond { test, then, else_ } => {
            collect_arith_writes(test, out);
            collect_arith_writes(then, out);
            collect_arith_writes(else_, out);
        }
        ArithAst::Un { arg, .. } => collect_arith_writes(arg, out),
        ArithAst::Index { key, .. } => collect_arith_writes(key, out),
        _ => {}
    }
}

// ────────────────────────────────────────────────────────────────────────
// const_prop — estree-20260813-183713 (the umbrella)
// ────────────────────────────────────────────────────────────────────────

/// A const var's foldable def value shape.
#[derive(Clone, Copy, PartialEq)]
enum DefKind {
    /// bare copy: `x = y` (y need not be Const; folded reads become reads
    /// of y — same value AT THE DEF POINT, so a y write between the def
    /// and a read breaks the fold)
    Copy,
    /// const int literal (numeric contexts only)
    ConstInt(i64),
}

/// Every read of a candidate var, keyed by statement-list position: the
/// list_id + the index inside it. Built in ONE pre-walk (the read map is
/// immutable during the fold; removal shifts indices, so the fold pass
/// recomputes its view per iteration — the whole pass runs to a fixpoint).
#[derive(Default)]
struct ReadMap {
    /// per var name: list positions that read it
    reads: HashMap<String, Vec<(usize, usize)>>,
}

fn expr_reads_name(name: &str, e: &IrExpr) -> bool {
    match e {
        IrExpr::Var(n, _) | IrExpr::Ident(n) => n == name,
        IrExpr::Index { var, key } => var == name || expr_reads_name(name, key),
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Bool(_) | IrExpr::Json(_)
        | IrExpr::RawExpr(_) | IrExpr::Regex { .. } | IrExpr::Range { .. } => false,
        IrExpr::BinOp { lhs, rhs, .. } => expr_reads_name(name, lhs) || expr_reads_name(name, rhs),
        IrExpr::Ternary { cond, then, else_ } => {
            expr_reads_name(name, cond) || expr_reads_name(name, then) || expr_reads_name(name, else_)
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_reads_name(name, expr) || expr_reads_name(name, default)
        }
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Lit(_) => false,
            InterpPart::Expr(e) => expr_reads_name(name, e),
        }),
        IrExpr::Capture { expr, .. } => expr_reads_name(name, expr),
        IrExpr::Arrow(stmts) | IrExpr::Lambda { body: stmts, .. } => {
            stmts.iter().any(|s| stmt_reads(name, s))
        }
        IrExpr::Array(items) => items.iter().any(|i| expr_reads_name(name, i)),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            expr_reads_name(name, iter) || expr_reads_name(name, elem)
                || cond.as_ref().is_some_and(|c| expr_reads_name(name, c))
        }
        IrExpr::Splice(inner) => expr_reads_name(name, inner),
        IrExpr::Arith(ast) => arith_reads_name(name, ast),
        IrExpr::Object(pairs) => pairs.iter().any(|(_, v)| expr_reads_name(name, v)),
        IrExpr::MethodCall { obj, args, .. } => {
            expr_reads_name(name, obj) || args.iter().any(|a| expr_reads_name(name, a))
        }
        IrExpr::Call { func, args } => {
            // param(op, name, …) READS name (all ops — go-sh-20260816-164300)
            if func == "param" {
                if let Some(IrExpr::Str(n, _)) = args.get(1) {
                    if n == name {
                        return true;
                    }
                }
            } else if matches!(func.as_str(), "getVar" | "listVar" | "arrayItems" | "arrayLen") {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    if n == name {
                        return true;
                    }
                }
            } else if func == "arrayIndex" {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    if n == name {
                        return true;
                    }
                }
            } else if func == "test" {
                for a in args {
                    if let IrExpr::Str(s, _) = a {
                        if test_string_reads(name, s) {
                            return true;
                        }
                    }
                }
            }
            args.iter().any(|a| expr_reads_name(name, a))
        }
    }
}

fn arith_reads_name(name: &str, a: &ArithAst) -> bool {
    match a {
        ArithAst::Num(_) => false,
        ArithAst::Var(v) | ArithAst::Ident(v) => v == name,
        ArithAst::Index { var, key } => var == name || arith_reads_name(name, key),
        ArithAst::Bin { lhs, rhs, .. } => arith_reads_name(name, lhs) || arith_reads_name(name, rhs),
        ArithAst::Un { arg, .. } | ArithAst::Cast { arg, .. } => arith_reads_name(name, arg),
        ArithAst::Cond { test, then, else_ } => {
            arith_reads_name(name, test) || arith_reads_name(name, then) || arith_reads_name(name, else_)
        }
        ArithAst::Assign { var, rhs, .. } => arith_reads_name(name, rhs),
        ArithAst::IncDec { .. } => false,
        ArithAst::Sizeof(_) => false,
    }
}

/// A definition site reads nothing of its own name (an assignment target
/// is a write, not a read — except when the RHS reads it: `x=$x+1` is
/// NOT a Const shape anyway since analyze_var_const excludes it? NO —
/// analyze_var_const only checks the SITE COUNT, so `x=$x+1` at a single
/// site IS Const. The read of x in the def's own RHS is a read BEFORE
/// any later read; our fold keeps the def in that case because the read
/// sits at the def's own index (a read at the def's list index is not
/// "after the def"). To be precise, a read INSIDE the def statement
/// makes the var un-foldable (reading a var you're defining mid-value is
/// order-sensitive).
fn def_expr_self_reads(name: &str, e: &IrExpr) -> bool {
    expr_reads_name(name, e)
}

/// Record reads of `name` in the expression at (list_id, pos).
fn record_expr_reads(rm: &mut ReadMap, name: &str, list: usize, pos: usize, e: &IrExpr) {
    if expr_reads_name(name, e) {
        rm.reads.entry(name.to_string()).or_default().push((list, pos));
    }
}

fn record_stmt_reads(rm: &mut ReadMap, name: &str, list: usize, pos: usize, st: &IrStmt) {
    match st {
        IrStmt::Expr(e) => record_expr_reads(rm, name, list, pos, e),
        IrStmt::Assign { targets, expr, .. } => {
            record_expr_reads(rm, name, list, pos, expr);
            for t in targets {
                for i in &t.indices {
                    record_expr_reads(rm, name, list, pos, i);
                }
            }
        }
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                record_expr_reads(rm, name, list, pos, i);
            }
        }
        IrStmt::Exec { cmd, args, .. } => {
            record_expr_reads(rm, name, list, pos, cmd);
            for a in args {
                record_expr_reads(rm, name, list, pos, a);
            }
        }
        _ => {}
    }
}

/// Is the statement straight-line (cannot divert control flow)?
fn is_straight_line(st: &IrStmt) -> bool {
    match st {
        IrStmt::Expr(e) => match e {
            IrExpr::Capture { .. } | IrExpr::Arrow(_) | IrExpr::Lambda { .. } => false,
            IrExpr::Call { func, .. } => !matches!(
                func.as_str(),
                "subshell" | "pipeline" | "redirect" | "background" | "fnCall" | "callDirect"
            ),
            _ => true,
        },
        IrStmt::Assign { .. } | IrStmt::Declare { .. } | IrStmt::DeclareArray { .. }
        | IrStmt::Output { .. } | IrStmt::WriteFile { .. } | IrStmt::Require(_)
        | IrStmt::RawText(_) | IrStmt::Return(_) | IrStmt::Exit(_) | IrStmt::SetChildError(_) => true,
        _ => false,
    }
}

/// The foldable value of a def expression (bare single value only).
fn def_kind(expr: &IrExpr) -> Option<DefKind> {
    match expr {
        IrExpr::Var(_, _) | IrExpr::Ident(_) => Some(DefKind::Copy),
        IrExpr::Int(n) => Some(DefKind::ConstInt(*n)),
        IrExpr::Str(s, _) => s.parse::<i64>().ok().map(DefKind::ConstInt),
        _ => None,
    }
}

fn copy_y_of(expr: &IrExpr) -> Option<String> {
    match expr {
        IrExpr::Var(n, _) => Some(n.clone()),
        _ => None,
    }
}

/// Copy-fold a read of `x` → read of `y` in the expression. Returns true
/// if any read was replaced.
fn copy_fold_expr(x: &str, y: &str, e: &mut IrExpr) -> bool {
    let mut changed = false;
    match e {
        IrExpr::Var(n, _) | IrExpr::Ident(n) if n == x => {
            *e = IrExpr::Var(y.to_string(), None);
            changed = true;
        }
        IrExpr::Index { var, key } => {
            changed |= copy_fold_expr(x, y, key);
            let _ = var;
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            changed |= copy_fold_expr(x, y, lhs);
            changed |= copy_fold_expr(x, y, rhs);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            changed |= copy_fold_expr(x, y, cond);
            changed |= copy_fold_expr(x, y, then);
            changed |= copy_fold_expr(x, y, else_);
        }
        IrExpr::DefinedOr { expr, default } => {
            changed |= copy_fold_expr(x, y, expr);
            changed |= copy_fold_expr(x, y, default);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let InterpPart::Expr(e) = p {
                    changed |= copy_fold_expr(x, y, e);
                }
            }
        }
        IrExpr::Capture { expr, .. } => changed |= copy_fold_expr(x, y, expr),
        IrExpr::Array(items) => {
            for i in items.iter_mut() {
                changed |= copy_fold_expr(x, y, i);
            }
        }
        IrExpr::Arith(ast) => changed |= copy_fold_arith(x, y, ast),
        IrExpr::Object(pairs) => {
            for (_, v) in pairs.iter_mut() {
                changed |= copy_fold_expr(x, y, v);
            }
        }
        IrExpr::Call { func, args } => {
            if func == "getVar" || func == "listVar" {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    if n == x {
                        *args.first_mut().unwrap() = IrExpr::Str(y.to_string(), StrStyle::DoubleQuoted);
                        changed = true;
                    }
                }
            } else if func == "param" {
                if let Some(IrExpr::Str(n, _)) = args.get(1) {
                    if n == x {
                        if let Some(IrExpr::Str(_, st)) = args.get_mut(1) {
                            let st = st.clone();
                            args[1] = IrExpr::Str(y.to_string(), st);
                            changed = true;
                        }
                    }
                }
            } else if func == "arrayIndex" {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    if n == x {
                        let _ = n;
                    }
                }
            }
            for a in args.iter_mut().skip(1) {
                changed |= copy_fold_expr(x, y, a);
            }
        }
        _ => {}
    }
    changed
}

fn copy_fold_arith(x: &str, y: &str, a: &mut ArithAst) -> bool {
    match a {
        ArithAst::Var(n) | ArithAst::Ident(n) if n == x => {
            *a = ArithAst::Var(y.to_string());
            true
        }
        ArithAst::Num(_) => false,
        ArithAst::Bin { lhs, rhs, .. } => copy_fold_arith(x, y, lhs) | copy_fold_arith(x, y, rhs),
        ArithAst::Un { arg, .. } | ArithAst::Cast { arg, .. } => copy_fold_arith(x, y, arg),
        ArithAst::Cond { test, then, else_ } => {
            copy_fold_arith(x, y, test) | copy_fold_arith(x, y, then) | copy_fold_arith(x, y, else_)
        }
        ArithAst::Index { key, .. } => copy_fold_arith(x, y, key),
        ArithAst::Assign { rhs, .. } => copy_fold_arith(x, y, rhs),
        _ => false,
    }
}

/// Fold const-int reads into NUMERIC contexts of the expression.
fn const_fold_expr(x: &str, v: i64, e: &mut IrExpr, folded: &mut bool) {
    match e {
        IrExpr::Arith(ast) => {
            fold_arith_pos(x, ast, v, folded);
        }
        IrExpr::Index { key, .. } => fold_numeric_expr(x, key, v, folded),
        IrExpr::BinOp { lhs, rhs, .. } => {
            const_fold_expr(x, v, lhs, folded);
            const_fold_expr(x, v, rhs, folded);
        }
        IrExpr::Call { func, args } => {
            if func == "test" {
                for a in args.iter_mut() {
                    if let IrExpr::Str(s, st) = a {
                        if let Some(ns) = replace_bare_int_in_test(s, x, v) {
                            *a = IrExpr::Str(ns, st.clone());
                            *folded = true;
                        }
                    } else {
                        const_fold_expr(x, v, a, folded);
                    }
                }
            }
        }
        IrExpr::Array(items) => {
            for i in items.iter_mut() {
                const_fold_expr(x, v, i, folded);
            }
        }
        _ => {}
    }
}

fn fold_arith_pos(x: &str, a: &mut ArithAst, v: i64, folded: &mut bool) {
    match a {
        ArithAst::Var(n) | ArithAst::Ident(n) if n == x => {
            *a = ArithAst::Num(v);
            *folded = true;
        }
        ArithAst::Var(_) | ArithAst::Ident(_) => {} // not this var — leave
        ArithAst::Num(_) => {}
        ArithAst::Index { key, .. } => fold_arith_pos(x, key, v, folded),
        ArithAst::Bin { lhs, rhs, .. } | ArithAst::Cond { test: lhs, then: _, else_: rhs } => {
            fold_arith_pos(x, lhs, v, folded);
            fold_arith_pos(x, rhs, v, folded);
        }
        ArithAst::Un { arg, .. } | ArithAst::Cast { arg, .. } => fold_arith_pos(x, arg, v, folded),
        ArithAst::Assign { rhs, .. } => fold_arith_pos(x, rhs, v, folded),
        ArithAst::Sizeof(_) | ArithAst::IncDec { .. } => {}
    }
}

fn fold_numeric_expr(x: &str, e: &mut IrExpr, v: i64, folded: &mut bool) {
    match e {
        IrExpr::Var(n, _) | IrExpr::Ident(n) if n == x => {
            *e = IrExpr::Int(v);
            *folded = true;
        }
        IrExpr::Arith(ast) => fold_arith_pos(x, ast, v, folded),
        IrExpr::BinOp { lhs, rhs, .. } => {
            fold_numeric_expr(x, lhs, v, folded);
            fold_numeric_expr(x, rhs, v, folded);
        }
        IrExpr::Index { key, .. } => fold_numeric_expr(x, key, v, folded),
        IrExpr::Array(items) => {
            for i in items.iter_mut() {
                fold_numeric_expr(x, i, v, folded);
            }
        }
        _ => {}
    }
}

/// Replace `$x` / `${x}` with the literal in a numeric test operand.
fn replace_bare_int_in_test(s: &str, name: &str, v: i64) -> Option<String> {
    if !s.contains(name) {
        return None;
    }
    let mut out = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut changed = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '$' && i + 1 < bytes.len() {
            let start = i + 1;
            let mut j = start;
            let mut braced = false;
            if bytes[j] == b'{' {
                braced = true;
                j += 1;
            }
            let w = j;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let nm = &s[w..j];
            if nm == name && (j >= bytes.len() || !braced || bytes[j] == b'}') {
                out.push_str(&v.to_string());
                i = if braced && j < bytes.len() { j + 1 } else { j };
                changed = true;
                continue;
            }
            out.push_str(&s[i..j]);
            i = j;
        } else {
            out.push(c);
            i += 1;
        }
    }
    changed.then_some(out)
}


/// Invoke `f` on every child statement list of `s` (immutable borrows).
fn walk_children<'a>(s: &'a IrStmt, mut f: impl FnMut(&'a [IrStmt])) {
    match s {
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            f(then);
            f(else_);
            for (_, b) in elsifs {
                f(b);
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            f(body);
            f(else_body);
            f(finally_body);
            for e in excepts {
                f(&e.body);
            }
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } | IrStmt::For { body, .. }
        | IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => f(body),
        IrStmt::ForInit {
            init,
            step,
            body,
            ..
        } => {
            f(init);
            f(step);
            f(body);
        }
        IrStmt::Case { clauses, .. } => {
            for c in clauses {
                f(&c.body);
            }
        }
        IrStmt::Function { body, .. } => f(body),
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages {
                f(stage);
            }
        }
        IrStmt::Redirect { inner, .. } => f(inner),
        _ => {}
    }
}

/// Invoke `f` on every child statement list of `s` (mutable borrows).
fn walk_children_mut<'a>(s: &'a mut IrStmt, mut f: impl FnMut(&'a mut Vec<IrStmt>)) {
    match s {
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            f(then);
            f(else_);
            for (_, b) in elsifs {
                f(b);
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            f(body);
            f(else_body);
            f(finally_body);
            for e in excepts {
                f(&mut e.body);
            }
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } | IrStmt::For { body, .. }
        | IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => f(body),
        IrStmt::ForInit {
            init,
            step,
            body,
            ..
        } => {
            f(init);
            f(step);
            f(body);
        }
        IrStmt::Case { clauses, .. } => {
            for c in clauses {
                f(&mut c.body);
            }
        }
        IrStmt::Function { body, .. } => f(body),
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages {
                f(stage);
            }
        }
        IrStmt::Redirect { inner, .. } => f(inner),
        _ => {}
    }
}

/// The whole-program analysis state for the fold:
/// - per Const var: the DEF's list ADDRESS (the `Vec<IrStmt>` that holds
///   the def) + the def's ORIGINAL index; every READ position (list
///   address + original index). List-address identity is stable because
///   the fold phase removes elements in-place (Vec::remove keeps the
///   buffer address) and the addresses are captured before any mutation.
#[derive(Default)]
struct FoldState {
    defs: HashMap<String, (usize, usize)>,
    reads: HashMap<String, Vec<(usize, usize)>>,
}

fn build_state(prog: &IrProgram, consts: &HashSet<String>) -> FoldState {
    let mut st = FoldState::default();
    fn walk_list(
        st: &mut FoldState,
        consts: &HashSet<String>,
        stmts: &[IrStmt],
    ) {
        let addr = stmts.as_ptr() as usize;
        for (pos, s) in stmts.iter().enumerate() {
            for c in consts {
                if stmt_reads(c, s) {
                    st.reads.entry(c.clone()).or_default().push((addr, pos));
                }
            }
            match s {
                IrStmt::Assign { targets, expr, .. }
                    if targets.len() == 1 && targets[0].indices.is_empty() =>
                {
                    if consts.contains(&targets[0].var) {
                        let _ = expr;
                        st.defs
                            .entry(targets[0].var.clone())
                            .or_insert((addr, pos));
                    }
                }
                IrStmt::Expr(IrExpr::Call { func, args })
                    if func == "setVar" && args.len() >= 2 =>
                {
                    if let Some(IrExpr::Str(n, _)) = args.first() {
                        if consts.contains(n.as_str()) {
                            st.defs.entry(n.clone()).or_insert((addr, pos));
                        }
                    }
                }
                IrStmt::Declare { vars, init, local }
                    if vars.len() == 1 && !*local && init.is_some() =>
                {
                    if let Some(d) = vars.first() {
                        if consts.contains(&d.name) {
                            st.defs.entry(d.name.clone()).or_insert((addr, pos));
                        }
                    }
                }
                _ => {}
            }
            walk_children(s, |b| walk_list(st, consts, b));
        }
    }
    walk_list(&mut st, consts, &prog.stmts);
    for sub in &prog.subs {
        walk_list(&mut st, consts, &sub.body);
    }
    st
}

/// Fold ONE def site: reads must ALL sit in the SAME list (address) as
/// the def, at original indices > di, with nothing between the def and
/// each read but straight-line statements. Returns true when the def was
/// dropped.
fn fold_def_at(
    list: &mut Vec<IrStmt>,
    di: usize,
    name: &str,
    kind: DefKind,
    copy_y: Option<&str>,
    reads: &[(usize, usize)],
    addr: usize,
) -> bool {
    // all reads here (same list) have index > di; reads elsewhere were
    // already rejected by the caller (fold_def decision in run_on_list).
    for (_, p) in reads {
        if *p <= di {
            return false; // read at/before the def — order violation
        }
        for seg in &list[di + 1..=*p] {
            if !is_straight_line(seg) {
                return false;
            }
        }
    }
    // copy-prop y-write check: y must not be written in the tail
    if let Some(y) = copy_y {
        if y == name {
            return false;
        }
        for seg in &list[di + 1..] {
            let mut ws = Vec::new();
            stmt_writes(seg, &mut ws);
            if ws.iter().any(|w| w == y) {
                return false;
            }
        }
    }
    let _ = addr;
    // fold every read in the tail (index > di)
    let mut folded = false;
    for seg in list.iter_mut().skip(di + 1) {
        match kind {
            DefKind::Copy => {
                if let Some(y) = copy_y {
                    fold_copy_stmt(name, y, seg, &mut folded);
                }
            }
            DefKind::ConstInt(v) => {
                fold_const_stmt(name, v, seg, &mut folded);
            }
        }
    }
    folded
}

fn fold_copy_stmt(x: &str, y: &str, st: &mut IrStmt, folded: &mut bool) {
    match st {
        IrStmt::Expr(e) => {
            if copy_fold_expr(x, y, e) {
                *folded = true;
            }
        }
        IrStmt::Assign { targets, expr, .. } => {
            if copy_fold_expr(x, y, expr) {
                *folded = true;
            }
            for t in targets {
                for i in t.indices.iter_mut() {
                    if copy_fold_expr(x, y, i) {
                        *folded = true;
                    }
                }
            }
        }
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                if copy_fold_expr(x, y, i) {
                    *folded = true;
                }
            }
        }
        _ => {}
    }
}

fn fold_const_stmt(x: &str, v: i64, st: &mut IrStmt, folded: &mut bool) {
    match st {
        IrStmt::Expr(e) => const_fold_expr(x, v, e, folded),
        IrStmt::Assign { targets, expr, .. } => {
            const_fold_expr(x, v, expr, folded);
            for t in targets {
                for i in t.indices.iter_mut() {
                    const_fold_expr(x, v, i, folded);
                }
            }
        }
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                const_fold_expr(x, v, i, folded);
            }
        }
        _ => {}
    }
}

/// Run the fold over one statement list. `st` is the build-time state
/// (immutable — positions/addresses are ORIGINAL; a removed def shifts
/// later indices, which the caller handles by iterating the fixpoint).
fn run_on_list(
    list: &mut Vec<IrStmt>,
    consts: &HashSet<String>,
    st: &FoldState,
) -> bool {
    let addr = list.as_ptr() as usize;
    let mut dropped = false;
    let mut i = 0;
    while i < list.len() {
        // is this a def site of a Const var?
        let name = match &list[i] {
            IrStmt::Assign { targets, .. }
                if targets.len() == 1 && targets[0].indices.is_empty() =>
            {
                if consts.contains(&targets[0].var) {
                    Some(targets[0].var.clone())
                } else {
                    None
                }
            }
            IrStmt::Expr(IrExpr::Call { func, args })
                if func == "setVar" && args.len() >= 2 =>
            {
                match args.first() {
                    Some(IrExpr::Str(n, _)) if consts.contains(n.as_str()) => Some(n.clone()),
                    _ => None,
                }
            }
            IrStmt::Declare { vars, init, local }
                if vars.len() == 1 && !*local && init.is_some() =>
            {
                vars.first().and_then(|d| {
                    if consts.contains(&d.name) {
                        Some(d.name.clone())
                    } else {
                        None
                    }
                })
            }
            _ => None,
        };
        let Some(name) = name else {
            i += 1;
            continue;
        };
        // the def's original (addr, pos) must match this (addr, i)
        let Some((d_addr, d_pos)) = st.defs.get(&name) else {
            i += 1;
            continue;
        };
        if *d_addr != addr || *d_pos != i {
            i += 1; // shifted by an earlier drop — the next fixpoint
            continue; // iteration re-syncs
        }
        // the def value
        let def_expr = match &list[i] {
            IrStmt::Assign { expr, .. } => Some(expr.clone()),
            IrStmt::Expr(IrExpr::Call { args, .. }) => args.get(1).cloned(),
            IrStmt::Declare { init, .. } => init.clone(),
            _ => None,
        };
        let Some(de) = def_expr else {
            i += 1;
            continue;
        };
        // self-read honesty: `x=$x+1` — order-sensitive, no fold
        if def_expr_self_reads(&name, &de) {
            i += 1;
            continue;
        }
        let Some(kind) = def_kind(&de) else {
            i += 1;
            continue;
        };
        let copy_y = copy_y_of(&de);
        // the read set for this var
        let all_reads = st.reads.get(&name).cloned().unwrap_or_default();
        // (a) any read in ANOTHER list (different address, or a nested
        // list address) → disqualify the fold (keep the def).
        if all_reads.iter().any(|(a, _)| *a != addr) {
            i += 1;
            continue;
        }
        let here: Vec<(usize, usize)> = all_reads
            .iter()
            .filter(|(a, _)| *a == addr)
            .cloned()
            .collect();
        if here.is_empty() {
            // zero reads anywhere → DSE drop
            list.remove(i);
            dropped = true;
            continue;
        }
        if fold_def_at(list, i, &name, kind, copy_y.as_deref(), &here, addr) {
            list.remove(i);
            dropped = true;
        } else {
            i += 1;
        }
    }
    dropped
}

/// The statement-list recursor (mutating fold; the state is immutable).
fn const_prop_recurse(list: &mut Vec<IrStmt>, consts: &HashSet<String>, st: &FoldState) -> bool {
    let mut changed = run_on_list(list, consts, st);
    for s in list.iter_mut() {
        walk_children_mut(s, |b| changed |= const_prop_recurse(b, consts, st));
    }
    changed
}

/// `shir_passes::const_prop(&mut IrProgram) -> bool` — the A1 SSA-style
/// const/copy propagation + dead-store elimination over the `Const`
/// verdict pool (estree-20260813-183713, honesty rules:
/// dominance/order, bare-copy only, numeric-context const substitution,
/// DSE for zero-read vars, never-touch list). Runs to a fixpoint.
pub fn const_prop(prog: &mut IrProgram) -> bool {
    let consts: HashSet<String> = prog
        .var_const
        .iter()
        .filter(|(_, k)| matches!(k, VarKind::Const))
        .map(|(n, _)| n.clone())
        .collect();
    if consts.is_empty() {
        return false;
    }
    let mut changed = false;
    loop {
        let st = build_state(prog, &consts);
        let mut round = const_prop_recurse(&mut prog.stmts, &consts, &st);
        for sub in prog.subs.iter_mut() {
            round |= const_prop_recurse(&mut sub.body, &consts, &st);
        }
        if !round {
            break;
        }
        changed = true;
    }
    changed
}

// ────────────────────────────────────────────────────────────────────────
// dead_store_elim — estree-20260813-182435 (+ go-sh-20260816-164300)
// ────────────────────────────────────────────────────────────────────────

/// Is the expression side-effect-free (safe to drop wholesale)?
fn expr_pure(e: &IrExpr) -> bool {
    match e {
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Var(_, _) | IrExpr::Ident(_)
        | IrExpr::Bool(_) | IrExpr::Json(_) | IrExpr::RawExpr(_) | IrExpr::Regex { .. }
        | IrExpr::Range { .. } => true,
        IrExpr::BinOp { lhs, rhs, .. } => expr_pure(lhs) && expr_pure(rhs),
        IrExpr::Ternary { cond, then, else_ } => {
            expr_pure(cond) && expr_pure(then) && expr_pure(else_)
        }
        IrExpr::DefinedOr { expr, default } => expr_pure(expr) && expr_pure(default),
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            InterpPart::Lit(_) => true,
            InterpPart::Expr(e) => expr_pure(e),
        }),
        IrExpr::Array(items) => items.iter().all(expr_pure),
        IrExpr::Object(pairs) => pairs.iter().all(|(_, v)| expr_pure(v)),
        IrExpr::Splice(inner) => expr_pure(inner),
        IrExpr::Index { key, .. } => expr_pure(key),
        IrExpr::Arith(_) => true, // compound writes checked by the caller
        IrExpr::MethodCall { obj, args, .. } => expr_pure(obj) && args.iter().all(expr_pure),
        IrExpr::Call { func, args } => {
            matches!(
                func.as_str(),
                "getVar" | "listVar" | "arrayIndex" | "arrayLen" | "param" | "test" | "arith"
            ) && args.iter().all(expr_pure)
        }
        IrExpr::Capture { .. } | IrExpr::Arrow(_) | IrExpr::Lambda { .. }
        | IrExpr::ArrayComp { .. } => false,
    }
}

/// Drop never-read scalar stores. Returns true if anything changed.
pub fn dead_store_elim(prog: &mut IrProgram) -> bool {
    // 1. the per-var READ signal (whole program)
    let mut read_names: HashSet<String> = HashSet::new();
    for st in &prog.stmts {
        collect_stmt_reads(st, &mut read_names);
    }
    for sub in &prog.subs {
        for st in &sub.body {
            collect_stmt_reads(st, &mut read_names);
        }
    }
    // 2. the never-touch names: export/readonly/declare(-p)-referenced
    let mut guarded: HashSet<String> = HashSet::new();
    for st in &prog.stmts {
        collect_decl_guard(st, &mut guarded);
    }
    for sub in &prog.subs {
        for st in &sub.body {
            collect_decl_guard(st, &mut guarded);
        }
    }
    // 3. drop stores for names with zero reads
    let mut changed = dce_list(&mut prog.stmts, &read_names, &guarded);
    for sub in prog.subs.iter_mut() {
        changed |= dce_list(&mut sub.body, &read_names, &guarded);
    }
    // 4. recurse into bodies
    for st in prog.stmts.iter_mut() {
        changed |= dce_stmt_bodies(st, &read_names, &guarded);
    }
    changed
}

fn dce_list(stmts: &mut Vec<IrStmt>, reads: &HashSet<String>, guarded: &HashSet<String>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        if drop_dead_store(&stmts[i], reads, guarded) {
            stmts.remove(i);
            changed = true;
        } else {
            i += 1;
        }
    }
    changed
}

fn dce_stmt_bodies(st: &mut IrStmt, reads: &HashSet<String>, guarded: &HashSet<String>) -> bool {
    let mut changed = false;
    walk_children_mut(st, |b| changed |= dce_list(b, reads, guarded));
    changed
}

/// Would dropping `st` be unobserved? (a dead scalar store with pure RHS)
fn drop_dead_store(st: &IrStmt, reads: &HashSet<String>, guarded: &HashSet<String>) -> bool {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            let all_dead = targets.iter().all(|t| {
                if !t.indices.is_empty() {
                    return false;
                }
                if reads.contains(&t.var) || guarded.contains(&t.var) {
                    return false;
                }
                // baked-subscript write (`aa[one]=1`): the read signal
                // registers the BASE name (`aa` from arrayIndex("aa",
                // "one")) — keep the write when the base is read.
                if let Some(pos) = t.var.find('[') {
                    let base = &t.var[..pos];
                    if reads.contains(base) || guarded.contains(base) {
                        return false;
                    }
                }
                true
            });
            if targets.is_empty() || !all_dead {
                return false;
            }
            if !expr_pure(expr) {
                return false;
            }
            if let IrExpr::Arith(ast) = expr {
                let mut ws = Vec::new();
                collect_arith_writes(ast, &mut ws);
                if ws.iter().any(|w| reads.contains(w) || guarded.contains(w)) {
                    return false;
                }
            }
            true
        }
        IrStmt::Declare { vars, init, local } => {
            if *local {
                // function-scope locals: keep (scope-stack semantics)
                return false;
            }
            let all_dead = vars
                .iter()
                .all(|d| !reads.contains(&d.name) && !guarded.contains(&d.name));
            if vars.is_empty() || !all_dead {
                return false;
            }
            match init {
                None => true,
                Some(e) => expr_pure(e),
            }
        }
        IrStmt::Expr(IrExpr::Call { func, args })
            if func == "setVar" && args.len() >= 2 =>
        {
            let Some(IrExpr::Str(n, _)) = args.first() else {
                return false;
            };
            if reads.contains(n) || guarded.contains(n) {
                return false;
            }
            expr_pure(&args[1])
        }
        _ => false,
    }
}

/// Collect every name READ by the statement (for the DCE read signal).
fn collect_stmt_reads(st: &IrStmt, out: &mut HashSet<String>) {
    let mut names: Vec<String> = Vec::new();
    collect_stmt_read_names(st, &mut names);
    for n in names {
        out.insert(n);
    }
}

fn collect_stmt_read_names(st: &IrStmt, out: &mut Vec<String>) {
    match st {
        IrStmt::Expr(e) => collect_expr_read_names(e, out),
        IrStmt::Assign { targets, expr, .. } => {
            collect_expr_read_names(expr, out);
            for t in targets {
                for i in t.indices.iter() {
                    collect_expr_read_names(i, out);
                }
            }
        }
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                collect_expr_read_names(i, out);
            }
        }
        IrStmt::DeclareArray { elements, .. } => {
            for e in elements {
                collect_expr_read_names(e, out);
            }
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            collect_expr_read_names(cond, out);
            for c in then {
                collect_stmt_read_names(c, out);
            }
            for (c, b) in elsifs {
                collect_expr_read_names(c, out);
                for s in b {
                    collect_stmt_read_names(s, out);
                }
            }
            for c in else_ {
                collect_stmt_read_names(c, out);
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            for c in body {
                collect_stmt_read_names(c, out);
            }
            for exc in excepts {
                for c in &exc.body {
                    collect_stmt_read_names(c, out);
                }
            }
            for c in else_body {
                collect_stmt_read_names(c, out);
            }
            for c in finally_body {
                collect_stmt_read_names(c, out);
            }
        }
        IrStmt::For { iter, body, .. } => {
            collect_expr_read_names(iter, out);
            for c in body {
                collect_stmt_read_names(c, out);
            }
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            for c in init {
                collect_stmt_read_names(c, out);
            }
            collect_expr_read_names(cond, out);
            for c in step {
                collect_stmt_read_names(c, out);
            }
            for c in body {
                collect_stmt_read_names(c, out);
            }
        }
        IrStmt::While { cond, body, .. } | IrStmt::DoWhile { body, cond, .. } => {
            collect_expr_read_names(cond, out);
            for c in body {
                collect_stmt_read_names(c, out);
            }
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            collect_expr_read_names(discriminant, out);
            for c in clauses {
                for s in &c.body {
                    collect_stmt_read_names(s, out);
                }
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages {
                for s in stage {
                    collect_stmt_read_names(s, out);
                }
            }
        }
        IrStmt::Redirect { inner, redirects } => {
            for s in inner {
                collect_stmt_read_names(s, out);
            }
            // redirect TARGETS are reads (getVar target — `echo hi > "$f"`;
            // missing them dropped the store and the target folded to "")
            for r in redirects {
                collect_expr_read_names(&r.target, out);
            }
        }
        IrStmt::Exec { cmd, args, .. } => {
            collect_expr_read_names(cmd, out);
            for a in args {
                collect_expr_read_names(a, out);
            }
        }
        IrStmt::Function { body, .. } => {
            for c in body {
                collect_stmt_read_names(c, out);
            }
        }
        IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
            for c in body {
                collect_stmt_read_names(c, out);
            }
        }
        IrStmt::Output { value, .. } => collect_expr_read_names(value, out),
        IrStmt::WriteFile { path, content, .. } => {
            collect_expr_read_names(path, out);
            collect_expr_read_names(content, out);
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e)
        | IrStmt::Die { expr: e, .. } | IrStmt::Warn { expr: e, .. } => {
            collect_expr_read_names(e, out)
        }
        IrStmt::Select { clauses } => {
            for c in clauses {
                for s in &c.body {
                    collect_stmt_read_names(s, out);
                }
            }
        }
        IrStmt::Asm {
            inputs, outputs, ..
        } => {
            for (_, e) in inputs.iter().chain(outputs.iter()) {
                collect_expr_read_names(e, out);
            }
        }
        _ => {}
    }
}

fn collect_expr_read_names(e: &IrExpr, out: &mut Vec<String>) {
    match e {
        IrExpr::Var(n, _) | IrExpr::Ident(n) => out.push(n.clone()),
        IrExpr::Index { var, key } => {
            out.push(var.clone());
            collect_expr_read_names(key, out);
        }
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Bool(_) | IrExpr::Json(_)
        | IrExpr::RawExpr(_) | IrExpr::Regex { .. } | IrExpr::Range { .. } => {}
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_expr_read_names(lhs, out);
            collect_expr_read_names(rhs, out);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            collect_expr_read_names(cond, out);
            collect_expr_read_names(then, out);
            collect_expr_read_names(else_, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_expr_read_names(expr, out);
            collect_expr_read_names(default, out);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(e) = p {
                    collect_expr_read_names(e, out);
                }
            }
        }
        IrExpr::Capture { expr, .. } => collect_expr_read_names(expr, out),
        IrExpr::Arrow(stmts) | IrExpr::Lambda { body: stmts, .. } => {
            for s in stmts {
                collect_stmt_read_names(s, out);
            }
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_expr_read_names(i, out);
            }
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            collect_expr_read_names(iter, out);
            collect_expr_read_names(elem, out);
            if let Some(c) = cond {
                collect_expr_read_names(c, out);
            }
        }
        IrExpr::Splice(inner) => collect_expr_read_names(inner, out),
        IrExpr::Arith(ast) => collect_arith_read_names(ast, out),
        IrExpr::Object(pairs) => {
            for (_, v) in pairs {
                collect_expr_read_names(v, out);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            collect_expr_read_names(obj, out);
            for a in args {
                collect_expr_read_names(a, out);
            }
        }
        IrExpr::Call { func, args } => {
            if func == "param" {
                // the NAME arg (args[1]) is a READ (go-sh-20260816-164300)
                if let Some(IrExpr::Str(n, _)) = args.get(1) {
                    out.push(n.clone());
                    // baked-subscript read (`aa[$k]` / `aa[one]`): the
                    // base name is the array — register it so a
                    // baked-subscript WRITE (`aa[two]=2`) is not
                    // dead-eliminated when the array is read dynamically.
                    if let Some(pos) = n.find('[') {
                        out.push(n[..pos].to_string());
                    }
                }
            } else if matches!(
                func.as_str(),
                "getVar" | "listVar" | "arrayItems" | "arrayLen"
            ) {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    out.push(n.clone());
                    if let Some(pos) = n.find('[') {
                        out.push(n[..pos].to_string());
                    }
                }
            } else if func == "arrayIndex" {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    out.push(n.clone());
                }
            } else if func == "test" || func == "let" {
                for a in args {
                    if let IrExpr::Str(s, _) = a {
                        for nm in bare_dollar_names(s) {
                            out.push(nm);
                        }
                    } else {
                        collect_expr_read_names(a, out);
                    }
                }
            }
            for a in args {
                collect_expr_read_names(a, out);
            }
        }
    }
}

/// Bare `$name` / `${name}` tokens inside a test/let string operand.
fn bare_dollar_names(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '$' && i + 1 < bytes.len() && bytes[i + 1] != b'(' {
            let start = i + 1;
            let mut j = start;
            let mut braced = false;
            if bytes[j] == b'{' {
                braced = true;
                j += 1;
            }
            let w = j;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            let nm = &s[w..j];
            if !nm.is_empty()
                && (j >= bytes.len() || !braced || bytes[j] == b'}')
                && crate::shared_utils::SharedUtils::is_variable_name(nm)
            {
                out.push(nm.to_string());
            }
            i = if braced && j < bytes.len() { j + 1 } else { j };
        } else {
            i += 1;
        }
    }
    out
}

fn collect_arith_read_names(a: &ArithAst, out: &mut Vec<String>) {
    match a {
        ArithAst::Var(n) | ArithAst::Ident(n) => out.push(n.clone()),
        ArithAst::Num(_) | ArithAst::Sizeof(_) => {}
        ArithAst::Index { var, key } => {
            out.push(var.clone());
            collect_arith_read_names(key, out);
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            collect_arith_read_names(lhs, out);
            collect_arith_read_names(rhs, out);
        }
        ArithAst::Un { arg, .. } | ArithAst::Cast { arg, .. } => collect_arith_read_names(arg, out),
        ArithAst::Cond { test, then, else_ } => {
            collect_arith_read_names(test, out);
            collect_arith_read_names(then, out);
            collect_arith_read_names(else_, out);
        }
        ArithAst::Assign { var, rhs, .. } => {
            out.push(var.clone());
            collect_arith_read_names(rhs, out);
        }
        ArithAst::IncDec { var, .. } => out.push(var.clone()),
    }
}

/// guard names: `declare -p x` / `export x` / `readonly x` exec args.
fn collect_decl_guard(st: &IrStmt, out: &mut HashSet<String>) {
    match st {
        IrStmt::Exec { args, .. } => {
            for a in args {
                if let IrExpr::Str(s, _) = a {
                    if s.starts_with("declare") || s.starts_with("export")
                        || s.starts_with("readonly") || s.starts_with("typeset")
                    {
                        for w in s.split_whitespace() {
                            if let Some(n) = w.strip_prefix("-p") {
                                if !n.is_empty() {
                                    out.insert(n.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

// ────────────────────────────────────────────────────────────────────────
// the combined entry — called at A1 ingress right after strip_cfor
// ────────────────────────────────────────────────────────────────────────

/// Run the A1 optimizer family. Returns true if anything changed.
pub fn optimize(prog: &mut IrProgram) -> bool {
    let mut changed = false;
    // 182434: the const-pool arith fold
    crate::shir::const_fold_arith(prog);
    changed = true;
    // 183713: const/copy prop + DSE for Const verdicts
    changed |= const_prop(prog);
    // 182435 + go-sh-20260816-164300: never-read store drop (param reads)
    changed |= dead_store_elim(prog);
    changed
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ArithAst, StrStyle};

    fn prog_of(stmts: Vec<IrStmt>) -> IrProgram {
        IrProgram {
            var_nospace: vec![],
            var_bash_env: vec![],
            imports: vec![],
            requires: vec![],
            stmts,
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![("x".to_string(), VarKind::Const)],
            var_lifetimes: vec![],
        }
    }

    fn assign(name: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![crate::ir::AssignTarget {
                var: name.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        }
    }

    #[test]
    fn dce_drops_never_read_store() {
        // x=5; echo hi  → the x store drops (x never read)
        let prog = prog_of(vec![
            assign("x", IrExpr::Int(5)),
            IrStmt::Expr(IrExpr::Call {
                func: "builtin".to_string(),
                args: vec![
                    IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Array(vec![IrExpr::Str("hi".to_string(), StrStyle::DoubleQuoted)]),
                ],
            }),
        ]);
        let mut prog = prog;
        assert!(dead_store_elim(&mut prog));
        assert_eq!(prog.stmts.len(), 1);
    }

    #[test]
    fn dce_keeps_param_read_store() {
        // s="hello"; … param("slice","s","1","2") reads s → the store stays
        // (go-sh-20260816-164300: dropping it = ReferenceError)
        let prog = prog_of(vec![
            assign("s", IrExpr::Str("hello".to_string(), StrStyle::DoubleQuoted)),
            IrStmt::Expr(IrExpr::Call {
                func: "param".to_string(),
                args: vec![
                    IrExpr::Str("slice".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Str("s".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Str("1".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Str("2".to_string(), StrStyle::DoubleQuoted),
                ],
            }),
        ]);
        let mut prog = prog;
        assert!(!dead_store_elim(&mut prog));
        assert_eq!(prog.stmts.len(), 2);
    }

    #[test]
    fn dce_keeps_redirect_target_read_store() {
        // f=out.txt; echo hi > "$f" — the redirect TARGET getVar(f) is a
        // READ: dropping the store folds the target to "" and the write
        // lands in a file named '' (bat-sh-go t36_redirect_var cluster).
        let prog = prog_of(vec![
            assign("f", IrExpr::Str("out.txt".to_string(), StrStyle::DoubleQuoted)),
            IrStmt::Redirect {
                inner: vec![IrStmt::Expr(IrExpr::Call {
                    func: "exec".to_string(),
                    args: vec![
                        IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                        IrExpr::Array(vec![IrExpr::Str("hi".to_string(), StrStyle::DoubleQuoted)]),
                    ],
                })],
                redirects: vec![crate::ir::IrRedirect {
                    fd: Some(1),
                    mode: "w".to_string(),
                    target: IrExpr::Call {
                        func: "getVar".to_string(),
                        args: vec![IrExpr::Str("f".to_string(), StrStyle::DoubleQuoted)],
                    },
                    interpolate: true,
                }],
            },
        ]);
        let mut prog = prog;
        assert!(!dead_store_elim(&mut prog), "redirect-target getVar read must keep the f store");
        assert_eq!(prog.stmts.len(), 2);
    }

    #[test]
    fn const_prop_folds_straight_line_arith_read() {
        // x=5; y=$(( x + 1 )); echo $y — the read of x is a NUMERIC
        // context (arith), the honesty rule allows the const fold; the
        // def drops (all reads folded).
        let prog = prog_of(vec![
            assign("x", IrExpr::Int(5)),
            assign(
                "y",
                IrExpr::Arith(Box::new(ArithAst::Bin {
                    op: "+".to_string(),
                    lhs: Box::new(ArithAst::Var("x".to_string())),
                    rhs: Box::new(ArithAst::Num(1)),
                })),
            ),
            IrStmt::Expr(IrExpr::Call {
                func: "builtin".to_string(),
                args: vec![
                    IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Array(vec![IrExpr::Var("y".to_string(), None)]),
                ],
            }),
        ]);
        let mut prog = prog;
        let changed = const_prop(&mut prog);
        assert!(changed, "x=5 read in arith should fold and drop the def");
        // x's def is gone and y's arith reads the literal
        if let IrStmt::Assign { expr, .. } = &prog.stmts[0] {
            if let IrExpr::Arith(ast) = expr {
                assert!(matches!(ast.as_ref(), ArithAst::Bin { lhs, .. } if matches!(lhs.as_ref(), ArithAst::Num(5))));
            } else {
                panic!("expected arith rhs");
            }
        } else {
            panic!("expected y assign");
        }
    }
}