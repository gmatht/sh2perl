//! render_all_nodes — probe: parse real shell snippets through ast_to_ir
//! (the reachable shIR node set) and render the result through each
//! non-Perl/non-sh backend, catching refusals (panic / mark_todo /
//! unsupported / die).
//!
//! usage: cargo run --bin render_all_nodes -- [backend]
use debashl::ir::*;
use debashl::shir::ast_to_ir;
use std::panic;

const SNIPPETS: &[(&str, &str)] = &[
    ("Output", "echo hi"),
    ("WriteFile", "echo hi > /tmp/f"),
    ("Assign", "x=1"),
    ("Assign2", "x=$y"),
    ("Arith", "x=$((1+2))"),
    ("Declare", "local x=1"),
    ("DeclareArray", "declare -a arr=(1 2)"),
    ("DeclareAssoc", "declare -A m=([a]=1)"),
    ("If", "if true; then echo a; fi"),
    ("For", "for i in a b; do echo $i; done"),
    ("ForInit", "for ((i=0;i<3;i++)); do echo $i; done"),
    ("Continue", "for i in a b; do continue; done"),
    ("Break", "for i in a b; do break; done"),
    ("While", "while true; do echo x; done"),
    ("DoWhile", "until true; do echo x; done"),
    ("Return", "f(){ return 1; }"),
    ("Exit", "exit 1"),
    ("Case", "case x in a) echo a;; esac"),
    ("Redirect", "echo hi > /tmp/r"),
    ("Function", "f(){ echo x; }"),
    ("Subshell", "(echo hi)"),
    ("Background", "echo hi &"),
    ("Block", "{ echo hi; }"),
    ("Pipeline", "echo a | wc -l"),
    ("VarTest", "[ -f /tmp/x ]"),
    ("SetVar", "x=5"),
];

fn main() {
    let backend = std::env::args().nth(1);
    let render: Vec<(&str, Box<dyn Fn(&IrProgram) -> Result<String, String>>)> = vec![
        ("c", Box::new(|p| Ok(debashl::c_backend::shir_to_c(p)))),
        ("glsl", Box::new(|p| Ok(debashl::glsl_backend::shir_to_glsl(p)))),
        ("go", Box::new(|p| Ok(debashl::go_backend::shir_to_go(p)))),
        ("java", Box::new(|p| debashl::java_backend::shir_to_java(p))),
        ("python", Box::new(|p| Ok(debashl::python_backend::shir_to_python(p)))),
        ("rust", Box::new(|p| Ok(debashl::rust_backend::shir_to_rust(p)))),
        ("zig", Box::new(|p| Ok(debashl::zig_backend::shir_to_zig(p)))),
        ("js", Box::new(|p| Ok(debashl::js_backend::shir_to_js(p)))),
    ];
    let markers = [
        "TODO(unsupported)", "TODO", "unsupported", "not yet supported",
        "not in the v1 subset", "not supported", "debashc: shIR", "refuse",
        "v1 subset",
    ];
    for (name, f) in &render {
        if backend.as_deref().map_or(false, |b| b != *name) {
            continue;
        }
        println!("== {name} ==");
        for (label, src) in SNIPPETS {
            let cmds = match debashl::parser::commands::parse_commands_from_text(src) {
                Ok(c) => c,
                Err(_) => { println!("{label:12} -> (parse failed)"); continue; }
            };
            let prog = ast_to_ir(&cmds);
            let r = panic::catch_unwind(panic::AssertUnwindSafe(|| f(&prog)));
            match r {
                Ok(Ok(text)) => {
                    let found: Vec<&str> = markers.iter().filter(|m| text.contains(**m)).copied().collect();
                    let status = if found.is_empty() { "ok".to_string() } else { format!("REFUSED {found:?}") };
                    println!("{label:12} -> {status}");
                }
                Ok(Err(e)) => println!("{label:12} -> ERR: {e}"),
                Err(_) => println!("{label:12} -> PANIC"),
            }
        }
    }
}
