//! Capability: `cd DIR && pwd` in CAPTURE position —
//! `d=$(cd /tmp && pwd)`, the idiom every `script_dir=$(cd -- "$(dirname
//! "$0")" && pwd)` preamble uses.
//!
//! ## Need
//!
//! The compound capture has no pipeline stage to lower, so it fell through
//! to `bash -c` with a leading `cd` — check_qx flags "wrapping builtin
//! 'cd'". The semantics are exactly Perl's `chdir` plus `Cwd::getcwd`, and
//! the `&&` means `pwd` runs only when the `cd` succeeded.
//!
//! ## Lowering
//!
//! ```perl
//! $__sh_cap = (chdir(DIR) ? (require Cwd, Cwd::getcwd()) : q{});
//! $main_exit_code = $CHILD_ERROR = chdir-succeeded ? 0 : 1;
//! ```
//!
//! bash: on `cd` failure the `&&` short-circuits, the capture is empty and
//! the status is `cd`'s (non-zero); on success the value is the new
//! directory and the status is `pwd`'s (0). A missing directory makes
//! `chdir` false, matching bash.
//!
//! ## Refusals
//!
//! Any other chain shape (an extra command, `||`, a flag other than a
//! leading `--`/`-P`/`-L`, a non-literal directory) returns None and keeps
//! the shell-out. NOTE the `DIR` operand must be literal — a `$(…)` inside
//! it is not expanded here.

use crate::ir::{BinOpKind, IrExpr};
use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Capture { call } = ctx else {
        return None;
    };
    let IrExpr::BinOp {
        lhs,
        op: BinOpKind::And,
        rhs,
    } = call
    else {
        return None;
    };
    // lhs = `cd [--|-P|-L] DIR`, rhs = bare `pwd`
    let dir = cd_target(lhs)?;
    if !is_bare_pwd(rhs) {
        return None;
    }
    let q = crate::ir::safe_perl_q_string(&dir);
    // `chdir` is evaluated once; its result selects both the value and the
    // status. `require Cwd` inline (a leaf cannot push to `imports`).
    Some(NativeEmit::Stmt(format!(
        "if (chdir({q})) {{ $__sh_cap = (require Cwd, Cwd::getcwd()); $main_exit_code = $CHILD_ERROR = 0; }} else {{ $__sh_cap = q{{}}; $main_exit_code = $CHILD_ERROR = 1; }}"
    )))
}

/// The literal target of a `cd [flags] DIR` exec call.
fn cd_target(e: &IrExpr) -> Option<String> {
    let IrExpr::Call { func, args } = e else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else {
        return None;
    };
    if cmd != "cd" {
        return None;
    }
    let mut target: Option<String> = None;
    for w in words {
        match w {
            // `cd -- DIR` / `cd -P DIR` — the flags a corpus preamble uses
            IrExpr::Str(s, _) if s == "--" || s == "-P" || s == "-L" => continue,
            IrExpr::Str(s, _) if s.starts_with('-') => return None,
            IrExpr::Str(s, _) => {
                if target.is_some() {
                    return None; // two operands
                }
                target = Some(s.clone());
            }
            _ => return None, // a variable/glob dir — not modelled
        }
    }
    target
}

/// TRUE for a bare `pwd` (no operands, no flags).
fn is_bare_pwd(e: &IrExpr) -> bool {
    let IrExpr::Call { func, args } = e else {
        return false;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return false;
    }
    matches!(args.as_slice(), [IrExpr::Str(c, _), IrExpr::Array(w)] if c == "pwd" && w.is_empty())
}
