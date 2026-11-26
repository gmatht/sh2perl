thread_local! { #[allow(non_upper_case_globals)] static __SHV_arr: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) }; }

#[allow(non_upper_case_globals)]
fn main() {
    __SHV_arr.with(|v| *v.borrow_mut() = vec!["1".to_string(), "2".to_string(), "3".to_string()]);
    let mut sum = 0;
    for x in ["1", "2", "3"] {
        sum += x.trim().parse::<i64>().unwrap_or(0);
    }
    println!("sum={} len={}", sum, (__SHV_arr.with(|v| v.borrow().len() as i64)));
}