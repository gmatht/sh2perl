//! BEHAVIOUR + TEXT lock for the Tier-1 correctness classes.
//!
//! `bash tests/bash-parity.sh --all`, the `perl -c -w` sweep and every
//! bash-vs-Perl diff used to be things a human ran by hand, so a regression in
//! command-condition truthiness, a lost native-builtin `$?`, a compound
//! position that forks, or a nested `$()` swallowed by `qx` escaping would
//! ship unnoticed by `cargo test`. This file makes that net automatic.
//!
//! Corpus (see `corpus()`):
//!   * `tests/tier1/*.sh` — the fixtures written for those classes:
//!     nested `$()` inside backticks, command conditions, natively lowered
//!     builtins in compound/condition position, and `$?` after every builtin;
//!   * a dozen `examples/*.sh` chosen for capture, redirection, here-docs,
//!     arithmetic, brace expansion, functions and quoting.
//!
//! Three independent assertions per corpus entry:
//!
//!   1. TEXT: the SHIPPED `perl_generator_fixed::generate` render equals the
//!      committed golden, so an escaping/idiom change is a reviewable diff.
//!      Regenerate tier1 goldens with
//!        SH2_UPDATE_SNAPSHOTS=1 cargo test --test renderer_behaviour
//!      (the example goldens under `tests/golden/examples/` are shared with
//!      `perl_examples_regression`; regenerate those with the CLI:
//!      `target/debug/perl_backend --fixed examples/<name>.sh`).
//!   2. COMPILES CLEAN: `perl -w -c` exits 0 with no diagnostic at all — in
//!      particular no `Useless use … in void context` (a status compared in
//!      void context, the inverted-condition seam) and no `masks earlier
//!      declaration` (a duplicated runtime helper).
//!   3. BEHAVES: bash and the emitted Perl are run in two temp dirs with the
//!      SAME basename and their stdout, stderr and exit status compared
//!      byte-for-byte — for every entry, with no per-entry exemption. This is
//!      what makes the goldens known-GOOD rather than merely stable.
//!
//! Normalisation, kept to what `tests/bash-parity.sh` already documents:
//!   * the two temp roots are rewritten to `<WORK>` (an example may print
//!     `pwd`; the roots differ by construction, the relative shape must not);
//!   * the `script.sh: line N: ` / `x.pl: line N: ` prefix of a shell
//!     diagnostic is dropped — recorded in `tests/bash-parity-known-gaps.txt`
//!     as the redirect-diagnostic-prefix difference.
//! Nothing else is normalised: a diverging byte is a failure.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// One corpus entry: a shell script plus the committed golden of its render.
struct Entry {
    name: String,
    src: PathBuf,
    golden: PathBuf,
}

/// The tier1 fixtures are the point of this test: each one was written for a
/// Tier-1 class and carries its expected bash output in the source.
const TIER1: &[&str] = &[
    "nested_backtick_cmdsub",     // nested `$()` inside backticks, backtick word parts
    "command_conditions",         // if/elsif/while/until/ternary/&&/||/! by status
    "native_builtin_compound",    // cd/rm/mkdir/touch/echo/printf/test in compound position
    "builtin_status",             // `$?` after every natively lowered builtin
    "native_capture_cmdsub",      // captures without a constant to fold into
    "redirect_forms",             // >, >>, <, >&2, group redirect, exec N>
    "dup_stderr_to_stdout",       // `cmd 2>&1` is a DUP, never a file named `&1`
];

/// A dozen `examples/*.sh` spanning capture, redirection, here-docs,
/// arithmetic, brace expansion, functions and quoting. The goldens are the
/// ones `tests/perl_examples_regression.rs` already pins, so the two suites
/// cannot disagree about what the renderer emits.
const EXAMPLES: &[&str] = &[
    "090_nested_if_else",
    "param-expand-default-operator",
    "002_control_flow",
    "010_substring_loop",
    "064_13_complex_string_manipulation",
    "000__07_find_path_commands",
    "000__04a_basic_command_substitution",
    "000__04b_file_directory_operations",
    "000__04h_complex_examples",
    "077_backslash_continuation",
    "021_ansi_quoting_escape",
    "unclosed-param-expansion",
    "native_capture",
];

fn corpus() -> Vec<Entry> {
    let mut out = Vec::new();
    for name in TIER1 {
        out.push(Entry {
            name: name.to_string(),
            src: root().join("tests/tier1").join(format!("{name}.sh")),
            golden: root().join("tests/golden/tier1").join(format!("{name}.pl")),
        });
    }
    for name in EXAMPLES {
        out.push(Entry {
            name: name.to_string(),
            src: root().join("examples").join(format!("{name}.sh")),
            golden: root().join("tests/golden/examples").join(format!("{name}.pl")),
        });
    }
    out
}

fn render(src: &str) -> String {
    let (commands, _) = debashl::Parser::new(src)
        .parse_with_lines()
        .expect("corpus source must parse");
    let prog = debashl::shir::ast_to_ir(&commands);
    debashl::perl_generator_fixed::generate(&prog)
}

