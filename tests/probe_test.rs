#[test]
fn probe_let_shapes() {
    use debashl::parser::commands::parse_commands_from_text;
    use debashl::shir::ast_to_ir_raw;
    use debashl::shir_json::shir_to_shir_json;
    for src in ["let \"x=1\"", "let \"x+=1\"", "let \"x++\"", "let \"x\""] {
        let cmds = parse_commands_from_text(src).expect("parse");
        let prog = ast_to_ir_raw(&cmds);
        eprintln!("=== {src} ===\n{}", shir_to_shir_json(&prog));
    }
}
