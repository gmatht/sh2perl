// ─── sh2glsl: bash → GLSL ES 1.00 shader compiler ─────────────────
// The MIMEcroft game authors BOTH shader stages IN BASH (pure integer
// programs over the bridge inputs) and compiles them here: the
// glsl_backend emits a GLSL ES 1.00 fragment shader by default (the
// frag_x/frag_y/vcolor/uv/tex/crack bridges + `putb` colour output)
// and a GLSL ES 1.00 VERTEX shader with `--vertex` (the
// ap_*/ash_*/auv_*/ucp_*/ucy_*/uop_*/usc_*/ublk_*/uov bridges + the
// vp_*/vc_*/vu_* outputs).
// usage: sh2glsl [--vertex] <file.sh>   (or stdin via `-`)
use debashl::glsl_backend::{shir_to_glsl_opts, ShGlslOptions};
use debashl::parser::commands::parse_commands_from_text;
use debashl::shir::ast_to_ir_raw;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut vert = false;
    let mut path = "-".to_string();
    for a in args.iter().skip(1) {
        if a == "--vertex" {
            vert = true;
        } else if !a.starts_with("--") {
            path = a.clone();
        }
    }
    let src = if path == "-" {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).unwrap();
        s
    } else {
        std::fs::read_to_string(&path).expect("read shader file")
    };
    let cmds = parse_commands_from_text(&src).expect("parse shader program");
    let prog = ast_to_ir_raw(&cmds);
    print!(
        "{}",
        shir_to_glsl_opts(
            &prog,
            &ShGlslOptions {
                es100: true,      // pair with the WebGL1 pipeline
                color_out: !vert, // fragment: out_buf bytes → gl_FragColor
                vert_out: vert,   // vertex: vp_*/vc_*/vu_* → gl_Position/varyings
                tex_size: 16,     // texture bridges
                max_view: 800,   // the sh2runtime device canvas is 800×600 (mediump gate)
            }
        )
    );
}
