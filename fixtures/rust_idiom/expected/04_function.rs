use std::sync::atomic::Ordering;
static __SH_RC: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

fn main() {
    println!("{}", greet("there".to_string()));
}

fn greet(p0: String) -> String {
    let who = p0;
    __SH_RC.store(0, Ordering::SeqCst);
    format!("hi {}", who)
}