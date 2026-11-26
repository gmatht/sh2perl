fn main() {
    let mut count = 0;
    count += 1;
    let name = "world".to_string();
    if name.clone().contains("orl") {
        println!("hi {} {}", name.clone(), count);
    }
    let mut total = 0;
    for n in [1, 2, 3] {
        total += n;
    }
    println!("total={}", total);
}