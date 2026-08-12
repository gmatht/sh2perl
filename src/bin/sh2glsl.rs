// ─── sh2glsl: bash → GLSL ES 1.00 render-fragment compiler ─────────
// The MIMEcroft game authors its fragment shader IN BASH (a pure
// integer program over the frag_x/frag_y/vcolor_* inputs) and compiles
// it here: the glsl_backend emits a GLSL ES 1.00 fragment shader that
// pairs with the game's WebGL1 vertex shader —
//   inputs:  frag_x/frag_y ← int(gl_FragCoord.xy)
//            vcolor_r/g/b  ← int(vColor.rgb * 255.0)  (varying)
//   output:  out_buf bytes 0..3 → gl_FragColor  (via `putb N`)
// usage: sh2glsl <file.sh>     (or stdin via `-`)
use debashl::glsl_backend::{shir_to_glsl_opts, ShGlslOptions};
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
        std::fs::read_to_string(&path).expect("read shader file")
    };
    let cmds = parse_commands_from_text(&src).expect("parse shader program");
    let prog = ast_to_ir_raw(&cmds);
    print!(
        "{}",
        shir_to_glsl_opts(
            &prog,
            &ShGlslOptions {
                es100: true,      // pair with the WebGL1 vertex shader
                color_out: true,  // out_buf bytes 0..3 → gl_FragColor
                tex_size: 16,     // texture bridges (uv_x/uv_y + tex_r/g/b)
            }
        )
    );
}
