use std::sync::atomic::Ordering;

static __SH_RC: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

fn main() {
    let mut a: i64 = 0;
    let mut i: i64 = 0;
    while { let __b = i.to_string().trim().parse::<i64>().unwrap_or(0) < 2; __SH_RC.store(if __b { 0 } else { 1 }, Ordering::SeqCst); __b } {
        a += 2000000000;
        i += 1;
    }
    let b: i64 = a * a;
    println!("{}", b);
}