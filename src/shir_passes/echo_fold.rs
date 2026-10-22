//! EchoFold — lower a constant `echo` to a native `IrStmt::Output`.
//!
//! A shell `echo` arrives from the frontends as
//! `IrStmt::Expr(Call { func: "exec"|"builtin", args: [Str("echo"), Array(words)] })`.
//! Every backend then has to re-implement the echo semantics (`-n`/`-e`/
//! `-E` flags, word joining, the trailing newline) — and the ones that
//! do it through a runtime helper (`__sh_print_words`, `sh2.printWords`)
//! pay for it at run time even when the whole command is a literal.
//!
//! When all words are compile-time constants (a bare literal, or a
//! quoted string whose only part is a literal — no expansion, no glob,
//! no arith guard) the entire output is known at transform time. The
//! fold rewrites it to
//!
//! ```text
//! IrStmt::Output { value: IrExpr::Str(text, DoubleQuoted), newline, target: None }
//! ```
//!
//! whose text already includes the space-joined words, the `-e`
//! unescaping and (when `newline`) the trailing `\n`. `IrStmt::Output`
//! is the node every backend renders natively (`print`/`say`, `print!`,
//! `System.out.print`, `fmt.Fprint`, …), so the helper is skipped in
//! every backend at once instead of each renderer re-deriving the
//! peephole.
//!
//! # Why this is sound
//!
//! * Only literal words fold; any `$var`/`$(...)`/glob word leaves the
//!   statement untouched (the runtime helper keeps its job).
//! * `echo`'s flags are only recognised in leading position and only
//!   from literal words, matching the shell's own rule.
//! * `-e` unescaping reuses [`echo_esc`], the same state machine the
//!   `--shir` contract documents for echo; the fold and any runtime
//!   unescaper must agree (they are kept in lockstep).
//! * `echo` always succeeds (status 0). The folded `Output` carries that
//!   status implicitly (it cannot fail); renderers that track `$?` treat
//!   `Output` as status 0, so dropping the `exec` call cannot change the
//!   script's exit status.

use crate::ir::{InterpPart, IrExpr, IrProgram, IrStmt, StrStyle};

/// Fold constant `echo`s into native `Output` statements — see the
/// module docs.
pub struct EchoFold;

impl super::Transform for EchoFold {
    fn name(&self) -> &'static str {
        "echo_fold"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &super::PassContext) {
        fold_stmts(&mut prog.stmts);
        for sub in &mut prog.subs {
            fold_stmts(&mut sub.body);
        }
    }
}

fn fold_stmts(stmts: &mut [IrStmt]) {
    for st in stmts.iter_mut() {
        fold_stmt(st);
    }
}

fn fold_stmt(st: &mut IrStmt) {
    // recurse first so a nested constant echo also folds, and so a
    // folded `Output` inside a block is left alone (it is already native)
    match st {
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            fold_stmts(then);
            for (_, b) in elsifs.iter_mut() {
                fold_stmts(b);
            }
            fold_stmts(else_);
        }
        IrStmt::For { body, .. } | IrStmt::While { body, .. } => fold_stmts(body),
        IrStmt::DoWhile { body, .. } => fold_stmts(body),
        IrStmt::Block(b) => fold_stmts(b),
        IrStmt::Redirect { inner, .. } => fold_stmts(inner),
        _ => {}
    }
    if let Some(out) = constant_echo(st).or_else(|| constant_printf(st)) {
        *st = out;
    }
}

