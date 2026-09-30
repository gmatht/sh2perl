//! Perl GENERATOR wrapper — targeted semantics fixes layered on top of
//! `ir::shir_to_perl`.
//!
//! # Why this file exists
//!
//! The Perl GENERATOR (the AST -> shIR -> Perl product path that `./fail`
//! gates via `otranspilerl-cli --target pl`) lives in the shared core as
//! `ir::shir_to_perl`. The shared core is owned by another worker, so this
//! module owns everything we may change *without* touching it: an IR-level
//! pre-pass that corrects defects in the incoming `IrProgram` before it
//! reaches the shared renderer, plus a thin textual post-pass.
//!
//! # Correction is IR-level, not text-level
//!
//! A regex over the emitted Perl cannot fix these: the defects are decisions
//! the renderer makes about *structure* (which value a condition tests, what
//! a statement position means). `IrProgram` is `Clone` and `shir_to_perl`
//! takes `&IrProgram`, so a pre-pass here is a sound place to correct the IR
//! and leave the shared renderer untouched.
//!
//! Each correction is deliberately narrow: it fires only on a shape it fully
//! understands, and leaves everything else alone (unknown shapes are the
//! shared renderer's business — and guessing would be worse than the status
//! quo, which at least runs).

use crate::ir::{shir_to_perl, InterpPart, IrExpr, IrProgram, IrStmt};

/// The Perl GENERATOR entry point: shared renderer + this module's fixes.
///
/// This is what `otranspilerl-cli`'s `--target pl` dispatch must call.
pub fn generate(prog: &IrProgram) -> String {
    let src = shir_to_perl(&fixups(prog));
    let src = post_pass(&src);
    // The shared renderer emits only the helpers it knows about, so the
    // `__sh_fn_status` helper `fix_cond` introduces is injected here. Gated on
    // the rendered body, same discipline as the renderer's own preamble
    // vars: a program that never calls it gets no dead sub.
    if needs_fn_status_helper(&src) {
        // `sub` is file scope: it must precede the first use, and Perl
        // resolves a named sub called without parens at compile time. Put it
        // right after the `use` block, before any statement runs.
        inject_before_first_stmt(&src, FN_STATUS_PREAMBLE)
    } else {
        src
    }
}

/// The textual post-pass over a rendered program.
///
/// The fixes that need it are structural facts about the *rendered* text —
/// a sub's extent, which argument a `printf` format is — and the shared
/// renderer, which owns both decisions, is not ours to change. Everything
/// here is anchored on the shape it rewrites and leaves anything else alone.
fn post_pass(src: &str) -> String {
    let out = fix_sub_params(src);
    let out = fix_case_pattern_tail(&out);
    fix_cmd_capture_split(&out)
}

/// Fix 5 — a command substitution whose output is ONE token must not be erased.
///
/// The renderer wraps every `$( … )` in
///
/// ```perl
/// join(' ', split(/\s+/, do { my $__cap; … $__cap =~ s/\n+\z//; $__cap }))
/// ```
///
/// which round-trips multi-word output but DESTROYS single-word output:
/// `split(/\s+/, "")` returns the empty list (Perl's documented special case
/// for splitting an empty string), so `x=$(echo hi)` became `x=""`. In
/// 051_primes that emptied `$sqrt_n`, `is_prime` then compared every `n`
/// against `""`, and the prime list was entirely wrong.
///
/// `join`+`split` round-trips any string that is non-empty and contains no
/// leading/trailing whitespace, so the fix is to stop discarding a result the
/// call site actually has words for: wrap the `join` in scalar context when the
/// joined value is used as ONE scalar (assignment to a scalar, an arithmetic
/// operand, a `[ … ]` test, a comparison, `return`).
///
/// The transform is anchored on the exact rendered prefix and its
/// matching close paren, counted with a scanner that understands single-quoted
/// strings — the capture body is full of both `'` and `{`/`}`, so a naive
/// brace count would stop at the wrong place.
fn fix_cmd_capture_split(src: &str) -> String {
    const PREFIX: &str = "join(' ', split(/\\s+/, do {";
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    loop {
        let Some(at) = rest.find(PREFIX) else {
            out.push_str(rest);
            return out;
        };
        let open = at + "join(".len();
        let Some(close) = matching_paren(rest, open) else {
            // Unterminated on this line: the renderer's business.
            out.push_str(&rest[..at]);
            out.push_str(&rest[at..]);
            return out;
        };
        // Both sides are needed: `my $x = join(…)` is decided by the target
        // on the LEFT, `(0+join(…))` by the operand on the RIGHT.
        let before = &rest[..at];
        let before_line = match before.rfind('\n') {
            Some(nl) => &before[nl + 1..],
            None => before,
        };
        let after = &rest[close + 1..];
        let scalar_use = scalar_assign_before(before_line) || starts_scalar_use(after);
        out.push_str(&rest[..open]);
        if scalar_use {
            out.push_str("scalar(");
        }
        out.push_str(&rest[open..=close]);
        if scalar_use {
            out.push(')');
        }
        rest = &rest[close + 1..];
    }
}

/// Does the text ending this line assign the capture to a SCALAR?
///
/// `my $x = …`, `$x = …` and `$ENV{X} = …` all want one value. `my @a = …`
/// does NOT — that is a list assignment and must stay a list, so the sigil of
/// the assignment target is what decides.
fn scalar_assign_before(before: &str) -> bool {
    let t = before.trim_end();
    let Some(eq) = t.rfind('=') else {
        return false;
    };
    // `==`, `!=`, `<=`, `>=` are comparisons, not assignments.
    let b = t.as_bytes();
    if b[eq] == b'=' {
        // `=` directly after `<`, `>`, `!`, `=` …
        if matches!(b.get(eq.wrapping_sub(1)), Some(b'=') | Some(b'!') | Some(b'<') | Some(b'>')) {
            return false;
        }
    } else {
        return false;
    }
    // Walk back over the assignment target to its sigil.
    let lhs = t[..eq].trim_end();
    let Some(sigil) = lhs.chars().last() else {
        return false;
    };
    match sigil {
        // A scalar, or a hash element (`$h{key}`) — one value either way.
        '$' => true,
        // Arrays/hashes take a list; `@`/`%` must keep the original shape.
        _ => false,
    }
}

