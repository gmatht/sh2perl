//! dead-fn-elim — remove shell functions that are never referenced.
//!
//! A `Function` definition is a no-op in bash unless it is called. Every
//! backend still renders the definition (and its body) as emitted code, so
//! an uncalled function is pure dead weight: it bloats the output and can
//! trip a renderer's unhandled-arm on a body shape that never runs.
//!
//! This is a GENERIC transform — backend-agnostic (all renderers consume
//! the same shIR), and safe: removing a function whose name never appears
//! anywhere in the program cannot change behavior.
//!
//! ## Safety / refusal
//! A function is removed ONLY when its name does not occur as a string
//! literal anywhere in the IR outside its own definition. That means no
//! `exec`/`fnCall` targets it by name, and no variable/trap/eval/`$@`
//! code path can produce its name at runtime (a name can only reach those
//! via a literal string in the program — e.g. `cmd="foo"; $cmd`).
//! Anything else — a name appearing in any string literal, however
//! indirect — keeps the function (refuse > guess).
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated), so it runs in
//! `ast_to_ir` for every backend.

use crate::ir::{InterpPart, IrExpr, IrStmt};
use std::collections::HashSet;

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // `KEEP_VARIABLES` (env) — debug/introspection escape hatch: when set,
    // keep every defined function (and so every shell variable it could
    // reference), even the never-called ones. Mirrors the `SH2_ASSUME_*`
    // env-flag convention; used e.g. when a program introspects its own
    // definitions (`declare -f`/`typeset -f`/`type name`).
    if std::env::var("KEEP_VARIABLES")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
    {
        return false;
    }

    // Pass 1: collect every defined function name.
    let mut defined: HashSet<String> = HashSet::new();
    collect_function_names(stmts, &mut defined);
    if defined.is_empty() {
        return false;
    }

    // Pass 2: collect every string literal in the program (excluding the
    // function names themselves — a def name is a String field, not a Str
    // literal, and must not count as a self-reference).
    let mut strs: HashSet<String> = HashSet::new();
    collect_str_literals(stmts, &mut strs);

    // Pass 3: retain a function iff its name occurs somewhere as a literal
    // (i.e. it is referenced by name — directly or via any string path).
    // Collect the dead names first (can't mutate while borrowing).
    let dead: Vec<String> = defined
        .iter()
        .filter(|n| !strs.contains(*n))
        .cloned()
        .collect();
    if dead.is_empty() {
        return false;
    }
    let dead_set: HashSet<&str> = dead.iter().map(|s| s.as_str()).collect();

    // Pass 4: remove the dead function definitions (and prune the same
    // names out of nested scopes / subshells / named blocks).
    let before = stmts.len();
    stmts.retain(|st| !is_dead_function(st, &dead_set));
    let changed = stmts.len() != before;
    prune(stmts, &dead_set);
    changed
}

fn is_dead_function(st: &IrStmt, dead: &HashSet<&str>) -> bool {
    if let IrStmt::Function { name, .. } = st {
        return dead.contains(name.as_str());
    }
    false
}

/// Collect the names of all defined functions (recursively — functions may
/// be defined inside bodies).
fn collect_function_names(stmts: &[IrStmt], out: &mut HashSet<String>) {
    for st in stmts {
        match st {
            IrStmt::Function { name, body, named_blocks } => {
                out.insert(name.clone());
                collect_function_names(body, out);
                for (_, nb) in named_blocks {
                    collect_function_names(nb, out);
                }
            }
            _ => {
                for b in stmts_bodies(st) {
                    collect_function_names(b, out);
                }
            }
        }
    }
}

/// Collect every `IrExpr::Str` literal value reachable from `stmts`.
fn collect_str_literals(stmts: &[IrStmt], out: &mut HashSet<String>) {
    for st in stmts {
        match st {
            IrStmt::Function { body, named_blocks, .. } => {
                collect_str_literals(body, out);
                for (_, nb) in named_blocks {
                    collect_str_literals(nb, out);
                }
            }
            _ => {
                for b in stmts_bodies(st) {
                    collect_str_literals(b, out);
                }
                collect_stmt_strs(st, out);
            }
        }
    }
}

