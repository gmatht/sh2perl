//! Capability: a bare `mktemp` (no args) in CAPTURE position — create a
//! fresh empty file IN-PROCESS and hand back its path.
//!
//! ## Need
//!
//! `x=$(mktemp)` — and, far more often, the `process_subst` transform's
//! materialisation `tmp=$(mktemp)` that backs every `<(producer)` — was a
//! process created purely to obtain a filename: `__sh_pipe_capture(q{'mktemp'})`
//! forked `bash -c mktemp`, captured stdout, and chomped it. bash itself does
//! the identical thing (open O_EXCL, print the path, exit 0); nothing about it
//! needs a subprocess. This is the single most frequent value-channel capture
//! in the corpus (35 of the `__sh_uu_capture` sites, plus the 43 procsub
//! materialisations — one per `<( )`).
//!
//! ## Lowering
//!
//! `File::Temp::tempfile` gives mktemp's exact semantics — O_EXCL, mode 0600,
//! a collision-resistant unique name drawn from a secure PRNG, in `$TMPDIR`
//! (which is what mktemp reads) — returning the path. `UNLINK => 0` keeps the
//! file, because the whole point of the procsub materialisation is that a
//! later producer `open '>', $tmp` reuses the path (and the trailing `rm`
//! cleans it up). The result carries no trailing newline, so the capture
//! parent's `s/\n+\z//` is a no-op — output is byte-identical.
//!
//! `File::Temp` is `require`d inline (a leaf cannot push to the `imports`
//! list), and the path is returned on the `$__sh_cap` value channel the same
//! way `pathops` returns `pwd`/`basename`/`dirname`.
//!
//! ## Refusals
//!
//! Anything but the two exact spellings (`mktemp` / `mktemp -d`): flags
//! (`--tmpdir`, `-t`, …), a TEMPLATE operand, or any other argument keeps
//! every existing seam — the shell path still runs `bash -c mktemp`.

use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Exec { call, cond } = ctx else {
        return None;
    };
    // Only the VALUE position (a capture / argument), never a condition.
    if *cond {
        return None;
    }
    let crate::ir::IrExpr::Call { func, args } = call else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    let [crate::ir::IrExpr::Str(cmd, _), crate::ir::IrExpr::Array(words)] = args.as_slice() else {
        return None;
    };
    if cmd != "mktemp" {
        return None;
    }
    // The child a successful `mktemp`/`mktemp -d` leaves behind records
    // exit 0; the surrounding render owns `$CHILD_ERROR`, so mirror
    // `pathops` and pin it here. `$main_exit_code` is set for the
    // statement-position consumer the dispatch may also route here.
    let body = match words.as_slice() {
        // Bare `mktemp`: create a fresh empty FILE and hand back its path.
        [] => "do { require File::Temp; (File::Temp::tempfile('sh2_ps_XXXXXX', TMPDIR => 1, UNLINK => 0))[1] }".to_string(),
        // `mktemp -d`: create a fresh DIRECTORY and hand back its path.
        // Both sides are `mkdtemp` underneath — mode 0700, name from a
        // secure PRNG, in `$TMPDIR` (mktemp reads TMPDIR too). `CLEANUP =>
        // 0` keeps it; the script's own `rm -rf` cleans it, exactly as with
        // the real `mktemp -d`. UNLINK does not apply to tempdirs.
        [crate::ir::IrExpr::Str(flag, _)] if flag == "-d" => {
            "do { require File::Temp; File::Temp::tempdir('sh2_ps_XXXXXX', TMPDIR => 1, CLEANUP => 0) }".to_string()
        }
        _ => return None,
    };
    Some(NativeEmit::Stmt(format!(
        "$__sh_cap = {body}; $main_exit_code = $CHILD_ERROR = 0;"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, StrStyle};

    fn mktemp_call(words: Vec<IrExpr>) -> IrExpr {
        IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("mktemp".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(words),
            ],
        }
    }

    #[test]
    fn bare_mktemp_lowers_to_filetemp() {
        let call = mktemp_call(vec![]);
        let out = emit(&NativeCtx::Exec { call: &call, cond: false }).expect("bare mktemp lowers");
        let NativeEmit::Stmt(s) = out else { panic!("want Stmt") };
        assert!(s.contains("File::Temp::tempfile"), "{s}");
        // O_EXCL / 0600 / TMPDIR semantics, and the file is KEPT.
        assert!(s.contains("UNLINK => 0"), "{s}");
        // a successful mktemp leaves exit 0 in bash.
        assert!(s.contains("$CHILD_ERROR = 0"), "{s}");
    }

    #[test]
    fn mktemp_d_lowers_to_tempdir() {
        let call = mktemp_call(vec![IrExpr::Str("-d".to_string(), StrStyle::DoubleQuoted)]);
        let out = emit(&NativeCtx::Exec { call: &call, cond: false }).expect("mktemp -d lowers");
        let NativeEmit::Stmt(s) = out else { panic!("want Stmt") };
        // tempdir = mkdtemp underneath (mode 0700, TMPDIR), and the
        // directory is KEPT for the script's own rm -rf.
        assert!(s.contains("File::Temp::tempdir"), "{s}");
        assert!(s.contains("CLEANUP => 0"), "{s}");
        assert!(s.contains("$CHILD_ERROR = 0"), "{s}");
    }

    #[test]
    fn mktemp_unknown_args_or_in_condition_are_refused() {
        // flags we do not model keep the shell path.
        for flag in ["--tmpdir", "-t", "-q"] {
            let c = mktemp_call(vec![IrExpr::Str(flag.to_string(), StrStyle::DoubleQuoted)]);
            assert!(
                emit(&NativeCtx::Exec { call: &c, cond: false }).is_none(),
                "{flag} must be refused"
            );
        }
        // a TEMPLATE operand (mktemp -d /tmp/xXXXXXXX has two words —
        // refused; single-word template likewise).
        let tpl = mktemp_call(vec![IrExpr::Str("tplXXXXXX".to_string(), StrStyle::DoubleQuoted)]);
        assert!(emit(&NativeCtx::Exec { call: &tpl, cond: false }).is_none());
        // condition position is never a value channel.
        let bare = mktemp_call(vec![]);
        assert!(emit(&NativeCtx::Exec { call: &bare, cond: true }).is_none());
    }
}
