//! exec-call-fold — canonicalise the payload-free `IrStmt::Exec` inside a
//! COMMAND CARRIER (an arrow body, a pipeline stage, and everything nested
//! in one) to the `Expr(Call { func: "exec", args: [cmd, Array(argv)] })`
//! command spelling.
//!
//! name: exec-call-fold
//! prereqs: [process-subst — it is what mints `Arrow([Exec])` for the
//!   materialised `<(...)` temp files]
//! invariant: an `IrStmt::Exec` whose `capture` is `None` and whose
//!   `redirects`/`env` are EMPTY carries exactly the payload
//!   `Call { func: "exec", args: [cmd, Array(argv)] }` carries — the two
//!   are the same command in two carriers, so the rewrite is information
//!   preserving. REFUSE > GUESS: a capture channel, a redirect set or a
//!   command-scoped env keeps its `Exec` (the call form has nowhere to
//!   put them), and the synthetic process-substitution cleanup
//!   (`shir::is_ps_tmp_cleanup`) keeps its `Exec` too — every renderer
//!   recognises THAT statement by its carrier to save/restore the
//!   consumer's status around it, and recognition is the point.
//! scope: offered to every backend; the `shir` A1-export target is
//!   excluded in `transform_applies` (the byte-pinned export path keeps
//!   its bytes; the renderers still see the fold).
//! updates: none (first offer)
//!
//! ## Need
//! `IrStmt::Exec` is the neutral "run this command" statement the C
//! frontend and the process-subst materialiser emit; `Expr(Call{"exec"})`
//! is the spelling the compound-context replayers were written against.
//! The corpus carries 71 `Exec` statements, 48 of them as arrow bodies
//! (every `$(mktemp)` behind a `<(...)`) and the rest as ordinary
//! commands. Both carriers render where a backend matches them — and the
//! backends' COMPOUND-CONTEXT code does not:
//!
//!   * go's `cmd_text` (the shell-text rebuild feeding capture/pipeline
//!     replay) returns `None` for `IrStmt::Exec`, and go's `stmt` marks
//!     it `TODO(unsupported): stmt Exec {…}` — 012_process_substitution
//!     renders 17 of those lines;
//!   * python's `body_shell_text`, `pipeline_run`,
//!     `pipeline_head_value` and `capture_body_is_native` all match
//!     `Expr(Call{"exec"|"builtin"})` only, so the same file fell
//!     through to `sh2_capture(sh2_arrow())` / `sh2_pipeline([…])`
//!     runtime stubs (97 + 90 + 35 call sites corpus-wide);
//!   * perl's `capture_body_is_native` whitelist is keyed on the Call
//!     form, so an `Exec`-bodied capture falls back to a qx fork even
//!     when the body is an `echo`.
//!
//! sh/zsh/fish/java render both carriers today (they arm `IrStmt::Exec`
//! explicitly), which is why this is a rewrite of the IR and not a pile
//! of per-backend patches: whether a compound-context replayer recognises
//! a command is a PROGRAM shape, not a backend detail. One fold retires
//! the callout for every backend that accepts the offer, and it feeds the
//! later walkers (`dead-store-elim`, `copy-propagation`) a canonical
//! command node.
//!
//! ## Placement
//! Registered LAST in `transforms::all()` (after `interp-lit-merge`):
//! every pass that keys `IrStmt::Exec` by name (`arith-forms`' `let`,
//! `for-recovery`, `bc-float-clean`, `shir-pipeline-native`,
//! `fuse-fill-consume`, `hoist-loop-invariants`, `isolate-accumulate`)
//! therefore sees the carrier it was written against, and only the
//! renderers receive the canonical form. Pure, idempotent, and gated by
//! the usual `SH2_TRANSFORMS` name.

use crate::ir::{IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
/// Signature matches `transforms::TransformFn` (`fn(&mut Vec<IrStmt>)`).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    walk(stmts, false)
}

/// `carrier` — are these statements inside an arrow body / pipeline
/// stage (or nested in one)? Only there does the fold apply: a bare
/// statement-position `Exec` is the shape `is_ps_tmp_cleanup`,
/// `arith-forms` and the renderers' own `IrStmt::Exec` arms key on, and
/// folding it would trade a recognised statement for an unrecognised one.
fn walk(stmts: &mut [IrStmt], carrier: bool) -> bool {
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt(st, carrier);
    }
    c
}

