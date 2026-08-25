fn main() {
    use debashl::parser::commands::parse_commands_from_text;
    use debashl::shir::ast_to_ir_raw;
    use debashl::shir_json::shir_to_shir_json;
    let src = std::env::args().nth(1).unwrap_or_else(|| "let \"x+=1\"".into());
    let cmds = parse_commands_from_text(&src).expect("parse");
    let prog = ast_to_ir_raw(&cmds);
    println!("{}", shir_to_shir_json(&prog));
}
