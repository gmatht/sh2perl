// probe: which vars prove out-of-±2^53 in the corpus
fn main() {
    let mut found: Vec<(String, String, i128, i128)> = Vec::new();
    for entry in std::fs::read_dir("examples").unwrap().flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "sh").unwrap_or(false) {
            let src = std::fs::read_to_string(&path).unwrap_or_default();
            let Ok(cmds) = debashl::Parser::new(&src).parse() else { continue };
            let prog = debashl::shir::ast_to_ir(&cmds);
            let ranges = debashl::shir::analyze_var_ranges(&prog);
            for (n, (lo, hi)) in &ranges {
                if *lo < -(1i128 << 53) || *hi > (1i128 << 53) {
                    found.push((path.display().to_string(), n.clone(), *lo, *hi));
                }
            }
        }
    }
    for (f, n, lo, hi) in &found {
        println!("{}: {} = [{}..{}]", f, n, lo, hi);
    }
    println!("total out-of-i53: {}", found.len());
}
