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

    // Mirror otranspilerl ingest() step by step.
    let mut p = prog2.clone();
    debashl::transforms::process_subst::transform_program(&mut p);
    dump("proc_subst  ", &p);
    debashl::transforms::arith_normalize::transform(&mut p.stmts);
    dump("arith_norm  ", &p);
    debashl::shir_passes::restructure_goto_only(&mut p);
    dump("restructure ", &p);
    debashl::shir_passes::strip_cfor(&mut p);
    dump("strip_cfor  ", &p);
    {
        use debashl::shir_passes::Transform;
        let ctx = debashl::shir_passes::PassContext::default();
        debashl::shir_passes::transform::ConstantFold.run(&mut p, &ctx);
        dump("const_fold  ", &p);
        debashl::shir_passes::echo_fold::EchoFold.run(&mut p, &ctx);
        dump("echo_fold   ", &p);
    }

    let est = debashl::shir::shir_to_estree(&p);
    let s = serde_json::to_string(&est).unwrap();
    std::fs::write("/home/llm/sh2loop/tmp_run/t73dbg/estree.json", &s).unwrap();
    println!("estree(ingest) written: {} bytes", s.len());

    // shir_to_estree_json() = optimize{const_fold_arith, const_prop, dead_store_elim} + emit
    let mut r = p.clone();
    debashl::shir::const_fold_arith(&mut r);
    dump("cf_arith     ", &r);
    let mut r2 = p.clone();
    debashl::shir_passes::optimize::const_prop(&mut r2);
    dump("const_prop   ", &r2);
    let mut r3 = p.clone();
    debashl::shir_passes::optimize::dead_store_elim(&mut r3);
    dump("dead_store_  ", &r3);
    let mut r4 = p.clone();
    debashl::shir_passes::optimize::optimize(&mut r4);
    dump("optimize(all)", &r4);
    let est4 = debashl::shir::shir_to_estree(&r4);
    let s4 = serde_json::to_string(&est4).unwrap();
    std::fs::write(
        "/home/llm/sh2loop/tmp_run/t73dbg/estree_opt.json",
        &s4,
    )
    .unwrap();
    println!("estree(after optimize): {} bytes", s4.len());
}
