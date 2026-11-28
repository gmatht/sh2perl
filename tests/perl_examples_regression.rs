//! The Perl renderer's PERMANENT regression suite.
//!
//! Three guarantees, each of which a silent emitter change would otherwise
//! break without any test noticing:
//!
//! 1. **Golden text.** A representative dozen `examples/*.sh` are compared
//!    byte-for-byte against committed fixtures under `tests/golden/examples/`.
//!    Escaping, idiom, naming and status-shadow changes show up as a reviewable
//!    diff instead of as a silent rewrite. Regenerate a fixture with
//!    `target/debug/perl_backend --fixed examples/<name>.sh > tests/golden/examples/<name>.pl`
//!    after an INTENTIONAL emitter change, and re-check it against bash.
//! 2. **Every corpus example compiles under `perl -w -c`,** through BOTH entry
//!    points (`ir::shir_to_perl` and the shipped
//!    `perl_generator_fixed::generate`), with exit 0 and NOTHING on stderr but
//!    the optional `uu-ffi` notice. A warning is a failure: the
//!    "Useless use of numeric eq in void context" that once shipped is the
//!    proof that this signal catches real defects.
//! 3. **The native-capture branch is exercised.** `000__02`'s captures all
//!    constant-fold to literals, so the `local *STDOUT` lowering never ran
//!    there; `examples/native_capture.sh` has a body that cannot fold, and the
//!    test asserts the emitted shape (so the branch cannot ship inert).
//!
//! The `parse-*` examples are deliberately MALFORMED inputs (they pin parser
//! diagnostics), so the sweep requires them to fail to PARSE rather than to
//! compile.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// `examples/*.sh` pinned as golden text (the SHIPPED `--fixed` path).
const EXAMPLE_GOLDENS: &[&str] = &[
    "090_nested_if_else",
    "param-expand-default-operator",
    "002_control_flow",
    "010_substring_loop",
    "064_13_complex_string_manipulation",
    "000__07_find_path_commands",
    "000__04a_basic_command_substitution",
    "000__04b_file_directory_operations",
    "000__04h_complex_examples",
    "test-expression-backslash-continuation",
    "unclosed-param-expansion",
    "native_capture",
];

fn examples_dir() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/examples").to_string()
}

fn fixture_dir() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/examples").to_string()
}

/// The shipped render, with the SOURCE path installed as `$0` (what the CLI
/// does). The goldens are produced the same way, so a script that prints `$0`
/// records the script name, not `x.pl`.
fn render_fixed(src: &str, name: &str) -> String {
    let (commands, _) = debashl::Parser::new(src).parse_with_lines().expect("parse");
    let prog = debashl::shir::ast_to_ir(&commands);
    debashl::perl_generator_fixed::generate_named(&prog, &format!("examples/{name}.sh"))
}

fn render_bare(src: &str) -> String {
    let (commands, _) = debashl::Parser::new(src).parse_with_lines().expect("parse");
    let prog = debashl::shir::ast_to_ir(&commands);
    debashl::ir::shir_to_perl(&prog)
}

/// Fixtures that exist PRECISELY to pin the parser's error reporting: they
/// must keep failing to PARSE, and never reach the renderer.
const MUST_NOT_PARSE: &[&str] = &[
    "parse-double-semicolon.sh",
    "parse-error-doublesemicolon.sh",
    "parse-parameter-expansion-eof.sh",
    "parse-paren-after-do.sh",
    "parse-unexpected-end-of-input.sh",
];