/// The index of the `)` that closes the `(` at `open`.
///
/// The capture body mixes quote styles — `join(' ', …)` is single-quoted,
/// `print int(sqrt(($n))), "\n"` is double-quoted, and `q{…}`/`qq{…}`
/// literals appear in the middle — so the scanner has to track all of them
/// or it stops at the first `)` inside a string. Only `(`/`)` outside any
/// literal are counted, and an unterminated literal runs to end-of-line
/// (the renderer never splits one across lines), which is what the rendered
/// text means here.
///
/// Returns `None` if the line ends unbalanced, which is the renderer's
/// business, not ours.
fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            // Single-quoted, double-quoted, and backtick — a `'` inside a
            // double-quoted string is literal and must not end this one.
            b'\'' | b'"' | b'`' => {
                let q = b[i];
                i += 1;
                while i < b.len() && b[i] != b'\n' {
                    if b[i] == b'\\' && q != b'\'' {
                        i += 1;
                    } else if b[i] == q {
                        break;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'q' | b'Q' | b'w' => {
                if matches!(b.get(i + 1), Some(b'{') | Some(b'(') | Some(b'[') | Some(b'<'))
                {
                    let ob = b[i + 1];
                    let cb = match ob {
                        b'{' => b'}',
                        b'(' => b')',
                        b'[' => b']',
                        _ => b'>',
                    };
                    let mut d = 0i32;
                    i += 2;
                    while i < b.len() && b[i] != b'\n' {
                        match b[i] {
                            c if c == ob => d += 1,
                            c if c == cb => {
                                d -= 1;
                                if d == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        i += 1;
                    }
                    i += 1;
                } else {
                    i += 1;
                }
            }
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
        }
    }
    None
}

/// Does the text after a capture's close paren use the value as ONE scalar?
///
/// `my $x = (…)`, `((0+$x) …)`, `[ -z (…) ]` and `return (…)` all want the
/// whole output as a single string; `$x = (…)` in a list context (an array
/// element or an argument) does not, and is left on the original shape.
fn starts_scalar_use(after: &str) -> bool {
    let mut t = after.trim_start();
    // Strip the statement it continues so `my $x = join(…)` is recognised.
    for lead in [
        "my ", "our ", "local ", "my(", "return ", "print ", "say ",
    ] {
        if let Some(r) = t.strip_prefix(lead) {
            let r = r.trim_start();
            // `$name = …` — a plain scalar assignment.
            if let Some(eq) = find_scalar_assign(r) {
                return eq;
            }
            t = r;
            break;
        }
    }
    // An arithmetic or comparison context: `(0+$x)`, `$x + 1`, `-eq`, …
    let c = t.as_bytes().first().copied().unwrap_or(b'\0');
    c == b'(' || t.starts_with('"')
}

/// `true` when `s` is `<scalar> = <rest>`, i.e. the value lands in a scalar.
fn find_scalar_assign(s: &str) -> Option<bool> {
    let b = s.as_bytes();
    if b.is_empty() || !(b[0] == b'$' || b[0] == b'{') {
        return None;
    }
    let mut i = 0;
    // The variable reference.
    if b[0] == b'{' {
        while i < b.len() && b[i] != b'}' {
            i += 1;
        }
        i += 1;
    } else {
        while i < b.len() && (b[i] == b'$' || b[i] == b'_' || b[i].is_ascii_alphanumeric()) {
            i += 1;
        }
    }
    // Optional subscript / attribute, then the `=`.
    while i < b.len() && (b[i] == b'[' || b[i] == b'{' || b[i].is_ascii_alphanumeric()) {
        i += 1;
    }
    let t = s[i..].trim_start();
    if let Some(r) = t.strip_prefix('=') {
        // `==` is a comparison, not an assignment.
        if r.starts_with('=') {
            return Some(false);
        }
        return Some(true);
    }
    Some(false)
}

/// Every rendered `sub NAME { … }` body, top level and nested.
///
/// The shared emitter writes one line per statement (with its `continue` /
// `__ok` bookkeeping), so the closing brace of a sub at nesting depth `d` is
/// the next line that is exactly `d` closing braces. Counting braces from the
/// `{` that opens the body gives the extent without a Perl parser.
fn sub_spans(src: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut line_start = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'\n' {
            i += 1;
            continue;
        }
        let line = &src[line_start..i];
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("sub ") else {
            i += 1;
            line_start = i;
            continue;
        };
        let name: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        // The signature runs to the first `{` that opens the body.
        let Some(open_rel) = rest.find('{') else {
            i += 1;
            line_start = i;
            continue;
        };
        // `rest` starts at the first non-space byte of the line, so the
        // `{` sits this far into the line.
        let open = open_rel + (line.len() - rest.len());
        // Only a REAL function definition opens its body with the `{` we
        // just found. The renderer's own preamble contains other blocks that
        // a bare `sub ` prefix cannot distinguish (`BEGIN { ... }` in the
        // UU-FFI preamble, and `sub NAME { ... }` written on one line), and
        // treating a non-`{` opener as a body start produced a span that ran
        // backwards. Require the `{` to be the last thing on the line, with
        // only a comment after it — the renderer's signature shape.
        let after_open = &src[open + 1..];
        let tail = match after_open.find('\n') {
            Some(nl) => after_open[..nl].trim(),
            None => after_open.trim(),
        };
        if !tail.is_empty() && !tail.starts_with('#') {
            i += 1;
            line_start = i;
            continue;
        }
        // The body starts immediately after the opening brace. It is NOT
        // tied to the end of the line: the renderer also emits one-line subs
        // (`sub f { ... }` — a user function whose whole body is a single
        // statement), and anchoring `body_start` to the newline made such a
        // sub's span read start-after-end and panic on the slice below.
        //
        // The one thing that must NOT be body is a comment riding the
        // signature line (`sub f {  # line 4`). Anything injected at
        // `body_start` would land inside that comment and be swallowed by
        // it, so the span starts after the comment text but KEEPS the
        // newline that terminates the signature line — the injected text
        // then lands on its own line.
        let body_start = match tail.starts_with('#') {
            false => open + 1,
            true => match src[open + 1..].find('\n') {
                Some(nl) => open + 1 + nl,
                None => open + 1,
            },
        };
        let mut depth = 0i32;
        let mut p = open;
        while p < bytes.len() {
            match bytes[p] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        // `p` is a byte index that has landed on an ASCII
                        // `}`, so it is a char boundary by construction.
                        out.push((body_start, p, name));
                        break;
                    }
                }
                _ => {}
            }
            p += 1;
        }
        i += 1;
        line_start = i;
    }
    out.sort_by_key(|(s, _, _)| *s);
    out
}

