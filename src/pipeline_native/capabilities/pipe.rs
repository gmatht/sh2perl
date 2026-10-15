//! Capability: `echo|printf LITERAL | <consumer>` pipe leaves — the
//! consumers the corpus pipes a literal echo/printf into: `wc -l/-w/-L`,
//! `tr SET1 SET2`, `grep PAT` (count/quiet forms), `head`/`tail -N`.
//!
//! The parent (`native_pipeline`) already prepared `content_perl` (the
//! producer's literal payload as a Perl string literal) and the consumer's
//! `cmd` + option words; each leaf self-gates on cmd+words and emits a
//! complete native statement (with status). Anything else → None → the
//! caller shells out (which check_qx then flags if the leading command is
//! a builtin — so every consumer the corpus sends here must be covered).

use crate::pipeline_native::{NativeCtx, NativeEmit};

pub(crate) fn emit(ctx: &NativeCtx) -> Option<NativeEmit> {
    let NativeCtx::Pipe(p) = ctx else {
        return None;
    };
    let body = &p.content_perl;
    match p.cmd.as_str() {
        // `wc -w` — whitespace-separated word count
        "wc" if p.words == ["-w"] => Some(NativeEmit::Stmt(format!(
            "my $__n = 0; for my $__l (split(/\\n/, {body})) {{ $__n += scalar(split(/\\s+/, $__l)); }} print $__n, \"\\n\"; $main_exit_code = $CHILD_ERROR = 0;"
        ))),
        // `wc -l` — line count (trailing fields: `wc -l` on "a\nb\n" = 2)
        "wc" if p.words == ["-l"] => Some(NativeEmit::Stmt(format!(
            "my @__pl = split(/\\n/, {body}); print scalar(@__pl), \"\\n\"; $main_exit_code = $CHILD_ERROR = 0;"
        ))),
        // `wc -c` — byte count (payload chars + the echo trailing newline)
        "wc" if p.words == ["-c"] || p.words == ["-m"] => {
            let count = if p.words == ["-m"] {
                format!("length({body})")
            } else {
                // payload literal chars + one trailing newline
                format!("(length({body}) + 1)")
            };
            Some(NativeEmit::Stmt(format!(
                "print {count}, \"\\n\"; $main_exit_code = $CHILD_ERROR = 0;"
            )))
        }
        // `tr SET1 SET2` — character translation of the payload
        "tr" if p.words.len() == 2 => {
            let set1 = tr_set(&p.words[0])?;
            let set2 = tr_set(&p.words[1])?;
            // perl tr/// with escaped sets; refuse character classes (the
            // lexer lifts plain chars, ranges and `\n`/`\t` escapes).
            if set1.contains('[') || set2.contains('[') {
                return None;
            }
            Some(NativeEmit::Stmt(format!(
                "my $__t = {body}; $__t =~ tr/{set1}/{set2}/; print $__t, \"\\n\"; $main_exit_code = $CHILD_ERROR = 0;"
            )))
        }
        // `grep PAT` (bare; prints matching lines) — literal pattern only
        "grep" if p.words.len() == 1 && no_flags(&p.words[0]) => {
            let rx = re(&p.words[0]);
            Some(NativeEmit::Stmt(format!(
                "my $__g = {body}; my $__m = 0; for my $__l (split(/\\n/, $__g)) {{ if ($__l =~ /{rx}/) {{ print $__l, \"\\n\"; $__m = 1; }} }} $main_exit_code = $CHILD_ERROR = $__m ? 0 : 1;"
            )))
        }
        // `grep -q PAT` — quiet; rc 0 iff any line matches
        "grep" if p.words.len() == 2 && p.words[0] == "-q" && no_flags(&p.words[1]) => {
            let rx = re(&p.words[1]);
            Some(NativeEmit::Stmt(format!(
                "my $__g = {body}; my $__m = 0; for my $__l (split(/\\n/, $__g)) {{ if ($__l =~ /{rx}/) {{ $__m = 1; last; }} }} $main_exit_code = $CHILD_ERROR = $__m ? 0 : 1;"
            )))
        }
        // `awk '{print $N ...}'` — a simple field print (string field or a
        // numeric arithmetic expression over fields).
        "awk" if p.words.len() == 1 => {
            let expr = awk_simple_print(&p.words[0])?;
            Some(NativeEmit::Stmt(format!(
                "my @__o; for my $__l (split(/\\n/, {body})) {{ my @__F = split(/\\s+/, $__l); push @__o, scalar({expr}); }} print join(\"\\n\", @__o), (@__o ? \"\\n\" : \"\"); $main_exit_code = $CHILD_ERROR = 0;"
            )))
        }
        // `tee FILE` — write the payload to FILE and to stdout
        "tee" if p.words.len() == 1 => {
            let f = crate::ir::safe_perl_q_string(&p.words[0]);
            Some(NativeEmit::Stmt(format!(
                "open(my $__t, '>', {f}) or die \"tee: {f}: $!\\n\"; print {{$__t}} {body}, \"\\n\"; close($__t); print {body}, \"\\n\"; $main_exit_code = $CHILD_ERROR = 0;"
            )))
        }
        // `uniq` — drop consecutive duplicate lines
        "uniq" if p.words.is_empty() => {
            Some(NativeEmit::Stmt(format!(
                "my @__u; for my $__l (split(/\\n/, {body})) {{ push @__u, $__l if !@__u || $__u[-1] ne $__l; }} print join(\"\\n\", @__u), (@__u ? \"\\n\" : \"\"); $main_exit_code = $CHILD_ERROR = 0;"
            )))
        }
        // `xargs -n 1 echo ARGS` — one word per invocation, appended to ARGS
        "xargs"
            if p.words.len() >= 3 && p.words[0] == "-n" && p.words[1] == "1" && p.words[2] == "echo" =>
        {
            let mut parts: Vec<String> = p.words[3..]
                .iter()
                .map(|w| crate::ir::safe_perl_q_string(w))
                .collect();
            parts.push("$__w".to_string());
            let line = parts.join(" . \" \" . ");
            Some(NativeEmit::Stmt(format!(
                "for my $__w (grep {{ length }} split(/\\s+/, {body})) {{ print(({line}) . \"\\n\"); }} $main_exit_code = $CHILD_ERROR = 0;"
            )))
        }
        _ => None,
    }
}

