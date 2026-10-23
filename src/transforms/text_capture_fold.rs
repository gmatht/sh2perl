//! text-capture-fold — fold CONSTANT command substitutions inside the raw
//! shell-text arguments `test`, `param` and `arith` carry.
//!
//! ## Why
//!
//! The parser keeps `[ "$(echo hello)" = hello ]`,
//! `${v:-$(echo dflt)}` and `$(echo …) + 1` as RAW TEXT arguments: the
//! renderer re-parses the text itself. No structured `Capture` node ever
//! exists there, so `const-capture-fold` cannot see these — and every
//! backend re-derives the substitution at RENDER time:
//!
//! - the Perl renderer rebuilds shell text and shells it out through
//!   `__sh_pipe_capture('echo hello', "pipefail")` — one fork + bash -c
//!   per test operand (and twice per operand when `nocasematch` renders
//!   the comparison into both arms);
//! - the estree renderer emits a `sh2.captureSync(…)` runtime call.
//!
//! When the substitution's output is PROVABLY constant and safe in the
//! surrounding text context, replacing it with that literal text leaves
//! the renderer's own re-parse with a constant operand: zero runtime
//! callouts, in every backend, with no renderer change.
//!
//! ## REFUSE > GUESS
//!
//! - only `$(echo WORD…)` / `` `echo WORD…` `` (all words literal — no
//!   `$`, backtick, escape, glob, `~` or unquoted metacharacter, no
//!   `#`-comment, no echo `-n`/`-e`/`-E` flag) and bare `$(true)` /
//!   `$(:)` (empty output). Anything else keeps its runtime form.
//! - inside DOUBLE quotes the replacement must not contain `"`, `\`,
//!   `$` or a backtick — it would end the quote, escape, or re-expand.
//! - OUTSIDE quotes the replacement must be shell-safe
//!   (`[A-Za-z0-9_@%+=:,./-]*`): no whitespace (word-splitting would
//!   change the operand count), no glob or expansion character.
//!   Multi-word output therefore folds only inside double quotes —
//!   exactly where bash also keeps it as ONE word.
//! - single-quoted regions are literal and are never scanned.
//! - `$((…))` arithmetic is balanced as one substitution whose content
//!   is not an `echo` — refused verbatim; a `$(echo …)` NESTED inside it
//!   is still folded (scanned with a fresh quote context), e.g.
//!   `$(( $(echo 3) + 1 ))` → `$(( 3 + 1 ))`.
//!
//! The fold is pure text → text on IR arguments, so it applies to every
//! backend (the §11 default) and runs before the renderers' own re-parse.

use crate::ir::{InterpPart, IrExpr, IrStmt};

/// The functions whose `Str` args are RAW SHELL TEXT the renderers
/// re-parse themselves. Command word args (`echo "…"`) never carry raw
/// text — the parser already lowers those into `Interpolate`/`Capture`.
const TEXT_FUNCS: &[&str] = &["test", "param", "arith"];

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for s in stmts.iter_mut() {
        c |= transform_stmt(s);
    }
    c
}

fn transform_stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut x = transform_expr(cond);
            x |= transform(then);
            for (ec, eb) in elsifs.iter_mut() {
                x |= transform_expr(ec);
                x |= transform(eb);
            }
            x | transform(else_)
        }
        IrStmt::For { iter, body, .. } => transform_expr(iter) | transform(body),
        IrStmt::While { cond, body, .. } => transform(body) | transform_expr(cond),
        IrStmt::DoWhile { body, cond, .. } => transform(body) | transform_expr(cond),
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut x = transform_expr(discriminant);
            for cl in clauses.iter_mut() {
                x |= transform(&mut cl.body);
            }
            x
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => transform(b),
        IrStmt::Function { body, .. } => transform(body),
        IrStmt::Pipeline { stages, .. } => {
            let mut x = false;
            for stg in stages.iter_mut() {
                x |= transform(stg);
            }
            x
        }
        IrStmt::Redirect { inner, redirects } => {
            let mut x = transform(inner);
            for r in redirects.iter_mut() {
                x |= transform_expr(&mut r.target);
            }
            x
        }
        IrStmt::Expr(e) => transform_expr(e),
        IrStmt::Assign { expr, .. } | IrStmt::Output { value: expr, .. } => transform_expr(expr),
        IrStmt::Declare {
            init: Some(expr), ..
        } => transform_expr(expr),
        IrStmt::DeclareArray { elements, .. } => {
            let mut x = false;
            for e in elements.iter_mut() {
                x |= transform_expr(e);
            }
            x
        }
        IrStmt::WriteFile { path, content, .. } => {
            transform_expr(path) | transform_expr(content)
        }
        IrStmt::Exec { cmd, args, env, .. } => {
            let mut x = transform_expr(cmd);
            for a in args.iter_mut() {
                x |= transform_expr(a);
            }
            for (_, v) in env.iter_mut() {
                x |= transform_expr(v);
            }
            x
        }
        _ => false,
    }
}

