fn main() {
    for entry in std::fs::read_dir("examples").unwrap().flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "sh").unwrap_or(false) {
            let src = std::fs::read_to_string(&path).unwrap_or_default();
            let Ok(cmds) = debashl::Parser::new(&src).parse() else { continue };
            let prog = debashl::shir::ast_to_ir(&cmds);
            let ranges = debashl::shir::analyze_var_ranges(&prog);
            let big = debashl::shir::analyze_big_i53(&prog);
            for v in &big {
                if let Some((lo,hi)) = ranges.get(v) {
                    println!("{}: {} = [{}..{}]", path.file_name().unwrap().to_string_lossy(), v, lo, hi);
                } else { println!("{}: {} = <no range>", path.file_name().unwrap().to_string_lossy(), v); }
            }
        }
    }
}