/// `Some(Output)` when `st` is a constant, ZERO-argument `printf` whose
/// format carries no conversion specifiers.
///
/// The Rust renderer wraps every printf in `__sh_print_words(&[vec!
/// [__sh_printf(…)]]…)` plus the `__sh_printf`/`__sh_q_printf`/`__sh_atoi`
/// polyfill cluster even when the whole output is a compile-time literal
/// (`printf "a\n"` — 70 such sites corpus-wide). This fold lowers that
/// shape to the same native `IrStmt::Output` the constant-`echo` fold
/// produces, so every backend prints it directly and the helper prune
/// drops the whole cluster wherever no other printf survives.
///
/// Soundness — every refusal keeps the statement on the runtime path:
///   * exactly ONE word: no args to service, no format-reuse rounds
///     (bash re-applies the format per remaining argument), no
///     `-v var fmt` target write;
///   * no `%` in the format: with zero args the conversions still
///     produce deterministic text (`%s` → "", `%d` → 0) — but that is
///     the runtime helper's contract to keep, not this fold's to
///     re-derive;
///   * no glob sentinel: `SH2GLOB` is the core's EXPAND-me
///     instruction, never literal text (the 064 lesson);
///   * `-v` alone is a usage error the renderer prints verbatim — leave
///     that path byte-identical.
///
/// The escape processing is [`echo_esc`], NOT a second implementation:
/// the runtime `__sh_printf` body and `__sh_echo_esc` carry the
/// line-for-line same table (`\n \t \r \a \b \f \v \e \\ \c`-truncates,
/// `\0nnn`, `\xHH`, incomplete-`\x` → `x`), so folding with `echo_esc`
/// is lockstep with what the helper would have produced. `newline:
/// false` — printf appends no newline of its own (the `\n` lives inside
/// the format, and `echo_esc` has already materialized it), and Output's
/// status-0 contract matches printf's success status.
fn constant_printf(st: &IrStmt) -> Option<IrStmt> {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return None;
    };
    if func != "exec" && func != "builtin" {
        return None;
    }
    let IrExpr::Str(cmd, _) = args.first()? else {
        return None;
    };
    if cmd != "printf" {
        return None;
    }
    let IrExpr::Array(words) = args.get(1)? else {
        return None;
    };
    let [fmt_word] = words.as_slice() else {
        return None;
    };
    if let IrExpr::Str(f, _) = fmt_word {
        if f == "-v" {
            return None;
        }
    }
    let text = const_word_text(fmt_word)?;
    if text.contains('%') {
        return None;
    }
    if text.contains('\u{1}') {
        return None;
    }
    Some(IrStmt::Output {
        value: IrExpr::Str(echo_esc(&text), StrStyle::DoubleQuoted),
        newline: false,
        target: None,
    })
}

/// `Some(Output)` when `st` is a constant `echo` — see [`fold_to_output`].
fn constant_echo(st: &IrStmt) -> Option<IrStmt> {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return None;
    };
    if func != "exec" && func != "builtin" {
        return None;
    }
    let IrExpr::Str(cmd, _) = args.first()? else {
        return None;
    };
    if cmd != "echo" {
        return None;
    }
    let IrExpr::Array(words) = args.get(1)? else {
        return None;
    };
    // an all-literal echo folds to a precomputed string; a single
    // INTEGER-valued word folds to an Output carrying the value (so the
    // print path formats the number directly instead of round-tripping
    // through `.to_string()`).
    fold_to_output(words).or_else(|| fold_numeric_echo(args))
}

/// The constant-echo fold: join the (all-literal) words, apply the
/// leading `-n`/`-e`/`-E` flags, append the trailing newline. `None`
/// when any word is not a compile-time constant (so the caller leaves
/// the statement for the runtime helper).
fn fold_to_output(words: &[IrExpr]) -> Option<IrStmt> {
    let mut nl = true;
    let mut esc = false;
    let mut idx = 0;
    while let Some(IrExpr::Str(f, _)) = words.get(idx) {
        match f.as_str() {
            "-n" => nl = false,
            "-e" => esc = true,
            "-E" => esc = false,
            "--" => {
                idx += 1;
                break;
            }
            _ => break,
        }
        idx += 1;
    }
    let mut text = String::new();
    let mut first = true;
    for w in &words[idx..] {
        let t = const_word_text(w)?;
        if !first {
            text.push(' ');
        }
        first = false;
        text.push_str(&t);
    }
    if esc {
        text = echo_esc(&text);
    }
    // `Output.newline` carries the trailing newline (its documented
    // meaning — every backend appends it), so the folded text holds only
    // the joined words. Baking `\n` into the text AND setting the flag
    // would double it.
    Some(IrStmt::Output {
        value: IrExpr::Str(text, StrStyle::DoubleQuoted),
        newline: nl,
        target: None,
    })
}

/// Fold a single-word `echo` whose word is an INTEGER by construction
/// into a native `Output` carrying the value itself.
///
/// `echo <int-expr>` is a print of a number: leaving it an exec call makes
/// every renderer stringify first (`__sh_len(&s).to_string()`), only for
/// the print to format it back. `Output { value, newline: true }` lets each
/// backend print the value directly (`println!("{}", __sh_len(&s))`).
///
/// Conservative: exactly `echo` with ONE word (a single word has no
/// separator, so word joining cannot matter) and a value that is numeric
/// BY CONSTRUCTION — a bare `Int`/`Arith`, or the int-returning builtins
/// (`param("len", …)`, `arrayLen`). Anything else refuses; the string path
/// is correct, just noisier.
fn fold_numeric_echo(args: &[IrExpr]) -> Option<IrStmt> {
    let IrExpr::Str(cmd, _) = args.first()? else {
        return None;
    };
    if cmd != "echo" {
        return None;
    }
    let IrExpr::Array(words) = args.get(1)? else {
        return None;
    };
    let [word] = words.as_slice() else {
        return None;
    };
    if !numeric_word(word) {
        return None;
    }
    Some(IrStmt::Output {
        value: word.clone(),
        newline: true,
        target: None,
    })
}

