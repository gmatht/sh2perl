//! const-capture-fold — fold a command substitution that runs only a
//! constant `echo` into the string literal (`v=$(echo foo)` → `v="foo"`),
//! the constant half of capture lowering (direct-calls handles
//! function-of-constant; this handles the plain literal echo).
//!
//! ## Need
//! Scripts and generated code capture constants via `$(echo …)`; every
//! backend spawns a process (or a string-parse) for a value known at IR
//! time. Folding it to a `Str` literal removes the spawn in all backends.
//!
//! ## Scope — the sound rule
//! A `Capture` folds to a `Str` only when its arrow body is EXACTLY ONE
//! statement whose raw stdout is known at IR time, and the result strips
//! the trailing newlines the way command substitution does:
//!   - `Output { value: lit, newline: true, target: None }`
//!     → the literal with ALL trailing `\n` removed;
//!   - `Output { value: lit, newline: false, … }` → the literal as-is;
//!   - `Expr(exec|builtin <literal echo>)` → the words joined by spaces
//!     (a `-n`/`-e`/`-E` flag disables the fold — it changes the output);
//!   - `Expr(exec|builtin <literal printf>)` → the format applied to the
//!     literal args (only `%s %d %i %u %x %X %o %%` and the common
//!     backslash escapes — widths, floats, format cycling and missing-arg
//!     defaults refuse);
//!   - `Expr(BinOp { op: Or | And, … })` over such literal commands, when
//!     both the OUTPUT and the EXIT STATUS of the reached arm are known:
//!     `$(echo a || echo b)` → `a` (literal echo exits 0, rhs never runs),
//!     `$(false || echo b)` → `b`, `$(echo a && echo b)` → `a\nb`.
//!     `Or`/`And` folds preserve the chain's exit status (a chain that
//!     could exit non-zero — `echo a && false`, `false && …` — refuses,
//!     because the assignment form would lose the status write).
//! Any capture that embeds a variable, a non-constant command, multiple
//! statements, or a redirect is left untouched (refuse > guess).
//!
//! A literal word arrives EITHER as `Str` or as an all-lit `Interpolate`
//! (the usual spelling for a double-quoted word — `"hello"`); refusing
//! the Interpolate spelling left `$(echo "hi")` un-folded where
//! `$(echo hi)` folded, i.e. most real scripts.
//!
//! ## Placement
//! Bundle for run_core_worker.sh (register + manifest). Registered in
//! `transforms.rs` (SH2_TRANSFORMS gated). Complements direct-calls.

use crate::ir::{ArithAst, BinOpKind, InterpPart, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt_pass(st);
    }
    c
}

fn stmt_pass(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => expr_pass(expr),
        IrStmt::Expr(e) => expr_pass(e),
        IrStmt::Output { value, .. } => expr_pass(value),
        IrStmt::Declare { init, .. } => init.as_mut().map(expr_pass).unwrap_or(false),
        IrStmt::WriteFile { path, content, .. } => {
            expr_pass(path) | expr_pass(content)
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            let mut c = expr_pass(cond);
            for s in then.iter_mut() {
                c |= stmt_pass(s);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= expr_pass(ec);
                for s in eb.iter_mut() {
                    c |= stmt_pass(s);
                }
            }
            for s in else_.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, var: _ } => {
            let mut c = expr_pass(cond);
            for s in body.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        _ => false,
    }
}

fn expr_pass(e: &mut IrExpr) -> bool {
    match e {
        // the FRONTEND spelling of a command substitution: a `capture`
        // Call whose single argument is the body arrow. The py-sh-go
        // frontend emits this for `subprocess.run(…, capture_output=True)`
        // / `$(…)`, with the body an `exec` call rather than an `Output`.
        IrExpr::Call { func, args, .. } if func == "capture" => {
            let mut changed = false;
            if let Some(inner) = args.first() {
                if let Some(s) = fold_const_echo(inner) {
                    let style = crate::ir::StrStyle::DoubleQuoted;
                    *e = IrExpr::Str(s, style);
                    return true;
                }
            }
            for a in args.iter_mut() {
                changed |= expr_pass(a);
            }
            changed
        }
        IrExpr::Capture { expr, native } => {
            if *native {
                return false; // already a native value — leave it
            }
            let mut changed = false;
            if let Some(s) = fold_const_echo(expr) {
                *e = IrExpr::Str(s, crate::ir::StrStyle::DoubleQuoted);
                changed = true;
            } else {
                changed = expr_pass(expr);
            }
            changed
        }
        IrExpr::BinOp { lhs, rhs, .. } => expr_pass(lhs) | expr_pass(rhs),
        IrExpr::Call { args, .. } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= expr_pass(a);
            }
            c
        }
        IrExpr::Index { key, .. } => expr_pass(key),
        IrExpr::Arith(a) => arith_pass(a),
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let InterpPart::Expr(x) = p {
                    c |= expr_pass(x);
                }
            }
            c
        }
        IrExpr::Arrow(body) => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt_pass(s);
            }
            c
        }
        _ => false,
    }
}

