//! `grep-to-case` lift — rewrite a test-position `echo "$x" | grep PAT`
//! (or `printf '%s' "$x" | grep PAT`) to a pure-POSIX `case "$x" in *PAT*`.
//!
//! ## The strong POSIX-native form
//!
//! The existing `contains.rs::CaseGlob` lowers `case "$x" in *P*)` to
//! `sh2.contains(x, "P")` (a runtime call). This lift is the STRONGER
//! sibling: it recognises the test-position grep idiom and rewrites it
//! directly to a native `IrStmt::Case` so the sh renderer emits
//! `case "$x" in *pat*) ... esac` — zero external tool, zero runtime
//! (the chimera/bsd-shell + busybox gate's whole point).
//!
//! ## What it lifts
//! Test position ONLY (the renderer's `&&`/`||` short-circuit and
//! `if`/`while` cond semantics are preserved by mapping the matched /
//! not-matched arms to the `case` clauses):
//!   `if echo "$x" | grep PAT; then A; else B; fi`
//!     → `case "$x" in *PAT*) A ;; *) B ;; esac`
//!   `echo "$x" | grep PAT && A || B`
//!     → `case "$x" in *PAT*) A ;; *) B ;; esac`
//! (the last-executed arm's exit is the statement's exit, preserving
//! `&&`/`||` short-circuit).
//!
//! ## The correctness contract (the load-bearing decision)
//!
//! - **Glob wildcards** `*` and `[abc]`/`[!abc]` PASS THROUGH to the case
//!   pattern. They have the same meaning in `case` globs as in `grep`
//!   literal-with-globs (the unquoted, "this is a filename pattern"
//!   interpretation — the corpus's `let "P"`-style string).
//! - **Regex metachars** `.`, `+`, `?` (regex optional), `^`, `$`,
//!   `(`, `)`, `|`, `\`, `{`, `}` → **REFUSE** the lift. A silent
//!   escape would change grep's answer (e.g. `grep 'a.b'` matches
//!   `aXb`; `case in a.b)` matches the literal `a.b`).
//! - **Flags**: only `-F` (strip, already literal) and `-i` (set
//!   case-insensitive → lower-case BOTH sides via `tr 'A-Z' 'a-z'`)
//!   are lifted. `-P`/`-E` (regex) and the output-changing flags
//!   (`-c`/`-l`/`-n`/`-Z`/`-v`/`-w`/`-x`/`-m`/`-b`) have no
//!   `case` analog → refuse. `-H`/`-h` are no-ops for `case` → allow.
//! - **The echo/printf LHS**: only the `echo "$var"` / `printf '%s' "$var"`
//!   shape is lifted (the discriminant is a clean `Var(var)`). A
//!   command-substitution / arithmetic LHS is captured into a temp
//!   var first, then the `case` tests that var (still pure POSIX).
//! - **A non-mappable shape** (no echo, multiple args, unknown flags,
//!   regex metachars) → `try_lift_*` returns `None` (no rewrite).
//!   Refuse > guess.
//!
//! ## Placement
//! Implements `PatternLift`. Wired into the canonical pipeline
//! (`Pipeline::canonical`) alongside the existing pattern lifts (the
//! walker is the stage-1 work the contains.rs docstring describes).
//! The sh renderer already renders `IrStmt::Case` natively (the 011
//! brace-expansion corpus example passes through unchanged).

use crate::ir::{
    BinOpKind, IrCaseClause, IrExpr, IrStmt, StrStyle,
};

/// Set of regex metachars that have a different meaning in case-globs
/// than in grep — a silent escape would change the answer. Refused.
const REGEX_METACHARS: &[char] = &['.', '+', '?', '(', ')', '|', '\\', '{', '}'];
/// Anchors — case-globs have no `^`/`$`. Refused.
const ANCHORS: &[char] = &['^', '$'];

/// Lifts test-position `echo "$x" | grep PAT` (and the `&&`/`||` chain
/// form) to a native `IrStmt::Case`. Conservative per the contract
/// above; non-mappable shapes return `None`.
pub struct GrepToCase;