/// Recursively remove dead function definitions from nested scopes too
/// (so a dead nested function doesn't survive a recursion we'd otherwise
/// skip in pass 4, which only prunes the top level).
fn prune(stmts: &mut Vec<IrStmt>, dead: &HashSet<&str>) {
    for st in stmts.iter_mut() {
        match st {
            IrStmt::Function { body, named_blocks, .. } => {
                body.retain(|s| !is_dead_function(s, dead));
                for (_, nb) in named_blocks.iter_mut() {
                    nb.retain(|s| !is_dead_function(s, dead));
                }
                prune(body, dead);
                for (_, nb) in named_blocks.iter_mut() {
                    prune(nb, dead);
                }
            }
            _ => {
                for b in stmts_bodies_mut(st) {
                    b.retain(|s| !is_dead_function(s, dead));
                    prune(b, dead);
                }
            }
        }
    }
}

fn stmts_bodies(st: &IrStmt) -> Vec<&Vec<IrStmt>> {
    let mut v = Vec::new();
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            v.push(then);
            for (_, b) in elsifs {
                v.push(b);
            }
            v.push(else_);
        }
        IrStmt::For { body, .. } | IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            v.push(body)
        }
        IrStmt::Case { clauses, .. } => {
            for c in clauses {
                v.push(&c.body);
            }
        }
        IrStmt::Subshell(b) | IrStmt::Block(b) | IrStmt::Background(b) | IrStmt::Redirect { inner: b, .. } => {
            v.push(b);
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            v.push(body);
            for e in excepts {
                v.push(&e.body);
            }
            v.push(else_body);
            v.push(finally_body);
        }
        _ => {}
    }
    v
}

fn stmts_bodies_mut(st: &mut IrStmt) -> Vec<&mut Vec<IrStmt>> {
    let mut v = Vec::new();
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            v.push(then);
            for (_, b) in elsifs {
                v.push(b);
            }
            v.push(else_);
        }
        IrStmt::For { body, .. } | IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => {
            v.push(body)
        }
        IrStmt::Case { clauses, .. } => {
            for c in clauses {
                v.push(&mut c.body);
            }
        }
        IrStmt::Subshell(b) | IrStmt::Block(b) | IrStmt::Background(b) | IrStmt::Redirect { inner: b, .. } => {
            v.push(b);
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            v.push(body);
            for e in excepts {
                v.push(&mut e.body);
            }
            v.push(else_body);
            v.push(finally_body);
        }
        _ => {}
    }
    v
}

/// Collect string literals that appear directly in a statement's own
/// expressions (not its nested statement bodies — those are handled by the
/// caller's recursion).
fn collect_stmt_strs(st: &IrStmt, out: &mut HashSet<String>) {
    match st {
        IrStmt::Output { value, .. } => collect_expr_strs(value, out),
        IrStmt::Expr(e) => collect_expr_strs(e, out),
        IrStmt::Assign { expr, targets, .. } => {
            collect_expr_strs(expr, out);
            for t in targets {
                collect_str_from_str(&t.var, out);
            }
        }
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                collect_expr_strs(i, out);
            }
        }
        IrStmt::DeclareArray { elements, .. } => {
            for e in elements {
                collect_expr_strs(e, out);
            }
        }
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => collect_expr_strs(e, out),
        IrStmt::If { cond, .. } => collect_expr_strs(cond, out),
        IrStmt::For { iter, .. } => collect_expr_strs(iter, out),
        IrStmt::While { cond, .. } | IrStmt::DoWhile { cond, .. } => collect_expr_strs(cond, out),
        IrStmt::Case { discriminant, .. } => collect_expr_strs(discriminant, out),
        IrStmt::WriteFile { content, path, .. } => {
            collect_expr_strs(content, out);
            collect_expr_strs(path, out);
        }
        IrStmt::Exec { cmd, args, .. } => {
            collect_expr_strs(cmd, out);
            for a in args {
                collect_expr_strs(a, out);
            }
        }
        _ => {}
    }
}

