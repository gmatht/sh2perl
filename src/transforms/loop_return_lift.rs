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
    let flag = fresh_flag(&names);
    block_pass(stmts, &flag)
}

/// Rewrite a statement block: lift the first loop with the echo+return
/// pattern, moving the post-loop statements into the new if's else.
fn block_pass(stmts: &mut Vec<IrStmt>, flag: &str) -> bool {
    let mut changed = false;
    let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
    let mut i = 0;
    while i < stmts.len() {
        if let Some((loop_replacement, echo_value)) = try_lift_loop(&stmts[i], flag) {
            // the post-loop statements run only when the loop completes
            // without the break (the flag is 0) — the original
            // fall-through semantics
            let post: Vec<IrStmt> = stmts[i + 1..].to_vec();
            out.push(declare_flag(flag));
            out.push(loop_replacement);
            out.push(if_flag_echo(flag, echo_value, post));
            changed = true;
            break;
        }
        let mut s = stmts[i].clone();
        changed |= stmt_pass(&mut s, flag);
        out.push(s);
        i += 1;
    }
    *stmts = out;
    changed
}

/// Recurse into nested containers (the loop's own body, ifs, blocks).
fn stmt_pass(st: &mut IrStmt, flag: &str) -> bool {
    match st {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut c = false;
            c |= block_pass(then, flag);
            for (ec, eb) in elsifs.iter_mut() {
                c |= block_pass(eb, flag);
            }
            c |= block_pass(else_, flag);
            let _ = cond;
            c
        }
        IrStmt::While { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body) => block_pass(body, flag),
        IrStmt::ForInit { body, .. } => block_pass(body, flag),
        IrStmt::Redirect { inner, .. } => block_pass(inner, flag),
        IrStmt::Function { body, .. } => block_pass(body, flag),
        _ => false,
    }
}

/// If the statement is a loop whose body has the echo+return If, return
/// the rewritten loop and the echo value.
fn try_lift_loop(st: &IrStmt, flag: &str) -> Option<(IrStmt, IrExpr)> {
    match st {
        IrStmt::While { cond, body } => {
            let (new_body, v) = lift_loop_body(body, flag)?;
            Some((
                IrStmt::While {
                    cond: cond.clone(),
                    body: new_body,
                },
                v,
            ))
        }
        IrStmt::For { var, iter, body } => {
            let (new_body, v) = lift_loop_body(body, flag)?;
            Some((
                IrStmt::For {
                    var: var.clone(),
                    iter: iter.clone(),
                    body: new_body,
                },
                v,
            ))
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            let (new_body, v) = lift_loop_body(body, flag)?;
            Some((
                IrStmt::ForInit {
                    init: init.clone(),
                    cond: cond.clone(),
                    step: step.clone(),
                    body: new_body,
                },
                v,
            ))
        }
        _ => None,
    }
}

/// Rewrite the loop body: the echo+return If's then arm becomes
/// `flag=1; break`. Returns the new body and the echo value.
fn lift_loop_body(body: &[IrStmt], flag: &str) -> Option<(Vec<IrStmt>, IrExpr)> {
    let mut out: Vec<IrStmt> = Vec::with_capacity(body.len());
    let mut value: Option<IrExpr> = None;
    for st in body {
        if let IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } = st
        {
            if elsifs.is_empty() && else_.is_empty() {
                if let Some(v) = echo_then_return(then) {
                    // the then arm is [echo "$v", return] — rewrite to
                    // [flag=1, break]
                    value = Some(v);
                    out.push(IrStmt::If {
                        cond: cond.clone(),
                        then: vec![
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
                        else_: vec![],
                    });
                    continue;
                }
            }
        }
        out.push(st.clone());
    }
    value.map(|v| (out, v))
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

/// `if (( __sh2_found == 1 )); then echo "$v"; else <post>; fi`.
fn if_flag_echo(flag: &str, value: IrExpr, post: Vec<IrStmt>) -> IrStmt {
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
            IrExpr::Array(vec![value]),
        ],
    });
    IrStmt::If {
        cond,
        then: vec![echo],
        elsifs: vec![],
        else_: post,
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
fn fresh_flag(names: &HashSet<String>) -> String {
    let base = "__sh2_found";
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
}
