//! PARITY between the two Perl entry points.
//!
//! `ir::shir_to_perl` is the single renderer; `perl_generator_fixed::generate`
//! is the shipped `--target pl` path and wraps it (IR pre-pass + text
//! post-pass), rendering TWICE when it needs the `$CHILD_ERROR` mirror kept
//! in step. That wrapper is where a duplicated lowering silently drifts, so
//! every native builtin lowering (cd, rm, mkdir, touch, printf, test) is
//! exercised here in every position that can hold a command — statement,
//! if/elif condition, while/until condition, negated `!`, and either side of
//! `&&`/`||` — in both success and failure form, and the two renders are
//! compared.
//!
//! Three differences are DOCUMENTED and normalised away before comparing:
//!
//!   * the `$CHILD_ERROR` mirror (`…; $CHILD_ERROR = $? >> 8;` plus the
//!     `our $CHILD_ERROR = 0;` declaration) — the fixed path keeps the shell
//!     CODE mirror in step for programs that move the status themselves;
//!   * the output idiom (`print(…)`/`say …`) — `perl_generator_fixed`'s
//!     print-to-say and interpolation passes rewrite output statements only;
//!   * blank lines and the `use feature 'say';` the say pass injects.
//!
//! Anything else is drift and fails. The native-builtin body itself (`chdir`,
//! `warn`, `unlink`, `mkdir`, `utime`, the `printf` operand validation, the
//! `$? = N << 8` status write) must be byte-identical in both paths, must
//! never fork a child (`system(`, `bash', '-c'`) and must never `die`.
//!
//! # Parallel tests
//!
//! This file used to be ONE serial `#[test]`: the renderer's status-write
//! liveness verdict lived in a PROCESS-GLOBAL slot (`shir::LASTEXIT_UNREAD`, a
//! `Mutex` keyed by statement pointers), so two renders running concurrently —
//! exactly what `cargo test` does with several `#[test]` functions — read each
//! other's verdict set and emitted different text for the same program. That
//! slot is now THREAD-LOCAL and reset per render (like `LOOP_STATUS_SEQ` and
//! the printf scratch counters), and every other render-seam counter/verdict
//! was converted with it, so a render depends only on its input. The contract
//! is therefore split into one `#[test]` per command position, and they run
//! concurrently on purpose: `tests/perl_render_purity.rs` proves the same
//! property directly (repeated, interleaved and 8-thread concurrent renders).

use debashl::ir::{shir_to_perl, IrProgram};

fn to_ir(src: &str) -> IrProgram {
    let (commands, _) = debashl::Parser::new(src).parse_with_lines().expect("parse");
    debashl::shir::ast_to_ir(&commands)
}

/// The documented differences, removed; anything left is drift.
fn normalised(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed == "our $CHILD_ERROR = 0;"
            || trimmed == "use feature 'say';"
        {
            continue;
        }
        // The mirror write trails a register write on the same line.
        let line = line.replace(" $CHILD_ERROR = $? >> 8;", "");
        // Output statements are the post-pass's business (print → say).
        let lead = line.trim_start();
        if lead.starts_with("print(") || lead.starts_with("print ") || lead.starts_with("say ") {
            out.push(format!("{}<OUTPUT>", &line[..line.len() - lead.len()]));
            continue;
        }
        out.push(line);
    }
    out
}

fn diff_report(name: &str, raw: &str, fixed: &str) -> String {
    let a = normalised(raw);
    let b = normalised(fixed);
    let mut msg = format!("=== {name}: native lowering differs between emitters\n");
    for i in 0..a.len().max(b.len()) {
        let l = a.get(i).map(String::as_str).unwrap_or("<missing>");
        let r = b.get(i).map(String::as_str).unwrap_or("<missing>");
        if l != r {
            msg.push_str(&format!("  raw:   {l}\n  fixed: {r}\n"));
        }
    }
    msg
}

/// name | command word (success form, failure form).
const BUILTINS: &[(&str, &str, &str)] = &[
    ("cd", "cd /tmp", "cd /no/such"),
    ("rm", "rm -f sh2_par_missing", "rm /no/such/dir/f"),
    ("mkdir", "mkdir sh2_par_dir", "mkdir /no/such/dir"),
    ("touch", "touch sh2_par_file", "touch /no/such/dir/t"),
    ("printf", "printf '%s\\n' ok", "printf '%d\\n' zz"),
    ("printf_cycle", "printf '%s\\n' a b c", "printf '%d\\n' 1 zz"),
    ("test", "test -n abc", "test -n ''"),
    ("bracket", "[ -n abc ]", "[ -z abc ]"),
];

/// Every position that can hold a command word.
const POSITIONS: &[(&str, &str)] = &[
    ("stmt", "{c}"),
    ("stmt_status", "{c}\necho rc=$?"),
    (
        "stmt_ifstatus",
        "{c}\nif [ $? -eq 0 ]; then echo zero; else echo nonzero; fi",
    ),
    ("if", "if {c}; then echo yes; else echo no; fi"),
    (
        "elif",
        "if false; then :; elif {c}; then echo yes; else echo no; fi",
    ),
    (
        "while",
        "n=0\nwhile {c}; do n=$((n+1)); [ $n -ge 1 ] && break; done\necho n=$n",
    ),
    (
        "until",
        "n=0\nuntil {c}; do n=$((n+1)); [ $n -ge 2 ] && break; done\necho n=$n",
    ),
    ("not", "if ! {c}; then echo yes; else echo no; fi"),
    ("and", "{c} && echo and-ok"),
    ("or", "{c} || echo or-ok"),
    ("andrhs", "true && {c}\necho rc=$?"),
    ("orhs", "false || {c}\necho rc=$?"),
    ("subshell", "( {c} )\necho rc=$?"),
];