fn have_perl() -> bool {
    std::process::Command::new("perl")
        .arg("-v")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ── 1. Golden text ──────────────────────────────────────────────────────

#[test]
fn example_renders_match_their_committed_golden() {
    let mut failures = String::new();
    for name in EXAMPLE_GOLDENS {
        let src = std::fs::read_to_string(format!("{}/{}.sh", examples_dir(), name))
            .unwrap_or_else(|e| panic!("read {name}.sh: {e}"));
        let expected = std::fs::read_to_string(format!("{}/{}.pl", fixture_dir(), name))
            .unwrap_or_else(|e| panic!("read golden {name}.pl: {e}"));
        let got = render_fixed(&src, name);
        if got != expected {
            // A compact diff: the first differing line, both sides.
            let mut msg = format!("golden drift for examples/{name}.sh\n");
            for (i, (a, b)) in expected.lines().zip(got.lines()).enumerate() {
                if a != b {
                    msg.push_str(&format!("  line {}:\n    fixture: {a}\n    render:  {b}\n", i + 1));
                    break;
                }
            }
            if expected.lines().count() != got.lines().count() {
                msg.push_str(&format!(
                    "  fixture has {} lines, render has {}\n",
                    expected.lines().count(),
                    got.lines().count()
                ));
            }
            failures.push_str(&msg);
        }
    }
    assert!(failures.is_empty(), "{failures}");
}

// ── 2. Whole-corpus `perl -w -c` sweep ──────────────────────────────────

/// Compile one rendered program; `None` when it is clean, else the
/// diagnostic lines (the optional `uu-ffi` notice is not a diagnostic).
fn compile_warnings(perl: &str) -> Option<String> {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "sh2_sweep_{}_{}.pl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::write(&path, perl).expect("write temp perl");
    let out = std::process::Command::new("perl")
        .args(["-w", "-c"])
        .arg(&path)
        .output()
        .expect("run perl");
    let _ = std::fs::remove_file(&path);
    let mut bad = String::new();
    if !out.status.success() {
        bad.push_str(&format!(
            "perl -c failed ({:?})\n{}\n",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    for line in String::from_utf8_lossy(&out.stderr).lines() {
        let line = line.trim();
        // `perl -c` announces success as "<file> syntax OK" — on stdout in
        // some builds and on stderr in others; either way it is not a
        // diagnostic. The `uu-ffi` notice is the renderer's own optional-
        // library message, printed from the preamble at compile time.
        if line.is_empty() || line.contains("sh2perl: uu-ffi ") || line.ends_with("syntax OK") {
            continue;
        }
        bad.push_str(&format!("stderr: {line}\n"));
    }
    if bad.is_empty() {
        None
    } else {
        Some(bad)
    }
}

#[test]
fn every_example_compiles_without_warnings_through_both_entry_points() {
    if !have_perl() {
        eprintln!("skip: perl not installed");
        return;
    }
    let dir = examples_dir();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("examples dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".sh"))
        .collect();
    names.sort();
    assert!(names.len() > 500, "corpus looks truncated: {}", names.len());

    let files: Arc<Vec<(String, String)>> = Arc::new(
        names
            .iter()
            .filter_map(|n| {
                std::fs::read_to_string(format!("{dir}/{n}"))
                    .ok()
                    .map(|s| (n.clone(), s))
            })
            .collect(),
    );

    // Parallel workers: the render path is thread-local end to end, so
    // concurrent renders must be equivalent to serial ones (see
    // `perl_render_purity`). Four keeps the perl process count reasonable.
    //
    // The workers use the DEFAULT thread stack on purpose. They used to ask
    // for 256 MB, which hid a real defect: `ir_expr_to_perl` recursed once
    // per brace-expanded word, so `064_hard_to_generate.sh` needed ~15 MB and
    // the shipped CLI aborted with a stack overflow while this sweep passed.
    // A renderer that cannot fit the default stack is the bug — raising the
    // stack here only hides it again.
    let next = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(std::sync::Mutex::new(String::new()));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let files = Arc::clone(&files);
        let next = Arc::clone(&next);
        let failures = Arc::clone(&failures);
        handles.push(
            std::thread::Builder::new()
                .spawn(move || {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some((name, src)) = files.get(i) else { break };
                        // A handful of fixtures exist PRECISELY to pin the
                        // parser's error reporting; they must keep failing.
                        // Every other example must parse and render.
                        let (commands, _) = match debashl::Parser::new(src).parse_with_lines() {
                            Ok(c) => c,
                            Err(e) => {
                                if !MUST_NOT_PARSE.contains(&name.as_str()) {
                                    failures.lock().unwrap().push_str(&format!(
                                        "{name}: parse failed (not a parse-error fixture): {e}\n"
                                    ));
                                }
                                continue;
                            }
                        };
                        if MUST_NOT_PARSE.contains(&name.as_str()) {
                            failures.lock().unwrap().push_str(&format!(
                                "{name}: a parse-error fixture parsed successfully\n"
                            ));
                            continue;
                        }
                        let prog = debashl::shir::ast_to_ir(&commands);
                        for (which, perl) in [
                            ("bare", debashl::ir::shir_to_perl(&prog)),
                            ("fixed", debashl::perl_generator_fixed::generate(&prog)),
                        ] {
                            if let Some(bad) = compile_warnings(&perl) {
                                failures
                                    .lock()
                                    .unwrap()
                                    .push_str(&format!("{name} [{which}]:\n{bad}\n"));
                            }
                        }
                    }
                })
                .expect("spawn sweep worker"),
        );
    }
    for h in handles {
        h.join().expect("sweep worker");
    }
    let failures = Arc::try_unwrap(failures)
        .expect("no other holders")
        .into_inner()
        .expect("lock");
    assert!(
        failures.is_empty(),
        "examples that do not compile cleanly:\n{failures}"
    );
}

// ── 2b. The SHIPPED CLI renders on a default stack ──────────────────────

/// The sweep above renders in-process (and now on default stack threads).
/// This pins the other half of the same guarantee: the built `perl_backend`
/// binary — the path `tests/bash-parity.sh` and users run — must render every
/// corpus example on a fresh process's DEFAULT stack.
///
/// `examples/064_hard_to_generate.sh` used to abort there with `thread 'main'
/// has overflowed its stack` (exit 134) while the sweep passed, because the
/// sweep had asked for a 256 MB stack. Rendering in a SUBPROCESS keeps that
/// class visible without killing the whole test binary: a crash becomes a
/// named failure that says which example, and the rest still run.
#[test]
fn shipped_cli_renders_every_example_on_the_default_stack() {
    let cli = env!("CARGO_BIN_EXE_perl_backend");
    let dir = examples_dir();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("examples dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".sh"))
        .collect();
    names.sort();
    assert!(names.len() > 500, "corpus looks truncated: {}", names.len());

    let mut failures = String::new();
    for name in &names {
        let out = std::process::Command::new(cli)
            .arg("--fixed")
            .arg(format!("{dir}/{name}"))
            .output()
            .expect("run perl_backend");
        let status = out.status.code().unwrap_or(-1);
        if MUST_NOT_PARSE.contains(&name.as_str()) {
            // Parser-diagnostic fixtures: the CLI reports and exits 2.
            if status != 2 {
                failures.push_str(&format!("{name}: parse-error fixture exited {status}\n"));
            }
            continue;
        }
        if status != 0 {
            let why = if status == 134 {
                " (stack overflow / abort)"
            } else {
                ""
            };
            failures.push_str(&format!("{name}: exit {status}{why}\n"));
            continue;
        }
        if out.stdout.is_empty() {
            failures.push_str(&format!("{name}: rendered nothing\n"));
        }
    }
    assert!(
        failures.is_empty(),
        "the shipped CLI does not render these examples on a default stack:\n{failures}"
    );
}

// ── 3. The native-capture branch is exercised ──────────────────────────

/// `000__02`'s captures constant-fold, so this branch used to ship
/// unexercised. The example's body depends on a variable and on two `echo`
/// builtins, so it must render the native `local *STDOUT` capture — never a
/// folded literal — from BOTH entry points.
#[test]
fn native_capture_example_renders_the_local_stdout_shape() {
    let src = std::fs::read_to_string(format!("{}/native_capture.sh", examples_dir()))
        .expect("read native_capture.sh");
    for (which, perl) in [
        ("bare", render_bare(&src)),
        ("fixed", render_fixed(&src, "native_capture")),
    ] {
        assert!(
            perl.contains("local *STDOUT"),
            "{which}: native capture did not render the local *STDOUT shape:\n{perl}"
        );
        assert!(
            !perl.contains("\"hi\\nsecond\\n\""),
            "{which}: the capture body was constant-folded:\n{perl}"
        );
    }
    // And the committed fixture pins it, so a change to the branch is a diff.
    let fixture = std::fs::read_to_string(format!("{}/native_capture.pl", fixture_dir()))
        .expect("read golden native_capture.pl");
    assert!(
        fixture.contains("local *STDOUT"),
        "golden fixture lost the native capture shape"
    );
}
