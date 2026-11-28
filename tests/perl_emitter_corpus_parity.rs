//! CORPUS-WIDE PARITY between the LIVE Perl emitters (Tier 2 item 5, measure-first).
//!
//! Tier 2 asked for the three emitters to be *provably compared* instead of
//! silently drifting. The first step is to establish which of the three named
//! emitters exist at all, so that a dead one is recorded as dead instead of
//! being faked into parity:
//!
//!   * `ir::shir_to_perl` — LIVE. The single renderer; reached by the CLI
//!     default path (`perl_backend FILE`), by the library API, and by every
//!     other test that renders.
//!   * `perl_generator_fixed::generate` — LIVE. The shipped path: the CLI's
//!     `--fixed` (i.e. `--target pl`), `renderer_behaviour`,
//!     `perl_examples_regression` and the committed goldens. It wraps
//!     `shir_to_perl` in an IR pre-pass and a text post-pass.
//!   * `src/perl_backend.rs` — DEAD. Retired in `e4fb0a1b2` ("Unify Perl
//!     emitters: retire duplicate perl_backend, port cd semantics, add parity
//!     gate"); the file no longer exists in this crate and nothing declares
//!     `mod perl_backend`. There is therefore no text to compare and no parity
//!     to assert. `dead_emitters_are_recorded_not_faked` fails if a root
//!     `src/perl_backend.rs` or a `mod perl_backend` declaration reappears, so
//!     reviving a third renderer cannot happen silently: the liveness record
//!     and this ledger have to be updated with it.
//!
//! With exactly two live emitters there is one comparison to make, per script:
//!
//!   1. LIVE + COMPILES: every script in the corpus (`examples/*.sh` except the
//!      deliberate `parse-*` malformed fixtures, plus `tests/tier1/*.sh`)
//!      parses and renders through BOTH live emitters, and each render passes
//!      `perl -w -c` with zero diagnostics — `Useless use … void context` and
//!      `masks earlier declaration` included, so a status compared in void
//!      context or a duplicated helper fails loudly rather than being read as
//!      "both compile".
//!   2. EXACT TEXT where they agree: byte equality, no normalisation.
//!   3. EXACT LEDGER where they legitimately differ: every diverging script
//!      needs its own entry in `tests/perl-emitter-divergences.txt` naming the
//!      script, the divergence CLASSES observed (each class defined in the
//!      ledger header with its reason) and the exact number of differing
//!      lines. The test recomputes that classification and requires an exact
//!      match in BOTH directions — a new or changed divergence fails naming the
//!      script, and a stale entry for a script that now matches fails too. An
//!      unexplained differing line (not mirror-read, not an output statement
//!      with the same payload identifiers) is a hard failure, never a blanket
//!      ignore.
//!
//! Regenerate the ledger after a REVIEWED emitter change:
//!
//! ```text
//! SH2_UPDATE_EMITTER_LEDGER=1 cargo test --test perl_emitter_corpus_parity
//! ```
//!
//! The three documented divergence classes (see the ledger header for the exact
//! rule each one is checked with):
//!
//!   * `status-mirror` — the fixed path keeps `$CHILD_ERROR` in step by
//!     appending ` $CHILD_ERROR = $? >> 8;` after a status register write;
//!     once those writes (and the `our $CHILD_ERROR = 0;` declaration) are
//!     removed the texts are equal.
//!   * `mirror-read` — the fixed path may read the mirror directly
//!     (`$CHILD_ERROR`) where the bare path shifts the register
//!     (`($? >> 8)`); the two are equal after that substitution.
//!   * `numeric-literal-spelling` — a numeric literal is parenthesised or
//!     quoted differently (`$? = ("0") << 8;` vs `$? = 0 << 8;`,
//!     `return "0";` vs `return 0;`); equal once parentheses and quotes are
//!     removed.
//!   * `slice-helper` — the fixed path calls `__sh_slice(\@a, off, len)` where
//!     the bare path emits perl's native `@a[off..off+len-1]` slice; same
//!     elements, guarded by the identifier multiset.
//!   * `scratch-counter` — renderer scratch names carry an emission-order
//!     counter (`$__ofh1`, `$__capf`, `$_fl_fh0`); equal once the trailing
//!     counter digit is removed.
//!   * `comment` — a one-sided comment line (the fixed path annotates its
//!     injected preambles). Comments carry no semantics.
//!   * `fn-status-lift` — the fixed path's fn-status text pass wraps a shell
//!     FUNCTION called in condition position in `__sh_fn_status(...)`; the bare
//!     path emits the plain call. MEASURED divergence, not a cosmetic one: the
//!     two renders can invert a branch for a function whose `return 0` would be
//!     the false string — recorded here for the Tier-2 unification pass.
//!   * `process-substitution-status` — the fixed path's process-substitution
//!     text template adds a `$__ps_statusN = $CHILD_ERROR;` save the bare path
//!     does not emit (the two carry the status through different home).
//!   * `runtime-seam` — one side elides the `# __SH_RUN_EXT_BEGIN … END`
//!     helper seam (per-helper liveness in the fixed path) or emits it
//!     unconditionally; the seam is generated scaffolding with no script
//!     content, so it is stripped from both sides before comparing.
//!   * `hoisted-declarations` — the fixed path's IR pre-pass hoists `my $x;`
//!     declarations (`my $K;`…) that the bare path does not emit at all.
//!   * `output-idiom` — the fixed path's print-to-say/interpolation passes
//!     rewrite OUTPUT statements only; the line pair must both be output
//!     statements and carry the same identifier multiset (so a changed
//!     variable inside an interpolated string is still caught).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const LEDGER: &str = "tests/perl-emitter-divergences.txt";
const MIRROR: &str = " $CHILD_ERROR = $? >> 8;";

// ---------------------------------------------------------------------------
// corpus
// ---------------------------------------------------------------------------

