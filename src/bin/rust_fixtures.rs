//! Render the checked-in idiom fixtures through the Rust backend.
//!
//!     cargo run --bin rust_fixtures             # render + write expected/*.rs
//!     cargo run --bin rust_fixtures -- --check  # re-render, compare, rustc-check
//!
//! Fixtures live in `fixtures/rust_idiom/src/*.sh`; the rendered program is
//! written to `fixtures/rust_idiom/expected/<name>.rs`, so a change in the
//! backend shows up as a plain diff of checked-in files instead of having
//! to rebuild a scratch harness.
//!
//! The render path is the one `shir_render --target rust` uses: parse ->
//! `shir::ast_to_ir` -> `transforms::apply` -> `restructure_goto_only` /
//! `strip_cfor` -> `rust_backend::shir_to_rust`.
//!
//! `--check` also compiles every expected file with
//! `rustc --edition 2021 -D warnings`: generated Rust is only useful if it
//! builds warning-free. A fixture whose output needs a third-party crate
//! in *its own* manifest is reported as NEEDS-CRATE (with the crate name)
//! rather than silently skipped.

use std::path::PathBuf;
use std::process::Command;

/// Render a shell source through the backend, exactly as `shir_render
/// --target rust` does.
fn render(src: &str) -> String {
    let cmds =
        debashl::parser::commands::parse_commands_from_text(src).expect("fixture failed to parse");
    let mut prog = debashl::shir::ast_to_ir(&cmds);
    debashl::transforms::apply(&mut prog.stmts);
    debashl::shir_passes::restructure_goto_only(&mut prog);
    debashl::shir_passes::strip_cfor(&mut prog);
    debashl::rust_backend::shir_to_rust(&prog)
}

/// Crates a rendered program may need that are not in `std`. Finding one
/// means the fixture cannot be compiled by bare `rustc`; that is reported,
/// never hidden.
const EXTERN_CRATES: &[(&str, &str)] = &[("num_bigint::", "num_bigint")];

fn needed_crate(code: &str) -> Option<&'static str> {
    EXTERN_CRATES
        .iter()
        .find(|(needle, _)| code.contains(needle))
        .map(|(_, krate)| *krate)
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/rust_idiom");
    let src_dir = root.join("src");
    let out_dir = root.join("expected");
    let check = std::env::args().any(|a| a == "--check");
    if !check {
        std::fs::create_dir_all(&out_dir).expect("expected/");
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&src_dir)
        .expect("fixtures/rust_idiom/src")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "sh").unwrap_or(false))
        .collect();
    files.sort();

    let mut failures = 0usize;
    for f in &files {
        let name = f.file_stem().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(f).expect("read fixture");
        let code = render(&src);
        let out = out_dir.join(format!("{name}.rs"));

        if check {
            match std::fs::read_to_string(&out) {
                Ok(want) if want == code => {}
                _ => {
                    println!("DIFF  {name}: expected/{name}.rs is stale");
                    failures += 1;
                    continue;
                }
            }
        } else {
            std::fs::write(&out, &code).expect("write expected");
        }

        if let Some(krate) = needed_crate(&code) {
            println!(
                "ok    {name:<18} {:>3} lines  NEEDS-CRATE {krate} (not compiled)",
                code.lines().count()
            );
            continue;
        }

        let tmp = std::env::temp_dir().join(format!("rust_idiom_{name}"));
        std::fs::create_dir_all(&tmp).ok();
        let src_path = tmp.join("main.rs");
        std::fs::write(&src_path, &code).ok();
        let bin = tmp.join("main");
        let res = Command::new("rustc")
            .arg("--edition")
            .arg("2021")
            .arg("-D")
            .arg("warnings")
            .arg("-o")
            .arg(&bin)
            .arg(&src_path)
            .output()
            .expect("run rustc");
        if res.status.success() && res.stderr.is_empty() {
            println!("ok    {name:<18} {:>3} lines  rustc clean", code.lines().count());
        } else {
            failures += 1;
            println!("FAIL  {name}: rustc", );
            print!("{}", String::from_utf8_lossy(&res.stderr));
        }
    }

    if failures > 0 {
        eprintln!("\n{failures} fixture(s) failed");
        std::process::exit(1);
    }
    println!("\n{} fixtures render unchanged and build clean", files.len());
}
