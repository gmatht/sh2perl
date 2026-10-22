//! `sh2_str` — the LENGTH-CARRYING string type (stage 1: the runtime).
//!
//! C strings cannot hold an embedded NUL; bash values must. Measured on
//! the simplest case:
//!
//!     s=$(printf 'A\000B')
//!     real bash:  ${#s} = 2,  '%s' writes "AB"   (bash DROPS the NUL)
//!     C render:   ${#s} = 1,  '%s' writes "A"    (the NUL TRUNCATES)
//!
//! so everything past the first NUL is silently lost — which is what
//! makes building a binary value (framebuffer pixels, a packed payload)
//! impossible in the script today.
//!
//! `sh2_str` carries `n` (bytes, NULs included) alongside a still
//! NUL-terminated `p`, so a value can be handed to any C API expecting a
//! C string while the LENGTH is the truth.
//!
//! This test is the RUNTIME half: it renders a program (to get the
//! emitted runtime), then compiles a driver that exercises `_sh_sset`,
//! `_sh_scat`, `_sh_scatc`, `_sh_slen`, `_sh_sget` and `_sh_sreset`
//! directly against values containing NULs — including the exact
//! `A\0B` case above, and a growth case that crosses the realloc
//! boundary with a NUL in the middle.
//!
//! It deliberately does NOT go through the shell front end: stage 1
//! changes no call sites, so the front end has no syntax that produces a
//! `sh2_str` yet. What is under test here is that the emitted type is
//! CORRECT, so stages 2-4 have something to build on. Requires gcc.

use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