fn arith_pass(a: &mut ArithAst) -> bool {
    match a {
        ArithAst::Un { arg, .. } => arith_pass(arg),
        ArithAst::Bin { lhs, rhs, .. } => arith_pass(lhs) | arith_pass(rhs),
        _ => false,
    }
}

/// If `expr` (the capture's inner Arrow) is exactly one statement whose
/// stdout is a compile-time constant, return that output with trailing
/// newlines stripped (as command substitution does); else None.
///
/// THREE statement spellings are recognised:
///   * `Output { value: lit, newline, target: None }` — the lowered form
///     (`echo foo` after echo-fold, or a shell-source capture);
///   * `Expr(Call { func: "exec"|"builtin", args: [Str(cmd), Array(lit…)] })`
///     — the FRONTEND form (py-sh-go lowers `subprocess.run(…,
///     capture_output=True)` this way) and the shell-source form (`echo`/
///     `printf` inside `$()` arrive as command calls before echo lowering);
///   * `Expr(BinOp { op: Or|And, lhs, rhs })` — a constant chain, via
///     [`chain_raw`] (which also proves the chain's exit status).
fn fold_const_echo(expr: &IrExpr) -> Option<String> {
    let body = match expr {
        IrExpr::Arrow(body) => body,
        _ => return None,
    };
    match body.as_slice() {
        [IrStmt::Output { value, newline, target: None }] => {
            let s = lit_word_text(value)?;
            Some(if *newline {
                strip_all_trailing_newlines(&s)
            } else {
                s
            })
        }
        [IrStmt::Expr(e)] => match e {
            IrExpr::Call { func, .. } if func == "exec" || func == "builtin" => {
                Some(strip_all_trailing_newlines(&const_raw(e)?))
            }
            IrExpr::BinOp {
                op: BinOpKind::Or | BinOpKind::And,
                ..
            } => Some(strip_all_trailing_newlines(&chain_raw(e)?)),
            _ => None,
        },
        _ => None,
    }
}

