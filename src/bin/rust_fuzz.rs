//! Differential fuzz net for the Rust backend: bash vs the emitted Rust.
//!
//! ```text
//! cargo run --bin rust_fuzz                       # documented default, seed 1
//! cargo run --bin rust_fuzz -- --cases 300 --seed 7
//! cargo run --bin rust_fuzz -- --script case.sh   # replay one program
//! cargo run --bin rust_fuzz -- --cases 50 --seed 7 --keep
//! ```
//!
//! ## What it proves
//!
//! Most of the Rust backend's idiom work is done by TEXT passes that rewrite
//! the rendered source (`compound_assign`, `drop_single_use_clones`,
//! `drop_dead_literal_lets`, the `__sh_cat` flattening, the literal-`printf`
//! specialization, `drop_esc_arg`, the helper pruner…), and several of them
//! decide a rewrite by CLASSIFYING or COUNTING identifiers in a string. A
//! pass like that can only be trusted if it is shown not to change MEANING on
//! shapes nobody hand-wrote, which is exactly what the twenty checked-in
//! fixtures cannot do.
//!
//! Each generated program is run twice on the SAME bytes: under `bash` and
//! under the freshly rendered + compiled Rust. stdout, stderr and the exit
//! status must all match. The program is built in BOTH profiles — plain
//! `rustc` (debug assertions + overflow checks on) and `rustc -O` — so a
//! resolved-to-plain-operator arithmetic path that wraps in bash but panics
//! in a debug build is a divergence, not a crash to ignore. A generated-code
//! panic is a divergence by construction: it writes to stderr and exits 101.
//!
//! The generator is deterministic (a seeded LCG). The seed is a first-class
//! input and every failing case is left on disk with the exact command that
//! replays it, so a failure is one command away from reproducing.
//!
//! ## Summary line
//!
//! `rust_fuzz: seed=<seed> profile=debug+release programs=<n> matched=<n>
//! diverged=<n> render_fail=<n> rustc_fail=<n>` — then the process exits 1 if
//! anything diverged. A quiet drop in coverage (say the generator starts
//! emitting only trivial programs) shows up as a change in `programs=` at the
//! documented seed.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Seeded LCG (Knuth's constants). No dependency, fully reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        let i = self.below(xs.len() as u64) as usize;
        &xs[i]
    }
    fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in) == 0
    }
}

/// The prelude every program starts with: two integers, two strings, and a
/// string with a space in it (so quoted vs unquoted expansion and word
/// splitting are reachable at every statement).
const PRELUDE: &str = "a=0\nb=\"seed\"\nc=3\nd=\"w\"\nsp=\"x y\"\n";

/// A `printf` call site: the generated shell text plus the classes it covers.
/// Literal formats with 0/1/N `%s` slots, `%%`, escapes, `\c`, braces, `%d`,
/// `%c`, both arity mismatches, a slotless format that still passes an
/// argument, and formats only known at run time — so the specialized and the
/// fallback path are both exercised constantly.
fn printf_stmt(rng: &mut Rng) -> String {
    let (fmt, args): (&str, &[&str]) = match rng.below(16) {
        0 => ("%s\\n", &["$b"]),
        1 => ("%s %s\\n", &["$b", "$d"]),
        2 => ("[%s]", &["$a"]),
        3 => ("%s", &["$d"]),
        4 => ("100%%\\n", &[]),
        5 => ("{a}%s\\n", &["$b"]),
        6 => ("x\\ty\\n", &[]),
        7 => ("a\\\\b\\n", &[]),
        8 => ("stop\\c after\\n", &["$b"]),
        9 => ("%d\\n", &["$a"]),
        10 => ("%c", &["abc"]),
        11 => ("%s %s\\n", &["$b"]),
        12 => ("%s\\n", &["$b", "$d"]),
        13 => ("plain\\n", &["$b"]),
        14 => ("%s\\n%s\\n", &["$a", "$b"]),
        _ => ("{}%s{}\\n", &["$sp"]),
    };
    if args.is_empty() {
        return format!("printf '{fmt}'\n");
    }
    let mut out = format!("printf '{fmt}'");
    for a in args {
        out.push(' ');
        out.push_str(a);
    }
    out.push('\n');
    out
}

