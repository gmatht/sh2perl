//! Native `print(sorted-array)` lowering: the pipeline idiom
//! `printf '%s\n' "${arr[@]}" | sort -n -u | awk 'NR>1{...}{...}'`
//! (py-sh-go's `print(sorted(set))` shape, also writable by hand)
//! must NOT fork/exec sort+awk for data already in memory. The C
//! backend sorts/dedupes/joins natively with exact GNU `sort -n -u`
//! semantics (stable numeric order, first-in-input dedupe — including
//! blanks, decimals, duplicates like 01-vs-1, huge magnitudes,
//! embedded newlines, empty arrays).
//!
//! End-to-end vehicle: render shell → C, gcc, run, byte-compare vs
//! bash. Pre-fix these fail (stage 0 prints one space-joined line,
//! so sort sees a single line and the output is insertion-ordered).

use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(2000);

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
        "c_sorted_join_{}_{}",
        std::process::id(),
        id
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
        .arg("-lgmp")
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

fn check(sh: &str) {
    let (c, b) = (c_stdout(sh), bash_stdout(sh));
    assert_eq!(
        String::from_utf8_lossy(&c),
        String::from_utf8_lossy(&b),
        "C vs bash mismatch on {sh:?}"
    );
}

const JOIN: &str = "awk 'NR>1{printf \"%s\",\", \"}{printf \"%s\",$0}'";

#[test]
fn sorted_join_basic() {
    // unsorted + dupes: pre-fix prints insertion order on one line.
    check(&format!(
        "a=(3 1 2 2 10); printf '%s\\n' \"${{a[@]}}\" | sort -n -u | {JOIN}"
    ));
}

#[test]
fn sorted_join_adversarial() {
    // blanks, decimals, 01-vs-1 dupes, negatives, huge magnitudes,
    // empty strings, zero forms — the GNU -n key-order corners.
    check(&format!(
        "a=(\"01\" \"1\" \" 1\" \"1.5\" \"1.50\" \"-3\" \"  -03  \" \"\" \"abc\" \"+2\" \"99999999999999999999999999\" \"100000000000000000000000000\" \"00\" \".5\" \"5.\"); printf '%s\\n' \"${{a[@]}}\" | sort -n -u | {JOIN}"
    ));
}

#[test]
fn sorted_join_empty() {
    check(&format!(
        "a=(); printf '%s\\n' \"${{a[@]}}\" | sort -n -u | {JOIN}; echo done"
    ));
}

#[test]
fn sorted_join_embedded_newlines() {
    // elements containing newlines fragment into lines exactly like
    // the pipeline sees them (printf emits element + \n each). This
    // shape routes to the child-bash fallback — guards that it stays
    // correct there.
    check(&format!(
        "b=$(printf 'x\\ny'); a2=(\"$b\" \"a\" \"c\"); printf '[%s]\\n' \"$(printf '%s\\n' \"${{a2[@]}}\" | sort -n -u | {JOIN})\""
    ));
}

#[test]
fn sorted_join_function_return() {
    // the t86 shape: the pipeline value produced inside a shell
    // function and captured by the caller.
    check(&format!(
        "g=(5 3 5 1); f() {{ printf '%s\\n' \"${{g[@]}}\" | sort -n -u | {JOIN}; }}; r=$(f); echo \"[$r]\""
    ));
}
