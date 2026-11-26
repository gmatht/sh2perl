use std::sync::atomic::Ordering;

static __SH_RC: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

static __SH_ARITH_ERR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

thread_local! { #[allow(non_upper_case_globals)] static __SHV_COUNT: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) }; }

#[allow(non_upper_case_globals)]
fn main() {
    __SHV_COUNT.with(|v| *v.borrow_mut() = "1".to_string());
    std::env::set_var("COUNT", "1");
    __SHV_COUNT.with(|v| *v.borrow_mut() = { let __v = (__SHV_COUNT.with(|v| v.borrow().clone()).trim().parse::<i64>().unwrap_or(0) + 1i64).to_string(); let __e = __SH_ARITH_ERR.swap(false, Ordering::SeqCst); __SH_RC.store(if __e { 1 } else { 0 }, Ordering::SeqCst); if __e { String::new() } else { __v } });
    println!("count={}", __SHV_COUNT.with(|v| v.borrow().clone()));
}