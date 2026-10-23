//! Regression: a function with TWO OR MORE discarded calls prints NOTHING.
//!
//! ## The failure
//!
//! ```bash
//! use() { echo "[x]"; }
//! f() { use; use; }
//! f
//! ```
//!
//! bash prints `[x]` twice. The C backend prints **nothing at all** —
//! and it does so with no diagnostic: the script compiles, links, runs
//! and exits 0. Every other kind of call is fine, which is what makes this
//! worth a test rather than a shrug:
//!
//! ```text
//! f() { use; }                 OK    (one call)
//! use; use                     OK    (top level, not in a function)
//! if [ 1 ]; then use; fi       OK    (two separate ifs at top level)
//! f() { a; b; }                BROKEN (two different callees)
//! f() { use; use; }            BROKEN
//! f() { i=0; while ...; use; } BROKEN (one call per iteration)
//! ```
//!
//! The scope is narrower and sharper than "loops" or "functions": it is
//! **a function containing a discarded call while the caller has already
//! installed the noecho guard** — i.e. more than one call site inside one
//! function body. A loop is a way of getting there, not a separate cause.
//!
//! ## Why (one root cause)
//!
//! `main` wraps a discarded call in a noecho guard, and the *callee's own*
//! `echo` is gated on the same flag:
//!
//! ```c
//! static int _sh_noecho;
//! static void use(void) {
//!     char *_rv0 = ("[x]");
//!     { _sh_rc = (int)atoll(_rv0); if (!_sh_noecho) printf("%s\n", ...); }
//! }
//! int main(void) { (_sh_noecho++, f(), _sh_noecho--); ... }
//! ```
//!
//! `_sh_capture_fn` redirects fd 1 to a tmpfile, then calls `f()`. Inside
//! `f()`, `_sh_noecho` is 1, so `use`'s `printf` is skipped and the
//! tmpfile stays empty — every discarded call's output is lost.
//!
//! A function with a SINGLE call is correct only by accident: the caller
//! emits the *capture* form (`_sh_call_fn` + an unconditional `printf` of
//! the result) instead of the noecho guard, so the guard is never entered.
//! Instrumenting the tmpfile read shows `0 bytes` in both cases and
//! `_sh_noecho=1` in both, which is why the one-call form works by a
//! different path rather than by the same path working twice.
//!
//! So the guard is applied per CALL SITE and leaks over the whole call
//! subtree: a callee cannot know whether its caller's discarded call is
//! one of several.
//!
//! ## Why the test is differential
//!
//! Exit status is useless here — the buggy build exits 0. Every case
//! below compares stdout byte-for-byte against `bash`. Vehicle matches
//! `c_growable_value.rs` (parse → `ast_to_ir` → `shir_to_c` → gcc → run);
//! needs gcc and bash.
//!
//! Found via the byte-exact fb0 equivalence suite for
//! `MIMEcroft_fb.sh`, whose HUD lost every glyph: `draw_text` is a
//! function whose loop body calls `glyph_index` and `draw_char`.

use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn scratch() -> std::path::PathBuf {
    let id = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "c_fn_multi_call_{}_{id}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn assert_matches_bash_named(script: &str, name: &str) {
    let cmds = debashl::Parser::new(script).parse().expect("parse shell");
    let prog = debashl::shir::ast_to_ir(&cmds);
    let c = debashl::c_backend::shir_to_c(&prog);

    let dir = scratch();
    let src = dir.join("t.c");
    let bin = dir.join("t");
    std::fs::File::create(&src)
        .expect("write C")
        .write_all(c.as_bytes())
        .expect("write C");
    let cc = Command::new("gcc")
        .arg(&src)
        .arg("-lm")
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run gcc");
    assert!(
        cc.status.success(),
        "gcc failed on {name}:\n{}",
        String::from_utf8_lossy(&cc.stderr)
    );
    let run = Command::new(&bin).output().expect("run C binary");
    let _ = std::fs::remove_dir_all(&dir);

    let bash = Command::new("bash")
        .arg("-c")
        .arg(script)
        .output()
        .expect("run bash");
    assert!(bash.status.success(), "bash failed on {name}: {bash:?}");

    assert_eq!(
        run.stdout, bash.stdout,
        "\n  case:   {name}\n  script: {script}\n  bash: {:?}\n  C:     {:?}",
        String::from_utf8_lossy(&bash.stdout),
        String::from_utf8_lossy(&run.stdout),
    );
}

fn assert_matches_bash(script: &str) {
    let id = SEQ.fetch_add(1, Ordering::Relaxed);
    assert_matches_bash_named(script, &format!("anon{id}"));
}

// ── The defect ──────────────────────────────────────────────────────