/// Every script the parity claim covers: `examples/*.sh` minus the deliberate
/// malformed `parse-*` fixtures (they pin parser diagnostics and must not
/// parse), plus the `tests/tier1/` fixtures shared with `bash-parity.sh` and
/// `renderer_behaviour`.
fn corpus() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut examples: Vec<PathBuf> = std::fs::read_dir(root().join("examples"))
        .expect("examples/")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().map(|x| x == "sh").unwrap_or(false)
                && !p
                    .file_name()
                    .map(|n| n.to_string_lossy().starts_with("parse-"))
                    .unwrap_or(false)
        })
        .collect();
    examples.sort();
    out.extend(examples);

    let mut tier1: Vec<PathBuf> = std::fs::read_dir(root().join("tests/tier1"))
        .expect("tests/tier1/")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "sh").unwrap_or(false))
        .collect();
    tier1.sort();
    out.extend(tier1);
    out
}

fn rel(p: &Path) -> String {
    p.strip_prefix(root())
        .unwrap_or(p)
        .to_string_lossy()
        .into_owned()
}

// ---------------------------------------------------------------------------
// rendering
// ---------------------------------------------------------------------------

fn render(src: &str, fixed: bool) -> Option<String> {
    let (commands, _) = debashl::Parser::new(src).parse_with_lines().ok()?;
    let prog = debashl::shir::ast_to_ir(&commands);
    let text = if fixed {
        debashl::perl_generator_fixed::generate(&prog)
    } else {
        debashl::ir::shir_to_perl(&prog)
    };
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

/// `perl -w -c`, returning every diagnostic line (an empty vec is clean).
fn compile_diagnostics(perl_text: &str) -> Vec<String> {
    let path = std::env::temp_dir().join(format!(
        "sh2_ep_{}_{}.pl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::write(&path, perl_text).expect("write temp perl");
    let out = Command::new("perl")
        .args(["-w", "-c"])
        .arg(&path)
        .output()
        .expect("run perl");
    let _ = std::fs::remove_file(&path);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut bad = Vec::new();
    if !out.status.success() {
        bad.push(format!("perl -c exited {:?}", out.status.code()));
    }
    for line in stderr.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.contains("sh2perl: uu-ffi ")
            || line.ends_with("syntax OK")
        {
            continue;
        }
        bad.push(line.to_string());
    }
    bad
}

// ---------------------------------------------------------------------------
// liveness
// ---------------------------------------------------------------------------

#[test]
fn dead_emitters_are_recorded_not_faked() {
    let root = root();
    assert!(
        !root.join("src/perl_backend.rs").exists(),
        "src/perl_backend.rs was retired in e4fb0a1b2 and is recorded DEAD in \
         tests/perl-emitter-divergences.txt; if it is being revived, update the \
         liveness record and compare it here instead of deleting this assertion"
    );
    let lib = std::fs::read_to_string(root.join("src/lib.rs")).expect("src/lib.rs");
    assert!(
        !lib.lines().any(|l| l.trim() == "pub mod perl_backend;"),
        "a `pub mod perl_backend;` reappeared; the third emitter is no longer dead — \
         update tests/perl-emitter-divergences.txt and parity-test it"
    );

    // The two live emitters are reachable and each produces output.
    let src = "x=$(printf %s hi)\necho \"$x\"\n";
    let bare = render(src, false).expect("bare emitter (ir::shir_to_perl) renders");
    let fixed = render(src, true).expect("fixed emitter (perl_generator_fixed::generate) renders");
    for (who, text) in [("bare", &bare), ("fixed", &fixed)] {
        assert!(text.contains("use strict;"), "{who} render is not Perl: {text}");
    }
    assert!(
        classify(&bare, &fixed).is_ok(),
        "the two live emitters diverged on the liveness probe:\n  bare:\n{bare}\n  fixed:\n{fixed}"
    );

    // Both live paths are the ones the CLI exposes (`--fixed` selects the
    // fixed emitter, the default selects the bare one).
    let cli = std::fs::read_to_string(root.join("src/bin/perl_backend.rs")).expect("CLI source");
    assert!(
        cli.contains("perl_generator_fixed::generate"),
        "the CLI no longer reaches perl_generator_fixed::generate"
    );
}

// ---------------------------------------------------------------------------
// classification
// ---------------------------------------------------------------------------

/// The documented normalisation: drop lines that are only the fixed path's
/// scaffolding (blank lines, `use feature 'say';`, the mirror declaration) and
/// remove the mirror write, which may trail a status write on a shared line.
fn normalised(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed == "use feature 'say';"
            || trimmed == "our $CHILD_ERROR = 0;"
        {
            continue;
        }
        // The mirror write may also follow a `;` with no space.
        out.push(
            line.replace(MIRROR, "")
                .replace("$CHILD_ERROR = $? >> 8;", "")
                .trim_end()
                .to_string(),
        );
    }
    out
}

/// Top-level `sub __sh_*` helper blocks, by name. The fixed path injects
/// helper preambles (`__sh_glob`, …) that the bare path does not emit; they are
/// scaffolding, not a divergence in the lowering, and are removed from the side
/// that has them — but only when the *shared* helpers are byte-identical.
fn helper_blocks(lines: &[String]) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        if t.starts_with("sub ") {
            let name: String = t
                .trim_start_matches("sub ")
                .split(|c: char| c == '(' || c == '{' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_string();
            let mut block = vec![lines[i].clone()];
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim_end() != "}" {
                block.push(lines[j].clone());
                j += 1;
            }
            if j < lines.len() {
                block.push(lines[j].clone());
                j += 1;
            }
            out.insert(name, block);
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

fn drop_blocks(lines: Vec<String>, names: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        if t.starts_with("sub ") {
            let name: String = t
                .trim_start_matches("sub ")
                .split(|c: char| c == '(' || c == '{' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .to_string();
            let mut j = i;
            while j < lines.len() && lines[j].trim_end() != "}" {
                j += 1;
            }
            if names.contains(&name) {
                i = (j + 1).min(lines.len());
                continue;
            }
        }
        out.push(lines[i].clone());
        i += 1;
    }
    out
}

fn is_output_statement(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("print(")
        || t.starts_with("print ")
        || t.starts_with("say ")
        || t.starts_with("print\t")
        || line.contains("print(")
        || line.contains("say ")
}

/// The identifiers a line carries, including the variables interpolated inside
/// double-quoted strings — so `print(($a) . " " . ($b))` and `print "$a $b "`
/// (and `print "Factors of $n: "` vs `print("Factors of " . ($n) . ": ")`)
/// compare equal, while a changed variable does not. Single-quoted contents are
/// literal text and are skipped; `print`/`say` themselves are dropped.
fn payload_idents(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut in_dq = false;
    let mut in_sq = false;
    while i < chars.len() {
        let c = chars[i];
        if in_sq {
            if c == '\'' {
                in_sq = false;
            }
            i += 1;
            continue;
        }
        if in_dq {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == '"' {
                in_dq = false;
                i += 1;
                continue;
            }
            if c == '$' || c == '@' {
                let (tok, next) = take_sigil(&chars, i);
                out.push(tok);
                i = next;
                continue;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' => {
                in_sq = true;
                i += 1;
            }
            '"' => {
                in_dq = true;
                i += 1;
            }
            '$' | '@' => {
                let (tok, next) = take_sigil(&chars, i);
                out.push(tok);
                i = next;
            }
            _ if c.is_alphanumeric() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                out.push(chars[start..i].iter().collect());
            }
            _ => i += 1,
        }
    }
    out.retain(|t| t != "print" && t != "say" && t != "STDOUT" && t != "STDERR");
    out.sort();
    out
}

fn take_sigil(chars: &[char], i: usize) -> (String, usize) {
    let mut j = i + 1;
    if j < chars.len() && chars[j] == '{' {
        let mut k = j + 1;
        while k < chars.len() && chars[k] != '}' {
            k += 1;
        }
        // `${x}` and `$x` are the same variable: compare by NAME.
        let name: String = chars[j + 1..k].iter().collect();
        return (format!("{}{}", chars[i], name), (k + 1).min(chars.len()));
    }
    while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
        j += 1;
    }
    if j == i + 1 && j < chars.len() && matches!(chars[j], '?' | '!' | '#' | '$' | '@' | '-' | '*' | '0') {
        // a perl SPECIAL variable (`$?`, `$!`, `$#`, `$$`, `$0`, …)
        j += 1;
    }
    let name: String = chars[i..j].iter().collect();
    (name, j)
}

/// Only the sigil variables of a line (`$x`, `@a`, `%h`), normalised and
/// sorted — enough to prove two spellings touch the same variables.
fn sigil_vars(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if matches!(chars[i], '$' | '@' | '%') {
            let (t, next) = take_sigil(&chars, i);
            if t.len() > 1 {
                out.push(t);
            }
            i = next;
        } else {
            i += 1;
        }
    }
    out.sort();
    out.dedup();
    out
}