/// The literal text of a word expression: `Str` verbatim, or the
/// concatenation of an all-lit `Interpolate` (a double-quoted word —
/// `"hello"` — arrives THIS way, not as `Str`; refusing it here is what
/// used to leave `$(echo "hi")` un-folded while `$(echo hi)` folded).
fn lit_word_text(w: &IrExpr) -> Option<String> {
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

/// Is this a command call whose words are all literal? Returns the head
/// command and the literal word texts.
fn literal_argv(e: &IrExpr) -> Option<(String, Vec<String>)> {
    let IrExpr::Call { func, args } = e else {
        return None;
    };
    if func != "exec" && func != "builtin" {
        return None;
    }
    let cmd = match args.first()? {
        IrExpr::Str(s, _) => s.clone(),
        _ => return None,
    };
    let items = match args.get(1)? {
        IrExpr::Array(items) => items,
        _ => return None,
    };
    let mut words = Vec::with_capacity(items.len());
    for w in items {
        words.push(lit_word_text(w)?);
    }
    Some((cmd, words))
}

/// The RAW stdout of a single literal command, when known at IR time.
/// (Raw = before command substitution strips trailing newlines, so a
/// chain can concatenate arms exactly.)
fn const_raw(e: &IrExpr) -> Option<String> {
    let (cmd, words) = literal_argv(e)?;
    match cmd.as_str() {
        "echo" => {
            let joined = const_echo_words_from(words)?;
            Some(format!("{}\n", joined.join(" ")))
        }
        "printf" => const_printf(&cmd, words),
        // `true`/`true ANYTHING`: no output — but only with LITERAL words,
        // so a side-effecting word (`true $(…)`) is never dropped.
        "true" => Some(String::new()),
        _ => None,
    }
}

/// The KNOWN exit status of a literal command (`Some(false)` only for
/// `false`). None = unknown (external/env-dependent, or a refused shape).
fn const_ok(e: &IrExpr) -> Option<bool> {
    let (cmd, words) = literal_argv(e)?;
    match cmd.as_str() {
        "echo" => const_echo_words_from(words).map(|_| true),
        "printf" => const_printf(&cmd, words).map(|_| true),
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Exit status of a literal command OR a chain of them (recursively
/// proving through `Or`/`And`), when provable.
fn chain_ok(e: &IrExpr) -> Option<bool> {
    if let Some(v) = const_ok(e) {
        return Some(v);
    }
    let IrExpr::BinOp { op, lhs, rhs } = e else {
        return None;
    };
    match op {
        // status = lhs's when it exits 0 (rhs never runs), else rhs's
        BinOpKind::Or => match chain_ok(lhs)? {
            true => Some(true),
            false => chain_ok(rhs),
        },
        // status = lhs's when it fails (rhs never runs), else rhs's
        BinOpKind::And => match chain_ok(lhs)? {
            false => Some(false),
            true => chain_ok(rhs),
        },
        _ => None,
    }
}

/// Raw stdout of a CONSTANT chain (`Or`/`And` over literal commands),
/// only when the chain's EXIT STATUS is provably 0 as well — the fold
/// replaces the capture (and its status write) with a plain literal, so a
/// chain that could exit non-zero must refuse (`echo a && false` → status
/// 1 lost; `false && …` → status 1 lost).
///
///   Or  { status-0 lhs } → lhs runs, rhs unreachable → raw(lhs)
///   Or  { failing lhs, status-0 rhs } → raw(rhs)
///   And { status-0 lhs, status-0 rhs } → raw(lhs) ++ raw(rhs)
///
/// INVARIANT: `Some(_)` implies raw stdout known AND exit status 0.
fn chain_raw(e: &IrExpr) -> Option<String> {
    if let Some(r) = const_raw(e) {
        return Some(r);
    }
    let IrExpr::BinOp { op, lhs, rhs } = e else {
        return None;
    };
    match op {
        BinOpKind::Or => match chain_ok(lhs)? {
            true => chain_raw(lhs),
            // rhs's status becomes the chain's — require 0
            false => {
                if chain_ok(rhs)? {
                    chain_raw(rhs)
                } else {
                    None
                }
            }
        },
        BinOpKind::And => {
            // status = rhs's when lhs runs (fails → status 1, refuse);
            // both arms' outputs concatenate raw.
            if !chain_ok(lhs)? || !chain_ok(rhs)? {
                return None;
            }
            Some(chain_raw(lhs)? + &chain_raw(rhs)?)
        }
        _ => None,
    }
}

/// Literal `printf` in a capture: format + literal args → the raw output.
/// Bash interprets backslash escapes IN THE FORMAT (args are printed
/// verbatim for `%s`). Exact conversion/argument count — bash's format
/// cycling and missing-arg defaults are NOT modelled, so those refuse.
/// Widths, precision, flags and float conversions refuse (refuse > guess).
fn const_printf(_cmd: &str, words: Vec<String>) -> Option<String> {
    let mut it = words.into_iter();
    let fmt = decode_fmt_escapes(&it.next()?)?;
    let argv: Vec<String> = it.collect();
    let chars: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut ai = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c != '%' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let conv = *chars.get(i)?; // dangling % → refuse
        if conv == '%' {
            out.push('%');
            i += 1;
            continue;
        }
        let arg = argv.get(ai)?; // fewer args than conversions → refuse
        ai += 1;
        match conv {
            's' => out.push_str(arg),
            'd' | 'i' => out.push_str(&fmt_int(arg, 10, true)?.to_string()),
            // unsigned/hex/octal: negatives wrap in C — not modelled → refuse
            'u' => out.push_str(&fmt_int(arg, 10, false)?.to_string()),
            'x' | 'X' => {
                let n = fmt_int(arg, 16, false)?;
                if conv == 'X' {
                    out.push_str(&format!("{:X}", n));
                } else {
                    out.push_str(&format!("{:x}", n));
                }
            }
            'o' => out.push_str(&format!("{:o}", fmt_int(arg, 8, false)?)),
            _ => return None, // %c/%f/%5s/flags → refuse
        }
        i += 1;
    }
    if ai != argv.len() {
        return None; // surplus args would CYCLE the format in bash
    }
    Some(out)
}

/// bash printf's argument-to-integer conversion (strtoll semantics):
/// optional leading whitespace and sign, then digits of `base` (a `0x`
/// prefix for hex), trailing junk ignored, no digits → 0 (when
/// `zero_default`, i.e. `%d`/`%i`); negatives refused for the unsigned
/// conversions (`allow_neg == false`). Overflow refuses.
fn fmt_int(arg: &str, base: u32, allow_neg: bool) -> Option<i64> {
    let t = arg.trim_start_matches([' ', '\t']);
    let (neg, rest) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    if neg && !allow_neg {
        return None;
    }
    let (digits, radix) = if base == 16 {
        match rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
            Some(h) => (h, 16u32),
            None => (rest, 16u32),
        }
    } else {
        (rest, base)
    };
    let end = digits
        .find(|c: char| !c.is_digit(radix))
        .unwrap_or(digits.len());
    if end == 0 {
        return if allow_neg { Some(0) } else { None };
    }
    // i64::from_str = ERANGE boundary; overflow refuses (bash clamps —
    // a value that large is out of corpus scope; refusing is sound).
    let n: i64 = digits[..end].parse().ok()?;
    Some(if neg { -n } else { n })
}

/// Decode the backslash escapes bash printf applies to the FORMAT.
/// Unknown escapes (and a trailing backslash) REFUSE — bash leaves them
/// implementation-defined, so guessing risks a wrong literal.
fn decode_fmt_escapes(f: &str) -> Option<String> {
    let mut out = String::with_capacity(f.len());
    let mut it = f.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let e = it.next()?;
        out.push(match e {
            'a' => '\x07',
            'b' => '\x08',
            'e' => '\x1b',
            'f' => '\x0c',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'v' => '\x0b',
            '\\' => '\\',
            '"' => '"',
            _ => return None,
        });
    }
    Some(out)
}

/// The literal words of an `exec`/`builtin` echo, when the command is
/// `echo` and every word is constant (a `-n`/`-e`/`-E` flag disables the
/// fold since it changes the output). Returns None on anything with a
/// variable, a capture, an interpolation or a flag.
fn const_echo_words(args: &[IrExpr]) -> Option<Vec<String>> {
    let cmd = match args.first() {
        Some(IrExpr::Str(s, _)) => s.as_str(),
        _ => return None,
    };
    if cmd != "echo" {
        return None;
    }
    let items = match args.get(1) {
        Some(IrExpr::Array(items)) => items,
        _ => return None,
    };
    let mut words = Vec::with_capacity(items.len());
    for w in items {
        let t = lit_word_text(w)?;
        // an echo FLAG (`-n`, `-e`, `-E`) changes the output
        if t.starts_with('-') && t.len() > 1 && t[1..].chars().all(|c| "neE".contains(c)) {
            return None;
        }
        words.push(t);
    }
    Some(words)
}

/// Same check over pre-extracted literal word texts.
fn const_echo_words_from(words: Vec<String>) -> Option<Vec<String>> {
    for t in &words {
        if t.starts_with('-') && t.len() > 1 && t[1..].chars().all(|c| "neE".contains(c)) {
            return None;
        }
    }
    Some(words)
}

fn strip_all_trailing_newlines(s: &str) -> String {
    s.trim_end_matches('\n').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{BinOpKind, InterpPart, IrExpr, IrStmt, StrStyle};

    fn call(func: &str, args: Vec<IrExpr>) -> IrExpr {
        IrExpr::Call { func: func.to_string(), args }
    }
    fn s(x: &str) -> IrExpr {
        IrExpr::Str(x.to_string(), StrStyle::DoubleQuoted)
    }
    /// A double-quoted word — the common spelling is all-lit Interpolate.
    fn dq(x: &str) -> IrExpr {
        IrExpr::Interpolate(vec![InterpPart::Lit(x.to_string())])
    }
    fn exec(cmd: &str, words: Vec<IrExpr>) -> IrExpr {
        call("exec", vec![s(cmd), IrExpr::Array(words)])
    }
    /// The py-frontend capture shape:
    /// `capture(Arrow([Expr(exec(echo, [words]))]))`.
    fn frontend_capture(words: Vec<IrExpr>) -> IrStmt {
        let body = IrExpr::Arrow(vec![IrStmt::Expr(call(
            "exec",
            vec![s("echo"), IrExpr::Array(words)],
        ))]);
        IrStmt::Assign {
            targets: vec![crate::ir::AssignTarget {
                var: "x".into(),
                indices: vec![],
                sigil: None,
            }],
            expr: call("capture", vec![body]),
            asm: None,
        }
    }
    /// A capture whose body is one arbitrary statement expression.
    fn capture_of(e: IrExpr) -> IrStmt {
        let body = IrExpr::Arrow(vec![IrStmt::Expr(e)]);
        IrStmt::Expr(call("capture", vec![body]))
    }
    fn folded(sg: IrStmt) -> String {
        let mut stmts = vec![sg];
        assert!(transform(&mut stmts), "expected a fold");
        match &stmts[0] {
            IrStmt::Expr(IrExpr::Str(v, _)) => v.clone(),
            IrStmt::Assign { expr: IrExpr::Str(v, _), .. } => v.clone(),
            other => panic!("not folded: {other:?}"),
        }
    }
    fn refuses(sg: IrStmt) {
        let mut stmts = vec![sg];
        assert!(!transform(&mut stmts), "expected refusal: {:?}", stmts[0]);
    }

    #[test]
    fn frontend_constant_echo_capture_folds() {
        let mut stmts = vec![frontend_capture(vec![s("hi")])];
        assert!(transform(&mut stmts));
        match &stmts[0] {
            IrStmt::Assign { expr: IrExpr::Str(v, _), .. } => assert_eq!(v, "hi"),
            other => panic!("not folded: {other:?}"),
        }
    }

    #[test]
    fn quoted_interpolate_word_folds() {
        // `$(echo "hi")` — a double-quoted word arrives as all-lit
        // Interpolate, not Str; it must fold like the unquoted spelling.
        assert_eq!(folded(frontend_capture(vec![dq("hi")])), "hi");
        assert_eq!(folded(frontend_capture(vec![dq("a b")])), "a b");
        // mixed literal parts concatenate
        assert_eq!(
            folded(frontend_capture(vec![IrExpr::Interpolate(vec![
                InterpPart::Lit("a ".into()),
                InterpPart::Lit("b".into()),
            ])])),
            "a b"
        );
    }

    #[test]
    fn interpolate_with_expr_part_refuses() {
        let w = IrExpr::Interpolate(vec![
            InterpPart::Lit("x= ".into()),
            InterpPart::Expr(Box::new(call("getVar", vec![s("v")]))),
        ]);
        refuses(frontend_capture(vec![w]));
    }

    #[test]
    fn multi_word_echo_joins_with_a_space() {
        let mut stmts = vec![frontend_capture(vec![s("a"), s("b")])];
        assert!(transform(&mut stmts));
        match &stmts[0] {
            IrStmt::Assign { expr: IrExpr::Str(v, _), .. } => assert_eq!(v, "a b"),
            other => panic!("not folded: {other:?}"),
        }
    }

    #[test]
    fn variable_word_refuses() {
        let mut stmts = vec![frontend_capture(vec![call("getVar", vec![s("v")])])];
        assert!(!transform(&mut stmts), "a var word must not fold");
    }

    #[test]
    fn echo_flag_refuses() {
        // `echo -n hi`: the flag changes the output, so refuse
        let mut stmts = vec![frontend_capture(vec![s("-n"), s("hi")])];
        assert!(!transform(&mut stmts), "an echo flag must not fold");
    }

    #[test]
    fn printf_literal_folds_via_format() {
        // printf no longer folds "as echo" (space-join) — it folds through
        // the format evaluator: `%s` verbatim, `%d` strtoll-parsed.
        assert_eq!(folded(capture_of(exec("printf", vec![s("hi")]))), "hi");
        assert_eq!(
            folded(capture_of(exec(
                "printf",
                vec![dq("%s=%d\n"), s("x"), s("42")]
            ))),
            "x=42"
        );
        // backslash escapes in the FORMAT decode (A1 carries the two-char
        // bash sequence); the trailing newline is command-substitution
        // stripped
        assert_eq!(
            folded(capture_of(exec("printf", vec![dq("a\\tb\n")]))),
            "a\tb"
        );
    }

    #[test]
    fn printf_unsupported_shapes_refuse() {
        // width flag
        refuses(capture_of(exec("printf", vec![s("%5s"), s("x")])));
        // float conversion
        refuses(capture_of(exec("printf", vec![s("%f"), s("1.5")])));
        // fewer args than conversions (missing-arg defaults not modelled)
        refuses(capture_of(exec("printf", vec![s("%s %s"), s("x")])));
        // surplus args (bash would cycle the format)
        refuses(capture_of(exec("printf", vec![s("%s"), s("x"), s("y")])));
        // unknown backslash escape in the format
        refuses(capture_of(exec("printf", vec![s("a\\qb")])));
        // non-literal arg
        refuses(capture_of(exec(
            "printf",
            vec![s("%s"), call("getVar", vec![s("v")])],
        )));
        // negative %u (C wrap not modelled)
        refuses(capture_of(exec("printf", vec![s("%u"), s("-1")])));
    }

    #[test]
    fn other_command_refuses() {
        let mut stmts = vec![IrStmt::Expr(call("capture", vec![IrExpr::Arrow(vec![
            IrStmt::Expr(exec("date", vec![s("+%Y")])),
        ])]))];
        assert!(!transform(&mut stmts), "date must not fold (env-dependent)");
    }

    #[test]
    fn or_chain_folds_to_reachable_arm() {
        // `$(echo a || echo b)` — literal echo exits 0, rhs unreachable
        assert_eq!(
            folded(capture_of(IrExpr::BinOp {
                lhs: Box::new(exec("echo", vec![s("a")])),
                op: BinOpKind::Or,
                rhs: Box::new(exec("echo", vec![s("b")])),
            })),
            "a"
        );
        // `$(false || echo b)` — lhs fails, rhs runs
        assert_eq!(
            folded(capture_of(IrExpr::BinOp {
                lhs: Box::new(exec("false", vec![])),
                op: BinOpKind::Or,
                rhs: Box::new(exec("echo", vec![dq("b")])),
            })),
            "b"
        );
    }

    #[test]
    fn and_chain_concatenates_when_status_known() {
        assert_eq!(
            folded(capture_of(IrExpr::BinOp {
                lhs: Box::new(exec("echo", vec![s("a")])),
                op: BinOpKind::And,
                rhs: Box::new(exec("echo", vec![s("b")])),
            })),
            "a\nb"
        );
    }

    #[test]
    fn chain_status_loss_refuses() {
        // `$(echo a && false)` exits 1 — an assignment fold would lose it
        refuses(capture_of(IrExpr::BinOp {
            lhs: Box::new(exec("echo", vec![s("a")])),
            op: BinOpKind::And,
            rhs: Box::new(exec("false", vec![])),
        }));
        // `$(false && echo b)` exits 1
        refuses(capture_of(IrExpr::BinOp {
            lhs: Box::new(exec("false", vec![])),
            op: BinOpKind::And,
            rhs: Box::new(exec("echo", vec![s("b")])),
        }));
        // `$(date || echo b)` — unknown lhs status AND env-dependent value
        refuses(capture_of(IrExpr::BinOp {
            lhs: Box::new(exec("date", vec![])),
            op: BinOpKind::Or,
            rhs: Box::new(exec("echo", vec![s("b")])),
        }));
        // dynamic rhs on the reachable side must not fold either
        refuses(capture_of(IrExpr::BinOp {
            lhs: Box::new(exec("cat", vec![s("f")])),
            op: BinOpKind::Or,
            rhs: Box::new(exec("echo", vec![s("b")])),
        }));
    }

    #[test]
    fn multi_statement_body_refuses() {
        let body = IrExpr::Arrow(vec![
            IrStmt::Expr(call("exec", vec![s("echo"), IrExpr::Array(vec![s("a")])])),
            IrStmt::Expr(call("exec", vec![s("echo"), IrExpr::Array(vec![s("b")])])),
        ]);
        let mut stmts = vec![IrStmt::Expr(call("capture", vec![body]))];
        assert!(!transform(&mut stmts), "two statements must not fold");
    }
}
