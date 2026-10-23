fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let mut ne = 0; let mut nr = 0; let mut tot = 0;
    let mut ne_names: Vec<String> = vec![]; let mut nr_names: Vec<String> = vec![];
    for path in std::env::args().skip(1) {
        let Ok(src) = std::fs::read_to_string(&path) else { continue };
        tot += 1;
        let cmds = match debashl::parser::commands::parse_commands_from_text(&src) { Ok(c) => c, Err(_) => continue };
        let base = debashl::shir::ast_to_ir(&cmds);
        let ea = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| debashl::estree::estree_to_json(&debashl::shir::shir_to_estree(&base)))).unwrap_or_default();
        let ra = debashl::rust_backend::shir_to_rust(&base);
        let mut p = base.clone();
        debashl::transforms::shir_native_stmt::transform(&mut p.stmts);
        let eb = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| debashl::estree::estree_to_json(&debashl::shir::shir_to_estree(&p)))).unwrap_or_default();
        let rb = debashl::rust_backend::shir_to_rust(&p);
        if ea != eb { ne += 1; ne_names.push(path.clone()); }
        if ra != rb { nr += 1; nr_names.push(path.clone()); }
    }
    println!("total={tot} estree_diff={ne} rust_diff={nr}");
    for n in ne_names.iter().take(15) { println!("E {n}"); }
    for n in nr_names.iter().take(15) { println!("R {n}"); }
}
