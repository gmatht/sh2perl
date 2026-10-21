//! Capability: single-command value producers the perl backend should never
//! fork for — `uname [FLAG]`, `date +FORMAT`, `sha256sum/sha512sum FILE`.
//!
//! ## Need
//!
//! Each reaches the value channel as one simple command and currently costs
//! a `__sh_uu_capture` fork/exec (corpus: 8 uname, 4 sha, 2 date sites):
//!
//! ```text
//! os=$(uname -s)   → __sh_uu_capture('uname', '-s')
//! sum=$(sha256sum test.txt) → __sh_uu_capture('sha256sum', 'test.txt')
//! d=$(date +%Y-%m-%d)       → __sh_uu_capture('date', '+%Y-%m-%d')
//! ```
//!
//! All three have exact core-Perl equivalents — no approximation:
//!
//! - **uname** — `POSIX::uname()` IS the uname(2) syscall: the same five
//!   fields coreutils prints (`sysname nodename release version machine`).
//!   Single flag (`-s`/`-n`/`-r`/`-v`/`-m`) or bare (POSIX default = `-s`).
//! - **date +FORMAT** — coreutils date formats through the SAME glibc
//!   `strftime` this uses. Refused: `%N` (nanoseconds — date's own
//!   extension, not strftime) and `%q` (quarter — likewise date-only), and
//!   the pad modifiers (`%-d`, `%_d`, `%0d`, `%:z`) which GNU date expands
//!   before calling strftime and strftime itself does not understand.
//!   Locale conversions (`%c`/`%x`/`%X`) go through glibc in both → same
//!   bytes.
//! - **sha256sum/sha512sum FILE** — `Digest::SHA` is a core module and the
//!   same algorithm; GNU's default BINARY mode is `<:raw` + two spaces
//!   before the filename. The miss-file path reproduces coreutils' stderr
//!   line (`sha256sum: FILE: No such file or directory`) and exit 1.
//!
//! ## Refusals
//!
//! Anything else: `uname -a`/`-p`/multi-flag, a bare `date` (ctime string),
//! `date -d`/`-u`/other options, sha `-c`/`-b`/multiple files/stdin (no
//! operand), and every other command — those keep the uu-ffi seam.
//!
//! The corpus's clock-dependent sites (`date +%Y-%m-%d`) are date-only
//! formats, so no tick race exists between the two sides of the gate.

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
    let perl = match cmd.as_str() {
        "uname" => uname(words)?,
        "date" => date(words)?,
        "sha256sum" | "sha512sum" => sha(cmd, words)?,
        _ => return None,
    };
    Some(NativeEmit::Stmt(format!(
        "$__sh_cap = {perl}; $main_exit_code = $CHILD_ERROR = 0;"
    )))
}

/// `uname` — the five uname(2) fields via POSIX::uname (list order: sysname,
/// nodename, release, version, machine). Bare = `-s` (POSIX default).
fn uname(words: &[crate::ir::IrExpr]) -> Option<String> {
    let idx = match words {
        [] => 0usize, // POSIX: no options prints sysname
        [crate::ir::IrExpr::Str(flag, _)] => match flag.as_str() {
            "-s" => 0,
            "-n" => 1,
            "-r" => 2,
            "-v" => 3,
            "-m" => 4,
            _ => return None, // -a/-p/-i/-o, multi-flag, long flags
        },
        _ => return None,
    };
    Some(format!(
        "(require POSIX, (POSIX::uname())[{idx}])"
    ))
}

