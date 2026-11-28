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
    generate_impl(prog, None)
}

/// One named text pass: `(name, fn)`. The name exists so the audit test can
/// disable a pass by name and prove it is load-bearing (or dead).
pub(crate) type TextPass = (&'static str, fn(&str) -> String);

/// The passes over the RENDERED program, in order.
///
/// Each one is named in `post_pass` where it runs; the audit test
/// (`audit_each_text_pass`, `cargo test --lib audit_each_text_pass --
/// --ignored --nocapture`) renders the whole corpus once per pass with that
/// pass disabled and reports which renders change.
fn text_passes() -> &'static [TextPass] {
    &[
        ("fix_printf_bash_conversions", fix_printf_bash_conversions),
        ("fix_printf_b_percent_b", fix_printf_b_percent_b),
        ("fix_print_newline_to_say", fix_print_newline_to_say),
        ("fix_print_concat_interp", fix_print_concat_interp),
        ("fix_literal_pipeline_pipestatus", fix_literal_pipeline_pipestatus),
    ]
}

/// The DECLARATION passes (the missing `our`/`my` names). They run after the
/// text passes and are what `ir::shir_to_perl` also calls at the end of the
/// bare path, so both entry points emit strict-clean Perl.
fn declaration_passes() -> &'static [TextPass] {
    &[
        ("declare_undeclared_vars", declare_undeclared_vars),
        ("declare_arith_operand_flag", declare_arith_operand_flag),
    ]
}