/// Is this word a number by construction (so a print can format it with no
/// stringification round-trip)? A bare `Int`/`Arith`, and the builtins that
/// return an integer.
pub fn numeric_word(w: &IrExpr) -> bool {
    match w {
        IrExpr::Int(_) | IrExpr::Arith(_) => true,
        IrExpr::Call { func, args } => match func.as_str() {
            "arrayLen" => true,
            // `param(op, …)`: only the LENGTH op yields an integer
            "param" => matches!(args.first(), Some(IrExpr::Str(op, _)) if op == "len"),
            _ => false,
        },
        _ => false,
    }
}

/// The compile-time text of a word, or `None` when it involves any
/// expansion/glob. A bare `Str` and an `Interpolate` whose parts are ALL
/// literals both fold (a quoted `"s1"` arrives as a one-part
/// Interpolate).
fn const_word_text(w: &IrExpr) -> Option<String> {
    match w {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(t) => out.push_str(t),
                    InterpPart::Expr(_) => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// bash `echo -e` backslash unescaping (`\n \t \r \a \b \f \v \e \\ \c
/// \0NNN \xHH`). This is the SHARED definition of the fold's `-e`
/// semantics: the `--shir` contract and the backends' runtime
/// unescapers must agree with it (`\c` truncates; an incomplete escape
/// is left as-is).
pub fn echo_esc(s: &str) -> String {
    let mut o = String::new();
    let ch: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < ch.len() {
        if ch[i] == '\\' && i + 1 < ch.len() {
            i += 1;
            match ch[i] {
                'n' => o.push('\n'),
                't' => o.push('\t'),
                'r' => o.push('\r'),
                'a' => o.push('\x07'),
                'b' => o.push('\x08'),
                'f' => o.push('\x0c'),
                'v' => o.push('\x0b'),
                'e' => o.push('\x1b'),
                '\\' => o.push('\\'),
                'c' => return o,
                '0'..='7' => {
                    let mut n = 0;
                    let mut k = 0;
                    while k < 3 && i + k < ch.len() && ch[i + k].is_ascii_digit() && ch[i + k] <= '7' {
                        n = n * 8 + (ch[i + k] as u32 - '0' as u32);
                        k += 1;
                    }
                    o.push(char::from_u32(n & 0xff).unwrap_or('\0'));
                    i += k - 1;
                }
                'x' => {
                    let mut n = 0;
                    let mut k = 0;
                    while k < 2 && i + 1 + k < ch.len() && ch[i + 1 + k].is_ascii_hexdigit() {
                        n = n * 16 + ch[i + 1 + k].to_digit(16).unwrap();
                        k += 1;
                    }
                    if k > 0 {
                        o.push(char::from_u32(n).unwrap_or('\0'));
                        i += k;
                    } else {
                        o.push('x');
                    }
                }
                c => {
                    o.push('\\');
                    o.push(c);
                }
            }
            i += 1;
        } else {
            o.push(ch[i]);
            i += 1;
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(v: &[&str]) -> Vec<IrExpr> {
        v.iter()
            .map(|s| IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted))
            .collect()
    }

    #[test]
    fn literal_echo_folds_to_output() {
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(words(&["hello"])),
            ],
        });
        let out = constant_echo(&st).expect("folds");
        match out {
            IrStmt::Output { value, newline, target } => {
                assert!(newline);
                assert_eq!(target, None);
                assert_eq!(value, IrExpr::Str("hello".to_string(), StrStyle::DoubleQuoted));
            }
            other => panic!("expected Output, got {other:?}"),
        }
    }

    #[test]
    fn flags_and_join() {
        let st = IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(words(&["-n", "a", "b"])),
            ],
        });
        let IrStmt::Output { value, newline, .. } = constant_echo(&st).unwrap() else {
            panic!("expected Output")
        };
        assert!(!newline);
        assert_eq!(value, IrExpr::Str("a b".to_string(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn dash_e_unescapes() {
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(words(&["-e", "x\\ty"])),
            ],
        });
        let IrStmt::Output { value, .. } = constant_echo(&st).unwrap() else {
            panic!("expected Output")
        };
        assert_eq!(value, IrExpr::Str("x\ty".to_string(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn expansion_word_does_not_fold() {
        // `echo "$x"` — the word is an Interpolate with an Expr part
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Interpolate(vec![InterpPart::Expr(
                    Box::new(IrExpr::Var("x".to_string(), None)),
                )])]),
            ],
        });
        assert!(constant_echo(&st).is_none());
    }

    #[test]
    fn quoted_literal_interpolate_folds() {
        // `echo "s1"` arrives as a one-part Interpolate
        let st = IrStmt::Expr(IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![
                IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Interpolate(vec![InterpPart::Lit(
                    "s1".to_string(),
                )])]),
            ],
        });
        let IrStmt::Output { value, .. } = constant_echo(&st).unwrap() else {
            panic!("expected Output")
        };
        assert_eq!(value, IrExpr::Str("s1".to_string(), StrStyle::DoubleQuoted));
    }

    #[test]
    fn non_echo_exec_does_not_fold() {
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("printf".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(words(&["hi"])),
            ],
        });
        assert!(constant_echo(&st).is_none());
    }

    #[test]
    fn echo_esc_matches_bash_common_cases() {
        assert_eq!(echo_esc("a\\nb"), "a\nb");
        assert_eq!(echo_esc("a\\\\b"), "a\\b");
        assert_eq!(echo_esc("ab\\cde"), "ab"); // \c truncates
        assert_eq!(echo_esc("\\101"), "A"); // octal
    }
}

