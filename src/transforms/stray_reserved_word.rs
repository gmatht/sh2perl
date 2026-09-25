//! stray-reserved-word — a command whose NAME is a bare shell grammar token
//! (`)` / `}`) is a bash syntax error; lower it (and everything after it) to
//! the abort, preserving the statements that bash already ran.
//!
//! ## Need
//!
//! bash treats `)` and `}` as reserved words in grammar positions. A stray
//! one is a *parse* error: bash runs the lines before it, prints
//! `syntax error near unexpected token ...` on stderr, and exits 2 —
//! discarding the rest of the script.
//!
//! The core parser does not reject them: it recovers a stray top-level `)`
//! as `SimpleCommand { name: Word::literal(")") }` (parser/commands.rs,
//! "the ESTree runner's stray-`)` path then matches bash"), and the same
//! shape reaches the IR for `}`. Every backend then treats `)` as a program
//! to EXECUTE:
//!
//! ```text
//! perl → system('bash', '-c', q(')')); $? >> 8    → rc 127
//! shir → {"func":"exec","args":[{"Str":")"},{"Array":[]}]}
//! ```
//!
//! So the gate saw `exit code (bash=2 perl=127)`
//! (parse-unexpected-parenclose.sh) and `stdout mismatch, exit code
//! (bash=2 perl=0)` (parse-unexpected-braceclose.sh) — bash has already
//! printed the preceding `echo "hello"` in both.
//!
//! ## Lowering
//!
//! At the first stray reserved word: keep the statements BEFORE it (bash
//! already ran them — their output must survive), then replace it and the
//! whole tail with `IrStmt::Exit(Some(2))`. The diagnostic goes to stderr
//! the way bash prints it.
//!
//! ## Placement
//!
//! A shared transform (`transforms.rs`), like the rest of the marketplace:
//! "a stray reserved word is a syntax error" is a property of the SOURCE
//! language, not of any one backend, so every renderer should inherit it.
//!
//! ## Semantics / limits
//!
//! - Only a command NAME is inspected. `)` / `}` as a *quoted* word
//!   (`echo ')'`, `echo ")"`) is a legal argument in bash and arrives as a
//!   Str/Interpolate inside an `Array`, never as the command name — so
//!   those are untouched.
//! - An unquoted `)` used as an *argument* (`echo hi )`) is dropped by the
//!   parser before we see it (the args array is empty), so this transform
//!   cannot catch that variant; it stays a core-parser gap.
//! - The stderr text mirrors bash's shape but not its byte-exact wording
//!   (bash quotes the token and the offending line). The gate compares
//!   stdout and exit status, not stderr.

use crate::ir::{IrExpr, IrStmt, StrStyle};

/// A bare shell grammar token that can never be a command name.
fn stray_reserved_word(cmd: &str) -> bool {
    matches!(cmd, ")" | "}")
}

/// A literal that round-trips through the A1 contract: `StrStyle::Raw` has
/// no wire representation (the deserializer's `_` arm maps it to
/// `DoubleQuoted`), and shir_json_in's `corpus_roundtrip_byte_equal` test
/// fails on the drift. `SingleQuoted` is on the wire AND renders as a Perl
/// single-quoted literal (no `$`/`@` interpolation) — which matters for the
/// diagnostic, whose token text can contain either.
fn st(s: &str) -> IrExpr {
    IrExpr::Str(s.to_string(), StrStyle::SingleQuoted)
}

/// The command-name word of an exec/builtin call, if literal.
fn exec_name(e: &IrExpr) -> Option<&str> {
    let IrExpr::Call { func, args } = e else {
        return None;
    };
    if !matches!(func.as_str(), "exec" | "builtin") {
        return None;
    }
    match args.first() {
        Some(IrExpr::Str(cmd, _)) => Some(cmd.as_str()),
        _ => None,
    }
}

/// The unconditional abort statement: bash's syntax-error verdict.
fn syntax_error_exit(token: &str) -> IrStmt {
    IrStmt::Block(vec![
        IrStmt::Warn {
            expr: st(&format!(
                "syntax error near unexpected token `{}'",
                token.replace('\'', "\\'")
            )),
            carp: false,
        },
        IrStmt::Exit(Some(IrExpr::Int(2))),
    ])
}

pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let Some(pos) = stmts.iter().position(|s| match s {
        IrStmt::Expr(e) => exec_name(e).is_some_and(stray_reserved_word),
        _ => false,
    }) else {
        return false;
    };
    let IrStmt::Expr(e) = &stmts[pos] else {
        return false;
    };
    let token = exec_name(e).unwrap_or(")").to_string();
    // keep the prefix (bash already ran it), drop the stray word and tail
    stmts.truncate(pos);
    stmts.push(syntax_error_exit(&token));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt};

    fn echo(word: &str) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Str(word.to_string(), StrStyle::DoubleQuoted)]),
            ],
        })
    }

    fn exec_word(word: &str) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str(word.to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![]),
            ],
        })
    }

    #[test]
    fn stray_parenclose_keeps_prefix_and_exits_2() {
        let mut stmts = vec![echo("hello"), exec_word(")"), echo("never")];
        assert!(transform(&mut stmts));
        // prefix survives, tail is replaced by the abort
        assert_eq!(stmts.len(), 2);
        assert!(matches!(&stmts[0], IrStmt::Expr(IrExpr::Call { func, .. }) if func == "builtin"));
        let IrStmt::Block(body) = &stmts[1] else { panic!("expected Block") };
        assert!(matches!(body.last(), Some(IrStmt::Exit(Some(IrExpr::Int(2))))));
    }

    #[test]
    fn stray_braceclose_is_caught() {
        let mut stmts = vec![echo("hi"), exec_word("}")];
        assert!(transform(&mut stmts));
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn quoted_and_ordinary_words_are_untouched() {
        // a quoted `)` is an ARGUMENT (inside the Array), not the name
        let mut stmts = vec![echo(")"), echo("done")];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 2);
        let mut stmts = vec![exec_word("date")];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
    }
}
