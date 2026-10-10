// Probe: does running shir_native_stmt change the RUST output for py-style A1?
fn rs(prog: &debashl::ir::IrProgram) -> String { debashl::rust_backend::shir_to_rust(prog) }
fn main() {
    let src = std::env::args().nth(1).unwrap_or_else(|| "echo hi > f\necho wrote\n".to_string());
    let cmds = debashl::parser::commands::parse_commands_from_text(&src).unwrap();
    let base = debashl::shir::ast_to_ir(&cmds);
    let a = rs(&base);
    let mut p = base.clone();
    debashl::transforms::shir_native_stmt::transform(&mut p.stmts);
    let b = rs(&p);
    if a == b { println!("SAME"); } else {
        println!("=== WITHOUT ===\n{a}\n=== WITH ===\n{b}");
    }
}
