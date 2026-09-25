//! GMP bigint lowering keyed off `Cast(Int64)` — the frontend's
//! proven-big verdict (every bigint leaf in t87's shIR carries it).
//! Demands: exact arithmetic past i64 (no clamp-to-0), and Python FLOOR
//! semantics for `//` / `%` (`mpz_fdiv_q/r`, not truncation: (-7)//2 is
//! -4, not -3 — the current native `/` miscompiles even in i64 range).
//!
//! End-to-end vehicle (mirrors t87_bignum.py): build the IR the way the
//! frontend emits it, render to C, compile with gcc -lgmp, run, and
//! byte-compare against Python-semantics literals. Requires gcc +
//! libgmp at test time.

use debashl::ir::{ArithAst, AssignTarget, IrExpr, IrProgram, IrStmt, IrType, StrStyle};
use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(1000);

/// A frontend-style bigint leaf: every integer the frontend cannot prove
/// within ±2^53 arrives wrapped exactly like this.
fn cast(n: i64) -> ArithAst {
    ArithAst::Cast {
        ty: IrType::Int64,
        arg: Box::new(ArithAst::Num(n)),
    }
}

fn bin(op: &str, l: ArithAst, r: ArithAst) -> ArithAst {
    ArithAst::Bin {
        op: op.to_string(),
        lhs: Box::new(l),
        rhs: Box::new(r),
    }
}

fn neg(a: ArithAst) -> ArithAst {
    ArithAst::Un {
        op: "-".to_string(),
        arg: Box::new(a),
    }
}

fn var(name: &str) -> ArithAst {
    ArithAst::Var(name.to_string())
}

/// Horner digit chain for a >i64 decimal literal, exactly the shape the
/// frontend emits (each digit Cast-wrapped).
fn horner_lit(digits: &str) -> ArithAst {
    let mut acc = cast(0);
    for ch in digits.chars() {
        let d = ch.to_digit(10).expect("digit") as i64;
        acc = bin("+", bin("*", acc, cast(10)), cast(d));
    }
    acc
}

fn assign(var: &str, a: ArithAst) -> IrStmt {
    IrStmt::Assign {
        targets: vec![AssignTarget {
            var: var.to_string(),
            sigil: None,
            indices: vec![],
        }],
        expr: IrExpr::Arith(Box::new(a)),
        asm: None,
    }
}

/// `print(<arith>)` in frontend shape: exec(echo, [arith]).
fn print_arith(a: ArithAst) -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "exec".to_string(),
        args: vec![
            IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
            IrExpr::Array(vec![IrExpr::Arith(Box::new(a))]),
        ],
    })
}

fn prog(stmts: Vec<IrStmt>) -> IrProgram {
    IrProgram {
        imports: vec![],
        requires: vec![],
        stmts,
        subs: vec![],
        var_types: vec![("x".to_string(), IrType::Int)],
        stmt_lines: vec![],
        var_lengths: vec![],
        var_const: vec![],
        var_lifetimes: vec![],
        var_nospace: vec![],
        var_storage: vec![],
        var_bash_env: vec![],
    }
}

fn c_stdout(p: &IrProgram) -> Vec<u8> {
    let c = debashl::c_backend::shir_to_c(p);
    let id = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "c_bigint_gmp_{}_{}",
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
        "gcc -lgmp failed:\n{}",
        String::from_utf8_lossy(&cc.stderr)
    );
    let run = Command::new(&bin).output().expect("run C binary");
    assert!(run.status.success(), "C binary failed: {run:?}");
    let _ = std::fs::remove_dir_all(&dir);
    run.stdout
}

fn lines(s: &[u8]) -> Vec<String> {
    String::from_utf8(s.to_vec())
        .expect("utf8")
        .lines()
        .map(|l| l.to_string())
        .collect()
}

#[test]
fn bigint_t87_core_exact() {
    // t87_bignum.py, minus the duplicate prints: every line must be
    // byte-exact, including past-i64 magnitudes (2**100, 2**128,
    // 2**64-1 as UNSIGNED magnitude, the 30-digit Horner literal).
    let x = || var("x");
    let stmts = vec![
        assign("x", bin("**", cast(2), cast(100))),
        print_arith(x()),
        print_arith(bin("+", x(), cast(1))),
        print_arith(bin("*", x(), cast(3))),
        print_arith(bin("%", x(), cast(7))),
        print_arith(bin("/", x(), cast(7))),
        print_arith(bin(
            "+",
            bin("**", cast(10), cast(30)),
            cast(123456789),
        )),
        print_arith(bin("**", bin("**", cast(2), cast(64)), cast(2))),
        print_arith(bin("-", bin("**", cast(2), cast(64)), cast(1))),
        print_arith(bin("+", horner_lit("123456789012345678901234567890"), cast(1))),
        print_arith(neg(x())),
        print_arith(bin("/", x(), cast(1000000007))),
        print_arith(bin("%", x(), cast(1000000007))),
    ];
    let got = lines(&c_stdout(&prog(stmts)));
    let want = [
        "1267650600228229401496703205376",
        "1267650600228229401496703205377",
        "3802951800684688204490109616128",
        "2",
        "181092942889747057356671886482",
        "1000000000000000000000123456789",
        "340282366920938463463374607431768211456",
        "18446744073709551615",
        "123456789012345678901234567891",
        "-1267650600228229401496703205376",
        "1267650591354675262013",
        "976371285",
    ];
    assert_eq!(got, want);
}

#[test]
fn bigint_floor_divmod_negative() {
    // Python floor semantics: the current native `/` truncates (-3),
    // GMP must use fdiv (-4). Fails pre-fix even though every value
    // fits i64 — the bug is the OPERATOR, not the magnitude.
    let stmts = vec![
        print_arith(bin("/", neg(cast(7)), cast(2))),
        print_arith(bin("%", neg(cast(7)), cast(2))),
        print_arith(bin("/", cast(7), neg(cast(2)))),
        print_arith(bin("%", cast(7), neg(cast(2)))),
    ];
    let got = lines(&c_stdout(&prog(stmts)));
    assert_eq!(got, ["-4", "1", "-4", "-1"]);
}