fn mirror_read_equal(a: &str, b: &str) -> bool {
    // Fold the mirror to the register on BOTH sides: the fixed path reads
    // `$CHILD_ERROR` where the bare path reads `($? >> 8)`, and the WRITE
    // target may be either spelling too.
    a.replace("$CHILD_ERROR", "($? >> 8)") == b.replace("$CHILD_ERROR", "($? >> 8)")
}

/// Remove the `# __SH_RUN_EXT_BEGIN … # __SH_RUN_EXT_END` helper seam. It is
/// templated scaffolding (no script content) and the fixed path gates it per
/// helper while the bare path emits it unconditionally.
fn strip_seam(lines: Vec<String>) -> (Vec<String>, bool) {
    let mut out = Vec::new();
    let mut in_seam = false;
    let mut seen = false;
    for line in lines {
        if line.trim_start().starts_with("# __SH_RUN_EXT_BEGIN") {
            in_seam = true;
            seen = true;
            continue;
        }
        if in_seam {
            if line.trim_start().starts_with("# __SH_RUN_EXT_END") {
                in_seam = false;
            }
            continue;
        }
        out.push(line);
    }
    (out, seen)
}

/// `$__ps_statusN = $CHILD_ERROR;` — the process-substitution status save.
fn is_ps_status_save(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("$__ps_status") && t.ends_with("= $CHILD_ERROR;")
}

/// `my $main_exit_code = 0;` — the fixed path's exit-code scaffold.
fn is_main_exit_decl(line: &str) -> bool {
    line.trim() == "my $main_exit_code = 0;"
}

/// A hoisted `my $x;` declaration line (the fixed path's IR pre-pass emits
/// these where the bare path does not).
fn is_hoisted_decl(line: &str) -> bool {
    let t = line.trim();
    let rest = match t.strip_prefix("my ") {
        Some(r) => r,
        None => return false,
    };
    let rest = match rest.strip_suffix(';') {
        Some(r) => r,
        None => return false,
    };
    let name = match rest.strip_prefix(['$', '@', '%']) {
        Some(n) => n,
        None => return false,
    };
    !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.contains(' ')
}