/// The render whose text is PINNED in `tests/golden/**`. It carries the
/// script's repo-relative path as `$0`, exactly as the CLI does (the shipped
/// `perl_backend --fixed examples/x.sh` embeds `examples/x.sh`), so the
/// fixtures and `tests/perl_examples_regression.rs` — which compare the same
/// files — agree on what the shipped output is.
fn render_for_golden(src: &str, script: &Path) -> String {
    let rel = script
        .strip_prefix(root())
        .unwrap_or(script)
        .to_string_lossy()
        .into_owned();
    let (commands, _) = debashl::Parser::new(src)
        .parse_with_lines()
        .expect("corpus source must parse");
    let prog = debashl::shir::ast_to_ir(&commands);
    debashl::perl_generator_fixed::generate_named(&prog, &rel)
}

fn have_perl() -> bool {
    Command::new("perl")
        .arg("-v")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// `perl -w -c` on `perl_text`; `None` when clean, else the diagnostics
/// (the renderer's optional `uu-ffi` notice and perl's own `syntax OK`
/// announcement are not diagnostics).
fn compile_diagnostics(perl_text: &str) -> Option<String> {
    let path = std::env::temp_dir().join(format!(
        "sh2_rb_c_{}_{}.pl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::write(&path, perl_text).expect("write temp perl");
    let out = Command::new("perl")
        .args(["-w", "-c"])
        .arg(&path)
        .output()
        .expect("run perl");
    let _ = std::fs::remove_file(&path);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut bad = String::new();
    if !out.status.success() {
        bad.push_str(&format!("perl -c exited {:?}\n", out.status.code()));
    }
    for line in stderr.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.contains("sh2perl: uu-ffi ")
            || line.ends_with("syntax OK")
        {
            continue;
        }
        bad.push_str(&format!("stderr: {line}\n"));
    }
    // Name the two warnings this net exists for, so a failure says which
    // class regressed instead of just "a warning".
    for needle in ["Useless use of", "masks earlier declaration"] {
        if stderr.contains(needle) {
            bad.push_str(&format!("forbidden warning: {needle}\n"));
        }
    }
    if bad.is_empty() {
        None
    } else {
        Some(bad)
    }
}

// ── 1. Golden text ──────────────────────────────────────────────────────

#[test]
fn corpus_renders_match_their_committed_golden() {
    let update = std::env::var_os("SH2_UPDATE_SNAPSHOTS").is_some();
    let mut failures = String::new();
    for entry in corpus() {
        let src = std::fs::read_to_string(&entry.src)
            .unwrap_or_else(|e| panic!("read {}: {e}", entry.src.display()));
        let got = render_for_golden(&src, &entry.src);
        if update {
            if let Some(dir) = entry.golden.parent() {
                std::fs::create_dir_all(dir).expect("create golden dir");
            }
            std::fs::write(&entry.golden, &got).expect("write golden");
            continue;
        }
        let expected = std::fs::read_to_string(&entry.golden).unwrap_or_else(|e| {
            panic!(
                "read golden {}: {e} (regenerate with SH2_UPDATE_SNAPSHOTS=1 cargo test --test renderer_behaviour)",
                entry.golden.display()
            )
        });
        if got != expected {
            let mut msg = format!("golden drift for {}\n", entry.name);
            for (i, (a, b)) in expected.lines().zip(got.lines()).enumerate() {
                if a != b {
                    msg.push_str(&format!(
                        "  line {}:\n    golden: {a}\n    render: {b}\n",
                        i + 1
                    ));
                    break;
                }
            }
            let (n_expected, n_got) = (expected.lines().count(), got.lines().count());
            if n_expected != n_got {
                msg.push_str(&format!(
                    "  golden has {n_expected} lines, render has {n_got}\n"
                ));
            }
            failures.push_str(&msg);
        }
    }
    assert!(failures.is_empty(), "{failures}");
}

// ── 2. Every render compiles with no diagnostic at all ──────────────────

#[test]
fn corpus_renders_compile_clean_under_perl_w_c() {
    if !have_perl() {
        eprintln!("skip: perl not installed");
        return;
    }
    let mut failures = String::new();
    for entry in corpus() {
        let src = std::fs::read_to_string(&entry.src).expect("read corpus source");
        if let Some(bad) = compile_diagnostics(&render_for_golden(&src, &entry.src)) {
            failures.push_str(&format!("{}:\n{bad}\n", entry.name));
        }
    }
    assert!(
        failures.is_empty(),
        "renders that are not clean under perl -w -c:\n{failures}"
    );
}

// ── 3. bash vs emitted Perl, stdout/stderr/exit ─────────────────────────

/// Both sides run in `<tmp>/a/work` and `<tmp>/b/work`: SAME directory name,
/// so `basename $(pwd)` and friends agree, and separate directories so a
/// script that writes files cannot feed its own output to the other side.
fn behaviour_dir(tag: &str, uniq: u128) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sh2_rb_{}_{}_{tag}/work",
        std::process::id(),
        uniq
    ));
    std::fs::create_dir_all(&dir).expect("create behaviour dir");
    dir
}

