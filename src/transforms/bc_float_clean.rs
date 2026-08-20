//! bc-float-clean — strip the redundant `+ 0.0` that shader authors
//! append to FLOAT bc captures (`echo "scale=K; … + 0.0" | bc`).
//!
//! ## Need
//! The bash-authored GLSL shaders (examples/mimecroft-*.sh) force the
//! GLSL float type by appending `+ 0.0` to every
//! `echo "scale=K; EXPR" | bc` capture. The sh→GLSL backend's
//! `bc_float_expr` emits the expression VERBATIM into the per-vertex
//! shader, so every assignment carries a literal `+ (0.0)` term (and
//! the precedence parser wraps it in extra parens). Stripping the
//! identity at the IR level cleans the generated GLSL; the JS/Perl
//! runtime captures (`qx{echo … | bc}`) are behaviourally identical
//! (`X + 0.0 == X` for the finite values involved).
//!
//! ## Scope
//! Only `echo … | bc` pipelines where:
//!   - the arg's LAST literal ends with `+ 0.0` (or `+0.0`), AND
//!   - the remainder of the expression already contains a decimal point
//!     (the backend's float verdict depends on a decimal literal; when
//!     the `+ 0.0` is the ONLY decimal, the capture is left alone).
//! Anything else is left untouched — refuse > guess.
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated, like the
//! rest of the registry). The sh2glsl CLI and the otranspiler wasm
//! render shaders through `ast_to_ir_raw` — they must use `ast_to_ir`
//! for the shared transform pipeline to run on shaders.

use crate::ir::{InterpPart, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for s in stmts.iter_mut() {
        c |= transform_stmt(s);
    }
    c
}

fn transform_stmt(st: &mut IrStmt) -> bool {
    let c = match st {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut x = transform_expr(cond);
            x |= transform(then);
            for (ec, eb) in elsifs.iter_mut() {
                x |= transform_expr(ec);
                x |= transform(eb);
            }
            x |= transform(else_);
            x
        }
        IrStmt::For { iter, body, .. } => {
            let mut x = transform_expr(iter);
            x |= transform(body);
            x
        }
        IrStmt::While { cond, body, .. } => {
            let mut x = transform(body);
            x |= transform_expr(cond);
            x
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut x = transform(body);
            x |= transform_expr(cond);
            x
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut x = transform_expr(discriminant);
            for cl in clauses.iter_mut() {
                x |= transform(&mut cl.body);
            }
            x
        }
        IrStmt::Block(b)
        | IrStmt::Subshell(b)
        | IrStmt::Background(b)
        | IrStmt::Function { body: b, .. } => transform(b),
        IrStmt::Pipeline { stages, .. } => {
            let mut x = false;
            for stg in stages.iter_mut() {
                x |= transform(stg);
            }
            x
        }
        IrStmt::Expr(e) => transform_expr(e),
        IrStmt::Assign { expr, .. }
        | IrStmt::Output { value: expr, .. }
        | IrStmt::Declare {
            init: Some(expr), ..
        } => transform_expr(expr),
        IrStmt::WriteFile { path, content, .. } => {
            transform_expr(path) | transform_expr(content)
        }
        IrStmt::Redirect { inner, .. } => transform(inner),
        IrStmt::Exec { cmd, args, env, .. } => {
            let mut x = transform_expr(cmd);
            for a in args.iter_mut() {
                x |= transform_expr(a);
            }
            for (_, v) in env.iter_mut() {
                x |= transform_expr(v);
            }
            x
        }
        _ => false,
    };
    c
}