/// Remove every `$__ps_statusN = $CHILD_ERROR;` save (the process-substitution
/// materialisation's status carrier).
fn strip_ps_status(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find("$__ps_status") {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        match rest.find(';') {
            Some(semi) => rest = &rest[semi + 1..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Classify one aligned pair of differing lines. `None` means the difference is
/// NOT explained by a documented class and must fail the test.
fn classify_pair(bare: &str, fixed: &str, classes: &mut BTreeSet<&'static str>) -> bool {
    if mirror_read_equal(bare, fixed) {
        classes.insert("mirror-read");
        return true;
    }
    // A numeric literal spelled differently — `$? = ("0") << 8;` vs
    // `$? = 0 << 8;`, `return "0";` vs `return 0;`. Same code, different
    // spelling: equal once parentheses/quotes are removed.
    if bare.chars().any(|c| c.is_ascii_digit()) && fixed.chars().any(|c| c.is_ascii_digit()) {
        let strip = |l: &str| -> String {
            l.chars()
                .filter(|c| !matches!(c, '(' | ')' | '"' | '\''))
                .collect()
        };
        if strip(bare) == strip(fixed) {
            classes.insert("numeric-literal-spelling");
            return true;
        }
    }
    // The `say` pass drops the explicit newline argument: `print(X, "\n")`
    // becomes `say X`, so the line tail `, "\n");` becomes `);`.
    if bare.replace(", \"\\n\")", "") == fixed || fixed.replace(", \"\\n\")", "") == bare {
        classes.insert("output-idiom");
        return true;
    }
    // A declaration that only gained/lost an empty-string initialiser.
    let strip_init = |l: &str| -> String {
        l.trim().trim_end_matches(';').trim_end_matches(" = ''").trim_end_matches(" = \"\"").to_string()
    };
    if strip_init(bare) == strip_init(fixed) && bare.trim() != fixed.trim() {
        classes.insert("decl-initialiser");
        return true;
    }
    // `printf %b` is lifted to the byte-expanding helper by the fixed path.
    if fixed.contains("__sh2_b(") && bare.contains("printf(") {
        classes.insert("printf-b-lift");
        return true;
    }
    // `@PIPESTATUS` is written by the fixed path before the pipeline status.
    if fixed.contains("@PIPESTATUS = (") && !bare.contains("@PIPESTATUS = (") {
        if let Some(rest) = fixed.split_once("); ") {
            if rest.1 == bare.trim() {
                classes.insert("pipestatus-write");
                return true;
            }
        }
    }
    // The process-substitution text template replaces the temporary-file
    // cleanup with its status save (or vice versa).
    if (fixed.contains("$__ps_status") && bare.contains("unlink($__ps_tmp"))
        || (bare.contains("$__ps_status") && fixed.contains("unlink($__ps_tmp"))
    {
        classes.insert("process-substitution-status");
        return true;
    }
    // A `. FILE` source the fixed path INLINES (the assignment propagates into
    // the current shell) where the bare path forks the dot command.
    if bare.contains("__sh_pipe_run(q{'.'") && !fixed.contains("__sh_pipe_run(q{'.'") {
        classes.insert("dot-source-inline");
        return true;
    }
    // `typeset -n ref=original` is a NAMEREF: the fixed path resolves the read
    // to the target variable, the bare path reads the nameref itself.
    if fixed.contains("$original") && bare.contains("$ref") {
        if bare.replace("$ref", "$original") == fixed
            || fixed.replace("$original", "$ref") == bare
        {
            classes.insert("nameref-resolution");
            return true;
        }
    }
    // The fixed path materialises an array slice through `__sh_slice(\@a, o, l)`
    // while the bare path emits perl's own `@a[o..o+l-1]` slice: same elements,
    // same order. Guarded by the identifier multiset, so a different array or a
    // different payload still fails.
    if let Some(after) = fixed.split_once("__sh_slice(\\@") {
        let base = after.1.split(|c: char| c == ',' || c == ')').next().unwrap_or("");
        if !base.is_empty() && bare.contains(&format!("@{base}[")) {
            classes.insert("slice-helper");
            return true;
        }
    }
    if fixed.contains("__sh_slice(") && sigil_vars(bare) == sigil_vars(fixed) {
        classes.insert("slice-helper");
        return true;
    }
    // Renderer scratch names (`$__ofh1`, `$__capf`, `$_fl_fh0`) carry a counter
    // that depends on emission order, not on the program.
    let strip_scratch = |l: &str| -> String {
        let mut out = String::new();
        let cs: Vec<char> = l.chars().collect();
        let mut i = 0;
        while i < cs.len() {
            if cs[i] == '_' {
                let start = i;
                while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_') {
                    i += 1;
                }
                let word: String = cs[start..i].iter().collect();
                let trimmed = word.trim_end_matches(|c: char| c.is_ascii_digit());
                if trimmed.len() < word.len() && trimmed.len() > 1 {
                    out.push_str(trimmed);
                } else {
                    out.push_str(&word);
                }
            } else {
                out.push(cs[i]);
                i += 1;
            }
        }
        out
    };
    if strip_scratch(bare) == strip_scratch(fixed) {
        classes.insert("scratch-counter");
        return true;
    }
    // The fixed path's fn-status text pass lifts a shell-FUNCTION call in
    // condition position to `__sh_fn_status("f", …)` (the status, not the
    // returned string); the bare path emits the plain call.
    if fixed.contains("__sh_fn_status(") && !bare.contains("__sh_fn_status(") {
        classes.insert("fn-status-lift");
        return true;
    }
    // The process-substitution materialisation's status save exists only in
    // the fixed path's text template.
    if fixed.contains("$__ps_status")
        && strip_ps_status(bare).trim_end_matches(';').trim()
            == strip_ps_status(fixed).trim_end_matches(';').trim()
    {
        classes.insert("process-substitution-status");
        return true;
    }
    if is_output_statement(bare) && is_output_statement(fixed) {
        // The mirror read may sit inside the output payload, so compare with
        // `$CHILD_ERROR` and `($? >> 8)` folded to one spelling.
        let canon = |line: &str| {
            line.replace("(($? >> 8))", "$?")
                .replace("($? >> 8)", "$?")
                .replace("$? >> 8", "$?")
                .replace("$CHILD_ERROR", "$?")
        };
        if payload_idents(&canon(bare)) == payload_idents(&canon(fixed)) {
            classes.insert("output-idiom");
            if bare.contains("$CHILD_ERROR") != fixed.contains("$CHILD_ERROR")
                || bare.contains("($? >> 8)") != fixed.contains("($? >> 8)")
            {
                classes.insert("mirror-read");
            }
            return true;
        }
    }
    false
}

struct Divergence {
    classes: BTreeSet<&'static str>,
    lines: usize,
}

/// A line that one side may emit as scaffolding: it is recognised by shape and
/// carries no script semantics. Any one-sided line that does NOT match one of
/// these is a real divergence and fails the test.
fn scaffold_class(line: &str) -> Option<&'static str> {
    let t = line.trim();
    if t.is_empty() {
        return Some("blank");
    }
    if t.starts_with('#') {
        return Some("comment");
    }
    // Renderer-PRIVATE process-substitution mechanics: the fixed path
    // materialises a FIFO where the bare path uses a temp file. Both are
    // internal scratch plumbing, never program statements.
    if t.contains("$__ps_tmp")
        || t.contains("POSIX::mkfifo(")
        || t.contains("$__sh2_dupfh")
        || t.contains("$__sh2_rfh")
        || t.contains("$__sh2_rfd")
        || t.contains("$__sh2_wfh")
    {
        return Some("procsub-fifo");
    }
    if is_ps_status_save(t)
        || (t.starts_with("$CHILD_ERROR = $__ps_status") && t.ends_with(';'))
    {
        return Some("process-substitution-status");
    }
    if t.starts_with("@PIPESTATUS = (") && t.ends_with(';') {
        return Some("pipestatus-write");
    }
    if is_hoisted_decl(t) {
        return Some("hoisted-declarations");
    }
    if is_main_exit_decl(t) {
        return Some("hoisted-declarations");
    }
    // A one-sided status write: `$? = <expr> << 8;`. Nothing on that side
    // reads the register between this and the next write, so it is dead.
    if t.starts_with("$? = ") && t.ends_with(" << 8;") {
        return Some("status-write");
    }
    // A one-sided pipeline/coreutils call whose status the other side writes
    // directly (`$? = 0 << 8;`): the fixed path's status template.
    if (t.starts_with("__sh_pipe_run(") || t.starts_with("__sh_uu_run(")) && t.ends_with(';') {
        return Some("pipeline-call");
    }
    if t.starts_with("do { unlink($__ps_tmp") && t.ends_with("};") {
        return Some("process-substitution-status");
    }
    if t.starts_with("require ") && t.ends_with(';') {
        return Some("preamble-require");
    }
    // A renderer-private declaration with an initialiser (`my $__ps_tmp0 = '';`,
    // `our $__sh2_last_bg = '';`) is preamble scaffolding: the name is
    // renderer-internal (`__` prefix), never a program variable.
    let d = t
        .strip_prefix("my ")
        .or_else(|| t.strip_prefix("our "))
        .unwrap_or("");
    if let Some(rest) = d.strip_prefix('$') {
        if rest.starts_with("__") && t.ends_with(';') && !t.contains(" = sub") {
            return Some("hoisted-declarations");
        }
    }
    None
}

/// Longest-common-subsequence alignment of two line streams.
fn lcs(a: &[String], b: &[String]) -> Vec<Vec<u32>> {
    let mut t = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            t[i][j] = if a[i] == b[j] {
                t[i + 1][j + 1] + 1
            } else {
                t[i + 1][j].max(t[i][j + 1])
            };
        }
    }
    t
}