/// Fix 2 (textual half) — bind and rewire a function's positional parameters.
///
/// Applied innermost-first: an outer sub's body contains the inner sub's
/// text, so rewriting inner bodies first and then splicing them into the
/// outer extent keeps the offsets of the not-yet-visited bodies valid.
fn fix_sub_params(src: &str) -> String {
    let spans = sub_spans(src);
    if spans.is_empty() {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len());
    let mut cursor = 0usize;
    // `(start, end, name)` ascending; walk it backwards so each splice is
    // applied to the not-yet-copied prefix.
    let mut i = spans.len();
    while i > 0 {
        i -= 1;
        let (start, end, _name) = spans[i].clone();
        let body = &src[start..end];
        let fixed = inject_param_binding(&rewrite_positional_params(body));
        out.push_str(&src[cursor..start]);
        out.push_str(&fixed);
        cursor = end;
    }
    out.push_str(&src[cursor..]);
    declare_param_array(&out)
}

/// `@__sh_args` is a package array under `use strict` — declare it once, at
/// file scope, on the same line as the renderer's own preamble `my` vars so
/// a program with no functions still carries no dead declaration.
fn declare_param_array(src: &str) -> String {
    if !src.contains("$__sh_args[") {
        return src.to_string();
    }
    for line in src.split_inclusive('\n') {
        if line.trim_start().starts_with("my $main_exit_code") {
            return format!("{}our @__sh_args;\n{}", line, &src[line.len()..]);
        }
    }
    src.to_string()
}

/// Fix 3 — a command-substitution case PATTERN must be evaluated.
///
/// The renderer desugars `case X in $(cmd)) …` into an `if` whose regex is
/// the pattern text with the substitution left unexpanded, so the pattern can
/// never match a literal string (parse-redirect-in-case-pattern matched
/// nothing). A `$(…)` inside a pattern is a command substitution the shell
/// runs before matching, so evaluate it in place.
///
/// Only the tail is quoted, which is what makes this safe: the rendered form
/// is `q{$( … )}` or `qq{$( … )}`, and leaving the head unquoted means a
/// pattern that is a bare `/regex/` or `text` is untouched.
fn fix_case_pattern_tail(src: &str) -> String {
    const NEEDLES: [&str; 4] = ["q{$(", "qq{$(", "q($(", "qq($("];
    let mut out = src.to_string();
    for needle in NEEDLES {
        out = quote_cmdsub_tails(&out, needle);
    }
    out
}

/// Replace the final `)}` of every `q{$( … )}` occurrence with `)qx( … )}`.
fn quote_cmdsub_tails(src: &str, needle: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(at) = rest.find(needle) {
        // A `q{` opener whose head must stay unquoted: bail on this one if the
        // command substitution is unterminated (a malformed pattern we do not
        // understand is the renderer's business).
        let head_end = at + 1; // just past `q`
        let Some(close) = rest[head_end..].find(")}") else {
            break;
        };
        out.push_str(&rest[..head_end]);
        out.push_str(&format!("qx({}", &rest[head_end..head_end + close]));
        out.push_str(")}");
        rest = &rest[head_end + close + 2..];
        let _ = at;
    }
    // The untouched tail is appended UNCONDITIONALLY. It used to be guarded
    // by a `found_any` flag, so when the needle was absent from the whole
    // program — i.e. the overwhelmingly common case, since the four needles
    // are `q{$(` / `qq{$(` / `q($(` / `qq($(` and only a case PATTERN with a
    // command substitution produces one — this returned the empty `out` and
    // DISCARDED the entire rendered program. Every program without a
    // cmdsub-in-case-pattern became a 0-byte Perl file that still exited 0,
    // which is the worst possible failure: a gate sees a clean run that
    // printed nothing.
    out.push_str(rest);
    out
}

/// Insert `text` after the leading shebang / `use` / `my`-declaration block
/// and before the first executable statement.
fn inject_before_first_stmt(src: &str, text: &str) -> String {
    let mut insert_at = 0usize;
    for line in src.split_inclusive('\n') {
        let t = line.trim_start();
        // Keep walking past the shebang, `use`/`no` pragmas, blank lines and
        // the `my $main_exit_code` / `our $CHILD_ERROR` declarations the
        // renderer puts at the top.
        if t.starts_with("#!")
            || t.starts_with("use ")
            || t.starts_with("use(")
            || t.starts_with("no ")
            || t.is_empty()
            || t.starts_with("my $main_exit_code")
            || t.starts_with("our $CHILD_ERROR")
        {
            insert_at += line.len();
        } else {
            break;
        }
    }
    let mut out = String::with_capacity(src.len() + 256);
    out.push_str(&src[..insert_at]);
    out.push_str(text);
    out.push('\n');
    out.push_str(&src[insert_at..]);
    out
}