fn transform_expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Arrow(stmts) => transform(stmts),
        IrExpr::Lambda { body, .. } => transform(body),
        IrExpr::Call { func, args } => {
            let mut c = false;
            if TEXT_FUNCS.contains(&func.as_str()) {
                for a in args.iter_mut() {
                    if let IrExpr::Str(v, _) = a {
                        if let Some(f) = fold_text(v) {
                            *v = f;
                            c = true;
                        }
                    }
                }
            }
            for a in args.iter_mut() {
                c |= transform_expr(a);
            }
            c
        }
        IrExpr::MethodCall { obj, args, .. } => {
            let mut c = transform_expr(obj);
            for a in args.iter_mut() {
                c |= transform_expr(a);
            }
            c
        }
        IrExpr::Array(items) => {
            let mut c = false;
            for a in items.iter_mut() {
                c |= transform_expr(a);
            }
            c
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            let mut c = transform_expr(iter) | transform_expr(elem);
            if let Some(cd) = cond {
                c |= transform_expr(cd);
            }
            c
        }
        IrExpr::Object(pairs) => {
            let mut c = false;
            for (_, v) in pairs.iter_mut() {
                c |= transform_expr(v);
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => transform_expr(lhs) | transform_expr(rhs),
        IrExpr::Ternary { cond, then, else_ } => {
            transform_expr(cond) | transform_expr(then) | transform_expr(else_)
        }
        IrExpr::DefinedOr { expr, default } => transform_expr(expr) | transform_expr(default),
        IrExpr::Index { key, .. } => transform_expr(key),
        IrExpr::Capture { expr, .. } => transform_expr(expr),
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let InterpPart::Expr(x) = p {
                    c |= transform_expr(x);
                }
            }
            c
        }
        _ => false,
    }
}

// ── the text fold ────────────────────────────────────────────────────────

/// One foldable substitution span (absolute indices into `s`).
struct Span {
    start: usize,
    end: usize,
    repl: String,
}

/// Quote state of the surrounding text.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Single,
    Dq,
}

/// Fold every provably-constant substitution in `s`. `None` = unchanged.
pub(crate) fn fold_text(s: &str) -> Option<String> {
    if !s.contains("$(") && !s.contains('`') {
        return None;
    }
    let mut spans = Vec::new();
    scan(s, 0..s.len(), Mode::Normal, &mut spans);
    if spans.is_empty() {
        return None;
    }
    spans.sort_by_key(|sp| sp.start);
    // Disjoint by construction: a FOLDABLE substitution's content cannot
    // contain another substitution (`constant_output` refuses any `$(`/
    // backtick word), and a non-foldable one only ever yields spans
    // strictly inside its own content region.
    let mut out = String::with_capacity(s.len());
    let mut pos = 0;
    for sp in &spans {
        debug_assert!(sp.start >= pos, "overlapping fold spans");
        out.push_str(&s[pos..sp.start]);
        out.push_str(&sp.repl);
        pos = sp.end;
    }
    out.push_str(&s[pos..]);
    Some(out)
}

