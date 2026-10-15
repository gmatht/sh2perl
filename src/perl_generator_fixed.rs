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

use crate::ir::{shir_to_perl, InterpPart, IrExpr, IrProgram, IrRedirect, IrStmt};

/// The Perl GENERATOR entry point: shared renderer + this module's fixes.
///
/// This is what `otranspilerl-cli`'s `--target pl` dispatch must call.
pub fn generate(prog: &IrProgram) -> String {
    if std::env::var("PG_DUMP_IR").is_ok() {
        eprintln!("PG_DUMP_IR: {prog:#?}");
    }
    // The array/hash census must be populated BEFORE `fixups`, not only
    // inside `shir_to_perl`: `fix_array_slice_bounds` consults it (via
    // `array_var`) to tell a scalar `${s:off:len}` (a SUBSTRING) from an
    // array slice, and with an empty census it refuses every slice — so
    // `${primes[@]: -10}` rendered as the WHOLE array
    // (`@primes[0..$#primes]`) and 051_primes printed all 100 primes after
    // "Last 10:". `shir_to_perl` re-populates it for its own use; this call
    // is idempotent.
    crate::ir::populate_var_census(prog);
    let fixed = fixups(prog);
    let src = shir_to_perl(&fixed);
    if std::env::var_os("SH2_DEBUG_PGFIX").is_some() {
        eprintln!("PGFIX-RENDERED:\n{src}");
    }
    let src = post_pass(&src);
    if std::env::var_os("SH2_DEBUG_PGFIX").is_some() {
        eprintln!("PGFIX-POSTPASS:\n{src}");
    }
    // The shared renderer emits only the helpers it knows about, so the
    // `__sh_fn_status` helper `fix_cond` introduces is injected here. Gated on
    // the rendered body, same discipline as the renderer's own preamble
    // vars: a program that never calls it gets no dead sub.
    let mut src = if needs_fn_status_helper(&src) {
        // `sub` is file scope: it must precede the first use, and Perl
        // resolves a named sub called without parens at compile time. Put it
        // right after the `use` block, before any statement runs.
        inject_before_first_stmt(&src, FN_STATUS_PREAMBLE)
    } else {
        src
    };
    // Fix 16 — same discipline for the array-slice helper.
    if needs_slice_helper(&src) {
        src = inject_before_first_stmt(&src, SLICE_PREAMBLE_2);
    }
    // Fix 22 — and for the positional-parameter slice helper.
    if needs_pos_slice_helper(&src) {
        src = inject_before_first_stmt(&src, SLICE_PREAMBLE);
    }
    src
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
    let out = fix_heredoc_delim(&out);
    let out = fix_cmd_capture_split(&out);
    let out = fix_for_array_iterand(&out);
    let out = fix_for_capture_paren(&out);
    let out = fix_glob_sentinel_in_for_iterand(&out);
    let out = fix_bareword_words(&out);
    let out = fix_condition_booleans(&out);
    let out = fix_shell_builtin_command_name(&out);
    let out = fix_eval_payload_expansion(&out);
    let out = fix_printf_bash_conversions(&out);
    let out = fix_printf_b_percent_b(&out);
    let out = fix_pos_param_slice(&out);
    // LAST: it must see the final text, and it only ever PREPENDS
    // declarations, so it cannot disturb what the earlier passes matched.
    let out = bind_toplevel_positional_params(&out);
    declare_undeclared_vars(&out)
}

// ── Top-level positional parameters ───────────────────────────────────
//
// Inside a sub, Fix 2 binds `@_` into `@__sh_args` and rewires `$1` to
// `$__sh_args[0]` — a sub's `$1` is ITS first parameter. At file scope
// there is no `@_`, so the renderer emits the read straight against `@ARGV`
// (`$ARGV[0]` for `$1`, `@ARGV` for `"$@"`).
//
// That is wrong under `./fail`, which invokes the program as
// `perl -e '$0 = shift @ARGV; my $f = shift @ARGV; do $f' SCRIPT FILE` —
// `do` runs the file in the main scope with `@ARGV` EMPTIED, so every
// `$ARGV[0]` read is `undef`. bash, given the same empty argument list,
// treats a missing `$1` as the EMPTY STRING, so the two diverge by exactly
// "undef warning plus a field where bash had an empty one"
// (parse-at-slice-param.sh).
//
// Binding file-scope `@ARGV` into `@__sh_args` makes the top-level read
// agree with bash's: both are empty when the script is given no arguments.
// The binding is placed before the first statement, matching where
// `inject_param_binding` puts a sub's.

/// Bind the file-scope positional parameters to `@__sh_args` and rewire the
/// renderer's `$ARGV[ … ]` / `$#ARGV` reads to it.
fn bind_toplevel_positional_params(src: &str) -> String {
    // `$ARGV[` reads and `$#ARGV` are the ONLY top-level spellings the
    // renderer produces for `$1` / `${@:…}`. The slice fix above already
    // rewrote `@ARGV[lo..hi]` slices, so what is left here is a bare
    // element read.
    if !src.contains("$ARGV[") && !src.contains("$#ARGV") {
        return src.to_string();
    }
    let out = replace_all_outside_strings(src, "$ARGV[", "$__sh_args[");
    let out = replace_all_outside_strings(&out, "$#ARGV", "$__sh_args_n");
    let out = replace_all_outside_strings(&out, "@ARGV", "@__sh_args");
    inject_toplevel_binding(&out)
}

/// The file-scope binding, spliced in after the declarations: `@__sh_args`
/// from `@ARGV`, and the count `$__sh_args_n` that `$#` reads at file scope.
///
/// The count is named `$__sh_args_n`, NOT `$#__sh_args_n`. `$#` is Perl's
/// length operator, not a declarable sigil, so `our $#foo;` is a hard
/// compile error ("syntax error … near \"our $#foo\"") — and a bare
/// assignment `our $#foo = …` is equally invalid. Emitting it put that
/// error in the preamble of every program with a top-level `$1`/`$@`, so
/// the whole file failed to compile and produced ZERO output instead of a
/// mismatch.
///
/// The assignment is `local`, not `our`: the top-level DECLARATIONS
/// (`TOPLEVEL_POS_DECLS`) already introduce both names next to the other
/// `our` preamble declarations, so `our @__sh_args = @ARGV;` re-declared
/// them and Perl warned "our variable … redeclared" on every run. `local`
/// is also what `inject_param_binding` uses for a sub's binding, so the two
/// levels now agree.
const TOPLEVEL_POS_BINDING: &str = "local @__sh_args = @ARGV;\nlocal $__sh_args_n = scalar(@ARGV);\n";

/// The file-scope DECLARATIONS, spliced in next to the preamble's other
/// `our` declarations so `use strict` accepts the reads.
const TOPLEVEL_POS_DECLS: &str = "our @__sh_args;\nour $__sh_args_n;\n";

/// Insert the file-scope positional binding so it runs BEFORE any statement
/// that reads `$__sh_args`, and declare the two names under `use strict`.
fn inject_toplevel_binding(src: &str) -> String {
    // 1. Declare both names on the renderer's own preamble line, exactly as
    //    `declare_param_array` does for `@__sh_args`.
    let declared = match src.find("use strict;") {
        Some(i) => {
            let at = i + "use strict;".len();
            let mut s = String::with_capacity(src.len() + 48);
            s.push_str(&src[..at]);
            s.push('\n');
            s.push_str(TOPLEVEL_POS_DECLS);
            s.push_str(&src[at..]);
            s
        }
        None => format!("{TOPLEVEL_POS_DECLS}{src}"),
    };

    // 2. Assign them immediately after the preamble — before the first
    //    statement. `inject_param_binding` splices a sub's binding in the
    //    same place, so the two agree on ordering.
    let mut out = String::with_capacity(declared.len() + 64);
    let mut lines = declared.lines();
    let mut placed = false;
    for line in lines.by_ref() {
        out.push_str(line);
        out.push('\n');
        // The preamble is `use strict; / use warnings; / my $main_exit_code…`.
        // Splicing right after `use warnings;` puts the binding ahead of
        // every statement without having to find the first one.
        if !placed && line.trim_start().starts_with("use warnings;") {
            out.push_str(TOPLEVEL_POS_BINDING);
            placed = true;
        }
    }
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
    // `lines()` dropped the final newline; restore it, and preserve a source
    // that did not end with one.
    if !src.ends_with('\n') {
        out.pop();
    }
    out
}

/// Replace every occurrence of `needle` that is not inside a single- or
/// double-quoted string or a comment, so a rewrite keyed on Perl text never
/// rewrites the same characters when they are DATA (a filename, a heredoc
/// body, an `echo`'d literal).
fn replace_all_outside_strings(src: &str, needle: &str, repl: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len() + 64);
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'\'' => {
                let start = i;
                i += 1;
                while i < b.len() {
                    if b[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if b[i] == b'\'' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                out.push_str(&src[start..i.min(src.len())]);
            }
            b'"' => {
                let start = i;
                i += 1;
                while i < b.len() {
                    if b[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if b[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                out.push_str(&src[start..i.min(src.len())]);
            }
            b'#' => {
                // A `#` runs to end of line is a comment — but only at the
                // start of a line or after whitespace, so `$x#1` and
                // `a#b` keep their literal meaning.
                let at_line_start = i == 0 || b[i - 1] == b'\n' || b[i - 1] == b' ' || b[i - 1] == b'\t';
                if at_line_start {
                    let start = i;
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                    out.push_str(&src[start..i]);
                } else {
                    out.push('#');
                    i += 1;
                }
            }
            _ if src[i..].starts_with(needle) => {
                out.push_str(repl);
                i += needle.len();
            }
            _ => {
                let ch_len = src[i..].chars().next().map(char::len_utf8).unwrap_or(1);
                out.push_str(&src[i..i + ch_len]);
                i += ch_len;
            }
        }
    }
    out
}

// ── Fix 18 — a shell BUILTIN name must not be quoted in a `bash -c` payload ──
//
// `sh_cmd_value_expr` (ir.rs) reconstructs a command for the `bash -c` seam by
// walking the command TEXT and quoting each literal span:
//
//     q{eval} "echo \"The answer is } . __sh_q($x) . q{\""}
//
// That is right for every literal EXCEPT the command name. `q{eval}` is an
// argv string, so `bash -c` runs a program literally NAMED `eval`, fails with
// "command not found", and prints nothing — the payload never runs at all:
//
//     $ x=42; eval "echo \"The answer is $x\""        # bash: The answer is 42
//     # generated: empty output, exit 0
//
// It is silent: the status is 0 and only stdout differs, so it reads as a
// content bug rather than a dispatch one. Fix 7 (see below) is meant to
// RE-ENTER an `eval` payload inline instead, but it only fires when the
// payload's word is fully literal (`word_source_text`) — an interpolated
// payload like the one above skips it, and this is the fallback that then
// runs.
//
// The shell builtins are exactly the names `is_shell_builtin` (ir.rs) already
// refuses to send down the uu-ffi argv seam for this same reason: they are
// resolved by the SHELL, not exec'd. Unquoting the leading name lets bash's own
// builtin table resolve it. Only the FIRST span is touched — a builtin name
// later in the text is an argument to it, and must stay quoted.
fn fix_shell_builtin_command_name(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.split_inclusive('\n') {
        out.push_str(&unquote_builtin_command_name(line));
    }
    out
}

/// Un-quote the command NAME at the head of a `system('bash', '-c', …)` call,
/// when that name is a shell builtin.
///
/// The renderer builds the payload by concatenating quoted spans, and
/// `safe_perl_q_string` picks the quoting per span, so the name arrives in one
/// of three shapes — all of which must be recognised:
///
///     'eval' "echo …"        # plain '…' literal (the common case)
///     q{'eval' "echo …"}     # a q{…} wrapper around that literal
///     eval "echo …"          # already bare (nothing to do)
///
/// Only the FIRST span is touched: a builtin name later in the text is an
/// argument to it and must stay quoted.
fn unquote_builtin_command_name(line: &str) -> String {
    // Cheap reject: the shape needs this exact seam, and most lines lack it.
    const SEAM: &str = "system('bash', '-c', ";
    let Some(call) = line.find(SEAM) else {
        return line.to_string();
    };
    let body_start = call + SEAM.len();
    let rest = &line[body_start..];
    let Some((name, consumed)) = leading_quoted_span(rest) else {
        return line.to_string();
    };
    if !is_shell_builtin_name(&name) || !is_plain_command_name(&name) {
        return line.to_string();
    }
    let mut fixed = String::with_capacity(line.len());
    fixed.push_str(&line[..body_start]);
    fixed.push_str(&name);
    fixed.push_str(&rest[consumed..]);
    fixed
}

/// The first quoted span of `rest`, returning its DECODED text and the number
/// of bytes it occupied. `None` when `rest` does not start with a quoted span.
///
/// Decodes the three shapes `safe_perl_q_string` produces. Anything with an
/// interior escape beyond the `\\` / `\'` pair it emits is refused: those do not
/// occur in a bare command name, and mis-decoding them would corrupt the text.
fn leading_quoted_span(rest: &str) -> Option<(String, usize)> {
    // Shape A: a q{…} (or q(…), q[…], …) wrapper. The CONTENTS are themselves
    // a quoted literal, so peel the wrapper, then recurse on what is inside.
    if rest.starts_with('q') && rest.as_bytes().get(1).is_some_and(|b| !b.is_ascii_alphanumeric())
    {
        let close = match rest.as_bytes()[1] {
            b'{' => b'}',
            b'(' => b')',
            b'[' => b']',
            b'<' => b'>',
            _ => return None,
        };
        let inner_end = rest[2..].find(close as char)? + 2;
        let inner = &rest[2..inner_end];
        // The wrapper must close the span: `q{'eval' "echo"}` is one span,
        // `q{'eval'} . "x"` is not. Only `'`/`"` may appear inside, so a
        // balanced check is enough here.
        let (name, used) = leading_quoted_span(inner)?;
        if used != inner.len() {
            return None;
        }
        return Some((name, inner_end + 1));
    }

    // Shape B: a plain '…' or "…" literal.
    let quote = *rest.as_bytes().first()?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let end = rest[1..].find(quote as char)? + 1;
    let body = &rest[1..end];
    // Only the escapes safe_perl_q_string emits are acceptable here.
    let unescaped = body.replace("\\\\", "").replace("\\'", "");
    if unescaped.contains('\\') {
        return None;
    }
    Some((body.replace("\\\\", "\\").replace("\\'", "'"), end + 1))
}

/// A command name made only of characters a bare command word can contain.
fn is_plain_command_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Shell builtins whose names must reach `bash -c` unquoted. Mirrors
/// `ir.rs::is_shell_builtin`, which refuses the same names for the uu-ffi seam.
fn is_shell_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "eval" | "exec" | "." | "source" | "break" | "continue" | "return" | "shift"
            | "set" | "unset" | "export" | "readonly" | "declare" | "typeset" | "local"
            | "alias" | "unalias" | "trap" | "shopt" | "wait" | "read" | "cd" | "pwd"
            | "echo" | "printf" | "test" | "[" | ":" | "true" | "false" | "let"
            | "getopts" | "command" | "builtin" | "type" | "umask" | "times" | "trap"
            | "kill" | "wait" | "hash" | "help" | "jobs" | "logout" | "mapfile"
            | "readarray" | "compgen" | "complete" | "dirs" | "pushd" | "popd"
            | "suspend" | "caller" | "disown" | "fc" | "history" | "enable"
    )
}

// ── Fix 21 — an `eval` payload's expansion must not be SHELL-quoted ─────
//
// `sh_cmd_value_expr` (ir.rs) rebuilds a `bash -c` payload by walking the
// command TEXT and splicing each `$expansion` as `__sh_q(<perl value>)` —
// a single-quoted SHELL word:
//
//     $ x=42; eval "echo \"The answer is $x\""
//     pl: system('bash','-c', q{'eval' "echo \"The answer is } . __sh_q($x) . '\\""');
//         ⇒ the payload text bash receives is
//              eval "echo \"The answer is '42'\""
//         ⇒ bash expands $x (already done by the splice) and the literal
//            single quotes survive into the OUTPUT:  The answer is '42'
//
// bash prints `The answer is 42`. The single quotes are the bug: the splice
// point is INSIDE a double-quoted span, and a shell-quoted word dropped into a
// double-quoted span contributes its QUOTING CHARACTERS to the text rather
// than its value.
//
// This is safe to fix narrowly precisely because of the double quotes. The
// emitted payload is a concatenation of three kinds of piece, and only the
// middle ones are expansions:
//
//     "…\"The answer is "   literal span (double-quoted shell text)
//          __sh_q($x)      expansion  ← lives INSIDE the double quotes
//     "…\""                literal span
//
// so an expansion is "inside double quotes" exactly when the literal span
// immediately before it has an UNCLOSED `"`. In that position the value is
// interpolated text and needs no shell quoting; `__sh_q` there both adds
// quotes and fails to protect a value containing `"` or `$`.
//
// Outside a double-quoted span (`eval ls $x`) the original `__sh_q` is
// correct and is left untouched — it is what makes the value one argv word.
fn fix_eval_payload_expansion(src: &str) -> String {
    if !src.contains("'eval'") && !src.contains("q{'eval'}") && !src.contains("q{eval}") {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len());
    for line in src.split_inclusive('\n') {
        out.push_str(&unquote_eval_splices(line));
    }
    out
}

/// One line: un-wrap every `__sh_q(` splice that sits inside a double-quoted
/// span of an `eval` payload.
fn unquote_eval_splices(line: &str) -> String {
    const SEAM: &str = "system('bash', '-c', ";
    let Some(call) = line.find(SEAM) else {
        return line.to_string();
    };
    let body_start = call + SEAM.len();
    let rest = &line[body_start..];

    // The payload is a Perl expression: a quoted literal, or a `q{…}`
    // wrapper around one, optionally continued with ` . <more literals>`.
    // The quoting state that matters is tracked over the TEXT those literals
    // spell out, so peel a `q{}` wrapper when present and note where the
    // wrapper's own closing delimiter was.
    let (payload, resume) = match rest.strip_prefix('q') {
        Some(tail) => match q_wrapper_span(tail) {
            Some(inner) => (inner, 1),
            None => return line.to_string(),
        },
        None => (rest, 0),
    };
    let _ = resume;

    // Peel the leading Perl literal (the command-name span) and work on what
    // the NEXT literal spells, which is where the double quotes live.
    let Some(first_lit) = eval_payload_literal_len(payload) else {
        return line.to_string();
    };
    let remainder = &payload[first_lit..];

    // Only an `eval` payload: the command name must be the builtin.
    match perl_literal_value(payload, first_lit) {
        Some(ref name) if name == "eval" => {}
        _ => return line.to_string(),
    }

    let rewritten = rewrite_splices_in_dq(remainder);
    if rewritten == remainder {
        return line.to_string();
    }
    let mut fixed = String::with_capacity(line.len());
    fixed.push_str(&line[..body_start]);
    if rest.starts_with('q') {
        // The whole payload was ONE `q<open>…<close>` span. Re-wrap the
        // rewritten text in the same delimiters. Safe because the rewrite
        // only DELETES the literal characters `__sh_q(` and `)` — it can
        // neither introduce nor unbalance a delimiter.
        let (open, close) = match q_delims(rest) {
            Some(d) => d,
            None => return line.to_string(),
        };
        fixed.push('q');
        fixed.push(open);
        fixed.push_str(&rewritten);
        fixed.push(close);
    } else {
        fixed.push_str(&rewritten);
    }
    // Whatever followed the payload's closing delimiter, verbatim.
    fixed.push_str(&payload[first_lit + remainder.len()..]);
    fixed
}

