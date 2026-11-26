//! Differential fuzz net for the Rust backend.
//!
//! ```text
//! cargo run --bin rust_fuzz -- --cases 200
//! cargo run --bin rust_fuzz -- --cases 50 --seed 7 --keep
//! ```
//!
//! Renders seeded random shell programs from the backend's supported subset,
//! compiles each with `rustc --edition 2021 -O -D warnings`, runs both the
//! generated program and `bash` on the SAME script in identical scratch
//! directories, and compares stdout and the exit status.
//!
//! ## Why this exists
//!
//! Most of the Rust backend's idiom work is done by TEXT passes that rewrite
//! the rendered source (`compound_assign`, `drop_single_use_clones`,
//! `drop_dead_literal_lets`, the `__sh_cat` flattening, the literal-`printf`
//! specialization, the helper pruner…). A text pass can only be trusted if it
//! is shown not to change MEANING on shapes nobody hand-wrote, which is
//! exactly what the ten-odd checked-in fixtures cannot do. Every pass in the
//! pipeline runs here, on scripts the pass author never saw.
//!
//! The generator is deterministic (a seeded LCG) and covers the subset that
//! lowers natively: assignments, integer and string arithmetic, `echo`,
//! `printf` with a literal format, tests, `if`/`for`/`while`/`case`, capture,
//! and a function with a `local`. No external commands, no `$RANDOM`, no
//! timestamps — a case that is not reproducible could not be compared.
//!
//! `--keep` leaves each case's directory in the temp dir for inspection.

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
}

/// One random program: a typed prelude plus 3..8 statements.
fn generate(rng: &mut Rng) -> (String, Vec<String>) {
    // integers: a, c, i (loop); strings: b, d
    let mut s = String::from("a=0\nb=\"seed\"\nc=3\nd=\"w\"\n");
    let mut want: Vec<String> = Vec::new();
    let n = 3 + rng.below(6);
    for _ in 0..n {
        match rng.below(13) {
            0 => s.push_str(&format!("a=$((a + {}))\n", 1 + rng.below(5))),
            1 => s.push_str("c=$((c - 1))\n"),
            2 => s.push_str("echo \"b=$b\"\n"),
            3 => s.push_str("echo \"$a\"\n"),
            4 => {
                // literal formats: the specializable shapes (text, `%%`,
                // `%s`, a literal brace) and ones that must FALL BACK to
                // the interpreter (`%d` with a numeric argument)
                let f = rng.pick(&[
                    "%s\\n",
                    "%s %s\\n",
                    "[%s]\\n",
                    "%s",
                    "100%%\\n",
                    "{a}%s\\n",
                    "%d\\n",
                ]);
                let a = if f.starts_with("%d") {
                    "$a"
                } else {
                    rng.pick(&["$b", "$d", "$a"])
                };
                if f.matches("%s").count() == 2 {
                    s.push_str(&format!("printf '{f}' {a} \"$a\"\n"));
                } else {
                    s.push_str(&format!("printf '{f}' {a}\n"));
                }
            }
            12 => {
                // a format assembled at run time: the fallback path
                s.push_str("fmt=\"%s/%s\\n\"\n");
                s.push_str("printf \"$fmt\" \"$b\" \"$d\"\n");
            }
            5 => s.push_str(&format!(
                "if [ \"$a\" -gt {} ]; then echo big; else echo small; fi\n",
                rng.below(6)
            )),
            6 => s.push_str("for i in 1 2 3; do a=$((a + i)); done\n"),
            7 => s.push_str("while [ \"$c\" -gt 0 ]; do c=$((c - 1)); a=$((a + 2)); done\n"),
            8 => {
                // define the helper ONCE (a second definition is E0428)
                if !s.contains("f() {") {
                    s.push_str("f() { local t=\"$1\"; echo \"t=$t\"; }\n");
                }
                s.push_str("f \"$b\"\n");
            }
            9 => s.push_str("out=$(echo \"$b\"); echo \"out=$out\"\n"),
            10 => s.push_str("case \"$b\" in seed) echo s;; *) echo o;; esac\n"),
            _ => s.push_str("b=\"$b-x\"\n"),
        }
        want.push(String::new());
    }
    s.push_str("echo \"end $a $b $c\"\n");
    (s, want)
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

fn run(dir: &Path, prog: &Path, arg: &Path) -> (String, String, i32) {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).ok();
    let out = Command::new(prog)
        .arg(arg)
        .current_dir(dir)
        .output()
        .expect("spawn");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cases = 100u64;
    let mut seed = 1u64;
    let mut keep = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--cases" => {
                cases = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(100);
                i += 2;
            }
            "--seed" => {
                seed = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(1);
                i += 2;
            }
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
    let mut rng = Rng(seed);
    let (mut ok, mut bad) = (0usize, 0usize);
    let skipped = 0usize;

    for case in 0..cases {
        let (src, _) = generate(&mut rng);
        let dir = scratch.join(format!("case{case:04}"));
        std::fs::create_dir_all(&dir).ok();
        let sh = dir.join("case.sh");
        std::fs::write(&sh, &src).expect("write script");
        let code = match render(&src) {
            Ok(c) => c,
            Err(e) => {
                println!("case {case:04} RENDER-FAIL {e}");
                bad += 1;
                continue;
            }
        };
        let rs = dir.join("main.rs");
        std::fs::write(&rs, &code).expect("write main.rs");
        let bin = dir.join("main");
        let built = Command::new("rustc")
            .args(["--edition", "2021", "-O", "-D", "warnings", "-o"])
            .arg(&bin)
            .arg(&rs)
            .output()
            .expect("rustc");
        if !built.status.success() {
            println!("case {case:04} RUSTC-FAIL");
            print!("{}", String::from_utf8_lossy(&built.stderr));
            println!("--- script ---\n{src}");
            bad += 1;
            continue;
        }
        let (b_out, _, b_rc) = run(&dir.join("run_bash"), Path::new("bash"), &sh);
        let (r_out, r_err, r_rc) = run(&dir.join("run_rust"), &bin, &sh);
        if b_out == r_out && b_rc == r_rc {
            ok += 1;
        } else {
            bad += 1;
            println!("case {case:04} MISMATCH  bash rc={b_rc} rust rc={r_rc}");
            println!("--- script ---\n{src}");
            println!("--- bash ---\n{b_out}");
            println!("--- rust ---\n{r_out}");
            if !r_err.is_empty() {
                println!("--- rust stderr ---\n{r_err}");
            }
        }
        if !keep {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    println!(
        "\n{ok} matched, {bad} mismatched, {skipped} skipped — {} case(s), seed {seed}",
        cases
    );
    if bad > 0 {
        std::process::exit(1);
    }
    let _ = PathBuf::from(&scratch);
}
