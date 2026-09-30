fn main() {
    let path = std::env::args().nth(1).unwrap();
    let src = std::fs::read_to_string(&path).unwrap();
    let cmds = debashl::parser::commands::parse_commands_from_text(&src).unwrap();
    let base = debashl::shir::ast_to_ir(&cmds);
    let mut p = base.clone();
    debashl::transforms::shir_native_stmt::transform(&mut p.stmts);
    eprintln!("before estree");
    let s1 = debashl::estree::estree_to_json(&debashl::shir::shir_to_estree(&p));
    eprintln!("after estree {} bytes", s1.len());
}