/// Apply every IR-level correction to a clone of `prog`.
fn fixups(prog: &IrProgram) -> IrProgram {
    let fns = program_function_names(&prog.stmts, &prog.subs);
    let mut fixed = prog.clone();
    fixed.stmts = fix_stmts(&prog.stmts, &fns);
    // The eval statement is not a normal Call as far as the IR is concerned:
    // its payload is SHELL TEXT that bash runs in the caller's scope, so it
    // is expanded (Fix 6) and then re-entered as a nested program (Fix 7).
    // Do that AFTER fix_stmts so the expansions are already corrected.
    fixed.stmts = fix_eval_stmts(&prog.stmts);
    fixed.subs = prog
        .subs
        .iter()
        .map(|s| crate::ir::IrSub {
            name: s.name.clone(),
            params: s.params.clone(),
            body: fix_stmts(&s.body, &fns),
        })
        .collect();
    fixed
}

// ── Fix 2 — a bash function's positional parameters ──────────────────
//
// The renderer binds a shell function's parameters as `local @ARGV = @_;`
// and leaves the body's `$1`..`$n` reads alone. `do FILE` empties `@ARGV`,
// so every `$1` in the body reads `undef` and the sub silently computes over
// nothing (063_09 printed "Processed 0 options" instead of "Processed 4").
//
// bash parameters are positional and 1-based, and `$0` inside a function is
// the SCRIPT name, not the function name. Binding `@_` into a 0-based
// `@__sh_args` and rewiring the reads is therefore what the body means —
// and the shared renderer already renders a function's own parameters that
// way (`my ($a, $b) = @_;`), so this makes the body agree with its own
// signature.
//
// Scope: only the names this module introduced. An `@ARGV` read the script
// itself makes (the CLI's `do FILE` contract) is left alone.

