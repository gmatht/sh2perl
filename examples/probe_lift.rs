fn main() {
    use debashl::parser::commands::parse_commands_from_text;
    use debashl::shir::ast_to_ir_raw;
    use debashl::shir_passes::Pipeline;
    let src = "if echo \"$x\" | grep PAT; then A; else B; fi";
    let cmds = parse_commands_from_text(src).expect("parse");
    let prog = ast_to_ir_raw(&cmds);
    let (_ctx, out, _m) = Pipeline::canonical().run(&prog);
    let json = debashl::shir_json::shir_to_shir_json(&out);
    println!("HAS_CASE: {}", json.contains("Case"));
    println!("FIRST_50: {}", &json[..json.len().min(200)]);
}
