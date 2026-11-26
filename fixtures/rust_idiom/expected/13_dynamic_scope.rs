thread_local! { static __SHV_VALUE: std::cell::Cell<i64> = const { std::cell::Cell::new(0) }; }

fn main() {
    __SHV_VALUE.with(|v| v.set(1));
    __SHV_VALUE.with(|v| v.set(2));
    println!("{}", show());
}

fn show() -> String {
    format!("value={}", __SHV_VALUE.with(|v| v.get()))
}