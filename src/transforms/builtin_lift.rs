//! builtin-lift — `exec("cmd", args)` → `builtin("cmd", args)` for every
//! command in the shared namespace contract (harness/builtins.json).
//!
//! THE keystone of the "backends render native shIR only" architecture:
//! the native-lowering decision moves OUT of the renderers INTO the A1.
//! Today each backend re-decides: estree's render-time JS_SYNC_BUILTINS
//! table → sh2.builtin; rust → `bash -c` via __sh_run; perl → qx{}; c →
//! system(). With this transform + the `builtin` Call op in the contract,
//! the A1 itself carries the native form and every backend renders it
//! natively; the check (harness/check_qx_shir.py) becomes a single
//! contract-level invariant.
//!
//! Measured gap it closes (the ladder): 536/546 corpus examples, 2738
//! exec-of-builtin violations (echo 486, printf 101, rm 55, set 38, grep
//! 33, cd 29, exit 26, sort 24, head 23, sed 22, local 22, true 20, …).
//!
//! Conservative: only a STATIC first-arg command name is lifted. A
//! variable-sourced command (`"$cmd" args`) or an unknown name keeps the
//! exec (correct, unoptimized — genuinely-external commands keep their
//! backend spawn/qx, which is legitimate: no native op exists).
//!
//! Contract companion: the deserializer (shir_json_in.rs) must admit
//! `Call { func: "builtin", args: [Str(cmd), Array(words)] }` and REFUSE
//! unknown cmd names at ingress (documented ERASURE, like the generics
//! typeArgs). Renderers then map the op to their native runtime (estree
//! sh2.builtin, rust __sh_builtin, perl native subs, c _sh_* helpers).
//! A4 purity is re-derived at serialization (builtin → Emulable).

use crate::ir::{InterpPart, IrCaseClause, IrExpr, IrRedirect, IrStmt, TryExcept};

/// The shared namespace contract (harness/builtins.json — single source
/// of truth; the go-sh frontend's a4_sync_builtins_matches_rust test
/// pins the same list, and the estree worker must add a matching
/// builtin_lift_sync_builtins_matches_json test so the two stay in sync).
pub const BUILTINS: &[&str] = &[
    ".", ":", "basename", "break", "cd", "cmp", "comm", "command", "continue",
    "cp", "cut", "date", "declare", "diff", "dirname", "echo", "egrep", "eval",
    "exit", "export", "false", "find", "grep", "gunzip", "gzip", "head",
    "hostname", "let", "local", "ls", "mapfile", "mkdir", "mv", "paste",
    "printf", "pwd", "read", "readarray", "readlink", "readonly", "return",
    "rm", "rmdir", "sed", "seq", "set", "sha256sum", "sha512sum", "shift",
    "sleep", "sort", "source", "stat", "tail", "tee", "touch", "tr", "trap",
    "true", "type", "typeset", "uname", "uniq", "unset", "wait", "wc",
    "which", "whoami", "xargs",
];

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    for st in stmts.iter_mut() {
        changed |= lift_stmt(st);
    }
    changed
}

