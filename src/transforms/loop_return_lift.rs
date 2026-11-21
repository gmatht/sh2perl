//! loop-return-lift — rewrite the "echo a value and return INSIDE a loop"
//! shape to a flag+break form, so the echo-return transform can lift the
//! function (CROSS_BACKEND_RUNTIME.md §8.3 follow-up, item 1).
//!
//! ## Need
//! strContainsAny / line_at echo their result and `return` from INSIDE a
//! loop body. The echo-return transform refuses returns inside loop
//! bodies: the runtime-loop callback lowers a return to the `sh2.return`
//! SIGNAL, whose value channel is lost. Moving the echo out of the loop
//! (a flag + break, then a post-loop conditional echo) makes the function
//! echo-return-eligible.
//!
//! ## The rewrite
//! ```text
//! [loop with: if C; then echo "$v"; return; fi]  [post-loop stmts]
//! →
//! [local __sh2_found=0]
//! [loop with: if C; then __sh2_found=1; break; fi]
//! [if (( __sh2_found == 1 )); then echo "$v"; else post-loop stmts; fi]
//! ```
//! The post-loop statements run exactly when the loop completes without
//! the break (the flag is 0) — the original fall-through semantics. The
//! echo value `$v` is evaluated after the loop (its vars are
//! function-scoped — the last iteration's values, exactly what the
//! original `echo` saw before the `return`).
//!
//! ## Scope — the sound rule
//! Fires on a loop (While/For/ForInit) whose body contains an If with:
//!   - a pure cond,
//!   - a then arm EXACTLY `[echo "$v", return]` (single-arg value echo,
//!     pure value, then a bare return),
//!   - an empty else.
//! One site per loop (the first); anything else (multiple echo+return
//! sites, a non-empty else, an impure value) is left untouched —
//! refuse > guess.
//!
//! ## Placement
//! Registered in `transforms.rs` BEFORE `echo-return` (the lifted shape
//! must be visible to its recognition). The flag name is fresh against
//! every variable in the program.

use crate::ir::{InterpPart, IrExpr, IrStmt, StrStyle};
use std::collections::HashSet;

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut names: HashSet<String> = HashSet::new();
    collect_names(stmts, &mut names);
    let flag = fresh_name(&names, "__sh2_found");
    let ret = fresh_name(&names, "__sh2_ret");
    // The TOP LEVEL never lifts: `local` is not valid outside a function and
    // `return` outside one does not end the script, so the break+flag encoding
    // would neither compile nor preserve the control flow.
    block_pass(stmts, &flag, &ret, false, false)
}

/// Rewrite a statement block: lift the first loop with the echo+return
/// pattern, moving the post-loop statements into the new if's else.
///
/// `in_fn` — inside a function body (`local` is valid there, and `return`
/// really exits the function).
/// `tail` — running past the END of this block leaves the function: every
/// enclosing container is the last statement of its own block up to the
/// function body. It is required because the encoding replaces a `return`
/// (leave the function) with a `break` (leave the loop); statements that the
/// original return skipped would otherwise run.
fn block_pass(stmts: &mut Vec<IrStmt>, flag: &str, ret: &str, in_fn: bool, tail: bool) -> bool {
    let mut changed = false;
    let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len() + 2);
    let mut i = 0;
    while i < stmts.len() {
        // the loop may be lifted only where a `break` and a `return` agree:
        // inside a function, and with nothing followed at function level
        if in_fn && tail {
            if let Some(loop_replacement) = try_lift_loop(&stmts[i], flag, ret) {
                // the post-loop statements run only when the loop completes
                // without the break (the flag is 0) — the original
                // fall-through semantics
                let post: Vec<IrStmt> = stmts[i + 1..].to_vec();
                out.push(declare_flag(flag));
                out.push(declare_ret(ret));
                out.push(loop_replacement);
                out.push(if_flag_echo(flag, ret, post));
                changed = true;
                break;
            }
        }
        let mut s = stmts[i].clone();
        // a statement is in tail position only as the LAST one of a tail block
        let child_tail = tail && i + 1 == stmts.len();
        changed |= stmt_pass(&mut s, flag, ret, in_fn, child_tail);
        out.push(s);
        i += 1;
    }
    *stmts = out;
    changed
}