/// One random program: the typed prelude plus 3..8 statements.
fn generate(rng: &mut Rng) -> String {
    let mut s = String::from(PRELUDE);
    let n = 3 + rng.below(6);
    for _ in 0..n {
        match rng.below(34) {
            0 => s.push_str(&format!("a=$((a + {}))\n", 1 + rng.below(5))),
            1 => s.push_str("c=$((c - 1))\n"),
            2 => s.push_str("echo \"b=$b\"\n"),
            3 => s.push_str("echo \"$a\" \"$b\"\n"),
            // quoted vs unquoted expansion of a value with a space
            4 => s.push_str(if rng.chance(2) {
                "echo $sp\n"
            } else {
                "echo \"$sp\"\n"
            }),
            5 => s.push_str(&printf_stmt(rng)),
            // a single-use string variable (drop_single_use_clones fodder)
            6 => s.push_str("tmp=\"$b-$a\"\necho \"tmp=$tmp\"\n"),
            // repeated literals (the literal folding passes)
            7 => s.push_str("echo same\necho same\n"),
            8 => s.push_str("out=$(printf '%s!' \"$b\")\necho \"out=$out\"\n"),
            // command substitution + word splitting in a loop
            9 => s.push_str("for w in $(echo \"$sp\"); do echo \"w=$w\"; done\n"),
            10 => s.push_str(&format!(
                "if [ \"$a\" -gt {} ]; then echo big; else echo small; fi\n",
                rng.below(6)
            )),
            11 => s.push_str("if [ \"$b\" = seed ]; then echo eq; else echo ne; fi\n"),
            12 => s.push_str("for i in 1 2 3; do a=$((a + i)); done\n"),
            13 => {
                s.push_str("while [ \"$c\" -gt 0 ]; do c=$((c - 1)); a=$((a + 2)); done\n")
            }
            14 => {
                // define the helper ONCE (a second definition is E0428)
                if !s.contains("f() {") {
                    s.push_str("f() { local t=\"$1\"; echo \"t=$t\"; }\n");
                }
                s.push_str("f \"$b\"\n");
            }
            15 => s.push_str("case \"$b\" in seed) echo s;; *) echo o;; esac\n"),
            16 => s.push_str("b=\"$b-x\"\n"),
            // a run-time-built format: the fallback path
            17 => {
                s.push_str("fmt=\"%s/%s\\n\"\nprintf \"$fmt\" \"$b\" \"$d\"\n");
            }
            // redirect a print and read it back through a capture
            18 => s.push_str("printf '%s\\n' \"$a\" > r.txt\necho \"r=$(cat r.txt)\"\n"),
            // a pipeline whose right side is a builtin shell loop
            19 => s.push_str("printf '%s\\n' \"$b\" | while read -r l; do echo \"l:$l\"; done\n"),
            // an exit status used by an `if` (a failing command must not abort)
            20 => s.push_str("if printf '%s' \"$b\"; then echo st-ok; fi\n"),
            // `$?` after a status-setting command
            21 => s.push_str("printf '%s' \"$b\"\necho \"rc=$?\"\n"),
            // `false || …` — a failing command followed by an alternative
            22 => s.push_str("false || echo alt\n"),
            // unquoted expansion in a for list (word splitting, no capture)
            23 => s.push_str("for w in $sp; do echo \"w=$w\"; done\n"),
            // `-n`, and an unquoted value fed to printf (the format repeats)
            24 => s.push_str(if rng.chance(2) {
                "echo -n ab\n"
            } else {
                "printf '[%s]' $sp\n"
            }),
            // brace syntax and a length expansion
            25 => s.push_str("echo \"len=${#b}\"\necho \"b=${b}\"\n"),
            // arithmetic that is not just an addition
            26 => s.push_str(&format!(
                "a=$((a * {}))\nc=$((c / {}))\n",
                1 + rng.below(3),
                1 + rng.below(2)
            )),
            // an append redirect, read back through a capture
            27 => s.push_str("printf '%s\\n' \"$b\" >> r.txt\necho \"a=$(cat r.txt)\"\n"),
            // a capture glued to literal text in ONE word — unquoted prefix,
            // unquoted suffix, several fragments, a quoted form, and a
            // capture next to another capture. All of them must keep every
            // part (`x=$(cmd)z` is `hiz`, not `hi`).
            28 => {
                let w = match rng.below(5) {
                    0 => "x=$(printf %s \"$b\")z",
                    1 => "x=z$(printf %s \"$b\")",
                    2 => "x=$(printf %s \"$b\")-$(printf %s \"$d\")",
                    3 => "x=\"$(printf %s \"$b\")z\"",
                    _ => "x=$(printf %s \"$(printf %s hi)\")",
                };
                s.push_str(&format!("{w}\necho \"x=$x\"\n"));
            }
            // two positional parameters in a function
            29 => {
                if !s.contains("g() {") {
                    s.push_str("g() { local u=\"$2\"; echo \"u=$u\"; }\n");
                }
                s.push_str("g \"$b\" \"$d\"\n");
            }
            // NATIVE pipelines: an all-external multi-stage chain, a
            // `builtin | external…` chain, a chain feeding `$()`, and one
            // feeding a word. The emitted program spawns the stages itself and
            // wires them with real pipes — a `bash -c` here would show up as a
            // divergence the moment the status or the bytes disagree.
            31 => {
                s.push_str("printf '%s\\n' \"$b\" \"$d\" > pa.txt\n");
                s.push_str("printf '%s\\n' zz \"$b\" > pb.txt\n");
                match rng.below(4) {
                    0 => s.push_str("cat pa.txt pb.txt | tr a-z A-Z | wc -l\n"),
                    1 => s.push_str("printf '%s\\n' \"$b\" \"$d\" | sort | wc -l\n"),
                    2 => s.push_str("n=$(cat pa.txt pb.txt | wc -l)\necho \"n=$n\"\n"),
                    _ => s.push_str("echo \"s:$(cat pa.txt pb.txt | sort | head -1)\"\n"),
                }
            }
            // shell output INTERLEAVED with a child's inherited fd 1: the
            // `printf` has no trailing newline, so Rust's line buffer must be
            // flushed before the child writes or the bytes come out in the
            // wrong order
            32 => {
                s.push_str("printf '%s\\n' \"$b\" \"$d\" > pi.txt\n");
                s.push_str("printf 'pre-'");
                if rng.below(2) == 0 {
                    s.push_str("\ncat pi.txt\n");
                } else {
                    s.push_str("\ncat pi.txt | tr a-z A-Z\n");
                }
                s.push_str("printf 'post'\n");
                s.push_str("echo -interleave\n");
            }
            // a pipeline stage carrying a redirection cannot be spelled as an
            // argv: it keeps the documented `bash -c` fallback and must still
            // agree with bash byte for byte
            33 => {
                s.push_str("printf '%s\\n' \"$b\" > pf.txt\n");
                s.push_str("grep -h . pf.txt > pj.txt | wc -c\n");
                s.push_str("wc -l < pj.txt\n");
            }
            // an arithmetic word inside a printed expansion
            _ => s.push_str("echo \"n=$((c + 1))\"\n"),
        }
    }
    s.push_str("echo \"end $a $b $c\"\n");
    s
}