/// `date +FORMAT` — glibc strftime, byte-identical to coreutils date's own
/// formatting for every conversion strftime understands.
fn date(words: &[crate::ir::IrExpr]) -> Option<String> {
    let [crate::ir::IrExpr::Str(spec, _)] = words else {
        return None; // bare `date`, -d/-u/… options: refused
    };
    let fmt = spec.strip_prefix('+')?; // must be a FORMAT operand
    // Walk the format: every conversion after '%' must be one of the
    // shared glibc/date spellings (no %N / %q / pad modifiers).
    let b = fmt.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let next = *b.get(i + 1)?;
            if next == b'%' {
                i += 2;
                continue;
            }
            // GNU pad modifiers (`%-`, `%_`, `%0`, `%:`) and the two
            // date-only conversions are refused.
            if matches!(next, b'-' | b'_' | b'0' | b':') {
                return None;
            }
            let ch = next as char;
            if !(ch.is_ascii_alphabetic() || ch == '%') || matches!(ch, 'N' | 'q') {
                return None;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    Some(format!(
        "(require POSIX, POSIX::strftime({q}, localtime))",
        q = crate::ir::safe_perl_q_string(fmt)
    ))
}

/// `sha256sum FILE` / `sha512sum FILE` — Digest::SHA (core), GNU binary
/// mode: `<hash>  <file>` (two spaces). On open failure coreutils prints
/// `<cmd>: <file>: <errno>` on stderr and exits 1 with empty stdout —
/// reproduce exactly (stderr is not captured by `$( )` either way).
fn sha(cmd: &str, words: &[crate::ir::IrExpr]) -> Option<String> {
    let bits = if cmd == "sha256sum" { 256 } else { 512 };
    let [crate::ir::IrExpr::Str(file, _)] = words else {
        return None; // no operand (stdin mode), flags, multiple files
    };
    // A leading-dash word is an OPTION to GNU sha256sum (`-c` check mode,
    // `-b`/`-t`), which errors on stdout-empty + exit 1 — the real binary
    // through the uu seam reproduces that, so refuse rather than hash a
    // file literally named "-c".
    if file.starts_with('-') {
        return None;
    }
    let fq = crate::ir::safe_perl_q_string(file);
    let cq = crate::ir::safe_perl_q_string(cmd);
    Some(format!(
        "do {{ require Digest::SHA; if (open(my $__fh, '<:raw', {fq})) {{ my $__h = Digest::SHA->new({bits}); $__h->addfile($__fh); close $__fh; $__h->hexdigest() . q{{  }} . {fq} }} else {{ print STDERR {cq} . q{{: }} . {fq} . q{{: }} . $ERRNO . qq{{\\n}}; $main_exit_code = $CHILD_ERROR = 1; '' }} }}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, StrStyle};

    fn call(cmd: &str, words: Vec<IrExpr>) -> IrExpr {
        IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str(cmd.to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(words),
            ],
        }
    }

    fn lower(cmd: &str, words: Vec<IrExpr>) -> Option<String> {
        let c = call(cmd, words);
        match emit(&NativeCtx::Exec { call: &c, cond: false })? {
            NativeEmit::Stmt(s) => Some(s),
            _ => None,
        }
    }

    #[test]
    fn uname_fields_map_to_posix_uname() {
        let s = lower("uname", vec![]).expect("bare uname lowers");
        assert!(s.contains("(POSIX::uname())[0]"), "{s}"); // default = -s
        for (flag, idx) in [("-s", 0), ("-n", 1), ("-r", 2), ("-v", 3), ("-m", 4)] {
            let s = lower("uname", vec![IrExpr::Str(flag.into(), StrStyle::DoubleQuoted)])
                .unwrap_or_else(|| panic!("{flag} must lower"));
            assert!(s.contains(&format!("(POSIX::uname())[{idx}]")), "{s}");
        }
        // -a / -p / multi-flag keep the shell path.
        assert!(lower("uname", vec![IrExpr::Str("-a".into(), StrStyle::DoubleQuoted)]).is_none());
        assert!(lower(
            "uname",
            vec![
                IrExpr::Str("-s".into(), StrStyle::DoubleQuoted),
                IrExpr::Str("-m".into(), StrStyle::DoubleQuoted)
            ]
        )
        .is_none());
    }

    #[test]
    fn date_format_shared_conversions_lower() {
        let s = lower("date", vec![IrExpr::Str("+%Y-%m-%d".into(), StrStyle::DoubleQuoted)])
            .expect("date +%Y-%m-%d lowers");
        assert!(s.contains("POSIX::strftime"), "{s}");
        assert!(s.contains("%Y-%m-%d"), "{s}");
        // %N and %q are date-only extensions — refused.
        assert!(lower("date", vec![IrExpr::Str("+%Y-%m-%d %N".into(), StrStyle::DoubleQuoted)]).is_none());
        assert!(lower("date", vec![IrExpr::Str("+%q".into(), StrStyle::DoubleQuoted)]).is_none());
        // GNU pad modifiers are refused (strftime would not expand them).
        assert!(lower("date", vec![IrExpr::Str("+%-d".into(), StrStyle::DoubleQuoted)]).is_none());
        // bare date / options refused.
        assert!(lower("date", vec![]).is_none());
        assert!(lower("date", vec![IrExpr::Str("-u".into(), StrStyle::DoubleQuoted)]).is_none());
    }

    #[test]
    fn sha_single_file_lowers_and_errors_refused() {
        let s = lower("sha256sum", vec![IrExpr::Str("a.txt".into(), StrStyle::DoubleQuoted)])
            .expect("sha256sum file lowers");
        // The module must be `require`d inline (a leaf cannot push imports):
        // without it the method call dies at runtime — and `do FILE` in the
        // gate runner surfaces that as a silently truncated program.
        assert!(s.contains("require Digest::SHA"), "{s}");
        assert!(s.contains("Digest::SHA->new(256)"), "{s}");
        assert!(s.contains("'<:raw'"), "{s}"); // GNU default = binary mode
        assert!(s.contains("sha256sum"), "{s}"); // stderr line names the tool
        let s = lower("sha512sum", vec![IrExpr::Str("a.txt".into(), StrStyle::DoubleQuoted)])
            .expect("sha512sum file lowers");
        assert!(s.contains("Digest::SHA->new(512)"), "{s}");
        // stdin mode (no operand), -c check mode, multiple files: refused.
        assert!(lower("sha256sum", vec![]).is_none());
        assert!(lower(
            "sha256sum",
            vec![IrExpr::Str("-c".into(), StrStyle::DoubleQuoted)]
        )
        .is_none());
        assert!(lower("sha256sum", vec![
            IrExpr::Str("a".into(), StrStyle::DoubleQuoted),
            IrExpr::Str("b".into(), StrStyle::DoubleQuoted)
        ])
        .is_none());
    }
}