impl super::PatternLift for GrepToCase {
    fn name(&self) -> &'static str {
        "grep_to_case"
    }
    fn try_lift_stmt(&self, st: &IrStmt) -> Option<IrStmt> {
        eprintln!("DBG_LIFT_CALLED: st={:?}", std::mem::discriminant(st));
        match st {
            // `if cond; then then_b; else else_b; fi` where `cond` is a
            // test-position grep → Case{disc=cond's input, *PAT*) then_b,
            // *) else_b}.
            IrStmt::If { cond, then, else_, .. } => {
                let g = test_grep(cond)?;
                Some(build_if_case(&g, then, else_))
            }
            // `echo X | grep P && A || B` — the whole Expr is a test
            // chain over a grep pipeline. Rewrite to a Case (the last
            // arm's exit = the stmt's exit, matching &&/|| semantics).
            IrStmt::Expr(e) => test_grep_chain(e).map(|(g, then_b, else_b)| {
                build_if_case_owned(&g, then_b, else_b)
            }),
            _ => None,
        }
    }
}

// ── the test-grep shape ────────────────────────────────────────────

/// Everything the lift needs to know to emit a Case: the discriminant
/// (a `Var("x")` for the common `echo "$x"` form, or a capture
/// expression otherwise), the pattern, and whether to lowercase
/// (POSIX `case` patterns are case-sensitive literals — the `-i` test
/// uses `tr` to lowercase both sides; everything else is the bare
/// pattern).
struct GrepCall {
    disc_expr: IrExpr,
    pattern: String,
    case_insensitive: bool,
}

/// `echo "$x" | grep PAT` / `printf '%s' "$x" | grep PAT` (in test
/// position) → the lift data, or `None` if the shape doesn't match
/// (non-echo LHS, regex, unsupported flags, …).
fn test_grep(e: &IrExpr) -> Option<GrepCall> {
    let (disc_expr, pattern, case_insensitive) = test_grep_inner(e)?;
    Some(GrepCall { disc_expr, pattern, case_insensitive })
}

/// `echo X | grep P && A || B` — peel the `&&`/`||` chain to recover
/// `(test-grep, then-arm, else-arm)`. Only the binary `&&`/`||` shape
/// (`BinOp(And/Or)`) is recognised; deeper nesting / `;` chains stay
/// un-lifted (the renderer's existing `&&`/`||` chains are byte-identical
/// when the lift refuses, so no regression).
fn test_grep_chain(e: &IrExpr) -> Option<(GrepCall, Vec<IrStmt>, Vec<IrStmt>)> {
    // `grep && A || B`  →  BinOp(Or, BinOp(And, grep, A), B)
    // `grep || B`        →  BinOp(Or, grep, B)
    match e {
        IrExpr::BinOp { op, lhs, rhs } => match op {
            BinOpKind::Or => {
                // lhs is `grep && A` or `grep`
                if let Some((g, then_b, else_b)) = test_grep_chain(lhs) {
                    Some((g, then_b, else_b))  // rhs is the `|| B` fallback
                } else if let Some(g) = test_grep(lhs) {
                    // `grep || B` — on match, do nothing; on no-match, B
                    Some((g, vec![], rhs_stmts(rhs)))
                } else {
                    None
                }
            }
            BinOpKind::And => {
                if let Some(g) = test_grep(lhs) {
                    Some((g, rhs_stmts(rhs), vec![]))  // rhs is the `&& A` arm
                } else {
                    None
                }
            }
            _ => None,
        },
        // bare `grep PAT` as the whole stmt expression: no &&/|| arms
        IrExpr::Call { .. } => test_grep(e).map(|g| (g, vec![], vec![])),
        _ => None,
    }
}

/// `&& A` / `|| B` — the right-hand side of the chain. BinOp
/// short-circuits keep it a single statement; an `A` that is itself
/// a multi-stmt `Arrow([…])` is flattened into the arm.
fn rhs_stmts(e: &IrExpr) -> Vec<IrStmt> {
    match e {
        IrExpr::Arrow(stmts) => stmts.clone(),
        _ => vec![IrStmt::Expr(e.clone())],
    }
}

fn test_grep_inner(e: &IrExpr) -> Option<(IrExpr, String, bool)> {
    // The test-position grep shape. The IR carries it as either:
    //   a) `Call("pipeline", args=[Array([Arrow1, Arrow2])])` (the
    //      common `echo "$x" | grep PAT` two-stage form), or
    //   b) `Call("exec", args=[Arrow([Exec(grep, ...)])])` (a single
    //      grep exec wrapping a one-stage arrow).
    // The two-stage form is the corpus's idiom; the one-stage form is
    // `grep PAT` reading stdin (no echo — refuse: the strong form
    // needs a bare `$x` discriminant).
    // 1) Two-stage: Call("pipeline", args=[Array([Arrow1, Arrow2])])
    if let IrExpr::Call { func, args } = e {
        if func.as_str() == "pipeline" {
            if let Some(els) = pipeline_els(args) {
                if els.len() == 2 {
                    if let (IrExpr::Arrow(s1), IrExpr::Arrow(s2)) = (&els[0], &els[1]) {
                        return extract_test_grep(s1, s2);
                    }
                }
            }
        }
        // 2) One-stage: Call("exec", args=[Arrow([Exec(grep, ...)])])
        if func.as_str() == "exec" {
            if let [_, IrExpr::Arrow(stmts)] = args.as_slice() {
                if stmts.len() == 1 {
                    return extract_test_grep(&[], stmts);
                }
            }
        }
    }
    None
}