/// Rewrite the positional-parameter reads of a rendered sub body into
/// `@__sh_args`.
///
/// Deliberately a string rewrite over the rendered text: `$1` is a Perl
/// *scalar* read (the 10th argument of the enclosing `print`, not the
/// function's), so it cannot be expressed as an `IrExpr` the shared renderer
/// would agree with. We therefore substitute the whole construct and keep
/// `$1` inside the replacement out of the scanner's reach.
fn rewrite_positional_params(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len() + 64);
    let mut i = 0usize;
    // Char offset (into `out`) of each currently-open `(`, so the depth walk
    // below never splits a multi-byte char.
    let mut opens: Vec<usize> = Vec::new();

    while i < b.len() {
        match b[i] {
            b'(' => {
                opens.push(out.chars().count());
                out.push('(');
                i += 1;
            }
            b')' => {
                opens.pop();
                out.push(')');
                i += 1;
            }
            b'$' => {
                let Some((digits, end)) = decimal_after_dollar(b, i) else {
                    out.push('$');
                    i += 1;
                    continue;
                };
                let n: usize = std::str::from_utf8(&b[i + 1..i + 1 + digits])
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                let Some(open) = opens.last().copied() else {
                    out.push('$');
                    i += 1;
                    continue;
                };
                // Inside a construct we are already rewriting: emit verbatim so
                // the inner `$1` cannot match again on the next iteration.
                if in_param_construct(&out, open) {
                    out.push('$');
                    i += 1;
                    continue;
                }
                // The subscript we emit is CLOSED (`$__sh_args[n]`), so it is
                // not an enclosing construct for the parameters that follow
                // it: `print($1, $2, $10)` must rewrite all three.
                out.push_str(&format!("$__sh_args[{}]", n - 1));
                i = end;
            }
            c if c.is_ascii() => {
                out.push(c as char);
                i += 1;
            }
            _ => {
                // A multi-byte UTF-8 sequence: copy the whole char, not one
                // byte re-encoded (which would corrupt the source).
                let ch = src[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    out
}

/// Is there an UNCLOSED `$__sh_args[ … ]` substitution between `open` and
/// the end of `out`? (i.e. are we already inside a construct being
/// rewritten, so the `$1` we are looking at belongs to its back-reference?)
/// Only an UNTERMINATED one counts. A complete substitution is not an
/// enclosing construct for the parameters that follow it — toggling parity
/// over closed ones made `print($1, $2, $10)` rewrite only `$1`, since the
/// emitted `$__sh_args[0]` left the guard latched for the rest of the list.
fn in_param_construct(out: &str, open: usize) -> bool {
    let Some(tail) = out.get(open..) else {
        return false;
    };
    let mut rest = tail;
    while let Some(at) = rest.find("$__sh_args[") {
        let after = at + "$__sh_args[".len();
        let Some(rel) = rest[after..].find(']') else {
            // Unterminated: we ARE inside it.
            return true;
        };
        rest = &rest[after + rel + 1..];
    }
    false
}

/// Length of the decimal run starting at `b[i]` (which must be a digit), and
/// the index just past it.
fn decimal_after_dollar(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut j = i + 1;
    while j < b.len() && b[j].is_ascii_digit() {
        j += 1;
    }
    if j > i + 1 {
        Some((j - i - 1, j))
    } else {
        None
    }
}

/// Insert `text` at the head of a rendered `sub` body.
///
/// The body starts after the `{`, and the renderer puts its own
/// `local @ARGV = @_;` there — so this lands before it, which is what a
/// bash function's `$1` needs: `@_` is untouched by that `local`, and
/// `@__sh_args` must be in place before any statement reads it.
fn inject_param_binding(sub_src: &str) -> String {
    let mut out = String::with_capacity(sub_src.len() + 40);
    let mut lines = sub_src.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    out.push_str(first);
    out.push_str("    local @__sh_args = @_;\n");
    for l in lines {
        out.push_str(l);
    }
    out
}


/// Every shell function name defined in the program, at any nesting depth.
///
/// The shared renderer collects the same set (`collect_fn_names`) to decide
/// that an `exec(name, …)` is a call to a local function rather than an
/// external command. We must agree with it on WHICH names those are,
/// otherwise this pass would rewrite a call the renderer treats as external.
fn program_function_names(
    stmts: &[IrStmt],
    subs: &[crate::ir::IrSub],
) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for s in subs {
        out.insert(s.name.clone());
    }
    collect_fn_names(stmts, &mut out);
    out
}

fn collect_fn_names(stmts: &[IrStmt], out: &mut std::collections::HashSet<String>) {
    for s in stmts {
        match s {
            IrStmt::Function {
                name, body, named_blocks, ..
            } => {
                out.insert(name.clone());
                collect_fn_names(body, out);
                for (_, nb) in named_blocks {
                    collect_fn_names(nb, out);
                }
            }
            IrStmt::If {
                then, elsifs, else_, ..
            } => {
                collect_fn_names(then, out);
                for (_, b) in elsifs {
                    collect_fn_names(b, out);
                }
                collect_fn_names(else_, out);
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Block(body) => collect_fn_names(body, out),
            IrStmt::ForInit {
                init, step, body, ..
            } => {
                collect_fn_names(init, out);
                collect_fn_names(step, out);
                collect_fn_names(body, out);
            }
            _ => {}
        }
    }
}

/// The command name of an `exec`/`builtin` call, if it is a literal.
fn exec_cmd_name(args: &[IrExpr]) -> Option<&str> {
    match args.first() {
        Some(IrExpr::Str(s, _)) => Some(s.as_str()),
        _ => None,
    }
}

fn fix_stmts(stmts: &[IrStmt], fns: &std::collections::HashSet<String>) -> Vec<IrStmt> {
    stmts.iter().map(|s| fix_stmt(s, fns)).collect()
}

// ── Fix 7 — an `eval` payload is SHELL TEXT, not arguments ────────────
//
// `eval "y=$x+1"` is not a call with a string argument. Its payload is shell
// text that bash parses and runs in the CALLER's scope: the variables it
// assigns, the functions it defines and the `return` it performs all belong
// to the enclosing program, not to a subshell.
//
// The shared renderer cannot express that. `eval` is a shell builtin, so the
// IR carries it as `Call { func: "builtin", args: [Str("eval"), Array([…])] }`
// — the same shape as any other command — and `shir_to_perl` emits
//
//     system('bash', '-c', q{'eval' "echo \"$x\""});
//
// which is wrong twice over. It quotes the command name AND the payload as
// argv to a fresh `bash -c`, so the payload runs in a child process: every
// assignment is lost the moment `eval` returns, and the inner `$x` is the
// child's, not the caller's. It also re-invokes `eval` inside that child,
// which bash resolves before the caller's assignments are visible.
//
// The correction is to RE-ENTER the payload: parse the shell text and splice
// its statements into the program at this position, so they run inline in
// the caller's scope exactly as bash would. That is what the caller's
// `fixups` comment describes ("expanded (Fix 6) and then re-entered as a
// nested program (Fix 7)") and what this function implements.
//
// # Safety of doing this structurally
//
// The payload is RE-PARSED, so a `return` inside it returns from the
// enclosing function rather than from an `eval` — which is correct, because
// in bash `eval "return 3"` inside a function returns from that function.
//
// Deliberately narrow, per this module's rule:
//   * only `eval` whose payload is a single word we can recover TEXT for;
//   * only when that text parses; a parse failure keeps the original call,
//     because a payload we cannot understand is the shared renderer's
//     business and a wrong inlining is worse than the status quo;
//   * the spliced statements are NOT re-fixed here. `fixups` calls this
//     AFTER `fix_stmts`, so the expansion lands in an already-corrected
//     program and running the fixes again would double-apply Fix 4.

/// Fix 7 — expand `eval "<shell text>"` into inline statements.
fn fix_eval_stmts(stmts: &[IrStmt]) -> Vec<IrStmt> {
    let mut out = Vec::with_capacity(stmts.len());
    for s in stmts {
        match s {
            IrStmt::Expr(expr) => {
                if let Some(text) = eval_payload_text(expr) {
                    match parse_shell_text(&text) {
                        Some(inner) => out.extend(inner),
                        // Unparseable payload: leave the call as it was.
                        None => out.push(s.clone()),
                    }
                } else {
                    out.push(s.clone());
                }
            }
            // Recurse into every statement position that holds a body, so an
            // `eval` inside a loop / function / branch is expanded too.
            IrStmt::If { cond, then, elsifs, else_ } => out.push(IrStmt::If {
                cond: cond.clone(),
                then: fix_eval_stmts(then),
                elsifs: elsifs
                    .iter()
                    .map(|(c, b)| (c.clone(), fix_eval_stmts(b)))
                    .collect(),
                else_: fix_eval_stmts(else_),
            }),
            IrStmt::While { cond, body } => out.push(IrStmt::While {
                cond: cond.clone(),
                body: fix_eval_stmts(body),
            }),
            IrStmt::DoWhile { cond, body, until } => out.push(IrStmt::DoWhile {
                cond: cond.clone(),
                body: fix_eval_stmts(body),
                until: *until,
            }),
            IrStmt::Block(b) => out.push(IrStmt::Block(fix_eval_stmts(b))),
            IrStmt::Subshell(b) => out.push(IrStmt::Subshell(fix_eval_stmts(b))),
            IrStmt::Background(b) => out.push(IrStmt::Background(fix_eval_stmts(b))),
            IrStmt::ForInit {
                init,
                cond,
                step,
                body,
            } => out.push(IrStmt::ForInit {
                init: fix_eval_stmts(init),
                cond: cond.clone(),
                step: fix_eval_stmts(step),
                body: fix_eval_stmts(body),
            }),
            IrStmt::Function {
                name,
                params,
                body,
                named_blocks,
            } => out.push(IrStmt::Function {
                name: name.clone(),
                params: params.clone(),
                body: fix_eval_stmts(body),
                named_blocks: named_blocks
                    .iter()
                    .map(|(n, b)| (n.clone(), fix_eval_stmts(b)))
                    .collect(),
            }),
            other => out.push(other.clone()),
        }
    }
    out
}

/// The shell TEXT of an `eval` statement's payload, if this is one.
///
/// Matches `Call { func: "builtin"|"exec", args: [Str("eval"), Array([w])] }`
/// and recovers `w`'s text. An interpolated payload is accepted only when it
/// is a single literal part: `eval "x=$y"` expands `$y` at eval time, and
/// inlining the pre-expansion text would change which value is assigned.
/// Anything we cannot reduce to fixed text is left to the renderer.
fn eval_payload_text(expr: &IrExpr) -> Option<String> {
    let IrExpr::Call { func, args } = expr else {
        return None;
    };
    if func != "builtin" && func != "exec" {
        return None;
    }
    if exec_cmd_name(args) != Some("eval") {
        return None;
    }
    let Some(IrExpr::Array(words)) = args.get(1) else {
        return None;
    };
    // `eval "$a" "$b"` concatenates its arguments; only a single-word
    // payload is a shape we can reason about.
    let [one] = &words[..] else {
        return None;
    };
    word_source_text(one)
}

/// The literal source text of a word, if it is entirely literal.
///
/// A double-quoted word arrives as `Interpolate(parts)`; the renderer keeps
/// the parts, so a payload with an embedded expression is only usable when
/// there is exactly one `Lit` part and nothing else.
fn word_source_text(w: &IrExpr) -> Option<String> {
    match w {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts) => match parts.as_slice() {
            [InterpPart::Lit(t)] => Some(t.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Parse shell text and return the IR statements it denotes.
///
/// `None` when the text does not parse, or when it uses a construct this
/// module cannot render correctly. The second condition is not caution for its
/// own sake: inlining is only an improvement when the spliced statements
/// render as well as the delegated call did. Measured, on
/// `063_12_complex_eval.sh` — `eval "result=$(( ${var:-0} + … ))"` — inlining
/// produced Perl that does not even compile (`${array[${index:_0}]:_0}` is
/// emitted verbatim inside a `my %array` program, so perl dies with
/// "Global symbol requires explicit package name"), whereas the delegated
/// form at least emitted and ran. A worse result than the status quo is the
/// one outcome this module's rules forbid, so the guard is on RENDERABILITY,
/// not on whether the text parses.
///
/// The allowed set is deliberately small and is checked per statement:
/// assignments and plain command calls. Anything else — loops, conditionals,
/// arithmetic, nested control flow — is left to the shared renderer, which
/// owns those decisions.
fn parse_shell_text(text: &str) -> Option<Vec<IrStmt>> {
    let mut p = crate::Parser::new(text);
    let commands = p.parse().ok()?;
    // `ast_to_ir_raw`, NOT `ast_to_ir`: the latter runs `optimize_stmts`,
    // whose dead-assignment elimination is correct for a whole program and
    // destroys an eval payload. `eval "y=7"` is a program whose ONLY
    // statement is an assignment that is never read — dead-store elimination
    // deletes it and the expansion becomes a no-op, so the variable the
    // caller then reads stays unset (`y=` instead of `y=7`). The payload of
    // an eval exists for its SIDE EFFECT; nothing in it is ever dead.
    let stmts = crate::shir::ast_to_ir_raw(&commands).stmts;
    if stmts.iter().all(is_inlinable_stmt) {
        Some(stmts)
    } else {
        None
    }
}

/// Is this statement one the shared Perl renderer already emits correctly?
///
/// Assignments and plain calls are the two shapes the corpus exercises
/// through `eval` and the two this module has evidence for. Everything else
/// is refused, so an unrenderable payload keeps the delegated form.
///
/// The assignment case is deliberately NARROWER than "is an Assign". A
/// payload's statements all have the right TYPE in the failing case
/// `eval "result=$(( ${var:-0} + ${array[${index:-0}]:-0} ))"` — it is a
/// single `IrStmt::Assign` — but the renderer's output for that assignment
/// is Perl that does not compile: the arithmetic expansion is emitted
/// verbatim (`${array[${index:_0}]:_0}`) into a program that has only
/// `my %array`, so perl aborts with "Global symbol requires explicit package
/// name". A type check therefore admits exactly the payload that broke, so
/// the guard is on the assignment's RIGHT-HAND SIDE: only a literal or a
/// plain call is inlined, and an arithmetic/variable expansion keeps the
/// delegated form.
fn is_inlinable_stmt(s: &IrStmt) -> bool {
    match s {
        IrStmt::Assign { expr, .. } => is_inlinable_rhs(expr),
        IrStmt::Expr(IrExpr::Call { func, .. }) => func == "exec" || func == "builtin",
        _ => false,
    }
}

/// Can this expression be rendered verbatim into the caller's program?
///
/// A literal, a plain command call, or a read of a variable. Anything that
/// would need the renderer's arithmetic / expansion machinery is refused: this
/// module does not own those decisions and a payload it cannot render is
/// better left delegated.
fn is_inlinable_rhs(e: &IrExpr) -> bool {
    match e {
        IrExpr::Str(..) | IrExpr::Int(..) => true,
        IrExpr::Call { func, .. } => {
            func == "exec" || func == "builtin" || func == "getVar" || func == "setVar"
        }
        _ => false,
    }
}

fn fix_stmt(s: &IrStmt, fns: &std::collections::HashSet<String>) -> IrStmt {
    match s {
        // Fix 4 — a bare `return 0` / `return 1` is an exit STATUS, and Perl
        // reads the two the other way round.
        //
        // The renderer emits the returned expression verbatim, so a status
        // that arrived as `IrExpr::Str("0", …)` becomes the Perl source
        // `return "0";` — a non-empty string, which is TRUE in Perl (only
        // `""`, `0` and `0.0` are false). `if is_prime $n` therefore took the
        // "prime" branch on every input (051_primes printed every odd number
        // as a prime and every "First 10" line was wrong). Correct the IR so
        // the renderer emits a numeric literal: `0` is falsy, everything else
        // truthy, which is exactly bash's `return 0` / `return 1` convention.
        //
        // Scope: only a bare numeric-literal string, and only in return
        // position — a computed string, or a `"0"` that means data, is left
        // alone.
        IrStmt::Return(Some(IrExpr::Str(n, _style))) => {
            if let Ok(code) = n.parse::<i64>() {
                return IrStmt::Return(Some(IrExpr::Int(code)));
            }
            s.clone()
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => IrStmt::If {
            cond: fix_cond(cond, fns),
            then: fix_stmts(then, fns),
            elsifs: elsifs
                .iter()
                .map(|(c, b)| (fix_cond(c, fns), fix_stmts(b, fns)))
                .collect(),
            else_: fix_stmts(else_, fns),
        },
        IrStmt::While { cond, body } => IrStmt::While {
            cond: fix_cond(cond, fns),
            body: fix_stmts(body, fns),
        },
        IrStmt::DoWhile {
            cond,
            body,
            until,
        } => IrStmt::DoWhile {
            cond: fix_cond(cond, fns),
            body: fix_stmts(body, fns),
            until: *until,
        },
        IrStmt::Block(b) => IrStmt::Block(fix_stmts(b, fns)),
        IrStmt::Subshell(b) => IrStmt::Subshell(fix_stmts(b, fns)),
        IrStmt::Background(b) => IrStmt::Background(fix_stmts(b, fns)),
        IrStmt::Function {
            name,
            params,
            body,
            named_blocks,
        } => IrStmt::Function {
            name: name.clone(),
            params: params.clone(),
            body: fix_stmts(body, fns),
            named_blocks: named_blocks
                .iter()
                .map(|(n, b)| (n.clone(), fix_stmts(b, fns)))
                .collect(),
        },
        other => other.clone(),
    }
}

/// Fix a condition expression.
///
/// # Fix 1 — a shell function called in condition position tests its EXIT
/// STATUS, not its return string.
///
/// `if is_prime $n; then …` means "run the body when `is_prime` succeeded".
/// The renderer emits `if (is_prime(...))`, and Perl tests the *return value*
/// for truthiness. A bash function signals success with `return 0` (rendered
/// as `return "0"`), which is the string `"0"` — **false** in Perl, because
/// `"0"` and `""` are the only false scalars. So the sense is inverted: the
/// body runs exactly when the function FAILED.
///
/// This is not cosmetic: for a predicate like `is_prime` it admits every
/// composite number (051_primes prints `2 9 15 21 25 …` instead of the
/// primes).
///
/// The correction: make the renderer see an explicit status comparison
/// instead of a bare call. `sub_exit_status("f", args)` compares against
/// `0`, so the emitted Perl is `if (sub_exit_status("is_prime", …))` — a real
/// true/false the renderer prints verbatim. This is anchored on the `$fn_call`
/// marker, which the shared renderer itself introduces (`rewrite_fn_calls`)
/// precisely for "a call to a function defined in this program", so it never
/// fires on an external command.
fn fix_cond(cond: &IrExpr, fns: &std::collections::HashSet<String>) -> IrExpr {
    match cond {
        // `exec(is_prime, …)` where `is_prime` is a function DEFINED IN THIS
        // PROGRAM is precisely what the shared renderer later marks as a
        // `$fn_call` (its `rewrite_fn_calls`). Anchoring on the same
        // predicate — a literal name that is a local function — means this
        // can only ever fire on a call the renderer would have turned into a
        // Perl sub call, never on an external command.
        //
        // We must match `exec`, not `$fn_call`: the marker is introduced
        // INSIDE `shir_to_perl`, i.e. after this pre-pass has run.
        IrExpr::Call { func, args } if func == "exec" || func == "builtin" => {
            let is_local_fn = exec_cmd_name(args)
                .map(|n| fns.contains(n))
                .unwrap_or(false);
            if is_local_fn {
                IrExpr::Call {
                    func: "__sh_fn_status".to_string(),
                    args: args.clone(),
                }
            } else {
                IrExpr::Call {
                    func: func.clone(),
                    args: args.clone(),
                }
            }
        }
        IrExpr::BinOp { lhs, op, rhs } => IrExpr::BinOp {
            lhs: Box::new(fix_cond(lhs, fns)),
            op: op.clone(),
            rhs: Box::new(fix_cond(rhs, fns)),
        },
        IrExpr::Ternary { cond, then, else_ } => IrExpr::Ternary {
            cond: Box::new(fix_cond(cond, fns)),
            then: then.clone(),
            else_: else_.clone(),
        },
        IrExpr::Array(elems) => {
            IrExpr::Array(elems.iter().map(|e| fix_cond(e, fns)).collect())
        }
        other => other.clone(),
    }
}

/// The Perl helper `fix_cond` makes the renderer reference.
///
/// Emitted on demand by the prelude when the body actually contains a
/// `__sh_fn_status(` call (same gating discipline as the shared renderer's own
/// `$main_exit_code` / `$CHILD_ERROR` preamble vars — nothing is emitted for
/// programs that do not need it).
pub const FN_STATUS_PREAMBLE: &str = r#"
sub __sh_fn_status {
    # A shell function called in CONDITION position is tested by its exit
    # status (`if f x; then …`), not by its return string: `return 0` means
    # success. Perl would read `return "0"` as the false string "0" and
    # invert the branch, so test the status explicitly. Mirrors the bash
    # convention the rest of the program already uses: 0 => branch taken.
    #
    # Argv: ($name, @args) — the renderer keeps the function name as the first
    # word of the call. A string is not a code ref under `use strict 'refs'`,
    # so resolve it through the symbol table.
    my $name = shift();
    no strict 'refs';
    my $rv = &{$name}(@_);
    # A function that `return`s a bare integer is signalling status; one that
    # returns something else (or nothing) is signalling via $?.
    my $st = defined($rv) && $rv =~ /^\s*-?[0-9]+\s*$/
        ? $rv + 0
        : ($? >> 8);
    $st = 1 if !defined $st;
    $CHILD_ERROR = $st;
    return $st == 0 ? 1 : 0;
}
"#;

/// True when the rendered program needs `__sh_fn_status` defined.
pub fn needs_fn_status_helper(perl_src: &str) -> bool {
    perl_src.contains("__sh_fn_status(")
}

#[cfg(test)]
mod pgf_tests {
    use super::*;
    #[test]
    fn span_is_body_only() {
        let s = "sub f {  # line 4\n    print \"x\";\n    $main_exit_code = 0;\n}\n\nprint \"after\";\n";
        let sp = sub_spans(s);
        assert_eq!(sp.len(), 1, "one sub");
        assert_eq!(&s[sp[0].0..sp[0].1], "\n    print \"x\";\n    $main_exit_code = 0;\n");
    }
    #[test]
    fn param_rewrite() {
        assert_eq!(
            rewrite_positional_params("print($1, $2, $10);\n"),
            "print($__sh_args[0], $__sh_args[1], $__sh_args[9]);\n"
        );
        assert_eq!(rewrite_positional_params("print(\"$1\");\n"), "print(\"$__sh_args[0]\");\n");
        assert_eq!(rewrite_positional_params("x =~ s{^--}{}r;\n"), "x =~ s{^--}{}r;\n");
        assert_eq!(rewrite_positional_params("my @ARGV = @_;\n"), "my @ARGV = @_;\n");
    }
    #[test]
    fn binding_injected_after_argv() {
        let body = "\n    local @ARGV = @_;\n    my $__argc = @ARGV;\n    print(1);\n";
        let out = inject_param_binding(body);
        assert!(out.contains("local @__sh_args = @_;\n"), "{out}");
        assert!(out.find("__sh_args").unwrap() < out.find("print(1)").unwrap(), "{out}");
    }
}

#[cfg(test)]
mod eval_fix_tests {
    use super::*;

    fn ir_of(src: &str) -> IrProgram {
        let mut p = crate::Parser::new(src);
        let cmds = p.parse().expect("parse");
        crate::shir::ast_to_ir(&cmds)
    }

    #[test]
    fn eval_payload_is_inlined_not_delegated() {
        // `eval "y=7"` must NOT survive as a `builtin("eval", …)` call.
        let prog = ir_of("x=1\neval \"y=7\"\necho done\n");
        let fixed = fix_eval_stmts(&prog.stmts);
        let still_eval = fixed.iter().any(|s| {
            matches!(s, IrStmt::Expr(IrExpr::Call { func, args })
                if (func == "builtin" || func == "exec")
                    && exec_cmd_name(args) == Some("eval"))
        });
        assert!(!still_eval, "eval call survived the expansion: {fixed:?}");
        // The assignment the payload carried is now a real statement AND
        // kept its value: dead-store elimination on the payload would drop
        // it, which is how `eval "y=7"` silently became a no-op.
        let assigns: Vec<_> = fixed
            .iter()
            .filter_map(|s| match s {
                IrStmt::Assign { targets, expr, .. } => Some((targets, expr)),
                _ => None,
            })
            .collect();
        assert!(
            assigns.iter().any(|(t, e)| {
                t.iter().any(|tg| tg.var == "y") && matches!(e, IrExpr::Str(v, _) if v == "7")
            }),
            "payload assignment lost or value-changed: {fixed:?}"
        );
    }

    #[test]
    fn non_eval_calls_are_untouched() {
        let prog = ir_of("echo hi\n");
        let fixed = fix_eval_stmts(&prog.stmts);
        assert_eq!(fixed.len(), prog.stmts.len(), "{fixed:?}");
    }

    #[test]
    fn a_payload_the_renderer_cannot_emit_is_left_alone() {
        // `eval "r=$(( a + b ))"` — parses fine, but inlining emits Perl
        // that does not compile. It must keep the delegated form.
        let prog = ir_of("eval \"r=$(( 1 + 2 ))\"\necho done\n");
        let fixed = fix_eval_stmts(&prog.stmts);
        assert!(
            fixed.iter().any(|s| {
                matches!(s, IrStmt::Expr(IrExpr::Call { func, args })
                    if (func == "builtin" || func == "exec")
                        && exec_cmd_name(args) == Some("eval"))
            }),
            "an unrenderable payload was inlined anyway: {fixed:?}"
        );
    }

    #[test]
    fn unparseable_payload_is_left_alone() {
        // A payload the parser rejects must not be dropped or mangled.
        let prog = ir_of("eval \"if(\"\necho after\n");
        let fixed = fix_eval_stmts(&prog.stmts);
        assert!(fixed.iter().any(|s| {
            matches!(s, IrStmt::Expr(IrExpr::Call { func, args })
                if (func == "builtin" || func == "exec")
                    && exec_cmd_name(args) == Some("eval"))
        }), "unparseable eval payload was not preserved: {fixed:?}");
    }
}




#[cfg(test)]
mod no_output_loss_tests {
    use super::*;

    // A post-pass that returns "" for an input it did not recognise is the
    // worst failure this module can have: the CLI writes a 0-byte file and
    // still exits 0, so a gate records a clean run of a program that printed
    // nothing. Both passes below are pure text rewrites, so for ANY input
    // with no needle present the output must be byte-identical to the input.
    #[test]
    fn case_pattern_tail_is_identity_without_a_needle() {
        let src = "#!/usr/bin/env perl\nuse strict;\nprint \"hi\\n\";\nexit 0;\n";
        for needle in ["q{$(", "qq{$(", "q($(", "qq($("] {
            assert_eq!(
                quote_cmdsub_tails(src, needle),
                src,
                "needle {needle:?} discarded the program"
            );
        }
    }

    #[test]
    fn fix_case_pattern_tail_is_identity_for_an_ordinary_program() {
        let src = "#!/usr/bin/env perl\nuse strict;\nprint \"hi\\n\";\nexit 0;\n";
        assert_eq!(fix_case_pattern_tail(src), src);
    }

    #[test]
    fn a_needle_that_is_present_is_still_rewritten() {
        // The rewrite must still fire — the identity fix must not have
        // disabled the pass it was repairing.
        let src = "if ($x =~ q{$(cmd)}a) { print 1; }\n";
        let out = fix_case_pattern_tail(src);
        assert!(out.contains("qx("), "the cmdsub tail was not rewritten: {out}");
    }
}