#[test]
fn two_discarded_calls_in_a_function_both_print() {
    assert_matches_bash("use() { echo \"[x]\"; }\nf() { use; use; }\nf\n");
}

#[test]
fn three_discarded_calls_in_a_function_all_print() {
    assert_matches_bash("use() { echo \"[x]\"; }\nf() { use; use; use; }\nf\n");
}

#[test]
fn three_discarded_calls_in_a_function_keep_side_effects() {
    // Not merely output: a callee that sets a global the next statement
    // reads must still run. This is `glyph_index` setting `gi` for
    // `draw_char`.
    assert_matches_bash(concat!(
        "setg() { G=42; return 0; }\n",
        "use() { echo \"[$G]\"; }\n",
        "f() { setg; use; setg; use; }\n",
        "f\n",
    ));
}

#[test]
fn calls_split_across_lines_in_a_function_print() {
    // Newlines are irrelevant to the cause; kept separate so a fix cannot
    // be line-shape-specific.
    assert_matches_bash("use() { echo \"[x]\"; }\nf() { use\nuse; }\nf\n");
}

#[test]
fn two_calls_to_different_functions_in_one_function_print() {
    assert_matches_bash("a() { echo A; }\nb() { echo B; }\nf() { a; b; }\nf\n");
}

/// A LOOP is not a mitigation: one discarded call per iteration loses
/// every iteration's output, which is the same noecho guard seen from a
/// different angle (the guard is installed once for the enclosing call,
/// so it silences all N of them).
#[test]
fn one_call_per_loop_iteration_in_a_function_prints() {
    assert_matches_bash("use() { echo \"[x]\"; }\nf() { i=0; while [ $i -lt 2 ]; do use; i=$((i+1)); done; }\nf\n");
}

#[test]
fn the_games_draw_text_shape() {
    // `MIMEcroft_fb.sh` `draw_text`, reduced. The per-glyph calls are the
    // ones that vanished, which is why the HUD rendered no text.
    assert_matches_bash(concat!(
        "glyph_index() { gi=65; return 0; }\n",
        "draw_char() { echo \"[$1]\"; }\n",
        "draw_text() { s=SCORE; i=0\n",
        "  while [ $i -lt 3 ]; do glyph_index \"${s:$i:1}\"; draw_char \"$gi\"; i=$((i+1)); done\n",
        "}\n",
        "draw_text\n",
    ));
}

#[test]
fn a_broken_function_is_still_broken_when_called_from_another() {
    // The noecho guard is installed by the CALLER, so the nesting depth
    // does not matter — it is the callee's own body that gets silenced.
    assert_matches_bash("use() { echo \"[x]\"; }\nf() { use; use; }\ng() { f; }\ng\n");
}

// ── Controls: correct today, must stay correct ──────────────────────
//
// Without these, a "fix" that suppressed even more output would pass the
// cases above while regressing these.

#[test]
fn one_discarded_call_in_a_function_prints() {
    assert_matches_bash("use() { echo \"[x]\"; }\nf() { use; }\nf\n");
}

#[test]
fn two_discarded_calls_at_top_level_print() {
    assert_matches_bash("use() { echo \"[x]\"; }\nuse; use\n");
}

#[test]
fn two_calls_in_separate_functions_called_from_top_level_print() {
    assert_matches_bash("a() { echo A; }\nb() { echo B; }\na; b\n");
}

#[test]
fn two_calls_in_separate_ifs_at_top_level_print() {
    assert_matches_bash("use() { echo \"[x]\"; }\nif [ 1 -eq 1 ]; then use; fi\nif [ 1 -eq 1 ]; then use; fi\n");
}

#[test]
fn two_calls_in_one_loop_at_top_level_print() {
    assert_matches_bash("use() { echo \"[x]\"; }\ni=0\nwhile [ $i -lt 2 ]; do use; use; i=$((i+1)); done\n");
}

#[test]
fn a_function_with_only_echoes_still_prints() {
    assert_matches_bash("f() { echo a; echo b; }\nf\n");
}

/// The other half of the contract: when a result IS consumed by a command
/// substitution, capturing the callee's output is right and must stay so.
/// Same function body as the failing case — only the caller differs.
#[test]
fn captured_output_is_still_captured() {
    assert_matches_bash("inner() { echo \"[x]\"; }\nf() { n=0; inner; }\nr=$(f)\necho \"got=[$r]\"\n");
}

#[test]
fn two_calls_in_a_captured_function_are_still_captured() {
    assert_matches_bash("a() { echo A; }\nb() { echo B; }\nf() { a; b; }\nr=$(f)\necho \"got=[$r]\"\n");
}

#[test]
fn command_substitution_output_is_unchanged() {
    assert_matches_bash("inner() { echo \"[x]\"; }\nr=$(inner)\necho \"got=[$r]\"\n");
}