//! Capability: `cp SRC DST` / `mv SRC DST` — the file operations check_qx
//! flags when shelled out. Emits nothing to stdout (like the real tools)
//! and sets the exit status from the Perl copy/rename result, so a
//! `cp A B && echo ok` capture works via the generic native capture.
//!
//! Refusals: flags (`-r`, `-f`, …), globs, and any operand count we do not
//! model; those keep the shell-out.

use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Exec { call, cond: false } = ctx else {
        return None;
    };
    let crate::ir::IrExpr::Call { func, args } = call else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    let [crate::ir::IrExpr::Str(cmd, _), crate::ir::IrExpr::Array(words)] = args.as_slice() else {
        return None;
    };
    // `mkdir [-p] DIR…` / `touch FILE…` — flag-free (or `-p`) literal/var
    // operands only.
    if cmd == "mkdir" || cmd == "touch" || cmd == "rmdir" {
        let mut parents = false;
        let mut names: Vec<String> = Vec::new();
        for w in words {
            match w {
                crate::ir::IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {
                    if cmd == "mkdir" && s == "-p" {
                        parents = true;
                    } else {
                        return None;
                    }
                }
                crate::ir::IrExpr::Str(s, _) => {
                    names.push(crate::ir::safe_perl_q_string(s));
                }
                crate::ir::IrExpr::Var(n, _) => names.push(format!("${n}")),
                crate::ir::IrExpr::Interpolate(..) => {
                    names.push(crate::ir::ir_expr_to_perl(w));
                }
                crate::ir::IrExpr::Call { func, args } if func == "brace" => {
                    names.push(crate::ir::render_brace_iter(args));
                }
                crate::ir::IrExpr::Call { .. } => {
                    names.push(crate::ir::ir_expr_to_perl(w));
                }
                _ => return None,
            }
        }
        if names.is_empty() {
            return None;
        }
        let joined = names.join(", ");
        let body = if cmd == "mkdir" {
            if parents {
                format!("require File::Path; File::Path::make_path({joined}); my $__ok = 1;")
            } else {
                format!("my $__ok = 1; for my $__d ({joined}) {{ $__ok = 0 unless mkdir($__d); }}")
            }
        } else if cmd == "rmdir" {
            format!("my $__ok = 1; for my $__d ({joined}) {{ $__ok = 0 unless rmdir($__d); }}")
        } else {
            format!("my $__ok = 1; for my $__t ({joined}) {{ open(my $__h, '>>', $__t) or ($__ok = 0); close($__h) if $__ok; }}")
        };
        return Some(NativeEmit::Stmt(format!(
            "{body} $main_exit_code = $CHILD_ERROR = $__ok ? 0 : 1;"
        )));
    }
    let ws: Vec<String> = words
        .iter()
        .map(crate::ir::grep_lit_str)
        .collect::<Option<_>>()?;
    let [a, b] = ws.as_slice() else {
        return None;
    };
    if a.contains('*') || a.contains('?') || b.contains('*') || b.contains('?') {
        return None;
    }
    let aq = crate::ir::safe_perl_q_string(a);
    let bq = crate::ir::safe_perl_q_string(b);
    let body = match cmd.as_str() {
        "cp" => format!("require File::Copy; my $__ok = File::Copy::copy({aq}, {bq});"),
        "mv" => format!("my $__ok = rename({aq}, {bq});"),
        _ => return None,
    };
    Some(NativeEmit::Stmt(format!(
        "{body} $main_exit_code = $CHILD_ERROR = $__ok ? 0 : 1;"
    )))
}