/// Render a shell script to C, then append a C driver and run it. The
/// shell script is only there to make the renderer emit its runtime
/// (including the sh2_str helpers); its own behaviour is not asserted
/// here.
fn run_with_driver(script: &str, driver: &str) -> (String, Vec<u8>) {
    let cmds = debashl::Parser::new(script).parse().expect("parse shell");
    let prog = debashl::shir::ast_to_ir(&cmds);
    let c = debashl::c_backend::shir_to_c(&prog);

    // The renderer emits `int main(void) { ... }`. We need our own, so
    // rename the rendered one and call it from ours (its side effects —
    // none for this script — must not run twice).
    let c = c.replacen("int main(void) {", "int _rendered_main(void) {", 1);
    assert!(
        c.contains("_rendered_main"),
        "could not find the rendered main to rename"
    );
    assert!(
        c.contains("typedef struct { char *p; size_t n; } sh2_str;"),
        "the sh2_str type is missing from the emitted runtime"
    );

    let full = format!("{c}\n{driver}\n");

    let id = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "c_sh2_str_{}_{}_{}",
        std::process::id(),
        id,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let src = dir.join("t.c");
    let bin = dir.join("t");
    std::fs::File::create(&src)
        .expect("write C")
        .write_all(full.as_bytes())
        .expect("write C");

    let out = Command::new("gcc")
        .arg(&src)
        .arg("-lm")
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("run gcc");
    assert!(
        out.status.success(),
        "gcc failed:\n{}\n--- emitted C ---\n{full}",
        String::from_utf8_lossy(&out.stderr)
    );

    let run = Command::new(&bin).output().expect("run driver");
    assert!(
        run.status.success(),
        "driver exited {:?}\nstdout:\n{}\nstderr:\n{}",
        run.status,
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
    (full, run.stdout)
}

#[test]
fn embedded_nul_survives_the_length_carrying_string() {
    // A script with a capture, so the render is not the trivial empty one.
    // Its output is irrelevant; the runtime helpers are the subject.
    let script = "x=$(printf 'hi')\necho \"$x\"\n";
    let driver = r#"
int main(void) {
    /* the measured reproducer: A, NUL, B */
    sh2_str s; s.p = 0; s.n = 0;
    _sh_sset(&s, "A\0B", 3);
    if (_sh_slen(&s) != 3) {
        fprintf(stderr, "len=%zu want 3\n", _sh_slen(&s));
        return 1;
    }
    if (memcmp(_sh_sget(&s), "A\0B", 3) != 0) {
        fprintf(stderr, "bytes differ\n");
        return 1;
    }
    /* the C-string view is NUL-terminated at n and shows only "A" —
     * that is expected, and precisely why the length must exist */
    if (strcmp(_sh_sget(&s), "A") != 0) {
        fprintf(stderr, "C-string view is not \"A\"\n");
        return 1;
    }

    /* appends are length-aware and keep NULs */
    _sh_scat(&s, "\0C", 2);
    if (_sh_slen(&s) != 5 || memcmp(_sh_sget(&s), "A\0B\0C", 5) != 0) {
        fprintf(stderr, "cat: len=%zu\n", _sh_slen(&s));
        return 1;
    }
    _sh_scatc(&s, '\0');
    if (_sh_slen(&s) != 6 || _sh_sget(&s)[5] != 0) {
        fprintf(stderr, "catc: len=%zu\n", _sh_slen(&s));
        return 1;
    }

    /* growth ACROSS the realloc boundary, with a NUL in the middle of
     * every chunk: 400 x ("\0\377" + one letter) forces several grows */
    sh2_str g; g.p = 0; g.n = 0;
    for (int i = 0; i < 400; i++) {
        if (i % 3 == 1) {
            _sh_scat(&g, "\0\377", 2);
            _sh_scatc(&g, (char)('a' + (i % 26)));
        } else if (i % 3 == 2) {
            _sh_scat(&g, "\377\0", 2);
            _sh_scatc(&g, (char)('A' + (i % 26)));
        } else {
            _sh_scat(&g, "\0\0", 2);
            _sh_scatc(&g, (char)('0' + (i % 10)));
        }
    }
    if (_sh_slen(&g) != 400 * 3) {
        fprintf(stderr, "grow: len=%zu want %d\n", _sh_slen(&g), 400 * 3);
        return 1;
    }
    /* spot-check that the NULs are really there, not skipped */
    if (_sh_sget(&g)[0] != 0 || _sh_sget(&g)[1] != 0 || _sh_sget(&g)[2] != '0') {
        fprintf(stderr, "grow: first chunk wrong\n");
        return 1;
    }
    if (_sh_sget(&g)[3] != 0 || _sh_sget(&g)[4] != (char)0xff) {
        fprintf(stderr, "grow: second chunk wrong\n");
        return 1;
    }
    if (_sh_sget(&g)[_sh_slen(&g)] != 0) {
        fprintf(stderr, "grow: not NUL-terminated at n\n");
        return 1;
    }

    /* reset returns it to the empty string */
    _sh_sreset(&g);
    if (_sh_slen(&g) != 0 || _sh_sget(&g)[0] != 0) {
        fprintf(stderr, "reset failed\n");
        return 1;
    }

    printf("sh2_str ok\n");
    free(s.p);
    free(g.p);
    return 0;
}
"#;
    let (_c, stdout) = run_with_driver(script, driver);
    assert_eq!(
        String::from_utf8_lossy(&stdout),
        "sh2_str ok\n",
        "the sh2_str runtime did not behave"
    );
}

#[test]
fn unset_and_empty_strings_read_as_empty() {
    // `_sh_sget`/`_sh_slen` must tolerate a zeroed struct — that is the
    // shape the migration will hand them for an unset variable, and a
    // NULL deref there would be a crash on any script that reads a var
    // before assigning it.
    let script = "echo hi\n";
    let driver = r#"
int main(void) {
    sh2_str z; z.p = 0; z.n = 0;
    if (strcmp(_sh_sget(&z), "") != 0) { fprintf(stderr, "get\n"); return 1; }
    if (_sh_slen(&z) != 0) { fprintf(stderr, "len\n"); return 1; }
    if (strcmp(_sh_sget(0), "") != 0) { fprintf(stderr, "get NULL\n"); return 1; }
    if (_sh_slen(0) != 0) { fprintf(stderr, "len NULL\n"); return 1; }
    /* _sh_sset(NULL-ptr) must clear, not crash */
    sh2_str s; s.p = 0; s.n = 0;
    _sh_sset(&s, "abcdef", 6);
    _sh_sset(&s, 0, 0);
    if (_sh_slen(&s) != 0 || _sh_sget(&s)[0] != 0) { fprintf(stderr, "set NULL\n"); return 1; }
    /* _sh_scat of nothing is a no-op */
    _sh_scat(&s, 0, 0);
    _sh_scat(&s, "x", 0);
    if (_sh_slen(&s) != 0) { fprintf(stderr, "scat empty\n"); return 1; }
    printf("empty ok\n");
    free(s.p);
    return 0;
}
"#;
    let (_c, stdout) = run_with_driver(script, driver);
    assert_eq!(String::from_utf8_lossy(&stdout), "empty ok\n");
}