fn collect_expr_strs(e: &IrExpr, out: &mut HashSet<String>) {
    match e {
        IrExpr::Str(s, _) => {
            out.insert(s.clone());
        }
        IrExpr::Index { var, key } => {
            collect_str_from_str(var, out);
            collect_expr_strs(key, out);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_expr_strs(lhs, out);
            collect_expr_strs(rhs, out);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            collect_expr_strs(cond, out);
            collect_expr_strs(then, out);
            collect_expr_strs(else_, out);
        }
        IrExpr::DefinedOr { expr, default } => {
            collect_expr_strs(expr, out);
            collect_expr_strs(default, out);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    collect_expr_strs(x, out);
                }
            }
        }
        IrExpr::Capture { expr, .. } => collect_expr_strs(expr, out),
        IrExpr::Arrow(stmts) => {
            collect_stmt_list_strs(stmts, out);
        }
        IrExpr::Array(items) => {
            for i in items {
                collect_expr_strs(i, out);
            }
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            collect_expr_strs(iter, out);
            collect_expr_strs(elem, out);
            if let Some(c) = cond {
                collect_expr_strs(c, out);
            }
        }
        IrExpr::Object(pairs) => {
            for (_, v) in pairs {
                collect_expr_strs(v, out);
            }
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                collect_expr_strs(a, out);
            }
        }
        _ => {}
    }
}

fn collect_stmt_list_strs(stmts: &[IrStmt], out: &mut HashSet<String>) {
    for st in stmts {
        collect_stmt_strs(st, out);
        for b in stmts_bodies(st) {
            collect_stmt_list_strs(b, out);
        }
    }
}

fn collect_str_from_str(s: &str, out: &mut HashSet<String>) {
    out.insert(s.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    fn fn_def(name: &str, body: Vec<IrStmt>) -> IrStmt {
        IrStmt::Function { name: name.to_string(), body, named_blocks: vec![] }
    }
    fn exec_call(name: &str) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![IrExpr::Str(name.to_string(), StrStyle::DoubleQuoted)],
        })
    }
    fn assign_str(name: &str) -> IrStmt {
        // `cmd="name"` — a string literal holding a function name (indirect ref)
        IrStmt::Assign {
            targets: vec![crate::ir::AssignTarget { var: "cmd".into(), sigil: None, indices: vec![] }],
            expr: IrExpr::Str(name.to_string(), StrStyle::DoubleQuoted),
            asm: None,
        }
    }

    #[test]
    fn removes_uncalled_fn_keeps_called_and_indirect() {
        // foo() called directly -> kept; bar() never referenced -> removed;
        // baz() referenced only via a variable string -> kept.
        let mut stmts = vec![
            fn_def("foo", vec![exec_call("echo")]),
            fn_def("bar", vec![exec_call("echo")]),
            fn_def("baz", vec![exec_call("echo")]),
            exec_call("foo"),
            assign_str("baz"),
        ];
        assert!(transform(&mut stmts));
        let names: Vec<&str> = stmts
            .iter()
            .filter_map(|s| if let IrStmt::Function { name, .. } = s { Some(name.as_str()) } else { None })
            .collect();
        assert!(names.contains(&"foo"), "called fn kept");
        assert!(names.contains(&"baz"), "indirectly-referenced fn kept");
        assert!(!names.contains(&"bar"), "uncalled fn removed");
    }

    #[test]
    fn no_dead_fns_is_noop() {
        let mut stmts = vec![fn_def("f", vec![]), exec_call("f")];
        assert!(!transform(&mut stmts));
    }
}
