use std::fs;
fn main() {
    let input = fs::read_to_string("/tmp/test_backtick_multiline.sh").unwrap();
    println!("Total length: {}", input.len());
    for (i, b) in input.bytes().enumerate() {
        if i >= 18 && i <= 50 {
            let c = if b == b'\n' { '⏎' } else if b == b'\\' { '\\' } else if b == b'\r' { '¶' } else { b as char };
            println!("  {}: {:3} ({})", i, b, c);
        }
    }
    println!("\nFull input:");
    println!("{:?}", &input[18..50]);
}
