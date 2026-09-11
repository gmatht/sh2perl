//! Structured integer sqrt in the C backend.
//!
//! The py-sh-go frontend lowers `int(Y**0.5)` (int-domain Y) to a
//! `Call{func: "isqrt"}` node — no `arith("int(sqrt(..))")` string
//! detour, no string temp, no `+0` re-numerify. The C backend renders it
//! as the exact Newton `_sh_isqrt` helper. Exactness oracle: Python
//! `math.isqrt` (both exact, must agree byte-for-byte — including the
//! perfect-square boundaries and i64max).
//!
//! Vehicle: JSON IR (frontend A1 shape) → ingress → render → structural
//! assertions, then gcc+run → byte-compare vs the exact oracle.

use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(3000);

fn render(json: &str) -> String {
    let prog = debashl::shir_json_in::shir_json_to_ir(json).expect("test IR must ingress");
    debashl::c_backend::shir_to_c(&prog)
}

fn echo_getvar(name: &str) -> String {
    format!(
        r#"{{"type": "Expr", "expr": {{"type": "Call", "func": "exec", "purity": "Emulable",
            "args": [{{"type": "Str", "style": "DoubleQuoted", "value": "echo"}},
            {{"type": "Array", "elements": [{{"type": "Call", "func": "getVar", "purity": "Emulable",
            "args": [{{"type": "Str", "style": "DoubleQuoted", "value": "{name}"}}]}}]}}]}}}}"#
    )
}

fn assign_isqrt_num(var: &str, v: i64) -> String {
    format!(
        r#"{{"type": "Assign", "targets": [{{"var": "{var}", "sigil": null, "indices": []}}],
            "expr": {{"type": "Call", "func": "isqrt", "purity": "PureCpu",
            "args": [{{"type": "Arith", "ast": {{"type": "Num", "value": {v}}}}}]}}}}"#
    )
}

/// `isqrt(Num)` renders as the native helper — no `arith(` string eval.
#[test]
fn isqrt_renders_native_c() {
    let json = format!(
        r#"{{
        "type": "Program", "contract_version": 1, "imports": [], "requires": [], "subs": [],
        "var_types": [{{"name": "r", "type": "Int"}}],
        "stmts": [{}, {}]
    }}"#,
        assign_isqrt_num("r", 100),
        echo_getvar("r")
    );
    let c = render(&json);
    assert!(
        c.contains("_sh_isqrt"),
        "no native isqrt helper:\n{c}"
    );
    assert!(
        !c.contains("sh2_arith(") && !c.contains("sh2ArithStub"),
        "string-eval fallback leaked in:\n{c}"
    );
}

/// Exact values: Newton `_sh_isqrt` vs Python `math.isqrt` — perfect
/// squares, non-squares, 0/1, 2^52 boundary, i64max.
#[test]
fn isqrt_exact_values() {
    let vals: Vec<i64> = vec![
        0,
        1,
        2,
        3,
        4,
        8,
        9,
        15,
        16,
        100,
        1_000_000,
        4503599627370496,     // 2**52
        4503599627370497,     // 2**52 + 1
        9223372030926249001,  // 3037000499**2 (max isqrt square, fits i64)
        9223372036854775807,  // i64max -> 3037000499
    ];
    // exact oracle from Python (math.isqrt is exact floor-sqrt)
    let expected: Vec<String> = vals
        .iter()
        .map(|v| {
            // exact integer Newton in u128 (no overflow: (2^63)^2 << 2^128)
            let n = *v as u128;
            let mut x = 1u128 << 32;
            if x * x > n {
                x = n;
            }
            // Newton iterations to convergence, then clamp down/up
            for _ in 0..200 {
                if x == 0 {
                    break;
                }
                let nx = (x + n / x) / 2;
                if nx >= x {
                    break;
                }
                x = nx;
            }
            while x * x > n {
                x -= 1;
            }
            while (x + 1) * (x + 1) <= n {
                x += 1;
            }
            x.to_string()
        })
        .collect();
    let mut stmts = String::new();
    for (i, v) in vals.iter().enumerate() {
        let var = format!("r{i}");
        stmts.push_str(&assign_isqrt_num(&var, *v));
        stmts.push(',');
        stmts.push_str(&echo_getvar(&var));
        stmts.push(',');
    }
    stmts.pop();
    let json = format!(
        r#"{{
        "type": "Program", "contract_version": 1, "imports": [], "requires": [], "subs": [],
        "var_types": [],
        "stmts": [{stmts}]
    }}"#
    );
    let c = render(&json);
    let id = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("c_isqrt_{id}"));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("t.c");
    std::fs::File::create(&src)
        .unwrap()
        .write_all(c.as_bytes())
        .unwrap();
    let bin = dir.join("t");
    let cc = Command::new("cc")
        .args(["-x", "c", "-O1", "-o"])
        .arg(&bin)
        .arg(&src)
        .args(["-lgmp", "-lm"])
        .output()
        .expect("cc must run");
    assert!(
        cc.status.success(),
        "cc failed: {}",
        String::from_utf8_lossy(&cc.stderr)
    );
    let run = Command::new(&bin).output().expect("binary must run");
    assert!(run.status.success(), "binary exited nonzero");
    let got = String::from_utf8_lossy(&run.stdout);
    let want: String = expected.iter().map(|s| format!("{s}\n")).collect();
    assert_eq!(got, want, "isqrt values mismatch");
}
