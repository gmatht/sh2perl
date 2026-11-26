use std::sync::atomic::Ordering;
static __SH_ARITH_ERR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn __sh_pow(a: i64, b: i64) -> i64 {
    let mut r: i64 = 1;
    let mut i = 0;
    while i < b { r = r.wrapping_mul(a); i += 1; }
    r
}

fn __sh_div(a: i64, b: i64) -> i64 {
    if b == 0 { __SH_ARITH_ERR.store(true, Ordering::SeqCst); 0 } else { a / b }
}

fn __sh_mod(a: i64, b: i64) -> i64 {
    if b == 0 { __SH_ARITH_ERR.store(true, Ordering::SeqCst); 0 } else { a % b }
}

fn main() {
    let x = __sh_div(7,2);
    let y = __sh_mod(7,3);
    let p = __sh_pow(2,10);
    println!("{} {} {}", x, y, p);
}