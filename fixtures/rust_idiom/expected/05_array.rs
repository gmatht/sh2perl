type Word = std::borrow::Cow<'static, str>;

thread_local! { #[allow(non_upper_case_globals)] static __SHV_arr: std::cell::RefCell<Vec<Word>> = const { std::cell::RefCell::new(Vec::new()) }; }

#[allow(non_upper_case_globals)]
fn main() {
    __SHV_arr.with(|v| *v.borrow_mut() = vec!["1".into(), "2".into(), "3".into()]);
    let mut sum: i64 = 0;
    for x in ["1", "2", "3"] {
        sum += x.trim().parse::<i64>().unwrap_or(0);
    }
    println!("sum={} len={}", sum, (__SHV_arr.with(|v| v.borrow().len() as i64)));
}