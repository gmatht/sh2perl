//! shir_render — the generic shIR-JSON → target renderer (the backend
//! worktree's copy; this one renders Perl).
//!
//! The generic binary every other worktree ships dispatches to ~15 backend
//! modules, including `lint_backend`. This worktree is on an OLDER
//! `shir_passes` than that, so the shared file does not compile here:
//!
//!     E0432  unresolved import `crate::shir_passes::used_before_decl`
//!     E0609  no field `use_before_decl` on `PassContext`
//!     E0609  no field `function_sccs` on `PassContext`
//!
//! Those are CORE analyses the perl worktree has not caught up with, and
//! backporting them is a core change, not a backend one. The gate only ever
//! runs this bin for `--target=js` or `--target=perl` (setup_backends.sh
//! `--backend-gate`), and a corpus-wide run needs Perl. So this copy keeps
//! the shared binary's *contract* — same flags, same stdin/file ingress,
//! same "ShIR JSON ingress" marker the gate probes for, same transform
//! channel — and drops only the targets this worktree cannot serve.
//!
//! When the worktree's `shir_passes` catches up, delete this file: cargo's
//! autobins will pick up the shared one and it is the better default.

use std::io::Read;

fn usage() -> ! {
    eprintln!(
        "usage: shir_render [--target L] [input|-]\n\
         \n\
         input   A1 shIR JSON file, or `-` for stdin\n\
         target  perl (default perl)"
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
        target = input
            .as_deref()
            .and_then(|p| std::path::Path::new(p).extension())
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "perl".into());
    }
    let kind = match target.as_str() {
        "pl" | "perl" => "perl",
        // estree/js are accepted and routed to the ESTree printer so the
        // `--target=js` path in --backend-gate still resolves; a js
        // translation is not this worktree's job and the gate's own
        // renderer check is what decides the verdict.
        "js" | "estree" | "json" => "estree",
        "shir" => "shir",
        other => {
            eprintln!(
                "shir_render: unknown target '{other}' (this worktree serves: \
                 perl, estree, shir)"
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
    // SH2_TRANSFORMS inside apply) — the same channel otranspilerl runs.
    debashl::transforms::apply(&mut prog.stmts);
    // the estree target renders goto/cfor natively — skip the structuring
    // passes (otranspilerl ingest parity; restructuring changed gate output
    // for goto/cfor-bearing examples).
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
        _ => debashl::perl_backend::shir_to_perl(&prog),
    };
    print!("{out}");
}
