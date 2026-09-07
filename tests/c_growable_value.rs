//! Regression: values larger than CAP_BUF (65536) must survive the C
//! value path (external capture → store → output, and shell-function
//! return → output).
//!
//! Every buffer on that path used to be a fixed `static char[65536]`
//! with SILENT truncation past 65535 bytes — no diagnostic, just wrong
//! output. Python-origin programs are unbounded by construction, so any
//! large value (file contents, big captures, large computed strings)
//! hit this. The fix is growable heap value buffers (`_sh_vbuf` +
//! `_sh_vgrow` doubling) on the capture/pipeline/return path.
//!
//! End-to-end vehicle: render shell → C via the library, compile with
//! gcc, run, and byte-compare stdout against bash. Requires gcc, bash,
//! seq at test time (same requirements as the backend/c corpus gate).

use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn bash_stdout(script: &str) -> Vec<u8> {
    let out = Command::new("bash")
        .arg("-c")
        .arg(script)
        .output()
        .expect("run bash");
    assert!(out.status.success(), "bash failed on {script:?}: {out:?}");
    out.stdout
}

fn c_stdout(script: &str) -> Vec<u8> {
    let cmds = debashl::Parser::new(script).parse().expect("parse shell");
    let prog = debashl::shir::ast_to_ir(&cmds);
    let c = debashl::c_backend::shir_to_c(&prog);
    let id = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "c_growable_value_{}_{}_{}",
        std::process::id(),
        id,
        // unique per test binary run, no external crates
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
        "gcc failed on {script:?}:\n{}",
        String::from_utf8_lossy(&cc.stderr)
    );
    let run = Command::new(&bin).output().expect("run C binary");
    assert!(run.status.success(), "C binary failed on {script:?}: {run:?}");
    let _ = std::fs::remove_dir_all(&dir);
    run.stdout
}

#[test]
fn capture_over_64k_survives() {
    // seq 1 20000 ≈ 109KB — well past CAP_BUF. Must be byte-identical
    // to bash, not truncated at 65535 bytes.
    let sh = "x=$(seq 1 20000); printf '%s' \"$x\"";
    let (c, b) = (c_stdout(sh), bash_stdout(sh));
    assert_eq!(c.len(), b.len(), "C output truncated: {} vs {} bytes", c.len(), b.len());
    assert_eq!(c, b);
}

#[test]
fn fn_return_over_64k_survives() {
    // the same volume through a shell-function return (`_fnret_` was a
    // fixed 64KB static too) instead of a direct capture.
    let sh = "big() { seq 1 20000; }\ny=$(big)\nprintf '%s' \"$y\"";
    let (c, b) = (c_stdout(sh), bash_stdout(sh));
    assert_eq!(c.len(), b.len(), "C output truncated: {} vs {} bytes", c.len(), b.len());
    assert_eq!(c, b);
}
