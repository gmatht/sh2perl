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

/// A source `$?` read still sees the shell CODE: the mirror
/// `$CHILD_ERROR` is refreshed by every register write.
#[test]
fn a_source_status_read_keeps_the_code() {
    let out = render("false\necho \"rc=$?\"\n");
    assert!(out.contains("$CHILD_ERROR"), "the $? read lost its carrier:\n{out}");
    assert!(
        out.contains("$CHILD_ERROR = $? >> 8"),
        "the mirror is not refreshed on write:\n{out}"
    );
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