/// Scan `range` of `s` starting in `mode`, collecting foldable spans.
/// A non-foldable `$( … )` is scanned recursively with a FRESH context
/// (its content is its own shell context), so `$(( $(echo 3) + 1 ))`
/// still folds the inner substitution. Single-quoted regions are literal
/// and skipped wholesale. An unterminated construct ends the scan — the
/// text is not shell, so nothing after it can be judged.
fn scan(s: &str, range: std::ops::Range<usize>, mode0: Mode, spans: &mut Vec<Span>) {
    let b = s.as_bytes();
    let end = range.end;
    let mut i = range.start;
    let mut mode = mode0;
    while i < end {
        let c = b[i];
        match mode {
            Mode::Single => {
                if c == b'\'' {
                    mode = Mode::Normal;
                }
                i += 1;
            }
            Mode::Dq => match c {
                b'"' => {
                    mode = Mode::Normal;
                    i += 1;
                }
                b'\\' => i += 2,
                b'`' => match subst_span(s, i, end, true, spans) {
                    Some(next) => i = next,
                    None => return,
                },
                b'$' if i + 1 < end && b[i + 1] == b'(' => {
                    match subst_span(s, i, end, true, spans) {
                        Some(next) => i = next,
                        None => return,
                    }
                }
                _ => i += 1,
            },
            Mode::Normal => match c {
                b'\'' => {
                    mode = Mode::Single;
                    i += 1;
                }
                b'"' => {
                    mode = Mode::Dq;
                    i += 1;
                }
                b'\\' => i += 2,
                b'`' => match subst_span(s, i, end, false, spans) {
                    Some(next) => i = next,
                    None => return,
                },
                b'$' if i + 1 < end && b[i + 1] == b'(' => {
                    match subst_span(s, i, end, false, spans) {
                        Some(next) => i = next,
                        None => return,
                    }
                }
                _ => i += 1,
            },
        }
    }
}

/// Handle one substitution starting at `i` (`$(` at `i`, or a backtick
/// at `i`). Returns the index just past it, or `None` when the
/// substitution is unterminated (the whole scan stops: not shell text).
fn subst_span(
    s: &str,
    i: usize,
    end: usize,
    quoted: bool,
    spans: &mut Vec<Span>,
) -> Option<usize> {
    let b = s.as_bytes();
    if b[i] == b'$' {
        // `$( … )` — balance parens with quote awareness.
        let close = find_close(s, i + 1, end)?;
        let inner = &s[i + 2..close];
        if let Some(repl) = constant_output(inner) {
            if safe_repl(&repl, quoted) {
                spans.push(Span {
                    start: i,
                    end: close + 1,
                    repl,
                });
                return Some(close + 1);
            }
        }
        // Not foldable: the content is its own shell context — look for
        // nested substitutions inside it with a fresh quote state.
        scan(s, (i + 2)..close, Mode::Normal, spans);
        Some(close + 1)
    } else {
        // Backtick form: no nesting, next backtick closes it.
        let mut j = i + 1;
        while j < end && b[j] != b'`' {
            if b[j] == b'\\' {
                j += 1; // escaped backtick does not close
            }
            j += 1;
        }
        if j >= end {
            return None;
        }
        let inner = &s[i + 1..j];
        if let Some(repl) = constant_output(inner) {
            if safe_repl(&repl, quoted) {
                spans.push(Span {
                    start: i,
                    end: j + 1,
                    repl,
                });
                return Some(j + 1);
            }
        }
        scan(s, (i + 1)..j, Mode::Normal, spans);
        Some(j + 1)
    }
}

/// Index of the `)` closing the `(` at `open`, balancing depth and
/// honoring quotes (parens inside `'…'`/`"…"` are literal). `None` when
/// the range ends first.
fn find_close(s: &str, open: usize, end: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = open + 1;
    let mut depth: u32 = 1;
    let mut mode = Mode::Normal;
    while i < end {
        let c = b[i];
        match mode {
            Mode::Single => {
                if c == b'\'' {
                    mode = Mode::Normal;
                }
                i += 1;
            }
            Mode::Dq => match c {
                b'"' => {
                    mode = Mode::Normal;
                    i += 1;
                }
                b'\\' => i += 2,
                _ => i += 1,
            },
            Mode::Normal => match c {
                b'\'' => {
                    mode = Mode::Single;
                    i += 1;
                }
                b'"' => {
                    mode = Mode::Dq;
                    i += 1;
                }
                b'\\' => i += 2,
                b'(' => {
                    depth += 1;
                    i += 1;
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                    i += 1;
                }
                _ => i += 1,
            },
        }
    }
    None
}

