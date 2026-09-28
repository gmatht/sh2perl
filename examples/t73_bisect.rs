use debashl::ir::{IrProgram, IrStmt};
use debashl::shir::shir_to_estree;

fn has_x_assign(prog: &IrProgram) -> bool {
    prog.stmts
        .iter()
        .any(|s| matches!(s, IrStmt::Assign { targets, .. } if targets.iter().any(|t| t.var == "x")))
}

fn dump(tag: &str, prog: &IrProgram) {
    println!(
        "{tag}: stmts={} x_assign={}",
        prog.stmts.len(),
        has_x_assign(prog)
    );
}

fn main() {
    let a1 = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();
    let prog = debashl::shir_json_in::shir_json_to_ir(&a1).expect("ingress");
    dump("ingress     ", &prog);

    let (_ctx, prog2, _m) = debashl::shir_passes::Pipeline::canonical().run(&prog);
    dump("after-pipe  ", &prog2);

    let est = debashl::shir::shir_to_estree(&prog2);
    let s = serde_json::to_string(&est).unwrap();
    println!("estree: {}", &s[..s.len().min(600)]);
}
