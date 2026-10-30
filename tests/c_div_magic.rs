//! Exact division-free lowering for positive literal divisors in the C
//! backend: ONE `_sh_divodd` mulhi magic for the whole divisor
//! (`mulhi(|x|, M) >> s`, M = ceil(2^S / d), S = 63 + bits(d)), with a
//! plain `_sh_divp2` shift reserved for pure powers of two (where the
//! magic would be 2^64 and does not fit). A composite divisor is NOT
//! split into shift-then-odd-magic: for d = 2^n·r both forms share the
//! same M and only the shift differs, so the split cost an extra helper
//! call — never inlined by tcc — for no accuracy.
//!
//! Why: tcc (bash-O4's default toolchain) does no strength reduction, so
//! every literal-divisor site stayed a ~30-cycle idiv at runtime; gcc -O2
//! finds the magics itself, emitting them makes the fast path
//! compiler-independent.
//!
//! Oracle: the SAME source under bash (its `$(( ))` semantics are the
//! spec), plus structural assertions that the magic actually fired.
//! Divisors are written as literals in the source — a loop variable would
//! legitimately fall through to the guarded division.
//!
//! The dividend comes from `read`, so no const fold can erase the
//! division before the render, and `m=$((a-1))` manufactures LLONG_MIN
//! from the -9223372036854775807 input (a literal that big cannot be
//! written directly in `$(( ))`).

use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(6000);

fn render(src: &str) -> String {
    let cmds = debashl::Parser::new(src).parse().expect("test source must parse");
    let prog = debashl::shir::ast_to_ir(&cmds);
    debashl::c_backend::shir_to_c(&prog)
}

/// The program under test: iterates dividend values and prints
/// div/mod for every literal divisor, for both `v` and `m = v-1`.
///
/// A `for`-list loop, deliberately: the raw render path's `while read`
/// stdin lowering drops the first input (redundant pre-loop read + read
/// in the condition) and hoists body statements — a PRE-EXISTING bug in
/// that lowering, independent of the div/mod magic (reproducible with a
/// division-free program), so this test does not ride on it.
fn test_source() -> String {
    let divisors: [i64; 24] = [
        1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 16, 17, 100, 127, 128, 1000, 1024, 4096, 65537,
        1_000_000, 1_048_576, 1_000_000_007, 2_147_483_648, 4_611_686_018_427_387_904,
    ];
    let values = [
        "5",
        "-5",
        "0",
        "1",
        "-1",
        "1000000",
        "-1000000",
        "9223372036854775807",
        "-9223372036854775807",
        "4294967296",
        "-4294967296",
        "123456789",
        "-123456789",
        "1000000007",
        "-1000000007",
        "2147483647",
        "-2147483648",
        "65536",
        "-65536",
        "999999999999",
        "-999999999999",
        "4611686018427387903",
        "-4611686018427387903",
        "9223372036854775806",
    ];
    let mut src = format!("D=1000\nfor v in {} ; do\n", values.join(" "));
    src.push_str("  m=$((v-1))\n");
    for d in divisors {
        src.push_str(&format!("  echo $((v / {d}))\n"));
        src.push_str(&format!("  echo $((v % {d}))\n"));
        src.push_str(&format!("  echo $((m / {d}))\n"));
        src.push_str(&format!("  echo $((m % {d}))\n"));
    }
    // the const-var divisor (1000 = 2^3 * 125 → both helper shapes);
    // resolves via const_rhs even if a copy-prop pass inlines it first.
    src.push_str("  echo $((v / D))\n");
    src.push_str("  echo $((m % D))\n");
    src.push_str("done\n");
    src
}

