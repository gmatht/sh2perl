use regex::Regex;
fn main() {
    let input = "`(uname -p 2>/dev/null || \\\n    /sbin/sysctl -n hw.machine_arch 2>/dev/null || \\\n    echo unknown)`";
    println!("Input length: {}", input.len());
    println!("Input: {:?}", input);
    
    let re = Regex::new(r"`([^`\\]|\\.)*`").unwrap();
    if let Some(m) = re.find(input) {
        println!("Matched: {:?} (len={})", m.as_str(), m.len());
    } else {
        println!("NO MATCH!");
    }
}
