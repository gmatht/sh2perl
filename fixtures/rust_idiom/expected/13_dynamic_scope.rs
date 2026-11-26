thread_local! { #[allow(non_upper_case_globals)] static __SHV_value: std::cell::Cell<i64> = const { std::cell::Cell::new(0) }; }

#[allow(non_upper_case_globals)]
fn main() {
    __SHV_value.with(|v| v.set(1));
    __SHV_value.with(|v| v.set(2));
    println!("{}", show());
}

fn show() -> String {
    format!("value={}", __SHV_value.with(|v| v.get()))
}