/// Structural: the magic renders (one helper per site, fused for
/// composite divisors), replaces the zero guard, and no helper is
/// emitted for a program with no division.
#[test]
fn div_magic_renders() {
    let c = render("read a\necho $(( a / 12 ))\necho $(( a % 12 ))\n");
    assert!(
        c.contains("_sh_divodd(") && !c.contains("_sh_divp2("),
        "12 = 4*3 fuses into ONE magic (no separate shift call):\n{c}"
    );
    assert!(
        !c.contains("== 0 ? 0 :"),
        "zero guard replaced by exact magic:\n{c}"
    );
    assert!(
        !c.contains("sh2_arith(") && !c.contains("sh2ArithStub"),
        "string-eval fallback leaked in:\n{c}"
    );

    // one helper shape only — 16 = 2^4 is pure power of two
    let c16 = render("read a\necho $(( a / 16 ))\n");
    assert!(c16.contains("_sh_divp2("), "p2 helper:\n{c16}");
    assert!(!c16.contains("_sh_divodd("), "no odd helper:\n{c16}");

    // odd divisor only
    let c3 = render("read a\necho $(( a / 3 ))\n");
    assert!(c3.contains("_sh_divodd("), "odd helper:\n{c3}");
    assert!(!c3.contains("_sh_divp2("), "no p2 helper:\n{c3}");

    // no division → no dead helpers (present-if-used discipline)
    let c0 = render("a=5\necho $((a+2))\n");
    assert!(!c0.contains("_sh_divp2("), "dead p2 helper:\n{c0}");
    assert!(!c0.contains("_sh_divodd("), "dead odd helper:\n{c0}");
    assert!(!c0.contains("_sh_mulhu("), "dead mulhu helper:\n{c0}");
}

/// End-to-end exactness: compiled C output == bash output for the same
/// source, over boundary dividends (LLONG_MIN via m=v-1, LLONG_MAX,
/// powers of two, negatives) × literal divisors.
#[test]
fn div_magic_exact_vs_bash() {
    let src = test_source();
    let c = render(&src);
    // the program must actually have taken the magic path
    assert!(
        c.contains("_sh_divp2(") && c.contains("_sh_divodd("),
        "render did not emit the magic:\n{}",
        &c[..c.len().min(2000)]
    );

    let id = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("c_div_magic_{id}"));
    std::fs::create_dir_all(&dir).unwrap();
    let csrc = dir.join("t.c");
    std::fs::File::create(&csrc)
        .unwrap()
        .write_all(c.as_bytes())
        .unwrap();
    let bsrc = dir.join("t.sh");
    std::fs::File::create(&bsrc)
        .unwrap()
        .write_all(src.as_bytes())
        .unwrap();

    let bin = dir.join("t");
    let cc = Command::new("cc")
        .args(["-x", "c", "-O1", "-o"])
        .arg(&bin)
        .arg(&csrc)
        .args(["-lgmp", "-lm"])
        .output()
        .expect("cc must run");
    assert!(
        cc.status.success(),
        "cc failed: {}",
        String::from_utf8_lossy(&cc.stderr)
    );

    let out_c = Command::new(&bin)
        .output()
        .expect("binary must run");
    let out_b = Command::new("bash")
        .arg(&bsrc)
        .output()
        .expect("bash must run");

    assert!(
        out_c.status.success(),
        "C program exited nonzero: {}",
        String::from_utf8_lossy(&out_c.stderr)
    );
    let got = String::from_utf8_lossy(&out_c.stdout);
    let want = String::from_utf8_lossy(&out_b.stdout);
    assert!(!want.is_empty(), "oracle produced no output");
    if got != want {
        let g: Vec<&str> = got.lines().collect();
        let w: Vec<&str> = want.lines().collect();
        let mut shown = 0;
        for (i, (ga, wa)) in g.iter().zip(w.iter()).enumerate() {
            if ga != wa && shown < 20 {
                eprintln!("line {}: C={ga:?} bash={wa:?}", i + 1);
                shown += 1;
            }
        }
        panic!(
            "div/mod mismatch: {} lines (C) vs {} lines (bash)",
            g.len(),
            w.len()
        );
    }
}
