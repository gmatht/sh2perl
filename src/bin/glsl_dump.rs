// ─── glsl_dump: sh → GLSL probe (debug the glsl_backend sketch) ────
// usage: cargo run --bin glsl_dump -- <file.sh> [es100] [color]
use debashl::glsl_backend::{shir_to_glsl, shir_to_glsl_opts, ShGlslOptions};
use debashl::parser::commands::parse_commands_from_text;
use debashl::shir::ast_to_ir_raw;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).cloned().unwrap_or_else(|| "-".to_string());
    let src = if path == "-" {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).unwrap();
        s
    } else {
        std::fs::read_to_string(&path).expect("read file")
    };
    let es100 = args.iter().any(|a| a == "es100");
    let color = args.iter().any(|a| a == "color");
    let cmds = parse_commands_from_text(&src).expect("parse");
    let prog = ast_to_ir_raw(&cmds);
    let out = if es100 || color {
        shir_to_glsl_opts(&prog, &ShGlslOptions { es100, color_out: color, ..Default::default() })
    } else {
        shir_to_glsl(&prog)
    };
    print!("{out}");
}