/// Recurse into nested containers (the loop's own body, ifs, blocks).
fn stmt_pass(st: &mut IrStmt, flag: &str, ret: &str, in_fn: bool, tail: bool) -> bool {
    match st {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut c = false;
            c |= block_pass(then, flag, ret, in_fn, tail);
            for (_ec, eb) in elsifs.iter_mut() {
                c |= block_pass(eb, flag, ret, in_fn, tail);
            }
            c |= block_pass(else_, flag, ret, in_fn, tail);
            let _ = cond;
            c
        }
        // A loop body is NEVER tail position: a `break` there exits only that
        // loop, so the enclosing loop keeps iterating — the original `return`
        // would have left the function.
        IrStmt::While { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::ForInit { body, .. } => block_pass(body, flag, ret, in_fn, false),
        // A subshell / background body runs in a child: a `return` there does
        // not leave the enclosing function, and the flag would not survive.
        IrStmt::Subshell(body) | IrStmt::Background(body) => {
            block_pass(body, flag, ret, in_fn, false)
        }
        IrStmt::Block(body) => block_pass(body, flag, ret, in_fn, tail),
        IrStmt::Redirect { inner, .. } => block_pass(inner, flag, ret, in_fn, tail),
        IrStmt::Function { body, .. } => block_pass(body, flag, ret, true, true),
        // the matchers' loops live inside `case` clause bodies
        IrStmt::Case { clauses, .. } => {
            let mut c = false;
            for clause in clauses.iter_mut() {
                c |= block_pass(&mut clause.body, flag, ret, in_fn, tail);
            }
            c
        }
        _ => false,
    }
}

/// If the statement is a loop whose body has the echo+return If, return
/// the rewritten loop. The echo value is CAPTURED in `ret` at the matching
/// iteration (see [`lift_stmt`]).
fn try_lift_loop(st: &IrStmt, flag: &str, ret: &str) -> Option<IrStmt> {
    match st {
        IrStmt::While { cond, body } => {
            let new_body = lift_loop_body(body, flag, ret)?;
            Some(IrStmt::While {
                cond: cond.clone(),
                body: new_body,
            })
        }
        IrStmt::For { var, iter, body } => {
            let new_body = lift_loop_body(body, flag, ret)?;
            Some(IrStmt::For {
                var: var.clone(),
                iter: iter.clone(),
                body: new_body,
            })
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            let new_body = lift_loop_body(body, flag, ret)?;
            Some(IrStmt::ForInit {
                init: init.clone(),
                cond: cond.clone(),
                step: step.clone(),
                body: new_body,
            })
        }
        _ => None,
    }
}

/// Rewrite the loop body: the echo+return If's then arm becomes
/// `ret="$v"; flag=1; break`. The echo+return may be NESTED anywhere in the
/// loop body (the matchers' `if [[ "$m" == "1" ]]; then echo 1; return; fi`
/// inside a larger if/case) — the rewrite replaces the echo+return with the
/// capture + flag + break, keeping the else arm (empty or SILENT — it runs
/// when C is false, inside the loop, unchanged).
fn lift_loop_body(body: &[IrStmt], flag: &str, ret: &str) -> Option<Vec<IrStmt>> {
    let mut out: Vec<IrStmt> = Vec::with_capacity(body.len());
    let mut found = false;
    for st in body {
        if let Some(replacement) = lift_stmt(st, flag, ret) {
            found = true;
            out.push(replacement);
            continue;
        }
        out.push(st.clone());
    }
    if found {
        Some(out)
    } else {
        None
    }
}