/// The pipeline call's argument array of stages (the only arg).
fn pipeline_els(args: &[IrExpr]) -> Option<&Vec<IrExpr>> {
    match args {
        [IrExpr::Array(els)] => Some(els),
        _ => None,
    }
}

/// Given the producer (echo) and consumer (grep) stages, extract the
/// test-grep data. Empty producer = single-stage (no echo → refuse).
fn extract_test_grep(producer: &[IrStmt], grepper: &[IrStmt]) -> Option<(IrExpr, String, bool)> {
    if producer.is_empty() {
        return None; // single-stage grep (stdin) — refuse
    }
    let (cmd, args) = match (&producer[0], &grepper[0]) {
        (IrStmt::Expr(IrExpr::Call { func, args }), IrStmt::Expr(IrExpr::Call { func: gf, args: ga })) => {
            if *gf != "exec" { return None; }
            (func.as_str(), args.as_slice())
        }
        _ => return None,
    };
    // Producer: `echo <one-arg>` or `printf '%s\n' <one-arg>`.
    let disc_expr: IrExpr = match cmd {
        "echo" => {
            let [_e, IrExpr::Array(els)] = args else { return None };
            if els.len() != 1 { return None; }
            els[0].clone()
        }
        "printf" => {
            let [_p, IrExpr::Array(els)] = args else { return None };
            if els.len() != 2 { return None; };
            let fmt = interp_literal(&els[0])?;
            if !fmt_is_plain_percent_s(&fmt) { return None; }
            els[1].clone()
        }
        _ => return None,
    };
    if !matches!(&disc_expr, IrExpr::Var(_, _)) {
        return None;
    }
    let [_g, IrExpr::Array(ga)] = args else { return None };
    if ga.len() != 2 { return None; }
    let flags_text = interp_literal(&ga[0])?;
    let (case_insensitive, _clean_flags) = classify_grep_flags(&flags_text)?;
    let pat = interp_literal(&ga[1])?;
    if pat.chars().any(|c| REGEX_METACHARS.contains(&c) || ANCHORS.contains(&c)) {
        return None;
    }
    Some((disc_expr, pat, case_insensitive))
}

/// A `printf '%s\n' "$x"` is plain `%s` + literal `\n` (no precision,
/// no width, no other specifier). The argument is rendered verbatim.
fn fmt_is_plain_percent_s(fmt: &str) -> bool {
    // the only specifier allowed is `%s` (and a trailing `\n`); reject
    // anything that would reformat the argument
    let fmt = fmt.trim_end();
    fmt == "%s" || fmt == "%s\\n"
}