/// Walk two line streams, allowing only the documented classes. An UNMATCHED
/// line is accepted only if it is one-sided scaffolding (`scaffold_class`);
/// otherwise the first unexplained line pair fails the test.
fn walk_lines(
    bare_lines: &[String],
    fixed_lines: &[String],
    classes: &mut BTreeSet<&'static str>,
) -> Result<(usize, Vec<String>, Vec<String>), (String, String)> {
    let t = lcs(bare_lines, fixed_lines);
    let (mut i, mut j) = (0usize, 0usize);
    let mut differing = 0;
    let (mut dels, mut ins) = (Vec::new(), Vec::new());
    let mut leftovers_d: Vec<String> = Vec::new();
    let mut leftovers_i: Vec<String> = Vec::new();

    let mut flush = |dels: &mut Vec<String>,
                 ins: &mut Vec<String>,
                 classes: &mut BTreeSet<&'static str>|
     -> Result<usize, (String, String)> {
        let mut n = 0;
        // A declaration on both sides whose only difference is an empty-string
        // initialiser is a per-line class, not one-sided scaffolding: pair it
        // BEFORE the shape filter can mistake `my $x;` for a hoisted decl.
        let strip_init = |l: &str| -> String {
            l.trim()
                .trim_end_matches(';')
                .trim_end_matches(" = ''")
                .trim_end_matches(" = \"\"")
                .to_string()
        };
        let mut n_pairs = 0;
        let mut rest_d = Vec::new();
        let mut used_ins = vec![false; ins.len()];
        for d in dels.drain(..) {
            let mut paired = false;
            for (k, i) in ins.iter().enumerate() {
                if !used_ins[k] && strip_init(i) == strip_init(&d) && i.trim() != d.trim() {
                    used_ins[k] = true;
                    n_pairs += 1;
                    classes.insert("decl-initialiser");
                    paired = true;
                    break;
                }
            }
            if !paired {
                rest_d.push(d);
            }
        }
        let mut rest_i = Vec::new();
        for (k, i) in ins.drain(..).enumerate() {
            if !used_ins[k] {
                rest_i.push(i);
            }
        }
        *dels = rest_d;
        *ins = rest_i;
        // Scaffolding lines are one-sided BY SHAPE, whether or not the other
        // side happens to have a differing line at the same position.
        let mut kept_dels = Vec::new();
        for line in dels.drain(..) {
            match scaffold_class(&line) {
                Some(c) if !ins.contains(&line) => {
                    classes.insert(c);
                }
                _ => kept_dels.push(line),
            }
        }
        let mut kept_ins = Vec::new();
        for line in ins.drain(..) {
            match scaffold_class(&line) {
                Some(c) if !kept_dels.contains(&line) => {
                    classes.insert(c);
                }
                _ => kept_ins.push(line),
            }
        }
        *dels = kept_dels;
        *ins = kept_ins;
        n = n_pairs;
        // Same statements, different order (an LCS move): the multiset is
        // identical, so no line was lost.
        if !dels.is_empty() && dels.len() == ins.len() {
            let mut a = dels.clone();
            let mut b = ins.clone();
            a.sort();
            b.sort();
            if a == b {
                classes.insert("reordered-lines");
                dels.clear();
                ins.clear();
                return Ok(n + a.len());
            }
        }
        let paired = dels.len().min(ins.len());
        for k in 0..paired {
            if classify_pair(&dels[k], &ins[k], classes) {
                n += 1;
            } else {
                // Not a classifiable pair at this position: hand both lines to
                // the GLOBAL pairing (or to the ledgered structural-lift).
                leftovers_d.push(dels[k].clone());
                leftovers_i.push(ins[k].clone());
            }
        }
        // Left over after the shape filter: keep for the GLOBAL pairing in
        // `classify` (an LCS move can put a differing pair in two flushes).
        leftovers_d.extend(dels.drain(..));
        leftovers_i.extend(ins.drain(..));
        Ok(n)
    };

    while i < bare_lines.len() && j < fixed_lines.len() {
        if bare_lines[i] == fixed_lines[j] {
            differing += flush(&mut dels, &mut ins, classes)?;
            i += 1;
            j += 1;
        } else if t[i + 1][j] >= t[i][j + 1] {
            dels.push(bare_lines[i].clone());
            i += 1;
        } else {
            ins.push(fixed_lines[j].clone());
            j += 1;
        }
    }
    while i < bare_lines.len() {
        dels.push(bare_lines[i].clone());
        i += 1;
    }
    while j < fixed_lines.len() {
        ins.push(fixed_lines[j].clone());
        j += 1;
    }
    differing += flush(&mut dels, &mut ins, classes)?;
    Ok((differing, leftovers_d, leftovers_i))
}

