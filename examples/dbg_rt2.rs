// Probe: does running shir_native_stmt ahead of the ESTree emitter change output?
fn main() {
    let src = std::env::args().nth(1).unwrap_or_else(|| "echo hi > f\necho wrote\n".to_string());
    let cmds = debashl::parser::commands::parse_commands_from_text(&src).unwrap();
    let base = debashl::shir::ast_to_ir(&cmds);
    let a = debashl::estree::estree_to_json(&debashl::shir::shir_to_estree(&base));
    let mut progc = base.clone();
    let mut stmts = progc.stmts.clone();
    debashl::transforms::shir_native_stmt::transform(&mut stmts);
    progc.stmts = stmts;
    let b = debashl::estree::estree_to_json(&debashl::shir::shir_to_estree(&progc));
    if a == b { println!("SAME"); } else {
        println!("=== WITHOUT ===\n{a}\n=== WITH ===\n{b}");
    }
}
