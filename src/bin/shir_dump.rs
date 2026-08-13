use debashl::parser::commands::parse_commands_from_text;
use debashl::shir::ast_to_ir_raw;
use debashl::shir_json::shir_to_shir_json;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = std::fs::read_to_string(&args[1]).unwrap();
    let cmds = parse_commands_from_text(&src).expect("parse");
    let prog = ast_to_ir_raw(&cmds);
    println!("{}", shir_to_shir_json(&prog));
}
