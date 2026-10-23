//! Capability: `cat <<'EOF' … EOF` — the parent already verified the shape
//! (cat with no args, a single stdin heredoc redirect with a literal body);
//! this leaf just emits the body as a native `print`.
//!
//! Also `cat FILE` (a single literal path or a Perl scalar variable) as a
//! capture — `x=$(cat "$tmp")`. check_qx flags the `bash -c` fallback for
//! `cat`, and reading a file in Perl is exact.
//!
//! Refusals: no operand (`cat` reads stdin), more than one operand, a
//! glob, or a flag (`-n`, `-A`, …) keep the shell-out. Note `cat FILE` with
//! MULTIPLE files concatenates them — not modelled.

use crate::ir::IrExpr;
use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    match ctx {
        NativeCtx::Heredoc(body) => Some(NativeEmit::Stmt(format!(
            "print({}); $main_exit_code = $CHILD_ERROR = 0;",
            crate::ir::safe_perl_q_string(body)
        ))),
        NativeCtx::Capture { call } => cat_file(call),
        _ => None,
    }
}

/// `cat FILE` with exactly one literal/variable operand — the value goes to
/// `$__sh_cap` (the capture parent's channel), not stdout.
fn cat_file(call: &IrExpr) -> Option<NativeEmit> {
    let IrExpr::Call { func, args } = call else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = args.as_slice() else {
        return None;
    };
    if cmd != "cat" {
        return None;
    }
    let [w] = words.as_slice() else {
        return None;
    };
    let path = match w {
        // a literal path (no glob — the shell would expand it)
        IrExpr::Str(s, _) => {
            if s.contains('*') || s.contains('?') || s.contains('[') || s.starts_with('-') {
                return None;
            }
            crate::ir::safe_perl_q_string(s)
        }
        // `cat "$tmp"` — a Perl scalar holding the path
        IrExpr::Var(name, _) => format!("${name}"),
        IrExpr::Call { func: f, args: a } if f == "getVar" => {
            let Some(IrExpr::Str(name, _)) = a.first() else {
                return None;
            };
            format!("${name}")
        }
        _ => return None,
    };
    // A missing file exits 1 with empty stdout (bash's cat).
    Some(NativeEmit::Stmt(format!(
        "if (open(my $__h, '<', {path})) {{ local $/; my $__d = <$__h>; close($__h); $__sh_cap = defined($__d) ? $__d : q{{}}; $main_exit_code = $CHILD_ERROR = 0; }} else {{ $__sh_cap = q{{}}; $main_exit_code = $CHILD_ERROR = 1; }}"
    )))
}
