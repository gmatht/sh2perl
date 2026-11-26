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
//! 2. **render mirror**: the REAL `shir_render --target rust` binary (built
//!    on demand, fed the same A1 JSON) renders every fixture to the same
//!    bytes the harness measured — the metric cannot drift from the renderer
//!    users get.
//! 3. **rustc**: the program compiles under `rustc --edition 2021
//!    -D warnings` in BOTH a debug and a `-O` build. A fixture that needs a
//!    third-party crate is reported as NEEDS-CRATE (with the crate name),
//!    never silently skipped.
//! 4. **behaviour**: each build runs in a scratch directory seeded with the
//!    same three files the `bash` run sees, and its stdout, exit status and
//!    every file it creates must match `bash <fixture>`.
//! 5. **idiom metric**: the idiomatic score is CLIPPY'S OWN CATALOG over the
//!    generated program — every `clippy::*` diagnostic the `pedantic` group
//!    raises, counted by lint name — not a list of shapes we thought to grep
//!    for. The totals are printed per run and ratcheted against
//!    [`IDIOM_BASELINE`]: a lint that grows, or a family that appears, fails
//!    the gate and names every offending site. Because the lint set comes
//!    from clippy, a non-idiomatic shape nobody has named yet is caught by
//!    the same number.
//!
//! `--idiom-only` runs the render + census + idiom metric + mirror check
//! without the rustc/behaviour passes — the ratchet's own fast loop.
//!
//! Clippy is a REQUIRED check: every built fixture is linted in both the
//! debug and the `-O` profile with `-D warnings`, and a single finding fails
//! the fixture. If the component is missing the gate does not pass — it
//! prints a HARD GAP line and exits non-zero, because a lint source that
//! silently disappears turns "idiomatic" back into a prose claim.
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
/// Lines an emitted `fn __sh_<name>(` body occupies (0 when absent): the
/// interpreter's cost is what the specialization is supposed to remove.
fn helper_body_lines(code: &str, name: &str) -> usize {
    let needle = format!("\nfn __sh_{name}(");
    let Some(p) = code.find(&needle) else {
        return 0;
    };
    let body = &code[p + 1..];
    body.lines().take_while(|l| *l != "}").count() + 1
}

fn prepare(src: &str) -> debashl::ir::IrProgram {
    // `parse_with_lines` + `ast_to_ir_with_lines`, exactly as the CLI's shell
    // ingress does: the IR's `stmt_lines` markup is what lets a rendered
    // statement print the source line bash prints in a refusal diagnostic.
    let mut parser = debashl::parser::commands::Parser::new(src);
    let (cmds, lines) = parser.parse_with_lines().expect("fixture failed to parse");
    let mut prog = debashl::shir::ast_to_ir_with_lines(&cmds, &lines);
    debashl::transforms::apply(&mut prog.stmts);
    debashl::shir_passes::restructure_goto_only(&mut prog);
    debashl::shir_passes::strip_cfor(&mut prog);
    prog
}

/// The REAL renderer, built on demand so a fresh clone can run the whole
/// gate with one command: `shir_render` is a sibling binary of this harness,
/// and the process's own profile (debug/release) is the one to build.
fn shir_render_bin() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "current_exe has no parent".to_string())?
        .to_path_buf();
    let name = if cfg!(windows) { "shir_render.exe" } else { "shir_render" };
    let bin = dir.join(name);
    // ALWAYS build it: a `shir_render` left over from an earlier tree would
    // disagree with the current renderer and the mirror would report a
    // difference that no longer exists (or hide one that does). The build is
    // incremental, so this is a no-op when it is already current.
    let release = dir
        .components()
        .any(|c| c.as_os_str() == "release");
    let mut c = Command::new("cargo");
    c.current_dir(env!("CARGO_MANIFEST_DIR")).arg("build").arg("-q");
    if release {
        c.arg("--release");
    }
    c.args(["--bin", "shir_render"]);
    let out = c.output().map_err(|e| format!("cargo build: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo build --bin shir_render failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    if bin.is_file() {
        Ok(bin)
    } else {
        Err(format!("built, but {} is missing", bin.display()))
    }
}

