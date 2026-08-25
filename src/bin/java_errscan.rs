use debashl::Parser;
use debashl::shir;
fn main() {
    let f = std::env::args().nth(1).unwrap();
    let bytes = std::fs::read(&f).unwrap();
    let content = String::from_utf8_lossy(&bytes).to_string();
    let commands = Parser::new(&content).parse().unwrap();
    let prog = shir::ast_to_ir(&commands);
    match debashl::java_backend::shir_to_java(&prog) {
        Ok(_) => println!("OK"),
        Err(e) => println!("ERR: {e}"),
    }
}
