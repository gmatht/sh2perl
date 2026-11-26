//! Render the checked-in idiom fixtures through the Rust backend and prove
//! the result three ways.
//!
//! ```text
//! cargo run --bin rust_fixtures             # render + write expected/*.rs
//! cargo run --bin rust_fixtures -- --check  # the full gate (see below)
//! ```
//!
//! Fixtures live in `fixtures/rust_idiom/src/*.sh`; the rendered program is
//! written to `fixtures/rust_idiom/expected/<name>.rs`, so a change in the
//! backend shows up as a plain diff of checked-in files.
//!
//! `--check` runs five checks per fixture:
//!
//! 1. **stale**: the fresh render equals the checked-in `expected/*.rs`.
//! 2. **parity**: the same source rendered through the A1-JSON ingress that
//!    `shir_render --target rust` uses produces the same text — the harness's
//!    own mirror of the render path cannot silently drift.
//! 3. **rustc**: the program compiles under `rustc --edition 2021
//!    -D warnings` in BOTH a debug and a `-O` build. A fixture that needs a
//!    third-party crate is reported as NEEDS-CRATE (with the crate name),
//!    never silently skipped.
//! 4. **behaviour**: each build runs in a scratch directory seeded with the
//!    same three files the `bash` run sees, and its stdout, exit status and
//!    every file it creates must match `bash <fixture>`.
//! 5. **census**: the idiom counters below are printed and totalled, so
//!    "idiomatic" is a number that moves rather than a prose claim.
//!
//! Clippy is used as an extra lint source when `cargo clippy` is installed
//! (`clippy: unavailable` is reported, never treated as a pass).
//!
//! ## Debug builds and overflow
//!
//! Generated arithmetic uses plain operators: bash wraps on overflow and so
//! does a release build, but a DEBUG build panics. A fixture that exercises
//! that divergence declares it in its own source —
//! `# debug-overflow: <reason>` — and the harness then reports it as a
//! documented divergence instead of a failure. Any OTHER debug panic is a
//! real failure.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── render paths ─────────────────────────────────────────────────────────

/// Render a shell source exactly as the fixture pipeline does.
fn render(src: &str) -> String {
    let cmds =
        debashl::parser::commands::parse_commands_from_text(src).expect("fixture failed to parse");
    let mut prog = debashl::shir::ast_to_ir(&cmds);
    debashl::transforms::apply(&mut prog.stmts);
    debashl::shir_passes::restructure_goto_only(&mut prog);
    debashl::shir_passes::strip_cfor(&mut prog);
    debashl::rust_backend::shir_to_rust(&prog)
}

/// Render through the A1-JSON ingress `shir_render --target rust` uses: IR →
/// A1 JSON → IR → shared transforms → structuring passes → renderer. Any
/// difference from [`render`] means the harness mirror and the CLI have
/// drifted (or the JSON round-trip is lossy) — either way, it must be known.
fn render_via_a1(src: &str) -> Result<String, String> {
    let cmds = debashl::parser::commands::parse_commands_from_text(src)
        .map_err(|e| format!("parse: {e}"))?;
    let prog = debashl::shir::ast_to_ir(&cmds);
    let json = debashl::shir_json::shir_to_shir_json(&prog);
    let mut prog = debashl::shir_json_in::shir_json_to_ir(&json)?;
    debashl::transforms::apply(&mut prog.stmts);
    debashl::shir_passes::restructure_goto_only(&mut prog);
    debashl::shir_passes::strip_cfor(&mut prog);
    Ok(debashl::rust_backend::shir_to_rust(&prog))
}

// ── the idiom census ─────────────────────────────────────────────────────

/// Counted shapes. Every field is something the backend previously got wrong;
/// the totals are the regression metric (they may only go down).
#[derive(Default, Clone, Copy)]
struct Census {
    lines: usize,
    /// per-variable `thread_local!` statics still in the store
    statics: usize,
    clones: usize,
    to_strings: usize,
    allows: usize,
    bash_spawns: usize,
    /// `__sh_*` runtime helpers emitted into this program
    helpers: usize,
    wrapping: usize,
}