fn cases() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (bin, ok, bad) in BUILTINS {
        for (form, cmd) in [("ok", *ok), ("bad", *bad)] {
            for (pos, tpl) in POSITIONS {
                out.push((
                    format!("{bin}_{form}__{pos}"),
                    format!("{}\n", tpl.replace("{c}", cmd)),
                ));
            }
        }
    }
    out
}

/// The cases for ONE command position.
fn cases_in(pos: &str) -> Vec<(String, String)> {
    let suffix = format!("__{pos}");
    cases()
        .into_iter()
        .filter(|(name, _)| name.ends_with(&suffix))
        .collect()
}

/// The parity contract for one command position.
fn check_position(label: &str, cases: Vec<(String, String)>) {
    assert!(!cases.is_empty(), "no cases for position {label}");
    let mut same: Vec<String> = Vec::new();
    let mut differing: Vec<String> = Vec::new();
    let mut failures = String::new();

    for (name, src) in cases {
        let prog = to_ir(&src);
        let raw = shir_to_perl(&prog);
        let fixed = debashl::perl_generator_fixed::generate(&prog);

        // 1. The native lowering itself: identical modulo the documented
        //    mirror / output-idiom differences.
        if normalised(&raw) == normalised(&fixed) {
            same.push(name.clone());
        } else {
            differing.push(name.clone());
            failures.push_str(&diff_report(&name, &raw, &fixed));
        }

        // 2. Both are NATIVE: no fork for a builtin, no `die` on failure.
        for (which, perl) in [("raw", &raw), ("fixed", &fixed)] {
            if perl.contains("system(") || perl.contains("bash', '-c'") {
                failures.push_str(&format!("{name}: {which} forks\n"));
            }
            if perl.contains("or die") || perl.contains("die \"") {
                failures.push_str(&format!("{name}: {which} dies\n"));
            }
        }

        // 3. The status shadow is a MIRROR, not a second lowering. The raw
        //    renderer may write `$CHILD_ERROR` as a VALUE (the `||`-guarded
        //    test chain does: `$CHILD_ERROR = (test) ? 0 : 1`), but it never
        //    carries the mirror update.
        if raw.contains("$CHILD_ERROR = $? >> 8") {
            failures.push_str(&format!("{name}: raw render carries the mirror\n"));
        }
        if fixed.contains("$CHILD_ERROR = $? >> 8") && !fixed.contains("our $CHILD_ERROR = 0;") {
            failures.push_str(&format!("{name}: mirror used without its declaration\n"));
        }
        let raw_writes = raw.matches("$? = ").count();
        let fixed_writes = fixed.matches("$? = ").count();
        if raw_writes != fixed_writes {
            failures.push_str(&format!(
                "{name}: register writes differ (raw {raw_writes}, fixed {fixed_writes})\n"
            ));
        }

        // 4. Neither entry point may carry state between renders. This caught
        //    the printf scratch names (`__sh2_pf_bad0` / `__sh2_pa0`) coming
        //    from a process-global counter: the wrapper's second render — the
        //    one that ships — renamed them `…1`.
        let again = shir_to_perl(&to_ir(&src));
        if again != raw {
            failures.push_str(&format!("{name}: shir_to_perl is not reproducible\n"));
        }
        let fixed_again = debashl::perl_generator_fixed::generate(&to_ir(&src));
        if fixed_again != fixed {
            failures.push_str(&format!("{name}: generate is not reproducible\n"));
        }
    }

    assert!(
        !same.is_empty() && differing.is_empty(),
        "emitters disagree on {} of {} cells ({label})\n{failures}",
        differing.len(),
        same.len() + differing.len()
    );
}

// One test per position. They run concurrently, which is the point: the
// render path is thread-local end to end, so every cell must be stable when
// several renders are in flight (see the module note).
#[test]
fn parity_stmt() {
    check_position("stmt", cases_in("stmt"));
}

#[test]
fn parity_stmt_status() {
    check_position("stmt_status", cases_in("stmt_status"));
}

#[test]
fn parity_stmt_ifstatus() {
    check_position("stmt_ifstatus", cases_in("stmt_ifstatus"));
}

#[test]
fn parity_if() {
    check_position("if", cases_in("if"));
}

#[test]
fn parity_elif() {
    check_position("elif", cases_in("elif"));
}

#[test]
fn parity_while() {
    check_position("while", cases_in("while"));
}

#[test]
fn parity_until() {
    check_position("until", cases_in("until"));
}

#[test]
fn parity_not() {
    check_position("not", cases_in("not"));
}

#[test]
fn parity_and() {
    check_position("and", cases_in("and"));
}

#[test]
fn parity_or() {
    check_position("or", cases_in("or"));
}

#[test]
fn parity_andrhs() {
    check_position("andrhs", cases_in("andrhs"));
}

#[test]
fn parity_orhs() {
    check_position("orhs", cases_in("orhs"));
}

#[test]
fn parity_subshell() {
    check_position("subshell", cases_in("subshell"));
}

/// The parser is the one the CLI uses; keep this test's assumption honest.
#[test]
fn parses_via_the_cli_path() {
    assert!(debashl::Parser::new("cd /tmp\n").parse().is_ok());
}
