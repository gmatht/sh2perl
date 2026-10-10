//! Capability: `rm -f <paths>` → native `unlink`.
//!
//! Force-removal of regular files (literal paths or perl scalar vars) via
//! `unlink`. `-f` ignores missing files (status 0). Recursive `-r/-R`,
//! other flags, globs and non-scalar operands refuse (stays a shell-out).

use crate::ir::IrExpr;
use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Exec { call, cond: false } = ctx else {
        return None;
    };
    let IrExpr::Call { func, args } = call else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    if !matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "rm") {
        return None;
    }
    let words: Vec<&IrExpr> = crate::ir::exec_word_args(args);
    let mut force = false;
    let mut recursive = false;
    let mut operands: Vec<String> = Vec::new();
    for w in words {
        match w {
            IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {
                let f: String = s[1..]
                    .chars()
                    .filter(|c| "fRr".contains(*c))
                    .collect();
                if f.len() != s.len() - 1 {
                    return None; // -i/... refuse
                }
                force |= f.contains('f');
                recursive |= f.contains('r') || f.contains('R');
            }
            IrExpr::Str(s, _) => {
                if s.contains('*') || s.contains('?') {
                    return None; // glob — rm expands it
                }
                operands.push(crate::ir::safe_perl_q_string(s));
            }
            IrExpr::Var(name, _) => {
                operands.push(format!("${name}"));
            }
            // Interpolated / getVar operand (`rm -rf "$d"`).
            IrExpr::Interpolate(..) => {
                operands.push(crate::ir::ir_expr_to_perl(w));
            }
            // Brace expansion (`rm f{1..3}`): a LIST operand.
            IrExpr::Call { func, args } if func == "brace" => {
                operands.push(crate::ir::render_brace_iter(args));
            }
            IrExpr::Call { .. } => {
                operands.push(crate::ir::ir_expr_to_perl(w));
            }
            _ => return None,
        }
    }
    if operands.is_empty() {
        return None;
    }
    // rm without -f on a missing file errors; with -f it's silent success.
    // Corpus sites are `rm -f file…` (or `rm -rf dir`) — require one of them.
    if !force && !recursive {
        let joined = operands.join(", ");
        return Some(NativeEmit::Stmt(format!(
            "my $__ok = 1; for my $__p ({joined}) {{ $__ok = 0 unless unlink($__p); }} $main_exit_code = $CHILD_ERROR = $__ok ? 0 : 1;"
        )));
    }
    let joined = operands.join(", ");
    if recursive {
        Some(NativeEmit::Stmt(format!(
            "require File::Path; File::Path::remove_tree({joined}); $main_exit_code = $CHILD_ERROR = 0;"
        )))
    } else {
        Some(NativeEmit::Stmt(format!(
            "unlink({joined}); $main_exit_code = $CHILD_ERROR = 0;"
        )))
    }
}