fn fold_here(st: &IrStmt, carrier: bool) -> bool {
    if !carrier || !matches!(st, IrStmt::Exec { .. }) {
        return false;
    }
    // the synthetic `<(...)` cleanup — recognisable only by this carrier
    if crate::shir::is_ps_tmp_cleanup(st) {
        return false;
    }
    let IrStmt::Exec {
        capture,
        redirects,
        env,
        ..
    } = st
    else {
        return false;
    };
    capture.is_none() && redirects.is_empty() && env.is_empty()
}

fn stmt(st: &mut IrStmt, carrier: bool) -> bool {
    if fold_here(st, carrier) {
        // `fold_here` only answers true for a payload-free Exec, so the
        // match below always takes — and an unfolded node is the status
        // quo, never a lost statement.
        if let IrStmt::Exec { cmd, args, .. } = st {
            let folded = IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![cmd.clone(), IrExpr::Array(args.clone())],
            });
            *st = folded;
            return true;
        }
    }
    let mut c = false;
    match st {
        IrStmt::Output { value, .. } => c |= expr(value),
        IrStmt::WriteFile { path, content, .. } => {
            c |= expr(path);
            c |= expr(content);
        }
        IrStmt::Assign { targets, expr: e, .. } => {
            c |= expr(e);
            for t in targets.iter_mut() {
                for i in t.indices.iter_mut() {
                    c |= expr(i);
                }
            }
        }
        IrStmt::Declare { init: Some(e), .. } => c |= expr(e),
        IrStmt::DeclareArray { elements, .. } => {
            for e in elements.iter_mut() {
                c |= expr(e);
            }
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            c |= expr(cond);
            c |= walk(then, carrier);
            for (ec, eb) in elsifs.iter_mut() {
                c |= expr(ec);
                c |= walk(eb, carrier);
            }
            c |= walk(else_, carrier);
        }
        IrStmt::For { iter, body, .. } => {
            c |= expr(iter);
            c |= walk(body, carrier);
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            c |= walk(init, carrier);
            c |= expr(cond);
            c |= walk(step, carrier);
            c |= walk(body, carrier);
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            c |= expr(cond);
            c |= walk(body, carrier);
        }
        IrStmt::Die { expr: e, .. } | IrStmt::Warn { expr: e, .. } => c |= expr(e),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) | IrStmt::SetChildError(e) => {
            c |= expr(e)
        }
        IrStmt::Expr(e) => c |= expr(e),
        IrStmt::Function { body, .. } => c |= walk(body, false),
        IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => {
            c |= walk(body, carrier)
        }
        IrStmt::Redirect { inner, redirects } => {
            c |= walk(inner, carrier);
            for r in redirects.iter_mut() {
                c |= expr(&mut r.target);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            // the other carrier: a stage is a command list the pipeline
            // replayer rebuilds shell text from
            for s in stages.iter_mut() {
                c |= walk(s, true);
            }
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            c |= expr(discriminant);
            for cl in clauses.iter_mut() {
                c |= walk(&mut cl.body, carrier);
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            c |= walk(body, carrier);
            for ex in excepts.iter_mut() {
                c |= expr_opt(&mut ex.match_expr);
                c |= walk(&mut ex.body, carrier);
            }
            c |= walk(else_body, carrier);
            c |= walk(finally_body, carrier);
        }
        IrStmt::Select { clauses } => {
            for cl in clauses.iter_mut() {
                c |= expr_opt(&mut cl.ch);
                c |= expr_opt(&mut cl.value);
                c |= walk(&mut cl.body, carrier);
            }
        }
        // statement-position Exec (handled at the top), Require, RawText,
        // Label/Goto, Asm, Shaderize, Continue/Break, Ext: nothing to fold.
        _ => {}
    }
    c
}

fn expr_opt(e: &mut Option<IrExpr>) -> bool {
    match e {
        Some(x) => expr(x),
        None => false,
    }
}