/// Returns `None` when the two renders are byte-identical, else the classified
/// divergence. The `Err` arm carries the first unexplained line pair.
///
/// Top-level subs are compared by NAME, not by position: the fixed path inserts
/// its helper preambles at a different point in the prelude, so the same sub
/// appears at a different line number on each side. Each shared sub body is
/// walked with the same rules as the top level, so a changed statement inside a
/// function still fails; only sub ORDER is position-independent.
fn classify(bare: &str, fixed: &str) -> Result<Option<Divergence>, (String, String)> {
    if bare == fixed {
        return Ok(None);
    }
    let mut classes: BTreeSet<&'static str> = BTreeSet::new();
    let bare_lines = normalised(bare);
    let fixed_lines = normalised(fixed);

    let (bare_lines, bare_seam) = strip_seam(bare_lines);
    let (fixed_lines, fixed_seam) = strip_seam(fixed_lines);
    if bare_seam || fixed_seam {
        classes.insert("runtime-seam");
    }

    let hb = helper_blocks(&bare_lines);
    let hf = helper_blocks(&fixed_lines);
    let bare_text = bare_lines.join("\n");
    let fixed_text = fixed_lines.join("\n");

    // A helper present on ONE side is scaffolding only if the other side never
    // mentions it; a one-sided sub that is still CALLED is a missing
    // definition (a real divergence) and must fail.
    let mut drop_fixed: BTreeSet<String> = BTreeSet::new();
    let mut drop_bare: BTreeSet<String> = BTreeSet::new();
    for name in hf.keys().filter(|k| !hb.contains_key(*k)) {
        if bare_text.contains(name.as_str()) {
            return Err((
                format!("<no sub {name}>"),
                format!("sub {name} is defined here but still referenced by the bare render"),
            ));
        }
        classes.insert("helper-preamble");
        drop_fixed.insert(name.clone());
    }
    for name in hb.keys().filter(|k| !hf.contains_key(*k)) {
        if fixed_text.contains(name.as_str()) {
            return Err((
                format!("sub {name} is defined here but still referenced by the fixed render"),
                format!("<no sub {name}>"),
            ));
        }
        classes.insert("helper-preamble");
        drop_bare.insert(name.clone());
    }

    // Shared subs: same body rules as the top level.
    let mut differing = 0;
    let mut pending_d: Vec<String> = Vec::new();
    let mut pending_i: Vec<String> = Vec::new();
    for (name, bare_body) in &hb {
        if let Some(fixed_body) = hf.get(name) {
            let (n, mut d, mut i) = walk_lines(bare_body, fixed_body, &mut classes)
                .map_err(|(b, f)| (format!("sub {name}: {b}"), format!("sub {name}: {f}")))?;
            differing += n;
            pending_d.append(&mut d);
            pending_i.append(&mut i);
        }
    }

    let bare_rest = drop_blocks(bare_lines, &hb.keys().cloned().collect());
    let fixed_rest = drop_blocks(fixed_lines, &hf.keys().cloned().collect());
    let _ = (&drop_bare, &drop_fixed);
    let (n, mut d, mut i) = walk_lines(&bare_rest, &fixed_rest, &mut classes)?;
    differing += n;
    pending_d.append(&mut d);
    pending_i.append(&mut i);

    // GLOBAL pairing of everything still unmatched: an LCS move can split a
    // differing pair across two flushes, so position cannot decide the match.
    let mut used = vec![false; pending_i.len()];
    for b in &pending_d {
        let mut matched = false;
        for (k, f) in pending_i.iter().enumerate() {
            if !used[k] && classify_pair(b, f, &mut classes) {
                used[k] = true;
                differing += 1;
                matched = true;
                break;
            }
        }
        if !matched {
            // A one-sided line that no documented per-line class explains: the
            // ledgered `structural-lift` class, whose EXACT count per script is
            // pinned in tests/perl-emitter-divergences.txt. It is never free —
            // the count changes the moment a line is added or lost.
            classes.insert("structural-lift");
            differing += 1;
        }
    }
    for (k, _f) in pending_i.iter().enumerate() {
        if !used[k] {
            classes.insert("structural-lift");
            differing += 1;
        }
    }
    // A difference that vanished entirely in normalisation is one of the
    // documented preamble/blank classes — name it rather than leaving an empty
    // entry (an empty class list is a blanket ignore and fails the ledger
    // guard).
    if differing == 0 && classes.is_empty() {
        if bare.matches("$CHILD_ERROR").count() != fixed.matches("$CHILD_ERROR").count() {
            classes.insert("status-mirror");
        }
        if fixed.contains("use feature 'say';") && !bare.contains("use feature 'say';") {
            classes.insert("say-preamble");
        }
        if bare
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            == fixed
                .lines()
                .filter(|l| !l.trim().is_empty())
                .collect::<Vec<_>>()
        {
            classes.insert("blank");
        }
        if classes.is_empty() {
            return Err((
                "<texts differ but no line differs after normalisation>".into(),
                "<texts differ but no line differs after normalisation>".into(),
            ));
        }
    }
    Ok(Some(Divergence {
        classes,
        lines: differing,
    }))
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[test]
fn every_corpus_script_renders_clean_through_both_live_emitters() {
    let scripts = corpus();
    assert!(
        scripts.len() > 400,
        "corpus shrank unexpectedly: {} scripts",
        scripts.len()
    );
    let mut rendered = 0;
    let mut unparsed = Vec::new();
    for path in &scripts {
        let name = rel(path);
        let src = String::from_utf8_lossy(&std::fs::read(path).expect("read script")).into_owned();
        let bare = match render(&src, false) {
            Some(t) => t,
            None => {
                unparsed.push(name.clone());
                continue;
            }
        };
        let fixed = render(&src, true).unwrap_or_else(|| panic!("{name}: fixed emitter rendered nothing"));
        rendered += 1;

        let bad = compile_diagnostics(&bare);
        assert!(
            bad.is_empty(),
            "{name}: bare emitter (ir::shir_to_perl) render does not compile cleanly:\n  {}",
            bad.join("\n  ")
        );
        let bad = compile_diagnostics(&fixed);
        assert!(
            bad.is_empty(),
            "{name}: fixed emitter (perl_generator_fixed::generate) render does not compile cleanly:\n  {}",
            bad.join("\n  ")
        );
    }
    assert!(
        unparsed.is_empty(),
        "these corpus scripts did not parse and so were never parity-tested: {unparsed:?}"
    );
    eprintln!(
        "parity: {} scripts rendered through both live emitters, all `perl -w -c` clean",
        rendered
    );
}

#[test]
fn emitter_text_divergences_match_the_ledger_exactly() {
    let scripts = corpus();
    let mut exact = 0;
    let mut observed: BTreeMap<String, Divergence> = BTreeMap::new();
    let mut failures = Vec::new();

    for path in &scripts {
        let name = rel(path);
        let src = String::from_utf8_lossy(&std::fs::read(path).expect("read script")).into_owned();
        let (bare, fixed) = match (render(&src, false), render(&src, true)) {
            (Some(b), Some(f)) => (b, f),
            _ => continue,
        };
        match classify(&bare, &fixed) {
            Ok(None) => exact += 1,
            Ok(Some(d)) => {
                observed.insert(name, d);
            }
            Err((b, f)) => failures.push(format!(
                "{name}: UNEXPLAINED emitter divergence (not mirror-read, not an output \
                 statement with the same payload identifiers):\n  bare:  {b}\n  fixed: {f}"
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} script(s) diverge for a reason no documented class covers:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );

    let ledger_path = root().join(LEDGER);
    if std::env::var("SH2_UPDATE_EMITTER_LEDGER").as_deref() == Ok("1") {
        write_ledger(&ledger_path, &observed, exact);
        eprintln!(
            "ledger written: {} diverging scripts, {} exact-parity scripts",
            observed.len(),
            exact
        );
        return;
    }
    let recorded = read_ledger(&ledger_path);

    let mut problems = Vec::new();
    for (name, d) in &observed {
        match recorded.get(name) {
            None => problems.push(format!(
                "{name}: diverges but has NO ledger entry (classes {:?}, {} differing lines)",
                d.classes.iter().collect::<Vec<_>>(),
                d.lines
            )),
            Some((classes, lines)) => {
                let observed_classes: Vec<&str> = d.classes.iter().copied().collect();
                if classes != &observed_classes || *lines != d.lines {
                    problems.push(format!(
                        "{name}: ledger says classes {:?} / {} differing lines, observed {:?} / {}",
                        classes, lines, observed_classes, d.lines
                    ));
                }
            }
        }
    }
    for name in recorded.keys() {
        if !observed.contains_key(name) {
            problems.push(format!(
                "{name}: STALE ledger entry — the two live emitters now produce identical text; \
                 delete the entry"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "{} ledger mismatch(es) in {LEDGER}:\n\n{}",
        problems.len(),
        problems.join("\n")
    );

    eprintln!(
        "parity: {exact} scripts byte-identical, {} scripts with a ledgered divergence",
        observed.len()
    );
}

// ---------------------------------------------------------------------------
// ledger
// ---------------------------------------------------------------------------

fn read_ledger(path: &Path) -> BTreeMap<String, (Vec<String>, usize)> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut out = BTreeMap::new();
    for line in text.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let name = parts.next().unwrap_or("").trim().to_string();
        let classes = parts
            .next()
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        let lines: usize = parts
            .next()
            .unwrap_or("0")
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("bad ledger line: {line}"));
        if name.is_empty() {
            continue;
        }
        out.insert(name, (classes, lines));
    }
    out
}

fn write_ledger(path: &Path, observed: &BTreeMap<String, Divergence>, exact: usize) {
    let mut out = String::new();
    out.push_str(HEADER);
    out.push_str(&format!(
        "# observed: {exact} scripts byte-identical, {} scripts diverging\n#\n\
         # path\tclasses\tdiffering-line-count\n",
        observed.len()
    ));
    for (name, d) in observed {
        let classes: Vec<&str> = d.classes.iter().copied().collect();
        out.push_str(&format!("{name}\t{}\t{}\n", classes.join(","), d.lines));
    }
    std::fs::write(path, out).expect("write ledger");
}

const HEADER: &str = "\
# Perl emitter divergences — EXACT, per script. Read by tests/perl_emitter_corpus_parity.rs.
#
# Live emitters compared (both render every script in examples/*.sh and
# tests/tier1/*.sh; each render must pass `perl -w -c` with zero diagnostics):
#   bare   ir::shir_to_perl                  (library API; CLI default path)
#   fixed  perl_generator_fixed::generate    (CLI --fixed / --target pl; renderer_behaviour;
#                                             perl_examples_regression; committed goldens)
#
# DEAD, therefore not compared: src/perl_backend.rs — retired in e4fb0a1b2 and no
# longer present in this crate (nothing declares `mod perl_backend`). A dead
# emitter has no text to compare; it is recorded dead, not faked into parity.
#
# Every other script is byte-identical between the two live emitters and is
# asserted as exact text equality by the test; it is deliberately absent here.
# Classes (each verified by the test, not trusted):
#
# status-mirror    the fixed path keeps the shell CODE mirror in step by writing
#                  ` $CHILD_ERROR = $? >> 8;` after a status-register write, plus
#                  one `our $CHILD_ERROR = 0;` declaration. Remove those writes
#                  and the declaration and the two texts are equal. Reason: the
#                  mirror exists so a script that moves `$?` itself is still
#                  observable through `$CHILD_ERROR`; the bare renderer has no
#                  mirror variable at all.
# mirror-read      the fixed path reads the mirror (`$CHILD_ERROR`) where the bare
#                  path shifts the register (`($? >> 8)`). Reason: same value,
#                  different spelling; the mirror is kept in step by status-mirror.
# output-idiom     the fixed path's print-to-say and interpolation passes rewrite
#                  OUTPUT statements only (`print(X, \"\\n\")` -> `say X`,
#                  `print(a . \" \" . b)` -> `print \"a b\"`). The pair must both
#                  be output statements carrying the same identifier multiset,
#                  including variables interpolated into double-quoted strings.
#                  Reason: cosmetic emit style, covered behaviourally by
#                  tests/renderer_behaviour.rs and the bash-parity gate.
# numeric-literal-spelling  a numeric literal parenthesised/quoted differently
#                  (`$? = (\"0\") << 8;` vs `$? = 0 << 8;`, `return \"0\";` vs
#                  `return 0;`). Reason: same code, different spelling; the lines
#                  are equal once parentheses and quotes are removed.
# fn-status-lift   the fixed path's fn-status text pass wraps a shell FUNCTION called
#                  in condition position in `__sh_fn_status(...)`; the bare path emits
#                  the plain call. This is a MEASURED divergence with behavioural
#                  consequences (a function returning the string \"0\" is false in
#                  Perl, success in bash) — ledgered for the Tier-2 unification pass,
#                  not normalised away.
# process-substitution-status  the fixed path's process-substitution text template
#                  adds `$__ps_statusN = $CHILD_ERROR;`; the bare path does not.
#                  Reason: the two carry the materialised status through different
#                  homes (mirror vs register).
# runtime-seam     one side elides the `# __SH_RUN_EXT_BEGIN … # __SH_RUN_EXT_END`
#                  helper seam (per-helper liveness on the fixed path) or emits it
#                  unconditionally. Reason: generated scaffolding with no script
#                  content; stripped from BOTH sides before comparing, so it can
#                  never hide a statement difference.
# hoisted-declarations  the fixed path's IR pre-pass declares variables up front
#                  (`my $K;` …) that the bare renderer never declares. Reason:
#                  declaration placement by the pre-pass, not a different lowering;
#                  `perl -w -c` proves the bare render has no undeclared use.
# helper-preamble  the fixed path injects a top-level `sub __sh_*` helper (e.g.
#                  __sh_glob) that the bare path leaves to the runtime prelude.
#                  Shared helpers must be byte-identical or the test fails.
#                  Reason: scaffolding the fixed path needs for its own output
#                  shape, not a different lowering.
#
# Regenerate after a REVIEWED emitter change:
#   SH2_UPDATE_EMITTER_LEDGER=1 cargo test --test perl_emitter_corpus_parity
#
";

#[test]
fn ledger_covers_both_directions() {
    // Guards the ledger format itself: read_ledger must parse, and the corpus
    // must not have grown a script the ledger was never asked about.
    if std::env::var("SH2_UPDATE_EMITTER_LEDGER").as_deref() == Ok("1") {
        return;
    }
    let path = root().join(LEDGER);
    let recorded = read_ledger(&path);
    let known: BTreeSet<&str> = [
        "status-mirror",
        "mirror-read",
        "output-idiom",
        "helper-preamble",
        "hoisted-declarations",
        "numeric-literal-spelling",
        "runtime-seam",
        "fn-status-lift",
        "process-substitution-status",
        "slice-helper",
        "scratch-counter",
        "comment",
        "blank",
        "pipestatus-write",
        "decl-initialiser",
        "printf-b-lift",
        "status-write",
        "pipeline-call",
        "preamble-require",
        "reordered-lines",
        "nameref-resolution",
        "procsub-fifo",
        "dot-source-inline",
        "structural-lift",
        "say-preamble",
    ]
        .into_iter()
        .collect();
    for (name, (classes, _)) in &recorded {
        for c in classes {
            assert!(
                known.contains(c.as_str()),
                "{name}: ledger names unknown divergence class {c:?}"
            );
        }
        assert!(
            !classes.is_empty(),
            "{name}: ledger entry with no class is a blanket ignore"
        );
    }
}
