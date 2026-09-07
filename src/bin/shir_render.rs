//! shir_render — the generic shIR-JSON → target renderer (the backend
//! worktree gate's entry point).
//!
//! Reads an A1 shIR contract (file arg, or `-` = stdin) and renders it
//! through THIS crate's backend fleet, in-process (no CLI crate, no
//! JSON round-trip through a sibling binary). The backend worktrees use
//! it after `setup_backends.sh --sync`: their branch's copy of this crate
//! compiles THEIR renderer state, and the gate probes/builds
//! `shir_render --target <lang>`.
//!
//!     shir_render --target rust - < a1.json     # stdin A1 → Rust source
//!     shir_render --target estree a1.json       # A1 → ESTree JSON
//!
//! The ingest mirrors otranspilerl's `render` (the byte-verified path the
//! fail/fail-estree gates run): JSON ingress → the shared transforms →
//! (non-estree) restructure_goto_only + strip_cfor → the backend.

use std::io::Read;

fn usage() -> ! {
    eprintln!(
        "usage: shir_render [--target L] [input|-]\n\
         \n\
         input   A1 shIR JSON file, or `-` for stdin\n\
         target  estree pl c go py sh java rs zig glsl glslv lint (default estree)"
    );
    std::process::exit(2);
}

fn main() {
    let mut target = String::new();
    let mut input: Option<String> = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--target" | "-t" => match it.next() {
                Some(t) => target = t,
                None => usage(),
            },
            s if s.starts_with("--target=") => target = s["--target=".len()..].to_string(),
            "-h" | "--help" => usage(),
            _ if input.is_none() => input = Some(a),
            _ => usage(),
        }
    }
    if target.is_empty() {
        // default: the output extension, else estree
        target = input
            .as_deref()
            .and_then(|p| std::path::Path::new(p).extension())
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "estree".into());
    }
    // normalize to the backend kind
    let kind = match target.as_str() {
        "js" | "estree" | "json" => "estree",
        "pl" | "perl" => "perl",
        "py" | "python" => "python",
        "rs" | "rust" => "rust",
        "glslv" => "glslv",
        "c" | "go" | "sh" | "java" | "zig" | "glsl" | "lint" | "shir" => {
            target.as_str()
        }
        other => {
            eprintln!(
                "shir_render: unknown target '{other}' (known: estree pl c go py sh java rs zig glsl glslv lint shir)"
            );
            std::process::exit(3);
        }
    };

    let content = match input.as_deref() {
        None | Some("-") => {
            let mut s = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut s) {
                eprintln!("stdin: {e}");
                std::process::exit(1);
            }
            s
        }
        Some(p) => std::fs::read_to_string(p).unwrap_or_else(|e| {
            eprintln!("shir_render: read {p}: {e}");
            std::process::exit(1);
        }),
    };

    let mut prog = match debashl::shir_json_in::shir_json_to_ir(&content) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ShIR JSON ingress: {e}");
            std::process::exit(1);
        }
    };
    // frontend A1 ingress: the shared transforms (text-ops opt-in via
    // SH2_TRANSFORMS inside apply) — the same channel otranspilerl runs
    debashl::transforms::apply(&mut prog.stmts);
    // the estree target renders goto/cfor natively — skip the structuring
    // passes (otranspilerl ingest parity; restructuring changed gate output
    // for goto/cfor-bearing examples)
    if kind != "estree" {
        debashl::shir_passes::restructure_goto_only(&mut prog);
        debashl::shir_passes::strip_cfor(&mut prog);
    }

    let out: String = match kind {
        "shir" => debashl::shir_json::shir_to_shir_json(&prog),
        "estree" => match debashl::shir::shir_to_estree_json(&prog) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("shir_render: estree: {e}");
                std::process::exit(1);
            }
        },
        "perl" => debashl::ir::shir_to_perl(&prog),
        "c" => debashl::c_backend::shir_to_c(&prog),
        "go" => debashl::go_backend::shir_to_go(&prog),
        "python" => debashl::python_backend::shir_to_python(&prog),
        "sh" => match debashl::sh_backend::shir_to_sh(&prog) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("shir_render: sh: {e}");
                std::process::exit(1);
            }
        },
        "java" => match debashl::java_backend::shir_to_java(&prog) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("shir_render: java: {e}");
                std::process::exit(1);
            }
        },
        "rust" => debashl::rust_backend::shir_to_rust(&prog),
        "zig" => debashl::zig_backend::shir_to_zig(&prog),
        "glsl" => debashl::glsl_backend::shir_to_glsl(&prog),
        "glslv" => debashl::glsl_backend::shir_to_glsl_opts(
            &prog,
            &debashl::glsl_backend::ShGlslOptions {
                es100: true,
                color_out: false,
                vert_out: true,
                tex_size: 32,
                max_view: 800, // the sh2runtime device canvas is 800×600
            },
        ),
        "lint" => debashl::lint_backend::shir_to_lint(&prog),
        _ => unreachable!("kind normalized above"),
    };
    use std::io::Write;
    std::io::stdout()
        .write_all(out.as_bytes())
        .expect("write stdout");
}
