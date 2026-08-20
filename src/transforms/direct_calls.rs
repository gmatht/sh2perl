//! direct-calls — resolve command substitutions of shell functions to
//! direct in-process calls, the shIR equivalent of sh2runtime's estree
//! pass `directShellFnCalls` (JS-only today: the measured hot cost in
//! the texture generators' per-pixel helpers).
//!
//! ## Need
//! `v=$(sq 3)` (command substitution of a defined shell function)
//! arrives in the IR as
//! `Assign { targets: [v], expr: Capture { expr: Arrow([Expr(Call {
//! func: "sq", args })]), native: false } }`. The C backend TODOs this
//! shape (`/* TODO(unsupported): expr Capture { expr: Arrow([...]) } */`)
//! — it has no way to run a function in-process and capture its output,
//! so the whole construct is left unsupported. The JS backend solved it
//! with `directShellFnCalls` (resolve the exec → direct call) — but only
//! for JS, and only on the estree.
//!
//! This transform makes the resolution at the IR level: when the capture
//! wraps a DIRECT call of a defined `IrStmt::Function` whose body is a
//! PURE-OUTPUT function (only `Output` statements and pure arithmetic —
//! no exec, no writes, no side effects), it rewrites the capture's inner
//! `Arrow([Expr(Call …)])` to a bare `Call { func, args }`. The
//! `Capture { expr: Call }` shape (as opposed to `Capture { expr: Arrow }`)
//! is the established "capture a SUBPROCESS" vs "capture a FUNCTION" split:
//! backends emit `v = capOutput(sq(args))` (in-process, no fork) instead
//! of the subprocess form.
//!
//! ## Scope
//! Only captures whose arrow is EXACTLY one `Expr(Call{func, args})`
//! where:
//!   - `func` names a defined `IrStmt::Function` in the same program,
//!   - that function's body is pure-output (Output + Assign/Declare
//!     with pure exprs only — no Exec/Pipeline/WriteFile/Capture/
//!     Background/Subshell/Return/Exit),
//!   - the function is never redefined after this capture site (a
//!     second `Function { name }` definition later = refuse).
//! Anything else is left untouched — refuse > guess.
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). The estree
//! worker mediates the renderer arms: estree.rs, cfront.rs and the
//! C/Go/Perl generators each add a `Capture { expr: Call }` arm (run the
//! function with a fresh output buffer, return the concatenated lines).

use crate::ir::{IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // collect the defined function names + their bodies
    let mut fns: Vec<(String, Vec<IrStmt>)> = Vec::new();
    for st in stmts.iter() {
        if let IrStmt::Function { name, body, .. } = st {
            fns.push((name.clone(), body.clone()));
        }
    }
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= transform_stmt(st, &fns);
    }
    c
}

fn transform_stmt(st: &mut IrStmt, fns: &[(String, Vec<IrStmt>)]) -> bool {
    let mut c = false;
    match st {
        IrStmt::Assign { expr, .. } => c |= transform_expr(expr, fns),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            c |= transform_expr(cond, fns);
            for s in then.iter_mut() {
                c |= transform_stmt(s, fns);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= transform_expr(ec, fns);
                for s in eb.iter_mut() {
                    c |= transform_stmt(s, fns);
                }
            }
            for s in else_.iter_mut() {
                c |= transform_stmt(s, fns);
            }
        }
        IrStmt::For { iter, body, .. } => {
            c |= transform_expr(iter, fns);
            for s in body.iter_mut() {
                c |= transform_stmt(s, fns);
            }
        }
        IrStmt::While { cond, body } => {
            c |= transform_expr(cond, fns);
            for s in body.iter_mut() {
                c |= transform_stmt(s, fns);
            }
        }
        IrStmt::DoWhile { body, cond, .. } => {
            for s in body.iter_mut() {
                c |= transform_stmt(s, fns);
            }
            c |= transform_expr(cond, fns);
        }
        IrStmt::Return(Some(e)) => c |= transform_expr(e, fns),
        IrStmt::Exit(Some(e)) => c |= transform_expr(e, fns),
        IrStmt::Function { body, .. } => {
            // a defined function's body may itself contain captures of
            // OTHER pure functions — recurse (its own name stays out)
            for s in body.iter_mut() {
                c |= transform_stmt(s, fns);
            }
        }
        IrStmt::Subshell(v) | IrStmt::Background(v) | IrStmt::Block(v) | IrStmt::Redirect { inner: v, .. } => {
            for s in v.iter_mut() {
                c |= transform_stmt(s, fns);
            }
        }
        IrStmt::Expr(e) => c |= transform_expr(e, fns),
        _ => {}
    }
    c
}

