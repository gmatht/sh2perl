// scratch: ShIR JSON on stdin -> sh source on stdout (core renderer).
use std::io::Read;
fn main() {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).unwrap();
    let prog = debashl::shir_json_in::shir_json_to_ir(&s).unwrap();
    print!("{}", debashl::sh_backend::shir_to_sh(&prog).unwrap());
}