#[cfg(test)]
mod printf_tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    fn printf_stmt(words: &[&str]) -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "exec".into(),
            args: vec![
                IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(
                    words
                        .iter()
                        .map(|s| IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted))
                        .collect(),
                ),
            ],
        })
    }

    #[test]
    fn constant_printf_folds_to_output() {
        // the format's `\n` is materialized by echo_esc (lockstep with
        // the runtime __sh_printf body); `newline: false` because printf
        // appends no newline of its own.
        let out = constant_printf(&printf_stmt(&["a\\nb\\n"]))
            .expect("a literal zero-arg printf must fold");
        match out {
            IrStmt::Output { value, newline, target } => {
                assert!(!newline);
                assert_eq!(target, None);
                assert_eq!(
                    value,
                    IrExpr::Str("a\nb\n".to_string(), StrStyle::DoubleQuoted)
                );
            }
            other => panic!("expected Output, got {other:?}"),
        }
    }

    #[test]
    fn percent_format_refuses() {
        // with zero args the conversions are still the runtime helper's
        // contract to keep — the fold must not re-derive them
        assert!(constant_printf(&printf_stmt(&["%s\\n"])).is_none());
    }

    #[test]
    fn extra_args_refuse() {
        // bash re-applies the format per remaining argument
        assert!(constant_printf(&printf_stmt(&["x\\n", "y"])).is_none());
        assert!(constant_printf(&printf_stmt(&["%s", "hi"])).is_none());
    }

    #[test]
    fn v_option_refuses() {
        // `-v var fmt` writes to a target, and `-v` alone is a usage
        // error the renderer prints verbatim — leave that path alone
        assert!(constant_printf(&printf_stmt(&["-v"])).is_none());
    }

    #[test]
    fn glob_sentinel_refuses() {
        // \u{1}SH2GLOB\u{1} is the core's EXPAND-me instruction, never
        // literal text (the 064 lesson)
        assert!(constant_printf(&printf_stmt(&["\u{1}SH2GLOB\u{1}"])).is_none());
    }

    #[test]
    fn expansion_word_refuses() {
        let st = IrStmt::Expr(IrExpr::Call {
            func: "builtin".into(),
            args: vec![
                IrExpr::Str("printf".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Interpolate(vec![InterpPart::Expr(
                    Box::new(IrExpr::Var("x".to_string(), None)),
                )])]),
            ],
        });
        assert!(constant_printf(&st).is_none());
    }

    #[test]
    fn non_printf_exec_does_not_fold() {
        // `echo` belongs to constant_echo; printf must not claim it
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".into(),
            args: vec![
                IrExpr::Str("echo".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Str("hi".to_string(), StrStyle::DoubleQuoted)]),
            ],
        });
        assert!(constant_printf(&st).is_none());
    }
}

#[cfg(test)]
mod numeric_tests {
    use super::*;
    use crate::ir::{IrExpr, IrStmt, StrStyle};

    #[test]
    fn numeric_echo_folds() {
        let st = IrStmt::Expr(IrExpr::Call {
            func: "exec".into(),
            args: vec![
                IrExpr::Str("echo".into(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Call {
                    func: "arrayLen".into(),
                    args: vec![IrExpr::Str("a".into(), StrStyle::DoubleQuoted)],
                }]),
            ],
        });
        let out = constant_echo(&st).expect("numeric echo must fold");
        match out {
            IrStmt::Output { newline, .. } => assert!(newline),
            other => panic!("not an Output: {other:?}"),
        }
    }
}