fn transform_expr(e: &mut IrExpr) -> bool {
    let mut c = false;
    match e {
        IrExpr::Capture { expr, .. } => {
            c |= transform_expr(expr);
            c |= clean_pipeline(expr);
        }
        IrExpr::Call { func, args } => {
            for a in args.iter_mut() {
                c |= transform_expr(a);
            }
            if func == "capture" || func == "captureWords" {
                // unwrap the capture(...) args to the wrapped pipeline
                match args.first_mut() {
                    Some(IrExpr::Arrow(body)) => {
                        if let [IrStmt::Expr(pipe)] = body.as_mut_slice() {
                            c |= clean_pipeline(pipe);
                        }
                    }
                    Some(IrExpr::Array(items)) => {
                        if let [IrExpr::Arrow(body)] = items.as_mut_slice() {
                            if let [IrStmt::Expr(pipe)] = body.as_mut_slice() {
                                c |= clean_pipeline(pipe);
                            }
                        }
                    }
                    Some(other) => {
                        let is_pipe = match other {
                            IrExpr::Call { func, .. } => func.as_str() == "pipeline",
                            _ => false,
                        };
                        if is_pipe {
                            c |= clean_pipeline(other);
                        }
                    }
                    None => {}
                }
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let InterpPart::Expr(e2) = p {
                    c |= transform_expr(e2);
                }
            }
        }
        IrExpr::Arrow(stmts) => {
            c |= transform(stmts);
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            c |= transform_expr(iter);
            c |= transform_expr(elem);
            if let Some(cd) = cond {
                c |= transform_expr(cd);
            }
        }
        IrExpr::Lambda { body, .. } => {
            c |= transform(body);
        }
        IrExpr::Array(items) => {
            for a in items.iter_mut() {
                c |= transform_expr(a);
            }
        }
        IrExpr::Object(pairs) => {
            for (_, v) in pairs.iter_mut() {
                c |= transform_expr(v);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            c |= transform_expr(lhs);
            c |= transform_expr(rhs);
        }
        IrExpr::MethodCall { obj, args, .. } => {
            c |= transform_expr(obj);
            for a in args.iter_mut() {
                c |= transform_expr(a);
            }
        }
        IrExpr::Index { key, .. } => {
            c |= transform_expr(key);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            c |= transform_expr(cond);
            c |= transform_expr(then);
            c |= transform_expr(else_);
        }
        _ => {}
    }
    c
}

/// `echo "scale=K; EXPR + 0.0" | bc` — strip the trailing `+ 0.0` when
/// the rest of the expression already has a decimal (so the float
/// verdict survives). Mirrors the sh→GLSL backend's `pipeline_echo_bc`
/// shape check.
fn clean_pipeline(pipe: &mut IrExpr) -> bool {
    let IrExpr::Call { func, args } = pipe else {
        return false;
    };
    if func != "pipeline" {
        return false;
    }
    let [IrExpr::Array(stages)] = args.as_mut_slice() else {
        return false;
    };
    if stages.len() != 2 {
        return false;
    }
    let [IrExpr::Arrow(s1), IrExpr::Arrow(s2)] = stages.as_mut_slice() else {
        return false;
    };
    let [IrStmt::Expr(IrExpr::Call { func: f1, args: a1 })] = s1.as_mut_slice() else {
        return false;
    };
    if !matches!(f1.as_str(), "exec" | "builtin") {
        return false;
    }
    let [IrExpr::Str(name1, _), IrExpr::Array(echo_args)] = a1.as_mut_slice() else {
        return false;
    };
    if name1 != "echo" {
        return false;
    }
    let [IrStmt::Expr(IrExpr::Call { func: f2, args: a2 })] = s2.as_mut_slice() else {
        return false;
    };
    if f2 != "exec" {
        return false;
    }
    let [IrExpr::Str(bc_name, _)] = a2.as_mut_slice() else {
        return false;
    };
    if bc_name != "bc" {
        return false;
    }
    if echo_args.len() != 1 {
        return false;
    }
    clean_bc_arg(&mut echo_args[0])
}

fn clean_bc_arg(arg: &mut IrExpr) -> bool {
    match arg {
        IrExpr::Str(s, _) => {
            if let Some(stripped) = strip_trailing_add_zero(s) {
                if stripped.contains('.') {
                    *s = stripped;
                    return true;
                }
            }
            false
        }
        IrExpr::Interpolate(parts) => {
            let n = parts.len();
            // peek (immutable) at the last part's text minus the + 0.0
            let Some(last_clean) = (match parts.last() {
                Some(InterpPart::Lit(t)) => strip_trailing_add_zero(t),
                _ => None,
            }) else {
                return false;
            };
            // the float verdict needs a decimal somewhere in the rest
            // (the last part itself may carry one — e.g. " / 1000.0")
            let mut rest_has_decimal = last_clean.contains('.');
            if !rest_has_decimal {
                for (i, p) in parts.iter().enumerate() {
                    if i == n - 1 {
                        continue;
                    }
                    if let InterpPart::Lit(l) = p {
                        if l.contains('.') {
                            rest_has_decimal = true;
                            break;
                        }
                    }
                }
            }
            if !rest_has_decimal {
                return false;
            }
            if let Some(InterpPart::Lit(t)) = parts.last_mut() {
                *t = last_clean;
                return true;
            }
            false
        }
        _ => false,
    }
}

/// The trailing `+ 0.0` / `+0.0` after the LAST '+', trimmed back.
fn strip_trailing_add_zero(t: &str) -> Option<String> {
    let trimmed = t.trim_end();
    let (prefix, rest) = trimmed.rsplit_once('+')?;
    if rest.trim() != "0.0" {
        return None;
    }
    Some(prefix.trim_end().to_string())
}
//
// ## Manifest (PLAN §11.4)
// name: bc-float-clean
// prereqs: [] (a pure IR-level identity strip — no analyses consumed)
// invariant: only `echo … | bc` captures whose LAST literal ends in
//   `+ 0.0` AND whose remainder already contains a decimal point are
//   touched; every other shape is left byte-identical (refuse > guess).
//   Output is behaviourally identical (X + 0.0 == X for finite values).
// scope: offered to glsl (owner — the mimecroft shaders) and sh (which
//   also has a `bc` path); all other backends expected to reject (their
//   `qx{echo … | bc}` captures are runtime, not IR, so the strip is a
//   no-op for them).
// updates: none (first offer; un-rejected from the pre-marketplace pile)