/// The delimiter pair of a `q<open>…<close>` wrapper: `rest` must be the text
/// starting AT the opening delimiter.
fn q_delims(rest: &str) -> Option<(char, char)> {
    let open = *rest.as_bytes().get(1)? as char;
    let close = match open {
        '{' => '}',
        '(' => ')',
        '[' => ']',
        '<' => '>',
        _ => return None,
    };
    Some((open, close))
}

/// Given the text starting just after a `q`, the wrapper's CONTENTS (through
/// its matching close) and whether a wrapper was actually present.
fn q_wrapper_span(tail: &str) -> Option<&str> {
    let (open, close) = q_delims(&format!("q{tail}"))?;
    let body_start = open.len_utf8();
    let mut depth = 1usize;
    let bytes = tail.as_bytes();
    let mut i = body_start;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            _ => {
                let ch = tail[i..].chars().next()?;
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&tail[body_start..i]);
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// Byte length of the Perl string literal at the head of `s`.
fn eval_payload_literal_len(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    match *bytes.first()? {
        b'\'' | b'"' => {
            let q = bytes[0];
            let mut i = 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == q {
                    return Some(i + 1);
                }
                i += 1;
            }
            None
        }
        b'q' => {
            let (open, close) = q_delims(s)?;
            let body = open.len_utf8();
            let mut depth = 1usize;
            let mut i = body;
            let bytes = s.as_bytes();
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                let ch = s[i..].chars().next()?;
                if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i + close.len_utf8());
                    }
                }
                i += ch.len_utf8();
            }
            None
        }
        _ => None,
    }
}

/// The DECODED text of the Perl literal occupying `s[0..len]`. Only the
/// `\'` / `\"` / `\\` escapes a shell payload can contain are decoded.
fn perl_literal_value(s: &str, len: usize) -> Option<String> {
    let lit = s.get(..len)?;
    let body = if lit.starts_with('q') {
        let open = lit[1..].chars().next()?.len_utf8() + 1;
        lit.get(open..lit.len().checked_sub(open)?)?
    } else {
        lit.get(1..len.checked_sub(1)?)?
    };
    Some(
        body.replace("\\'", "'")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\"),
    )
}