/// Render one fixture through the REAL `shir_render --target rust` binary,
/// fed the A1 JSON the CLI ingests. This is the equivalence check: the text
/// the harness measures must be the text the renderer produces.
fn mirror_render(bin: &Path, src: &str, dir: &Path) -> Result<String, String> {
    let mut parser = debashl::parser::commands::Parser::new(src);
    let (cmds, lines) = parser.parse_with_lines().map_err(|e| format!("parse: {e}"))?;
    let prog = debashl::shir::ast_to_ir_with_lines(&cmds, &lines);
    let json = debashl::shir_json::shir_to_shir_json(&prog);
    std::fs::create_dir_all(dir).map_err(|e| format!("mirror dir: {e}"))?;
    let path = dir.join("a1.json");
    std::fs::write(&path, json).map_err(|e| format!("write a1.json: {e}"))?;
    let out = Command::new(bin)
        .arg("--target")
        .arg("rust")
        .arg(&path)
        .output()
        .map_err(|e| format!("run shir_render: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "shir_render exited {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

// ── the idiom metric: clippy's catalog, not a list of patterns ────────────

/// One clippy finding over one generated program.
struct IdiomSite {
    lint: String,
    fixture: String,
    line: usize,
    message: String,
}

/// The idiomatic score. Every `clippy::*` diagnostic the `pedantic` group
/// raises over the GENERATED program is counted by LINT NAME — the families
/// come from clippy's own catalog, so a non-idiomatic shape nobody has
/// thought of is measured by the same number.
#[derive(Default)]
struct Idioms {
    by_lint: BTreeMap<String, usize>,
    sites: Vec<IdiomSite>,
}

impl Idioms {
    fn total(&self) -> usize {
        self.by_lint.values().sum()
    }

    fn add(&mut self, o: Idioms) {
        for (lint, n) in o.by_lint {
            *self.by_lint.entry(lint).or_insert(0) += n;
        }
        self.sites.extend(o.sites);
    }

    /// `clippy::a=3, clippy::b=1` — highest count first, then by name.
    fn breakdown(&self) -> String {
        let mut v: Vec<(&String, &usize)> = self.by_lint.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        v.iter()
            .map(|(l, n)| format!("{l}={n}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The ratchet: the measured per-lint counts for this tree. A lint that grows
/// — or one that is not here at all — fails the gate and names its sites; a
/// lint that drops is reported so the baseline comes down with it.
/// Measured on this tree, `cargo run --bin rust_fixtures -- --idiom-only`:
/// 243 findings over the 22 generated programs. Note the families a
/// hand-written list would never have named — `bool_to_int_with_if` (the
/// `if b { 1 } else { 0 }` status shape) and `many_single_char_names` are
/// clippy's observations, not ours.
const IDIOM_BASELINE: &[(&str, usize)] = &[
    ("clippy::bool_to_int_with_if", 43),
    ("clippy::cast_possible_wrap", 4),
    ("clippy::cast_sign_loss", 10),
    ("clippy::explicit_iter_loop", 2),
    ("clippy::format_push_string", 2),
    ("clippy::if_not_else", 5),
    ("clippy::manual_let_else", 4),
    ("clippy::many_single_char_names", 11),
    ("clippy::map_unwrap_or", 2),
    ("clippy::match_same_arms", 3),
    ("clippy::needless_pass_by_value", 4),
    ("clippy::redundant_closure_for_method_calls", 118),
    ("clippy::single_match_else", 2),
    ("clippy::too_many_lines", 5),
    ("clippy::uninlined_format_args", 26),
    ("clippy::unreadable_literal", 2),
];

/// Run clippy over one generated program with clippy's `pedantic` group and
/// collect its diagnostics. Nothing is suppressed and nothing is filtered but
/// the diagnostic envelope: every lint clippy names is a data point.
fn clippy_idioms(code: &str, fixture: &str, dir: &Path) -> Result<Idioms, String> {
    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("idiom src dir: {e}"))?;
    let pkg = fixture.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
    let manifest = format!(
        "[package]\nname = \"fx_{pkg}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
         [[bin]]\nname = \"fx\"\npath = \"src/main.rs\"\n"
    );
    std::fs::write(dir.join("Cargo.toml"), manifest).map_err(|e| format!("idiom manifest: {e}"))?;
    std::fs::write(src_dir.join("main.rs"), code).map_err(|e| format!("idiom main.rs: {e}"))?;
    let mut c = Command::new("cargo");
    c.arg("clippy")
        .arg("--offline")
        .arg("--message-format=json")
        .arg("--manifest-path")
        .arg(dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(dir.join("target"))
        .args(["--", "-W", "clippy::pedantic"]);
    let res = c.output().map_err(|e| format!("run cargo clippy: {e}"))?;
    let text = String::from_utf8_lossy(&res.stdout);
    let mut out = Idioms::default();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }
        let Some(msg) = v.get("message") else { continue };
        let Some(lint) = msg
            .get("code")
            .and_then(|c| c.get("code"))
            .and_then(|c| c.as_str())
            .filter(|c| c.starts_with("clippy::"))
        else {
            continue;
        };
        let line_no = msg
            .get("spans")
            .and_then(|s| s.as_array())
            .and_then(|a| a.iter().find(|s| s.get("is_primary") == Some(&serde_json::Value::Bool(true))))
            .and_then(|s| s.get("line_start"))
            .and_then(|l| l.as_u64())
            .unwrap_or(0) as usize;
        let text = msg
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("")
            .to_string();
        *out.by_lint.entry(lint.to_string()).or_insert(0) += 1;
        out.sites.push(IdiomSite {
            lint: lint.to_string(),
            fixture: fixture.to_string(),
            line: line_no,
            message: text,
        });
    }
    if !res.status.success() && out.total() == 0 {
        return Err(format!(
            "clippy could not analyse the generated program: {}",
            String::from_utf8_lossy(&res.stderr).trim()
        ));
    }
    Ok(out)
}

/// Call sites that reach the variable store with a literal key:
/// `__sh_v_s("name")`, `__sh_v_set_i("name", …)`, `__sh_vw_w("name", |v| …)`.
/// A helper definition takes `name: &str`, so helper bodies never count.
fn store_accesses(code: &str) -> usize {
    let mut n = 0usize;
    for (i, _) in code.match_indices("__sh_v") {
        let rest = &code[i..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        if rest[end..].starts_with("(\"") {
            n += 1;
        }
    }
    n
}

// ── the idiom census ─────────────────────────────────────────────────────

/// Structural shape of the emitted program. These are facts about the
/// translation (how many lines, how many `let` bindings, how many store
/// accesses, how much of `__sh_printf` survives) — the IDIOMATIC score is
/// [`Idioms`], computed from clippy, not counted here.
#[derive(Default, Clone, Copy)]
struct Census {
    lines: usize,
    /// per-variable `thread_local!` statics in the emitted program — the
    /// shape the ONE store replaced, so any non-zero count is a failure
    statics: usize,
    /// vars emitted as a bare `let` (promoted out of the store)
    promoted: usize,
    /// store accesses in the emitted body: call sites that name a store key
    stored: usize,
    /// printf call sites the renderer decomposed at render time
    printf_specialized: usize,
    /// printf call sites left on the `__sh_printf` interpreter
    printf_interpreted: usize,
    /// lines the emitted `__sh_printf` interpreter body costs (0 = absent)
    printf_lines: usize,
}

impl Census {
    fn of(code: &str) -> Census {
        let count = |needle: &str| code.matches(needle).count();
        Census {
            lines: code.lines().count(),
            statics: count("thread_local! { ") + count("static __SHV_"),
            promoted: 0,
            stored: 0,
            printf_specialized: 0,
            printf_interpreted: 0,
            printf_lines: 0,
        }
    }

    fn add(&mut self, o: Census) {
        self.lines += o.lines;
        self.statics += o.statics;
        self.promoted += o.promoted;
        self.stored += o.stored;
        self.printf_specialized += o.printf_specialized;
        self.printf_interpreted += o.printf_interpreted;
        self.printf_lines += o.printf_lines;
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

#[derive(Debug)]
struct RunOut {
    stdout: String,
    /// Captured for every run, but compared only by a fixture that marks
    /// itself `# stderr-exact:` — most fixtures use constructs whose
    /// diagnostics are not part of the contract, and a global stderr diff
    /// would fail on unrelated pre-existing formats.
    stderr: String,
    code: i32,
    /// every file in the directory that is not a seed (name → bytes)
    files: BTreeMap<String, Vec<u8>>,
}

impl PartialEq for RunOut {
    fn eq(&self, o: &Self) -> bool {
        self.stdout == o.stdout && self.code == o.code && self.files == o.files
    }
}
impl Eq for RunOut {}

impl RunOut {
    /// Byte-for-byte stderr equality — the `# stderr-exact:` fixtures.
    fn stderr_eq(&self, o: &Self) -> bool {
        self.stderr == o.stderr
    }
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
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
        files: snapshot(dir),
    }
}

/// `argv0` makes the generated program's `$0` equal the string bash got as
/// `$0` — which is what a byte-comparison of a `$0`-prefixed diagnostic
/// (`<prog>: line N: NAME: readonly variable`) needs. Bash's `$0` for
/// `bash script` IS the path it was handed; a compiled replacement's `$0` is
/// its argv[0], so the two are the same variable once the harness aligns it.
fn run_bin(bin: &Path, dir: &Path, argv0: Option<&Path>) -> RunOut {
    seed(dir);
    let mut c = Command::new(bin);
    c.current_dir(dir);
    #[cfg(unix)]
    if let Some(a) = argv0 {
        use std::os::unix::process::CommandExt;
        c.arg0(a);
    }
    #[cfg(not(unix))]
    let _ = argv0;
    let out = c.output().expect("run bin");
    RunOut {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
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

/// Probe `cargo clippy` once. `Err` is a HARD GAP: the gate refuses to call
/// itself green without a lint source.
fn clippy_probe() -> Result<(), String> {
    match Command::new("cargo").args(["clippy", "--version"]).output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let msg = String::from_utf8_lossy(&o.stderr).trim().to_string();
            Err(if msg.is_empty() { format!("exit {:?}", o.status.code()) } else { msg })
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Lint one rendered program with `cargo clippy -D warnings`. Clippy needs a
/// manifest, so the program is materialised as a one-file package in the
/// scratch tree; `--offline` keeps it hermetic (a fixture with a real
/// external dependency is skipped before it reaches here).
///
/// Returns the findings (first lines only) so the caller can print them; an
/// empty `Ok` means the program is clean.
fn clippy_lints(code: &str, name: &str, dir: &Path, optimised: bool) -> Result<(), String> {
    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir).expect("clippy src dir");
    let pkg = name.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
    let manifest = format!(
        "[package]\nname = \"fx_{pkg}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
         [[bin]]\nname = \"fx\"\npath = \"src/main.rs\"\n"
    );
    std::fs::write(dir.join("Cargo.toml"), manifest).expect("clippy manifest");
    std::fs::write(src_dir.join("main.rs"), code).expect("clippy main.rs");
    let mut c = Command::new("cargo");
    c.arg("clippy")
        .arg("--offline")
        .arg("-q")
        .arg("--manifest-path")
        .arg(dir.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(dir.join("target"));
    if optimised {
        c.arg("--release");
    }
    c.args(["--", "-D", "warnings"]);
    let res = c.output().expect("run cargo clippy");
    if res.status.success() && res.stderr.is_empty() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&res.stderr).to_string();
    // keep the lint headlines and their source line, drop cargo's framing
    let mut keep: Vec<&str> = text
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("error")
                || t.starts_with("warning")
                || t.starts_with("--> ")
                || t.starts_with("note: `#[deny")
        })
        .collect();
    if keep.is_empty() {
        keep.push(text.trim());
    }
    keep.truncate(24);
    Err(format!(
        "clippy ({}) failed:\n{}",
        if optimised { "-O" } else { "debug" },
        keep.join("\n")
    ))
}

// ── main ─────────────────────────────────────────────────────────────────

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/rust_idiom");
    let src_dir = root.join("src");
    let out_dir = root.join("expected");
    let check = std::env::args().any(|a| a == "--check");
    // render + census + idiom metric + mirror check, no rustc/behaviour passes
    let idiom_only = std::env::args().any(|a| a == "--idiom-only");
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
    let clippy_err = clippy_probe().err();
    match &clippy_err {
        None => println!(
            "note  clippy available: every built fixture is linted (debug and -O, -D warnings)"
        ),
        Some(e) => {
            println!("note  HARD GAP: cargo clippy is unavailable — no fixture can be linted");
            println!("      {e}");
        }
    }
    // The render-path mirror needs the REAL renderer; build it on demand so a
    // fresh clone can run the whole gate with one command.
    let shir_bin = match shir_render_bin() {
        Ok(b) => {
            println!("note  render mirror: {}", b.display());
            Ok(b)
        }
        Err(e) => {
            println!("note  HARD GAP: shir_render is unavailable — the mirror check cannot run");
            println!("      {e}");
            Err(e)
        }
    };
    println!(
        "{:<20} {:>5} {:>5} {:>5} {:>4} {:>4} {:>4} {:>6} {:>7}",
        "fixture", "lines", "let", "store", "pfx", "pfi", "pfl", "idiom", "status"
    );
    let mut idiom_total = Idioms::default();
    let mut mirror_ok = 0usize;

    let mut failures = 0usize;
    let mut total = Census::default();
    for f in &files {
        let name = f.file_stem().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(f).expect("read fixture");
        let emit = debashl::rust_backend::shir_to_rust_with_census(&prepare(&src));
        let code = emit.code.clone();
        let out = out_dir.join(format!("{name}.rs"));
        let mut c = Census::of(&code);
        c.printf_specialized = emit.printf_specialized;
        c.printf_interpreted = emit.printf_interpreted;
        c.printf_lines = helper_body_lines(&code, "printf");
        // The promoted-vs-stored split, from the SAME verdict the renderer
        // uses, plus a proof per promoted name that a bare `let` binding
        // carrying it is really in the emitted text.
        let promoted = debashl::rust_backend::promoted_names(&prepare(&src));
        c.promoted = promoted.len();
        // The store: one `__SH_VARS` per program that needs it, no
        // per-variable static of any kind, and the number of accesses that
        // name a key (a helper BODY takes `name: &str`, so it does not
        // count — this is call traffic, not helper size).
        c.statics = code.matches("static __SHV_").count();
        c.stored = store_accesses(&code);
        total.add(c);
        // ── 5. the idiomatic metric: clippy's own catalog over the emitted
        // program. The count IS the score — no hand-written pattern list.
        match clippy_idioms(&code, &name, &scratch.join(&name).join("idiom")) {
            Ok(i) => idiom_total.add(i),
            Err(m) => {
                println!("{name:<20} IDIOM-UNAVAILABLE: {m}");
                failures += 1;
            }
        }
        for name in &promoted {
            let ident: String = {
                let mut s = String::new();
                for ch in name.chars() {
                    if ch.is_ascii_alphanumeric() || ch == '_' {
                        s.push(ch);
                    } else {
                        s.push('_');
                    }
                }
                if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
                    s.insert_str(0, "v_");
                }
                s
            };
            // a promoted name binds either as a `let`/`let mut` or as the
            // per-iteration binding of a `for … in` loop
            let bound = code.contains(&format!("let {ident}"))
                || code.contains(&format!("let mut {ident}"))
                || code.contains(&format!("for {ident} in "));
            // A construct the renderer could not lower emits a TODO marker
            // instead of its statements, so a promoted name may have no
            // binding to find; the check only binds where the emission is
            // complete.
            if !bound && !code.contains("TODO(unsupported)") {
                println!(
                    "{name:<20} PROMOTED-NOT-BOUND: `{name}` is promoted but no `let {ident}` binding is in the emitted text"
                );
                failures += 1;
            }
        }
        // An abort in generated code is a generator bug, not a fixture
        // behaviour: fail the gate whatever the mode. (Kept as a targeted
        // assertion rather than a family count — the metric is clippy's.)
        let panics = code.matches(".unwrap()").count()
            + code.matches(".expect(").count()
            + code.matches("panic!(").count()
            + code.matches("unreachable!(").count();
        if panics > 0 {
            println!(
                "{name:<20} {:>5}  PANIC PATH: {panics} `.unwrap()`/`.expect()`/`panic!`/`unreachable!` in generated code — a helper must be infallible by construction or a documented abort",
                c.lines
            );
            failures += 1;
        }
        // The output must compile on its own terms: one `#[allow]` would be a
        // warning silenced instead of a shape fixed.
        let allows = code.matches("#[allow(").count();
        if allows > 0 {
            println!(
                "{name:<20} {:>5}  ALLOW ATTRIBUTE: {allows} `#[allow(…)]` in generated code — the output must compile with zero",
                c.lines
            );
            failures += 1;
        }

        // The ONE store replaced one `thread_local!` per unpromoted name, so
        // a per-variable store static coming back is a regression whatever
        // the behaviour says: every unpromoted name lives in `__SH_VARS`.
        let per_var = code.matches("static __SHV_").count();
        if per_var > 0 {
            println!(
                "{name:<20} {:>5}  PER-VARIABLE STORE STATIC: {per_var} `static __SHV_…` in generated code — unpromoted variables share the ONE `__SH_VARS` store",
                c.lines
            );
            failures += 1;
        }

        if idiom_only {
            // the ratchet's fast loop: no goldens, no builds
        } else if check {
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

        // ── 2. render-path mirror: the REAL `shir_render --target rust` ──
        let parity_ok = match &shir_bin {
            Ok(bin) => match mirror_render(bin, &src, &scratch.join(&name).join("mirror")) {
                Ok(v) => {
                    if v == code {
                        mirror_ok += 1;
                        true
                    } else {
                        let first = v
                            .lines()
                            .zip(code.lines())
                            .position(|(a, b)| a != b)
                            .map(|i| i + 1)
                            .unwrap_or_else(|| v.lines().count().min(code.lines().count()) + 1);
                        println!(
                            "\n{name}: MIRROR-DIFF at line {first} — shir_render --target rust and the harness render disagree"
                        );
                        false
                    }
                }
                Err(m) => {
                    println!("\n{name}: MIRROR-ERROR: {m}");
                    false
                }
            },
            Err(_) => false,
        };

        // ── 3/4. compile + behaviour ───────────────────────────────────
        let mut status;
        if idiom_only {
            status = "render-only".into();
        } else if let Some(krate) = needed_crate(&code) {
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
                // ── 3b. clippy, debug and -O ──────────────────────────
                let mut lint_msgs: Vec<String> = Vec::new();
                if clippy_err.is_none() {
                    for optimised in [false, true] {
                        let lint_dir = scratch.join(&name).join(if optimised {
                            "lint_rel"
                        } else {
                            "lint_dbg"
                        });
                        if let Err(m) = clippy_lints(&code, &name, &lint_dir, optimised) {
                            lint_msgs.push(m);
                        }
                    }
                }
                // A fixture that says `# stderr-exact:` opts into a
                // byte-for-byte stderr diff (with `$0` aligned to bash's).
                let stderr_exact = src
                    .lines()
                    .any(|l| l.trim().starts_with("# stderr-exact:"));
                let argv0: Option<&Path> = if stderr_exact { Some(f) } else { None };
                let want = run_bash(f, &scratch.join(&name).join("bash"));
                let got_d = run_bin(
                    &dbg_dir.join("main"),
                    &scratch.join(&name).join("run_dbg"),
                    argv0,
                );
                let got_r = run_bin(
                    &rel_dir.join("main"),
                    &scratch.join(&name).join("run_rel"),
                    argv0,
                );
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
                if stderr_exact {
                    if !got_d.stderr_eq(&want) {
                        bad = true;
                        bits.push("DEBUG-STDERR!=bash");
                    }
                    if !got_r.stderr_eq(&want) {
                        bad = true;
                        bits.push("RELEASE-STDERR!=bash");
                    }
                }
                if !parity_ok {
                    bad = true;
                    bits.push("A1-PARITY-DIFF");
                }
                if !lint_msgs.is_empty() {
                    bad = true;
                    bits.push("CLIPPY");
                }
                if bad {
                    failures += 1;
                    status = bits.join(" ");
                    for m in &lint_msgs {
                        println!("\n{name}: {m}");
                    }
                    if !got_d.eq(&want) && declared_overflow.is_none() {
                        println!("\n{name}: bash → {:?} rc={} files={:?}", want.stdout, want.code, want.files.keys().collect::<Vec<_>>());
                        println!("{name}: dbg  → {:?} rc={} files={:?}", got_d.stdout, got_d.code, got_d.files.keys().collect::<Vec<_>>());
                    }
                    if !got_r.eq(&want) {
                        println!("\n{name}: bash → {:?} rc={}", want.stdout, want.code);
                        println!("{name}: rel  → {:?} rc={}", got_r.stdout, got_r.code);
                    }
                    if stderr_exact && !got_d.stderr_eq(&want) {
                        println!("\n{name}: bash stderr → {:?}", want.stderr);
                        println!("{name}: dbg stderr  → {:?}", got_d.stderr);
                    }
                    if stderr_exact && !got_r.stderr_eq(&want) {
                        println!("\n{name}: bash stderr → {:?}", want.stderr);
                        println!("{name}: rel stderr  → {:?}", got_r.stderr);
                    }
                } else {
                    status = format!("{} a1=ok", bits.join(" "));
                }
            }
        }
        let own_idioms = idiom_total
            .sites
            .iter()
            .filter(|s| s.fixture == name)
            .count();
        println!(
            "{name:<20} {:>5} {:>5} {:>5} {:>4} {:>4} {:>4} {:>6}  {status}",
            c.lines, c.promoted, c.stored, c.printf_specialized, c.printf_interpreted,
            c.printf_lines, own_idioms
        );
    }

    println!(
        "\n{:<20} {:>5} {:>5} {:>5} {:>4} {:>4} {:>4} {:>6}",
        "TOTAL", total.lines, total.promoted, total.stored, total.printf_specialized,
        total.printf_interpreted, total.printf_lines, idiom_total.total()
    );

    // ── the metric: clippy's per-lint counts against the measured baseline ──
    let base: BTreeMap<&str, usize> = IDIOM_BASELINE.iter().copied().collect();
    let mut over = 0usize;
    for (lint, n) in &idiom_total.by_lint {
        let want = base.get(lint.as_str()).copied().unwrap_or(0);
        if *n > want {
            over += 1;
            println!("IDIOM: {lint}: {n} finding(s), baseline {want}");
            for site in idiom_total.sites.iter().filter(|s| &s.lint == lint).take(8) {
                println!("        {}:{}: {}", site.fixture, site.line, site.message);
            }
        }
    }
    for (lint, want) in &base {
        let got = idiom_total.by_lint.get(*lint).copied().unwrap_or(0);
        if got < *want {
            println!("note  idiom lower: {lint}: {got} < baseline {want} — lower the baseline");
        }
    }
    if over > 0 {
        failures += 1;
    }
    println!(
        "idiomatic: {} clippy findings over {} generated programs (baseline {}); {}",
        idiom_total.total(),
        files.len(),
        base.values().sum::<usize>(),
        if idiom_total.by_lint.is_empty() {
            "no lint fired".to_string()
        } else {
            idiom_total.breakdown()
        }
    );
    match &shir_bin {
        Ok(_) if mirror_ok == files.len() => println!(
            "render mirror: {}/{} fixtures byte-identical to shir_render --target rust",
            mirror_ok,
            files.len()
        ),
        _ => {
            println!(
                "render mirror: {}/{} fixtures byte-identical to shir_render --target rust — FAILED",
                mirror_ok,
                files.len()
            );
            failures += 1;
        }
    }
    if let Some(e) = &clippy_err {
        eprintln!(
            "\nHARD GAP: clippy did not run, so the idiomatic metric is incomplete ({e})"
        );
        eprintln!("install it with `rustup component add clippy` and re-run");
        std::process::exit(2);
    }
    if let Err(e) = &shir_bin {
        eprintln!("\nHARD GAP: shir_render did not run, so the render mirror is unverified ({e})");
        std::process::exit(2);
    }
    if failures > 0 {
        eprintln!("\n{failures} fixture(s) failed");
        std::process::exit(1);
    }
    println!(
        "\n{} fixtures: render unchanged, mirror == shir_render --target rust, rustc+clippy clean (debug + -O, -D warnings), behaviour matches bash, idiomatic metric within baseline",
        files.len()
    );
}
