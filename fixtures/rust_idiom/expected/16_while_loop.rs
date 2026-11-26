use std::sync::atomic::Ordering;

static __SH_RC: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

fn main() {
    let mut c = 3;
    while { let __b = c.to_string().trim().parse::<i64>().unwrap_or(0) > 0; __SH_RC.store(if __b { 0 } else { 1 }, Ordering::SeqCst); __b } {
        c -= 1;
    }
    println!("{}", c);
}