/// Recursively find the echo+return If in a statement; rewrite it to
/// `ret="$v"; flag=1; break` (the else arm stays).
///
/// The value is read AT THE ECHO POINT, exactly as the original `echo` read
/// it. Re-evaluating `$v` after the loop would read whatever the loop's own
/// machinery left behind (the matchers' `for f in ...` leaves the variable on
/// the element AFTER the match that broke out), i.e. the wrong iteration's
/// value.
fn lift_stmt(st: &IrStmt, flag: &str, ret: &str) -> Option<IrStmt> {
    match st {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            if elsifs.is_empty() {
                if let Some(v) = echo_then_return(then) {
                    if else_.iter().all(stmt_silent) {
                        return Some(IrStmt::If {
                            cond: cond.clone(),
                            then: vec![
                                IrStmt::Assign {
                                    targets: vec![crate::ir::AssignTarget {
                                        var: ret.to_string(),
                                        sigil: None,
                                        indices: vec![],
                                    }],
                                    expr: v,
                                    asm: None,
                                },
                                IrStmt::Assign {
                                    targets: vec![crate::ir::AssignTarget {
                                        var: flag.to_string(),
                                        sigil: None,
                                        indices: vec![],
                                    }],
                                    expr: IrExpr::Int(1),
                                    asm: None,
                                },
                                IrStmt::Break,
                            ],
                            elsifs: vec![],
                            else_: else_.clone(),
                        });
                    }
                }
            }
            // recurse into the arms
            let mut t: Vec<IrStmt> = Vec::with_capacity(then.len());
            for s in then {
                if let Some(r) = lift_stmt(s, flag, ret) {
                    t.push(r);
                    return Some(IrStmt::If {
                        cond: cond.clone(),
                        then: t,
                        elsifs: elsifs.clone(),
                        else_: else_.clone(),
                    });
                }
                t.push(s.clone());
            }
            let mut el: Vec<IrStmt> = Vec::with_capacity(else_.len());
            for s in else_ {
                if let Some(r) = lift_stmt(s, flag, ret) {
                    el.push(r);
                    return Some(IrStmt::If {
                        cond: cond.clone(),
                        then: then.clone(),
                        elsifs: elsifs.clone(),
                        else_: el,
                    });
                }
                el.push(s.clone());
            }
            None
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut new_clauses: Vec<crate::ir::IrCaseClause> = Vec::with_capacity(clauses.len());
            for clause in clauses {
                let mut b: Vec<IrStmt> = Vec::with_capacity(clause.body.len());
                for s in &clause.body {
                    if let Some(r) = lift_stmt(s, flag, ret) {
                        b.push(r);
                        new_clauses.push(crate::ir::IrCaseClause {
                            patterns: clause.patterns.clone(),
                            body: b,
                        });
                        return Some(IrStmt::Case {
                            discriminant: discriminant.clone(),
                            clauses: new_clauses,
                        });
                    }
                    b.push(s.clone());
                }
                new_clauses.push(crate::ir::IrCaseClause {
                    patterns: clause.patterns.clone(),
                    body: b,
                });
            }
            None
        }
        IrStmt::Block(body) => {
            let mut b: Vec<IrStmt> = Vec::with_capacity(body.len());
            for s in body {
                if let Some(r) = lift_stmt(s, flag, ret) {
                    b.push(r);
                    return Some(IrStmt::Block(b));
                }
                b.push(s.clone());
            }
            None
        }
        _ => None,
    }
}

/// Is the arm exactly `[echo "$v", return]`? Returns the echo value.
fn echo_then_return(arm: &[IrStmt]) -> Option<IrExpr> {
    let [IrStmt::Expr(IrExpr::Call { func, args, .. }), IrStmt::Return(None)] = arm else {
        return None;
    };
    if !matches!(func.as_str(), "builtin" | "exec") {
        return None;
    }
    let [IrExpr::Str(n, _), IrExpr::Array(words)] = args.as_slice() else {
        return None;
    };
    if n != "echo" {
        return None;
    }
    let [word] = words.as_slice() else {
        return None;
    };
    if !expr_pure(word) {
        return None;
    }
    Some(word.clone())
}

/// `local __sh2_found=0`.
fn declare_flag(flag: &str) -> IrStmt {
    IrStmt::Declare {
        vars: vec![crate::ir::Decl {
            name: flag.to_string(),
            sigil: None,
        }],
        init: Some(IrExpr::Str("0".to_string(), StrStyle::DoubleQuoted)),
        local: true,
    }
}

/// `local __sh2_ret` — NO initializer. The capture is only read on the
/// found path, after it has been assigned, so an unset local is exact; and the
/// renderers box an uninitialized pointer safely (a *literal* initializer in a
/// computed-width box is freed by the first `_sh_sset` in C, which aborts).
fn declare_ret(ret: &str) -> IrStmt {
    IrStmt::Declare {
        vars: vec![crate::ir::Decl {
            name: ret.to_string(),
            sigil: None,
        }],
        init: None,
        local: true,
    }
}

/// The QUOTED expansion of the captured variable, so the post-loop echo keeps
/// the original word's quoting (word splitting / globbing must not reappear).
fn ret_word(ret: &str) -> IrExpr {
    IrExpr::Interpolate(vec![InterpPart::Expr(Box::new(IrExpr::Var(
        ret.to_string(),
        None,
    )))])
}