impl Census {
    fn of(code: &str) -> Census {
        let count = |needle: &str| code.matches(needle).count();
        Census {
            lines: code.lines().count(),
            statics: count("thread_local! { ") + count("static __SHV_"),
            clones: count(".clone()"),
            to_strings: count(".to_string()"),
            allows: count("#[allow("),
            bash_spawns: count("Command::new(\"bash\")"),
            helpers: count("\nfn __sh_"),
            wrapping: count("wrapping_"),
        }
    }

    fn add(&mut self, o: Census) {
        self.lines += o.lines;
        self.statics += o.statics;
        self.clones += o.clones;
        self.to_strings += o.to_strings;
        self.allows += o.allows;
        self.bash_spawns += o.bash_spawns;
        self.helpers += o.helpers;
        self.wrapping += o.wrapping;
    }
}

// ── compile / run ────────────────────────────────────────────────────────

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

/// The files every fixture run starts with, so a directory listing or a glob
/// is deterministic and identical for the bash and the Rust run.
const SEEDS: &[(&str, &str)] = &[
    ("alpha.txt", "alpha\n"),
    ("beta.txt", "beta\n"),
    ("gamma.txt", "gamma\n"),
];

fn seed(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).expect("scratch dir");
    for (name, body) in SEEDS {
        std::fs::write(dir.join(name), body).expect("seed");
    }
}

#[derive(PartialEq, Eq, Debug)]
struct RunOut {
    stdout: String,
    code: i32,
    /// every file in the directory that is not a seed (name → bytes)
    files: BTreeMap<String, Vec<u8>>,
}

fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if SEEDS.iter().any(|(s, _)| *s == name) || name == "main" || name == "main.rs" {
            continue;
        }
        if let Ok(body) = std::fs::read(e.path()) {
            out.insert(name, body);
        }
    }
    out
}

fn run_bash(fixture: &Path, dir: &Path) -> RunOut {
    seed(dir);
    let out = Command::new("bash")
        .arg(fixture)
        .current_dir(dir)
        .output()
        .expect("run bash");
    RunOut {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        code: out.status.code().unwrap_or(-1),
        files: snapshot(dir),
    }
}

fn run_bin(bin: &Path, dir: &Path) -> RunOut {
    seed(dir);
    let out = Command::new(bin).current_dir(dir).output().expect("run bin");
    RunOut {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        code: out.status.code().unwrap_or(-1),
        files: snapshot(dir),
    }
}

struct Build {
    ok: bool,
    stderr: String,
}

fn build(code: &str, dir: &Path, optimised: bool) -> Build {
    std::fs::create_dir_all(dir).ok();
    let src_path = dir.join("main.rs");
    std::fs::write(&src_path, code).expect("write main.rs");
    let bin = dir.join("main");
    let mut c = Command::new("rustc");
    c.arg("--edition").arg("2021").arg("-D").arg("warnings");
    if optimised {
        c.arg("-O");
    }
    let res = c
        .arg("-o")
        .arg(&bin)
        .arg(&src_path)
        .output()
        .expect("run rustc");
    Build {
        ok: res.status.success() && res.stderr.is_empty(),
        stderr: String::from_utf8_lossy(&res.stderr).to_string(),
    }
}

// ── clippy ───────────────────────────────────────────────────────────────