/// The exact output of the substitution when it is provably constant.
/// `None` for anything else — refuse > guess.
fn constant_output(inner: &str) -> Option<String> {
    let words = shell_words(inner)?;
    let (cmd, rest) = words.split_first()?;
    match cmd.as_str() {
        "true" | ":" if rest.is_empty() => Some(String::new()),
        "echo" => {
            for w in rest {
                // an echo FLAG changes the output (`-n`, `-e`, `-E`,
                // and combinations like `-ne`)
                if w.starts_with('-') && w.len() > 1 && w[1..].chars().all(|c| "neE".contains(c)) {
                    return None;
                }
                // a word that broke a line (only possible from quotes)
                // would not survive the renderer's single-line re-parse
                if w.contains('\n') || w.contains('\r') {
                    return None;
                }
            }
            Some(rest.join(" "))
        }
        _ => None,
    }
}

/// POSIX word-split of the substitution's content into literal words.
/// Refuses escapes, expansions, and every unquoted metacharacter
/// (`| & ; < > ( ) { } * ? [ ] # ~ $ ` \`) — those make the output
/// depend on the runtime environment.
fn shell_words(inner: &str) -> Option<Vec<String>> {
    let b = inner.as_bytes();
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\n' | b'\r' => {
                if has {
                    words.push(std::mem::take(&mut cur));
                    has = false;
                }
                i += 1;
            }
            b'\'' => {
                // single quotes: literal until the closing quote
                has = true;
                i += 1;
                let start = i;
                while i < b.len() && b[i] != b'\'' {
                    i += 1;
                }
                if i >= b.len() {
                    return None; // unclosed
                }
                cur.push_str(&inner[start..i]);
                i += 1;
            }
            b'"' => {
                // double quotes: literal, but no expansion inside a word
                // we claim to compute (`$`, backtick, backslash)
                has = true;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if matches!(b[i], b'$' | b'`' | b'\\') {
                        return None;
                    }
                    let ch = inner[i..].chars().next()?;
                    cur.push(ch);
                    i += ch.len_utf8();
                }
                if i >= b.len() {
                    return None; // unclosed
                }
                i += 1;
            }
            _ => {
                if matches!(
                    c,
                    b'\\' | b'$' | b'`' | b'|' | b'&' | b';' | b'<' | b'>' | b'(' | b')'
                        | b'{' | b'}' | b'*' | b'?' | b'[' | b']' | b'#' | b'!' | b'~'
                ) {
                    return None;
                }
                has = true;
                let ch = inner[i..].chars().next()?;
                cur.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    if has {
        words.push(cur);
    }
    Some(words)
}