/// `if (( __sh2_found == 1 )); then echo "$ret"; else <post>; fi`.
fn if_flag_echo(flag: &str, ret: &str, post: Vec<IrStmt>) -> IrStmt {
    let cond = IrExpr::Call {
        func: "builtin".to_string(),
        args: vec![
            IrExpr::Str("let".to_string(), StrStyle::DoubleQuoted),
            IrExpr::Array(vec![IrExpr::Str(
                format!("{flag} == 1"),
                StrStyle::DoubleQuoted,
            )]),
        ],
    };
    let echo = IrStmt::Expr(IrExpr::Call {
        func: "builtin".to_string(),
        args: vec![
            IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
            IrExpr::Array(vec![ret_word(ret)]),
        ],
    });
    IrStmt::If {
        cond,
        then: vec![echo],
        elsifs: vec![],
        else_: post,
    }
}

/// Is the statement silent (no stdout, no exec/capture/`$?`)? The
/// loop-return-lift's else arm must be silent — the echo+return moves
/// out of the loop, the else stays inside.
fn stmt_silent(st: &IrStmt) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => expr_pure(expr),
        IrStmt::Declare { init, .. } => init.as_ref().map(expr_pure).unwrap_or(true),
        IrStmt::DeclareArray { elements, .. } => elements.iter().all(expr_pure),
        IrStmt::Expr(IrExpr::Call { func, args, .. })
            if matches!(func.as_str(), "builtin" | "exec") =>
        {
            if let Some(IrExpr::Str(n, _)) = args.first() {
                if matches!(n.as_str(), "local" | "declare" | "typeset" | "readonly" | "shift") {
                    return args.iter().skip(1).all(expr_pure);
                }
            }
            false
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            expr_pure(cond)
                && then.iter().all(stmt_silent)
                && elsifs.iter().all(|(c, b)| expr_pure(c) && b.iter().all(stmt_silent))
                && else_.iter().all(stmt_silent)
        }
        IrStmt::Block(body) => body.iter().all(stmt_silent),
        _ => false,
    }
}

/// Is the expression free of side effects (no capture/exec/`$?`)?
fn expr_pure(e: &IrExpr) -> bool {
    match e {
        IrExpr::Capture { .. } | IrExpr::RawExpr(_) => false,
        IrExpr::Var(n, _) => n != "?",
        IrExpr::Str(s, _) => !s.contains("$?"),
        IrExpr::Call { func, args } => {
            if matches!(
                func.as_str(),
                "exec" | "builtin" | "pipeline" | "capture" | "captureSync" | "captureWords"
                    | "captureWordsSync" | "background" | "subshell" | "exit" | "fnCall"
            ) {
                return false;
            }
            if func == "getVar" && matches!(args.as_slice(), [IrExpr::Str(n, _)] if n == "?") {
                return false;
            }
            args.iter().all(expr_pure)
        }
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            InterpPart::Lit(s) => !s.contains("$?"),
            InterpPart::Expr(ie) => expr_pure(ie),
        }),
        IrExpr::BinOp { lhs, rhs, .. } => expr_pure(lhs) && expr_pure(rhs),
        IrExpr::Array(items) => items.iter().all(expr_pure),
        _ => true,
    }
}

/// Collect every variable name in the program (for the fresh flag).
fn collect_names(stmts: &[IrStmt], names: &mut HashSet<String>) {
    for st in stmts {
        stmt_names(st, names);
    }
}

fn stmt_names(st: &IrStmt, names: &mut HashSet<String>) {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                names.insert(t.var.clone());
            }
            expr_names(expr, names);
        }
        IrStmt::Declare { vars, init, .. } => {
            for v in vars {
                names.insert(v.name.clone());
            }
            if let Some(i) = init {
                expr_names(i, names);
            }
        }
        IrStmt::DeclareArray { var, elements, .. } => {
            names.insert(var.clone());
            for el in elements {
                expr_names(el, names);
            }
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            expr_names(cond, names);
            for s in then.iter().chain(else_) {
                stmt_names(s, names);
            }
            for (c, b) in elsifs {
                expr_names(c, names);
                for s in b {
                    stmt_names(s, names);
                }
            }
        }
        IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, .. } => {
            expr_names(cond, names);
            for s in body {
                stmt_names(s, names);
            }
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            for s in init.iter().chain(step.iter()) {
                stmt_names(s, names);
            }
            expr_names(cond, names);
            for s in body {
                stmt_names(s, names);
            }
        }
        IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => {
            for s in body {
                stmt_names(s, names);
            }
        }
        IrStmt::Redirect { inner, .. } => {
            for s in inner {
                stmt_names(s, names);
            }
        }
        IrStmt::Function { body, .. } => {
            for s in body {
                stmt_names(s, names);
            }
        }
        IrStmt::Expr(e) => expr_names(e, names),
        IrStmt::Output { value, .. } => expr_names(value, names),
        IrStmt::Return(Some(v)) => expr_names(v, names),
        _ => {}
    }
}

