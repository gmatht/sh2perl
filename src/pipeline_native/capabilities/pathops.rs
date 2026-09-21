//! Capability: `pwd`, `basename PATH`, `dirname PATH` — pure path
//! operations the perl backend should never shell out for.
//!
//! ## Need
//!
//! check_qx flags `bash/sh -c` wrapping a builtin; these three are in the
//! builtin/shellout list (`pwd`, `basename`, `dirname`). They reach the
//! capture path as single commands, e.g.
//!
//! ```text
//! b=$(basename "$0")      → system('bash','-c', q{'basename' "$0"})
//! d=$(dirname "$0")       → ...                              (rc 127 risk)
//! h=$(pwd)                → ...
//! ```
//!
//! The command text is split by `sh_split_simple_cmd`, which refuses shell
//! metacharacters — so this only fires for a genuinely simple command
//! (a literal or a `$var`-free word). That is deliberate: a word carrying
//! `$(…)` or a glob needs the shell.
//!
//! ## Lowering
//!
//! - `pwd` → `Cwd::getcwd()` (the same value the runtime's `__sh_pwd`
//!   uses); no args.
//! - `basename PATH` → strip any trailing slashes, then everything up to
//!   the last `/`; `basename PATH SUFFIX` also strips a trailing SUFFIX.
//! - `dirname PATH` → everything before the last `/`, with bash's rules
//!   for a path with no slash (`.`) and for `/` (itself).
//!
//! ## Refusals
//!
//! Flags (`basename -s`, `dirname -z`, …), a non-literal path (the caller
//! only offers us literal words), and any operand count we do not model.

use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    // `NativeCtx::Exec` is the exec-call shape; the capture parent
    // synthesizes one from the command text.
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
    let words: Vec<String> = words
        .iter()
        .map(crate::ir::grep_lit_str)
        .collect::<Option<_>>()?;
    let perl = match cmd.as_str() {
        // `pwd` — the physical working directory. `Cwd::getcwd` is
        // require'd inline so no preamble import is needed (a capability
        // leaf cannot push to the `imports` list).
        "pwd" if words.is_empty() => {
            "(require Cwd, Cwd::getcwd())".to_string()
        }
        "basename" if words.len() == 1 => basename(&words[0], None)?,
        "basename" if words.len() == 2 => basename(&words[0], Some(&words[1]))?,
        "dirname" if words.len() == 1 => dirname(&words[0])?,
        _ => return None,
    };
    // The capture parent reads `$__sh_cap`; the VALUE form is the same
    // expression, so a caller that only wants the expression can use it
    // too. No stdout is written — the parent decides how to consume it.
    Some(NativeEmit::Stmt(format!(
        "$__sh_cap = {perl}; $main_exit_code = $CHILD_ERROR = 0;"
    )))
}

/// A perl string literal for `s` (single-quoted unless it contains a quote).
fn lit(s: &str) -> String {
    format!("q{{{}}}", s.replace('\\', "\\\\").replace('}', "\\}"))
}

/// `basename PATH [SUFFIX]` — bash strips trailing slashes, takes the
/// component after the last slash, then removes a trailing SUFFIX (only
/// when it is not the whole name).
fn basename(path: &str, suffix: Option<&str>) -> Option<String> {
    let mut p = path;
    while p.len() > 1 && p.ends_with('/') {
        p = &p[..p.len() - 1];
    }
    let name = match p.rfind('/') {
        Some(i) => &p[i + 1..],
        None => p,
    };
    if name.is_empty() {
        return None; // `basename /` prints `/`; refuse rather than guess
    }
    let stripped = match suffix {
        Some(sfx) if !sfx.is_empty() && name != sfx && name.ends_with(sfx) => {
            &name[..name.len() - sfx.len()]
        }
        _ => name,
    };
    Some(lit(stripped))
}

/// `dirname PATH` — everything before the last slash (trailing slashes are
/// removed first); no slash → `.`; a leading `/` with nothing else → `/`.
fn dirname(path: &str) -> Option<String> {
    let mut p = path;
    while p.len() > 1 && p.ends_with('/') {
        p = &p[..p.len() - 1];
    }
    match p.rfind('/') {
        None => Some(lit(".")),
        Some(0) => Some(lit("/")),
        Some(i) => {
            let mut head = &p[..i];
            while head.len() > 1 && head.ends_with('/') {
                head = &head[..head.len() - 1];
            }
            Some(lit(head))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_and_dirname_match_bash() {
        // bash: basename /a/b/c.txt → c.txt ; dirname → /a/b
        assert_eq!(basename("/a/b/c.txt", None).unwrap(), "q{c.txt}");
        assert_eq!(dirname("/a/b/c.txt").unwrap(), "q{/a/b}");
        // no slash
        assert_eq!(dirname("x.txt").unwrap(), "q{.}");
        assert_eq!(basename("x.txt", None).unwrap(), "q{x.txt}");
        // trailing slashes
        assert_eq!(basename("/a/b/", None).unwrap(), "q{b}");
        assert_eq!(dirname("/a/b/").unwrap(), "q{/a}");
        // root
        assert_eq!(dirname("/").unwrap(), "q{/}");
        // suffix
        assert_eq!(basename("/a/b/c.txt", Some(".txt")).unwrap(), "q{c}");
        // suffix equal to the whole name is not stripped
        assert_eq!(basename("txt", Some("txt")).unwrap(), "q{txt}");
    }
}
