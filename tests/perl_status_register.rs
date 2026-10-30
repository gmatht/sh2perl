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

/// A source `$?` read still sees the shell CODE (`$CHILD_ERROR`).
#[test]
fn a_source_status_read_keeps_the_code() {
    let out = render("false\necho \"rc=$?\"\n");
    assert!(
        out.contains("$CHILD_ERROR") && out.contains("rc="),
        "the $? read lost its carrier:\n{out}"
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
