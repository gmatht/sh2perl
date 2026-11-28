//! The Perl renderer's STATUS-REGISTER idiom.
//!
//! The rendered status bookkeeping used to be a private tracker pair —
//! `$main_exit_code = $CHILD_ERROR = 0;` on every statement — that nothing
//! but the epilogue read. The register both variables described is Perl's
//! own `$?` (a wait status: the shell code shifted left by 8), so each write
//! is now spelled as that register update and the epilogue exits with
//! `$? >> 8`. `$CHILD_ERROR` stays the shell CODE, which is what a source
//! `$?` read maps to.
//!
//! These tests pin both halves: the shape of the output, and the semantics
//! (`$? = N` must be shifted, or the code reads back as 0).

use debashl::ir::*;

fn render(src: &str) -> String {
    let (commands, _) = debashl::Parser::new(src).parse_with_lines().expect("parse");
    let prog: IrProgram = debashl::shir::ast_to_ir(&commands);
    debashl::perl_generator_fixed::generate(&prog)
}

/// The tracker pair is gone; the register carries the status.
#[test]
fn status_writes_are_register_updates() {
    let out = render("echo one\necho two\n");
    assert!(!out.contains("$main_exit_code ="), "tracker write survived:\n{out}");
    assert!(out.contains("$? = 0"), "no register write:\n{out}");
    assert!(out.contains("exit $? >> 8;"), "epilogue does not read the register:\n{out}");
}

/// A shell CODE written to `$?` must be SHIFTED: Perl assigns `$?` directly,
/// so `$? = 1` reads back as code 0 (measured).
#[test]
fn status_codes_are_shifted_into_the_register() {
    let out = render("false\n");
    assert!(out.contains("<< 8"), "unshifted register write:\n{out}");
}

/// A source `$?` read still sees the shell CODE.
///
/// Two carriers are valid and the renderer picks by liveness: the
/// `$CHILD_ERROR` mirror (refreshed by every register write) for programs
/// that move the status themselves, or a shifted read of the register
/// (`($? >> 8)`) when the mirror would be dead. An UNSHIFTED `$?` read is the
/// bug this pins — Perl's `$?` is a wait status, so it would print 256.
///
/// The text check says a shifted carrier exists; the runtime check says the
/// value is the one bash prints, which is what a script branches on.
#[test]
fn a_source_status_read_keeps_the_code() {
    let src = "false\necho \"rc=$?\"\n";
    let out = render(src);
    assert!(
        out.contains("$CHILD_ERROR") || out.contains("$? >> 8"),
        "the $? read lost its carrier:\n{out}"
    );
    assert!(
        !out.contains("print(\"rc=\" . ($?),"),
        "an UNSHIFTED $? read survived (it prints the wait status):\n{out}"
    );

    // Behaviour: bash and the render must print the same line.
    let dir = std::env::temp_dir();
    let stamp = std::process::id();
    let sh = dir.join(format!("sh2_status_read_{stamp}.sh"));
    let pl = dir.join(format!("sh2_status_read_{stamp}.pl"));
    std::fs::write(&sh, src).expect("write script");
    std::fs::write(&pl, &out).expect("write render");
    let run = |cmd: &mut std::process::Command| {
        let o = cmd.output().expect("run");
        String::from_utf8_lossy(&o.stdout).into_owned()
    };
    let b = run(std::process::Command::new("bash").arg(&sh));
    let p = run(std::process::Command::new("perl").arg(&pl));
    let _ = std::fs::remove_file(&sh);
    let _ = std::fs::remove_file(&pl);
    assert_eq!(b, p, "a `$?` read diverged from bash");
    assert_eq!(b.trim(), "rc=1", "bash reference itself moved: {b:?}");
}

/// `((i++))` writes a status that must not be clobbered by precedence:
/// `a ? 0 : 1 << 8` would shift only the `1` branch.
#[test]
fn ternary_status_shift_is_parenthesised() {
    let out = render("i=0\n((i++))\necho \"post=$?\"\n");
    assert!(
        !out.contains("? 0 : 1 << 8"),
        "the shift bound to one branch only:\n{out}"
    );
}

/// `print($a . " " . $b)` is rendered as one interpolated string.
#[test]
fn print_concat_becomes_interpolation() {
    let out = render("a=1\nb=2\necho \"$a $b\"\n");
    assert!(!out.contains("print(($a) . "), "concat form survived:\n{out}");
}

/// A print whose list has a top-level comma is left alone (its trailing
/// `, "\n"` is the renderer's explicit newline).
#[test]
fn print_with_comma_is_untouched() {
    let out = render("v=x\necho \"k=$v\"\n");
    assert!(
        !out.contains("print \"k=") || out.contains(","),
        "a comma list was re-joined:\n{out}"
    );
}

/// `print(X, "\n")` — the renderer's explicit-newline output — is `say X`,
/// and the `say` feature is imported when the renderer had not already.
#[test]
fn print_with_newline_is_say() {
    let out = render("i=3\necho \"Number: $i\"\n");
    assert!(out.contains("say "), "no say:\n{out}");
    assert!(!out.contains(", \"\\n\")"), "explicit newline argument survived:\n{out}");
    assert!(
        out.contains("use feature 'say'"),
        "say used without the feature:\n{out}"
    );
}

/// A concat chain in `say` becomes one interpolated string.
#[test]
fn say_interpolates_a_concat_chain() {
    let out = render("x=abc\necho \"x is $x\"\n");
    assert!(out.contains("say \"x is abc\""), "got:\n{out}");
}