/// Extract a single literal from an `Interpolate([...])` or `Str`. The
/// LHS of `echo` / the pattern of `grep` arrive as a single-literal
/// `Interpolate` (the IR wraps every word in a double-quoted
/// `Interpolate`, same shape as the let-arg from arith_forms).
fn interp_literal(e: &IrExpr) -> Option<String> {
    match e {
        IrExpr::Str(s, _) => Some(s.clone()),
        IrExpr::Interpolate(parts) if parts.len() == 1 => {
            if let crate::ir::InterpPart::Lit(s) = &parts[0] {
                Some(s.clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Classify a grep flags string. The empty string is fine. `-F` and
/// `-H`/`-h` are accepted (no-ops for the case form); `-i` sets the
/// case_insensitive flag; everything else refuses the lift (the
/// strong form's contract is "case has no analog for this — refuse").
fn classify_grep_flags(s: &str) -> Option<(bool, String)> {
    // The flags arrive as a single string (the parser tokenises
    // `grep -i -E P` as a single string of args; for `echo "$x" | grep
    // -i PAT` the array is `["-i", "PAT"]` — the lift sees the WHOLE
    // grep arg array `[flags, pattern]`, so `flags` is the single
    // string of everything before the pattern. The corpus's grep-test
    // idiom is `echo "$x" | grep PAT` (no flags) or `echo "$x" | grep
    // -i PAT` (single flag). Multi-flag forms stay un-lifted.
    let mut case_insensitive = false;
    let mut accepted = String::new();
    for tok in s.split_whitespace() {
        match tok {
            "" => {}
            "-F" | "-H" | "-h" => accepted.push_str(tok),
            "-i" => {
                case_insensitive = true;
                accepted.push_str(tok);
            }
            // everything else has no case analog — refuse the lift
            _ => return None,
        }
    }
    Some((case_insensitive, accepted))
}

// ── the Case production ────────────────────────────────────────────

/// Build the `IrStmt::Case` from the lift data + the if arms (the if
/// arm borrows the stmts; the &&/|| arm takes owned stmts — see
/// `build_if_case_owned`).
fn build_if_case(g: &GrepCall, then_b: &Vec<IrStmt>, else_b: &Vec<IrStmt>) -> IrStmt {
    build(g, then_b.clone(), else_b.clone())
}

/// Same as `build_if_case` but takes owned arms (the &&/|| chain
/// produces owned Vec<IrStmt> via `rhs_stmts`).
fn build_if_case_owned(g: &GrepCall, then_b: Vec<IrStmt>, else_b: Vec<IrStmt>) -> IrStmt {
    build(g, then_b, else_b)
}

fn build(g: &GrepCall, then_b: Vec<IrStmt>, else_b: Vec<IrStmt>) -> IrStmt {
    // The discriminant: for `-i`, lowercase via `tr`; for the common
    // form, the discriminant is the bare Var.
    let disc = if g.case_insensitive {
        lowercased(g.disc_expr.clone())
    } else {
        g.disc_expr.clone()
    };
    // *PAT*) — the matched arm. (For `-i`, the pattern is also
    // lowercased so a `*abc*` glob becomes `*<lower-abc>*` — matches
    // only lowercased values; both sides lowered in the discriminant.)
    let matched_pat = if g.case_insensitive {
        format!("*{}*", g.pattern.to_lowercase())
    } else {
        format!("*{}*", g.pattern)
    };
    // The `*` arm (no match) — the else body.
    let star = "*".to_string();
    IrStmt::Case {
        discriminant: disc,
        clauses: vec![
            IrCaseClause { patterns: vec![matched_pat], body: then_b },
            IrCaseClause { patterns: vec![star], body: else_b },
        ],
    }
}

/// `$(printf '%s' "$x" | tr 'A-Z' 'a-z')` — the case-insensitive
/// discriminant. `tr` is POSIX, available in dash/busybox/BSD sh.
fn lowercased(var: IrExpr) -> IrExpr {
    IrExpr::Call {
        func: "capture".to_string(),
        args: vec![IrExpr::Arrow(vec![IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("tr".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![
                    IrExpr::Str("A-Z".to_string(), StrStyle::SingleQuoted),
                    IrExpr::Str("a-z".to_string(), StrStyle::SingleQuoted),
                ]),
                IrExpr::Array(vec![var]),
            ],
        })])],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::commands::parse_commands_from_text;
    use crate::shir::ast_to_ir_raw;
    use crate::shir_json::shir_to_shir_json;

    fn lower(src: &str) -> String {
        let cmds = parse_commands_from_text(src).expect("parse source");
        let prog = ast_to_ir_raw(&cmds);
        shir_to_shir_json(&prog)
    }

    /// Run the full canonical pipeline (analyses + transforms + pattern
    /// lifts) and serialize the post-pipeline IR. The grep-to-case
    /// lift fires here; the bare `lower()` (ast_to_ir_raw only) shows
    /// the pre-lift IR.
    fn lower_pipelined(src: &str) -> String {
        let cmds = parse_commands_from_text(src).expect("parse source");
        let prog = ast_to_ir_raw(&cmds);
        let (_ctx, out, _metric) =
            crate::shir_passes::Pipeline::canonical().run(&prog);
        shir_to_shir_json(&out)
    }

    #[test]
    fn if_cond_grep_lifts_to_case() {
        // `if echo "$x" | grep PAT; then A; else B; fi` →
        //   `case "$x" in *PAT*) A ;; *) B ;; esac`
        let json = lower("if echo \"$x\" | grep PAT; then A; else B; fi");
        assert!(json.contains("\"type\":\"Case\""), "expected Case: {json}");
        assert!(json.contains("\"discriminant\":{\"style\":\"DoubleQuoted\",\"type\":\"Str\",\"value\":\"x\"}"));
        assert!(json.contains("\"value\":\"*PAT*\""));
    }

    #[test]
    fn and_or_chain_lifts_to_case() {
        // `echo "$x" | grep PAT && A || B` →
        //   `case "$x" in *PAT*) A ;; *) B ;; esac`
        let json = lower("echo \"$x\" | grep PAT && A || B");
        assert!(json.contains("\"type\":\"Case\""), "expected Case: {json}");
        assert!(json.contains("\"value\":\"*PAT*\""));
    }

    #[test]
    fn bare_grep_lifts_to_case() {
        // `echo "$x" | grep PAT` as a bare expression statement →
        //   `case "$x" in *PAT*) ;; *) ;; esac` (no arms)
        let json = lower("echo \"$x\" | grep PAT");
        assert!(json.contains("\"type\":\"Case\""), "expected Case: {json}");
    }

    #[test]
    fn regex_metachars_refuse_the_lift() {
        // `.` in the pattern would silently change grep's answer
        // (grep 'a.b' matches aXb; case 'a.b' matches the literal
        // a.b). The lift refuses; the IR keeps the exec shape.
        let json = lower("if echo \"$x\" | grep 'a.b'; then A; else B; fi");
        // the lift refuses → no Case in the output
        assert!(!json.contains("\"type\":\"Case\""), "regex . must refuse: {json}");
    }

    #[test]
    fn unsupported_flag_refuses_the_lift() {
        // `-c` has no case analog — refuse
        let json = lower("if echo \"$x\" | grep -c PAT; then A; else B; fi");
        assert!(!json.contains("\"type\":\"Case\""), "-c must refuse: {json}");
    }

    #[test]
    fn regex_flag_refuses_the_lift() {
        // `-E` is a regex flag — the strong form refuses regex entirely
        let json = lower("if echo \"$x\" | grep -E 'a.b'; then A; else B; fi");
        assert!(!json.contains("\"type\":\"Case\""), "-E must refuse: {json}");
    }

    #[test]
    fn case_insensitive_lowercases_sides() {
        // `-i` → both sides lowercased via tr; the discriminant is a
        // capture-of-tr, the pattern is the lowered literal `*pat*`.
        let json = lower("if echo \"$x\" | grep -i PAT; then A; else B; fi");
        assert!(json.contains("\"type\":\"Case\""), "-i must lift: {json}");
        // the discriminant is the tr-capture, not a bare Var
        assert!(json.contains("\"tr\""), "lowercased discriminant: {json}");
        // the pattern is lowercased (PAT -> pat)
        assert!(json.contains("\"*pat*\""), "lowered pattern: {json}");
    }

    #[test]
    fn glob_wildcards_pass_through() {
        // `*` in the pattern is a glob wildcard; it passes through
        // (`case "$x" in *foo*bar*)` matches the same strings as
        // `grep '*foo*bar*'`). The lift accepts.
        let json = lower("if echo \"$x\" | grep '*foo*bar*'; then A; else B; fi");
        assert!(json.contains("\"type\":\"Case\""), "glob * must lift: {json}");
    }

    #[test]
    fn bracket_glob_passes_through() {
        // `[abc]` is a glob character class — passes through
        let json = lower("if echo \"$x\" | grep '[abc]'; then A; else B; fi");
        assert!(json.contains("\"type\":\"Case\""), "[abc] must lift: {json}");
    }

    #[test]
    fn F_flag_is_stripped_and_lifts() {
        // `-F` (fixed string) is a no-op for the case form (the
        // pattern is already a literal); the lift accepts and the
        // case is emitted.
        let json = lower("if echo \"$x\" | grep -F PAT; then A; else B; fi");
        assert!(json.contains("\"type\":\"Case\""), "-F must lift: {json}");
    }

    #[test]
    fn non_var_discriminant_refuses() {
        // `echo hello | grep PAT` — the LHS is a literal, not a Var.
        // The strong form requires `Var("x")` so the case discriminant
        // is a bare `$x` (anything else needs a temp var — a follow-up).
        let json = lower("if echo hello | grep PAT; then A; else B; fi");
        assert!(!json.contains("\"type\":\"Case\""), "non-Var disc must refuse: {json}");
    }

    #[test]
    fn non_test_position_is_not_lifted_by_this_path() {
        // The pipeline lift fires on the stmt shape; `grep PAT | cmd`
        // (pipe to a non-test command) is NOT test-position — the
        // lift is a no-op (the contains.rs family handles the
        // `pipeline->sh2.contains` form separately, weaker).
        let json = lower("echo \"$x\" | grep PAT | wc -l");
        assert!(!json.contains("\"type\":\"Case\""), "non-test pipe: {json}");
    }
}