/// Is the replacement safe in its surrounding quote context?
fn safe_repl(repl: &str, quoted: bool) -> bool {
    if quoted {
        // inside `"…"` the value must not end the quote, escape, or
        // trigger a re-expansion when the renderer re-parses the text
        !repl.contains('"') && !repl.contains('\\') && !repl.contains('$') && !repl.contains('`')
    } else {
        // unquoted: exactly one shell-safe word — no word-splitting,
        // globbing, or expansion can differ from what bash would do
        repl.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, StrStyle};

    fn s(x: &str) -> IrExpr {
        IrExpr::Str(x.to_string(), StrStyle::DoubleQuoted)
    }

    fn call(func: &str, args: Vec<IrExpr>) -> IrExpr {
        IrExpr::Call {
            func: func.to_string(),
            args,
        }
    }

    fn stmt_with(func: &str, arg: &str) -> Vec<IrStmt> {
        vec![IrStmt::Expr(call(func, vec![s(arg)]))]
    }

    fn arg_of(st: &[IrStmt]) -> String {
        let IrStmt::Expr(IrExpr::Call { args, .. }) = &st[0] else {
            panic!("not a call stmt")
        };
        match &args[0] {
            IrExpr::Str(v, _) => v.clone(),
            other => panic!("not a str arg: {other:?}"),
        }
    }

    #[test]
    fn folds_quoted_test_operand() {
        let mut st = stmt_with("test", "\"$(echo hello)\"=\"hello\"");
        assert!(transform(&mut st));
        assert_eq!(arg_of(&st), "\"hello\"=\"hello\"");
    }

    #[test]
    fn folds_unquoted_single_word_operand() {
        let mut st = stmt_with("test", "$(echo foo) -ne 0");
        assert!(transform(&mut st));
        assert_eq!(arg_of(&st), "foo -ne 0");
    }

    #[test]
    fn refuses_unquoted_multi_word_output() {
        // word-splitting would produce three operands — refuse
        let mut st = stmt_with("test", "$(echo a b) -eq 1");
        assert!(!transform(&mut st));
        assert_eq!(arg_of(&st), "$(echo a b) -eq 1");
    }

    #[test]
    fn folds_multi_word_output_inside_quotes() {
        // inside "…" bash keeps the whole output as ONE word
        let mut st = stmt_with("test", "-n \"$(echo File not found)\"");
        assert!(transform(&mut st));
        assert_eq!(arg_of(&st), "-n \"File not found\"");
    }

    #[test]
    fn refuses_variable_words() {
        let mut st = stmt_with("test", "[ \"$(echo $x)\" = y ]");
        assert!(!transform(&mut st));
        assert_eq!(arg_of(&st), "[ \"$(echo $x)\" = y ]");
    }

    #[test]
    fn refuses_echo_flags() {
        let mut st = stmt_with("test", "$(echo -n hi) -eq 1");
        assert!(!transform(&mut st));
        assert_eq!(arg_of(&st), "$(echo -n hi) -eq 1");
    }

    #[test]
    fn single_quoted_region_is_literal() {
        let mut st = stmt_with("test", "-n '$(echo hi)'");
        assert!(!transform(&mut st));
        assert_eq!(arg_of(&st), "-n '$(echo hi)'");
    }

    #[test]
    fn folds_param_default() {
        let mut st = vec![IrStmt::Expr(call(
            "param",
            vec![s(":-"), s("v"), s("$(echo dflt)")],
        ))];
        assert!(transform(&mut st));
        let IrStmt::Expr(IrExpr::Call { args, .. }) = &st[0] else {
            panic!()
        };
        match &args[2] {
            IrExpr::Str(v, _) => assert_eq!(v, "dflt"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn folds_nested_arith_substitution() {
        let mut st = stmt_with("arith", "$(echo 3) + 1");
        assert!(transform(&mut st));
        assert_eq!(arg_of(&st), "3 + 1");
    }

    #[test]
    fn folds_true_to_empty() {
        assert_eq!(constant_output("true"), Some(String::new()));
        let mut st = stmt_with("test", "x$(true)y");
        assert!(transform(&mut st));
        assert_eq!(arg_of(&st), "xy");
    }

    #[test]
    fn refuses_glob_and_tilde_words() {
        // `echo *` / `echo ~` expand before echo runs — not constant
        assert_eq!(constant_output("echo *"), None);
        assert_eq!(constant_output("echo ~"), None);
        assert_eq!(constant_output("echo a$HOME"), None);
    }

    #[test]
    fn backtick_form_folds() {
        let mut st = stmt_with("test", "\"`echo hello`\"=\"hello\"");
        assert!(transform(&mut st));
        assert_eq!(arg_of(&st), "\"hello\"=\"hello\"");
    }

    #[test]
    fn non_text_func_is_untouched() {
        // word args are NOT raw text — the parser owns their expansion
        let mut st = vec![IrStmt::Expr(call(
            "builtin",
            vec![s("echo"), IrExpr::Array(vec![s("$(echo x)")])],
        ))];
        assert!(!transform(&mut st));
    }

    #[test]
    fn quoted_literal_replacement_chars_refused() {
        // `"$(echo 'a"b')"` would end the double quote
        assert_eq!(constant_output("echo 'a\"b'"), Some("a\"b".into()));
        assert!(!safe_repl("a\"b", true));
        assert!(!safe_repl("a b", false));
        assert!(safe_repl("a b", true));
        assert!(safe_repl("a-b", false));
    }

    #[test]
    fn assign_targets_compile_shape() {
        // smoke: a test inside an If still folds
        let mut st = vec![IrStmt::If {
            cond: call("test", vec![s("\"$(echo hi)\"=\"hi\"")]),
            then: vec![],
            elsifs: vec![],
            else_: vec![],
        }];
        assert!(transform(&mut st));
        let IrStmt::If { cond, .. } = &st[0] else { panic!() };
        let IrExpr::Call { args, .. } = cond else { panic!() };
        match &args[0] {
            IrExpr::Str(v, _) => assert_eq!(v, "\"hi\"=\"hi\""),
            other => panic!("{other:?}"),
        }
        let _ = AssignTarget {
            var: "x".into(),
            indices: vec![],
            sigil: None,
        };
    }
}
