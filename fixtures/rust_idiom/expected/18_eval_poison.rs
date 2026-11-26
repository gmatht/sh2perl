use std::sync::atomic::Ordering;
static __SH_RC: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

static __SH_ARITH_ERR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

thread_local! { static __SHV_N: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) }; }

fn main() {
    __SHV_N.with(|v| *v.borrow_mut() = "0".to_string());
    __SHV_N.with(|v| *v.borrow_mut() = { let __v = (__SHV_N.with(|v| v.borrow().trim().parse::<i64>().unwrap_or(0)) + 1i64).to_string(); let __e = __SH_ARITH_ERR.swap(false, Ordering::SeqCst); __SH_RC.store(if __e { 1 } else { 0 }, Ordering::SeqCst); if __e { String::new() } else { __v } });
    __SHV_N.with(|v| *v.borrow_mut() = format!("{}", (__SHV_N.with(|v| v.borrow().trim().parse::<i64>().unwrap_or(0)) + 10)));
    println!("{}", __SHV_N.with(|v| v.borrow().clone()));
}