//! otranspiler — the unified user-facing transpiler command (parity with
//! the Go frontends/busybox contract, backed by THIS repo's shell parser +
//! renderer fleet):
//!
//!   otranspiler <input> [<output>] [--source-lang L] [--target L]
//!
//!   input   file.sh / file.bash / … (shell is the default source language)
//!           file.shir (pre-made A1 contract — skips parse stage)
//!   output  {-,file}.{pl,c,js,go,rs,zig,java,py,sh}   (- = stdout;
//!           extension picks the target backend, default sh)
//!
//! Implementation note: stages reuse THIS SAME BINARY via current_exe()
//! (`--shir … --raw` for parse, `--shir-in-<lang> -` for render), exactly
//! like cli/src/lib.rs's own spawn helper — one tested code path serves
//! both the CLI and this composed entry point.

use std::io::Write;
use std::process::Command;

fn usage() -> ! {
    eprintln!(
        "usage: otranspiler <input> [<output>] [--source-lang L] [--target L]\n\
         \n\
         input   .sh/.bash/… (shell; default) | .shir (A1 contract)\n\
         output  extension selects the backend: pl c js go rs zig java py sh\n\
         -       stdout (also the default when <output> is omitted)"
    );
    std::process::exit(2);
}

fn ext_of(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // superset CLI: any debashc-level flag (--shir, --shir-in-*, gates'
    // flags, …) delegates straight to the full command-line processor
    if let Some(a) = args.first() {
        if a.starts_with('-')
            && !matches!(a.as_str(), "--source-lang" | "--target" | "-h" | "--help")
        {
            debashcl::main_with_args(std::env::args().collect());
            return;
        }
    }
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut source_lang: Option<String> = None;
    let mut target: Option<String> = None;
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--source-lang" => source_lang = it.next().cloned(),
            "--target" => target = it.next().cloned(),
            "-h" | "--help" => usage(),
            _ if input.is_none() => input = Some(a.clone()),
            _ if output.is_none() => output = Some(a.clone()),
            _ => usage(),
        }
    }
    let input = match input {
        Some(i) => i,
        None => usage(),
    };

    // ── source language ────────────────────────────────────────────────
    let src_ext = ext_of(&input);
    let lang = source_lang.unwrap_or_else(|| match src_ext.as_str() {
        "shir" | "json" => "shir".into(),
        "sh" | "bash" | "zsh" | "ksh" | "dash" => "sh".into(),
        other => other.to_string(),
    });
    if !matches!(lang.as_str(), "sh" | "bash" | "zsh" | "ksh" | "dash") && lang != "shir" {
        eprintln!(
            "otranspiler: source language '{lang}' needs its frontend \
             (the Go fleet: frontends/{lang}-*); this binary parses SHELL. \
             Feed a .shir A1 contract directly, or use the Go busybox."
        );
        std::process::exit(3);
    }

    // ── stage 1: shell → A1 shIR JSON (self-spawn, machine-fresh) ──────
    // stages run through the sibling debashc (full CLI); spawning THIS
    // binary would re-enter the composed command above
    let exe = std::env::current_exe()
        .expect("current_exe")
        .parent()
        .expect("exe dir")
        .join("debashc");
    assert!(
        exe.exists(),
        "otranspiler: sibling debashc not found next to {:?} — build both: \
         cargo build -p debashcl --bin otranspiler --bin debashc",
        std::env::current_exe().unwrap_or_default()
    );
    let a1 = if lang == "shir" {
        std::fs::read_to_string(&input).unwrap_or_else(|e| {
            eprintln!("otranspiler: read {}: {e}", input);
            std::process::exit(1);
        })
    } else {
        let out = Command::new(&exe)
            .args(["--shir", &input, "--raw"])
            .output()
            .unwrap_or_else(|e| panic!("spawn {:?}: {e}", exe));
        if !out.status.success() {
            eprintln!(
                "otranspiler: parse failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            std::process::exit(1);
        }
        assert!(!out.stdout.is_empty(), "empty A1 — unparseable input?");
        String::from_utf8_lossy(&out.stdout).to_string()
    };

    // ── target language: --target > output extension > sh ─────────────
    let out_path = output.clone().unwrap_or_else(|| "-".into());
    let tgt = target.unwrap_or_else(|| match ext_of(&out_path).as_str() {
        "" => "sh".into(),
        e => e.to_string(),
    });
    // normalize to BACKEND-FLAG names (--shir-in-<tgt>)
    let tgt = match tgt.as_str() {
        "pl" | "perl" => "perl",
        "py" | "python" => "python",
        "rs" | "rust" => "rust",
        "javascript" | "js" => "js",
        other => other,
    };
    const BACKENDS: &[&str] = &[
        "perl", "c", "js", "go", "rs", "rust", "zig", "java", "python", "sh",
        "estree", "glsl",
    ];
    if !BACKENDS.contains(&tgt) {
        eprintln!("otranspiler: unknown target '{tgt}' (known: {})", BACKENDS.join(" "));
        std::process::exit(3);
    }
    if tgt == "estree" {
        eprintln!("otranspiler: estree emits JSON via --shir-in-estree; not wired here yet");
        std::process::exit(3);
    }

    // ── stage 2: A1 → target render (self-spawn) ──────────────────────
    let mut child = Command::new(&exe)
        .arg(format!("--shir-in-{tgt}"))
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("spawn render stage");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(a1.as_bytes())
        .expect("write A1 to render stage");
    let out = child.wait_with_output().expect("render stage");
    if !out.status.success() {
        std::process::exit(out.status.code().unwrap_or(1));
    }
    let rendered = out.stdout;

    // ── output: '-' → stdout, else file ────────────────────────────────
    if out_path == "-" || output.is_none() {
        std::io::stdout()
            .write_all(&rendered)
            .expect("write stdout");
    } else {
        std::fs::write(&out_path, &rendered).unwrap_or_else(|e| {
            eprintln!("otranspiler: write {}: {e}", out_path);
            std::process::exit(1);
        });
    }
}
