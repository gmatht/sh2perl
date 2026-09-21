//! Capability: `printf LITERAL | wc -L/-l` → native longest-line / count,
//! and `wc {-l|-w|-c|-m} < FILE` → a native read of the file.
//!
//! ## Need
//!
//! `wc -l < FILE` is a capture whose input arrives as an fd-0 redirect, not
//! a pipeline (the IR is `redirect(Arrow([exec wc …]), [{fd:0,in,FILE}])`).
//! No `NativeCtx::Pipe` parent sees it, so it fell through to the uu-ffi
//! seam / `bash -c` — and check_qx flags the fallback ("wrapping builtin
//! 'wc'"). Reading the file in Perl is exact and needs no child process.
//!
//! ## Lowering
//!
//! Read the whole file, then count. Status is 0 on success and 1 when the
//! file cannot be opened (bash's `wc` on a missing file exits 1 and prints
//! nothing on stdout, so the empty capture matches).
//!
//! - `-l` lines: count `\n` (bash counts newline-terminated lines; a final
//!   line without a newline is NOT counted)
//! - `-w` words: whitespace-separated fields
//! - `-c` bytes: the file's byte length
//! - `-m` chars: the character count
//!
//! ## Refusals
//!
//! No input redirect, a non-literal path, an unknown flag, or extra
//! operands return None and keep the shell-out.

use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    match ctx {
        NativeCtx::Pipe(p) => wc_pipe(&p.cmd, &p.words, &p.content_perl),
        NativeCtx::Capture { call } => wc_file(call),
        _ => None,
    }
}

fn wc_pipe(cmd: &str, words: &[String], content: &str) -> Option<NativeEmit> {
    if cmd != "wc" {
        return None;
    }
    if words == ["-L"] {
        Some(NativeEmit::Stmt(format!(
            "my $__m = 0; for my $__l (split(/\\n/, {content})) {{ $__m = $__m < length($__l) ? length($__l) : $__m; }} print $__m, \"\\n\"; $main_exit_code = $CHILD_ERROR = 0;"
        )))
    } else if words == ["-l"] {
        Some(NativeEmit::Stmt(format!(
            "my @__pl = split(/\\n/, {content}); print scalar(@__pl), \"\\n\"; $main_exit_code = $CHILD_ERROR = 0;"
        )))
    } else {
        None
    }
}

/// `wc {-l|-w|-c|-m} < FILE` — the redirect-on-an-exec shape.
fn wc_file(call: &crate::ir::IrExpr) -> Option<NativeEmit> {
    use crate::ir::{IrExpr, IrStmt};
    // `redirect(Arrow([exec wc …]), [specs])`
    let IrExpr::Call { func, args } = call else {
        return None;
    };
    if func != "redirect" {
        return None;
    }
    let [IrExpr::Arrow(inner), IrExpr::Array(specs)] = args.as_slice() else {
        return None;
    };
    let [IrStmt::Expr(exec)] = inner.as_slice() else {
        return None;
    };
    let IrExpr::Call { func: f, args: a } = exec else {
        return None;
    };
    if !matches!(f.as_str(), "exec" | "builtin") {
        return None;
    }
    let [IrExpr::Str(cmd, _), IrExpr::Array(words)] = a.as_slice() else {
        return None;
    };
    if cmd != "wc" {
        return None;
    }
    // exactly `-l`/`-w`/`-c`/`-m`
    if words.len() != 1 {
        return None;
    }
    let flag = match &words[0] {
        IrExpr::Str(s, _) if matches!(s.as_str(), "-l" | "-w" | "-c" | "-m") => s.clone(),
        _ => return None,
    };
    // exactly one fd-0 input redirect to a literal path
    let [spec] = specs.as_slice() else {
        return None;
    };
    let IrExpr::Object(entries) = spec else {
        return None;
    };
    let mut fd = None;
    let mut mode = None;
    let mut target = None;
    for (k, v) in entries {
        match k.as_str() {
            "fd" => fd = Some(v.clone()),
            "mode" => mode = Some(v.clone()),
            "target" => target = Some(v.clone()),
            _ => return None,
        }
    }
    if !matches!(fd, Some(IrExpr::Int(0))) {
        return None;
    }
    if !matches!(mode, Some(IrExpr::Str(ref m, _)) if m == "in" || m == "r") {
        return None;
    }
    let Some(IrExpr::Str(path, _)) = target else {
        return None;
    };
    // read the whole file; a missing file exits 1 with no output
    let count = match flag.as_str() {
        "-l" => "$__d =~ tr/\\n//",
        "-w" => "scalar(split(/\\s+/, $__d))",
        "-c" => "length($__d)",
        _ => "scalar(split(//, $__d))", // -m: characters
    };
    // `-w` on an empty file is 0, not 1: `split` on "" yields the empty list
    // in list context, so guard it explicitly.
    // The VALUE goes to $__sh_cap (the capture parent's channel) — not
    // stdout, which would be re-captured as text by the caller.
    let val = if flag == "-w" {
        "($__d eq q{}) ? 0 : scalar(split(/\\s+/, $__d))".to_string()
    } else {
        count.to_string()
    };
    Some(NativeEmit::Stmt(format!(
        "my $__f = {path}; if (open(my $__h, '<', $__f)) {{ my $__d = do {{ local $/; <$__h> }}; close($__h); $__d //= q{{}}; $__sh_cap = {val}; $main_exit_code = $CHILD_ERROR = 0; }} else {{ $__sh_cap = q{{}}; $main_exit_code = $CHILD_ERROR = 1; }}",
        path = crate::ir::safe_perl_q_string(&path)
    )))
}