fn expr_names(e: &IrExpr, names: &mut HashSet<String>) {
    match e {
        IrExpr::Var(n, _) => {
            names.insert(n.clone());
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                expr_names(a, names);
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let InterpPart::Expr(ie) = p {
                    expr_names(ie, names);
                }
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_names(lhs, names);
            expr_names(rhs, names);
        }
        IrExpr::Array(items) => {
            for it in items {
                expr_names(it, names);
            }
        }
        IrExpr::Capture { expr, .. } => expr_names(expr, names),
        IrExpr::Arrow(stmts) => {
            for s in stmts {
                stmt_names(s, names);
            }
        }
        _ => {}
    }
}

/// A fresh flag name: `__sh2_found`, `__sh2_found2`, … not colliding with
/// any program variable.
fn fresh_name(names: &HashSet<String>, base: &str) -> String {
    if !names.contains(base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let cand = format!("{base}{n}");
        if !names.contains(&cand) {
            return cand;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::commands::parse_commands_from_text;
    use crate::shir::ast_to_ir_raw;
    use crate::shir_json::shir_to_shir_json;

    fn lower(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(transform(&mut prog.stmts), "transform was a no-op for {src}");
        shir_to_shir_json(&prog)
    }

    #[test]
    fn loop_echo_return_lifts_to_flag_break() {
        // strContainsAny's shape: echo+return inside a for loop, echo 0 after.
        let json = lower(
            "f() { local s=\"$1\" c=\"$2\"; local i; for ((i=0; i<3; i++)); do if [[ \"$s\" == *\"$c\"* ]]; then echo 1; return; fi; done; echo 0; }; f a b",
        );
        assert!(json.contains("\"Break\""), "missing break: {json}");
        assert!(json.contains("__sh2_found"), "missing flag: {json}");
        // the echo moved out of the loop (the post-loop if)
        assert!(json.contains("\"echo\""), "missing post-loop echo: {json}");
    }

    #[test]
    fn no_echo_return_loop_untouched() {
        // a plain loop (no echo+return) must be a NO-OP
        let commands = parse_commands_from_text(
            "f() { local i; for ((i=0; i<3; i++)); do echo $i; done; }; f",
        )
        .expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(!transform(&mut prog.stmts), "plain loop was rewritten");
        let json = shir_to_shir_json(&prog);
        assert!(!json.contains("__sh2_found"), "plain loop rewritten: {json}");
    }

    /// Parse and run the transform, returning the IR (no `local` outside a
    /// function: the transform must refuse, so callers check the boolean).
    fn ir_of(src: &str) -> (crate::ir::IrProgram, bool) {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        let changed = transform(&mut prog.stmts);
        (prog, changed)
    }

    /// The census requires a FUNCTION: `local` is not valid at top level and a
    /// top-level `return` does not end the script (bash errors and continues),
    /// so the break+flag encoding would change the control flow.
    #[test]
    fn a_loop_outside_a_function_is_not_lifted() {
        let (prog, changed) = ir_of(
            "for x in 1 2 3; do if [ \"$x\" = 2 ]; then echo \"$x\"; return; fi; done",
        );
        assert!(!changed, "top-level loop was lifted");
        let json = shir_to_shir_json(&prog);
        assert!(!json.contains("__sh2_found"), "top-level loop lifted: {json}");
    }

    /// The census requires TAIL position: a `return` leaves the whole function,
    /// so a loop nested in a container that is NOT last would, when lifted, let
    /// the statements after that container run (`break` leaves only the loop).
    /// (A loop that IS last in the function block is fine: its post statements
    /// move into the else arm and are skipped on the found path.)
    #[test]
    fn a_loop_that_is_not_in_tail_position_is_not_lifted() {
        let (prog, changed) = ir_of(
            "f() { if [ 1 = 1 ]; then for x in 1 2 3; do if [ \"$x\" = 2 ]; then echo \"$x\"; return; fi; done; fi; echo after; }; f",
        );
        assert!(!changed, "non-tail loop was lifted");
        let json = shir_to_shir_json(&prog);
        assert!(!json.contains("__sh2_found"), "non-tail loop lifted: {json}");
    }

    /// A loop body is never tail position: a `break` there exits only the INNER
    /// loop and the enclosing loop keeps iterating, while the original `return`
    /// left the function.
    #[test]
    fn a_loop_inside_a_loop_is_not_lifted() {
        let (prog, changed) = ir_of(
            "f() { for i in 1 2; do for x in a b; do if [ \"$x\" = b ]; then echo \"$x\"; return; fi; done; done; }; f",
        );
        assert!(!changed, "nested loop was lifted");
        let json = shir_to_shir_json(&prog);
        assert!(!json.contains("__sh2_found"), "nested loop lifted: {json}");
    }

    /// The positive case: a tail loop in a function IS lifted, to flag+break.
    #[test]
    fn a_tail_loop_in_a_function_is_lifted() {
        let (prog, changed) = ir_of(
            "f() { for x in 1 2 3; do if [ \"$x\" = 2 ]; then echo \"$x\"; return; fi; done; }; f",
        );
        assert!(changed, "tail loop in a function was not lifted");
        let json = shir_to_shir_json(&prog);
        assert!(json.contains("\"Break\""), "missing break: {json}");
        assert!(json.contains("__sh2_found"), "missing flag: {json}");
        assert!(json.contains("__sh2_ret"), "missing capture: {json}");
    }

    /// THE DEFECT PIN: the value is captured AT THE ECHO POINT. Re-evaluating
    /// `$v` after the loop reads the loop variable on the element AFTER the
    /// match that broke out (`found 3` where bash prints `found 2`).
    #[test]
    fn the_value_is_captured_at_the_matching_iteration() {
        let (prog, changed) = ir_of(
            "f() { for x in 1 2 3; do if [ \"$x\" = 2 ]; then echo \"$x\"; return; fi; done; }; f",
        );
        assert!(changed, "tail loop in a function was not lifted");
        let body = match &prog.stmts[0] {
            IrStmt::Function { body, .. } => body,
            other => panic!("expected the function definition, got {other:?}"),
        };
        assert_eq!(
            body.len(),
            4,
            "expected [flag, ret, loop, if], got {body:?}"
        );
        let IrStmt::For { body: loop_body, .. } = &body[2] else {
            panic!("the third statement is not the loop: {:?}", body[2]);
        };
        let IrStmt::If { then, .. } = &loop_body[0] else {
            panic!("the loop body is not the lifted if: {loop_body:?}");
        };
        // capture FIRST (at the echo point), then the flag, then the break
        match then.as_slice() {
            [IrStmt::Assign { targets, expr, .. }, IrStmt::Assign { targets: flag_targets, expr: IrExpr::Int(1), .. }, IrStmt::Break] => {
                assert_eq!(targets[0].var, "__sh2_ret", "first write is not the capture");
                // the capture reads exactly the echoed word (`getVar("x")` on
                // this frontend), and nothing else
                let reads_x = match expr {
                    IrExpr::Var(n, _) => n == "x",
                    IrExpr::Call { func, args } => {
                        func == "getVar"
                            && matches!(args.as_slice(), [IrExpr::Str(n, _)] if n == "x")
                    }
                    _ => false,
                };
                assert!(reads_x, "the capture does not read the echoed word: {expr:?}");
                assert!(expr_pure(expr), "the capture is impure: {expr:?}");
                assert_eq!(flag_targets[0].var, "__sh2_found");
            }
            other => panic!("unexpected lifted arm: {other:?}"),
        }
        // the post-loop echo reads the CAPTURE, never the loop variable again
        let IrStmt::If { then, .. } = &body[3] else {
            panic!("no post-loop if: {:?}", body[3]);
        };
        let IrStmt::Expr(IrExpr::Call { args, .. }) = &then[0] else {
            panic!("no post-loop echo: {:?}", then[0]);
        };
        let IrExpr::Array(words) = &args[1] else {
            panic!("no echo words: {:?}", args[1]);
        };
        let IrExpr::Interpolate(parts) = &words[0] else {
            panic!("no quoted echo word: {:?}", words[0]);
        };
        match parts.as_slice() {
            [InterpPart::Expr(e)] => assert!(
                matches!(&**e, IrExpr::Var(n, _) if n == "__sh2_ret"),
                "post-loop echo re-evaluates {e:?} instead of the capture"
            ),
            other => panic!("unexpected echo word: {other:?}"),
        }
    }
}