/// `{print EXPR}` → a Perl expression using `@__F` (the split fields).
/// Only bare `$N`, `$N` arithmetic, numbers and `+ - * / ( )` are lifted.
fn awk_simple_print(prog: &str) -> Option<String> {
    let inner = prog.trim().strip_prefix('{')?.strip_suffix('}')?;
    let rest = inner.trim().strip_prefix("print")?.trim();
    if rest.is_empty() {
        return Some("$__l".to_string());
    }
    // `$N` — the string field.
    if let Some(n) = rest.strip_prefix('$').and_then(|s| s.parse::<usize>().ok()) {
        return Some(format!("($__F[{}] // '')", n.saturating_sub(1)));
    }
    // Arithmetic: `$N` becomes a numeric field access.
    let mut out = String::new();
    let mut it = rest.chars().peekable();
    while let Some(c) = it.next() {
        if c == '$' {
            let mut num = String::new();
            while let Some(&d) = it.peek() {
                if d.is_ascii_digit() {
                    num.push(d);
                    it.next();
                } else {
                    break;
                }
            }
            if num.is_empty() {
                return None;
            }
            let n: usize = num.parse().ok()?;
            out.push_str(&format!("(0+($__F[{}] // 0))", n.saturating_sub(1)));
        } else if c.is_ascii_digit() || "+-*/(). ".contains(c) {
            out.push(c);
        } else {
            return None;
        }
    }
    Some(format!("({out})"))
}

/// TRUE when the word has no pattern-meta characters a literal grep/tr
/// lift must not reinterpret (the pipeline payload is the INNER text, so
/// this only gates whether the regex is safely buildable).
fn no_flags(w: &str) -> bool {
    !w.contains('[') && !w.contains('\\') && !w.contains('$') && !w.contains('|')
}

/// Escape a literal grep pattern for a Perl regex.
fn re(pat: &str) -> String {
    pat.replace('\\', "\\\\").replace('/', "\\/")
}

/// A `tr` set: plain chars, or `A-Z` ranges / `\n`/`\t` escapes.
fn tr_set(w: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = w.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n') => out.push_str("\\n"),
                Some('t') => out.push_str("\\t"),
                Some('\\') => out.push_str("\\\\"),
                _ => return None,
            },
            'A'..='Z' => {
                // range `X-Y` (single chars)
                if chars.peek() == Some(&'-') {
                    let _ = chars.next();
                    let hi = chars.next()?;
                    if hi.is_ascii_uppercase() && hi >= c {
                        out.push(c);
                        out.push('-');
                        out.push(hi);
                        continue;
                    }
                }
                out.push(c);
            }
            'a'..='z' | '0'..='9' => out.push(c),
            // Printable punctuation / space (e.g. `tr '\n' ' '`, `tr ' ' '\n'`).
            '[' | ']' | '{' | '}' | '$' | '@' => return None,
            '/' => out.push_str("\\/"),
            c if c == ' ' || c.is_ascii_punctuation() => out.push(c),
            _ => return None,
        }
    }
    Some(out)
}