/// Replace `__sh_q(EXPR)` with `EXPR` where EXPR lies inside an unclosed
/// double-quoted span; leave every other occurrence alone.
fn rewrite_splices_in_dq(rest: &str) -> String {
    let mut out = String::with_capacity(rest.len());
    let mut in_dq = false;
    let mut i = 0usize;
    let bytes = rest.as_bytes();
    while i < bytes.len() {
        if rest[i..].starts_with("__sh_q(") {
            // Balanced-paren scan for the splice's expression.
            let open = i + "__sh_q(".len();
            let mut depth = 1usize;
            let mut j = open;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'\\' => j += 1,
                    _ => {}
                }
                j += 1;
            }
            if depth != 0 {
                // Unbalanced (truncated line): copy the rest verbatim.
                out.push_str(&rest[i..]);
                return out;
            }
            if in_dq {
                // Inside a double-quoted span: the value is interpolated
                // text, so it must be spliced RAW.
                out.push_str(&rest[open..j]);
            } else {
                out.push_str(&rest[i..j + 1]);
            }
            i = j + 1;
            continue;
        }
        // Track quoting state over the LITERAL parts only. `__sh_q(` is
        // consumed whole above, so a quote inside a splice never flips state.
        if bytes[i] == b'\\' {
            out.push('\\');
            if let Some(ch) = rest[i + 1..].chars().next() {
                out.push(ch);
            }
            i += 1 + rest[i + 1..].chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let ch = rest[i..].chars().next().unwrap();
        if ch == '"' {
            in_dq = !in_dq;
        }
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

// ── Fix 19 — a bash-only `printf` conversion must not reach Perl's `printf` ──
//
// `printf` here is BASH'S builtin, so its conversion set is bash's, not Perl's.
// The shared renderer emits the format word verbatim into `printf(FMT, …)`, and
// two of bash's conversions mean something else entirely to Perl:
//
//     %q   bash: quote the argument so it can be read back by the shell
//          Perl: NOT a conversion → `Invalid conversion in printf: "%q"`,
//                 the whole call dies and prints NOTHING
//
//     $'a\nb'  a NUL/newline-bearing IFS then `printf '%q' "$IFS"`   # bash:
//              $'\n\t'
//
// It is the same silent-total-loss shape as Fix 18: no output, exit status 0
// (the die is caught by nothing, but the comparison in the gate is on stdout,
// and `printf` dies before `print "…"` runs).
//
// `%q` is rewritten to a call to a `__sh2_q` helper prepended to the program,
// which reproduces bash's shell-quoting rules exactly — measured against bash:
//
//     ''       →  ''            (empty is quoted, never bare)
//     'simple' →  simple        (nothing to escape)
//     'a b'    →  a\ b          (space escaped)
//     "it's"   →  it\'s         (single quote escaped)
//     $'x\ny'  →  $'x\ny'       (any control char → the $'…' form)
//
// Only `%q` is touched: every other conversion (`%s %d %b %c %%` …) already
// agrees between the two, and rewriting them would change passing output.
fn fix_printf_bash_conversions(src: &str) -> String {
    if !src.contains("\"%q\"") && !src.contains("'%q'") {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len() + Q_HELPER.len());
    out.push_str(&Q_HELPER);
    out.push('\n');
    for line in src.split_inclusive('\n') {
        // Only a FORMAT position is rewritten — a `%q` inside an argument is
        // data, not a conversion, and must survive untouched.
        out.push_str(&rewrite_q_in_format(line));
    }
    out
}

/// The `__sh2_q` runtime: bash's `printf %q`.
const Q_HELPER: &str = r#"# bash `printf %q`: quote a value so the shell reads it back identically.
sub __sh2_q {
    my $s = shift;
    $s = '' unless defined $s;
    return "''" if $s eq '';
    # Any character the shell cannot carry bare (control chars incl. newline,
    # tab and DEL) selects the $'…' form; everything else is backslash-escaped.
    if ($s =~ /[\x00-\x1f\x7f]/) {
        my $e = $s;
        $e =~ s/\\/\\\\/g;
        $e =~ s/'/\\'/g;
        $e =~ s/\n/\\n/g;
        $e =~ s/\t/\\t/g;
        $e =~ s/\r/\\r/g;
        $e =~ s/([\x00-\x08\x0b\x0c\x0e-\x1f\x7f])/sprintf('\\x%02x', ord($1))/ge;
        return "\$'".$e."'";
    }
    my $e = $s;
    $e =~ s/([ \t"'\\\$\`!\#\(\)\*\;\:\<\>\?\[\]\|\~])/\\$1/g;
    return $e;
}"#;

/// Rewrite a leading `%q` conversion in a `printf(...)` FORMAT argument to
/// `__sh2_q(<operand>)`. Returns the line unchanged when it is not such a format.
fn rewrite_q_in_format(line: &str) -> String {
    const MARK: &str = "printf(";
    let Some(start) = line.find(MARK) else {
        return line.to_string();
    };
    let args_start = start + MARK.len();
    let rest = &line[args_start..];
    let Some(fmt_len) = leading_perl_literal_len(rest) else {
        return line.to_string();
    };
    let fmt = &rest[..fmt_len];
    if !bash_format_has_q(fmt) {
        return line.to_string();
    }
    // Split the remaining arguments off the closing paren.
    let tail = &rest[fmt_len..];
    let close = match tail.rfind(')') {
        Some(i) => i,
        None => return line.to_string(),
    };
    let operands = split_perl_args(&tail[..close]);
    if operands.is_empty() {
        return line.to_string();
    }
    let Some((new_fmt, q_positions)) = rewrite_q_operands(fmt) else {
        return line.to_string();
    };
    // The rewritten format no longer consumes the `%q` operands: they are
    // woven into the format as concatenation instead, so they are dropped
    // from the printf argument list. Every OTHER operand stays, in order.
    let mut new_fmt = new_fmt;
    for slot in &q_positions {
        let Some(operand) = operands.get(*slot) else {
            return line.to_string();
        };
        new_fmt = new_fmt.replace(&format!("\u{1}{slot}\u{1}"), &format!("__sh2_q({operand})"));
    }
    let kept: Vec<String> = operands
        .iter()
        .enumerate()
        .filter(|(i, _)| !q_positions.contains(i))
        .map(|(_, o)| o.clone())
        .collect();
    let mut fixed = String::with_capacity(line.len());
    fixed.push_str(&line[..args_start]);
    fixed.push_str(&new_fmt);
    if !kept.is_empty() {
        fixed.push_str(", ");
        fixed.push_str(&kept.join(", "));
    }
    fixed.push(')');
    fixed.push_str(&tail[close + 1..]);
    fixed
}

/// Byte length of the Perl literal (`"…"` or `'…'`) at the head of `rest`.
fn leading_perl_literal_len(rest: &str) -> Option<usize> {
    let quote = *rest.as_bytes().first()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut i = 1;
    let b = rest.as_bytes();
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == quote {
            return Some(i + 1);
        }
        i += 1;
    }
    None
}

/// Does this format literal contain a `%q` conversion (not `%%q`)?
fn bash_format_has_q(fmt: &str) -> bool {
    let cs: Vec<char> = fmt.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '%' {
            if cs.get(i + 1) == Some(&'%') {
                i += 2;
                continue;
            }
            if cs.get(i + 1) == Some(&'q') {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Replace the `%q` specifiers in a format literal with `__sh2_q(<operand>)`
/// concatenation, returning the new format AND the indices (into the printf
/// operand list) consumed by those `%q` slots, so the caller can drop them.
fn rewrite_q_operands(fmt: &str) -> Option<(String, Vec<usize>)> {
    let (quote, body) = match fmt.as_bytes().first() {
        Some(&b'"') => ('"', &fmt[1..fmt.len().checked_sub(1)?]),
        Some(&b'\'') => ('\'', &fmt[1..fmt.len().checked_sub(1)?]),
        _ => return None,
    };
    let cs: Vec<char> = body.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut lit = String::new();
    let mut i = 0;
    let mut slot = 0usize;
    let mut taken: Vec<usize> = Vec::new();
    while i < cs.len() {
        if cs[i] == '%' && cs.get(i + 1) == Some(&'%') {
            lit.push_str("%%");
            i += 2;
            continue;
        }
        if cs[i] == '%' && cs.get(i + 1) == Some(&'q') {
            if !lit.is_empty() {
                parts.push(perl_literal(quote, &lit));
                lit.clear();
            }
            // The operand is not known here; the caller substitutes it by slot.
            parts.push(format!("\u{1}{slot}\u{1}"));
            taken.push(slot);
            slot += 1;
            i += 2;
            continue;
        }
        lit.push(cs[i]);
        i += 1;
    }
    if !lit.is_empty() {
        parts.push(perl_literal(quote, &lit));
    }
    let fmt_out = if parts.is_empty() {
        perl_literal(quote, "")
    } else {
        parts.join(" . ")
    };
    Some((fmt_out, taken))
}

/// Split a Perl argument list on top-level commas (bracket/paren/brace and
/// quote aware). Returns `None` on an unbalanced span.
fn split_perl_args(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if let Some(q) = quote {
            cur.push(c);
            if c == '\\' {
                if let Some(n) = it.next() {
                    cur.push(n);
                }
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                cur.push(c);
            }
            '(' | '[' | '{' => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                let t = cur.trim().to_string();
                if !t.is_empty() {
                    out.push(t);
                }
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    let t = cur.trim().to_string();
    if !t.is_empty() {
        out.push(t);
    }
    out
}

/// Re-quote a literal span under `quote`, escaping only what that quote needs.
fn perl_literal(quote: char, text: &str) -> String {
    if quote == '\'' {
        return format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"));
    }
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

// ── Fix 20 — `printf %b` expands escapes in the ARGUMENT, not the format ────
//
// The two `printf`s agree on `%s`, `%d`, `%c` and `%%`, but `%b` is the one
// conversion whose OPERANDS mean different things:
//
//     bash:  %b  the argument is a string whose \n \t \\ … ESCAPES are expanded
//     Perl:  %b  the argument is a NUMBER, rendered in binary ("a\tb\n" → 0)
//
// so `printf '%b' 'a\tb\n'` prints a real tab+newline in bash and the single
// character `0` in the generated program — plus a warning on stderr. The
// shared renderer emits `printf("%b", <arg>)` with the argument untouched,
// because deciding that an argument carries escapes is an operand-level fact.
//
// The operand is wrapped in `__sh2_b`, which performs bash's expansion:
// `\a \b \e \E \f \n \r \t \v \0nnn \xHH \\` and leaves every other backslash
// pair ALONE (bash does not interpret `\d` and neither do we).
fn fix_printf_b_percent_b(src: &str) -> String {
    if !src.contains("\"%b\"") && !src.contains("'%b'") {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len() + B_HELPER.len());
    out.push_str(&B_HELPER);
    out.push('\n');
    for line in src.split_inclusive('\n') {
        out.push_str(&rewrite_b_operands(line));
    }
    out
}

/// The `__sh2_b` runtime: bash's `printf %b` operand expansion.
const B_HELPER: &str = r#"# bash `printf %b`: expand backslash escapes in the operand.
sub __sh2_b {
    my $s = shift;
    $s = '' unless defined $s;
    $s =~ s{\\(?:x([0-9a-fA-F]{1,2})|([0-7]{1,3})|([abefnrtv]))}{
        my ($hex, $oct, $esc) = ($1, $2, $3);
        if (defined $hex)  { chr(hex($hex)) }
        elsif (defined $oct) { chr(oct($oct)) }
        else { $esc eq 'e' ? "\e" : ($esc eq 'a' ? "\a" : $esc eq 'b' ? "\b"
                                  : $esc eq 'f' ? "\f" : $esc eq 'n' ? "\n"
                                  : $esc eq 'r' ? "\r" : $esc eq 't' ? "\t" : "\v") }
    }ge;
    return $s;
}"#;

/// Wrap every operand of a `printf` whose format contains `%b` in `__sh2_b`
/// AND rewrite the conversion itself to `%s`.
///
/// The conversion must change, not just its operand: after `__sh2_b` expands
/// the escapes the operand really is a plain string, but Perl's `%b` would
/// then take that string as a NUMBER and print `0` (with an "isn't numeric"
/// warning). `%s` prints the already-expanded string verbatim — which is
/// what bash's `%b` means.
fn rewrite_b_operands(line: &str) -> String {
    const MARK: &str = "printf(";
    let Some(start) = line.find(MARK) else {
        return line.to_string();
    };
    let args_start = start + MARK.len();
    let rest = &line[args_start..];
    let Some(fmt_len) = leading_perl_literal_len(rest) else {
        return line.to_string();
    };
    let fmt = &rest[..fmt_len];
    let slots = bash_b_operand_slots(fmt);
    if slots.is_empty() {
        return line.to_string();
    }
    let new_fmt = replace_conversion(fmt, 'b', 's');
    let tail = &rest[fmt_len..];
    let Some(close) = tail.rfind(')') else {
        return line.to_string();
    };
    let operands = split_perl_args(&tail[..close]);
    if operands.is_empty() {
        return line.to_string();
    }
    let mut wrapped: Vec<String> = operands.clone();
    for s in &slots {
        if let Some(o) = wrapped.get(*s) {
            // Already wrapped (a line rewritten twice) — leave it alone.
            if !o.starts_with("__sh2_b(") {
                wrapped[*s] = format!("__sh2_b({o})");
            }
        }
    }
    let mut fixed = String::with_capacity(line.len());
    fixed.push_str(&line[..args_start]);
    fixed.push_str(&new_fmt);
    fixed.push_str(", ");
    fixed.push_str(&wrapped.join(", "));
    fixed.push(')');
    fixed.push_str(&tail[close + 1..]);
    fixed
}

/// In a printf format literal, replace every `…<from>` conversion with the
/// same flags/width but a `to` conversion letter. `%%` is never a conversion.
fn replace_conversion(fmt: &str, from: char, to: char) -> String {
    let (quote, body) = match fmt.as_bytes().first() {
        Some(&b'"') => ('"', &fmt[1..fmt.len().saturating_sub(1)]),
        Some(&b'\'') => ('\'', &fmt[1..fmt.len().saturating_sub(1)]),
        _ => return fmt.to_string(),
    };
    let cs: Vec<char> = body.chars().collect();
    let mut out = String::with_capacity(body.len());
    let mut i = 0;
    while i < cs.len() {
        if cs[i] != '%' {
            out.push(cs[i]);
            i += 1;
            continue;
        }
        if cs.get(i + 1) == Some(&'%') {
            out.push_str("%%");
            i += 2;
            continue;
        }
        let mut j = i + 1;
        // Flags/width between `%` and the conversion letter (`-5`, `08.3`…).
        // Collected, not emitted as we go, so a trailing bare `%` (no letter
        // follows) can still be copied verbatim in the else-arm below without
        // duplicating what we consumed.
        let mut flags = String::new();
        while j < cs.len() && !cs[j].is_ascii_alphabetic() {
            flags.push(cs[j]);
            j += 1;
        }
        if j < cs.len() {
            // The LEADING `%` belongs to the conversion. Dropping it (as this
            // loop once did) turns the format `%b` into a bare `s` and `%d`
            // into a bare `d`, so `printf("%s", …)` printed the LITERAL `s`
            // with an "Redundant argument" warning and ate the argument:
            // bash printed `a<TAB>b`, the generated Perl printed `s`
            // (printf-format-and-v.sh).
            out.push('%');
            out.push_str(&flags);
            out.push(if cs[j] == from { to } else { cs[j] });
            i = j + 1;
        } else {
            out.push_str(&cs[i..].iter().collect::<String>());
            break;
        }
    }
    perl_literal(quote, &out)
}

/// Operand indices consumed by `%b` conversions, in format order.
///
/// A conversion may carry flags/width/precision (`%-10b`), so the letter is
/// taken as the first alphabetic character after the `%`, matching the way
/// the renderer itself picks a conversion out (`literal_printf_conversions`).
fn bash_b_operand_slots(fmt: &str) -> Vec<usize> {
    let body = match fmt.as_bytes().first() {
        Some(&b'"') => &fmt[1..fmt.len().saturating_sub(1)],
        Some(&b'\'') => &fmt[1..fmt.len().saturating_sub(1)],
        _ => return Vec::new(),
    };
    let cs: Vec<char> = body.chars().collect();
    let mut slots = Vec::new();
    let mut conv = 0usize;
    let mut i = 0;
    while i < cs.len() {
        if cs[i] != '%' {
            i += 1;
            continue;
        }
        if cs.get(i + 1) == Some(&'%') {
            i += 2;
            continue;
        }
        // skip flags/width/precision, then read the conversion letter
        let mut j = i + 1;
        while j < cs.len() && !cs[j].is_ascii_alphabetic() {
            j += 1;
        }
        if j < cs.len() {
            if cs[j] == 'b' {
                slots.push(conv);
            }
            // `%c`/`%s` etc. consume an argument; `%%`-style aside, a
            // conversion is one argument unless it is a star-width (rare).
            conv += 1;
            i = j + 1;
        } else {
            break;
        }
    }
    slots
}

// ── Fix 14 — an unbuildable condition must be TRUE, not a bare `0` ──────
//
// The shared renderer lowers a condition-position `redirect` / `block` /
// `exec` to `(system('bash', '-c', q{…}) == 0)`. The `== 0` is the whole
// point: it converts a bash STATUS into a Perl BOOLEAN, which is what the
// condition slot needs. When the inner command cannot be rebuilt the arm
// now emits `ir::UNBUILDABLE_COND` — a `0` carrying a distinguishing
// comment — which this pass rewrites to `1`.
//
//   sh:  while ! { : >> /tmp/f; } 2>/dev/null; do echo waiting; done
//   pl:  while (!(0/*sh2:unbuildable-cond*/)) …   ->  while (!(1)) …
//
// bash's `!` on a zero status is false, so bash runs zero iterations; a bare
// falsy `0` would make Perl run forever and the gate would kill it at the
// 20 s timeout.
//
// Keyed on the MARKER, never on the shape: a legitimately computed
// `Bool(false)` also renders as a bare `0`, and rewriting that inverted real
// conditions (redirect-all: `if ! command -v CMD` renders `!(0)` = TRUE).
/// Rewrite the renderer's UNBUILDABLE-condition placeholder to `1`.
///
/// The renderer marks a condition-position command it cannot rebuild with
/// `0/*sh2:unbuildable-cond*/` (see `ir::UNBUILDABLE_COND`). A bare `0` is a
/// STATUS, and Perl reads it as FALSE — so a `while` never runs and an `if`
/// silently drops its branch. `"this holds"` is the only answer that keeps
/// the control flow a loop rather than a hang.
///
/// The rewrite is keyed on that exact marker, NOT on a bare `(0)`. An earlier
/// version matched the shape instead, which also caught a legitimately
/// computed `Bool(false)`: `if ! command -v CMD > /dev/null` renders
/// `!(0)` — TRUE, which is what bash means — and the blanket
/// `(!(0))`→`(!(1))` rewrite inverted it and dropped the branch
/// (redirect-all lost its `not found` line).
fn fix_condition_booleans(src: &str) -> String {
    src.replace(crate::ir::UNBUILDABLE_COND, "1")
}

// ── Fix 12 — a bareword argument to `print` must be a quoted string ────────
//
// `echo ab` renders as `print(ab);`. Under the `use strict` the preamble
// always emits, an unquoted `ab` is a *bareword* — Perl reads it as the
// string "ab" but warns, and warns to stderr, and "ab" is one token away
// from a future reserved word:
//
// ```perl
// Unquoted string "ab" may clash with future reserved word at out.pl line 362.
// ```
//
// The value is already correct; only the quoting is missing. `render_word`
// in the shared renderer returns a bareword for a single unquoted literal
// word, and the renderer routes that straight into `print(…)` (ir.rs's
// echo/print path). Anchoring on the renderer's own call shape — a `print`
// whose argument list is a bare identifier that is NOT a declared Perl
// symbol — keeps this from touching `$x`, `foo(…)`, `'q'`, or a `print` whose
// argument is already an expression.
fn fix_bareword_words(src: &str) -> String {
    let declared = declared_symbols(src);
    let mut out = String::with_capacity(src.len());
    for line in src.split_inclusive('\n') {
        out.push_str(&quote_bareword_in_line(line, &declared));
    }
    out
}

/// Rewrite `print(AB);` / `print(AB, "\n");` where `AB` is a bareword into
/// the quoted form. Returns `line` unchanged unless it matches.
fn quote_bareword_in_line(line: &str, declared: &std::collections::HashSet<String>) -> String {
    // Cheap reject: the defect needs a `print(` and no `$`/`'`/`"` anywhere
    // in the call, so most lines never reach the scan.
    if !line.contains("print(") || line.contains('"') {
        return line.to_string();
    }
    let mut fixed = line.to_string();
    for (open, close) in [("print(", ");"), ("print (", ");")] {
        let mut from = 0usize;
        while let Some(rel) = fixed[from..].find(open) {
            let start = from + rel + open.len();
            let Some(end_rel) = fixed[start..].find(close) else {
                break;
            };
            let end = start + end_rel;
            let inner = &fixed[start..end];
            from = end + close.len();
            // Only a bare identifier is a bareword. `$x` is a variable, and
            // `f(...)`/`q{...}`/`$x + 1` are already expressions.
            if !is_bareword_ident(inner, declared) {
                continue;
            }
            let q = format!("'{inner}'");
            fixed.replace_range(start..end, &q);
            from = start + q.len();
        }
    }
    fixed
}

/// True when `s` is exactly a Perl identifier that is a bareword: no sigil,
/// no call, no quotes, and not a name the program declares (a declared
/// sub or a `my` variable used without a sigil is not our business).
fn is_bareword_ident(s: &str, declared: &std::collections::HashSet<String>) -> bool {
    if s.is_empty() || declared.contains(s) {
        return false;
    }
    let mut cs = s.chars();
    let first = cs.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return false;
    }
    true
}

// ── Fix 13 — declare every variable the program USES but never declares ────
//
// The preamble always emits `use strict`, under which reading an undeclared
// `$name` is a COMPILE error, not a warning:
//
// ```perl
// Global symbol "$__cap" requires explicit package name
//     (did you forget to declare "my $__cap"?) at out.pl line 7.
// ```
//
// The shared renderer hoists a `my` for variables it can see assigned, but
// an environment-assignment PREFIX (`VAR=value cmd`) renders the name as a
// Perl *read* with nothing to bind it:
//
// ```perl
// $ENV{__cap} = $__cap;   # line 4   ← read of an undeclared $__cap
// ```
//
// and the same gap shows up for a command-substitution target and for a
// `local`-less loop var. Rather than teach the shared renderer about each
// construct, scan the finished text for `$name` / `@name` / `%name` reads
// and emit one `my` per symbol that is used but never declared. That is
// exactly what the strict pragma asks for, and it is a no-op for a program
// the renderer already got right.
fn declare_undeclared_vars(src: &str) -> String {
    if !src.contains("use strict") {
        return src.to_string();
    }
    let declared = declared_symbols(src);
    // `use English qw($INPUT_RECORD_SEPARATOR …)` predeclares those aliases.
    // A `my` for one of them is not merely redundant, it is ILLEGAL — the
    // later `local $INPUT_RECORD_SEPARATOR` would be "Can't localize lexical
    // variable" — so a name any `use` imports is off limits.
    let imported = imported_names(src);
    let mut wanted: Vec<(char, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (sigil, name) in referenced_symbols(src) {
        if !is_declared_name(&name) || seen.contains(&(sigil, name.clone())) {
            continue;
        }
        // A name the program already binds — by `my`/`our`/`local`, or as a
        // sub — is not ours to declare.
        if declared.contains(&name) {
            continue;
        }
        if imported.contains(&name) {
            continue;
        }
        // `$_`, `$0`..`$9`, `$@`, and the renderer's own `$main_exit_code`
        // / `$CHILD_ERROR` / `$__sh_*` machinery are either special or
        // already bound by the preamble.
        if is_special_perl_name(&name, &declared) {
            continue;
        }
        seen.insert((sigil, name.clone()));
        wanted.push((sigil, name));
    }
    if wanted.is_empty() {
        return src.to_string();
    }
    // Sort so the output is deterministic: a re-render of the same program
    // must produce the same bytes, or every gate run shows a spurious diff.
    wanted.sort();
    let mut decl = String::new();
    for (sigil, name) in &wanted {
        decl.push_str(&format!("my {sigil}{name};\n"));
    }
    inject_before_first_stmt(src, &decl)
}

/// Every identifier a `use Module qw(a b c)` import list predeclares.
///
/// `use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR
/// $OS_ERROR $PROGRAM_NAME)` is in the renderer's own preamble, and those
/// aliases must never receive a `my`: `local $INPUT_RECORD_SEPARATOR = …`
/// is legal against the English alias and fatal against a fresh lexical.
fn imported_names(src: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for line in src.lines() {
        let t = line.trim_start();
        if !(t.starts_with("use ") || t.starts_with("no ")) {
            continue;
        }
        // Collect every bareword in the import list, with any sigil stripped.
        for tok in t.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$')) {
            let name = tok.trim_start_matches('$');
            if name.is_empty() {
                continue;
            }
            let mut cs = name.chars();
            let first = cs.next().unwrap();
            if (first.is_ascii_alphabetic() || first == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                out.insert(name.to_string());
            }
        }
    }
    out
}

/// True for names Perl reserves or the renderer pre-binds, which must never
/// get a fresh `my`.
fn is_special_perl_name(name: &str, declared: &std::collections::HashSet<String>) -> bool {
    if declared.contains(name) {
        return true;
    }
    if name.chars().all(|c| c.is_ascii_digit()) {
        return true; // $1 .. $9
    }
    matches!(
        name,
        // `a`/`b` are Perl's `sort` package globals — the renderer lowers a
        // pipeline's sort stage to `sort { $a cmp $b } @__fs`, and a `my $a`
        // of ours would shadow them for the whole file and make every sort
        // compare two undefs. See the sort_globals_* regression tests.
        "a" | "b"
            | "_"
            | "main_exit_code"
            | "CHILD_ERROR"
            | "sh_args"
            | "fn_status"
            | "OK"
            | "ARGV"
            | "ENV"
            | "INC"
            | "stdout"
            | "stderr"
            | "stdin"
    )
}

/// Names a shell program can actually use: skip render-internal `__sh_`/
/// `__`-prefixed temporaries (the renderer hoists those itself) and Perl's
/// own special punctuation.
fn is_declared_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.starts_with("__") {
        return false;
    }
    let mut cs = name.chars();
    let first = cs.next().unwrap();
    (first.is_ascii_alphabetic() || first == '_') && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Every `$name` / `@name` symbol mentioned in the text, with its sigil.
///
/// `#`/`%` is deliberately NOT scanned. A hash sigil and a printf format
/// spec are the same two characters in the same place — `printf("%d %o", …)`
/// reads as `%d` and `%o` — and modulo, and a regex, and a substitution all
/// look the same again. A false `%d` declaration was not merely wasteful: it
/// made `cmp -l` fail to compile and hang. `$` and `@` have no such
/// ambiguity, so they are safe to scan.
///
/// The remaining over-collection (a `$name` inside a single-quoted string is
/// text, not a variable) costs at most a harmless extra `my`.
fn referenced_symbols(src: &str) -> Vec<(char, String)> {
    let mut out = Vec::new();
    let b: Vec<char> = src.chars().collect();
    for i in 0..b.len() {
        let c = b[i];
        if c != '$' && c != '@' {
            continue;
        }
        // A sigil directly followed by a sigil is a deref/array-slice, not a
        // new name (`$$ref`, `@$ref`).
        if i + 1 < b.len() && (b[i + 1] == '$' || b[i + 1] == '@') {
            continue;
        }
        let mut j = i + 1;
        let mut name = String::new();
        while j < b.len() {
            let cj = b[j];
            if cj.is_ascii_alphanumeric() || cj == '_' {
                name.push(cj);
                j += 1;
            } else {
                break;
            }
        }
        if !name.is_empty() {
            // A subscripted or braced reference is not a scalar: Perl reads
            // `$PIPESTATUS[0]` as `@PIPESTATUS` and `$h{key}` as `%h`, and a
            // `my $PIPESTATUS` would leave the read as a compile error under
            // `use strict` ("Global symbol @PIPESTATUS requires explicit
            // package name"). Promote the sigil to what the usage demands.
            let sigil = match b.get(j) {
                Some('[') => '@',
                Some('{') => '%',
                _ => c,
            };
            out.push((sigil, name));
        }
    }
    out
}

/// Every symbol the program already binds: the names introduced by any `my`
/// / `our` / `local` / `state` declaration, and every `sub NAME`.
///
/// A declaration is NOT always at the start of a line — the renderer emits
/// `open(my $fh, …)`, `for my $i (…)`, and `local $/` in mid-line position,
/// and a body statement can carry several (`my ($a, $b) = split(…)`). Reading
/// only the first name after a line-leading keyword missed all three, and a
/// missed declaration becomes a spurious `my` from Fix 13 — which for a
/// `local` of that same name is a compile error, not just noise.
fn declared_symbols(src: &str) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    for line in src.lines() {
        for kw in ["my ", "our ", "local ", "state "] {
            let mut from = 0usize;
            while let Some(rel) = line[from..].find(kw) {
                let at = from + rel;
                from = at + kw.len();
                // The keyword must be a standalone word, not the tail of an
                // identifier (`my` in `$my_var`).
                let before_ok = at == 0
                    || !line[..at]
                        .chars()
                        .next_back()
                        .map(|c| c.is_ascii_alphanumeric() || c == '_')
                        .unwrap_or(false);
                if !before_ok {
                    continue;
                }
                // The declaration binds the sigil-bearing names in its BINDING
                // LIST only — everything from the initialiser on is an
                // expression that may mention variables this does not bind
                // (`my $n = length($x)` binds `$n`, not `$x`). Stop at the
                // first `=` or `;`.
                let rest = &line[from..];
                let binding_end = rest
                    .char_indices()
                    .find(|(i, c)| *c == '=' || *c == ';')
                    .map(|(i, _)| i)
                    .unwrap_or(rest.len());
                for name in declared_names_in(&rest[..binding_end]) {
                    out.insert(name);
                }
                // Keep sweeping: a single rendered line routinely carries
                // several declarations (`do { my $a = …; my $b = … }`).
            }
        }
        if let Some(rest) = line.trim_start().strip_prefix("sub ") {
            let name: String = rest
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                out.insert(name);
            }
        }
    }
    out
}

/// Fix 13 — a `my` must be emitted at most ONCE per symbol.
///
/// `declared_symbols` used to answer "is this name bound?" by calling
/// `referenced_symbols`, which deliberately does not scan `%`: a hash sigil
/// and a printf conversion are the same two characters in the same place, so
/// `%d`/`%o` would be read as hash reads and `%` was left out. The consequence
/// was that `my %words;` — a perfectly good declaration — was INVISIBLE to
/// the pass, and a later `%words`-bearing read (`$words{$i}` promotes to `%`
/// in `referenced_symbols`) added a SECOND `my %words;`:
///
/// ```perl
/// my %words;      # the renderer's declaration
/// my %words;      # Fix 13, because the first one did not count
/// if (($words{$i}) =~ /…/) { }   # …which now reads an undeclared, shadowed %words
/// ```
///
/// Two `my` of one name in the same scope is not a warning, it is a binding:
/// the second shadows the first from its own line onward, so everything the
/// renderer stored in the first `%words` is invisible to every later read —
/// `case "${words[$counter]}"` matched nothing. And it is a warning on top of
/// that ("masks earlier declaration in same scope"), which several examples
/// emit on stderr.
///
/// The fix is to collect the DECLARED names over all three sigils. Two
/// details keep it honest:
///
///  * a `%` byte is accepted only when it immediately follows `@name` or
///    `$name` (the slice forms `$h{%$k}`, `$h{%$k}`), so a printf
///    conversion or a bare `%` modulo can never be read as a name;
///  * the first token of a binding list is skipped, because
///    `my $h = {%$x}` binds `$h` — not `%x` — and binding `%x` here would
///    mask the real `%x` Fix 13 is about to emit.
fn declared_names_in(binding_list: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b: Vec<char> = binding_list.chars().collect();
    let mut i = 0;
    // True at a position where a sigil could START a binding-list element —
    // i.e. not in the middle of a name we are already scanning. Only here may
    // a bare `%` be a sigil; anywhere else it is printf/modulo syntax.
    let mut at_element = true;
    while i < b.len() {
        let c = b[i];
        if c != '$' && c != '@' && !(c == '%' && at_element) {
            if c.is_ascii_alphanumeric() || c == '_' {
                // Still inside a name: a trailing sigil (`@rest`, `%rest`) is
                // the slice form of the name just read, not a new binding.
                at_element = false;
            } else {
                at_element = true;
            }
            i += 1;
            continue;
        }
        if i + 1 < b.len() && (b[i + 1] == '$' || b[i + 1] == '@' || b[i + 1] == '%') {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut j = start;
        let mut name = String::new();
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == '_') {
            name.push(b[j]);
            j += 1;
        }
        if name.is_empty() {
            i = j.max(i + 1);
            at_element = true;
            continue;
        }
        i = j;
        // `@rest` / `%rest`: the slice and hash forms of one and the same
        // array — the binding list gave `$rest`, not a second `%rest`.
        let trailing_sigil = j < b.len() && (b[j] == '@' || b[j] == '%');
        if trailing_sigil {
            i = j + 1;
            at_element = false;
            continue;
        }
        at_element = false;
        out.push(name);
    }
    out
}

/// Fix 11 — a `for … in "${arr[@]}"` iterand must stay a LIST, not a scalar.
///
/// The renderer wraps a `for` iterand in a redundant pair of parens, exactly
/// as it does for a capture (Fix 10), but for an ARRAY expansion the inner
/// expression is the `join`/`split` round-trip rather than a bare `split`:
///
/// ```perl
/// for my $__num ((join(' ', @numbers))) {  # from `for num in "${numbers[@]}"`
/// ```
///
/// Both problems are Fix 10's, at once. The inner parens put `join` in
/// **scalar context**, so the loop runs exactly once with the whole array as
/// one word: `for num in "${numbers[@]}"` over `(1 2 3 4 5)` printed nothing
/// where the nested filter should have printed 8 lines (058), and `for arg in
/// "$@"` (realpath-cmdsub) collapsed every argument into a single blob. The
/// `join` is separately wrong here — `join` is what *collapses* the list, and
/// the list is what the loop wants.
///
/// The fix is the same unwrap Fix 10 performs, and for the same reason: the
/// word-splitting `for` wants is precisely what the array already holds, so
/// the iterand is the array itself. `for my $__num (@numbers)` iterates one
/// element at a time, as bash's `for … in "${arr[@]}"` does.
///
/// Scoped to the array shape only. An iterand that is a command capture is
/// Fix 10's business (there the list must come from `split`), and anything
/// else — a literal list, a range, a glob — is left exactly as rendered.
fn fix_for_array_iterand(src: &str) -> String {
    const PREFIX: &str = "for my $";
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    loop {
        let Some(at) = rest.find(PREFIX) else {
            out.push_str(rest);
            return out;
        };
        // The iterand paren pair: `for my $NAME ((`.
        let after_prefix = &rest[at + PREFIX.len()..];
        let Some(name_end) = after_prefix.find(' ') else {
            out.push_str(&rest[..at + PREFIX.len()]);
            rest = after_prefix;
            continue;
        };
        let open = at + PREFIX.len() + name_end + 1;
        if !rest[open..].starts_with("((") {
            out.push_str(&rest[..open]);
            rest = &rest[open..];
            continue;
        }
        // Inner `(` — the one that must go.
        let inner_open = open + 1;
        let Some(inner_close) = matching_paren(rest, inner_open) else {
            out.push_str(&rest[..open]);
            rest = &rest[open..];
            continue;
        };
        // The wrap must be `join(' ', @NAME)` — the array shape. A capture is
        // Fix 10's; anything else (a literal list, a range) is not ours.
        let inner = rest[inner_open + 1..inner_close].trim();
        let Some(arr) = inner
            .strip_prefix("join(' ', @")
            .and_then(|t| t.strip_suffix(')'))
        else {
            out.push_str(&rest[..open]);
            rest = &rest[open..];
            continue;
        };
        // A bare `@name` is the iterand; `@name[ … ]` and `@{ … }` are not
        // shapes this fix claims to understand.
        if arr.is_empty() || !arr.chars().all(|c| c.is_alphanumeric() || c == '_') {
            out.push_str(&rest[..open]);
            rest = &rest[open..];
            continue;
        }
        // `for my $NAME ((join(' ', @arr))) {` -> `for my $NAME (@arr) {`
        // `inner_close` is the `)` that closes the `join`; the iterand's own
        // `)` follows it, so resumption is one past THAT, not past this one.
        // A shape that ends before that second `)` is not ours — leave it.
        let Some(resume) = rest.get(inner_close + 2..) else {
            out.push_str(&rest[..open]);
            rest = &rest[open..];
            continue;
        };
        out.push_str(&rest[..inner_open]);
        out.push('@');
        out.push_str(arr);
        out.push(')');
        rest = resume;
    }
}

/// Fix 10 — a `for … in $(cmd)` capture must stay a LIST, not a scalar.
///
/// The renderer emits the command substitution as
/// `join(' ', split(/\s+/, …))` and wraps a `for` iterand in a further pair
/// of parens:
///
/// ```perl
/// for my $f ((join(' ', split(/\s+/, __sh_pipe_capture(…))))) {
/// ```
///
/// Those inner parens put `join` in **scalar context**, so the loop runs
/// exactly once with the whole line as one word:
/// `for f in $(echo "a b c")` printed `[a b c]` instead of `[a] [b] [c]`
/// (examples/sqs-overlap-multiline-awk.sh, 070_gnuisms_thorough.sh line 65).
///
/// The word-splitting `for` wants is precisely what `split` already produces —
/// `join` is what collapses it. So the fix is to drop the redundant parens AND
/// the `join` wrapper, leaving `for my $f (split(/\s+/, …))`, which iterates
/// per field exactly as bash's `for … in $(cmd)` does (IFS field splitting).
///
/// Scoped to the `for`-iterand shape only: a capture used in a scalar context
/// (assignment, arithmetic, test) must keep the `join` that
/// `fix_cmd_capture_split` puts in scalar context.
fn fix_for_capture_paren(src: &str) -> String {
    const PREFIX: &str = "for my $";
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    loop {
        let Some(at) = rest.find(PREFIX) else {
            out.push_str(rest);
            return out;
        };
        // The iterand paren pair: `for my $NAME ((`.
        let after_prefix = &rest[at + PREFIX.len()..];
        let Some(name_end) = after_prefix.find(' ') else {
            out.push_str(&rest[..at + PREFIX.len()]);
            rest = after_prefix;
            continue;
        };
        let open = at + PREFIX.len() + name_end + 1;
        if !rest[open..].starts_with("((") {
            out.push_str(&rest[..open]);
            rest = &rest[open..];
            continue;
        }
        // Inner `(` — the one that must go.
        let inner_open = open + 1;
        let Some(inner_close) = matching_paren(rest, inner_open) else {
            out.push_str(&rest[..open]);
            rest = &rest[open..];
            continue;
        };
        // The wrap must be `join(' ', split(/\s+/, …))` — the capture shape.
        // Anything else (a plain `@list`, an array ref) is not ours.
        let inner = rest[inner_open + 1..inner_close].trim();
        if !inner.starts_with("join(' ', split(/\\s+/, ") {
            out.push_str(&rest[..open]);
            rest = &rest[open..];
            continue;
        }
        // Unwrap the `split`: `join(' ', split(/\s+/, X))` -> `split(/\s+/, X)`.
        let split_at = inner.find("split(/\\s+/, ").expect("prefix-checked") + "split(/\\s+/, ".len();
        // The `split` call's own close paren ends the expression.
        let Some(split_close) = matching_paren(inner, split_at) else {
            out.push_str(&rest[..open]);
            rest = &rest[open..];
            continue;
        };
        if split_close + 1 != inner.len() || !inner.ends_with(')') {
            out.push_str(&rest[..open]);
            rest = &rest[open..];
            continue;
        }
        let split_expr = &inner[split_at..split_close];
        out.push_str(&rest[..inner_open]);
        out.push('(');
        out.push_str(split_expr);
        out.push(')');
        rest = &rest[inner_close + 1..];
    }
}

// ── Fix 20 — the core's GLOB sentinel must not survive into a Perl string ──
//
// An unquoted glob word is tagged by the core with a `\x01SH2GLOB\x01`
// sentinel so a renderer can tell "expand this pattern" from "this is a
// literal" (sh_backend.rs / java_backend.rs strip it; the shared
// `render_word` in ir.rs strips it with `glob(…)`).
//
// The BRACE-EXPANSION path is the one place the Perl side never sees that
// strip: `render_brace_iter` builds its item list from the brace word's own
// prefix, quotes each result, and emits them as the `for` iterand — so the
// sentinel rides along inside the quotes:
//
//   sh:  for f in *.{txt,log,dat}; do case "$f" in *.txt) …
//   pl:  for my $__f (("\x01SH2GLOB\x01*.txt", "\x01SH2GLOB\x01*.log", …)) {
//
// Every element then matches nothing: `case` sees a literal path that cannot
// exist, so the whole loop prints `Text file: \x01SH2GLOB\x01*.txt` — the
// sentinel is *user-visible output*, not just a wrong branch. It is written
// straight to stdout, so this is a byte-for-byte gate failure on
// 064_19_complex_pattern_matching_extended_globs and 064_hard_to_generate
// (and the same shape in 000__04h, whose glob runs through an array instead).
//
// The fix is the shared renderer's own convention applied to the one
// rendering that skipped it: a sentinel-tagged item is a glob to be
// EXPANDED, so it renders as `glob('…')` — the same form `render_word`
// produces for a non-braced glob. Left alone, `glob` returns the empty list
// for a non-matching pattern, which is also what bash does with `nullglob`
// off for the *word count* in this position, so a pattern that matches
// nothing degrades the same way on both sides.
fn fix_glob_sentinel_in_for_iterand(src: &str) -> String {
    const PREFIX: &str = "for my $";
    const SENTINEL: &str = "\u{1}SH2GLOB\u{1}";
    if !src.contains(SENTINEL) {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    loop {
        let Some(at) = rest.find(PREFIX) else {
            out.push_str(rest);
            return out;
        };
        // To the end of the `for … (…)` header: the iterand list is a single
        // rendered line, and the sentinel only ever appears inside it.
        let header_end = rest[at..].find('\n').map(|i| at + i).unwrap_or(rest.len());
        let (head, tail) = rest.split_at(header_end);
        out.push_str(&strip_glob_in_header(head));
        rest = tail;
    }
}

/// Replace every sentinel-tagged `'…'` list element in one `for` header with
/// the `glob(…)` expansion. Elements that are not sentinel-tagged are left
/// byte-identical, so a plain literal list is not ours to touch.
fn strip_glob_in_header(head: &str) -> String {
    const SENTINEL: &str = "\u{1}SH2GLOB\u{1}";
    let mut out = String::with_capacity(head.len());
    let mut rest = head;
    loop {
        let Some(at) = rest.find(SENTINEL) else {
            out.push_str(rest);
            return out;
        };
        // The sentinel is followed by the pattern inside a single-quoted
        // Perl string; take that whole element. The pattern cannot contain
        // `'` (the renderer escapes it), so the next `'` is the terminator.
let after = at + SENTINEL.len();
        let Some(close_rel) = rest[after..].find('\'') else {
            out.push_str(rest);
            return out;
        };
        let close = after + close_rel;
        let pattern = &rest[after..close];
        // The element is a single-quoted Perl string, and the sentinel sits
        // INSIDE it — so the element opens one quote BEFORE the sentinel and
        // closes at the quote found above. `glob('…')` brings its own
        // quotes, so BOTH of the element's quotes go with the sentinel:
        // consuming only the closing one left the opening quote in place and
        // produced `'glob('*.txt')'`, which is a quoted string that is not a
        // call — the glob then matched nothing and the loop printed NOTHING
        // (064_19 / 064_hard_to_generate / 000__04h). Consume the opener by
        // requiring the byte before the sentinel to be `'` and stepping back
        // over it; if it is not (an unquoted element — nothing to undo), the
        // sentinel is dropped on its own and the element's quoting is
        // already correct, so the byte before it is left alone.
        let open = if at > 0 && rest.as_bytes()[at - 1] == b'\'' {
            Some(at - 1)
        } else {
            None
        };
        out.push_str(match open {
            Some(o) => &rest[..o],
            None => &rest[..at],
        });
        // The pattern itself keeps the renderer's own `\'` escaping,
        // unchanged, so a quote in a pattern still cannot break out.
        out.push_str(&format!("glob('{}')", pattern.replace('\'', "\\\\'")));
        rest = &rest[close + 1..];
    }
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
                    // Depth starts at 1 because `i += 2` below has ALREADY
                    // consumed the opening delimiter. Starting at 0 made the
                    // first closer drive `d` to -1, so the loop never broke and
                    // the scanner ran to end-of-line — silently swallowing the
                    // real close paren and leaving every `matching_paren` on a
                    // line containing a `q(…)` unbalanced.
                    let mut d = 1i32;
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
        // `{` sits this far into the line — and `open` must be an ABSOLUTE
        // offset into `src`, because every consumer below (`after_open`,
        // `body_start`, the brace walk) indexes into the whole program.
        // `line_start` is therefore part of the sum: without it, any sub
        // that is not the first line of the program got a body offset that
        // pointed back into the preamble, the `#`-comment guard then rejected
        // it, and `sub_spans` silently returned an empty list for every
        // program whose functions were not at byte 0.
        let open = line_start + open_rel + (line.len() - rest.len());
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
/// Spans ascend by byte offset and a sub nested inside one already spliced is
/// skipped: the enclosing body carried it into `out` already.
fn fix_sub_params(src: &str) -> String {
    let spans = sub_spans(src);
    if spans.is_empty() {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len() + 64);
    let mut cursor = 0usize;
    // `(start, end, name)` ascending by start. Splicing must walk them
    // FORWARD: a span's `start`/`end` are byte offsets into the original
    // `src`, so `src[cursor..start]` is only a valid (non-inverted) range
    // while `cursor <= start`. A span nested inside one already consumed is
    // skipped — its text was rewritten as part of the enclosing body.
    for (start, end, _name) in &spans {
        if *start < cursor || *end > src.len() || *start > *end {
            continue;
        }
        let body = &src[*start..*end];
        // The binding is only needed when the body actually READS a
        // positional parameter. Injecting it unconditionally put a
        // `local @__sh_args = @_;` into every function — including ones that
        // never touch `$1` — and, because `declare_param_array` only declares
        // the array when a `$__sh_args[…` read exists, that lone `@__sh_args`
        // was an undeclared package variable: a `use strict` compile error
        // that killed the whole program (func-after-or-assign printed nothing).
        let rewritten = rewrite_positional_params(body);
        let fixed = if rewritten.contains("$__sh_args[") {
            inject_param_binding(&rewritten)
        } else {
            rewritten
        };
        out.push_str(&src[cursor..*start]);
        out.push_str(&fixed);
        cursor = *end;
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

/// Fix 6 — the heredoc seam's terminator line carries a duplicated pipeline tail.
///
/// The renderer builds the shell command first and only then appends the
/// heredoc (`ir.rs:2226-2233`):
///
/// ```text
/// cmd = "<cmd with its pipeline>"
/// cmd += "\n" + body + "\n" + delim
/// ```
///
/// which is correct only while the command has nothing after the heredoc. For
/// `cat << 'EOF' | grep -v "^#" | sed 's/^/  /'` the pipeline is already part of
/// `cmd`, so the emitted text is
///
/// ```perl
/// system('bash', '-c', q['cat' <<'_SH2DOC_'
/// # This is a comment
/// …
/// _SH2DOC_ | 'grep' '-v' "^#" | 'sed' 's/^/  /']);
/// ```
///
/// A heredoc terminator must be ALONE on its line. Here it is followed by a
/// copy of the pipeline that the opener line already carries, so bash never
/// closes the heredoc: it warns "here-document … delimited by end-of-file",
/// swallows the terminator line into the body, and the pipeline runs on that
/// garbage — the literal `$(echo …)` lines come out un-indented and un-grepped.
///
/// The fix is to drop the tail that follows the terminator token on its line,
/// leaving the terminator alone. The pipeline it duplicates is already on the
/// opener, so nothing is lost.
fn fix_heredoc_delim(src: &str) -> String {
    if !src.contains("_SH2DOC_") {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len());
    for block in split_heredoc_blocks(src) {
        match block {
            HeredocBlock::Plain(t) => out.push_str(t),
            HeredocBlock::Heredoc { open, body, close } => {
                out.push_str(open);
                out.push_str(body);
                out.push_str(&closer_line(close));
            }
        }
    }
    out
}

enum HeredocBlock<'a> {
    Plain(&'a str),
    Heredoc {
        open: &'a str,
        body: &'a str,
        close: &'a str,
    },
}

/// The terminator line of a heredoc, with anything after the delimiter token
/// removed so the terminator stands alone — and the rest of the line
/// (everything up to and including its newline) dropped with it.
fn closer_line(close: &str) -> String {
    match close.find('\n') {
        Some(i) => format!("{}\n", &close[..=i]),
        None => close.to_string(),
    }
}

/// Split `src` into plain text and heredoc blocks, in order.
///
/// A block starts at the first `<<'_SH2DOC_'` (or bare `<<_SH2DOC_`) and ends
/// at the line that closes it. Anything that is not a well-formed open→close
/// pair is emitted as `Plain` so an unrecognised shape is left exactly as the
/// renderer wrote it.
fn split_heredoc_blocks(src: &str) -> Vec<HeredocBlock<'_>> {
    const OPENER: &str = "_SH2DOC_";
    let mut blocks = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = src[cursor..].find(OPENER) {
        let at = cursor + rel;
        // The opener must be a `<<` introducer, not an arbitrary mention.
        let Some(lt2) = src[..at].rfind("<<") else {
            break;
        };
        if lt2 + 2 != at {
            // `_SH2DOC_` not immediately after `<<` — plain text.
            blocks.push(HeredocBlock::Plain(&src[cursor..at + OPENER.len()]));
            cursor = at + OPENER.len();
            continue;
        }
        // `open` runs to the end of the line containing the `<<`; the BODY
        // follows, and the closer is the first later line that is exactly the
        // delimiter (bare, or quoted).
        let open_end = match src[at..].find('\n') {
            Some(i) => at + i + 1,
            None => src.len(),
        };
        let body_start = open_end;
        let Some(body_end) = find_heredoc_close(src, body_start, OPENER) else {
            blocks.push(HeredocBlock::Plain(&src[cursor..body_start]));
            cursor = body_start;
            continue;
        };
        blocks.push(HeredocBlock::Plain(&src[cursor..at]));
        blocks.push(HeredocBlock::Heredoc {
            open: &src[at..open_end],
            body: &src[body_start..body_end],
            close: &src[body_end..],
        });
        cursor = body_end;
        // `close` was pushed including the delimiter line; advance past it.
        let after = match src[cursor..].find('\n') {
            Some(i) => cursor + i + 1,
            None => src.len(),
        };
        cursor = after;
    }
    blocks.push(HeredocBlock::Plain(&src[cursor..]));
    blocks
}

/// The offset of the line that closes a heredoc whose delimiter is `delim`.
///
/// A terminator is the first line at/after `from` that BEGINS with the
/// delimiter as its own token — bare, or single/double quoted. It is matched on
/// that prefix rather than on whole-line equality, because the broken emission
/// puts the duplicated pipeline tail on the same line, and a whole-line test
/// would never find the terminator at all.
fn find_heredoc_close(src: &str, from: usize, delim: &str) -> Option<usize> {
    let mut at = from;
    while at <= src.len() {
        let end = src[at..].find('\n').map(|i| at + i).unwrap_or(src.len());
        if line_opens_with_delim(&src[at..end], delim) {
            return Some(at);
        }
        if end >= src.len() {
            return None;
        }
        at = end + 1;
    }
    None
}

/// True when `line` starts with `delim` as a standalone token (the rest of the
/// line may be the duplicated pipeline tail).
fn line_opens_with_delim(line: &str, delim: &str) -> bool {
    let line = line.trim_start();
    for open in [format!("{delim}"), format!("'{delim}'"), format!("\"{delim}\"")] {
        if let Some(rest) = line.strip_prefix(open.as_str()) {
            // The token must be followed by end-of-line or whitespace — `|` in
            // the broken tail — so `delim` never matches a longer identifier.
            if rest.is_empty() || rest.starts_with([' ', '\t', '|']) {
                return true;
            }
        }
    }
    false
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
    if std::env::var_os("SH2_DEBUG_PGFIX").is_some() {
        eprintln!("PGFIX-IN: {:#?}", prog.stmts);
    }
    let mut fixed = prog.clone();
    fixed.stmts = fix_stmts(&prog.stmts, &fns);
    if std::env::var_os("SH2_DEBUG_PGFIX").is_some() {
        eprintln!("PGFIX-OUT: {:#?}", fixed.stmts);
    }
    // The eval statement is not a normal Call as far as the IR is concerned:
    // its payload is SHELL TEXT that bash runs in the caller's scope, so it
    // is expanded (Fix 6) and then re-entered as a nested program (Fix 7).
    // Do that AFTER fix_stmts so the expansions are already corrected — so
    // it must run over the ALREADY-FIXED stmts, not the original (passing
    // `&prog.stmts` here discards every correction fix_stmts made above).
    fixed.stmts = fix_eval_stmts(&fixed.stmts);
    // Fix 24 — `. FILE` / `source FILE` shares state with the caller, so a
    // file this program wrote from a literal is inlined rather than
    // delegated to a subprocess.
    fixed.stmts = fix_dot_source_stmts(&fixed.stmts);
    // Fix 9 — the materialised process-substitution consumer's OWN status.
    // Same ordering rule as Fix 7: it walks whole statements (a rewrite of
    // the block's shape), so it runs over the ALREADY-FIXED statements.
    fixed.stmts = fix_ps_status_blocks(&fixed.stmts);
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

// ── Fix 13 — a `param` op's PATTERN/REPLACEMENT arrive pre-quoted ─────
//
// `shir::ast_to_ir` lowers `${s/abc/XYZ}` to
// `Call { func: "param", args: [Str("/"), Str("s"), Str("abc"), Str("XYZ")] }`,
// and the shared renderer runs BOTH operands through `param_value`, which
// calls `render_str_literal` on the text. That is right for a DEFAULT
// (`${a:-abc}` wants a Perl string) and wrong for a substitution: the
// operands of a `s///` are a PATTERN and a REPLACEMENT, not strings to be
// printed. A quoted operand reaches the regex as a literal run of quote
// characters, so
//
//   s=abc:def:ghi; echo "${s/abc/XYZ}"
//
// emitted `s/"abc"/"XYZ"/` — a pattern that matches `"abc"`, which never
// occurs — and the substitution was a silent no-op (bench-strsubst printed
// `front=abc:def:ghi`). The same mistake hits every glob/brace-expansion
// removal: `${p##*[/\\]}` emitted `s{^.*\[/\\\]}{}` and returned the whole
// string (parse-param-pattern-match printed `/opt/tool/git-tool` where bash
// prints `git-tool`).
//
// The fix is IR-level and exact: strip the operand quoting *in the IR*, so
// the renderer receives the bare pattern text and its own `glob_to_regex`
// does the escaping it was written to do. Nothing is re-implemented here —
// only the quotes that the substitution operands should never have had are
// removed.
//
// Scoped deliberately to the ops whose operands are PATTERNS (`/`, `//`,
// `#*`, `%*`) and to a `Str` operand carrying a balanced pair of the same
// quote character — the spelling `render_str_literal` itself produces. An
// operand that is not quoted, or whose quotes do not pair, is left alone:
// a pattern that genuinely contains a quote is the renderer's business.
fn fix_param_pattern_operands(expr: &IrExpr) -> IrExpr {
    let IrExpr::Call { func, args } = expr else {
        return expr.clone();
    };
    if func != "param" || args.len() < 3 {
        return expr.clone();
    }
    let Some(op) = args.first().and_then(crate::ir::call_arg_str) else {
        return expr.clone();
    };
    if !matches!(
        op.as_str(),
        "/" | "//" | "#" | "#:" | "##" | "##:" | "%" | "%:" | "%%" | "%%:"
    ) {
        return expr.clone();
    }
    let mut fixed = args.clone();
    // arg 2 is the pattern for every one of these ops. For `#`/`%` the
    // renderer re-reads it as a raw `Str` (it calls `call_arg_str` directly),
    // so unquoting it is what makes the glob compile. For `/` and `//` the
    // renderer runs it through `glob_to_regex` — also correct once unquoted.
    if let Some(unquoted) = unquote_operand(&args[2]) {
        fixed[2] = match &args[2] {
            IrExpr::Str(_, style) => IrExpr::Str(unquoted, style.clone()),
            _ => unreachable!("unquote_operand only matches a Str"),
        };
    }
    // arg 3 is the REPLACEMENT text, and only for the `/` and `//` ops: it
    // is a value to substitute in, not a pattern to compile. `#`/`%` have
    // no replacement operand, so an index-3 write would be out of range.
    if matches!(op.as_str(), "/" | "//") && args.len() >= 4 {
        if let Some(unquoted) = unquote_operand(&args[3]) {
            fixed[3] = match &args[3] {
                IrExpr::Str(_, style) => IrExpr::Str(unquoted, style.clone()),
                _ => unreachable!("unquote_operand only matches a Str"),
            };
        }
    }
    IrExpr::Call {
        func: func.clone(),
        args: fixed,
    }
}

// ── Fix 16 — `${arr[@]:offset:length}` must honour a NEGATIVE offset ────
//
// `shir::ast_to_ir` lowers `${primes[@]: -10}` to
// `Call { func: "param", args: [Str("slice"), Str("primes"), Str(" -10"), Str("")] }` —
// note the offset arrives with bash's space after the colon intact.
//
// The shared renderer parses that with `raw_off.parse::<i64>().unwrap_or(0)`
// (ir.rs, the `"slice"` arm). Three things go wrong, all from that one parse:
//
//   1. `" -10"` does not parse, so it became 0 — the LAST N elements came out
//      as ALL of them (051_primes printed every prime after "Last 10:").
//   2. Even trimmed, a negative offset was emitted as a bare Perl range bound:
//      `@a[-3..$#a]` runs from -3 to the END, not to -1.
//   3. A length past the end was emitted verbatim (`@a[1..10]`), and Perl pads
//      the missing elements with undef — `join` then renders 5 empty fields
//      where bash stops at the last element. Only a runtime length can clamp.
//
// bash's rule for a negative offset on an array is: count back from the LAST
// element, and an offset before the first is empty. That is exactly what
// {@link SLICE_PREAMBLE}'s `__sh_slice` implements, so every negative offset
// AND every bounded slice is routed through the helper.
//
// Scoped to the shape we fully understand: a `param("slice", …)` whose offset
// is numeric (after trimming) or whose length is present. An offset that is a
// VARIABLE (`${a[@]:$i:2}`) is left alone — it needs a runtime value this
// pre-pass cannot evaluate, and the shared renderer already has its own path
// for that.
fn fix_array_slice_bounds(expr: &IrExpr) -> IrExpr {
    let IrExpr::Call { func, args } = expr else {
        return expr.clone();
    };
    if func != "param" || args.len() < 3 {
        return expr.clone();
    }
    if args.first().and_then(crate::ir::call_arg_str).as_deref() != Some("slice") {
        return expr.clone();
    }
    // `${@:off:len}` / `${*:off:len}` slice the positional parameters, which
    // the renderer already handles with its own 1-based `@ARGV` arithmetic.
    let Some(raw_name) = args.get(1).and_then(crate::ir::call_arg_str) else {
        return expr.clone();
    };
    if raw_name == "@" || raw_name == "*" {
        return expr.clone();
    }
    let raw_off = args
        .get(2)
        .and_then(crate::ir::call_arg_str)
        .unwrap_or_default();
    let raw_len = args.get(3).and_then(crate::ir::call_arg_str).unwrap_or_default();
    // A VARIABLE offset or length cannot be resolved statically — leave the
    // shared renderer its own handling rather than emitting a literal 0.
    let off = match raw_off.trim().parse::<i64>() {
        Ok(n) => n,
        Err(_) => return expr.clone(),
    };
    let len = if raw_len.trim().is_empty() {
        None
    } else {
        match raw_len.trim().parse::<i64>() {
            Ok(n) => Some(n),
            Err(_) => return expr.clone(),
        }
    };
    // Nothing is wrong with a non-negative offset and no length: the range
    // `@a[off..$#a]` is exactly bash's unbounded slice.
    if off >= 0 && len.is_none() {
        return expr.clone();
    }
    // `${#arr[@]}` (length), `${!map[@]}` (keys) and `${arr[@]}` (all) reach
    // this arm with a non-numeric offset and are refused above.
    let base = raw_name
        .strip_suffix("[@]")
        .or_else(|| raw_name.strip_suffix("[0]"))
        .unwrap_or(&raw_name)
        .to_string();
    if base.is_empty() {
        return expr.clone();
    }
    // A SCALAR `${s:off:len}` is a SUBSTRING, not an array slice, and must
    // stay with the shared renderer, which lowers it to `substr`. The A1
    // drops the `[@]` that distinguished them, so the program's own array
    // census is the only signal left.
    //
    // Without this, `x="hello world"; echo "${x::-2}"` was routed into
    // `__sh_slice(\@x, 0, -2)` — an ARRAY helper handed a scalar's address.
    // Two things broke at once (bench-substr.sh,
    // parse-substring-double-colon.sh): `\@x` on a scalar declared `my $x`
    // does not compile under `strict` ("Global symbol \"@x\" requires
    // explicit package name"), so the program aborted at compile time
    // instead of printing; and had it compiled, the helper's
    // `$__len <= 0` guard would have returned the EMPTY list, where bash
    // prints the last two characters.
    if !crate::ir::array_var_pub(&base) {
        return expr.clone();
    }
    IrExpr::Call {
        func: "__sh_slice".to_string(),
        // The args are RAW PERL, not shell text: the renderer quotes a
        // `Str(arg, Raw)` as a string literal, and `@primes` must arrive as a
        // real array reference or `no strict 'refs'` has nothing to resolve.
        args: vec![
            IrExpr::RawExpr(format!("\\@{base}")),
            IrExpr::RawExpr(off.to_string()),
            match len {
                Some(n) => IrExpr::RawExpr(n.to_string()),
                None => IrExpr::RawExpr("undef".to_string()),
            },
        ],
    }
}

/// The text of a substitution operand with a balanced pair of identical
/// surrounding quotes removed, or `None` when it carries none.
///
/// `render_str_literal` emits `"abc"` / `'abc'`, so a `Str` operand that is
/// still quoted arrived through that path. Unbalanced or mismatched quotes
/// return `None` so a pattern which genuinely contains a quote survives.
fn unquote_operand(e: &IrExpr) -> Option<String> {
    let IrExpr::Str(s, _) = e else {
        return None;
    };
    let t = s.trim();
    let mut chars = t.chars();
    let first = chars.next()?;
    let last = t.chars().last()?;
    // A single-character operand ("`" or `'`) has no inner content to keep.
    if first != last || (first != '"' && first != '\'') || t.chars().count() < 2 {
        return None;
    }
    let inner = &t[first.len_utf8()..t.len() - last.len_utf8()];
    // `trim_matches('\\')` is what the renderer does to a replacement, so a
    // value that only became quoted by OUR caller must not double-unescape.
    Some(inner.to_string())
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
/// Fix 24 — `. FILE` / `source FILE` must share state with the caller.
///
/// bash sources a file INTO THE CURRENT SHELL: its assignments, function
/// definitions and `set` changes all persist. Delegating to a subprocess
/// (`bash -c '. ./lib'`) loses every one of them, so
/// dot-source-lib printed `shared=` where bash prints `shared=secret`.
///
/// The general case (an arbitrary file on disk) cannot be resolved at
/// translation time. The one shape that CAN is a file this very program
/// wrote earlier from a known literal — the corpus's
/// `echo 'v=secret' > lib.sh` then `. ./lib.sh` — so this pass collects the
/// literal bodies written to each path and INLINES the parsed statements at
/// the source site. `parse_shell_text` (shared with the eval lowering)
/// supplies them, and `ast_to_ir_raw` keeps every statement (nothing in a
/// sourced file is dead — the whole point is its side effects).
///
/// A path the program never wrote, or one written from a non-literal, is
/// left alone: the delegated call is wrong for it too, but guessing would be
/// worse than the status quo.
fn fix_dot_source_stmts(stmts: &[IrStmt]) -> Vec<IrStmt> {
    use std::collections::HashMap;
    /// path -> literal body, in source order.
    fn collect_writes(stmts: &[IrStmt], out: &mut HashMap<String, String>) {
        for s in stmts {
            match s {
                // `echo LIT > FILE` arrives as Redirect{inner:[Expr(echo …)]}.
                IrStmt::Redirect { inner, redirects } => {
                    if let Some(r) = redirects.iter().find(|r| {
                        matches!(r.mode.as_str(), "w" | "wc")
                            && matches!(r.fd, None | Some(1))
                    }) {
                        if let Some(path) = crate::ir::call_arg_str(&r.target) {
                            if let Some(body) = echo_body_text(inner) {
                                out.insert(path, body);
                            }
                        }
                    }
                    collect_writes(inner, out);
                }
                // The neutral `echo LIT > FILE` form.
                IrStmt::WriteFile { path, content, .. } => {
                    if let (Some(p), Some(c)) =
                        (crate::ir::call_arg_str(path), word_source_text(content))
                    {
                        out.insert(p, c);
                    }
                }
                IrStmt::If { then, elsifs, else_, .. } => {
                    collect_writes(then, out);
                    for (_, b) in elsifs {
                        collect_writes(b, out);
                    }
                    collect_writes(else_, out);
                }
                IrStmt::While { body, .. }
                | IrStmt::DoWhile { body, .. }
                | IrStmt::For { body, .. }
                | IrStmt::Block(body)
                | IrStmt::Subshell(body) => collect_writes(body, out),
                _ => {}
            }
        }
    }
    /// The literal text an `echo LIT` body writes (with its newline).
    fn echo_body_text(inner: &[IrStmt]) -> Option<String> {
        // The native-statement transform folds `echo LIT` into an `Output`
        // before this pass runs, but the un-folded `Expr(builtin("echo"))`
        // form also occurs. Accept both.
        let [one] = inner else { return None };
        match one {
            IrStmt::Output { value, newline, .. } => {
                let text = word_source_text(value)?;
                Some(if *newline { format!("{text}\n") } else { text })
            }
            IrStmt::Expr(e) => {
                let IrExpr::Call { func, args } = e else { return None };
                if func != "builtin" && func != "exec" {
                    return None;
                }
                if crate::ir::call_arg_str(args.first()?)?.as_str() != "echo" {
                    return None;
                }
                let mut words: Vec<String> = Vec::new();
                for a in crate::ir::exec_word_args(args) {
                    words.push(word_source_text(a)?);
                }
                if words.is_empty() {
                    return Some("\n".to_string());
                }
                // `echo -n` suppresses the terminator.
                let (nl, rest) = if words[0] == "-n" {
                    ("", &words[1..])
                } else {
                    ("\n", &words[..])
                };
                Some(format!("{}{nl}", rest.join(" ")))
            }
            _ => None,
        }
    }

    let mut writes: HashMap<String, String> = HashMap::new();
    collect_writes(stmts, &mut writes);

    fn walk(stmts: &[IrStmt], writes: &HashMap<String, String>) -> Vec<IrStmt> {
        let mut out = Vec::with_capacity(stmts.len());
        for s in stmts {
            match s {
                IrStmt::Expr(expr) => {
                    if let Some(path) = dot_source_path(expr) {
                        // Try the literal path, then the `./`-stripped and
                        // `./`-prefixed spellings (`./lib.sh` vs `lib.sh`).
                        let bare = path.strip_prefix("./").unwrap_or(&path).to_string();
                        let dotted = format!("./{bare}");
                        let body = writes
                            .get(&path)
                            .or_else(|| writes.get(&bare))
                            .or_else(|| writes.get(&dotted));
                        if let Some(body) = body {
                            if let Some(inner) = parse_shell_text(body) {
                                out.extend(inner);
                                continue;
                            }
                        }
                    }
                    out.push(s.clone());
                }
                IrStmt::If { cond, then, elsifs, else_ } => out.push(IrStmt::If {
                    cond: cond.clone(),
                    then: walk(then, writes),
                    elsifs: elsifs
                        .iter()
                        .map(|(c, b)| (c.clone(), walk(b, writes)))
                        .collect(),
                    else_: walk(else_, writes),
                }),
                IrStmt::While { cond, body } => out.push(IrStmt::While {
                    cond: cond.clone(),
                    body: walk(body, writes),
                }),
                IrStmt::DoWhile { cond, body, until } => out.push(IrStmt::DoWhile {
                    cond: cond.clone(),
                    body: walk(body, writes),
                    until: *until,
                }),
                IrStmt::For { var, iter, body } => out.push(IrStmt::For {
                    var: var.clone(),
                    iter: iter.clone(),
                    body: walk(body, writes),
                }),
                IrStmt::Block(b) => out.push(IrStmt::Block(walk(b, writes))),
                IrStmt::Subshell(b) => out.push(IrStmt::Subshell(walk(b, writes))),
                other => out.push(other.clone()),
            }
        }
        out
    }
    walk(stmts, &writes)
}

/// The literal path of a `. FILE` / `source FILE` call, if that is its shape.
fn dot_source_path(expr: &IrExpr) -> Option<String> {
    let IrExpr::Call { func, args } = expr else {
        return None;
    };
    if func != "builtin" && func != "exec" {
        return None;
    }
    let cmd = crate::ir::call_arg_str(args.first()?)?;
    if cmd != "." && cmd != "source" {
        return None;
    }
    let words = crate::ir::exec_word_args(args);
    let [one] = &words[..] else { return None };
    word_source_text(one)
}

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

/// Recursively apply {@link fix_param_pattern_operands} to every expression
/// reachable from `e`.
///
/// The substitution operand can appear at any depth — inside a command's
/// word list, a concatenation, a nested expansion, a condition, a ternary
/// arm — so a fix that only inspected the outermost expression would repair
/// `echo "${s/a/b}"` and leave `echo "x=${s/a/b}"` broken.
///
/// Every variant is enumerated rather than catching `_ => e.clone()`: a
/// catch-all would silently skip a construct the moment one is added, which
/// is exactly the failure mode this walk exists to prevent.
fn fix_expr(e: &IrExpr) -> IrExpr {
    match e {
        IrExpr::Int(_) | IrExpr::Str(..) | IrExpr::Var(..) | IrExpr::Regex { .. }
        | IrExpr::Range { .. } | IrExpr::RawExpr(_) | IrExpr::Bool(_)
        | IrExpr::Json(_) | IrExpr::Ident(_) => e.clone(),
        IrExpr::Index { var, key } => IrExpr::Index {
            var: var.clone(),
            key: Box::new(fix_expr(key)),
        },
        IrExpr::BinOp { lhs, op, rhs } => IrExpr::BinOp {
            lhs: Box::new(fix_expr(lhs)),
            op: op.clone(),
            rhs: Box::new(fix_expr(rhs)),
        },
        IrExpr::Call { func, args } => {
            // The correction applies to THIS call first, then to its
            // arguments: a `param` operand can itself contain a `param`
            // (`${s/${a}/b}`), and the outer unquoting must not hide the
            // inner one.
            let corrected = fix_param_pattern_operands(e);
            // Fix 16: a `${arr[@]:o:l}` whose offset/length is static must go
            // through the runtime helper so a negative offset and an
            // over-long length behave like bash's.
            let corrected = fix_array_slice_bounds(&corrected);
            match corrected {
                IrExpr::Call { func, args } => IrExpr::Call {
                    func,
                    args: args.iter().map(fix_expr).collect(),
                },
                other => other,
            }
        }
        IrExpr::MethodCall { obj, method, args } => IrExpr::MethodCall {
            obj: Box::new(fix_expr(obj)),
            method: method.clone(),
            args: args.iter().map(fix_expr).collect(),
        },
        IrExpr::Ternary { cond, then, else_ } => IrExpr::Ternary {
            cond: Box::new(fix_expr(cond)),
            then: Box::new(fix_expr(then)),
            else_: Box::new(fix_expr(else_)),
        },
        IrExpr::DefinedOr { expr, default } => IrExpr::DefinedOr {
            expr: Box::new(fix_expr(expr)),
            default: Box::new(fix_expr(default)),
        },
        IrExpr::Interpolate(parts) => IrExpr::Interpolate(
            parts
                .iter()
                .map(|p| match p {
                    InterpPart::Lit(t) => InterpPart::Lit(t.clone()),
                    InterpPart::Expr(inner) => InterpPart::Expr(Box::new(fix_expr(inner))),
                })
                .collect(),
        ),
        IrExpr::Capture { expr, native } => IrExpr::Capture {
            expr: Box::new(fix_expr(expr)),
            native: *native,
        },
        IrExpr::Arrow(stmts) => IrExpr::Arrow(stmts.clone()),
        IrExpr::Array(elems) => IrExpr::Array(elems.iter().map(fix_expr).collect()),
        IrExpr::ArrayComp {
            var,
            iter,
            elem,
            cond,
        } => IrExpr::ArrayComp {
            var: var.clone(),
            iter: Box::new(fix_expr(iter)),
            elem: Box::new(fix_expr(elem)),
            cond: cond.as_ref().map(|c| Box::new(fix_expr(c))),
        },
        IrExpr::Lambda { params, body } => IrExpr::Lambda {
            params: params.clone(),
            body: body.clone(),
        },
        IrExpr::Arith(_) | IrExpr::Ext(_) | IrExpr::Splice(_) => e.clone(),
        IrExpr::Object(fields) => IrExpr::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), fix_expr(v)))
                .collect(),
        ),
    }
}

fn fix_stmt(s: &IrStmt, fns: &std::collections::HashSet<String>) -> IrStmt {
    match s {
        // Fix 13 — a `param` substitution/pattern-removal's operands are a
        // PATTERN and a REPLACEMENT, not strings to be printed, and they
        // arrive carrying the quoting `render_str_literal` would add. Unquote
        // them in the IR so the renderer's own `glob_to_regex` does the
        // escaping, instead of compiling a regex that matches a run of quote
        // characters and silently never matching.
        //
        // `fix_expr` is the recursive entry point: a `${s/…}` can sit in an
        // assignment's RHS, a command word, a condition, or a nested
        // expansion, so walking only the top-level expression would fix the
        // shallow case and leave the rest broken.
        IrStmt::Expr(e) => IrStmt::Expr(fix_expr(e)),
        IrStmt::Assign {
            targets,
            expr,
            asm,
        } => IrStmt::Assign {
            targets: targets.clone(),
            expr: fix_expr(expr),
            asm: asm.clone(),
        },
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
        // Fix 8 — a redirect stage INSIDE a pipeline is lifted out to an outer
        // `Redirect` statement, so the renderer reaches its `Redirect` arm
        // instead of the bare `redirect` call path that refuses.
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "pipeline" => {
            match fix_pipeline_redirect_stage(s, fns) {
                Some(rebuilt) => rebuilt,
                None => s.clone(),
            }
        }
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

/// Fix 7 — a `redirect` stage inside a pipeline is rewritten to the OUTER
/// `Redirect` statement the renderer already knows how to lower.
///
/// The two spellings of
///
/// ```sh
/// while read -r line; do …; done < /etc/passwd | head -3
/// ```
///
/// arrive as genuinely different IR:
///
/// * **no pipeline** → `IrStmt::Redirect { inner: [While …], redirects: [fd 0 r] }`.
///   The renderer's `Redirect` arm opens the file on `local *STDIN` and runs
///   the loop natively — this is the shape that already worked.
/// * **with a pipeline** → `IrStmt::Pipeline { stages: [[ redirect(
///   [whileLoop(…)], fd 0 r )], [exec head -3]] }`, and `redirect` is an
///   ordinary *call* node, not the `Redirect` statement.
///
/// The renderer has a `pipeline` arm that understands `try_native_while_read_sort`
/// (`… < FILE | sort`), but no general arm for "a pipeline whose FIRST stage
/// carries its own redirect". So the stage fell through to the bare-`redirect`
/// call path, which only honours an **fd 1** write target: with an fd 0 read
/// spec it found no output redirect, `droppable` was false, and it emitted
///
/// ```perl
/// die "otranspilerl: shIR redirect not yet supported by the Perl backend\n";
/// ```
///
/// — the loop and its output were gone and the whole program stopped (091,
/// 065, 070_gnuisms, 063_11, 063_hard_to_parse all produced no output at all).
///
/// bash's semantics for `cmd < FILE | head` are: the producer runs with STDIN
/// bound to FILE, and its STDOUT is piped onward. Restoring exactly that shape
/// — the pipeline wrapped in an `IrStmt::Redirect` around the producer — hands
/// it to the already-exercised, already-correct `Redirect` arm, so the fix adds
/// no new emission of its own.
///
/// Scoped deliberately:
/// * only the FIRST stage may carry the redirect (bash binds `|` left to right,
///   so a redirect on a later stage is a different construct);
/// * only ONE redirect is lifted (`cmd < a > b | head` keeps its old shape —
///   guessing at the rest would be worse than leaving the existing behaviour);
/// * a pipeline that captures its stdout (`x=$(… | head)`) is left untouched,
///   since that renderer arm consumes the stages directly.
/// Fix 8 — a `redirect` stage inside a pipeline is lifted to an OUTER
/// `Redirect` statement, which the renderer already knows how to lower.
///
/// The two spellings of
///
/// ```sh
/// while read -r line; do …; done < /etc/passwd | head -3
/// ```
///
/// arrive as genuinely different IR:
///
/// * **no pipeline** → `IrStmt::Redirect { inner: [While …], redirects: [fd 0 r] }`.
///   The renderer's `Redirect` arm opens the file on `local *STDIN` and runs
///   the loop natively — this is the shape that already worked.
/// * **with a pipeline** → a `pipeline` CALL whose stages are
///   `[Arrow([redirect([whileLoop(…)], fd 0 r)]), Arrow([exec head -3])]`,
///   where `redirect` is an ordinary call node, not the `Redirect` statement.
///
/// The renderer has a `pipeline` arm that understands `try_native_while_read_sort`
/// (`… < FILE | sort`), but no general arm for "a pipeline whose FIRST stage
/// carries its own redirect". So the stage fell through to the bare-`redirect`
/// call path, which only honours an **fd 1** write target: with an fd 0 read
/// spec it found no output redirect, `droppable` was false, and it emitted
///
/// ```perl
/// die "otranspilerl: shIR redirect not yet supported by the Perl backend\n";
/// ```
///
/// — the loop and its output were gone and the whole program stopped (091,
/// 065, 070_gnuisms, 063_11, 063_hard_to_parse all produced no output at all).
///
/// bash's semantics for `cmd < FILE | head` are: the producer runs with STDIN
/// bound to FILE, and its STDOUT is piped onward. Since a `Redirect` statement
/// applies its redirects to everything inside it, wrapping the *whole* pipeline
/// in one is semantically identical for the producer and leaves the consumer
/// alone — so the correction needs no new emission, only the already-exercised
/// `Redirect` arm to be reached instead of the bare call's refusal.
///
/// Scoped deliberately:
/// * only the FIRST stage may carry the redirect (bash binds `|` left to right,
///   so a redirect on a later stage is a different construct);
/// * only ONE redirect is lifted (`cmd < a > b | head` keeps its old shape —
///   guessing at the rest would be worse than leaving the existing behaviour);
/// * a pipeline that captures its stdout is left untouched, since the
///   `Capture` renderer arm consumes the stages directly.
fn fix_pipeline_redirect_stage(
    s: &IrStmt,
    fns: &std::collections::HashSet<String>,
) -> Option<IrStmt> {
    let IrStmt::Expr(IrExpr::Call { func, args }) = s else {
        return None;
    };
    if func != "pipeline" {
        return None;
    }
    // `pipeline(Array([Arrow([stage]), …]))` — the call form the renderer
    // unwraps in `pipeline_stages_of`.
    let stages: Vec<Vec<IrStmt>> = pipeline_stages_of_call(args)?;
    // Exactly TWO stages, and every argument contributed a stage. Anything
    // else is a shape we do not recognise and must come through UNCHANGED.
    if std::env::var_os("SH2_DEBUG_PGFIX").is_some() {
        eprintln!(
            "PGFIX pipeline: stages={} array_len={} spec={:?}",
            stages.len(),
            pipeline_array_len(args),
            redirect_stage_spec(&stages[0]).map(|(f, m, _)| (f, m))
        );
    }
    if stages.len() != 2 || stages.len() != pipeline_array_len(args) {
        return None;
    }
    let (fd, mode, target) = redirect_stage_spec(&stages[0])?;
    // `fd: 0` is the `done < FILE` reading shape. An fd-1 write, and an
    // absent fd (which means STDOUT), are both handled by the renderer's
    // existing stage-local `select()` dance — leave them alone.
    if fd != Some(0) {
        return None;
    }
    // A redirect Call is not a statement the renderer lowers directly, so the
    // producer is re-entered as the `Redirect` statement's inner body and the
    // consumer rides along in the same pipeline, keeping the producer's
    // STDOUT piped onward exactly as bash binds `|`.
    let producer = redirect_stage_inner(&stages[0])?;
    if std::env::var_os("SH2_DEBUG_PGFIX").is_some() {
        eprintln!("PGFIX lift: {} producer stmts", producer.len());
    }
    let rebuilt = IrStmt::Redirect {
        inner: vec![IrStmt::Pipeline {
            stages: vec![fix_stmts(&producer, fns), stages[1].clone()],
            last_output: None,
            capture: None,
            cmd_str: None,
        }],
        redirects: vec![IrRedirect {
            fd: Some(0),
            mode,
            target,
            interpolate: false,
        }],
    };
    Some(rebuilt)
}

/// `pipeline_stages_of`'s expression half: the stages of a `pipeline(...)`
/// call. `None` unless every argument is an `Arrow`-wrapped stage, so a shape
/// the renderer could not unwrap is never half-understood here.
fn pipeline_stages_of_call(args: &[IrExpr]) -> Option<Vec<Vec<IrStmt>>> {
    let mut out: Vec<Vec<IrStmt>> = Vec::with_capacity(args.len());
    for a in args {
        match a {
            IrExpr::Array(elems) => {
                for el in elems {
                    match el {
                        IrExpr::Arrow(v) => out.push(v.clone()),
                        _ => return None,
                    }
                }
            }
            IrExpr::Arrow(v) => out.push(v.clone()),
            _ => return None,
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// How many stages the `pipeline` call's arrays actually carry, so a call
/// whose stage count disagrees with its array length is refused rather than
/// silently truncated.
///
/// Counted over the NESTED shape the IR actually uses. `pipeline` reaches
/// this module either as a bare `Call` (the form `IrStmt::Pipeline` lowers
/// to) or already wrapped by the IR builder into
///
/// ```text
/// pipeline(Array([ <expr>, Arrow([ <expr>, … ]) ]))
/// ```
///
/// — one outer array whose elements are the stage bodies, the LAST of which
/// is itself the `Arrow`-wrapped stage list. `pipeline_stages_of_call` flattens
/// both levels, so the count has to as well: counting only the top level
/// counts that wrapper as one stage, which made every two-stage
/// `< while read … done < FILE | sort` pipeline disagree with its own
/// `stages.len()` and be refused by Fix 8 — the bare `redirect` call path then
/// emitted
///
/// ```perl
/// die "otranspilerl: shIR redirect not yet supported by the Perl backend\n";
/// ```
///
/// and the loop's output vanished (088, 091).
fn pipeline_array_len(args: &[IrExpr]) -> usize {
    args.iter()
        .map(|a| match a {
            IrExpr::Array(elems) => elems
                .iter()
                .map(|el| match el {
                    IrExpr::Arrow(inner) => inner.len(),
                    _ => 1,
                })
                .sum(),
            IrExpr::Arrow(_) => 1,
            _ => 0,
        })
        .sum()
}

/// The single redirect spec of a `redirect(Arrow([…]), …)` pipeline stage, as
/// `(fd, mode, target)`. `None` unless the stage is exactly that shape with
/// exactly one spec.
fn redirect_stage_spec(stage: &[IrStmt]) -> Option<(Option<i32>, String, IrExpr)> {
    let Some(IrStmt::Expr(IrExpr::Call { func, args })) = stage.first() else {
        return None;
    };
    if func != "redirect" || args.len() != 2 {
        return None;
    }
    if !matches!(args[0], IrExpr::Arrow(_)) {
        return None;
    }
    let spec = match &args[1] {
        IrExpr::Array(elems) if elems.len() == 1 => elems[0].clone(),
        _ => return None,
    };
    let IrExpr::Object(fields) = spec else {
        return None;
    };
    let get = |k: &str| {
        fields
            .iter()
            .find_map(|(name, v)| if name == k { Some(v.clone()) } else { None })
    };
    let fd = match get("fd") {
        Some(IrExpr::Int(i)) => Some(i as i32),
        Some(IrExpr::Str(s, _)) => s.parse::<i32>().ok(),
        // An absent `fd` means STDOUT (1) — never liftable.
        _ => None,
    };
    let mode = match get("mode") {
        Some(IrExpr::Str(m, _)) => m,
        _ => return None,
    };
    let target = get("target")?;
    Some((fd, mode, target))
}

/// The producer statements a `redirect(Arrow([…]), […])` stage wraps. `None`
/// unless the stage is exactly that shape — a redirect alongside other
/// statements, or carrying more than one spec, is a different construct.
fn redirect_stage_inner(stage: &[IrStmt]) -> Option<Vec<IrStmt>> {
    let Some(IrStmt::Expr(IrExpr::Call { args, .. })) = stage.first() else {
        return None;
    };
    let (IrExpr::Arrow(inner), IrExpr::Array(specs)) = (&args[0], &args[1]) else {
        return None;
    };
    if specs.len() != 1 {
        return None;
    }
    Some(inner.clone())
}
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
            // A local-function call is wrapped in `__sh_fn_status`; every
            // other `exec`/`builtin` call is passed through with its
            // arguments untouched. Fix 15 used to rewrite a `test` operand's
            // `${v+word}` HERE as well as in `fix_expr`, but the renderer's
            // own `+` arm renders the set/unset DECISION correctly, so the
            // rewrite was no longer needed — and it was actively wrong: it
            // mapped `+` onto the `=` arm, whose predicate is
            // `defined && length`, i.e. the STRICTER `:+` test. A variable
            // set to the empty string is set, so `${V+set}` is non-empty in
            // bash while the rewritten form yielded `V` itself (empty) —
            // inverting the guard. Fix 15's comment argued for the `=` arm
            // on the grounds that "set (even empty) keeps the value"; that
            // is true of `=` and false of the arm the rewrite actually
            // reached once the `+` arm landed.
            let fixed_args = args.clone();
            if is_local_fn {
                IrExpr::Call {
                    func: "__sh_fn_status".to_string(),
                    args: fixed_args,
                }
            } else {
                IrExpr::Call {
                    func: func.clone(),
                    args: fixed_args,
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
    # This sub is injected right after the `use` block, BEFORE the shared
    # renderer's own preamble (`our $CHILD_ERROR = 0;`), so a bare
    # $CHILD_ERROR here is a `use strict` COMPILE error ("Global symbol
    # $CHILD_ERROR requires explicit package name") and the whole program
    # aborts — 051_primes printed NOTHING. Re-declaring the package global
    # inside the sub is always legal, makes the helper independent of
    # preamble ordering, and (same package) still writes the caller's
    # variable.
    our $CHILD_ERROR;
    $CHILD_ERROR = $st;
    return $st == 0 ? 1 : 0;
}
"#;

/// True when the rendered program needs `__sh_fn_status` defined.
pub fn needs_fn_status_helper(perl_src: &str) -> bool {
    perl_src.contains("__sh_fn_status(")
}

/// Emitted on demand when the body actually contains a `__sh_slice(` call
/// (Fix 16). Same gating discipline as {@link FN_STATUS_PREAMBLE}.
pub const SLICE_PREAMBLE: &str = r#"
sub __sh_pos {
    # bash's `${@:offset}` / `${*:offset}` — a slice of the POSITIONAL
    # PARAMETERS, which is a LIST. The renderer lowers it to
    # `join(' ', @ARGV[N..$#ARGV])`, and that join is WRONG for an empty
    # slice: `@ARGV[N..$#ARGV]` on an empty `@ARGV` is the range `N..-1`,
    # which yields NO elements, so the inner join returns '' — but the
    # OUTER join then sees that '' as ONE empty field and emits a single
    # space. bash emits nothing at all (parse-at-slice-param.sh: `' $'` vs
    # `  $'`).
    #
    # Returning the LIST, not a joined string, is what fixes it: an empty
    # list contributes zero fields to the enclosing join, and a non-empty
    # one contributes exactly its elements. The offset is 1-based, as bash
    # counts it; a non-positive or past-the-end offset is empty.
    my $__off = shift();
    my $__len = shift();
    return () if $__off < 1;
    return () if $__off > $#ARGV + 1;
    my $__end = defined($__len) ? $__off + $__len - 1 : $#ARGV;
    $__end = $#ARGV if $__end > $#ARGV;
    return () if $__end < $__off - 1;
    return @ARGV[$__off - 1 .. $__end];
}
"#;
/// {@link SLICE_PREAMBLE}, continued: the array-slice helper of Fix 16.
pub const SLICE_PREAMBLE_2: &str = r#"
sub __sh_slice {
    # bash's `${arr[@]:offset:length}` semantics, which a Perl range bound
    # does not reproduce:
    #
    #   * a NEGATIVE offset counts back from the last element, and the slice
    #     ends at the last element (not at `$#arr`);
    #   * an offset before the first element yields the EMPTY list;
    #   * a length past the end stops at the last element — a Perl range would
    #     instead pad with undef, which `join` renders as empty fields.
    #
    # So both bounds are clamped against the real element count here.
    my $__av = shift();   # \@name
    my $__off = shift();
    my $__len = shift();
    my $__n = scalar(@$__av);
    my $__start = $__off < 0 ? $__n + $__off : $__off;
    return () if $__start < 0;
    return () if $__start > $__n;
    return @$__av[$__start .. $__n - 1] if !defined($__len);
    return () if $__len <= 0;
    my $__end = $__start + $__len - 1;
    $__end = $__n - 1 if $__end > $__n - 1;
    return @$__av[$__start .. $__end];
}
"#;

/// True when the rendered program needs `__sh_slice` defined.
pub fn needs_slice_helper(perl_src: &str) -> bool {
    perl_src.contains("__sh_slice(")
}

/// True when the rendered program needs `__sh_pos` defined (Fix 22).
pub fn needs_pos_slice_helper(perl_src: &str) -> bool {
    perl_src.contains("__sh_pos(")
}

// ── Fix 22 — `${@:offset}` is a LIST slice; the renderer's join is not ────
//
// The `"slice"` arm of `param` in ir.rs lowers `${@:off}` to
//     join(' ', @ARGV[off-1..$#ARGV])
// and `${@:off:len}` to
//     join(' ', @ARGV[off-1..off-1+len-1]).
// Both are inside whatever the renderer was building, so a `${@:1}` that
// became the BODY of an enclosing `join(' ', …)` is doubly wrapped:
//
//     print("1:[" . (join(' ', join(' ', @ARGV[0..$#ARGV]))) . "]", "\n");
//
// An empty slice then contributes one empty FIELD where bash contributes
// none, printing a lone space (parse-at-slice-param.sh, `  $` vs ` $`).
// A bounded slice that runs past the end is worse: Perl pads the missing
// elements with undef and `join` prints the padding as empty fields, where
// bash stops at the last argument.
//
// `__sh_pos` returns the LIST, so an empty slice contributes zero fields and
// a short slice contributes only real arguments. The bounded form is routed
// through it too, with the same end index the renderer computed — a bounded
// slice past the end must not fabricate fields either.
//
// Only the UNBOUNDED-to-end form (`..$#ARGV`) and the bounded numeric form
// are rewritten, and only when the index is a plain literal, so anything
// else is left exactly as the renderer wrote it.
fn fix_pos_param_slice(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        out.push_str(&fix_pos_param_slice_in_line(line));
        out.push('\n');
    }
    // `lines()` drops a trailing newline; put it back so a program that ended
    // with one still does.
    if !src.ends_with('\n') && !out.is_empty() {
        out.pop();
    }
    out
}

/// One line of {@link fix_pos_param_slice}.
fn fix_pos_param_slice_in_line(line: &str) -> String {
    const NEEDLE: &str = "join(' ', @ARGV[";
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    loop {
        let Some(at) = rest.find(NEEDLE) else {
            out.push_str(rest);
            return out;
        };
        let after = &rest[at + NEEDLE.len()..];
        let Some(close) = after.find(']') else {
            out.push_str(rest);
            return out;
        };
        let bounds = &after[..close];
        let Some((lo, hi)) = bounds.split_once("..") else {
            out.push_str(rest);
            return out;
        };
        // Both bounds must be plain literals — a computed index is left to
        // the renderer, which may have a reason we do not.
        let is_lit = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
        // Unbounded: `@ARGV[lo..$#ARGV]` — bash's `${@:lo+1}`.
        let replacement = if hi == "$#ARGV" && is_lit(lo) {
            let start = lo.parse::<u64>().unwrap_or(0) + 1;
            format!("join(' ', __sh_pos({start}))")
        } else if is_lit(lo) && is_lit(hi) {
            // Bounded: bash's `${@:off:len}`. The renderer already turned
            // that into a closed 0-based range, so pass the 1-based start
            // and keep the count; the helper clamps at the real end.
            if let (Ok(l), Ok(h)) = (lo.parse::<u64>(), hi.parse::<u64>()) {
                if h < l {
                    // A reversed range means bash asked for nothing.
                    "''".to_string()
                } else {
                    format!("join(' ', __sh_pos({}, {}))", l + 1, h + 1 - l)
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        if replacement.is_empty() {
            // Not a shape we rewrite: copy through and continue AFTER the
            // needle so the next occurrence is still considered.
            out.push_str(&rest[..at + NEEDLE.len()]);
            rest = after;
            continue;
        }
        out.push_str(&rest[..at]);
        out.push_str(&replacement);
        // The needle is `join(' ', @ARGV[…]`: `]` closes the slice, and the
        // character after it is the `)` that closes the ORIGINAL `join(`.
        // The replacement re-opens its own balanced `join(…)`, so copying
        // that `)` through would leave one paren too many —
        // `join(' ', __sh_pos(3))))` — which is a Perl COMPILE error that
        // takes the whole program down, not a wrong value
        // (parse-at-slice-param.sh, whose only other symptom is the
        // single-space diff this pass exists to fix).
        //
        // Consume exactly one `)`, and only when it really is the join's
        // closer: `join(' ', @ARGV[0..1] . "x")` puts `. "x"` there, so the
        // `strip_prefix` miss is the correct fallback, not an error.
        rest = after[close + 1..].strip_prefix(')').unwrap_or(&after[close + 1..]);
    }
}

#[cfg(test)]
mod pgf_tests {
    use super::*;

    /// Render `src` through the full GENERATOR entry point (the shared
    /// renderer plus this module's fixes), so a test exercises the same path
    /// `./fail` gates.
    fn gen(src: &str) -> String {
        let mut p = crate::Parser::new(src);
        let cmds = p.parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        generate(&prog)
    }

    // ── Fix 16 — the array-slice bounds ─────────────────────────────────
    //
    // Each of these pins ONE of the three ways the shared renderer's
    // `raw_off.parse::<i64>().unwrap_or(0)` mis-lowered `${a[@]:o:l}`.

    #[test]
    fn slice_negative_offset_uses_the_helper() {
        // The offset arrives as " -10" (bash's space after the colon). It did
        // not parse, became 0, and "Last 10:" printed every element.
        let out = gen("a=(2 3 5 7 11)\necho \"${a[@]: -3}\"\n");
        assert!(
            out.contains("__sh_slice(\\@a, -3, undef)"),
            "negative offset must route through the helper, got:\n{out}"
        );
        assert!(
            !out.contains("@a[0..$#a]"),
            "must not degrade to the whole array, got:\n{out}"
        );
    }

    #[test]
    fn slice_overlong_length_is_clamped_at_runtime() {
        // `@a[1..10]` pads with undef; bash stops at the last element. Only
        // the helper, which knows the element count, can express this.
        let out = gen("a=(1 2 3 4 5)\necho \"${a[@]:1:10}\"\n");
        assert!(
            out.contains("__sh_slice(\\@a, 1, 10)"),
            "bounded slice must go through the helper, got:\n{out}"
        );
        assert!(!out.contains("@a[1..10]"), "unclamped range, got:\n{out}");
    }

    #[test]
    fn slice_unbounded_nonnegative_is_left_to_the_renderer() {
        // No negative bound and no length — `@a[1..$#a]` is exactly bash, so
        // the helper must NOT be introduced for it.
        let out = gen("a=(1 2 3)\necho \"${a[@]:1}\"\n");
        assert!(
            out.contains("@a[1..$#a]") && !out.contains("__sh_slice("),
            "plain unbounded slice must stay a range, got:\n{out}"
        );
    }

    #[test]
    fn slice_helper_is_declared_only_when_called() {
        // Same discipline as __sh_fn_status: a program with no slice must not
        // grow a dead sub.
        let out = gen("a=(1 2 3)\necho \"${a[@]}\"\n");
        assert!(!needs_slice_helper(&out), "helper needed unexpectedly:\n{out}");
        assert!(!out.contains("sub __sh_slice"), "dead sub emitted:\n{out}");
        assert!(needs_slice_helper(&gen("a=(1 2 3)\necho \"${a[@]: -1}\"\n")));
    }

    #[test]
    fn slice_positional_params_are_left_alone() {
        // `${@:3}` is the positionals, with the renderer's own 1-based
        // arithmetic — rewriting it would break that.
        let out = gen("set -- a b c d\necho \"${@:3}\"\n");
        assert!(
            !out.contains("__sh_slice("),
            "positional slice must keep the renderer's path, got:\n{out}"
        );
    }

    #[test]
    fn slice_helper_matches_bash_bounds() {
        // Every expectation below is what bash prints for
        // `a=(1 2 3 4 5)` — derived from the interpreter, not reasoned out.
        let helper = SLICE_PREAMBLE;
        for (off, len, want) in [
            ("0", "2", "1 2"),
            ("2", "2", "3 4"),
            ("1", "10", "2 3 4 5"),
            ("-3", "undef", "3 4 5"),
            ("-10", "undef", ""),
            ("-2", "1", "4"),
            ("3", "undef", "4 5"),
            ("9", "undef", ""),
            ("0", "0", ""),
        ] {
            let script = format!(
                "{helper}\n@a=(1,2,3,4,5);\n\
                 print join(' ', __sh_slice(\\@a, {off}, {len})), \"\\n\";\n"
            );
            let dir = std::env::temp_dir().join(format!("pgf_slice_{off}_{len}"));
            std::fs::create_dir_all(&dir).expect("tmpdir");
            let path = dir.join("t.pl");
            std::fs::write(&path, &script).expect("write");
            let out = std::process::Command::new("perl")
                .arg(&path)
                .output()
                .expect("run perl");
            let got = String::from_utf8_lossy(&out.stdout);
            assert_eq!(
                got, format!("{want}\n"),
                "__sh_slice(@a, {off}, {len}) should be {want:?}"
            );
        }
    }
    #[test]
    fn _dbg_dump_ir() {
        let src = "is_prime() { if [ $1 -eq 2 ]; then return 0; fi; return 1; }\nx=3; if is_prime $x; then echo yes; fi\n";
        let mut p = crate::Parser::new(src);
        let cmds = p.parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        println!("DBG IR: {prog:#?}");
        println!("DBG AST: {cmds:#?}");
    }
    #[test]
    fn _dbg_sub_ir() {
        let mut p = crate::Parser::new("s=abc:def:ghi\necho \"${s/abc/XYZ}\"\n");
        let cmds = p.parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        let rendered = crate::ir::shir_to_perl(&prog);
        println!("DBGSUB DIRECT:\n{rendered}");
        for st in &prog.stmts {
            pgf_probe(st, 0);
        }
    }
    fn pgf_probe(st: &IrStmt, d: usize) {
        if let IrStmt::Expr(e) = st {
            pgf_probe_expr(e, d);
        }
    }
    fn pgf_probe_expr(e: &IrExpr, d: usize) {
        let pad = "  ".repeat(d);
        if let IrExpr::Call { func, args } = e {
            if func == "param" {
                for (i, a) in args.iter().enumerate() {
                    if let IrExpr::Str(s, _) = a {
                        println!("{pad}param[{i}] = {s:?}");
                    }
                }
            }
            for a in args { pgf_probe_expr(a, d + 1); }
        } else if let IrExpr::Array(elems) = e {
            for a in elems { pgf_probe_expr(a, d + 1); }
        }
    }
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

    /// Regression (Fix 22): the slice rewrite must consume the `)` that
    /// closed the ORIGINAL `join(`, or it leaves one paren too many.
    ///
    /// The needle stops at `]` — the character after it is the join's own
    /// closer, and the replacement supplies its own balanced `join(…)`.
    /// Copying that `)` through produced `join(' ', __sh_pos(3))))`, a Perl
    /// COMPILE error, so the program printed nothing at all rather than
    /// merely the wrong value.
    #[test]
    fn pos_slice_rewrite_consumes_join_close_paren() {
        // The exact shape the renderer emits inside an array assignment,
        // which is where parse-at-slice-param.sh hits it.
        assert_eq!(
            fix_pos_param_slice_in_line(
                "@ARGV = ($ARGV[0], $tempfile, join(' ', @ARGV[2..$#ARGV]));  # line 3"
            ),
            "@ARGV = ($ARGV[0], $tempfile, join(' ', __sh_pos(3)));  # line 3"
        );
        // The `print(...)` shape: same needle, different surrounding closers.
        assert_eq!(
            fix_pos_param_slice_in_line("print(join(' ', @ARGV[0..$#ARGV]), \"\\n\");"),
            "print(join(' ', __sh_pos(1)), \"\\n\");"
        );
        // A bounded slice is rewritten through the same path.
        assert_eq!(
            fix_pos_param_slice_in_line("x = join(' ', @ARGV[0..1]);"),
            "x = join(' ', __sh_pos(1, 2));"
        );
        // A `)` that is NOT the join's closer must survive: stripping it
        // would corrupt `join(' ', @ARGV[0..1] . "x")`.
        assert_eq!(
            fix_pos_param_slice_in_line("x = join(' ', @ARGV[0..$#ARGV] . 'x');"),
            "x = join(' ', __sh_pos(1)) . 'x');"
        );
    }

    /// The paren fix is only real if the emitted program COMPILES. Assert the
    /// whole generated program is paren-balanced, since an unbalanced program
    /// is a compile error that yields no stdout at all.
    #[test]
    fn pos_slice_generated_program_is_paren_balanced() {
        let src = "set -- \"$1\" \"$tempfile\" \"${@:3}\"\necho \"${@:1}\"\n";
        let out = gen(src);
        let mut depth: i64 = 0;
        for ch in out.chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            assert!(depth >= 0, "paren underflow in:\n{out}");
        }
        assert_eq!(depth, 0, "unbalanced parens in:\n{out}");
    }

    /// The file-scope binding must appear BEFORE the first read of
    /// `@__sh_args`, and both names must be declared for `use strict`.
    #[test]
    fn toplevel_positional_binding_precedes_its_reads() {
        let out = gen("echo $1\n");
        let decl = out.find("our @__sh_args;").expect("declared");
        let bind = out.find("local @__sh_args = @ARGV;").expect("bound");
        let read = out.find("$__sh_args[").expect("read");
        assert!(decl < bind && bind < read, "order wrong in:\n{out}");
        assert!(out.contains("our $__sh_args_n;"), "count declared in:\n{out}");
    }

    /// The count binding must be a name Perl can actually declare.
    ///
    /// `$#` is the length operator, not a sigil: `our $#foo;` and
    /// `our $#foo = 1;` are both compile errors. Emitting one put a syntax
    /// error in the preamble of every program with a top-level `$1`/`$@`,
    /// which compiled to nothing and produced zero output — so this pins
    /// the *name*, not merely the presence of a count, and additionally
    /// requires the emitted text to be real Perl.
    #[test]
    fn toplevel_pos_count_binding_is_valid_perl() {
        let out = gen("echo $1\n");
        assert!(
            !out.contains("$#__sh_args"),
            "undeclarable $# name re-emitted in:\n{out}"
        );
        // `perl -c` is the oracle: it is what actually rejected the old
        // spelling, and it rejects it without needing @ARGV to be set up.
        let dir = std::env::temp_dir().join(format!(
            "pg_toplevel_pos_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("t.pl");
        std::fs::write(&path, &out).expect("write");
        let chk = std::process::Command::new("perl")
            .arg("-c")
            .arg(&path)
            .output();
        let _ = std::fs::remove_dir_all(&dir);
        let chk = chk.expect("run perl -c");
        assert!(
            chk.status.success(),
            "emitted preamble is not valid Perl:\n{}\n---\n{out}",
            String::from_utf8_lossy(&chk.stderr)
        );
        // `perl -c` cannot catch a *warning*; redeclaring the names the
        // preamble already declared made every run print
        // `"our" variable @__sh_args redeclared` on stderr.
        assert!(
            !out.contains("our @__sh_args = @ARGV;") && !out.contains("our $__sh_args_n = scalar"),
            "binding re-declares the names TOPLEVEL_POS_DECLS already declared:\n{out}"
        );
    }

    /// The rewire must NOT touch `@ARGV` text that is DATA — a quoted
    /// literal, or the tail of a comment.
    #[test]
    fn toplevel_rewire_skips_strings_and_comments() {
        // A quoted `$ARGV[0]` is the string the script prints, not a read.
        let quoted = replace_all_outside_strings("print \"$ARGV[0]\";\n", "$ARGV[", "$__sh_args[");
        assert_eq!(quoted, "print \"$ARGV[0]\";\n", "string was rewritten");
        // The same in a heredoc-fed `print`.
        let sq = replace_all_outside_strings("print '$ARGV[0]';\n", "$ARGV[", "$__sh_args[");
        assert_eq!(sq, "print '$ARGV[0]';\n", "single-quoted string was rewritten");
        // A comment's text is not code.
        let comment = replace_all_outside_strings("x = 1;  # uses $ARGV[0] here\n", "$ARGV[", "$__sh_args[");
        assert_eq!(comment, "x = 1;  # uses $ARGV[0] here\n", "comment was rewritten");
        // Code outside those IS rewritten.
        let code = replace_all_outside_strings("y = $ARGV[0];\n", "$ARGV[", "$__sh_args[");
        assert_eq!(code, "y = $__sh_args[0];\n");
    }
    #[test]
    fn binding_injected_after_argv() {
        let body = "\n    local @ARGV = @_;\n    my $__argc = @ARGV;\n    print(1);\n";
        let out = inject_param_binding(body);
        assert!(out.contains("local @__sh_args = @_;\n"), "{out}");
        assert!(out.find("__sh_args").unwrap() < out.find("print(1)").unwrap(), "{out}");
    }

    /// Regression: the undeclared-variable pass must not declare Perl's `sort`
    /// package globals.
    ///
    /// The shared renderer lowers a pipeline's `sort` stage to
    /// `sort { $a cmp $b } @__fs`, where `$a` / `$b` are the two package
    /// globals Perl's `sort` sets for the comparison. The pass sees those
    /// reads, finds no `my` for them, and declared them itself — emitting
    /// `my $a;` / `my $b;` into the program's lexical scope. That shadows the
    /// globals for the whole file, so every `sort` then compared two `undef`s
    /// and returned its input in an arbitrary order:
    ///
    /// ```text
    /// bash:  a b c        perl:  c b a
    /// ```
    ///
    /// A `my $a` is worse than a missing declaration: it is not a warning, it
    /// is a silent wrong answer. So `$a` and `$b` are special names.
    #[test]
    fn sort_globals_are_never_declared_as_lexicals() {
        let src = "use strict;\nmy @__pl = ('c','b','a'); @__pl = sort { $a cmp $b } @__pl; print \"$_\\n\" for @__pl;\n";
        let out = declare_undeclared_vars(src);
        assert!(
            !out.contains("my $a;") && !out.contains("my $b;"),
            "declared a sort global as a lexical:\n{out}"
        );
    }

    /// The same rule, exercised the way it actually fires: through a real
    /// program's rendered text, where the sort block sits in the middle.
    #[test]
    fn sort_global_declaration_never_reaches_a_real_render() {
        let mut p = crate::Parser::new("printf \"c\\nb\\na\\n\" | sort\n");
        let cmds = p.parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        let out = generate(&prog);
        assert!(!out.contains("my $a;"), "{out}");
        assert!(!out.contains("my $b;"), "{out}");
    }

    /// And the pass must still do its job: an ordinary undeclared read is
    /// still declared, so the fix is not just "declare less".
    #[test]
    fn ordinary_undeclared_read_is_still_declared() {
        let src = "use strict;\nprint $never_declared_zz;\n";
        let out = declare_undeclared_vars(src);
        assert!(out.contains("my $never_declared_zz;"), "{out}");
    }

    /// A hash the renderer already declared must not be declared a second
    /// time by Fix 13. The second `my` is a binding, not a warning: it
    /// shadows the first from its own line on, so every earlier store into
    /// the hash becomes invisible to every later read. This is the shape
    /// `case "${words[$counter]}"` rendered to, and it made the case match
    /// nothing.
    #[test]
    fn a_declared_hash_is_not_declared_twice() {
        let src = "use strict;\nmy %words;\nmy $counter = '';\nif (($words{$counter}) =~ /x/) { print 1; }\n";
        let out = declare_undeclared_vars(src);
        assert_eq!(
            out.matches("my %words;").count(),
            1,
            "Fix 13 declared an already-declared hash again:\n{out}"
        );
    }

    /// The same, through a real render — the pass only sees this text after
    /// the shared renderer has hoisted its own declarations. Exactly ONE
    /// `my %words;` may exist: the renderer's. Before the fix there were two,
    /// and the second shadowed the first.
    #[test]
    fn hash_double_declaration_never_reaches_a_real_render() {
        let src = "case \"${words[$counter]}\" in\n  x) echo match ;;\nesac\n";
        let mut p = crate::Parser::new(src);
        let cmds = p.parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        let out = generate(&prog);
        assert_eq!(
            out.matches("my %words;").count(),
            1,
            "Fix 13 re-declared %words, shadowing the renderer's own:\n{out}"
        );
    }

    /// The `my $h = {%$x}` guard: the binding list binds `$h`, and treating
    /// `%$x` as a binding would mark `%x` declared and suppress the `my %x;`
    /// that `use strict` still needs.
    #[test]
    fn binding_list_first_token_is_not_a_hash_binding() {
        let src = "use strict;\nmy $h = {%$x};\nprint $x{0};\n";
        let out = declare_undeclared_vars(src);
        assert!(out.contains("my %x;"), "{out}");
    }

    /// Regression: a condition-position `redirect` / `block` that the shared
    /// renderer cannot rebuild must render as TRUE, not as a bare `0`.
    ///
    /// The shared renderer lowers a condition-position redirect to
    /// `(system('bash', '-c', q{…}) == 0)` -- the `== 0` is what converts a
    /// bash *status* into a Perl *boolean*. When the inner command cannot be
    /// rebuilt it falls back to the bare string `"0"`, which reads as a
    /// bash status but is FALSY in a Perl boolean slot. So a construct whose
    /// condition is genuinely truthy in bash came out false in Perl:
    ///
    /// ```sh
    /// while ! { : >> /tmp/f; } 2>/dev/null; do echo waiting; done   # bash: 0 iterations
    /// ```
    /// rendered as `while (!(0))`, and `!0` is TRUE in Perl -- the opposite
    /// of `!` on a zero status in bash. The loop ran forever and the gate
    /// killed it at the 20 s timeout (`perl=124`).
    ///
    /// A constant-condition loop is a hard hang, not a wrong value, so this
    /// is the most expensive shape in the whole failure set.
    #[test]
    fn unbuildable_condition_render_is_true_not_false() {
        let mut p = crate::Parser::new(
            "while ! { : >> /tmp/pg_bool_cond_test; } 2>/dev/null; do\n  echo waiting\ndone\n",
        );
        let cmds = p.parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        let out = generate(&prog);
        assert!(
            !out.contains(crate::ir::UNBUILDABLE_COND),
            "the unbuildable-condition marker survived into the output:\n{out}"
        );
        assert!(
            !out.contains("while (!(0))"),
            "condition slot holds a bare falsy 0:\n{out}"
        );
    }

    /// The same defect in the other arms that share it: an unbuildable
    /// condition-position `block` and an unbuildable `exec`-in-condition.
    /// Both must be TRUE, and both must be a Perl boolean rather than a
    /// status — `!`/`&&`/`||` on them have to read the way bash reads them.
    ///
    /// Each case is a CONTROL-FLOW head, which is the only place a condition
    /// boolean legitimately lives. A `(0)` in an arithmetic or list context is
    /// the number zero and must survive — that is what
    /// `a_bare_zero_outside_a_condition_is_untouched` pins.
    #[test]
    fn every_unbuildable_condition_arm_is_a_perl_boolean() {
        // Each arm emits `0/*sh2:unbuildable-cond*/`, which the pass must
        // turn into `1` in every control-flow head.
        for src in [
            "use strict;\nif (!(0/*sh2:unbuildable-cond*/)) { print \"x\\n\"; }\n",
            "use strict;\nif ((0/*sh2:unbuildable-cond*/)) { print \"x\\n\"; }\n",
            "use strict;\nwhile ((0/*sh2:unbuildable-cond*/)) { print \"x\\n\"; }\n",
            "use strict;\nelsif ((0/*sh2:unbuildable-cond*/)) { print \"x\\n\"; }\n",
        ] {
            let out = fix_condition_booleans(src);
            assert!(
                !out.contains(crate::ir::UNBUILDABLE_COND) && out.contains('1'),
                "the marker was not rewritten to a truthy boolean:\n{out}"
            );
        }
    }

    /// A legitimately computed `Bool(false)` renders as a bare `0` and must
    /// NOT be rewritten. This is the redirect-all regression: `if !
    /// command -v CMD > /dev/null` is TRUE in bash, the renderer emitted
    /// `!(0)`, and the old shape-matching rewrite turned it into `!(1)` and
    /// dropped the branch.
    #[test]
    fn a_computed_false_condition_is_not_inverted() {
        for src in [
            "use strict;\nif (!(0)) { print \"x\\n\"; }\n",
            "use strict;\nif ((0)) { print \"x\\n\"; }\n",
            "use strict;\nwhile ((0)) { print \"x\\n\"; }\n",
        ] {
            assert_eq!(
                fix_condition_booleans(src),
                src,
                "a computed falsy condition was rewritten:\n{src}"
            );
        }
    }

    /// The control that makes the fix above safe: a parenthesised zero that
    /// is NOT a condition head is the number zero and must be left alone.
    /// Without this, "find `(0)` and replace it" would silently corrupt
    /// arithmetic — and the renderer emits `(0)` in plenty of real arithmetic.
    #[test]
    fn a_bare_zero_outside_a_condition_is_untouched() {
        for src in [
            "use strict;\nmy $n = (0);\n",
            "use strict;\nprint((0) + 1, \"\\n\");\n",
            "use strict;\nmy @l = (0, 1, 2);\n",
            "use strict;\nif ((system('bash', '-c', 'exit 1') == 0)) { print \"y\\n\"; }\n",
        ] {
            assert_eq!(fix_condition_booleans(src), src, "clobbered a real zero:\n{src}");
        }
    }

    /// And the control: a REAL zero-status condition must stay false, or the
    /// fix would invert a correct program. The renderer writes
    /// `(system('bash', '-c', q{false}) == 0)`, which is already a boolean
    /// and must be left alone.
    #[test]
    fn a_real_status_comparison_is_left_alone() {
        let src = "use strict;\nif ((system('bash', '-c', 'exit 1') == 0)) { print \"y\\n\"; }\n";
        assert_eq!(fix_condition_booleans(src), src);
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

    // ── Fix 11 — the `for`-over-array iterand ───────────────────────────

    #[test]
    fn for_array_iterand_unwraps_the_join() {
        // The exact shape the renderer emits for `for num in "${numbers[@]}"`.
        let out = fix_for_array_iterand("for my $__num ((join(' ', @numbers))) {\n");
        assert_eq!(out, "for my $__num (@numbers) {\n");
    }

    #[test]
    fn for_array_iterand_keeps_each_element_separate() {
        // The behavior the unwrap buys: the loop body must run once per
        // element, not once with the joined blob. This is the regression the
        // 058 failure was — 8 expected lines, 0 produced.
        let out = fix_for_array_iterand("for my $__num ((join(' ', @numbers))) {\n");
        assert!(out.contains("@numbers"), "iterand lost the array: {out}");
        assert!(!out.contains("join"), "the collapsing join survived: {out}");
        // Exactly one paren pair remains: the list context the loop needs.
        assert_eq!(out.matches('(').count(), 1, "iterand not in list context: {out}");
    }

    #[test]
    fn for_array_iterand_leaves_a_capture_to_fix_10() {
        // A command capture is the OTHER shape; it must pass through
        // untouched so Fix 10 (which unwraps the `split` instead) owns it.
        let src = "for my $__f ((join(' ', split(/\\s+/, __sh_pipe_capture(q('cmd'))))) {\n";
        assert_eq!(fix_for_array_iterand(src), src);
    }

    // ── Fix 20 — the GLOB sentinel in a `for` iterand ──────────────────

    #[test]
    fn glob_sentinel_in_for_iterand_becomes_an_expansion() {
        // The exact shape `render_brace_iter` emits for `for f in *.{txt,log,dat}`.
        // The sentinel is user-VISIBLE: the case arms below cannot match it, so
        // the loop printed the marker itself. The fix must remove every byte of
        // it, not just re-quote it.
        let src = "for my $__f (('\u{1}SH2GLOB\u{1}*.txt', '\u{1}SH2GLOB\u{1}*.log')) {\n";
        let out = fix_glob_sentinel_in_for_iterand(src);
        assert!(
            !out.contains("SH2GLOB"),
            "the sentinel survived into the emitted program: {out:?}"
        );
        assert!(
            !out.contains('\u{1}'),
            "the magic bytes survived into the emitted program: {out:?}"
        );
        assert_eq!(
            out,
            "for my $__f ((glob('*.txt'), glob('*.log'))) {\n"
        );
        // The element's OWN quotes must be consumed too. A previous version
        // kept the opening quote and emitted `('glob('*.txt')', …)`: the glob
        // then matched nothing and the loop silently printed nothing, while
        // this same assertion still passed because the expected string was
        // never compared against the real element shape. Assert the exact
        // bytes, and assert no nested-quote string survived.
        assert!(
            !out.contains("'glob("),
            "the element quotes survived around the glob call: {out:?}"
        );
    }

    #[test]
    fn glob_sentinel_fix_leaves_an_untagged_list_alone() {
        // A plain literal list is not ours: every element stays byte-identical
        // so an ordinary `for x in a b c` is byte-for-byte unchanged.
        let src = "for my $__x (('a', 'b', 'c')) {\n";
        assert_eq!(fix_glob_sentinel_in_for_iterand(src), src);
    }

    #[test]
    fn glob_sentinel_fix_leaves_a_non_for_line_alone() {
        // The pass is anchored on the `for my $` header. The same sentinel in
        // another position belongs to whichever fix owns that position.
        let src = "my @x = ('\u{1}SH2GLOB\u{1}*.txt');\n";
        assert_eq!(fix_glob_sentinel_in_for_iterand(src), src);
    }

    #[test]
    fn for_array_iterand_leaves_other_iterands_alone() {
        // A literal list and a range are not this fix's business.
        for src in [
            "for my $__i (('0', '1', '2')) {\n",
            "for my $n (1..5) {\n",
            "for my $x (@{$ref}) {\n",
            "for my $y (@arr[0..2]) {\n",
        ] {
            assert_eq!(fix_for_array_iterand(src), src, "over-rewrote: {src:?}");
        }
    }

    #[test]
    fn for_array_iterand_is_identity_without_a_needle() {
        let src = "#!/usr/bin/env perl\nuse strict;\nfor my $i (1..3) { print $i; }\n";
        assert_eq!(fix_for_array_iterand(src), src);
    }
}

// ── Fix 9 — the materialised process-substitution consumer keeps its
// OWN status ──────────────────────────────────────────────────────────
//
// `transforms::process_subst` materialises `<(…)` / `>(…)` into a
// sequential block:
//
//     Block[ __ps_tmpN = $(mktemp),  { producer } > __ps_tmpN,
//            … ,  <the consumer>,  … ,  rm -f __ps_tmpN… ]
//
// The CONSUMER is the statement whose `$?` bash reports: in
// `cmp -s <(echo b) <(echo c); echo "exit: $?"` it is `cmp`, and bash
// prints 1 because `b` and `c` differ.
//
// The trailing cleanup `rm` is the block's LAST statement, so the status
// the consumer left behind is overwritten before anything reads it. Every
// statement-emitter in the shared renderer closes with
// `$main_exit_code = $CHILD_ERROR = …`, so `rm` (success) clobbered the
// consumer's status with 0 and `echo "$?"` printed `exit: 0` — a
// correctness bug in every materialised process substitution that is
// followed by a `$?` read (070_cmp_basic, 012/042_process_substitution).
//
// `IrStmt` has no "preserve the status" node and the renderer owns both
// decisions, so the correction is structural and IR-level: when the
// consumer's OWN exit status is read before the block ends, carry it in a
// generated variable and restore it from the cleanup statement, which is
// the exact value the consumer produced. With no such read the block is
// left untouched — restoring a status nobody reads can only cost.
//
// Scope, per this module's rule:
//   * a top-level `Block` whose shape is the materialisation above
//     (mktemp assignments, producer redirects, exactly ONE bare
//     command-statement consumer, and a trailing `rm -f` of the same
//     `__ps_tmpN` names);
//   * a subsequent read of `$?` — the IR's `Call { func: "getVar",
//     args: [Str("?")] }` — before the enclosing statement list ends. A
//     read inside a LATER block's condition is deliberately NOT counted:
//     that is an `&&`/`||` short-circuit branch, where a status written
//     inside a branch would leak out of it.
fn fix_ps_status_blocks(stmts: &[IrStmt]) -> Vec<IrStmt> {
    // A `__ps_tmp` name only ever comes from the materialisation, so its
    // presence in a name is proof the shape came from there.
    fn is_ps_tmp(w: &IrExpr) -> bool {
        matches!(w, IrExpr::Var(n, _) if n.starts_with("__ps_tmp"))
    }
    fn is_plain_consumer(s: &IrStmt) -> bool {
        matches!(
            s,
            IrStmt::Exec { .. } | IrStmt::Expr(IrExpr::Call { .. })
        )
    }
    fn expr_reads_exit_status(e: &IrExpr) -> bool {
        match e {
            IrExpr::Call { func, args } if func == "getVar" => {
                matches!(args.first(), Some(IrExpr::Str(v, _)) if v == "?")
            }
            IrExpr::Call { args, .. } => args.iter().any(expr_reads_exit_status),
            IrExpr::Index { key, .. } => expr_reads_exit_status(key),
            IrExpr::BinOp { lhs, rhs, .. } => {
                expr_reads_exit_status(lhs) || expr_reads_exit_status(rhs)
            }
            IrExpr::Array(es) => es.iter().any(expr_reads_exit_status),
            IrExpr::Arrow(stmts) => stmts.iter().any(stmt_reads_exit_status),
            IrExpr::ArrayComp { iter, elem, cond, .. } => {
                expr_reads_exit_status(iter)
                    || expr_reads_exit_status(elem)
                    || cond
                        .as_deref()
                        .is_some_and(expr_reads_exit_status)
            }
            IrExpr::Lambda { body, .. } => body.iter().any(stmt_reads_exit_status),
            IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
                InterpPart::Expr(x) => expr_reads_exit_status(x),
                _ => false,
            }),
            IrExpr::Ternary { cond, then, else_ } => {
                expr_reads_exit_status(cond)
                    || expr_reads_exit_status(then)
                    || expr_reads_exit_status(else_)
            }
            IrExpr::DefinedOr { expr, default } => {
                expr_reads_exit_status(expr) || expr_reads_exit_status(default)
            }
            _ => false,
        }
    }
    fn stmt_reads_exit_status(s: &IrStmt) -> bool {
        match s {
            IrStmt::Expr(e) => expr_reads_exit_status(e),
            IrStmt::Assign { expr, .. } => expr_reads_exit_status(expr),
            IrStmt::Return(Some(e)) => expr_reads_exit_status(e),
            IrStmt::Redirect { inner, .. } => inner.iter().any(stmt_reads_exit_status),
            IrStmt::If { cond, then, .. } => {
                expr_reads_exit_status(cond) || then.iter().any(stmt_reads_exit_status)
            }
            // A loop's body status is not the loop's status; only its
            // condition reads `$?` in a way a later read can observe.
            IrStmt::While { cond, .. } | IrStmt::DoWhile { cond, .. } => {
                expr_reads_exit_status(cond)
            }
            IrStmt::ForInit { cond, init, step, body } => {
                expr_reads_exit_status(cond)
                    || init.iter().any(stmt_reads_exit_status)
                    || step.iter().any(stmt_reads_exit_status)
                    || body.iter().any(stmt_reads_exit_status)
            }
            // A subshell runs its body out of process: nothing it writes is
            // observable here, and walking in could rename a temp in a
            // scope the caller cannot see.
            IrStmt::Block(_) | IrStmt::Subshell(_) | IrStmt::Background(_) => false,
            _ => false,
        }
    }
    /// The `rm -f tmp…` cleanup, and the exact temp set it removes.
    fn rm_cleanup(s: &IrStmt) -> Option<Vec<String>> {
        let IrStmt::Exec { cmd, args, .. } = s else {
            return None;
        };
        if !matches!(cmd, IrExpr::Str(c, _) if c == "rm") {
            return None;
        }
        match args.split_first() {
            Some((IrExpr::Str(flag, _), rest)) if flag == "-f" => {
                let mut tmps: Vec<String> = Vec::new();
                for w in rest {
                    match w {
                        IrExpr::Var(n, _) if n.starts_with("__ps_tmp") => tmps.push(n.clone()),
                        _ => return None,
                    }
                }
                if tmps.is_empty() {
                    None
                } else {
                    Some(tmps)
                }
            }
            _ => None,
        }
    }

    let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
    // A block's status is only observable when the VERY NEXT statement in
    // this list reads `$?`. Any other command in between sets `$?` again,
    // so a read further along observes that command, not this block — and
    // with no read at all, restoring the status could only cost.
    for (i, s) in stmts.iter().enumerate() {
        let read_next = stmts
            .get(i + 1)
            .is_some_and(|next| stmt_reads_exit_status(next));
        if let IrStmt::Block(body) = s {
            if read_next {
                if let Some((start, tmps, saved)) =
                    ps_consumer_position(body, &rm_cleanup, &is_ps_tmp)
                {
                    let mut rewritten = body.clone();
                    // INSERT the capture AFTER the consumer, never over it.
                    // Assigning `rewritten[start] = save_status(..)` replaced
                    // the consumer statement itself, so the whole command
                    // vanished from the emitted program: `cmp -s <(echo b)
                    // <(echo c); echo "$?"` rendered the two producers, the
                    // cleanup, and the echo — and NO `cmp` — so `$?` could
                    // only ever be the producer's status, and `cmp`'s own
                    // stdout (the GNU "differ: byte N" lines) was lost
                    // entirely (070_cmp_basic, process-substitution.sh,
                    // 040_process_substitution_comm).
                    rewritten.insert(start + 1, save_status(saved.clone()));
                    // …so the cleanup index shifted by one with it.
                    let cleanup = rewritten.len() - 1;
                    rewritten[cleanup] = restore_status(rewritten[cleanup].clone(), tmps, saved);
                    out.push(IrStmt::Block(rewritten));
                    continue;
                }
            }
        }
        out.push(s.clone());
    }
    out
}

/// Locate the consumer inside a materialised block: returns
/// `(index, temps, fresh-variable-name)`, or `None` when this block is not
/// the materialisation shape at all.
fn ps_consumer_position(
    body: &[IrStmt],
    rm_cleanup: &dyn Fn(&IrStmt) -> Option<Vec<String>>,
    is_ps_tmp: &dyn Fn(&IrExpr) -> bool,
) -> Option<(usize, Vec<String>, String)> {
    let is_plain_consumer = |s: &IrStmt| {
        matches!(s, IrStmt::Exec { .. } | IrStmt::Expr(IrExpr::Call { .. }))
    };
    let rm_at = body.len().checked_sub(1)?;
    let tmps = rm_cleanup(&body[rm_at])?;
    // Every temp must have been created by a `mktemp` assignment in this
    // same block — otherwise these names belong to something else.
    for t in &tmps {
        let created = body.iter().take(rm_at).any(|s| {
            matches!(s, IrStmt::Assign { targets, .. }
                if targets.iter().any(|tt| &tt.var == t))
        });
        if !created {
            return None;
        }
    }
    let consumer_at = body
        .iter()
        .enumerate()
        .take(rm_at)
        .filter(|(_, s)| is_plain_consumer(s))
        .map(|(i, _)| i)
        .last()?;
    // Exactly ONE consumer: two bare commands means we cannot tell which
    // one's status bash reports, and guessing is worse than the status quo.
    let consumers = body
        .iter()
        .take(rm_at)
        .filter(|s| is_plain_consumer(s))
        .count();
    if consumers != 1 {
        return None;
    }
    let saved = fresh_status_var(body, &tmps);
    Some((consumer_at, tmps, saved))
}

/// A variable name no statement in the block can already be using.
fn fresh_status_var(body: &[IrStmt], tmps: &[String]) -> String {
    let mut n = 0usize;
    loop {
        let cand = format!("__ps_status{n}");
        let taken = body.iter().any(|s| mentions_var(s, &cand))
            || tmps.iter().any(|t| t == &cand);
        if !taken {
            return cand;
        }
        n += 1;
    }
}

/// Every statement the name could collide with: the bodies recurse into,
/// and the redirect targets / command words a materialisation block uses.
fn mentions_var(s: &IrStmt, name: &str) -> bool {
    match s {
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
            b.iter().any(|x| mentions_var(x, name))
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            mentions_expr(cond, name)
                || then.iter().any(|x| mentions_var(x, name))
                || elsifs
                    .iter()
                    .any(|(c, b)| {
                        mentions_expr(c, name) || b.iter().any(|x| mentions_var(x, name))
                    })
                || else_.iter().any(|x| mentions_var(x, name))
        }
        IrStmt::While { cond, body } | IrStmt::DoWhile { cond, body, .. } => {
            mentions_expr(cond, name) || body.iter().any(|x| mentions_var(x, name))
        }
        IrStmt::For { .. } => false,
        IrStmt::Function { body, .. } => body.iter().any(|x| mentions_var(x, name)),
        IrStmt::Redirect { inner, redirects } => {
            inner.iter().any(|x| mentions_var(x, name))
                || redirects.iter().any(|r| mentions_expr(&r.target, name))
        }
        IrStmt::Assign { targets, expr, .. } => {
            targets.iter().any(|t| t.var == name) || mentions_expr(expr, name)
        }
        IrStmt::Exec { cmd, args, redirects, .. } => {
            mentions_expr(cmd, name)
                || args.iter().any(|a| mentions_expr(a, name))
                || redirects.iter().any(|r| mentions_expr(r, name))
        }
        IrStmt::Expr(e) => mentions_expr(e, name),
        IrStmt::Return(Some(e)) => mentions_expr(e, name),
        _ => false,
    }
}

fn mentions_expr(e: &IrExpr, name: &str) -> bool {
    match e {
        IrExpr::Var(n, _) => n == name,
        IrExpr::Index { key, .. } => mentions_expr(key, name),
        IrExpr::Call { args, .. } | IrExpr::MethodCall { args, .. } => {
            args.iter().any(|a| mentions_expr(a, name))
        }
        IrExpr::Array(es) => es.iter().any(|a| mentions_expr(a, name)),
        IrExpr::Arrow(stmts) | IrExpr::Lambda { body: stmts, .. } => {
            stmts.iter().any(|x| mentions_var(x, name))
        }
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Expr(x) => mentions_expr(x, name),
            _ => false,
        }),
        IrExpr::BinOp { lhs, rhs, .. } => mentions_expr(lhs, name) || mentions_expr(rhs, name),
        IrExpr::Ternary { cond, then, else_ } => {
            mentions_expr(cond, name)
                || mentions_expr(then, name)
                || mentions_expr(else_, name)
        }
        IrExpr::DefinedOr { expr, default } => {
            mentions_expr(expr, name) || mentions_expr(default, name)
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            mentions_expr(iter, name)
                || mentions_expr(elem, name)
                || cond.as_deref().is_some_and(|c| mentions_expr(c, name))
        }
        _ => false,
    }
}

/// `<consumer>; <saved> = $CHILD_ERROR` — capture the status the consumer
/// just produced, before anything else overwrites it.
///
/// `$CHILD_ERROR` (not `$main_exit_code`) because that is the name the
/// renderer's own `$?` read resolves to (`getVar("?")` → `$CHILD_ERROR`).
///
/// The VALUE spells the variable WITHOUT a leading `$`: the renderer writes a
/// var as `${name}` (ir.rs `ir_expr_to_perl`), so the name `"$CHILD_ERROR"`
/// came out as `$$CHILD_ERROR` — Perl's PID, not the status. `diff <(echo
/// one) <(echo two)` then printed `exit: 24653` (process-substitution.sh),
/// and every `$( … )`/`<( … )` consumer's status was silently the pid.
fn save_status(saved: String) -> IrStmt {
    IrStmt::Assign {
        targets: vec![crate::ir::AssignTarget {
            var: saved,
            sigil: None,
            indices: vec![],
        }],
        expr: IrExpr::Var("CHILD_ERROR".to_string(), None),
        asm: None,
    }
}

/// `<cleanup>; $CHILD_ERROR = <saved>` — restore the status the consumer
/// produced, which is the value bash's `$?` holds once the whole
/// substitution has run.
///
/// `SetChildError`, not an `Assign`: an `Assign` to the name `$CHILD_ERROR`
/// declares a SECOND lexical (`my $$CHILD_ERROR`) that MASKS the preamble's
/// `our $CHILD_ERROR`. Under `use strict` that is a fatal compile error, so
/// the whole program produced no output at all — a far worse failure than
/// the clobbered status. `IrStmt::SetChildError` is the IR's own spelling
/// of "write the child's exit status" and renders as a plain `$CHILD_ERROR`
/// assignment.
fn restore_status(cleanup: IrStmt, _tmps: Vec<String>, saved: String) -> IrStmt {
    IrStmt::Block(vec![
        cleanup,
        IrStmt::SetChildError(IrExpr::Var(saved, None)),
    ])
}








