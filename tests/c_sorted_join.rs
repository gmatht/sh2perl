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

/// sortedBigintJoin over magnitudes past i64 (and i64-range values
/// mixed in): exact mpz order, first-in-input dedupe, joined text.
/// Hand-built IR — exercises the node directly, no frontend needed.
#[test]
fn sorted_bigint_join_exact() {
    use debashl::ir::{AssignTarget, IrExpr, IrProgram, IrStmt, IrType, StrStyle};
    let st = |s: &str| IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted);
    let vals = [
        "340282366920938463463374607431768211456", // 2**128
        "18446744073709551615",                   // 2**64 - 1
        "1267650600228229401496703205376",       // 2**100
        "5",
        "-99999999999999999999999999",
        "5", // dupe: deduped, first occurrence kept
        "18446744073709551615", // dupe
        "42",
    ];
    let prog = IrProgram {
        imports: vec![],
        requires: vec![],
        stmts: vec![
            IrStmt::Assign {
                targets: vec![AssignTarget {
                    var: "b".to_string(),
                    sigil: None,
                    indices: vec![],
                }],
                expr: IrExpr::Call {
                    func: "setArray".to_string(),
                    args: vec![
                        st("b"),
                        IrExpr::Array(vals.iter().map(|v| st(v)).collect()),
                    ],
                },
                asm: None,
            },
            IrStmt::Expr(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![
                    st("echo"),
                    IrExpr::Array(vec![IrExpr::Call {
                        func: "sortedBigintJoin".to_string(),
                        args: vec![st("b")],
                    }]),
                ],
            }),
        ],
        subs: vec![],
        var_types: vec![("b".to_string(), IrType::Str)],
        stmt_lines: vec![],
        var_lengths: vec![],
        var_const: vec![],
        var_lifetimes: vec![],
        var_nospace: vec![],
        var_storage: vec![],
        var_bash_env: vec![],
    };
    let c = debashl::c_backend::shir_to_c(&prog);
    assert!(c.contains("_sh_sorted_bigint_join"), "no bigint lowering");
    let id = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("c_sorted_big_{}", id));
    std::fs::create_dir_all(&dir).expect("scratch");
    let src = dir.join("t.c");
    let bin = dir.join("t");
    std::fs::File::create(&src)
        .expect("write")
        .write_all(c.as_bytes())
        .expect("write");
    let cc = Command::new("gcc")
        .arg(&src)
        .arg("-lm")
        .arg("-lgmp")
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("gcc");
    assert!(
        cc.status.success(),
        "gcc failed:\n{}",
        String::from_utf8_lossy(&cc.stderr)
    );
    let run = Command::new(&bin).output().expect("run");
    assert!(run.status.success());
    let got = String::from_utf8(run.stdout).expect("utf8");
    // numeric order incl. the negative huge; dupes (5, 2**64-1) once.
    // (2**64-1 < 2**100: 20 digits vs 31 — verified against python3.)
    assert_eq!(
        got,
        "-99999999999999999999999999, 5, 42, 18446744073709551615, 1267650600228229401496703205376, 340282366920938463463374607431768211456\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

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