fn lift_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Expr(e) => lift_expr(e),
        IrStmt::Assign { expr, .. } => lift_expr(expr),
        IrStmt::Declare { init, .. } => init.as_mut().map(lift_expr).unwrap_or(false),
        IrStmt::Output { value, .. } => lift_expr(value),
        IrStmt::SetChildError(e) => lift_expr(e),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => lift_expr(e),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut c = lift_expr(cond);
            for s in then.iter_mut() {
                c |= lift_stmt(s);
            }
            for (cond2, body) in elsifs.iter_mut() {
                c |= lift_expr(cond2);
                for s in body.iter_mut() {
                    c |= lift_stmt(s);
                }
            }
            for s in else_.iter_mut() {
                c |= lift_stmt(s);
            }
            c
        }
        IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
            let mut c = lift_expr(cond);
            for s in body.iter_mut() {
                c |= lift_stmt(s);
            }
            c
        }
        IrStmt::For { iter, body, .. } => {
            let mut c = lift_expr(iter);
            for s in body.iter_mut() {
                c |= lift_stmt(s);
            }
            c
        }
        IrStmt::ForInit { init, cond, step, body } => {
            let mut c = false;
            for s in init.iter_mut() {
                c |= lift_stmt(s);
            }
            c |= lift_expr(cond);
            for s in step.iter_mut() {
                c |= lift_stmt(s);
            }
            for s in body.iter_mut() {
                c |= lift_stmt(s);
            }
            c
        }
        IrStmt::Block(b)
        | IrStmt::Subshell(b)
        | IrStmt::Background(b)
        | IrStmt::Function { body: b, .. } => {
            let mut c = false;
            for s in b.iter_mut() {
                c |= lift_stmt(s);
            }
            c
        }
        IrStmt::Redirect { inner, redirects } => {
            let mut c = false;
            for s in inner.iter_mut() {
                c |= lift_stmt(s);
            }
            for r in redirects.iter_mut() {
                let IrRedirect { target, .. } = r;
                c |= lift_expr(target);
            }
            c
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for stage in stages.iter_mut() {
                for s in stage.iter_mut() {
                    c |= lift_stmt(s);
                }
            }
            c
        }
        IrStmt::Case {
            discriminant,
            clauses,
            ..
        } => {
            let mut c = lift_expr(discriminant);
            for IrCaseClause { body, .. } in clauses.iter_mut() {
                for s in body.iter_mut() {
                    c |= lift_stmt(s);
                }
            }
            c
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= lift_stmt(s);
            }
            for TryExcept {
                match_expr, body, ..
            } in excepts.iter_mut()
            {
                if let Some(m) = match_expr.as_mut() {
                    c |= lift_expr(m);
                }
                for s in body.iter_mut() {
                    c |= lift_stmt(s);
                }
            }
            for s in else_body.iter_mut() {
                c |= lift_stmt(s);
            }
            for s in finally_body.iter_mut() {
                c |= lift_stmt(s);
            }
            c
        }
        _ => false,
    }
}

fn lift_expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Call { func, args } if func == "exec" => {
            // the lift: a static first-arg command in the namespace
            let is_builtin = matches!(
                args.first(),
                Some(IrExpr::Str(cmd, _)) if BUILTINS.contains(&cmd.as_str())
            );
            if is_builtin {
                *func = "builtin".to_string();
                // the remaining words stay as-is (word-shaping already done)
                args.iter_mut().skip(1).fold(false, |c, a| lift_expr(a) || c)
            } else {
                args.iter_mut().fold(false, |c, a| lift_expr(a) || c)
            }
        }
        // any other call (builtin/pipeline/capture/param/...): walk args —
        // a pipeline's stage Arrows live in the args
        IrExpr::Call { args, .. }
        | IrExpr::MethodCall { args, .. }
        | IrExpr::Array(args) => args.iter_mut().fold(false, |c, a| lift_expr(a) || c),
        IrExpr::Arrow(stmts) | IrExpr::Lambda { body: stmts, .. } => {
            let mut c = false;
            for s in stmts.iter_mut() {
                c |= lift_stmt(s);
            }
            c
        }
        IrExpr::Capture { expr, .. } => lift_expr(expr),
        IrExpr::Ternary { cond, then, else_, .. } => {
            lift_expr(cond) || lift_expr(then) || lift_expr(else_)
        }
        IrExpr::DefinedOr { expr, default } => lift_expr(expr) || lift_expr(default),
        IrExpr::Interpolate(parts) => parts.iter_mut().fold(false, |c, p| match p {
            InterpPart::Expr(x) => lift_expr(x) || c,
            _ => c,
        }),
        IrExpr::BinOp { lhs, rhs, .. } => lift_expr(lhs) || lift_expr(rhs),
        IrExpr::Index { key, .. } => lift_expr(key),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            lift_expr(iter) || lift_expr(elem) || cond.as_mut().map(lift_expr).unwrap_or(false)
        }
        _ => false,
    }
}
