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
                // `-p`: create each missing path COMPONENT, skip the ones
                // that exist (bash's rule), and report like `mkdir` does.
                // `File::Path::make_path` swallowed the error (an empty
                // errmsg here) and always reported success, so a
                // `mkdir -p /no/such/dir` failure read as 0.
                format!(
                    "my $__ok = 1; for my $__d ({joined}) {{ my $__acc = $__d =~ m{{^/}} ? '' : '.'; for my $__c (split m{{/+}}, $__d) {{ next if $__c eq '' || $__c eq '.'; $__acc .= \"/$__c\"; next if -d $__acc; unless (mkdir($__acc)) {{ $__ok = 0; print STDERR \"mkdir: $!\\n\"; last; }} }} }}"
                )
            } else {
                format!("my $__ok = 1; for my $__d ({joined}) {{ unless (mkdir($__d)) {{ $__ok = 0; print STDERR \"mkdir: $!\\n\"; }} }}")
            }
        } else if cmd == "rmdir" {
            format!("my $__ok = 1; for my $__d ({joined}) {{ unless (rmdir($__d)) {{ $__ok = 0; print STDERR \"rmdir: failed to remove '$__d': $!\\n\"; }} }}")
        } else {
            format!("my $__ok = 1; for my $__t ({joined}) {{ if (open(my $__h, '>>', $__t)) {{ close($__h); }} else {{ $__ok = 0; print STDERR \"touch: cannot touch '$__t': $!\\n\"; }} }}")
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
        // A failed cp is not silent: GNU cp reports the missing source on
        // stderr (`cp: cannot stat 'file1': No such file or directory`) and
        // exits 1. The status below already matched; only the diagnostic was
        // dropped (builtin-system-open3).
        "cp" => format!(
            "require File::Copy; my $__ok = File::Copy::copy({aq}, {bq}); \
             if (!$__ok && $ERRNO eq q{{No such file or directory}}) {{ \
                 print STDERR \"cp: cannot stat '\", {aq}, \"': $ERRNO\\n\"; }}"
        ),
        "mv" => format!("my $__ok = rename({aq}, {bq});"),
        _ => return None,
    };
    Some(NativeEmit::Stmt(format!(
        "{body} $main_exit_code = $CHILD_ERROR = $__ok ? 0 : 1;"
    )))
}