/// The prefix a shell puts in front of a diagnostic (`script.sh: line 4: `,
/// `x.pl: line 1: `). Documented in `tests/bash-parity-known-gaps.txt`.
fn strip_diag_prefix(line: &str) -> String {
    // Same rule as `sed -E 's/^.*: line [0-9]+: //'` in bash-parity.sh.
    if let Some(idx) = line.find(": line ") {
        let after = &line[idx + ": line ".len()..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            if let Some(tail) = after[digits.len()..].strip_prefix(": ") {
                return tail.to_string();
            }
        }
    }
    line.to_string()
}

fn normalise(text: &str, dir: &Path) -> String {
    let dir = dir.to_string_lossy().to_string();
    let mut out = String::new();
    for line in text.lines() {
        // The renderer's own optional-library notice is not script output.
        if line.starts_with("sh2perl: uu-ffi ") {
            continue;
        }
        let line = line.replace(&dir, "<WORK>");
        out.push_str(&strip_diag_prefix(&line));
        out.push('\n');
    }
    out
}

fn run(cmd: &mut Command, cwd: &Path) -> (String, String, i32) {
    let out = cmd
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("spawn corpus runner");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn corpus_renders_behave_like_bash() {
    if !have_perl() {
        eprintln!("skip: perl not installed");
        return;
    }
    let base = std::env::temp_dir();
    let uniq = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut failures = String::new();

    for (i, entry) in corpus().into_iter().enumerate() {
        let src = std::fs::read_to_string(&entry.src).expect("read corpus source");
        let perl_text = render(&src);

        // Both sides run the SAME relative path shape: `<base>/<name>.sh`
        // for bash and `<base>/<name>.pl` for perl, from the two work dirs.
        let script = base.join(format!("sh2_rb_{uniq}_{i}_{}.sh", entry.name));
        let prog = base.join(format!("sh2_rb_{uniq}_{i}_{}.pl", entry.name));
        std::fs::write(&script, &src).expect("write corpus script");
        std::fs::write(&prog, &perl_text).expect("write corpus render");

        let a = behaviour_dir("a", uniq * 1000 + i as u128);
        let b = behaviour_dir("b", uniq * 1000 + i as u128);

        // An example that invokes a sibling by relative path needs the
        // corpus beside it (same rule as tests/bash-parity.sh).
        if src.contains("examples/") {
            let examples = root().join("examples");
            for dir in [&a, &b] {
                let dest = dir.join("examples");
                let _ = std::fs::remove_dir_all(&dest);
                copy_tree(&examples, &dest);
            }
        }

        let (b_out, b_err, b_rc) = run(Command::new("bash").arg(&script), &a);
        let (p_out, p_err, p_rc) = run(Command::new("perl").arg("-w").arg(&prog), &b);

        let mut mismatch = String::new();
        if normalise(&b_out, &a) != normalise(&p_out, &b) {
            mismatch.push_str("  stdout differs\n");
            mismatch.push_str(&first_diff(&normalise(&b_out, &a), &normalise(&p_out, &b)));
        }
        // stderr is part of the contract for EVERY entry: a dropped
        // diagnostic or a new warning is a wrong answer even when stdout and
        // the exit status agree.
        if normalise(&b_err, &a) != normalise(&p_err, &b) {
            mismatch.push_str("  stderr differs\n");
            mismatch.push_str(&first_diff(&normalise(&b_err, &a), &normalise(&p_err, &b)));
        }
        if b_rc != p_rc {
            mismatch.push_str(&format!("  exit: bash={b_rc} perl={p_rc}\n"));
        }
        if !mismatch.is_empty() {
            failures.push_str(&format!("{}:\n{mismatch}", entry.name));
        }

        let _ = std::fs::remove_file(&script);
        let _ = std::fs::remove_file(&prog);
        let _ = std::fs::remove_dir_all(a.parent().unwrap_or(&a));
        let _ = std::fs::remove_dir_all(b.parent().unwrap_or(&b));
    }
    assert!(
        failures.is_empty(),
        "emitted Perl does not behave like bash:\n{failures}"
    );
}

fn first_diff(a: &str, b: &str) -> String {
    for (i, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return format!("  line {}:\n    bash: {x}\n    perl: {y}\n", i + 1);
        }
    }
    format!(
        "  bash has {} lines, perl has {}\n",
        a.lines().count(),
        b.lines().count()
    )
}

/// Recursive copy, for the examples that call a sibling script.
fn copy_tree(from: &Path, to: &Path) {
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    let _ = std::fs::create_dir_all(to);
    for entry in entries.flatten() {
        let src = entry.path();
        let dest = to.join(entry.file_name());
        if src.is_dir() {
            copy_tree(&src, &dest);
        } else {
            let _ = std::fs::copy(&src, &dest);
        }
    }
}