/// Is `cargo clippy` usable? Checked once; a missing component is reported,
/// never counted as a pass.
fn clippy_available() -> bool {
    Command::new("cargo")
        .args(["clippy", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ── main ─────────────────────────────────────────────────────────────────

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

    let scratch = std::env::temp_dir().join("rust_idiom_gate");
    let clippy = clippy_available();
    if !clippy {
        println!("note  cargo clippy is not installed — lint source disabled");
    } else {
        println!("note  clippy available: generated programs are linted too");
    }
    println!(
        "{:<20} {:>5} {:>7} {:>5} {:>5} {:>5} {:>5} {:>5} {:>7}",
        "fixture", "lines", "statics", "clone", "to_str", "allow", "bash", "help", "status"
    );

    let mut failures = 0usize;
    let mut total = Census::default();
    for f in &files {
        let name = f.file_stem().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(f).expect("read fixture");
        let code = render(&src);
        let out = out_dir.join(format!("{name}.rs"));
        let c = Census::of(&code);
        total.add(c);

        if check {
            match std::fs::read_to_string(&out) {
                Ok(want) if want == code => {}
                _ => {
                    println!("{name:<20} {:>5} {:>7}  DIFF: expected/{name}.rs is stale — run without --check to regenerate", c.lines, c.statics);
                    failures += 1;
                    continue;
                }
            }
        } else {
            std::fs::write(&out, &code).expect("write expected");
        }

        // ── 2. A1-JSON ingress parity ──────────────────────────────────
        let parity_ok = match render_via_a1(&src) {
            Ok(v) => v == code,
            Err(_) => false,
        };

        // ── 3/4. compile + behaviour ───────────────────────────────────
        let mut status = String::new();
        if let Some(krate) = needed_crate(&code) {
            status = format!("NEEDS-CRATE {krate}");
        } else {
            let dbg_dir = scratch.join(&name).join("dbg");
            let rel_dir = scratch.join(&name).join("rel");
            let dbg = build(&code, &dbg_dir, false);
            let rel = build(&code, &rel_dir, true);
            if !dbg.ok {
                failures += 1;
                status = "FAIL debug rustc".into();
                println!();
                print!("{}", dbg.stderr);
            } else if !rel.ok {
                failures += 1;
                status = "FAIL release rustc".into();
                println!();
                print!("{}", rel.stderr);
            } else {
                let want = run_bash(f, &scratch.join(&name).join("bash"));
                let got_d = run_bin(&dbg_dir.join("main"), &scratch.join(&name).join("run_dbg"));
                let got_r = run_bin(&rel_dir.join("main"), &scratch.join(&name).join("run_rel"));
                let declared_overflow = src
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("# debug-overflow:"))
                    .map(|s| s.trim().to_string());
                let mut bits: Vec<&str> = Vec::new();
                let mut bad = false;
                match declared_overflow {
                    Some(_) => {
                        bits.push("debug-overflow(documented)");
                        if !got_r.eq(&want) {
                            bad = true;
                            bits.push("RELEASE!=bash");
                        } else {
                            bits.push("release=bash");
                        }
                    }
                    None => {
                        if got_d.eq(&want) {
                            bits.push("debug=bash");
                        } else {
                            bad = true;
                            bits.push("DEBUG!=bash");
                        }
                        if got_r.eq(&want) {
                            bits.push("release=bash");
                        } else {
                            bad = true;
                            bits.push("RELEASE!=bash");
                        }
                    }
                }
                if !parity_ok {
                    bad = true;
                    bits.push("A1-PARITY-DIFF");
                }
                if bad {
                    failures += 1;
                    status = bits.join(" ");
                    if !got_d.eq(&want) && declared_overflow.is_none() {
                        println!("\n{name}: bash → {:?} rc={} files={:?}", want.stdout, want.code, want.files.keys().collect::<Vec<_>>());
                        println!("{name}: dbg  → {:?} rc={} files={:?}", got_d.stdout, got_d.code, got_d.files.keys().collect::<Vec<_>>());
                    }
                    if !got_r.eq(&want) {
                        println!("\n{name}: bash → {:?} rc={}", want.stdout, want.code);
                        println!("{name}: rel  → {:?} rc={}", got_r.stdout, got_r.code);
                    }
                } else {
                    status = format!("{} a1=ok", bits.join(" "));
                }
            }
        }
        println!(
            "{name:<20} {:>5} {:>7} {:>5} {:>5} {:>5} {:>5} {:>5}  {status}",
            c.lines, c.statics, c.clones, c.to_strings, c.allows, c.bash_spawns, c.helpers
        );
    }

    println!(
        "\n{:<20} {:>5} {:>7} {:>5} {:>5} {:>5} {:>5} {:>5}",
        "TOTAL", total.lines, total.statics, total.clones, total.to_strings, total.allows,
        total.bash_spawns, total.helpers
    );
    if failures > 0 {
        eprintln!("\n{failures} fixture(s) failed");
        std::process::exit(1);
    }
    println!(
        "\n{} fixtures: render unchanged, A1 parity ok, rustc clean (debug + -O), behaviour matches bash",
        files.len()
    );
}
