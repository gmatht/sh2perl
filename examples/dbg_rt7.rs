fn main() {
    let src = std::env::args().nth(1).unwrap_or_else(|| "echo hi > f\necho wrote\n".to_string());
    let cmds = debashl::parser::commands::parse_commands_from_text(&src).unwrap();
    let base = debashl::shir::ast_to_ir(&cmds);
    let mut p = base.clone();
    let c = debashl::transforms::shir_native_stmt::transform(&mut p.stmts);
    println!("changed={c} stmts={:#?}", p.stmts);
}
