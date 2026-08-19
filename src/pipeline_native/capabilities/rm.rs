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
    let mut operands: Vec<String> = Vec::new();
    for w in words {
        match w {
            IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {
                let f: String = s[1..].chars().filter(|c| "f".contains(*c)).collect();
                if f.len() != s.len() - 1 {
                    return None; // -r/-R/i/... refuse
                }
                force |= f.contains('f');
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
            _ => return None,
        }
    }
    if operands.is_empty() {
        return None;
    }
    // rm without -f on a missing file errors; with -f it's silent success.
    // Corpus sites are `rm -f file…`; require -f to lift (unlink + status 0).
    if !force {
        return None;
    }
    let joined = operands.join(", ");
    Some(NativeEmit::Stmt(format!(
        "unlink({joined}); $main_exit_code = $CHILD_ERROR = 0;"
    )))
}