fn expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Call { func: _, args } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= expr(a);
            }
            c
        }
        IrExpr::MethodCall { obj, args, .. } => {
            let mut c = expr(obj);
            for a in args.iter_mut() {
                c |= expr(a);
            }
            c
        }
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let crate::ir::InterpPart::Expr(x) = p {
                    c |= expr(x);
                }
            }
            c
        }
        IrExpr::Array(items) => {
            let mut c = false;
            for i in items.iter_mut() {
                c |= expr(i);
            }
            c
        }
        IrExpr::ArrayComp {
            iter, elem, cond, ..
        } => {
            let mut c = expr(iter);
            c |= expr(elem);
            if let Some(cond) = cond {
                c |= expr(cond);
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            let mut c = expr(lhs);
            c |= expr(rhs);
            c
        }
        IrExpr::Index { key, .. } => expr(key),
        IrExpr::Ternary { cond, then, else_ } => {
            let mut c = expr(cond);
            c |= expr(then);
            c |= expr(else_);
            c
        }
        IrExpr::DefinedOr { expr: x, default } => {
            let mut c = expr(x);
            c |= expr(default);
            c
        }
        // the first-class node and the call spelling both funnel into the
        // Arrow body — recurse only (idempotent).
        IrExpr::Capture { expr: x, .. } => expr(x),
        // THE carrier: everything the replayer rebuilds runs through here.
        IrExpr::Arrow(body) => walk(body, true),
        // a function BODY is statement position (a shell function's
        // commands are ordinary commands, not a capture/pipeline
        // replayer's payload) — leave its Exec carriers alone.
        IrExpr::Lambda { body, .. } => walk(body, false),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    fn exec_mktemp() -> IrStmt {
        IrStmt::Exec {
            cmd: IrExpr::Str("mktemp".into(), StrStyle::DoubleQuoted),
            args: vec![],
            capture: None,
            redirects: vec![],
            env: vec![],
        }
    }

    #[test]
    fn arrow_body_exec_folds_to_the_call_form() {
        let mut stmts = vec![IrStmt::Expr(IrExpr::Arrow(vec![exec_mktemp()]))];
        assert!(transform(&mut stmts));
        let IrStmt::Expr(IrExpr::Arrow(body)) = &stmts[0] else {
            panic!("arrow lost");
        };
        assert!(matches!(
            &body[0],
            IrStmt::Expr(IrExpr::Call { func, .. }) if func == "exec"
        ));
        // idempotent
        let mut again = stmts.clone();
        assert!(!transform(&mut again));
        assert_eq!(again, stmts);
    }

    #[test]
    fn statement_position_exec_is_left_alone() {
        let mut stmts = vec![exec_mktemp()];
        assert!(!transform(&mut stmts));
        assert!(matches!(stmts[0], IrStmt::Exec { .. }));
    }

    #[test]
    fn payload_bearing_exec_refuses() {
        let with_capture = IrStmt::Exec {
            cmd: IrExpr::Str("mktemp".into(), StrStyle::DoubleQuoted),
            args: vec![],
            capture: Some("out".into()),
            redirects: vec![],
            env: vec![],
        };
        let mut stmts = vec![IrStmt::Expr(IrExpr::Arrow(vec![with_capture]))];
        assert!(!transform(&mut stmts));
        assert!(matches!(
            &stmts[0],
            IrStmt::Expr(IrExpr::Arrow(b)) if matches!(b[0], IrStmt::Exec { .. })
        ));
    }

    #[test]
    fn ps_tmp_cleanup_keeps_its_carrier() {
        let cleanup = IrStmt::Exec {
            cmd: IrExpr::Str("rm".into(), StrStyle::DoubleQuoted),
            args: vec![
                IrExpr::Str("-f".into(), StrStyle::DoubleQuoted),
                IrExpr::Var("__ps_tmp0".into(), None),
            ],
            capture: None,
            redirects: vec![],
            env: vec![],
        };
        let mut stmts = vec![IrStmt::Expr(IrExpr::Arrow(vec![cleanup]))];
        assert!(!transform(&mut stmts));
        assert!(matches!(
            &stmts[0],
            IrStmt::Expr(IrExpr::Arrow(b)) if matches!(b[0], IrStmt::Exec { .. })
        ));
    }
}