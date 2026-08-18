//! Capability: `ls <literal args> 2>/dev/null || echo FALLBACK` — the
//! directory-listing-with-fallback idiom. Native: for each (sorted) arg,
//! print a regular file's name or a directory's sorted entries; a missing
//! operand makes ls exit 2, so the literal-echo fallback runs. Refused for
//! -l/-a/-R/… flags, non-literal args, or a non-literal fallback.

use crate::ir::{BinOpKind, IrExpr, IrStmt};
use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Chain(e) = ctx else {
        return None;
    };
    let IrExpr::BinOp {
        lhs,
        op: BinOpKind::Or,
        rhs,
    } = e
    else {
        return None;
    };
    // unwrap an optional 2>/dev/null redirect around the ls
    let ls = unwrap_stderr_redirect(lhs);
    let (flags, args) = ls_args(ls)?;
    if !flags.is_empty() {
        return None; // -l/-a/-R… not reproduced
    }
    if args.is_empty() {
        return None; // no-arg `ls` lists the CWD — different semantics
    }
    let argv: Vec<String> = args.iter().map(|a| crate::ir::grep_lit_str(a)).collect::<Option<_>>()?;
    let fallback = crate::pipeline_native::capabilities::grep::literal_echo(rhs)?;
    let perl = build_ls(&argv, &fallback);
    Some(NativeEmit::Stmt(perl))
}

/// `redirect(…, specs)` where every spec is an fd-2 redirect → the inner call.
fn unwrap_stderr_redirect(e: &IrExpr) -> &IrExpr {
    let IrExpr::Call { func, args } = e else {
        return e;
    };
    if func != "redirect" {
        return e;
    }
    let [IrExpr::Arrow(inner), IrExpr::Array(specs)] = args.as_slice() else {
        return e;
    };
    let stderr_only = specs.iter().all(|s| {
        if let IrExpr::Object(entries) = s {
            entries.iter().all(|(k, v)| {
                if k == "fd" {
                    matches!(v, IrExpr::Int(2))
                } else {
                    true
                }
            })
        } else {
            false
        }
    });
    let [IrStmt::Expr(call)] = inner.as_slice() else {
        return e;
    };
    if stderr_only {
        call
    } else {
        e
    }
}

fn ls_args(e: &IrExpr) -> Option<(String, Vec<&IrExpr>)> {
    let IrExpr::Call { func, args } = e else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    if !matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "ls") {
        return None;
    }
    let words = crate::ir::exec_word_args(args);
    let mut flags = String::new();
    let mut rest = Vec::new();
    for w in words {
        if let IrExpr::Str(s, _) = w {
            if s.starts_with('-') && s.len() > 1 && s != "--" {
                flags.push_str(s);
                continue;
            }
        }
        rest.push(w);
    }
    Some((flags, rest))
}

fn build_ls(argv: &[String], fallback: &str) -> String {
    let mut argv_perl = String::new();
    for (i, a) in argv.iter().enumerate() {
        if i > 0 {
            argv_perl.push_str(", ");
        }
        argv_perl.push_str(&crate::ir::safe_perl_q_string(a));
    }
    format!(
        "my @__ls_argv = ({argv_perl}); my @__ls_out = (); my $__ls_missing = 0;\n\
         my @__ls_argv_sorted = sort @__ls_argv;\n\
         for my $__f (@__ls_argv_sorted) {{ if (-d $__f) {{ opendir(my $__dh, $__f); push @__ls_out, grep {{ !/^\\.\\.?$/ }} sort readdir $__dh; closedir $__dh; }} elsif (-e $__f) {{ push @__ls_out, $__f; }} else {{ $__ls_missing = 1; }} }}\n\
         print \"$_\\n\" for @__ls_out;\n\
         if ($__ls_missing) {{ {fallback} }}\n\
         $main_exit_code = $CHILD_ERROR = 0;"
    )
}