fn render(src: &str) -> Result<String, String> {
    let cmds = debashl::parser::commands::parse_commands_from_text(src)
        .map_err(|e| format!("parse: {e}"))?;
    let mut prog = debashl::shir::ast_to_ir(&cmds);
    debashl::transforms::apply(&mut prog.stmts);
    debashl::shir_passes::restructure_goto_only(&mut prog);
    debashl::shir_passes::strip_cfor(&mut prog);
    Ok(debashl::rust_backend::shir_to_rust(&prog))
}

/// Run `prog` in `dir` (with an optional single argument); return
/// (stdout, stderr, exit code).
fn run(dir: &Path, prog: &Path, arg: Option<&Path>) -> (String, String, i32) {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).ok();
    let mut cmd = Command::new(prog);
    if let Some(a) = arg {
        cmd.arg(a);
    }
    let out = cmd.current_dir(dir).output().expect("spawn");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

/// A divergence: which stage failed and what the two sides said.
enum Outcome {
    Matched,
    RenderFail(String),
    RustcFail(String),
    Diverged {
        profile: &'static str,
        bash: (String, String, i32),
        rust: (String, String, i32),
    },
}

fn check_one(src: &str, dir: &Path, profiles: &[&'static str]) -> Outcome {
    std::fs::create_dir_all(dir).ok();
    let sh = dir.join("case.sh");
    std::fs::write(&sh, src).expect("write script");
    let code = match render(src) {
        Ok(c) => c,
        Err(e) => return Outcome::RenderFail(e),
    };
    let rs = dir.join("main.rs");
    std::fs::write(&rs, &code).expect("write main.rs");
    // the bash side once: same bytes, its own scratch directory
    let bash_dir = dir.join("run_bash");
    let (b_out, b_err, b_rc) = run(&bash_dir, Path::new("bash"), Some(&sh));
    for profile in profiles {
        let bin = dir.join(format!("main_{profile}"));
        let mut cmd = Command::new("rustc");
        cmd.args(["--edition", "2021", "-D", "warnings"]);
        if *profile == "release" {
            cmd.arg("-O");
        }
        let built = cmd.arg("-o").arg(&bin).arg(&rs).output().expect("rustc");
        if !built.status.success() {
            return Outcome::RustcFail(String::from_utf8_lossy(&built.stderr).to_string());
        }
        let (r_out, r_err, r_rc) = run(&dir.join(format!("run_rust_{profile}")), &bin, None);
        let bash = (b_out.clone(), b_err.clone(), b_rc);
        let rust = (r_out, r_err, r_rc);
        if bash != rust {
            return Outcome::Diverged {
                profile,
                bash,
                rust,
            };
        }
    }
    Outcome::Matched
}

fn show(case: &str, src: &str, outcome: &Outcome) {
    match outcome {
        Outcome::Matched => {}
        Outcome::RenderFail(e) => {
            println!("{case} RENDER-FAIL {e}\n--- script ---\n{src}");
        }
        Outcome::RustcFail(e) => {
            println!("{case} RUSTC-FAIL\n{e}\n--- script ---\n{src}");
        }
        Outcome::Diverged {
            profile,
            bash,
            rust,
        } => {
            println!(
                "{case} DIVERGED ({profile})  bash rc={} rust rc={}",
                bash.2, rust.2
            );
            println!("--- script ---\n{src}");
            if !bash.0.is_empty() || !rust.0.is_empty() {
                println!("--- bash stdout ---\n{}", bash.0);
                println!("--- rust stdout ---\n{}", rust.0);
            }
            if !bash.1.is_empty() || !rust.1.is_empty() {
                println!("--- bash stderr ---\n{}", bash.1);
                println!("--- rust stderr ---\n{}", rust.1);
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cases = 60u64;
    let mut seed = 1u64;
    let mut keep = false;
    let mut script: Option<PathBuf> = None;
    let mut profiles: Vec<&'static str> = vec!["debug", "release"];
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--cases" => {
                cases = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(60);
                i += 2;
            }
            "--seed" => {
                seed = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(1);
                i += 2;
            }
            "--script" => {
                script = args.get(i + 1).map(PathBuf::from);
                if script.is_none() {
                    eprintln!("rust_fuzz: --script needs a path");
                    std::process::exit(2);
                }
                i += 2;
            }
            "--profile" => match args.get(i + 1).map(String::as_str) {
                Some("debug") => {
                    profiles = vec!["debug"];
                    i += 2;
                }
                Some("release") => {
                    profiles = vec!["release"];
                    i += 2;
                }
                Some("both") => {
                    profiles = vec!["debug", "release"];
                    i += 2;
                }
                _ => {
                    eprintln!("rust_fuzz: --profile wants debug|release|both");
                    std::process::exit(2);
                }
            },
            "--keep" => {
                keep = true;
                i += 1;
            }
            other => {
                eprintln!("rust_fuzz: unknown argument '{other}'");
                std::process::exit(2);
            }
        }
    }

    let scratch = std::env::temp_dir().join(format!("rust_fuzz_{seed}"));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("scratch");
    let profile_label = profiles.join("+");

    // replay one program: the minimized-reproducer path
    if let Some(path) = script {
        let src = std::fs::read_to_string(&path).expect("read script");
        let dir = scratch.join("replay");
        let outcome = check_one(&src, &dir, &profiles);
        show(path.display().to_string().as_str(), &src, &outcome);
        let diverged = !matches!(outcome, Outcome::Matched);
        println!(
            "rust_fuzz: seed={seed} profile={profile_label} programs=1 matched={} diverged={}",
            if diverged { 0 } else { 1 },
            if diverged { 1 } else { 0 }
        );
        if diverged {
            std::process::exit(1);
        }
        return;
    }

    let mut rng = Rng(seed);
    let (mut matched, mut diverged, mut render_fail, mut rustc_fail) = (0u64, 0u64, 0u64, 0u64);

    for case in 0..cases {
        let src = generate(&mut rng);
        let dir = scratch.join(format!("case{case:04}"));
        let outcome = check_one(&src, &dir, &profiles);
        match outcome {
            Outcome::Matched => matched += 1,
            Outcome::RenderFail(_) => render_fail += 1,
            Outcome::RustcFail(_) => rustc_fail += 1,
            Outcome::Diverged { .. } => diverged += 1,
        }
        if !matches!(outcome, Outcome::Matched) {
            let tag = format!("case {case:04}");
            show(&tag, &src, &outcome);
            println!(
                "  reproducer: cargo run --bin rust_fuzz -- --script {} --seed {seed}\n",
                dir.join("case.sh").display()
            );
        } else if !keep {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    println!(
        "rust_fuzz: seed={seed} profile={profile_label} programs={cases} matched={matched} \
         diverged={diverged} render_fail={render_fail} rustc_fail={rustc_fail}"
    );
    if diverged + render_fail + rustc_fail > 0 {
        std::process::exit(1);
    }
}