fn transform_expr(e: &mut IrExpr, fns: &[(String, Vec<IrStmt>)]) -> bool {
    let mut c = false;
    match e {
        // the target shape: v=$(sq 3)
        IrExpr::Capture { expr, .. } => {
            // clone the call OUT first — assigning `*expr` below would
            // invalidate a borrow of the old tree
            let direct = match extract_call(&**expr).cloned() {
                Some(call) if is_defined_pure_function(&call, fns) => Some(call),
                _ => None,
            };
            if let Some(call) = direct {
                *expr = Box::new(call);
                c = true;
            } else {
                c |= transform_expr(expr, fns);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            c |= transform_expr(lhs, fns);
            c |= transform_expr(rhs, fns);
        }
        IrExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                c |= transform_expr(a, fns);
            }
        }
        IrExpr::Index { key, .. } => c |= transform_expr(key, fns),
        IrExpr::Ternary { cond, then, else_ } => {
            c |= transform_expr(cond, fns);
            c |= transform_expr(then, fns);
            c |= transform_expr(else_, fns);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let crate::ir::InterpPart::Expr(x) = p {
                    c |= transform_expr(x, fns);
                }
            }
        }
        IrExpr::Arrow(body) => {
            for s in body.iter_mut() {
                c |= transform_stmt(s, fns);
            }
        }
        _ => {}
    }
    c
}

/// Is `call` a `Call { func }` of a DEFINED pure-output function?
fn is_defined_pure_function(call: &IrExpr, fns: &[(String, Vec<IrStmt>)]) -> bool {
    let func = match call {
        IrExpr::Call { func, .. } => func,
        _ => return false,
    };
    // a defined function with that name, pure-output body
    let body = match fns.iter().find(|(n, _)| n == func) {
        Some((_, b)) => b,
        None => return false,
    };
    body.iter().all(pure_output_stmt)
}

/// `Arrow([Expr(Call { func, args })])` → the Call, if that is exactly
/// the arrow's shape.
fn extract_call(expr: &IrExpr) -> Option<&IrExpr> {
    match expr {
        IrExpr::Arrow(body) => match body.as_slice() {
            [IrStmt::Expr(c)] => Some(c),
            _ => None,
        },
        _ => None,
    }
}

/// A pure-output statement: Output of a pure expr, or an Assign/Declare
/// whose value is pure arithmetic/strings (no exec/spawn/capture).
fn pure_output_stmt(st: &IrStmt) -> bool {
    match st {
        IrStmt::Output { value, .. } => pure_expr(value),
        IrStmt::Assign { expr, .. } => pure_expr(expr),
        IrStmt::Declare { init: Some(i), .. } => pure_expr(i),
        IrStmt::Declare { init: None, .. } => true,
        IrStmt::Expr(e) => pure_expr(e),
        _ => false,
    }
}

fn pure_expr(e: &IrExpr) -> bool {
    match e {
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Var(_, _) | IrExpr::Range { .. } => true,
        IrExpr::BinOp { lhs, rhs, .. } => pure_expr(lhs) && pure_expr(rhs),
        IrExpr::Arith(a) => {
            // arithmetic is pure unless it hides an index read of an
            // external array — conservative: accept plain var/num trees
            arith_pure(a)
        }
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            crate::ir::InterpPart::Lit(_) => true,
            crate::ir::InterpPart::Expr(x) => pure_expr(x),
        }),
        _ => false,
    }
}

fn arith_pure(a: &ArithAstRef) -> bool {
    match a {
        ArithAstRef::Num(_) | ArithAstRef::Var(_) => true,
        ArithAstRef::Bin { lhs, rhs, .. } => arith_pure(lhs) && arith_pure(rhs),
        ArithAstRef::Un { arg, .. } => arith_pure(arg),
        ArithAstRef::Cond { test, then, else_ } => {
            arith_pure(test) && arith_pure(then) && arith_pure(else_)
        }
        ArithAstRef::Index { .. } => false, // an array read could be a store read
        _ => false,
    }
}

// small alias so the arith walker above reads cleanly
type ArithAstRef = crate::ir::ArithAst;

// name: direct-calls
// prereqs: [function_purity — the pure-output body test is the weak
//   form of that analysis; full purity lands when function_purity does]
// invariant: `Capture { expr: Arrow([Expr(Call…)]) }` of a DEFINED
//   pure-output function becomes `Capture { expr: Call }`; every other
//   capture and every other statement is untouched.
// scope: offered to c (closes the `v=$(sq 3)` TODO), go, perl, sh,
//   estree (the estree owner — `directShellFnCalls` is superseded once
//   this lands)
// updates: none (first offer)