/// `generate`, optionally with one text pass disabled. Only the audit test
/// (and a pass's own pinning test) has business passing `Some`.
pub(crate) fn generate_impl(prog: &IrProgram, skip_pass: Option<&str>) -> String {
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
    // Scalar-phase names (env-style vars with a plain assignment) store
    // in perl scalars and only mirror into %ENV when already exported
    // (ir.rs::scalar_phase_var — Bug A: `SHELL_VAR=\"…\"` used to leak
    // into children before `export SHELL_VAR` ran). The set must be
    // installed before ANY rendering — lhs routing, reads and the
    // export arm all consult it.
    let mut scalar_phase = std::collections::HashSet::new();
    crate::ir::collect_scalar_phase_vars(&prog.stmts, &mut scalar_phase);
    crate::ir::set_scalar_phase_vars(scalar_phase);
    let fixed = fixups(prog);
    // A program that reads `$?` renders that read through the
    // `$CHILD_ERROR` mirror; a program that MOVES the status through a
    // variable (`$__ps_statusN = $CHILD_ERROR`) needs the mirror kept in
    // step by every register write. Everything else reads the register
    // directly and gets the clean form.
    // Does the SOURCE test a status (`if cmd; then`, `[ "$?" = 0 ]`)? Then
    // the register is read directly and must not be clobbered by a body
    // statement's write.
    let read_carrier = crate::shir::ir_stmts_read_status(&fixed.stmts)
        || fixed
            .subs
            .iter()
            .any(|sb| crate::shir::ir_stmts_read_status(&sb.body));
    // The verdict cannot come from the IR alone: the process-substitution
    // materialisation is a TEXT template, so its `$__ps_statusN =
    // $CHILD_ERROR` save exists only after rendering. Render once with the
    // mirror OFF, and if the body turns out to move the value through
    // `$CHILD_ERROR`, render again with the mirror kept in step.
    crate::ir::set_childe_error_carrier(false);
    let src = shir_to_perl(&fixed);
    // A condition that runs a command (`if ps …; then`) READS the status
    // through the register: the test's own `system()` set it, and the
    // `$? = …` written by a body statement would clobber it.
    let src = if src.contains("$CHILD_ERROR") || src.contains("($? >> 8)") && !read_carrier {
        crate::ir::set_childe_error_carrier(true);
        let again = shir_to_perl(&fixed);
        crate::ir::set_childe_error_carrier(false);
        again
    } else {
        src
    };
    if std::env::var_os("SH2_DEBUG_PGFIX").is_some() {
        eprintln!("PGFIX-RENDERED:\n{src}");
    }
    let src = post_pass_without(&src, skip_pass);
    if std::env::var_os("SH2_DEBUG_PGFIX").is_some() {
        eprintln!("PGFIX-POSTPASS:\n{src}");
    }
    // The shared renderer emits only the helpers it knows about, so the
    // `__sh_fn_status` helper `fix_cond` introduces is injected here. Gated on
    // the rendered body, same discipline as the renderer's own preamble
    // vars: a program that never calls it gets no dead sub.
    // Every runtime helper the SHARED renderer calls is defined here, in ONE
    // place both entry points call: `perl_generator_fixed::generate` and
    // `ir::shir_to_perl` (which used to emit the CALLS with no definition,
    // so the bare render died with `Undefined subroutine &main::__sh_glob`
    // and friends). Idempotent: a helper whose `sub` line is already present
    // is left alone, so the fixed path's own call is a no-op.
    let mut src = inject_renderer_helpers(src);
    // The test-lowering transform's affix primitives: `contains` has a
    // native arm in ir_expr_to_perl (`index >= 0`) but
    // `strHasPrefix`/`strHasSuffix` had NONE, so a program whose
    // status-unobserved test chain lifted an affix test called an
    // UNDEFINED SUB at runtime — measured: `[[ "$s" != "abc"* ]]` →
    // `Undefined subroutine &main::strHasPrefix called` (pre-existing:
    // no corpus example lifts an affix test in an unobserved-status
    // cond, so no gate ever hit the path — probeC in the 2026-10-20
    // callout session did). Same injection discipline as above.
    // Fix 26 turns `print(X, "\n")` into `say X`. The shared renderer adds
    // `use feature 'say'` when IT emits a `say`; a program whose output was
    // all explicit-newline prints gets no such import, and `say` is then a
    // `strict subs` compile error. Add it when the body says `say` and the
    // preamble does not.
    if src.contains("say ") && !src.contains("use feature 'say'") {
        src = inject_before_first_stmt(&src, "use feature 'say';\n");
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
    post_pass_without(src, None)
}

/// The rendered-text pipeline. `skip` names one pass to leave out (audit /
/// pinning tests only). Order matters in two places, recorded at the passes
/// themselves: the output idioms must run before `fix_pos_param_slice` (which
/// can reformat a statement tail and leave `print($x, "\n")` untouched), and
/// `fix_literal_pipeline_pipestatus` must run before the declaration passes
/// (which pick its `@PIPESTATUS` write up as a reference).
fn post_pass_without(src: &str, skip: Option<&str>) -> String {
    // Debug/audit seam: `SH2_SKIP_TEXT_PASS=<name>` leaves one pass out so a
    // fresh process can render the comparison (the in-process audit cannot use
    // it for a program whose render depends on renderer-internal state).
    let env_skip = std::env::var("SH2_SKIP_TEXT_PASS").ok();
    let skip = skip.or(env_skip.as_deref());
    let mut out = src.to_string();
    for (name, pass) in text_passes().iter().chain(declaration_passes().iter()) {
        if Some(*name) == skip {
            continue;
        }
        out = pass(&out);
    }
    out
}

/// The DECLARATION half of `post_pass`: the renderer-owned flags and the
/// names the program READS but never binds. Shared with the bare entry point
/// (`ir::shir_to_perl` calls it last) so both paths emit strict-clean Perl —
/// the bare path used to leave `$L`, `${PIPESTATUS[@]}` and `$__SH2_AE`
/// undeclared, a hard compile error that killed the whole file.
pub(crate) fn declare_renderer_symbols(src: &str) -> String {
    // Same audit seam as `post_pass_without`: the bare entry point
    // (`ir::shir_to_perl`) runs these passes too, so a declaration pass's
    // liveness must be measured on BOTH entry points.
    let env_skip = std::env::var("SH2_SKIP_TEXT_PASS").ok();
    let mut out = src.to_string();
    for (name, pass) in declaration_passes() {
        if Some(*name) == env_skip.as_deref() {
            continue;
        }
        out = pass(&out);
    }
    out
}


/// `$__SH2_AE` — the empty-operand flag `arith_str_to_perl` writes inside
/// expression positions (`… && ($__SH2_AE = 1)`), which
/// `declare_undeclared_vars` deliberately skips: `__`-prefixed names are
/// renderer-owned and hoisted by the renderer itself (the same rule as
/// `TOPLEVEL_POS_DECLS` for `@__sh_args`). Without this `our`, `use
/// strict` rejects the flag write and the WHOLE file fails to compile.
fn declare_arith_operand_flag(src: &str) -> String {
    // Mission item 5 — why this is still a TEXT pass.
    // Declaration-only, and for a renderer-owned name: `$__SH2_AE` is written
    // by the arithmetic lowering deep inside expression rendering, which has
    // no program-level declaration hook. Idempotent, so the fixed path's
    // later call is a no-op. Pinned by
    // `retained_pass_tests::arith_operand_flag_is_declared`.
    if !src.contains("$__SH2_AE") || src.contains("our $__SH2_AE") {
        return src.to_string();
    }
    inject_before_first_stmt(src, "our $__SH2_AE;\n")
}
// (declare_renderer_symbols lives below `post_pass`.)

/// `false | true` (any all-literal-`true`/`false` pipeline) followed by a
/// read of `${PIPESTATUS[n]}`: populate the array before the run.
///
/// `__sh_pipe_run` reports only the pipeline's final status, so
/// `@PIPESTATUS` stayed EMPTY and `echo "ps=${PIPESTATUS[0]},${PIPESTATUS[1]}"`
/// printed `ps=,` where bash prints `ps=1,0` (070_gnuisms_thorough). When
/// every stage is the literal builtin `true` or `false`, bash's statuses are
/// known statically (false→1, true→0), so prepend the exact array before the
/// statement. Any other stage leaves the program untouched — guessing would
/// be wrong, and the empty array is at least honest.
///
/// Runs BEFORE `declare_undeclared_vars`, which picks the write up as a
/// referenced `@PIPESTATUS` and declares it when the script has no read.
fn fix_literal_pipeline_pipestatus(src: &str) -> String {
    // Mission item 5 — why this is still a TEXT pass.
    // bash records one status per pipeline STAGE; `__sh_pipe_run` reports only
    // the final one. When every stage is the literal `true`/`false` builtin
    // the statuses are static, so the exact array is spliced in front of the
    // run. The stage list exists in rendered form only (the pipeline is an
    // opaque `q{...}` payload), and guessing for any other stage would be
    // wrong, so the pass refuses everything else. Pinned by
    // `retained_pass_tests::literal_pipeline_fills_pipestatus`.
    // Both statement forms: the tracked one (`$main_exit_code =
    // $CHILD_ERROR = __sh_pipe_run(q{…`) and the status-unread elision
    // (`__sh_pipe_run(q{…` at statement start — the rc write was
    // dropped, but the PIPESTATUS splice must still happen: 070_gnuisms
    // printed `ps=,` when the anchor missed the elided form).
    // The tracked forms: the register spelling the renderer now emits
    // (`$? = (__sh_pipe_run(…) << 8;`) and the older tracker prefix, kept so
    // the pass still fires on either shape.
    const ANCHORS: [&str; 3] = [
        "$? = (__sh_pipe_run(q{",
        "$? = __sh_pipe_run(q{",
        "$main_exit_code = $CHILD_ERROR = __sh_pipe_run(q{",
    ];
    const ANCHOR: &str = ANCHORS[2];
    const ANCHOR_ELV: &str = "__sh_pipe_run(q{";
    fn find_anchor(s: &str) -> Option<usize> {
        let tracked = ANCHORS.iter().filter_map(|a| s.find(a)).min();
        let elided = s.find(ANCHOR_ELV).filter(|i| {
            // elided form sits at a statement start: the rest of its line
            // (up to the previous newline) is only indent. The tracked
            // form has `$main_exit_code…` before it on the same line and
            // is correctly rejected here, then found by `tracked`.
            s[..*i].rsplit('\n').next().map_or(true, |l| l.trim().is_empty())
        });
        match (tracked, elided) {
            (Some(t), Some(e)) => Some(t.min(e)),
            (t, e) => t.or(e),
        }
    }
    let mut out = String::with_capacity(src.len() + 64);
    let mut rest = src;
    while let Some(at) = find_anchor(rest) {
        // The q{ payload starts at the end of whichever anchor matched.
        let inner_start = match ANCHORS.iter().find(|a| rest[at..].starts_with(**a)) {
            Some(a) => at + a.len(),
            None => at + ANCHOR_ELV.len(),
        };
        // Stages with no `}` in them (the literal true/false forms have no
        // braces) — anything else bails out of this occurrence.
        let Some(close_rel) = rest[inner_start..].find('}') else {
            out.push_str(rest);
            return out;
        };
        let inner = &rest[inner_start..inner_start + close_rel];
        let statuses = literal_true_false_statuses(inner);
        match statuses {
            Some(list) => {
                // Splice the array assignment in front of the statement,
                // then copy the statement itself and step past it.
                out.push_str(&rest[..at]);
                out.push_str("@PIPESTATUS = (");
                out.push_str(&list.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", "));
                out.push_str("); ");
                out.push_str(&rest[at..inner_start + close_rel]);
                rest = &rest[inner_start + close_rel..];
            }
            None => {
                out.push_str(&rest[..inner_start + close_rel]);
                rest = &rest[inner_start + close_rel..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `('true' | 'false') (| ('true' | 'false'))*` → the statuses bash would
/// record. Anything else → None.
fn literal_true_false_statuses(inner: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for stage in inner.split('|') {
        match stage.trim() {
            "'true'" => out.push(0),
            "'false'" => out.push(1),
            _ => return None,
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

// ── Top-level positional parameters (mission item 5: NOT a text fixup) ──
//
// Both scopes are the IR's now. A sub's `$1` is its own parameter: the
// Function arm emits `local @__sh_args = @_;` and spells the reads
// `$__sh_args[0]`.. . At file scope `@__sh_args` is bound to `@ARGV` by the
// IR's own preamble, because `do FILE` (the CLI's `./fail` contract) runs
// the file with `@ARGV` EMPTIED while bash reads a missing `$1` as the
// empty string — `$ARGV[0]` there was `undef` plus a warning
// (parse-at-slice-param.sh).
//
// This module used to do both by REWRITING the rendered text (`$1` →
// `$__sh_args[0]`, `$ARGV[` → `$__sh_args[`, plus a spliced binding). That
// is exactly the seam that corrupted generated code resembling a positional
// reference — the printf numeric helper's `$1..$4` regex captures — so the
// rewrites are gone and `ir.rs` emits the names directly. Pinned by
// `positional_text_tests` (below) and
// `tests/tier1/positional_text_is_not_rewritten.sh`.







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
    // Unquoting only makes sense when a QUOTED ARGUMENT follows in the same
    // payload (`'eval' "echo …"`). When the name is the whole payload
    // (`system('bash', '-c', q{'true'})`) or is concatenated with an
    // operator (`'true' . $x`), stripping the quotes leaves a bareword and
    // perl dies under `use strict` — measured on
    // `if if false; then :; fi; then …`.
    let after = &rest[consumed..];
    let arg = after.trim_start();
    if !after.starts_with(char::is_whitespace) || !(arg.starts_with('\'') || arg.starts_with('"')) {
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
pub(crate) fn is_plain_command_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Shell builtins whose names must reach `bash -c` unquoted. Mirrors
/// `ir.rs::is_shell_builtin`, which refuses the same names for the uu-ffi seam.
pub(crate) fn is_shell_builtin_name(name: &str) -> bool {
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
    // Mission item 5 — why this is still a TEXT pass.
    // The rewrite needs to know which argument of an emitted `printf(...)`
    // call is the FORMAT (bash's `%q` is a conversion, a `%q` inside an
    // operand is data). That fact exists only in the rendered call: the IR's
    // printf node carries a bash format string, and the Perl lowering turns
    // it into a sprintf format, so `%q` is not representable per-node without
    // teaching the printf emitter bash's whole conversion set. The pass is
    // anchored on the FORMAT position and leaves operands alone; pinned by
    // `retained_pass_tests::printf_percent_q_uses_the_shell_quoting_helper`.
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
    // Mission item 5 — why this is still a TEXT pass.
    // Same seam as `%q`: bash's `%b` expands escapes in the OPERAND, which
    // only the rendered call identifies. Pinned by
    // `retained_pass_tests::printf_percent_b_expands_the_operand`.
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
                                  : $esc eq 'r' ? "\r" : $esc eq 't' ? "\t" : "\x0b") }
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


// ── Fix 25 — `print($a . " " . $b)` reads better as one interpolated string ─
//
// The shared renderer joins `echo`-style words with Perl's concat operator:
//
//     print(($a) . " " . ($b) . " ");      print(($k) . " => " . ($v));
//
// A Perl programmer writes the same statement as a single interpolated
// string, which is easier to read and to diff:
//
//     print "$a $b ";                      print "$k => $v";
//
// The rewrite is deliberately narrow:
//   * the WHOLE argument list must be one chain of `.`-joined pieces, each a
//     plain scalar `($x)`, an interpolatable double-quoted literal, or a
//     number — any other call, index or expression keeps the concat form
//     (re-parenthesising its precedence is not ours to do);
//   * a list with a TOP-LEVEL comma (`print($x, "\n")`) is left alone: the
//     trailing `, "\n"` is the renderer's explicit newline, re-joining which
//     is a separate, riskier change;
//   * `say` is not touched.
fn fix_print_concat_interp(src: &str) -> String {
    // Mission item 5 — why this is still a TEXT pass.
    // Pure spelling: a flat `.` chain becomes one interpolating literal with
    // the same values in the same order. Applying it at emission would need
    // the string builder to know the final literal form after every word part
    // has been rendered. Pinned by
    // `retained_pass_tests::print_concat_chain_interpolates`.
    const CALL: &str = "print(";
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    loop {
        let Some(at) = rest.find(CALL) else {
            out.push_str(rest);
            return out;
        };
        if at > 0 {
            let prev = rest[..at].chars().next_back().unwrap_or(' ');
            if prev.is_ascii_alphanumeric() || prev == '_' {
                out.push_str(&rest[..at + CALL.len()]);
                rest = &rest[at + CALL.len()..];
                continue;
            }
        }
        let open = at + CALL.len() - 1;
        let Some(close) = call_close(rest, open) else {
            out.push_str(&rest[..at + CALL.len()]);
            rest = &rest[at + CALL.len()..];
            continue;
        };
        match concat_chain_to_interp(&rest[open + 1..close]) {
            Some(interp) => {
                out.push_str(&rest[..at]);
                out.push_str(&format!("print {interp}"));
                rest = &rest[close + 1..];
            }
            None => {
                out.push_str(&rest[..close + 1]);
                rest = &rest[close + 1..];
            }
        }
    }
}

/// Escape the characters that must not interpolate in a literal being folded
/// into a larger double-quoted string. An ALREADY-escaped `\$` stays as it
/// is — escaping it again turned `"\$" . ("42")` into `\\$42`, which prints a
/// backslash (dollar-escape-in-interpolate).
fn escape_literal_dollars(lit: &str) -> String {
    let mut out = String::with_capacity(lit.len() + 2);
    let mut chars = lit.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                out.push('\\');
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            '$' | '@' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Turn a `.`-joined print argument list into ONE double-quoted string, or
/// `None` when any piece is not safely interpolatable.
fn concat_chain_to_interp(inner: &str) -> Option<String> {
    let pieces = split_concat_chain(inner.trim())?;
    if pieces.len() < 2 {
        return None;
    }
    // A `$var` in a double-quoted string swallows what follows it: `$x` . "1"
    // is `$x1` (another variable), `$x` . "{a}" is `$x{a}` (a hash element).
    // bash's word is the CONCATENATION, so brace the name whenever the next
    // piece starts with a name character, `{`, `[` or `->`.
    enum Seg {
        Var(String),
        Text(String),
    }
    let mut segs: Vec<Seg> = Vec::new();
    for p in pieces {
        let p = p.trim();
        if let Some(name) = p.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
            let name = name.trim();
            if let Some(var) = name.strip_prefix('$') {
                if !var.is_empty()
                    && var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    segs.push(Seg::Var(var.to_string()));
                    continue;
                }
            }
            // A CONSTANT operand (`("abc")`, the renderer's spelling for a
            // folded literal) is the literal it names.
            if let Some(lit) = name.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
                segs.push(Seg::Text(escape_literal_dollars(lit)));
                continue;
            }
            return None;
        }
        if let Some(lit) = p.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
            // Already a literal: keep its escapes and keep it a literal.
            segs.push(Seg::Text(escape_literal_dollars(lit)));
            continue;
        }
        if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) {
            segs.push(Seg::Text(p.to_string()));
            continue;
        }
        return None;
    }
    let mut body = String::new();
    for i in 0..segs.len() {
        match &segs[i] {
            Seg::Var(v) => {
                let next = segs.get(i + 1).and_then(|s| match s {
                    Seg::Text(t) => t.chars().next(),
                    Seg::Var(_) => Some('$'),
                });
                let needs_braces = matches!(
                    next,
                    Some(c) if c.is_ascii_alphanumeric() || c == '_' || c == '{' || c == '[' || c == '-' || c == '$'
                );
                if needs_braces {
                    body.push_str(&format!("${{{v}}}"));
                } else {
                    body.push_str(&format!("${v}"));
                }
            }
            Seg::Text(t) => body.push_str(t),
        }
    }
    Some(format!("\"{body}\""))
}

/// Split on a top-level ` . ` — the renderer's concat spelling (spaced), which
/// cannot be confused with a `.` inside a string or a number. Returns `None`
/// when the list has a top-level comma (a multi-argument `print`).
fn split_concat_chain(s: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    let chars: Vec<char> = s.chars().collect();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            ',' if depth == 0 => return None,
            ' ' if depth == 0 && i + 2 < chars.len() && chars[i + 1] == '.' && chars[i + 2] == ' ' => {
                parts.push(&s[start..i]);
                start = i + 3;
                i += 3;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    if in_str {
        return None;
    }
    parts.push(&s[start..]);
    Some(parts)
}

// ── Fix 26 — `print(X, "\n")` is `say X` ─────────────────────────────────
//
// The renderer ends every newline-terminated output with an explicit
// `"\n"` argument:
//
//     print("Counter: " . ($i), "\n");        print($result, "\n");
//
// Perl has a statement for exactly this, and `use feature 'say'` is already
// in the preamble of every program that prints a newline — so the two-arg
// form is pure ceremony:
//
//     say "Counter: $i";                      say $result;
//
// Only the exact `, "\n")` tail is rewritten (651 of the corpus's 670
// two-argument prints), and only when the preceding argument list has no
// top-level comma of its own (so `print(a, b, "\n")` — a genuine list print —
// is left alone; its join is a different question).
fn fix_print_newline_to_say(src: &str) -> String {
    // Mission item 5 — why this is still a TEXT pass (and why that is safe).
    // The renderer emits `print(...)` from many independent sites (echo, the
    // printf `%s` fallback, capture tails); folding each one into `say` at
    // emission would duplicate this rule in every one of them. It cannot
    // change meaning: `print(LIST, "\n")` and `say LIST` produce the same
    // bytes, the pass only fires when the LAST argument is exactly `"\n"` and
    // the head is one flat value, and anything else is left untouched.
    // Pinned by `retained_pass_tests::print_newline_folds_into_say`.
    const CALL: &str = "print(";
    const TAIL: &str = ", \"\\n\")";
    if !src.contains(TAIL) {
        return src.to_string();
    }
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    loop {
        let Some(at) = rest.find(CALL) else {
            out.push_str(rest);
            return out;
        };
        if at > 0 {
            let prev = rest[..at].chars().next_back().unwrap_or(' ');
            if prev.is_ascii_alphanumeric() || prev == '_' {
                out.push_str(&rest[..at + CALL.len()]);
                rest = &rest[at + CALL.len()..];
                continue;
            }
        }
        let open = at + CALL.len() - 1;
        // `print`/`say` take a FLAT list, so the call ends at the first `)`
        // outside a string — a `)` inside a literal (`die "…: $!\n"`) is
        // data, not structure. (A paren-depth walk is wrong here AND
        // fragile: the renderer's own literals carry unbalanced `)`.)
        let Some(close) = call_close(rest, open) else {
            out.push_str(&rest[..at + CALL.len()]);
            rest = &rest[at + CALL.len()..];
            continue;
        };
        let inner = &rest[open + 1..close];
        match inner.strip_suffix(", \"\\n\"") {
            // the head must be a single argument: no top-level comma
            Some(head) if !has_toplevel_comma(head) => {
                // `say "Number: " . ($i)` is `say "Number: $i"`.
                let head = match concat_chain_to_interp(head) {
                    Some(interp) => interp,
                    // A single scalar/literal head needs no chain rewrite —
                    // it is already an interpolating `say` operand, and
                    // `print($x, "\n")` -> `say $x` is the whole point.
                    None if is_plain_say_operand(head) => head.to_string(),
                    // NOT a safe chain: keep the parenthesised call. A bare
                    // `say ($k) . " => " . …` is parsed as `say(...)` fed the
                    // CONCATENATED result — Perl warns ("say (...) interpreted
                    // as function") and the value changes shape.
                    None => {
                        out.push_str(&rest[..close + 1]);
                        rest = &rest[close + 1..];
                        continue;
                    }
                };
                out.push_str(&rest[..at]);
                out.push_str(&format!("say {head}"));
                rest = &rest[close + 1..];
            }
            _ => {
                out.push_str(&rest[..close + 1]);
                rest = &rest[close + 1..];
            }
        }
    }
}

/// Is `head` a single value `say` can take bare?
///
/// Only when there is no top-level comma and no top-level operator that
/// would bind differently: `say $x = 1` is an assignment, `say $x . "!"` is
/// fine but is handled by the interpolation chain instead. Anything with a
/// space-separated binary operator at depth 0 (`.`, `+`, `=`, `?:`, `//`, …)
/// is left to `concat_chain_to_interp` so the shape never changes.
fn is_plain_say_operand(head: &str) -> bool {
    let t = head.trim();
    if t.is_empty() || has_toplevel_comma(t) {
        return false;
    }
    // A scalar/hash-element/array-element/bareword/literal, plus simple
    // indexing and dereference sugar — no ` `, `.`, `+`, `=`, `?` at depth 0.
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    let mut esc = false;
    for c in t.chars() {
        if let Some(q) = in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '"' | '\'' | '`' => in_str = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ' ' | '.' | '+' | '-' | '*' | '/' | '%' | '=' | '?' | ':' | '|' | '&'
            | '<' | '>' | '!' if depth == 0 => return false,
            _ => {}
        }
    }
    depth == 0 && in_str.is_none()
}

/// The `)` that closes a print/say call.
///
/// Bracket depth IS tracked (an argument may itself be a call or a grouped
/// expression) but a quoted span is skipped whole — a `(` or `)` inside a
/// literal (`die "capture: $!\n"`, `q{…}` shell text) is data, not
/// structure. [`matching_paren`] can't be used here because it also has to
/// understand `q{…}`/`qw(…)` literals, whose braces unbalance a plain walk.
fn call_close(s: &str, open: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            q @ (b'\'' | b'"' | b'`') => {
                // Skip the whole quoted span, closing quote included: a `(`
                // or `)` inside a literal is DATA (`die "capture: $!\n"`,
                // `q{…}` shell text), not structure.
                i += 1;
                while i < b.len() && b[i] != b'\n' {
                    if b[i] == b'\\' && q != b'\'' {
                        i += 1;
                    } else if b[i] == q {
                        break;
                    }
                    i += 1;
                }
                // Only past the closer if there IS one: an unterminated
                // literal runs to end-of-line, and this `+1` then used to
                // swallow the following newline and desynchronise the scan
                // for the rest of the source.
                if i < b.len() && b[i] == q {
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

/// A comma at bracket depth 0 outside a quoted string — a list separator.
fn has_toplevel_comma(s: &str) -> bool {
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    let mut esc = false;
    for c in s.chars() {
        if let Some(q) = in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '"' | '\'' | '`' => in_str = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => return true,
            _ => {}
        }
    }
    false
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
pub(crate) fn declare_undeclared_vars(src: &str) -> String {
    // Mission item 5 — why this is still a TEXT pass.
    // Declaration-only: it adds `my`/`our` for names whose only occurrence in
    // the emitted Perl is a READ. It cannot change a value or a control-flow
    // decision, and the input it needs (a whole-program symbol table of what
    // the renderer emitted) exists only after rendering — the renderer writes
    // reads without keeping a binding registry. Without it the output is
    // strict-invalid Perl. Pinned by
    // `retained_pass_tests::undeclared_reads_are_declared`.
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
            q @ (b'\'' | b'"' | b'`') => {
                i += 1;
                while i < b.len() && b[i] != b'\n' {
                    if b[i] == b'\\' && q != b'\'' {
                        i += 1;
                    } else if b[i] == q {
                        break;
                    }
                    i += 1;
                }
                if i < b.len() && b[i] == q {
                    i += 1;
                }
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
    // `typeset -n REF=TGT` (every later read/write of REF must hit TGT) and
    // the `typeset -f/-F/-p` display forms run FIRST: the alias walk
    // rewrites reads/writes in place as bash parsed them, and the display
    // walk clones function bodies before the other fixes reshape them
    // (typeset-cmdsub: `ref="Changed via ref"` must change `original`, and
    // -f/-F/-p must print what bash prints — a shell-out to a nonexistent
    // `typeset` binary prints nothing at all). Both replace statements 1:1,
    // so the downstream `# line` index mapping stays aligned.
    fix_nameref_aliases(&mut fixed.stmts);
    fix_typeset_display(&prog.subs, &mut fixed.stmts);
    let fixed_stmts = fix_stmts(&fixed.stmts, &fns);
    fixed.stmts = fixed_stmts;
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
    // `type NAME` → its answer (transforms::type_query, SHARED core).
    // Re-run HERE, not only in the transform phase: Fix 7 above spliced
    // the eval'd program's function definitions into the statement list,
    // and a transform-phase run alone would miss them — `type doselect`
    // of a function defined inside an `eval '…'` would keep its shell-out
    // and print nothing. Any backend that inlines eval text can call the
    // same transform at its own equivalent point.
    crate::transforms::type_query::transform(&mut fixed.stmts);
    // Fix 10 — an INFINITE process-substitution producer must not run to
    // completion. It rewrites the block's shape (forks the producer), so
    // it runs BEFORE Fix 9, whose shape check then declines the rewritten
    // block — Fix 10 saves and restores the consumer's status itself.
    fixed.stmts = fix_ps_infinite_producer(&fixed.stmts);
    // Fix 9 — the materialised process-substitution consumer's OWN status.
    // Same ordering rule as Fix 7: it walks whole statements (a rewrite of
    // the block's shape), so it runs over the ALREADY-FIXED statements.
    fixed.stmts = fix_ps_status_blocks(&fixed.stmts);
    // `exec N>&M` is PERSISTENT in bash (the parent's fd table), but the
    // renderer lowers it as a one-shot `system('bash', …)` shell-out — fd N
    // never exists in the parent, so a later `echo … >&N` resolved to
    // "STDOUT follows STDERR" (the stand-in for any unknown fd) and the line
    // leaked to stderr while bash kept it on stdout (parse-invalid-redirect:
    // `exec 3>&1; echo "test" >&3` printed "test" on stderr). Record the
    // installs and rewrite later dup targets through the map.
    fixed.stmts = fix_exec_fd_dups(fixed.stmts);
    fixed.subs = prog
        .subs
        .iter()
        .map(|s| crate::ir::IrSub {
            name: s.name.clone(),
            params: s.params.clone(),
            param_defaults: s.param_defaults.clone(),
            body: fix_stmts(&s.body, &fns),
        })
        .collect();
    // Function bodies execute in the same shell as file scope, so they
    // inherit the fd map the top-level statements install.
    let mut fd_map = fd_map_of(&fixed.stmts);
    for sub in fixed.subs.iter_mut() {
        walk_fd_dups(&mut sub.body, &mut fd_map);
    }
    fixed
}

// ── typeset: nameref aliasing + the -f/-F/-p display forms ────────────────
//
// typeset-cmdsub.sh pins three behaviours a shell-out to `typeset` cannot
// deliver on Linux (there is no `typeset` binary; the shell-out prints
// nothing and exits 127):
//
// * `typeset -n REF=TGT` — REF becomes an ALIAS: every later read and write
//   of REF hits TGT (`ref="Changed via ref"` must change `original`).
// * `typeset -f NAME` — bash re-prints the definition, measured byte-exact:
//   `NAME () \n{ \n    stmt;\n    stmt\n}\n` (trailing spaces after the
//   `()` and `{`; all but the last statement carry `;`).
// * `typeset -F NAME` / `typeset -p NAME` — the name alone / bash's own
//   `declare -ir NAME="99"` line. `-f`/`-F` on an unknown name exit 1 with
//   no output.
//
// Refuse > guess: display replacement only fires for plain literal operands
// and bodies every statement of which renders back to bash; anything else
// keeps the old shell-out rather than printing a wrong answer.

/// A statement's shell call: command name + WORD list (`exec_word_args`
/// skips `args[0]`). `Expr(Call{…})` only — a `Redirect` wrapper is unwrapped
/// by the caller when it wants the whole statement replaced.
fn shell_call_words(st: &IrStmt) -> Option<(String, Vec<&IrExpr>)> {
    let IrStmt::Expr(IrExpr::Call { func, args }) = st else {
        return None;
    };
    if !matches!(func.as_str(), "builtin" | "exec") {
        return None;
    }
    let Some(IrExpr::Str(cmd, _)) = args.first() else {
        return None;
    };
    Some((cmd.clone(), crate::ir::exec_word_args(args)))
}

/// The `$main_exit_code = $CHILD_ERROR = N;` write a replaced statement
/// leaves behind — the status bash's command would have reported.
fn status_raw(rc: u8) -> IrStmt {
    IrStmt::RawText(format!("$main_exit_code = $CHILD_ERROR = {rc};\n"))
}

/// A perl `print` of the exact text, as its own statement.
fn print_raw(text: &str) -> IrStmt {
    IrStmt::RawText(format!(
        "print(\"{}\");\n$main_exit_code = $CHILD_ERROR = 0;\n",
        perl_dq_body(text)
    ))
}

/// Escape text for a perl double-quoted string literal — the printed bytes
/// must be the text itself (`$`/`@` must not interpolate, `\`/`"` must not
/// terminate).
fn perl_dq_body(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' => out.push_str("\\$"),
            '@' => out.push_str("\\@"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// A shell variable NAME (letters/digits/underscore, not starting with a
/// digit).
fn shell_plain_name(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

// ── nameref ───────────────────────────────────────────────────────────────

/// What a statement does to the nameref map.
enum NamerefCtl {
    /// Not a nameref control statement.
    No,
    /// `unset -n NAME…` — the aliases go away.
    Unset(Vec<String>),
    /// `typeset -n REF=TGT` — REF becomes an alias for TGT.
    Alias(String, String),
}

fn nameref_ctl(st: &IrStmt) -> NamerefCtl {
    // `unset -n ref 2>/dev/null` arrives Redirect-wrapped; the control
    // statement replaces the WHOLE thing (the redirect goes with it — the
    // shell-out it belonged to is gone).
    let call_st = match st {
        IrStmt::Redirect { inner, .. } => match inner.as_slice() {
            [one @ IrStmt::Expr(IrExpr::Call { .. })] => one,
            _ => return NamerefCtl::No,
        },
        other => other,
    };
    let Some((cmd, words)) = shell_call_words(call_st) else {
        return NamerefCtl::No;
    };
    let mut word_strs = Vec::with_capacity(words.len());
    for w in words {
        let IrExpr::Str(s, _) = w else {
            return NamerefCtl::No;
        };
        word_strs.push(s.as_str());
    }
    if cmd == "unset" {
        if !word_strs.iter().any(|w| *w == "-n") {
            return NamerefCtl::No;
        }
        let mut names = Vec::new();
        for w in &word_strs {
            if w.starts_with('-') {
                continue;
            }
            if shell_plain_name(w) {
                names.push(w.to_string());
            }
        }
        return NamerefCtl::Unset(names);
    }
    if matches!(cmd.as_str(), "typeset" | "declare") {
        let mut ops = Vec::new();
        for w in &word_strs {
            if w.starts_with('-') && w.len() > 1 {
                if *w != "-n" {
                    return NamerefCtl::No; // any other flag — not ours
                }
            } else {
                ops.push(*w);
            }
        }
        if !word_strs.iter().any(|w| *w == "-n") || ops.len() != 1 {
            return NamerefCtl::No;
        }
        let Some((r, t)) = ops[0].split_once('=') else {
            return NamerefCtl::No;
        };
        if !shell_plain_name(r) || !shell_plain_name(t) {
            return NamerefCtl::No;
        }
        return NamerefCtl::Alias(r.to_string(), t.to_string());
    }
    NamerefCtl::No
}

/// Sequential alias walk: each `typeset -n` installs its alias, each
/// `unset -n` drops it, every statement in between has its reads and writes
/// of the aliased name retargeted.
fn fix_nameref_aliases(stmts: &mut Vec<IrStmt>) {
    let mut al: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    nameref_walk(stmts, &mut al);
}

fn nameref_walk(stmts: &mut Vec<IrStmt>, al: &mut std::collections::HashMap<String, String>) {
    for st in stmts.iter_mut() {
        match nameref_ctl(st) {
            NamerefCtl::Unset(names) => {
                for n in names {
                    al.remove(&n);
                }
                *st = status_raw(0);
            }
            NamerefCtl::Alias(r, t) => {
                al.insert(r, t);
                *st = status_raw(0);
            }
            NamerefCtl::No => nameref_rewrite_stmt(st, al),
        }
    }
}

fn nameref_rewrite_stmt(st: &mut IrStmt, al: &mut std::collections::HashMap<String, String>) {
    match st {
        IrStmt::Expr(e) => nameref_rewrite_expr(e, al),
        IrStmt::Output { value, .. } => nameref_rewrite_expr(value, al),
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets.iter_mut() {
                if t.indices.is_empty() {
                    if let Some(n) = al.get(&t.var) {
                        t.var = n.clone();
                    }
                }
            }
            nameref_rewrite_expr(expr, al);
        }
        IrStmt::Declare { init: Some(e), .. } => nameref_rewrite_expr(e, al),
        IrStmt::Redirect { inner, redirects } => {
            for r in redirects.iter_mut() {
                nameref_rewrite_expr(&mut r.target, al);
            }
            nameref_walk(inner, al);
        }
        IrStmt::Function { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body) => nameref_walk(body, al),
        IrStmt::While { cond, body } => {
            nameref_rewrite_expr(cond, al);
            nameref_walk(body, al);
        }
        IrStmt::DoWhile { body, cond, .. } => {
            nameref_rewrite_expr(cond, al);
            nameref_walk(body, al);
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            nameref_rewrite_expr(cond, al);
            nameref_walk(then, al);
            for (c, b) in elsifs.iter_mut() {
                nameref_rewrite_expr(c, al);
                nameref_walk(b, al);
            }
            nameref_walk(else_, al);
        }
        // Try/For/Case/…: no corpus nameref reaches them — refuse > guess
        // (their reads stay as written, exactly the pre-fix behaviour).
        _ => {}
    }
}

fn nameref_rewrite_expr(e: &mut IrExpr, al: &mut std::collections::HashMap<String, String>) {
    match e {
        IrExpr::Call { func, args } => {
            if func == "getVar" {
                if let Some(IrExpr::Str(n, style)) = args.first_mut() {
                    if let Some(t) = al.get(n) {
                        *n = t.clone();
                        let _ = style;
                        return; // name swapped; no need to walk the arg
                    }
                }
            }
            if func == "param" && args.len() == 2 {
                let tgt = match &args[1] {
                    IrExpr::Str(n, _) if shell_plain_name(n) => al.get(n).cloned(),
                    _ => None,
                };
                if let (Some(t), Some(IrExpr::Str(n, _))) = (tgt, args.get_mut(1)) {
                    *n = t;
                    return;
                }
            }
            for a in args.iter_mut() {
                nameref_rewrite_expr(a, al);
            }
        }
        IrExpr::Var(n, _) => {
            if let Some(t) = al.get(n) {
                *n = t.clone();
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let InterpPart::Expr(inner) = p {
                    nameref_rewrite_expr(inner, al);
                }
            }
        }
        IrExpr::Arrow(stmts) => nameref_walk(stmts, al),
        IrExpr::Array(items) => {
            for i in items.iter_mut() {
                nameref_rewrite_expr(i, al);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            nameref_rewrite_expr(lhs, al);
            nameref_rewrite_expr(rhs, al);
        }
        IrExpr::DefinedOr { expr, default } => {
            nameref_rewrite_expr(expr, al);
            nameref_rewrite_expr(default, al);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            nameref_rewrite_expr(cond, al);
            nameref_rewrite_expr(then, al);
            nameref_rewrite_expr(else_, al);
        }
        IrExpr::Capture { expr, .. } => nameref_rewrite_expr(expr, al),
        IrExpr::Index { key, .. } => nameref_rewrite_expr(key, al),
        IrExpr::Splice(inner) => nameref_rewrite_expr(inner, al),
        _ => {}
    }
}

// ── typeset -f / -F / -p ──────────────────────────────────────────────────

/// Function registry (name → bash re-print text, or None when the body
/// does not render back to bash) and the declaration tracker
/// (name → (attribute flags, literal value)).
type TypesetCtx = (
    std::collections::HashMap<String, Option<String>>,
    std::collections::HashMap<String, (String, Option<String>)>,
);

fn fix_typeset_display(subs: &[crate::ir::IrSub], stmts: &mut Vec<IrStmt>) {
    let mut funcs: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    for s in subs {
        funcs.insert(s.name.clone(), fn_bash_text(&s.name, &s.body));
    }
    let mut attrs: std::collections::HashMap<String, (String, Option<String>)> =
        std::collections::HashMap::new();
    typeset_display_walk(stmts, &mut funcs, &mut attrs);
}

fn typeset_display_walk(
    stmts: &mut Vec<IrStmt>,
    funcs: &mut std::collections::HashMap<String, Option<String>>,
    attrs: &mut std::collections::HashMap<String, (String, Option<String>)>,
) {
    for st in stmts.iter_mut() {
        if let IrStmt::Function { name, body, .. } = st {
            let text = fn_bash_text(name, body);
            funcs.insert(name.clone(), text);
            typeset_display_walk(body, funcs, attrs);
            continue;
        }
        if typeset_display_replace(st, funcs, attrs) {
            continue;
        }
        typeset_track(st, attrs);
    }
}

/// Replace a `typeset|declare -f|-F|-p NAME` statement with the print bash
/// would produce (or its exit status). True when replaced.
fn typeset_display_replace(
    st: &mut IrStmt,
    funcs: &std::collections::HashMap<String, Option<String>>,
    attrs: &std::collections::HashMap<String, (String, Option<String>)>,
) -> bool {
    let Some((cmd, words)) = shell_call_words(st) else {
        return false;
    };
    if cmd != "typeset" && cmd != "declare" {
        return false;
    }
    let mut disp: Option<char> = None;
    let mut other_flags = 0usize;
    let mut ops: Vec<String> = Vec::new();
    for w in words {
        let IrExpr::Str(s, _) = w else {
            return false; // non-literal word: not our display form
        };
        if s.starts_with('-') && s.len() > 1 {
            let flag = &s[1..];
            if flag.len() == 1 && "fFp".contains(flag) {
                if disp.is_some() {
                    return false;
                }
                disp = flag.chars().next();
            } else {
                other_flags += 1;
            }
        } else {
            ops.push(s.clone());
        }
    }
    let Some(d) = disp else {
        return false;
    };
    if other_flags > 0 || ops.len() != 1 || !shell_plain_name(&ops[0]) {
        return false;
    }
    let name = ops[0].clone();
    match d {
        'f' => match funcs.get(&name) {
            Some(Some(text)) => {
                let text = text.clone();
                *st = print_raw(&text);
                true
            }
            // Defined but does not render back to bash: keep the old path.
            Some(None) => false,
            // bash: `typeset -f nope` → rc 1, no output.
            None => {
                *st = status_raw(1);
                true
            }
        },
        'F' => match funcs.get(&name) {
            Some(_) => {
                *st = print_raw(&format!("{name}\n"));
                true
            }
            None => {
                *st = status_raw(1);
                true
            }
        },
        'p' => match attrs.get(&name) {
            Some((flags, Some(value))) if declare_value_ok(value) => {
                let line = if flags.is_empty() {
                    format!("declare -- {name}=\"{value}\"")
                } else {
                    let mut f: Vec<char> = flags.chars().collect();
                    f.sort_unstable();
                    f.dedup();
                    let f: String = f.into_iter().collect();
                    format!("declare -{f} {name}=\"{value}\"")
                };
                *st = print_raw(&format!("{line}\n"));
                true
            }
            // Not tracked (we cannot know the value) or an unsafe value:
            // refuse rather than print bash's answer from a guess.
            _ => false,
        },
        _ => false,
    }
}

/// `declare -p`'s double-quoted value must round-trip through perl's
/// string AND bash's own quoting; only values with no characters bash
/// would escape are printed.
fn declare_value_ok(v: &str) -> bool {
    !v.is_empty() && !v.contains(['"', '\\', '$', '`', '\n', '\r'])
}

/// Declaration / unset / assignment tracking for `declare -p`.
fn typeset_track(
    st: &IrStmt,
    attrs: &mut std::collections::HashMap<String, (String, Option<String>)>,
) {
    if let IrStmt::Assign { targets, expr, .. } = st {
        if let [t] = targets.as_slice() {
            if t.indices.is_empty() && shell_plain_name(&t.var) {
                let value = track_literal(expr);
                let slot = attrs
                    .entry(t.var.clone())
                    .or_insert_with(|| (String::new(), None));
                slot.1 = value;
            }
        }
        return;
    }
    let Some((cmd, words)) = shell_call_words(st) else {
        return;
    };
    if cmd == "unset" {
        for w in words {
            if let IrExpr::Str(s, _) = w {
                if !s.starts_with('-') && shell_plain_name(s) {
                    attrs.remove(s);
                }
            }
        }
        return;
    }
    if !matches!(cmd.as_str(), "typeset" | "declare") {
        return;
    }
    // Only the attribute flags this tracker honours declare anything;
    // anything else (`-a`, `-g`, `-t`, the display forms) is not ours.
    let mut flags = String::new();
    let mut operands: Vec<&IrExpr> = Vec::new();
    for w in words {
        match w {
            IrExpr::Str(s, _) if s.starts_with('-') && s.len() > 1 => {
                let f: String = s[1..].chars().filter(|c| "irlux".contains(*c)).collect();
                if f.len() != s.len() - 1 {
                    return;
                }
                flags.push_str(&f);
            }
            other => operands.push(other),
        }
    }
    if flags.is_empty() {
        return;
    }
    let mut i = 0usize;
    while i < operands.len() {
        if let IrExpr::Str(s, _) = operands[i] {
            if let Some((name, v)) = s.split_once('=') {
                if shell_plain_name(name) {
                    // `rovar="immutable"` arrives as `rovar=` + an
                    // interpolated word; a dynamic value is value-unknown.
                    let mut value = Some(v.to_string());
                    if v.is_empty() && i + 1 < operands.len() {
                        if let IrExpr::Interpolate(parts) = operands[i + 1] {
                            value = match parts.as_slice() {
                                [InterpPart::Lit(l)] => Some(l.clone()),
                                _ => None,
                            };
                            i += 1;
                        } else {
                            value = None;
                        }
                    }
                    let slot = attrs
                        .entry(name.to_string())
                        .or_insert_with(|| (String::new(), None));
                    for c in flags.chars() {
                        if !slot.0.contains(c) {
                            slot.0.push(c);
                        }
                    }
                    slot.1 = value;
                }
            } else if shell_plain_name(s) {
                // `typeset -r printtest` — attributes only, value unchanged.
                let slot = attrs
                    .entry(s.to_string())
                    .or_insert_with(|| (String::new(), None));
                for c in flags.chars() {
                    if !slot.0.contains(c) {
                        slot.0.push(c);
                    }
                }
            }
        }
        i += 1;
    }
}

/// The literal text of an assignment RHS, if it has no expansions.
fn track_literal(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts) => {
            let mut out = String::new();
            for p in parts {
                match p {
                    InterpPart::Lit(s) => out.push_str(s),
                    InterpPart::Expr(_) => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

// ── bash re-print of a function body (`typeset -f`) ───────────────────────

/// bash's own re-print format, measured on bash 5.2:
/// `NAME () \n{ \n    stmt;\n    stmt\n}\n` — trailing space after the
/// `()` and the `{`, 4-space indent, `;` on every statement but the last.
fn fn_bash_text(name: &str, body: &[IrStmt]) -> Option<String> {
    let mut lines = Vec::with_capacity(body.len());
    for st in body {
        lines.push(stmt_bash_line(st)?);
    }
    let mut out = format!("{name} () \n{{ \n");
    let n = lines.len();
    for (i, l) in lines.into_iter().enumerate() {
        out.push_str("    ");
        out.push_str(&l);
        if i + 1 < n {
            out.push(';');
        }
        out.push('\n');
    }
    out.push_str("}\n");
    Some(out)
}

fn stmt_bash_line(st: &IrStmt) -> Option<String> {
    match st {
        IrStmt::Expr(IrExpr::Call { func, args })
            if matches!(func.as_str(), "builtin" | "exec") =>
        {
            let Some(IrExpr::Str(cmd, _)) = args.first() else {
                return None;
            };
            let mut out = cmd.clone();
            for w in crate::ir::exec_word_args(args) {
                out.push(' ');
                out.push_str(&bash_word(w)?);
            }
            Some(out)
        }
        IrStmt::Declare {
            vars, init: Some(init), local: true, ..
        } => {
            if vars.len() != 1 || vars[0].sigil.is_some() || !shell_plain_name(&vars[0].name) {
                return None;
            }
            Some(format!("local {}={}", vars[0].name, bash_value(init)?))
        }
        // The echo fold's canonical node, re-spelled as the `echo` that
        // produced it. `fold_to_output`/`fold_dynamic_to_output` bake the
        // joined words into the value and carry the trailing newline in
        // the flag (never in the text), so `bash_word` round-trips the
        // two shapes the fold emits: a literal and an interpolation
        // whose expansions re-render as `$name`. A target-bearing
        // Output is not an echo; anything else keeps the old refusal.
        IrStmt::Output {
            value,
            newline,
            target: None,
        } => {
            let w = bash_word(value)?;
            Some(format!("echo{} {}", if *newline { "" } else { " -n" }, w))
        }
        // Output folds, Assigns, control flow in a printed body: no source
        // text survives to reproduce — refuse and keep the shell-out.
        _ => None,
    }
}

/// A command word back to bash: a bare-safe literal word printed as-is,
/// a quoted literal word re-quoted with bash's own spelling (the parser
/// keeps the text LITERAL in `Str` with its [`StrStyle`] — an expansion
/// would be `Interpolate`), or a double-quoted interpolation whose
/// expansions re-render as `$name`.
fn bash_word(w: &IrExpr) -> Option<String> {
    match w {
        IrExpr::Str(s, style) => {
            if !s.is_empty()
                && !s
                    .chars()
                    .any(|c| c.is_whitespace() || "'\"\\$`*?[]{};|&<>()#~".contains(c))
            {
                return Some(s.clone());
            }
            literal_word_quoted(s, style.clone())
        }
        IrExpr::Interpolate(parts) => {
            let mut out = String::from("\"");
            for p in parts {
                match p {
                    InterpPart::Lit(s) => {
                        // `$`/`\`/`"` in a literal came from escapes whose
                        // source spelling we no longer have — refuse.
                        if s.contains(['"', '\\', '$', '`']) {
                            return None;
                        }
                        out.push_str(s);
                    }
                    InterpPart::Expr(e) => match &**e {
                        IrExpr::Call { func, args } if func == "getVar" => {
                            let Some(IrExpr::Str(n, _)) = args.first() else {
                                return None;
                            };
                            if !shell_plain_name(n) {
                                return None;
                            }
                            out.push('$');
                            out.push_str(n);
                        }
                        _ => return None,
                    },
                }
            }
            out.push('"');
            Some(out)
        }
        _ => None,
    }
}

/// An initialiser value (`local x=5`): a bare literal or a quoted string.
fn bash_value(v: &IrExpr) -> Option<String> {
    match v {
        IrExpr::Str(s, style) => {
            if !s.is_empty()
                && !s
                    .chars()
                    .any(|c| c.is_whitespace() || "'\"\\$`*?[]{};|&<>()#~".contains(c))
            {
                return Some(s.clone());
            }
            literal_word_quoted(s, style.clone())
        }
        IrExpr::Interpolate(parts) => match parts.as_slice() {
            [InterpPart::Lit(s)] if !s.contains(['"', '\\', '$', '`']) => {
                Some(format!("\"{s}\""))
            }
            _ => None,
        },
        _ => None,
    }
}

/// A literal text whose bare reprint is not the same word (it carries a
/// whitespace or shell-special character) re-quoted the way bash itself
/// re-quotes it in `typeset -f`: double quotes for a double-quoted word,
/// single quotes for a single-quoted one. Only the characters a
/// double-quoted re-render would RE-EXPAND (`"` `\\` `$` `` ` `` — their
/// escape spelling is gone) or a `'` inside single quotes refuse.
///
/// Byte-exactness note: the parser does not record bare-vs-quoted for a
/// word whose text happens to contain a special (`a#b` and `"a#b"` are
/// both `Str("a#b", DoubleQuoted)`), so a rare bare `#`/`~` word reprints
/// quoted — the same WORD, one spelling different from bash's echo of the
/// source. That is no worse than the shell-out it replaces (prints
/// nothing), and every currently-reproducing body keeps its bare reprint
/// (this is only reached when the bare form would not parse back).
fn literal_word_quoted(s: &str, style: crate::ir::StrStyle) -> Option<String> {
    match style {
        crate::ir::StrStyle::DoubleQuoted if !s.contains(['"', '\\', '$', '`']) => {
            Some(format!("\"{s}\""))
        }
        crate::ir::StrStyle::SingleQuoted if !s.contains('\'') => Some(format!("'{s}'")),
        _ => None,
    }
}

/// Rewrite `exec N>&M` installs and later `>&N` uses in one sequential walk.
///
/// A bare `exec N>&M` (the literal `exec` builtin, no command words) records
/// N→M; every fd-dup target `&K` in a later statement is then replaced by
/// `&resolve(K)` — transitively, so `exec 3>&4` + `exec 4>&1` gives `&1`.
/// `Subshell`/`Background` bodies walk a CLONE of the map (their fd table is
/// a copy; installs there must not leak out), functions share it (same shell).
fn fix_exec_fd_dups(stmts: Vec<IrStmt>) -> Vec<IrStmt> {
    let mut stmts = stmts;
    let mut map = std::collections::HashMap::new();
    walk_fd_dups(&mut stmts, &mut map);
    stmts
}

/// The fd map a statement list installs (used to seed function bodies, which
/// execute in the same shell after the top-level statements have run).
fn fd_map_of(stmts: &[IrStmt]) -> std::collections::HashMap<i32, i32> {
    let mut map = std::collections::HashMap::new();
    let mut owned = stmts.to_vec();
    walk_fd_dups(&mut owned, &mut map);
    map
}

fn walk_fd_dups(stmts: &mut Vec<IrStmt>, map: &mut std::collections::HashMap<i32, i32>) {
    for st in stmts.iter_mut() {
        match st {
            IrStmt::Redirect { inner, redirects } => {
                // This statement's own targets resolve through fds
                // established EARLIER (an `exec` here installs AFTER its
                // own redirect, matching bash: `exec 3>&3` is a no-op).
                for r in redirects.iter_mut() {
                    let Some(n) = redirect_dup_num(&r.target) else { continue; };
                    let to = resolve_fd(map, n);
                    if to != n {
                        r.target = crate::ir::IrExpr::Str(
                            format!("&{to}"),
                            crate::ir::StrStyle::DoubleQuoted,
                        );
                    }
                }
                if is_bare_exec_call_stmt(inner) {
                    for r in redirects.iter() {
                        if let (Some(n), Some(m)) = (r.fd, redirect_dup_num(&r.target)) {
                            map.insert(n, m);
                        }
                    }
                }
                walk_fd_dups(inner, map);
            }
            IrStmt::Block(v) | IrStmt::Subshell(v) | IrStmt::Background(v) => {
                walk_fd_dups(v, map)
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                walk_fd_dups(then, map);
                for (_, b) in elsifs.iter_mut() {
                    walk_fd_dups(b, map);
                }
                walk_fd_dups(else_, map);
            }
            IrStmt::Try { body, excepts, else_body, finally_body } => {
                walk_fd_dups(body, map);
                for e in excepts.iter_mut() {
                    walk_fd_dups(&mut e.body, map);
                }
                walk_fd_dups(else_body, map);
                walk_fd_dups(finally_body, map);
            }
            IrStmt::For { body, .. }
            | IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::Function { body, .. } => walk_fd_dups(body, map),
            IrStmt::ForInit { init, step, body, .. } => {
                walk_fd_dups(init, map);
                walk_fd_dups(step, map);
                walk_fd_dups(body, map);
            }
            IrStmt::Pipeline { stages, .. } => {
                for s in stages.iter_mut() {
                    walk_fd_dups(s, map);
                }
            }
            _ => {}
        }
    }
}

/// The numeric fd after `&` in a dup target (`"&3"` → 3).
fn redirect_dup_num(target: &crate::ir::IrExpr) -> Option<i32> {
    let crate::ir::IrExpr::Str(s, _) = target else { return None; };
    s.strip_prefix('&')?.parse().ok()
}

/// Follow N→M chains; a cycle or unknown fd returns N unchanged.
fn resolve_fd(map: &std::collections::HashMap<i32, i32>, mut n: i32) -> i32 {
    for _ in 0..16 {
        match map.get(&n) {
            Some(&m) if m != n => n = m,
            _ => break,
        }
    }
    n
}

/// `[Expr(Call{ func: "exec", args: [Str("exec"), Array([])] })]` — the
/// literal `exec` builtin with no command words (its redirects persist).
fn is_bare_exec_call_stmt(inner: &[IrStmt]) -> bool {
    matches!(
        inner,
        [IrStmt::Expr(IrExpr::Call { func, args })]
            if func == "exec"
                && matches!(args.as_slice(), [IrExpr::Str(c, _), IrExpr::Array(w)]
                    if c == "exec" && w.is_empty())
    )
}

// ── A bash function's positional parameters (moved into the IR) ──────
//
// The Function arm now emits `local @__sh_args = @_;` plus
// `my $__argc = @__sh_args;` and spells `$1`..`$n` as `$__sh_args[n-1]`,
// so the body agrees with its own signature without a text pass. Before
// that, the body's `$1` read `@ARGV`, which `do FILE` empties, so subs
// silently computed over nothing (063_09 printed "Processed 0 options").
// `$0` inside a function stays the SCRIPT name, not the function name.






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
    # This sub is injected right after the `use` block, BEFORE the
    # file-scope `our @__sh_args; our $__sh_args_n;` declarations, so a bare
    # read here is a `use strict` COMPILE error and the whole program
    # aborts. Re-declaring the package globals inside the sub is always
    # legal and independent of preamble ordering (same package, so it still
    # sees the caller's values).
    our (@__sh_args, $__sh_args_n);
    my $__off = shift();
    my $__len = shift();
    return () if $__off < 1;
    return () if $__off > $__sh_args_n;
    # `@__sh_args` / `$__sh_args_n`, NOT `@ARGV` / `$#ARGV`: this helper is
    # injected AFTER `post_pass`, which is what rebinds the file-scope
    # positional reads to `@__sh_args` (a `set --` writes there, and `do
    # FILE` runs with `@ARGV` already emptied). Reading `@ARGV` here saw a
    # different (empty) list than the statements around it, so
    # parse-at-slice-param printed no space where bash printed one.
    my $__end = defined($__len) ? $__off + $__len - 1 : $__sh_args_n - 1;
    $__end = $__sh_args_n - 1 if $__end > $__sh_args_n - 1;
    return () if $__end < $__off - 1;
    return @__sh_args[$__off - 1 .. $__end];
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

// ── Native glob words ───────────────────────────────────────────────────
//
// `render_word` lowers an unquoted glob word (`ls file_*.txt`) to
// `__sh_glob('file_*.txt')` so the uu-ffi seam can take the command
// instead of falling back to `system('bash', '-c', …)` — one process
// instead of two, and perl-native output.
//
// Bash parity: with no nullglob, an UNMATCHED pattern reaches the command
// as the literal word (`ls *.nope` errors on `*.nope`); a bare perl
// `glob()` would return an EMPTY list and run `ls` with no arguments —
// listing the directory instead of failing. Matches are returned as the
// list, sorted, no dotfiles (same as bash's default collation).
pub const GLOB_PREAMBLE: &str = r#"sub __sh_glob {
    my @__g = glob($_[0]);
    return @__g ? @__g : ($_[0]);
}
"#;

/// True when the rendered program needs `__sh_glob` defined.
pub fn needs_glob_helper(perl_src: &str) -> bool {
    perl_src.contains("__sh_glob(")
}

// ── Affix primitives (test-lowering transform's glob arm) ──────────────
//
// estree renders these as `.startsWith`/`.endsWith`, C as
// `strncmp`/suffix compares, and `contains` natively as `index >= 0`
// — Perl had neither an arm nor a sub, so the lifted calls were bare
// and died at runtime. LITERAL prefix/suffix decisions (the same value
// semantics the transform's `operand()` guarantees: a var read or a
// metachar-free literal). The empty needle is a prefix and a suffix of
// everything (bash `[[ x == ""* ]]` is true), which the length guard
// gives for free.
pub const AFFIX_PREAMBLE: &str = r#"sub strHasPrefix {
    return index($_[0], $_[1]) == 0;
}
sub strHasSuffix {
    my ($__h, $__n) = @_;
    return length($__n) <= length($__h) && substr($__h, length($__h) - length($__n)) eq $__n;
}
"#;

/// Define every runtime helper the rendered program CALLS, exactly once.
///
/// This is the single owner of the helper preambles: the calls are emitted by
/// the shared lowering, so the definitions belong to the shared renderer, not to
/// one wrapper. `ir::shir_to_perl` calls it on its own output and
/// `perl_generator_fixed::generate` calls it again after its text passes — the
/// second call is a no-op because each helper's `sub` line is already present.
pub fn inject_renderer_helpers(mut src: String) -> String {
    // `sub` is file scope: a helper must precede the first use, and Perl
    // resolves a named sub called without parens at compile time, so each one
    // is injected right after the `use` block, before any statement runs.
    let helpers: [(fn(&str) -> bool, &str); 5] = [
        (needs_fn_status_helper, FN_STATUS_PREAMBLE),
        (needs_slice_helper, SLICE_PREAMBLE_2),
        (needs_pos_slice_helper, SLICE_PREAMBLE),
        (needs_glob_helper, GLOB_PREAMBLE),
        (needs_affix_helper, AFFIX_PREAMBLE),
    ];
    for (needed, preamble) in helpers {
        let marker = preamble
            .lines()
            .find(|l| l.starts_with("sub "))
            .unwrap_or_default();
        if needed(&src) && !marker.is_empty() && !src.contains(marker) {
            src = inject_before_first_stmt(&src, preamble);
        }
    }
    src
}

/// True when the rendered program calls the affix primitives.
pub fn needs_affix_helper(perl_src: &str) -> bool {
    perl_src.contains("strHasPrefix(") || perl_src.contains("strHasSuffix(")
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

    // ── Fix 10 — an infinite process-substitution producer ─────────────

    /// `head <(while true; …)` — the producer must be FORKED (and later
    /// killed), never run to completion in the parent.
    #[test]
    fn infinite_procsub_producer_forks_and_is_killed() {
        let perl = gen("head <(while true; do echo .; sleep 1; done)\n");
        assert!(perl.contains("fork()"), "producer is forked: {perl}");
        // pipe semantics: a regular temp file would hand head an immediate
        // EOF where bash blocks for the writer's lines
        assert!(perl.contains("POSIX::mkfifo"), "temp becomes a fifo: {perl}");
        assert!(perl.contains("kill('TERM'"), "producer is signalled: {perl}");
        assert!(perl.contains("waitpid("), "producer is reaped: {perl}");
        // the CONSUMER's status still reaches `$?`
        assert!(
            perl.contains("$__ps_status0 = $CHILD_ERROR;"),
            "consumer status saved: {perl}"
        );
    }

    /// A FINITE producer keeps the sequential temp-file form (equivalent
    /// there, and cheaper than a fork + fifo).
    #[test]
    fn finite_procsub_producer_stays_sequential() {
        let perl = gen("head <(echo hi)\n");
        assert!(!perl.contains("mkfifo"), "no fifo: {perl}");
        assert!(!perl.contains("fork()"), "no fork: {perl}");
    }

    /// A `while true` whose body CAN leave (`break`) is not provably
    /// infinite — the sequential form stays.
    #[test]
    fn breakable_loop_producer_stays_sequential() {
        let perl = gen("head <(while true; do echo x; break; done)\n");
        assert!(!perl.contains("mkfifo"), "no fifo: {perl}");
        assert!(!perl.contains("fork()"), "no fork: {perl}");
    }

    /// `kill` lowers to Perl's builtin — never a shell-out — and a literal
    /// `0` operand (bash's "my whole process group") is refused rather
    /// than turned into a self-TERM.
    #[test]
    fn kill_builtin_renders_native_perl() {
        let perl = gen("bgpid=5\nkill -TERM $bgpid\n");
        // native Perl kill with the pid operand — no uu-ffi spawn (the
        // seam has no kill) and no bash -c shell-out
        assert!(perl.contains("kill('TERM', "), "native kill: {perl}");
        assert!(
            !perl.contains("__sh_uu_run('kill'"),
            "no uu-ffi spawn: {perl}"
        );
        assert!(!perl.contains("system('bash'"), "no shell-out: {perl}");
    }

    #[test]
    fn kill_literal_zero_pid_is_refused() {
        let perl = gen("kill -TERM 0\n");
        assert!(
            !perl.contains("kill('TERM', 0)"),
            "never signals the process group: {perl}"
        );
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
        // SLICE_PREAMBLE now carries only `__sh_pos`; `__sh_slice` lives in
        // SLICE_PREAMBLE_2 (the helpers are emitted independently, so each
        // preamble holds exactly one sub). These assertions are about
        // __sh_slice, so include _2.
        let helper = SLICE_PREAMBLE_2;
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
    /// The `${v#pat}` family with `:`, `[`, `]`, `!`, `*` and a POSIX class
    /// in the PATTERN, rendered through the `pl` target's real entry point
    /// and compared to bash. Two renderer bugs made this fail before: the
    /// class scanner ended the bracket expression at the inner `]` of
    /// `[:space:]` (emitting `[[:space:]\]`, an unmatched `[` that killed
    /// the program at compile time), and `%` was rendered as a leftmost
    /// regex match — i.e. as `%%` — so `${v%[[:space:]]*}` emptied the
    /// string instead of trimming one trailing space.
    #[test]
    fn parameter_trim_patterns_execute_like_bash() {
        let src = "for v in \"  padded  \" \"x y\" \"abc\"; do\n\
                   a=${v##*[![:space:]]}\n\
                   b=${v%%[[:space:]]*}\n\
                   c=${v#*[![:space:]]}\n\
                   d=${v%[[:space:]]*}\n\
                   echo \"[$a][$b][$c][$d]\"\n\
                   done\n\
                   s=\"a:b:c\"\n\
                   echo \"[${s%%:*}][${s##*:}][${s#*:}][${s%:*}][${s##*/}][${s%/*}]\"\n\
                   w=\"foo.tar.gz\"\n\
                   echo \"[${w##*.}][${w%.*}][${w#*.}][${w%%.*}]\"\n\
                   u=\"hello\"\n\
                   echo \"[${u:-def}][${u:+alt}][${u:1}][${u:1:2}][${u:1:-1}]\"\n\
                   unset z\n\
                   echo \"[${z:-fallback}][${z:+nope}]\"\n";
        let mut p = crate::Parser::new(src);
        let cmds = p.parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        let pl = generate(&prog);
        assert!(
            !pl.contains("v##*[!["),
            "the pattern leaked into the variable name:\n{pl}"
        );
        let dir = std::env::temp_dir().join("pgf_trim_patterns");
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let sh_path = dir.join("t.sh");
        let pl_path = dir.join("t.pl");
        std::fs::write(&sh_path, src).expect("write sh");
        std::fs::write(&pl_path, &pl).expect("write pl");
        let sh = std::process::Command::new("bash")
            .arg(&sh_path)
            .output()
            .expect("run bash");
        let got = std::process::Command::new("perl")
            .arg(&pl_path)
            .output()
            .expect("run perl");
        assert_eq!(
            String::from_utf8_lossy(&got.stdout),
            String::from_utf8_lossy(&sh.stdout),
            "stdout differs; perl stderr:\n{}\n--- perl ---\n{pl}",
            String::from_utf8_lossy(&got.stderr)
        );
        assert_eq!(
            got.status.code(),
            sh.status.code(),
            "exit status differs; perl stderr:\n{}",
            String::from_utf8_lossy(&got.stderr)
        );
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

    /// Regression (Fix 22): the slice rewrite must consume the `)` that
    /// closed the ORIGINAL `join(`, or it leaves one paren too many.
    ///
    /// The needle stops at `]` — the character after it is the join's own
    /// closer, and the replacement supplies its own balanced `join(…)`.
    /// Copying that `)` through produced `join(' ', __sh_pos(3))))`, a Perl
    /// COMPILE error, so the program printed nothing at all rather than
    /// merely the wrong value.

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
    ///
    /// Both come from the IR's own preamble now (mission item 5): the CLI's
    /// text layer no longer renames `$ARGV[`/`@ARGV` or splices a binding, so
    /// this test fails if the IR stops emitting them — not if a fixup that no
    /// longer exists is skipped.
    #[test]
    fn toplevel_positional_binding_precedes_its_reads() {
        let out = gen("echo $1\n");
        let decl = out.find("our @__sh_args;").expect("declared");
        let bind = out.find("@__sh_args = @ARGV;").expect("bound");
        let read = out.find("$__sh_args[").expect("read");
        assert!(decl < bind && bind < read, "order wrong in:\n{out}");
        assert!(out.contains("our $__sh_args_n;"), "count declared in:\n{out}");
        assert!(
            out.contains("$__sh_args_n = scalar(@__sh_args);"),
            "count initialized in:\n{out}"
        );
        // `$#` is the length operator, not a sigil: `our $#foo;` is a compile
        // error. Pin the COUNT's name, not merely its presence.
        assert!(!out.contains("our $#"), "undeclarable count in:\n{out}");
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

    /// `@PIPESTATUS` — written by `fix_literal_pipeline_pipestatus` and
    /// read as `$PIPESTATUS[n]` — must still receive its `my`. Without it
    /// `use strict` rejects the WHOLE file at compile time and the program
    /// prints nothing at all (070_gnuisms_thorough regressed exactly this
    /// way: `Global symbol "@PIPESTATUS" requires explicit package name`).
    #[test]
    fn pipestatus_write_and_read_get_declared() {
        let src = "use strict;\nmy $main_exit_code = 0;\nour $CHILD_ERROR = 0;\n@PIPESTATUS = (1, 0); $x = ($PIPESTATUS[0]);\nprint \"ps=$x\\n\";\n";
        let out = declare_undeclared_vars(src);
        assert!(out.contains("my @PIPESTATUS;"), "{out}");
    }

    /// The same requirement end-to-end: a real `false | true` pipeline
    /// with a `${PIPESTATUS[n]}` read must render a compilable file.
    #[test]
    fn pipestatus_declared_in_real_render() {
        let mut p = crate::Parser::new(
            "false | true\necho \"ps=${PIPESTATUS[0]},${PIPESTATUS[1]}\"\n",
        );
        let cmds = p.parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        let out = generate(&prog);
        assert!(out.contains("my @PIPESTATUS;"), "{out}");
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



    // ── Fix 11 — the `for`-over-array iterand ───────────────────────────




    // ── Fix 20 — the GLOB sentinel in a `for` iterand ──────────────────





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
    let rm_at = body.len().checked_sub(1)?;
    let tmps = rm_cleanup(&body[rm_at])?;
    // A consumer may be wrapped in a redirect (`diff <(…) <(…) > out`
    // materialises as a `Redirect` whose inner reads the `__ps_tmp`
    // vars) — producers are also `Redirect`s, but their tmp appears only
    // as a redirect TARGET, never inside the inner statements, so the
    // inner-reference test separates the two exactly.
    let is_consumer = |s: &IrStmt| -> bool {
        if matches!(s, IrStmt::Exec { .. } | IrStmt::Expr(IrExpr::Call { .. })) {
            return true;
        }
        if let IrStmt::Redirect { inner, .. } = s {
            return tmps.iter().any(|t| inner.iter().any(|is| mentions_var(is, t)));
        }
        false
    };
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
        .filter(|(_, s)| is_consumer(s))
        .map(|(i, _)| i)
        .last()?;
    // Exactly ONE consumer: two bare commands means we cannot tell which
    // one's status bash reports, and guessing is worse than the status quo.
    let consumers = body
        .iter()
        .take(rm_at)
        .filter(|s| is_consumer(s))
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

// ── Fix 10 — an INFINITE process-substitution producer runs in the
// background, not to completion ──────────────────────────────────────
//
// `transforms::process_subst` materialises `<(…)` / `>(…)` SEQUENTIALLY:
// the producer fills a temp file, then the consumer runs over it. That is
// equivalent for a producer that terminates — and hangs forever for one
// that does not. `head <(while true; do echo .; sleep 1; done)`
// (096_head_procsub) emitted the `while true` loop as a plain statement
// block redirected at the temp, so the producer never finished and `head`
// never ran: perl=124 (and a 1.1 GB temp file in the yes|head case) against
// bash's 10 lines and exit 0. bash itself never waits for the producer: it
// runs concurrently and the writer dies when the consumer's side closes.
//
// The correction is perl-side and shape-gated: a materialisation block
// whose producer is PROVABLY infinite (a `while true` / `while :` loop —
// or a `do {…} while` — whose body cannot leave it: no break/return/exit
// at any depth). The producer is then FORKED via the renderer's own
// `Background` arm (which already emits the fork and records the child
// pid in `$__sh2_last_bg`), the pid is saved, and after the consumer runs,
// the child is killed and reaped before the temp cleanup. The block still
// reports the CONSUMER's status — the same save/restore Fix 9 performs —
// so `echo "exit: $?"` after the statement keeps printing head's 0.
//
// Every other materialisation (a finite producer, a loop whose exit is
// conditional…) keeps the sequential form, which is equivalent there.
fn fix_ps_infinite_producer(stmts: &[IrStmt]) -> Vec<IrStmt> {
    stmts
        .iter()
        .map(|s| match s {
            IrStmt::Block(b) => IrStmt::Block(fix_ps_infinite_producer_block(b)),
            IrStmt::Subshell(b) => IrStmt::Subshell(fix_ps_infinite_producer(b)),
            IrStmt::Background(b) => IrStmt::Background(fix_ps_infinite_producer(b)),
            IrStmt::If {
                cond,
                then,
                elsifs,
                else_,
            } => IrStmt::If {
                cond: cond.clone(),
                then: fix_ps_infinite_producer(then),
                elsifs: elsifs
                    .iter()
                    .map(|(c, b)| (c.clone(), fix_ps_infinite_producer(b)))
                    .collect(),
                else_: fix_ps_infinite_producer(else_),
            },
            IrStmt::While { cond, body } => IrStmt::While {
                cond: cond.clone(),
                body: fix_ps_infinite_producer(body),
            },
            IrStmt::DoWhile { cond, body, until } => IrStmt::DoWhile {
                cond: cond.clone(),
                body: fix_ps_infinite_producer(body),
                until: *until,
            },
            IrStmt::ForInit {
                init,
                cond,
                step,
                body,
            } => IrStmt::ForInit {
                init: fix_ps_infinite_producer(init),
                cond: cond.clone(),
                step: fix_ps_infinite_producer(step),
                body: fix_ps_infinite_producer(body),
            },
            IrStmt::Function {
                name,
                params,
                body,
                named_blocks,
            } => IrStmt::Function {
                name: name.clone(),
                params: params.clone(),
                body: fix_ps_infinite_producer(body),
                named_blocks: named_blocks
                    .iter()
                    .map(|(n, b)| (n.clone(), fix_ps_infinite_producer(b)))
                    .collect(),
            },
            other => other.clone(),
        })
        .collect()
}

/// One materialisation block: `[mktemp assigns, producer Redirect, …,
/// consumer, rm -f tmp]` with a provably infinite producer — rewrite to
/// fork/kill form. Anything not fully understood is returned unchanged.
fn fix_ps_infinite_producer_block(body: &[IrStmt]) -> Vec<IrStmt> {
    let Some(rm_at) = body.len().checked_sub(1) else {
        return body.to_vec();
    };
    // Trailing `rm -f __ps_tmp…` — the same cleanup Fix 9 keys on.
    let Some(rm_tmps) = ps_rm_cleanup(&body[rm_at]) else {
        return body.to_vec();
    };
    // Every temp must be created by an assignment in this same block —
    // proof this is the materialisation, not a user's own `__ps_tmp` name.
    for t in &rm_tmps {
        let created = body.iter().take(rm_at).any(|s| {
            matches!(s, IrStmt::Assign { targets, .. }
                if targets.iter().any(|tt| &tt.var == t))
        });
        if !created {
            return body.to_vec();
        }
    }
    // The producer: a stdout Redirect into one of the temps whose body is
    // provably infinite.
    let Some((prod_at, prod_tmp)) = body
        .iter()
        .take(rm_at)
        .enumerate()
        .find_map(|(i, s)| match s {
            IrStmt::Redirect { inner, redirects } => redirects.iter().find_map(|r| {
                let writes_tmp = r.fd == Some(1)
                    && r.mode == "w"
                    && matches!(&r.target, IrExpr::Var(n, _) if rm_tmps.contains(n));
                if writes_tmp && stmts_run_forever(inner) {
                    if let IrExpr::Var(n, _) = &r.target {
                        return Some((i, n.clone()));
                    }
                }
                None
            }),
            _ => None,
        })
    else {
        return body.to_vec();
    };
    // The consumer must be the LAST statement before the cleanup: the
    // status saved for the block is taken right there, so anything between
    // them could have clobbered it.
    if rm_at == 0
        || !matches!(
            body[rm_at - 1],
            IrStmt::Exec { .. } | IrStmt::Expr(IrExpr::Call { .. })
        )
    {
        return body.to_vec();
    }

    let pid = fresh_ps_var(body, &rm_tmps, "__ps_bgp");
    let status = fresh_status_var(body, &rm_tmps);
    let mut out: Vec<IrStmt> = Vec::with_capacity(body.len() + 4);
    for (i, s) in body.iter().enumerate() {
        if i == prod_at {
            // The temp becomes a FIFO before the fork. `<(...)` IS a pipe
            // in bash, and a regular temp FILE hands the consumer an
            // immediate EOF at whatever the writer had flushed — `head`
            // printed nothing where bash printed 10 lines (a read on a
            // regular file does not wait for more data; a pipe read does).
            // A fifo restores pipe semantics exactly: the consumer's
            // open/read blocks for the writer's data, and — like bash — it
            // sees EOF only when the writer side is gone. `POSIX::mkfifo`
            // is core Perl, so no fork/exec is involved; the mktemp file
            // is unlinked first (mkfifo refuses an existing path).
            out.push(IrStmt::Require("POSIX".to_string()));
            out.push(IrStmt::RawText(format!(
                "unlink(${prod_tmp}); POSIX::mkfifo(${prod_tmp}, 0600) or die \"mkfifo: $!\\n\";\n"
            )));
            // fork: the Background arm emits the fork, records the pid in
            // `$__sh2_last_bg`, and runs the body in the child. The pid is
            // READ via `getVar("!")` — that is the path `var_read("!")`
            // resolves to `$__sh2_last_bg`; a plain `Var("!")` renders as
            // Perl's errno `$!`, and `kill('TERM', '')` numifies to pid 0
            // — the ENTIRE process group, including this program.
            out.push(IrStmt::Background(vec![s.clone()]));
            out.push(ps_assign(
                &pid,
                IrExpr::Call {
                    func: "getVar".to_string(),
                    args: vec![IrExpr::Str("!".to_string(), crate::ir::StrStyle::DoubleQuoted)],
                },
            ));
            continue;
        }
        if i == rm_at {
            // the CONSUMER's status is what the block must report; kill,
            // reap and the cleanup's own 0 are all overwritten by the
            // restore below.
            out.push(save_status(status.clone()));
            out.push(ps_builtin_call(
                "kill",
                vec![
                    IrExpr::Str("-TERM".to_string(), crate::ir::StrStyle::DoubleQuoted),
                    IrExpr::Var(pid.clone(), None),
                ],
            ));
            out.push(ps_builtin_call(
                "wait",
                vec![IrExpr::Var(pid.clone(), None)],
            ));
            out.push(s.clone());
            out.push(IrStmt::SetChildError(IrExpr::Var(status.clone(), None)));
            continue;
        }
        out.push(s.clone());
    }
    out
}

/// `rm -f __ps_tmp…` cleanup and the temps it removes — Fix 9's local
/// shape check, repeated here so the two fixes stay independent.
fn ps_rm_cleanup(s: &IrStmt) -> Option<Vec<String>> {
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

/// Provably non-terminating: a `while true` / `while :` loop whose body
/// cannot leave it, a `do {…} while` with the same body guarantee (the
/// body always runs and the loop comes back around), or a block holding
/// one at the top level.
fn stmts_run_forever(stmts: &[IrStmt]) -> bool {
    stmts.iter().any(|s| match s {
        IrStmt::While { cond, body } => always_true_cond(cond) && !stmts_may_leave(body),
        IrStmt::DoWhile { body, .. } => !stmts_may_leave(body),
        IrStmt::Block(b) => stmts_run_forever(b),
        _ => false,
    })
}

/// `while true` / `while :` — the command-condition spellings of
/// "unconditional". `exec` covers the pre-builtin-lift shape.
///
/// The CONSTANT spelling counts too: the shared `const-status-cond` transform
/// replaces a `true`/`:` COMMAND in condition position with `Bool(true)` (so
/// the condition stops forking a subprocess), and that is the shape the
/// optimizer hands this pass for `while true; do …; done`. Missing it here
/// meant an infinite process-substitution producer was never recognised as
/// infinite, so it ran to completion in the sequential materialisation and
/// the consumer (`head <(while true; …)` — example 096) never ran at all.
fn always_true_cond(cond: &IrExpr) -> bool {
    match cond {
        IrExpr::Call { func, args, .. } if func == "builtin" || func == "exec" => {
            matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "true" || s == ":")
        }
        // Constant condition: `Bool(true)`, or a non-zero integer literal —
        // the same things a shell treats as success. `Int(0)` is FALSE, so
        // it must not be taken as unconditional (and `while 0` never runs).
        IrExpr::Bool(b) => *b,
        IrExpr::Int(n) => *n != 0,
        _ => false,
    }
}

/// Conservatively: could ANY of these statements end the enclosing loop?
/// A `break` inside an `if` does; a `return`/`exit`/`die` anywhere does.
/// Over-approximating (a case clause, a select, anything unrecognised)
/// only keeps the sequential form — what every producer had before this
/// fix — so the conservative direction can never change behaviour.
fn stmts_may_leave(stmts: &[IrStmt]) -> bool {
    stmts.iter().any(|s| match s {
        IrStmt::Break | IrStmt::Return(_) | IrStmt::Exit(_) | IrStmt::Die { .. } => true,
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => stmts_may_leave(b),
        IrStmt::Redirect { inner, .. } => stmts_may_leave(inner),
        IrStmt::If {
            then, elsifs, else_, ..
        } => {
            stmts_may_leave(then)
                || elsifs.iter().any(|(_, b)| stmts_may_leave(b))
                || stmts_may_leave(else_)
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => stmts_may_leave(body),
        IrStmt::ForInit { body, .. } => stmts_may_leave(body),
        _ => false,
    })
}

/// A variable name no statement in the block can already be using.
fn fresh_ps_var(body: &[IrStmt], tmps: &[String], prefix: &str) -> String {
    let mut n = 0usize;
    loop {
        let cand = format!("{prefix}{n}");
        let taken = body.iter().any(|s| mentions_var(s, &cand))
            || tmps.iter().any(|t| t == &cand);
        if !taken {
            return cand;
        }
        n += 1;
    }
}

fn ps_assign(var: &str, expr: IrExpr) -> IrStmt {
    IrStmt::Assign {
        targets: vec![crate::ir::AssignTarget {
            var: var.to_string(),
            sigil: None,
            indices: vec![],
        }],
        expr,
        asm: None,
    }
}

/// `builtin(cmd, args…)` — the statement shape the renderer's builtin
/// dispatch consumes (echo/sleep/wait/kill all arrive this way).
fn ps_builtin_call(cmd: &str, args: Vec<IrExpr>) -> IrStmt {
    let mut full = vec![IrExpr::Str(
        cmd.to_string(),
        crate::ir::StrStyle::DoubleQuoted,
    )];
    full.push(IrExpr::Array(args));
    IrStmt::Expr(IrExpr::Call {
        func: "builtin".to_string(),
        args: full,
    })
}








#[cfg(test)]
mod positional_text_tests {
    use super::generate;

    fn render_both(src: &str) -> (String, String) {
        let cmds = crate::Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        (crate::ir::shir_to_perl(&prog), generate(&prog))
    }

    /// Mission item 5, the exact bug that motivated it: the printf numeric
    /// helper is GENERATED code containing regex captures. When the CLI
    /// rewrote bare `$1..$4` as positional parameters, this program converted
    /// the wrong string (`realpath-cmdsub` printed exit 0 where bash prints
    /// 1). The helper now uses named captures AND the rewrite no longer
    /// exists, so this fails if either half comes back.
    #[test]
    fn generated_captures_are_not_rewritten_as_positional_params() {
        let (_bare, fixed) = render_both("printf '%d\\n' \"$1\"\n");
        assert!(fixed.contains("$+{dg}"), "named capture missing:\n{fixed}");
        assert!(
            !fixed.contains("$__sh_args[1]"),
            "the printf helper's captures were rewritten as positional params:\n{fixed}"
        );
    }

    /// The other half of the property: a REAL `$1` must still be the first
    /// positional parameter, spelled by the IR (not by a text pass).
    #[test]
    fn a_source_read_of_dollar_one_is_the_first_positional_param() {
        let (bare, fixed) = render_both("echo \"$1\"\n");
        assert!(fixed.contains("$__sh_args[0]"), "{fixed}");
        assert!(bare.contains("$__sh_args[0]"), "{bare}");
        assert!(
            !fixed.contains("$ARGV[0]"),
            "positional read still rendered as @ARGV:\n{fixed}"
        );
        assert!(
            fixed.contains("@__sh_args = @ARGV;"),
            "file-scope binding missing:\n{fixed}"
        );
    }

    /// Text that RESEMBLES a positional reference is DATA: awk's `$1` and a
    /// single-quoted `$ARGV[0] $1` must reach the emitted Perl unchanged, so
    /// no text pass may claim them.
    #[test]
    fn literal_text_that_resembles_a_positional_param_is_data() {
        let (_bare, fixed) = render_both("awk '{print $1}' f\necho '$ARGV[0] $1'\n");
        assert!(fixed.contains("{print $1}"), "awk program rewritten:\n{fixed}");
        assert!(
            fixed.contains("\\$ARGV[0] \\$1"),
            "quoted literal rewritten:\n{fixed}"
        );
    }

    /// The sub binding is emitted by the IR now, so a function sees its OWN
    /// arguments (the `local @__sh_args = @_;` line is the IR's, and the
    /// argument-running form uses the same array).
    #[test]
    fn function_bodies_bind_their_own_positional_parameters() {
        let (_bare, fixed) = render_both("f(){ echo \"$1\"; }\nf x\n");
        assert!(fixed.contains("local @__sh_args = @_;"), "{fixed}");
        assert!(!fixed.contains("local @ARGV = @_;"), "{fixed}");
    }
}

/// The audit that decides each text pass's fate: render the whole corpus once,
/// then once per pass with that pass disabled, and report which renders change.
/// A pass that changes NOTHING is dead weight; a pass whose changed renders are
/// wrong is repairing the IR at the wrong layer. Run with:
///   cargo test --lib audit_each_text_pass -- --ignored --nocapture
#[cfg(test)]
mod text_pass_audit {
    use super::*;

    fn corpus() -> Vec<(String, crate::ir::IrProgram)> {
        let root = env!("CARGO_MANIFEST_DIR");
        let mut names: Vec<(String, String)> = Vec::new();
        for dir in [
            format!("{root}/examples"),
            format!("{root}/tests/tier1"),
            format!("{root}/tests/golden"),
        ] {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("sh") {
                    continue;
                }
                let n = p.file_name().unwrap().to_str().unwrap().to_string();
                names.push((n, p.to_string_lossy().to_string()));
            }
        }
        names.sort();
        let mut out = Vec::new();
        for (n, path) in names {
            // The non-UTF-8 round-trip fixture never reaches this renderer
            // (the CLI takes the byte path), so it is not part of the audit.
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok((cmds, _)) = crate::Parser::new(&text).parse_with_lines() else {
                continue;
            };
            out.push((n, crate::shir::ast_to_ir(&cmds)));
        }
        out
    }

    #[test]
    #[ignore]
    fn audit_each_text_pass() {
        let corpus = corpus();
        println!("corpus: {} renderable examples", corpus.len());
        let baseline: Vec<String> = corpus.iter().map(|(_, p)| generate(p)).collect();
        // Self-check: rendering the SAME program again with no pass disabled
        // must be byte-identical, or the audit is measuring renderer state
        // (counters, temp names) rather than the pass.
        let again: Vec<String> = corpus.iter().map(|(_, p)| generate(p)).collect();
        let unstable: Vec<&String> = corpus
            .iter()
            .zip(baseline.iter().zip(again.iter()))
            .filter(|(_, (a, b))| a != b)
            .map(|((n, _), _)| n)
            .collect();
        println!("unstable rerenders (excluded): {unstable:?}");
        let stable: Vec<(&String, &crate::ir::IrProgram, &String)> = corpus
            .iter()
            .zip(baseline.iter())
            .filter(|((n, _), _)| !unstable.contains(&n))
            .map(|((n, p), b)| (n, p, b))
            .collect();
        for pass in text_passes().iter().chain(declaration_passes().iter()) {
            let mut changed: Vec<String> = Vec::new();
            for (name, prog, base) in stable.iter() {
                if &generate_impl(prog, Some(pass.0)) != *base {
                    changed.push(name.to_string());
                }
            }
            println!(
                "PASS {:<38} changed {:>3}  {}",
                pass.0,
                changed.len(),
                changed.iter().take(4).cloned().collect::<Vec<_>>().join(" ")
            );
        }
    }
}

/// Pin the SEVEN text passes that survive the mission-item-5 audit.
///
/// Each test does the same three things:
///   1. asserts the pass is still in the pipeline table (deleting the entry
///      fails here);
///   2. asserts the full render and the render with that pass disabled DIFFER
///      for a program that exercises it (disabling it fails here);
///   3. asserts the semantic shape the pass is responsible for, so the pass
///      cannot be replaced by something that merely changes bytes.
///
/// The companion audit test (`text_pass_audit::audit_each_text_pass`) prints
/// the corpus-wide effect of every pass; this module is the part that must
/// stay true.
#[cfg(test)]
mod retained_pass_tests {
    use super::*;

    fn render(src: &str) -> (String, crate::ir::IrProgram) {
        let cmds = crate::Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir(&cmds);
        (generate(&prog), prog)
    }

    fn render_example(name: &str) -> (String, crate::ir::IrProgram) {
        let path = format!("{}/examples/{name}.sh", env!("CARGO_MANIFEST_DIR"));
        let src = std::fs::read_to_string(&path).expect("read example");
        render(&src)
    }

    fn in_table(name: &str) -> bool {
        text_passes()
            .iter()
            .chain(declaration_passes().iter())
            .any(|(n, _)| *n == name)
    }

    /// The retained set is exactly these seven. A new pass must come with its
    /// own audit and pin, and a deleted one must be deleted here too.
    #[test]
    fn the_retained_pass_set_is_exactly_these_seven() {
        let mut names: Vec<&str> = text_passes()
            .iter()
            .chain(declaration_passes().iter())
            .map(|(n, _)| *n)
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "declare_arith_operand_flag",
                "declare_undeclared_vars",
                "fix_literal_pipeline_pipestatus",
                "fix_print_concat_interp",
                "fix_print_newline_to_say",
                "fix_printf_b_percent_b",
                "fix_printf_bash_conversions",
            ]
        );
    }

    /// `echo X` → `say X`: the trailing `, "\n"` is folded into `say` and the
    /// flat `.` chain becomes one interpolating literal. Same value, same
    /// newline; only the Perl idiom differs.
    #[test]
    fn print_newline_folds_into_say() {
        assert!(in_table("fix_print_newline_to_say"));
        let (full, prog) = render_example("000__02_output_formatting_commands");
        let off = generate_impl(&prog, Some("fix_print_newline_to_say"));
        assert_ne!(full, off, "the say fold no longer fires");
        assert!(full.contains("say \"Echo result: Hello from backticks\";"), "{full}");
        assert!(
            off.contains("print(\"Echo result: \" . (\"Hello from backticks\"), \"\\n\");"),
            "the pre-pass shape is not what the pin describes:\n{off}"
        );
    }

    /// A flat concat chain becomes one interpolating literal — spelling only.
    #[test]
    fn print_concat_chain_interpolates() {
        assert!(in_table("fix_print_concat_interp"));
        let (full, prog) = render_example("055_factorize");
        let off = generate_impl(&prog, Some("fix_print_concat_interp"));
        assert_ne!(full, off, "the concat chain fold no longer fires");
        assert!(full.contains("print \"Factors of $n: \";"), "{full}");
        assert!(off.contains("print(\"Factors of \" . ($n) . \": \");"), "{off}");
    }

    /// A literal `true | false` pipeline: the per-stage statuses bash records
    /// must be in `@PIPESTATUS` before the run, because `__sh_pipe_run` only
    /// reports the final one.
    #[test]
    fn literal_pipeline_fills_pipestatus() {
        assert!(in_table("fix_literal_pipeline_pipestatus"));
        let (full, prog) = render("true | false\necho \"${PIPESTATUS[0]},${PIPESTATUS[1]}\"\n");
        let off = generate_impl(&prog, Some("fix_literal_pipeline_pipestatus"));
        assert!(full.contains("@PIPESTATUS = (0, 1); $? = (__sh_pipe_run("), "{full}");
        assert!(!off.contains("@PIPESTATUS = (0, 1);"), "{off}");
    }

    /// bash's `printf %q` has no Perl conversion: the emitted format is
    /// rewritten to the `__sh2_q` helper and the helper is declared.
    #[test]
    fn printf_percent_q_uses_the_shell_quoting_helper() {
        assert!(in_table("fix_printf_bash_conversions"));
        let (full, prog) = render_example("parse-dollar-single-quote");
        let off = generate_impl(&prog, Some("fix_printf_bash_conversions"));
        assert_ne!(full, off, "the %q rewrite no longer fires");
        assert!(full.contains("sub __sh2_q {"), "{full}");
        // The FORMAT now calls the helper; the literal `%q` conversion must be
        // gone from the emitted call (it is not a Perl sprintf conversion).
        assert!(!full.contains("printf(\"%q"), "{full}");
        assert!(!off.contains("sub __sh2_q"), "{off}");
    }

    /// bash's `printf %b` expands backslash escapes in the OPERAND; the helper
    /// does that, and the format is rewritten to call it.
    #[test]
    fn printf_percent_b_expands_the_operand() {
        assert!(in_table("fix_printf_b_percent_b"));
        let (full, prog) = render_example("printf-format-and-v");
        let off = generate_impl(&prog, Some("fix_printf_b_percent_b"));
        assert_ne!(full, off, "the %b rewrite no longer fires");
        assert!(full.contains("sub __sh2_b {"), "{full}");
        assert!(!off.contains("sub __sh2_b"), "{off}");
    }

    /// Declaration-only passes: they cannot change a value, only make the
    /// emitted Perl strict-clean. Each is asserted on the shape it adds.
    #[test]
    fn undeclared_reads_are_declared() {
        assert!(in_table("declare_undeclared_vars"));
        let out = declare_undeclared_vars("use strict;\nprint($nope);\n");
        assert!(out.contains("my $nope;"), "{out}");
        // The pass is load-bearing exactly for a read the program never
        // binds: without the declaration, `use strict` rejects the file.
        assert!(!declare_undeclared_vars("use strict;\nmy $x = 1;\nprint $x;\n").contains("my $x;\nmy $x"));
    }

    #[test]
    fn arith_operand_flag_is_declared() {
        assert!(in_table("declare_arith_operand_flag"));
        let out = declare_arith_operand_flag("use strict;\nmy $x = 1 && ($__SH2_AE = 1);\n");
        assert!(out.contains("our $__SH2_AE;"), "{out}");
        // Idempotent: an already-declared flag gets no second `our`.
        assert_eq!(out.matches("our $__SH2_AE;").count(), 1);
    }
}
