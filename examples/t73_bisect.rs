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

    // render_ingested(): set_source_lang + promote_ascii_vars
    let mut q = p.clone();
    debashl::shir::promote_ascii_vars(&mut q);
    dump("ascii_vars  ", &q);
    let est2 = debashl::shir::shir_to_estree(&q);
    let s2 = serde_json::to_string(&est2).unwrap();
    std::fs::write("/home/llm/sh2loop/tmp_run/t73dbg/estree2.json", &s2).unwrap();
    println!("estree(after ascii_vars): {} bytes", s2.len());

    // render_with_source(): set_source_lang(SOME) before rendering
    for lang in ["zsh", "sh", "fish", "py", "go", "c", "pl"] {
        debashl::shir::set_source_lang(Some(lang));
        let est3 = debashl::shir::shir_to_estree(&q);
        let s3 = serde_json::to_string(&est3).unwrap();
        std::fs::write(
            format!("/home/llm/sh2loop/tmp_run/t73dbg/estree_{lang}.json"),
            &s3,
        )
        .unwrap();
        let has = s3.contains("\"x\"");
        println!("src_lang={lang}: {} bytes, mentions x={has}", s3.len());
    }
    debashl::shir::set_source_lang(None);
}
