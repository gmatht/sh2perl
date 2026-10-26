//! test-lowering — glob-affix `[[ ]]` tests with VARIABLE patterns →
//! boolean-returning sh2.* primitives (strHasPrefix / strHasSuffix /
//! contains), the cross-backend speedup for the runtime polyfills
//! (CROSS_BACKEND_RUNTIME.md §8.1).
//!
//! ## Need
//! The polyfill's hot conditions are glob-affix tests with variable
//! patterns — `[[ "$s" == "$p"* ]]`, `[[ "$s" == *"$p"* ]]`,
//! `[[ "$s" == *"$p" ]]` — and they all dispatch `sh2.test("...")` with
//! the test as a STRING (runtime tokenize + parse + glob match per
//! evaluation). The emitter's native test lowering
//! (`try_native_glob_test`) only handles LITERAL patterns (`[ "$x" = *P* ]`
//! → `.includes(P)`); a `$`-containing pattern is refused (the runtime
//! would expand it), so the polyfill's variable-pattern tests stay
//! runtime dispatches. Rewriting them to the sh2.* primitives lets every
//! backend render a direct call (the estree emitter lowers `contains`/
//! `strHasPrefix`/`strHasSuffix` natively to `.includes`/`.startsWith`/
//! `.endsWith`; a backend without a runtime links the polyfill's own
//! primitive).
//!
//! ## Scope — the sound rule
//! A `test` Call whose text is exactly one of the glob-affix shapes with
//! plain-var or literal operands:
//!   `"$s"=="$p"*`  → `strHasPrefix(s, p)`
//!   `"$s"==*"$p"*` → `contains(s, p)`
//!   `"$s"==*"$p"`  → `strHasSuffix(s, p)`
//!   `"$s"==*/`     → `strHasSuffix(s, "/")`
//!   `"$s"==/*`     → `strHasPrefix(s, "/")`
//!   `!=` variants  → `Not(...)` of the above
//! or one of the typed-compare shapes:
//!   `A -op B` (op ∈ `-eq -ne -gt -ge -lt -le`) → a numeric `BinOp`
//!     (both operands `$name` / `"$name"` / signed int / `$((…))`)
//!   `-n X` / `-z X` (exactly one operand) → `X ne ""` / `Not(X ne "")`
//!   `A == B` / `A = B` / `A != B` → `Not(L ne R)` / `L ne R` (STRING
//!     compare — a typed `Eq` renders perl's numeric `==`, so equality
//!     is expressed as the negation of `Ne`, which every backend maps
//!     to a string compare (` ne ` / `!==` / `!=` / `==` on strings))
//! Operands are `$name` / `"$name"` (a plain identifier — the read is
//! the same value the runtime's test would expand) or a literal with no
//! expansion chars (e.g. `/`). Anything else (compound `-a`/`-o` texts,
//! extglob, `$()` operands, `${...}` param operands, `=~`, numeric ops)
//! is left untouched — refuse > guess.
//!
//! Guards:
//! 1. **Status liveness** — fires only in If/While cond position whose
//!    status write is provably unread (the Plan-4 backward scan: no
//!    `$?` reader in the arms before the first arm writer, and the
//!    statement's own status is not consumed). A rewritten cond emits a
//!    bare boolean (the estree `contains` arm in bare position records no
//!    status); a LIVE status would go stale, so the guard refuses it.
//! 2. **Self-recursion** — refuses when the enclosing function is the
//!    target primitive itself (the polyfill's own `contains` body must
//!    not call `sh2.contains` — the per-backend adapter would recurse
//!    infinitely).
//! 3. **nocasematch** — `[[ ]]` matching (`==`/`!=`, glob affixes AND
//!    exact) honors `shopt nocasematch` dynamically in bash, while a
//!    typed `ne`/`Not(ne)` is case-sensitive by construction. If the
//!    program text mentions `nocasematch` at all (conservative: includes
//!    `shopt`/`set -o`/`eval` payloads and even a mere string mention —
//!    refusing a lift is free), the string-shape arms (glob-affix,
//!    exact `==`) stay on the runtime test path, which IS nocasematch-
//!    aware (the emitters wrap `($__nocasematch) ? lc(a) eq lc(b) : …`).
//!    `-n`/`-z` and the numeric arm are unaffected (length/numeric
//!    semantics), so they are not gated.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS gated). Reuses the
//! shir.rs lastExit-liveness helpers (`walk_lastexit_liveness` /
//! `lastexit_scan_top_read` / `is_pure_test_chain` — made pub(crate)).

use crate::ir::{BinOpKind, IrExpr, IrStmt, StrStyle};
use std::collections::HashSet;

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // The live set is computed ONCE on the original tree (statement
    // pointers as keys). The rewrites below only mutate cond EXPRESSIONS
    // in place — the If/While statement pointers stay stable, so the
    // verdicts remain valid. A rewritten glob-affix test is never a `$?`
    // reader (the shapes contain no `$?`), so other statements' liveness
    // is unchanged by the rewrite.
    let mut live: HashSet<usize> = HashSet::new();
    crate::shir::walk_lastexit_liveness(stmts, true, &mut live);
    // Conservative whole-program nocasematch probe (guard 3): the Debug
    // rendering carries every literal, `shopt`/`set`/`eval` payload and
    // nested body, so a mention ANYWHERE refuses the string-shape lifts
    // everywhere — an over-approximation can only under-lift, never
    // mis-lift. A transform pass runs once per program; the allocation
    // is irrelevant next to the walk itself.
    let nocasematch = format!("{stmts:?}").contains("nocasematch");
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt_pass(st, &live, nocasematch);
    }
    c
}

/// Walk statements.
fn stmt_pass(st: &mut IrStmt, live: &HashSet<usize>, nc: bool) -> bool {
    // The statement pointer (the liveness key) is computed BEFORE the
    // mutable field borrows below.
    let self_live = live.contains(&(st as *const IrStmt as usize));
    match st {
        IrStmt::Function { body, .. } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt_pass(s, live, nc);
            }
            c
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            // The conds' status liveness is computed ONCE (immutable
            // borrows), then the rewrites run (mutable borrows).
            let observable = cond_status_observable(then, elsifs, else_, self_live);
            let mut c = rewrite_cond(cond, observable, nc);
            for s in then.iter_mut() {
                c |= stmt_pass(s, live, nc);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= rewrite_cond(ec, observable, nc);
                for s in eb.iter_mut() {
                    c |= stmt_pass(s, live, nc);
                }
            }
            for s in else_.iter_mut() {
                c |= stmt_pass(s, live, nc);
            }
            c
        }
        IrStmt::While { cond, body } => {
            let observable = cond_status_observable(body, &[], &[], self_live);
            let mut c = rewrite_cond(cond, observable, nc);
            for s in body.iter_mut() {
                c |= stmt_pass(s, live, nc);
            }
            c
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let observable = cond_status_observable(body, &[], &[], self_live);
            let mut c = rewrite_cond(cond, observable, nc);
            for s in body.iter_mut() {
                c |= stmt_pass(s, live, nc);
            }
            c
        }
        IrStmt::For { body, .. } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt_pass(s, live, nc);
            }
            c
        }
        IrStmt::ForInit { init, cond, step, body } => {
            let observable = cond_status_observable(body, &[], &[], self_live);
            let mut c = rewrite_cond(cond, observable, nc);
            for s in init.iter_mut().chain(step.iter_mut()).chain(body.iter_mut()) {
                c |= stmt_pass(s, live, nc);
            }
            c
        }
        IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt_pass(s, live, nc);
            }
            c
        }
        IrStmt::Redirect { inner, .. } => {
            let mut c = false;
            for s in inner.iter_mut() {
                c |= stmt_pass(s, live, nc);
            }
            c
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for stage in stages.iter_mut() {
                for s in stage.iter_mut() {
                    c |= stmt_pass(s, live, nc);
                }
            }
            c
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            let mut c = false;
            for s in body.iter_mut().chain(else_body.iter_mut()).chain(finally_body.iter_mut()) {
                c |= stmt_pass(s, live, nc);
            }
            for ex in excepts.iter_mut() {
                for s in ex.body.iter_mut() {
                    c |= stmt_pass(s, live, nc);
                }
            }
            c
        }
        // Statement-position test chains: `[[ … ]] && cmd` /
        // `[[ … ]] || cmd` desugar to Expr(And/Or(test, cmd)) — a
        // GUARD idiom the cond path never sees. The chain's value IS
        // its status (the renderers branch on the And/Or value), so a
        // lowered leaf keeps the branch decision identical to bash's
        // test status — provided nothing can observe the difference
        // between a test CALL's status write and a pure value:
        //   * the statement's own status must be dead (self_live —
        //     the same consumer contract rewrite_cond honours), and
        //   * no operand inside the chain may read `$?` (the rhs of
        //     `&&` runs after the leaf, so `[[ t ]] && echo "$?"`
        //     observes the leaf's status write directly).
        // Both refuse → keep the test call untouched.
        IrStmt::Expr(e) => {
            if self_live || crate::shir::ir_expr_reads_status(e) {
                return false;
            }
            rewrite_stmt_chain(e, nc)
        }
        _ => false,
    }
}

/// Lower the `test` leaves of a STATEMENT-position `&&`/`||` chain.
/// Unlike [`rewrite_test_chain`] (cond position) the chain is NOT
/// required to be pure — its rhs is a command — so the walk recurses
/// only through And/Or/Not combinators and lowers every reachable
/// `test` leaf, leaving commands and any other expression untouched.
fn rewrite_stmt_chain(e: &mut IrExpr, nc: bool) -> bool {
    match e {
        IrExpr::BinOp { op, lhs, rhs } if matches!(op, BinOpKind::And | BinOpKind::Or) => {
            let l = rewrite_stmt_chain(lhs, nc);
            let r = rewrite_stmt_chain(rhs, nc);
            l | r
        }
        IrExpr::BinOp { op: BinOpKind::Not, lhs, rhs } => {
            // Same `!`-shape handling as the cond walk: lower the
            // inner leaf, wrap stays Not; rewrite a dead rhs twin too.
            let mut c = false;
            if let IrExpr::Call { func, args, .. } = &**lhs {
                if func == "test" {
                    if let Some(prim) = lower_test_leaf(args, nc) {
                        **lhs = prim;
                        c = true;
                    }
                }
            }
            if !c {
                c = rewrite_stmt_chain(lhs, nc);
            }
            c | rewrite_stmt_chain(rhs, nc)
        }
        IrExpr::Call { func, args, .. } if func == "test" => {
            if let Some(prim) = lower_test_leaf(args, nc) {
                *e = prim;
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Is the cond's status write observable from the arms or the statement's
/// consumer? Mirrors compute_test_cond_deadness: no arm/body statement
/// reads `$?` before the first arm writer, and the run arm has a writer
/// of its own (or the statement's own status is not consumed).
fn cond_status_observable(
    then: &[IrStmt],
    elsifs: &[(IrExpr, Vec<IrStmt>)],
    else_: &[IrStmt],
    self_live: bool,
) -> bool {
    if elsifs.is_empty() && else_.is_empty() {
        // While / DoWhile / ForInit: the body is the only arm.
        return crate::shir::lastexit_scan_top_read(then, self_live);
    }
    let mut top = crate::shir::lastexit_scan_top_read(then, self_live);
    for (_, arm) in elsifs {
        top |= crate::shir::lastexit_scan_top_read(arm, self_live);
    }
    top |= crate::shir::lastexit_scan_top_read(else_, self_live);
    top
}

/// Rewrite a cond that is a pure `test` chain whose status write is
/// provably unread. Returns whether anything changed.
fn rewrite_cond(cond: &mut IrExpr, observable: bool, nc: bool) -> bool {
    if std::env::var("SH2_TL_DEBUG").is_ok() && observable {
        eprintln!("[tl] refuse: status observable :: {cond:?}");
    }
    if std::env::var("SH2_TL_DEBUG").is_ok() && !crate::shir::is_pure_test_chain(cond) {
        eprintln!("[tl] refuse: not a pure test chain :: {cond:?}");
    }
    if observable || !crate::shir::is_pure_test_chain(cond) {
        return false;
    }
    rewrite_test_chain(cond, nc)
}

/// Rewrite a pure test chain's glob-affix leaves. Returns whether
/// anything changed.
fn rewrite_test_chain(e: &mut IrExpr, nc: bool) -> bool {
    match e {
        IrExpr::BinOp { op, lhs, rhs } if matches!(op, BinOpKind::And | BinOpKind::Or) => {
            let l = rewrite_test_chain(lhs, nc);
            let r = rewrite_test_chain(rhs, nc);
            l | r
        }
        IrExpr::BinOp { op: BinOpKind::Not, lhs, rhs } => {
            // `!`-negated leaf: rewrite the inner test, wrap in Not. The
            // desugar sometimes leaves a DEAD rhs twin of the same test
            // (renderers ignore it) — rewrite it too so no backend can
            // resurrect the string-test path from the unused side.
            let mut c = false;
            if let IrExpr::Call { func, args, .. } = &**lhs {
                if func == "test" {
                    if let Some(prim) = lower_test_leaf(args, nc) {
                        **lhs = prim;
                        c = true;
                    }
                }
            }
            if !c {
                c = rewrite_test_chain(lhs, nc);
            }
            c | rewrite_test_chain(rhs, nc)
        }
        IrExpr::Call { func, args, .. } if func == "test" => {
            if let Some(prim) = lower_test_leaf(args, nc) {
                *e = prim;
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// One `test` leaf → the structured primitive (or None): the glob-affix
/// shapes first, then the numeric-compare, nonempty and string-equality
/// shapes, then a two-operand `-a`/`-o` compound of those. The test text
/// is args[0] (a Str); args[1] is the `[[`/`[` style tag (ignored).
fn lower_test_leaf(args: &[IrExpr], nc: bool) -> Option<IrExpr> {
    let r = glob_affix_primitive(args, nc)
        .or_else(|| numeric_compare_primitive(args))
        .or_else(|| nonempty_primitive(args))
        .or_else(|| string_eq_primitive(args, nc))
        .or_else(|| compound_primitive(args, nc));
    if r.is_none() && std::env::var("SH2_TL_DEBUG").is_ok() {
        eprintln!("[tl] refuse: no arm :: {args:?}");
    }
    r
}

/// `A -a B` / `A -o B` (EXACTLY one top-level connective) →
/// `And(lift(A), lift(B))` / `Or(…)` where each half lifts through the
/// same four arms.
///
/// ## Need
/// The runtime test-string path re-tokenizes the whole text per
/// evaluation for every backend that has no flat-test reader. The
/// corpus's compound sites are the `[[ … -a … ]]` idioms
/// (`$n -gt 3 -a $l != "c"`, `"$m"=="test" -o "$m"=="debug"`,
/// `[ -n "$v" -a -z "$o" ]`) — each half is already a liftable shape,
/// but the connective kept the WHOLE text on the runtime path.
///
/// ## Sound rule — refuse > guess
/// - Exactly ONE top-level `-a`/`-o` connective. Two connectives
///   (`A -o B -a C`) mix POSIX `[ ]` precedence (-a binds tighter) with
///   `[[ ]]` left-to-right evaluation — refuse rather than pick.
/// - The connective must sit OUTSIDE quotes (`[ "a -o b" = x ]` is a
///   single-operand test whose literal contains the token — the scan is
///   quote-aware).
/// - Any `(` / `)` / `\\` in the text refuses: grouping and escapes go
///   through the runtime tokenizer (parse-complex-test-parens,
///   test-expression-backslash-continuation).
/// - Each half must lift through the SAME arms (so `-f "$file"`,
///   positional `$2`, `${v:0:1}` halves keep the whole text on the
///   runtime path — 063_16 / 064_hard_to_generate stay untouched).
/// - The halves are lifted with lower_test_leaf itself, so the
///   nocasematch gate, operand() checks and every refusal carry over
///   per-half; the result is an And/Or BinOp leaf that writes NO status
///   (the caller's observable-status guard already covers the leaf
///   replacement, same as the other arms).
fn compound_primitive(args: &[IrExpr], nc: bool) -> Option<IrExpr> {
    let text = match args.first() {
        Some(IrExpr::Str(s, _)) => s.as_str(),
        _ => return None,
    };
    let text = text.trim();
    if text.contains('(') || text.contains(')') || text.contains('\\') {
        return None; // grouping / escapes — the runtime tokenizer's job
    }
    let (lhs, op, rhs) = split_compound(text)?;
    // Synthetic leaf args — the arms read args[0] only (the style tag is
    // ignored), and each half is strictly shorter than the whole, so the
    // recursion terminates (a half cannot contain the ONLY connective).
    let tag = args.get(1).cloned().unwrap_or(IrExpr::Bool(true));
    let l = lower_test_leaf(&[IrExpr::Str(lhs.to_string(), StrStyle::DoubleQuoted), tag.clone()], nc)?;
    let r = lower_test_leaf(&[IrExpr::Str(rhs.to_string(), StrStyle::DoubleQuoted), tag], nc)?;
    Some(IrExpr::BinOp {
        op,
        lhs: Box::new(l),
        rhs: Box::new(r),
    })
}

/// Split on the text's ONE top-level ` -a ` / ` -o ` connective
/// (quote-aware; None unless exactly one occurs outside quotes).
fn split_compound(text: &str) -> Option<(&str, BinOpKind, &str)> {
    let b = text.as_bytes();
    let mut in_dq = false;
    let mut in_sq = false;
    let mut found: Option<(usize, BinOpKind, usize)> = None; // (start, op, end)
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'"' if !in_sq => {
                in_dq = !in_dq;
                i += 1; // toggle WITHOUT advancing = spin forever at
                // this byte (the compiler can't see it: a quoted text
                // like `-n "${v:-x}"` hung every entry to this arm —
                // measured: hangprobe SIGTERM at stage `test_lowering`)
            }
            b'\'' if !in_dq => {
                in_sq = !in_sq;
                i += 1;
            }
            b'-' if !in_dq && !in_sq && i + 3 < b.len() => {
                let op = match (b[i + 1], b[i + 2]) {
                    (b'a', b' ') => BinOpKind::And,
                    (b'o', b' ') => BinOpKind::Or,
                    _ => {
                        i += 1;
                        continue;
                    }
                };
                // Must be delimited on the left by whitespace (or start).
                if i > 0 && !b[i - 1].is_ascii_whitespace() {
                    i += 1;
                    continue;
                }
                if found.is_some() {
                    return None; // two connectives — precedence ambiguous
                }
                found = Some((i, op, i + 3));
                i += 3;
            }
            _ => i += 1,
        }
    }
    let (start, op, end) = found?;
    let lhs = text[..start].trim();
    let rhs = text[end..].trim();
    if lhs.is_empty() || rhs.is_empty() {
        return None; // missing-operand forms — bash's error path
    }
    Some((lhs, op, rhs))
}

/// The canonical `Not` wrapper: the operand on BOTH sides.
///
/// The desugar's convention ("`until cond` wraps the cond in
/// `BinOp(Not, cond, cond)`") — every renderer accepts it, because most
/// read the LHS (estree/c/go/rust/zig/python: `(!{lhs})`) while the
/// active perl emitter reads the RHS (`!({rhs})`, `ir_expr_to_perl`). A
/// Not whose rhs is a placeholder renders as that placeholder in PERL:
/// the glob arm used to emit `Not { lhs: prim, rhs: Bool(false) }` and
/// perl produced `if (!(0))` — an ALWAYS-true condition (measured:
/// `[[ "$s" != "abc"* ]]` printed `BAD` where bash printed `GOOD`).
/// Duplicating the operand satisfies both conventions.
pub(crate) fn not_wrap(inner: IrExpr) -> IrExpr {
    IrExpr::BinOp {
        op: BinOpKind::Not,
        lhs: Box::new(inner.clone()),
        rhs: Box::new(inner),
    }
}

/// `-n X` / `-z X` (exactly one operand) → `X ne ""` / `Not(X ne "")`.
///
/// ## Need
/// `-n`/`-z` is the corpus's dominant non-numeric test shape (the
/// `tty-cmdsub` / `064_*` conditionals), and the runtime/test-string
/// path re-parses the text on every evaluation — the perl emitter
/// renders `length($x) ne 0` for it, the other backends go through
/// `sh2.test` dispatch.
///
/// ## Sound rule — refuse > guess
/// Exactly one whitespace-separated operand after the flag; the operand
/// must be a plain-var read / signed literal / `$((…))`-free shape the
/// shared `operand()` accepts, or a double-/single-quoted EMPTY string
/// (`-n ""` → `Str("")` — `operand()` refuses empty literals, so the
/// empty shapes are matched before it runs). Unquoted-empty (bash
/// one-arg error form), multi-word operands (bash error form),
/// positional `$1`-style names and anything with expansion chars stay
/// on the runtime path. nocasematch does not matter: length comparison
/// is case-independent.
fn nonempty_primitive(args: &[IrExpr]) -> Option<IrExpr> {
    let text = match args.first() {
        Some(IrExpr::Str(s, _)) => s.as_str(),
        _ => return None,
    };
    let text = text.trim();
    let (negate, rest) = if let Some(r) = text.strip_prefix("-n ") {
        (false, r)
    } else if let Some(r) = text.strip_prefix("-z ") {
        (true, r)
    } else {
        return None;
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return None; // `test -n` alone — bash's missing-operand error
    }
    // An unquoted `$…` operand can word-split into the too-many-args
    // error form under POSIX `[ ]` — refuse it for both styles rather
    // than guess (the corpus's dominant shape is the quoted `"$v"`).
    if rest.starts_with('$') {
        return None;
    }
    let op = if rest == "\"\"" || rest == "''" {
        IrExpr::Str(String::new(), StrStyle::DoubleQuoted)
    } else {
        operand(rest)?
    };
    let ne = IrExpr::BinOp {
        op: BinOpKind::Ne,
        lhs: Box::new(op),
        rhs: Box::new(IrExpr::Str(String::new(), StrStyle::DoubleQuoted)),
    };
    Some(if negate { not_wrap(ne) } else { ne })
}

/// `A == B` / `A = B` / `A != B` (plain-var or literal operands) → the
/// STRING compare `Not(L ne R)` / `L ne R`.
///
/// ## Need
/// Exact string equality is the second big test class left on the
/// runtime path (`test-literal-string-compare`, `064_*`, the `[ x = y ]
///` POSIX form). A typed leaf renders natively everywhere and lets the
/// constant-fold / condition analyses see the comparison.
///
/// ## Why Not(ne) for `==` instead of a typed Eq
/// `ir_expr_to_perl` maps `BinOpKind::Eq` to perl's ` == ` — a NUMERIC
/// compare (`'abc' == 'x'` is TRUE in perl: both numify to 0). `Ne`
/// maps to ` ne ` (string) and every other backend maps `Ne` to its
/// string-capable inequality (`!==`, `!=`, `==` on python strings), so
/// equality is expressed as the negation of `Ne` via `not_wrap` — the
/// canonical both-sides form every renderer accepts.
///
/// ## Sound rule — refuse > guess
/// The text is read exactly like the runtime's own flat-test reader
/// (`render_test_expr` / `render_flat_test` in ir.rs): only a text with
/// NO WHITESPACE takes the flat split (spaced texts — `-a`/`-o`
/// compounds, `-ge` numeric ops — go through the runtime's tokenizer and
/// are refused here), and the operator scan is `==`, `=~`, `!=`, `=` in
/// that order — the first PRESENT op wins at its first position. `=~`
/// refuses (regex is the runtime's business). Each side must then pass
/// the shared `operand()` (plain var read or a literal without
/// expansion/glob/space chars — affix globs are the glob arm's business
/// and are tried FIRST, other globs such as `a?b` fall out of
/// `operand()`). Matching the runtime's READING is what makes the lift
/// sound at the text level: the flattened text cannot distinguish a
/// comparison from a one-arg test whose single word contains `=`
/// (`[ a=b ]` vs `[ a = b ]` both serialize to `a=b`, and the runtime
/// parses BOTH as the comparison — the one-arg divergence is
/// pre-existing: bash prints `T1`, the transpiled perl prints `F1`, and
/// this lift changes neither). Guarded (caller, via `nc`) on the program
/// not mentioning nocasematch: `[[ ]]` string matching honors that
/// option dynamically, and the typed `ne` is case-sensitive (the
/// runtime reader wraps `($__nocasematch) ? lc… : …` unconditionally).
fn string_eq_primitive(args: &[IrExpr], nc: bool) -> Option<IrExpr> {
    if nc {
        return None;
    }
    let text = match args.first() {
        Some(IrExpr::Str(s, _)) => s.as_str(),
        _ => return None,
    };
    let text = text.trim();
    if text.chars().any(|c| c.is_whitespace()) {
        return None; // spaced texts → the runtime tokenizer path
    }
    let (op, is_ne) = if text.contains("==") {
        ("==", false)
    } else if text.contains("=~") {
        return None; // regex match — never lowered here
    } else if text.contains("!=") {
        ("!=", true)
    } else if text.contains('=') {
        ("=", false)
    } else {
        return None;
    };
    let pos = text.find(op)?;
    let l_raw = &text[..pos];
    let r_raw = &text[pos + op.len()..];
    // An rhs that references a VARIABLE stays on the runtime probe.
    // Two reasons, both refuse>guess: (1) an UNQUOTED `$…` rhs is a
    // PATTERN in `[[ ]]` — `pat='a*'; [[ "abc" != $pat ]]` is a glob
    // match and the rhs VALUE may carry metachars no static reader can
    // see; (2) an UNQUOTED rhs keeps glob capability downstream — the go-sh
    // frontend's `strings.Contains` → `"$s"=*world*` lowering relies on it
    // (rust's `glob_rhs_probe_is_quoting_gated`), while a QUOTED rhs is a
    // literal compare in the runtime reader AND in rust (the evalTest
    // `rQuoted` rule; measured: bash F2/F1/F3 and the probe path both print
    // F for `pat='a*'; [ "abc" = "$pat" ]`). Refusing here costs nothing
    // in equivalence: the raw `test(...)` path honours both rules.
    if r_raw.contains('$') {
        return None;
    }
    // `~` renders as $HOME (or a HOME path) in the runtime's flat reader
    // — a literal `Str("~")` here would compare the tilde character.
    if l_raw.contains('~') || r_raw.contains('~') {
        return None;
    }
    // A PATTERN is not a string. `[[ a = b ]]`'s rhs (and a `!=`'s) is
    // a GLOB/extglob pattern whenever the source carried unquoted
    // metachars — `@(foo|bar)`, `*x*`, `?`, `[ab]`, `{a,b}` — and
    // lowering it to a string `ne` silently changes the match into a
    // literal compare (measured regression: at-in-test.sh's
    // `[[ "$x" = @(foo|bar) ]]` stopped printing `matched`, because
    // perl compared against the 10-character text `@(foo|bar)`). Refuse
    // every pattern metachar on either side; under-lifting keeps the
    // runtime test path, which IS glob- (and nocasematch-) aware. The
    // refusal also covers a metachar QUOTED in the source (`"a*b"` is
    // a literal to bash) — that under-lift is free, and refusing beats
    // reconstructing per-quote-style meaning here.
    const GLOB_METACHARS: [char; 12] = [
        '*', '?', '[', ']', '(', ')', '{', '}', '|', '@', '+', '!',
    ];
    if l_raw.chars().any(|c| GLOB_METACHARS.contains(&c))
        || r_raw.chars().any(|c| GLOB_METACHARS.contains(&c))
    {
        return None;
    }
    // CONSTANT-CONSTANT belongs to the backends' own const-string-test
    // specialization, not here. rust_backend folds `[ "a" = "a" ]` at
    // render time: at -Og it renders the literal `"a" == "a"` (no owned
    // String), at -Os it elides the whole `if` — pinned by
    // `const_string_test_folds_only_with_size_opt`. Lifting the shape
    // here to `Not(Ne)` would pre-empt that path with a form its folder
    // cannot see (measured: the -Og output became
    // `!("a".to_string() != "a".to_string())` and the -Os elision never
    // fired). With the rhs-`$` refusal above, the ONLY shape this arm
    // uniquely serves is a variable-reading lhs — keep exactly that.
    if !l_raw.contains('$') {
        return None;
    }
    let l = operand(l_raw)?;
    let r = operand(r_raw)?;
    let ne = IrExpr::BinOp {
        op: BinOpKind::Ne,
        lhs: Box::new(l),
        rhs: Box::new(r),
    };
    Some(if is_ne { ne } else { not_wrap(ne) })
}

/// `A -op B` (op ∈ `-eq -ne -gt -ge -lt -le`) with plain-var or
/// integer-literal operands → a typed numeric `BinOp` leaf.
///
/// ## Need
/// The test text stays a STRING operand, so every backend re-parses it at
/// render time: rust emits `i.to_string().trim().parse::<i64>().unwrap_or(0)
/// >= 3` for a variable ITSELF keeps as a native `i64` (`let mut i = 0;`),
/// C re-atolls, JS re-Numbers (measured 2026-10-19 on tdw.sh). A typed
/// BinOp renders natively in every backend (`i >= 3`) — fewer string
/// round-trips, and the arith analyses (interval / i32-provable / const
/// folding) can see the comparison afterwards.
///
/// ## Sound rule — refuse > guess
/// Both operands must be `$name` / `${name}` / `"$name"` (a plain
/// identifier read — the same value the runtime test would expand), a
/// signed integer literal, or a `$((…))` arith expansion (a NUMBER by
/// construction — its ArithAst is accepted only via `parse_arith_native`,
/// the same "every backend renders this natively" predicate `(( ))`
/// statements are gated on). `=`/`!=` are STRING compares in bash and are
/// never rewritten; `${x:1}`, globs, arithmetic-operator operands,
/// multiple operator occurrences, an unparseable/`non-native` arith
/// expansion — all refused. The rewrite is additionally gated (in
/// `rewrite_cond`) on an unobservable status: a `test` call records a
/// status write, a bare BinOp leaf does not.
fn numeric_compare_primitive(args: &[IrExpr]) -> Option<IrExpr> {
    use BinOpKind::{Eq, Ge, Gt, Le, Lt, Ne};
    let text = match args.first() {
        Some(IrExpr::Str(s, _)) => s.as_str(),
        _ => return None,
    };
    let text = text.trim();
    // The operator is a whitespace-delimited token between the operands.
    // Operands are single tokens (no spaces accepted below), so at most
    // one occurrence is unambiguous; anything else is refused.
    let mut found: Option<(&str, BinOpKind, &str)> = None;
    for (tok, op) in [
        (" -eq ", Eq),
        (" -ne ", Ne),
        (" -gt ", Gt),
        (" -ge ", Ge),
        (" -lt ", Lt),
        (" -le ", Le),
    ] {
        if let Some(idx) = text.find(tok) {
            if found.is_some() {
                return None;
            }
            found = Some((&text[..idx], op, &text[idx + tok.len()..]));
        }
    }
    let (lhs, op, rhs) = found?;
    let lhs = numeric_operand(lhs)?;
    let rhs = numeric_operand(rhs)?;
    Some(IrExpr::BinOp {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    })
}

/// A numeric-compare operand: a plain var read or a signed integer
/// literal (both wrapped in optional double quotes). The result is
/// wrapped in an `Arith` node — `-lt` is an ARITHMETIC compare in bash,
/// and the Arith node is what every backend renders numerically. A bare
/// `Var`/`Var` BinOp is a STRING compare in some backends (measured
/// 2026-10-19: rust emitted `i.clone() < len` — lexicographic "2" <
/// "10" is false, so 010_substring_loop stopped after two characters).
fn numeric_operand(e: &str) -> Option<IrExpr> {
    let e = e.trim();
    let inner = e
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(e);
    if inner.is_empty() {
        return None;
    }
    // `$((…))` — an arith expansion yields a NUMBER by construction, so
    // the compare is numeric exactly like the var/literal shapes below.
    // `parse_arith_native` is the shared "every backend renders this
    // natively" gate (same as `(( ))` statements); an expansion it
    // refuses (or a malformed/empty one) stays on the runtime test
    // path. Spaces inside the expansion are fine: the op-split above
    // already consumed them as part of this operand, and arith syntax
    // can never contain a whitespace-delimited ` -eq `-style token.
    if let Some(a) = inner
        .strip_prefix('$')
        .and_then(|x| x.strip_prefix("(("))
        .and_then(|x| x.strip_suffix(")"))
        .and_then(|x| x.strip_suffix(')'))
    {
        let ast = crate::shir::parse_arith_native(a)?;
        return Some(IrExpr::Arith(Box::new(ast)));
    }
    if let Some(name) = inner.strip_prefix('$') {
        // `${name}` and `$name` are the same read.
        let name = name
            .strip_prefix('{')
            .and_then(|n| n.strip_suffix('}'))
            .unwrap_or(name);
        if is_plain_ident(name) {
            return Some(IrExpr::Arith(Box::new(crate::ir::ArithAst::Var(
                name.to_string(),
            ))));
        }
        return None;
    }
    // A signed integer literal (`3`, `-5`, `+7`). The magnitude bound
    // keeps the literal inside i64 (and inside what the numeric backends
    // parse exactly); anything else — `1e3`, `0x10`, `1.5`, `1+1` — is
    // refused (bash's arithmetic context would evaluate those
    // differently from a plain integer parse).
    let digits = inner.strip_prefix(['-', '+']).unwrap_or(inner);
    if !digits.is_empty()
        && digits.len() <= 18
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        return Some(IrExpr::Arith(Box::new(crate::ir::ArithAst::Num(
            inner.parse::<i64>().ok()?,
        ))));
    }
    None
}

/// The glob-affix shapes → the primitive call (or None). The test text is
/// args[0] (a Str); args[1] is the `[[` style tag (ignored). Gated on
/// `nc` (guard 3): nocasematch makes `[[ == ]]`-style glob matching
/// case-INsensitive dynamically, and `strHasPrefix`/`contains` are
/// case-sensitive — a program that mentions nocasematch keeps these on
/// the runtime test path (which renders the
/// `($__nocasematch) ? lc… eq lc… : …` wrapper).
fn glob_affix_primitive(args: &[IrExpr], nc: bool) -> Option<IrExpr> {
    if nc {
        return None;
    }
    let text = match args.first() {
        Some(IrExpr::Str(s, _)) => s.as_str(),
        _ => return None,
    };
    let text = text.trim();
    // `"$s"==*"$p"*` — the operator splits the text into lhs/rhs. The
    // runtime's test-string grammar: quoted `$var` operands, glob
    // metachars outside quotes.
    let (_lhs, _rhs, _negate) = if let Some((l, r)) = text.split_once("==") {
        (l, r, false)
    } else if let Some((l, r)) = text.split_once("!=") {
        (l, r, true)
    } else {
        return None;
    };
    let (lhs, rhs, negate) = if let Some((l, r)) = text.split_once("==") {
        (l, r, false)
    } else if let Some((l, r)) = text.split_once("!=") {
        (l, r, true)
    } else {
        return None;
    };
    let s = operand(lhs)?;
    // Classify the pattern side: `*P*` (substr), `*P` (suffix), `P*`
    // (prefix) — the star-stripping order matters (`*"$p"*` must be
    // seen as a SUBSTRING before the prefix scan sees `*"$p"`). The
    // pattern text between the stars must be a plain operand (a var or a
    // literal).
    let rhs = rhs.trim();
    let prim = if let Some(inner) = rhs.strip_prefix('*') {
        if let Some(inner) = inner.strip_suffix('*') {
            // `*P*` — substring test
            if inner.is_empty() {
                return None; // `**` — matches anything, not an affix
            }
            ("contains", inner)
        } else {
            // `*P` — suffix test
            if inner.is_empty() {
                return None; // bare `*` — matches anything
            }
            ("strHasSuffix", inner)
        }
    } else if let Some(rest) = rhs.strip_suffix('*') {
        // `P*` — prefix test
        if rest.is_empty() {
            return None; // bare `*` — matches anything
        }
        ("strHasPrefix", rest)
    } else {
        return None; // exact equality is not a glob-affix shape
    };
    let (prim_name, pat) = prim;
    // NOTE: no self-recursion guard here. The polyfill's own primitive
    // bodies (`contains`'s `[[ "$h" == *"$n"* ]]`) lower to the native
    // call — every backend renders `contains`/`strHasPrefix`/
    // `strHasSuffix` NATIVELY (estree `.includes`/`.startsWith`/
    // `.endsWith`, perl `index >= 0`, C `strstr`, go `strings.Contains`,
    // python `in`, zig `std.mem.indexOf`), so the transpiled body never
    // dispatches through the runtime's sh2.* namespace — no recursion.
    // (The old guard refused this lowering, leaving the primitive bodies
    // on the slow `sh2.test` parser path — contains 0.28% of hand-written
    // speed; relaxing it is ~60-80x.)
    // The pattern operand: a var read or a literal. `"$p"` → Var(p);
    // `"/"` → Str("/").
    let pat_expr = operand(pat)?;
    let call = IrExpr::Call {
        func: prim_name.to_string(),
        args: vec![s, pat_expr],
    };
    if negate {
        // `not_wrap` — the canonical both-sides Not. The old shape
        // (`lhs: call, rhs: Bool(false)`) rendered as `!(0)` in perl
        // (which reads rhs) — an always-true branch, i.e. a latent
        // equivalence bug on every `!=`-affix site the corpus never
        // exercised (measured: `[[ "$s" != "abc"* ]]` printed BAD).
        Some(not_wrap(call))
    } else {
        Some(call)
    }
}

/// A test operand: `$name` / `"${name}"` (a plain identifier read) or a
/// literal with no expansion chars (`/`). Returns the IrExpr for the
/// operand's VALUE.
fn operand(e: &str) -> Option<IrExpr> {
    let e = e.trim();
    let inner = e
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(e);
    if let Some(name) = inner.strip_prefix('$') {
        // `${name}` and `$name` are the same read. The numeric arm
        // (numeric_operand) has always accepted both spellings, but
        // this shared STRING operand refused the braced one — which
        // left the corpus's `[ -n "${VAR}" ]` sites on the runtime
        // test-string path (measured 2026-10-20: 10 remaining `test`
        // call sites across 7 examples, the `-n "${so}"`/`-n "${se}"`
        // family in id-cmdsub/readonly-cmdsub/tty-cmdsub and the
        // `${VAR}`/`${iface}`/`${INTERPRETER_UNDER_TEST}` singletons).
        // Only a PLAIN identifier inside the braces is accepted:
        // `${v:0:1}`, `${#v}`, `${v:-x}` still refuse via
        // is_plain_ident (pinned by non_affix_and_complex_operands_refuse).
        let name = name
            .strip_prefix('{')
            .and_then(|n| n.strip_suffix('}'))
            .unwrap_or(name);
        if is_plain_ident(name) {
            return Some(IrExpr::Var(name.to_string(), None));
        }
        return None;
    }
    // A literal operand: no expansion chars, no glob metachars (a literal
    // `/` in `== */` / `== /*`).
    if !inner.is_empty()
        && !inner
            .chars()
            .any(|c| matches!(c, '$' | '*' | '?' | '[' | ']' | '\\' | '"' | '\'' | ' ' | '\t'))
    {
        return Some(IrExpr::Str(inner.to_string(), StrStyle::DoubleQuoted));
    }
    None
}

/// A shell variable name: `[A-Za-z_][A-Za-z0-9_]*`.
fn is_plain_ident(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::commands::parse_commands_from_text;
    use crate::shir::ast_to_ir_raw;
    use crate::shir_json::shir_to_shir_json;

    fn lower(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(transform(&mut prog.stmts), "transform was a no-op for {src}");
        shir_to_shir_json(&prog)
    }

    fn lower_raw(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        transform(&mut prog.stmts);
        shir_to_shir_json(&prog)
    }

    #[test]
    fn glob_affix_tests_lower_to_primitives() {
        // `[[ "$s" == "$p"* ]]` → strHasPrefix(s, p)
        let json = lower(
            "f() { local s=\"$1\" p=\"$2\"; if [[ \"$s\" == \"$p\"* ]]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json.contains("\"strHasPrefix\""), "missing strHasPrefix: {json}");
        assert!(!json.contains("\"test\""), "test call survived: {json}");
        // `[[ "$s" == *"$p"* ]]` → contains(s, p)
        let json2 = lower(
            "f() { local s=\"$1\" p=\"$2\"; if [[ \"$s\" == *\"$p\"* ]]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json2.contains("\"contains\""), "missing contains: {json2}");
        // `[[ "$s" == *"$p" ]]` → strHasSuffix(s, p)
        let json3 = lower(
            "f() { local s=\"$1\" p=\"$2\"; if [[ \"$s\" == *\"$p\" ]]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json3.contains("\"strHasSuffix\""), "missing strHasSuffix: {json3}");
        // `[[ "$s" == */ ]]` → strHasSuffix(s, "/")
        let json4 = lower(
            "f() { local s=\"$1\"; if [[ \"$s\" == */ ]]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json4.contains("\"strHasSuffix\""), "missing strHasSuffix: {json4}");
    }

    #[test]
    fn self_recursion_guard_keeps_primitive_bodies() {
        // The polyfill's own `contains` body lowers to the NATIVE call
        // (every backend renders contains/strHasPrefix/strHasSuffix
        // natively — no runtime dispatch, no recursion). The old guard
        // refused this, leaving the primitive bodies on the slow
        // `sh2.test` parser path.
        let json = lower_raw(
            "contains() { local h=\"$1\" n=\"$2\"; if [[ \"$h\" == *\"$n\"* ]]; then echo 1; else echo 0; fi; }; contains a b",
        );
        assert!(
            json.contains("\"func\":\"contains\""),
            "primitive body must lower to the native contains: {json}"
        );
        assert!(!json.contains("\"test\""), "test call survived: {json}");
        // A DIFFERENT function's same-shaped test lowers too.
        let json2 = lower(
            "g() { local h=\"$1\" n=\"$2\"; if [[ \"$h\" == *\"$n\"* ]]; then echo 1; else echo 0; fi; }; g a b",
        );
        assert!(json2.contains("\"contains\""), "missing contains: {json2}");
    }

    #[test]
    fn live_status_keeps_the_test() {
        // The cond's status is observable when the run arm has no writer
        // and the consumer reads it (the if is the function's last
        // statement and the arm is a bare assignment — the assignment's
        // status flows from the cond). REFUSE > GUESS: stay a test call.
        let json = lower_raw(
            "f() { local s=\"$1\" p=\"$2\"; if [[ \"$s\" == *\"$p\"* ]]; then x=1; fi; }; f",
        );
        assert!(
            json.contains("\"test\""),
            "live-status cond must stay a test call: {json}"
        );
    }

    #[test]
    fn numeric_compare_tests_lower_to_binop() {
        // `-ge` with a plain var and an integer literal → a typed BinOp
        // (rendered natively by every backend — no test-string re-parse).
        let json = lower(
            "f() { local i=\"$1\"; if [ \"$i\" -ge 3 ]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json.contains("\"op\":\"Ge\""), "missing Ge BinOp: {json}");
        assert!(!json.contains("\"test\""), "test call survived: {json}");
        // `${v}` and `$v` are the same read; `-lt` maps to Lt.
        let json2 = lower(
            "f() { local n=\"$1\"; if [ \"${n}\" -lt 10 ]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json2.contains("\"op\":\"Lt\""), "missing Lt BinOp: {json2}");
        // Signed literal operands compare numerically: -5 -eq -5.
        let json3 = lower(
            "f() { if [ -5 -eq \"-5\" ]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json3.contains("\"op\":\"Eq\""), "missing Eq BinOp: {json3}");
        assert!(!json3.contains("\"test\""), "test call survived: {json3}");
    }

    #[test]
    fn string_eq_lowers_string_ne_and_non_integer_operands_refuse_numeric() {
        // `=`/`!=` are STRING compares in bash — never the numeric `Eq`.
        // With the string-equality arm they lower to the STRING `Ne`
        // (`[ "$s" = 3 ]` → `Not(s ne "3")`, rendered as a string
        // compare by every backend — perl's numeric `==` would be wrong:
        // `'3' == '3.0'` style coercion is not bash's `=`).
        let json = lower(
            "f() { local s=\"$1\"; if [ \"$s\" = 3 ]; then echo 1; fi; }; f",
        );
        assert!(!json.contains("\"test\""), "string eq kept its test: {json}");
        assert!(json.contains("\"op\":\"Ne\""), "missing Ne BinOp: {json}");
        assert!(json.contains("\"op\":\"Not\""), "missing Not wrapper: {json}");
        // A float literal is refused (bash arithmetic would evaluate it;
        // a plain integer parse would not).
        let json2 = lower_raw(
            "f() { local s=\"$1\"; if [ \"$s\" -ge 3.5 ]; then echo 1; fi; }; f",
        );
        assert!(json2.contains("\"test\""), "float literal lost its test: {json2}");
        // A NON-arith-shaped expansion operand on an otherwise-valid
        // pair still refuses when the expansion cannot be parsed
        // natively (refuse > guess).
        let json3 = lower_raw(
            "f() { local s=\"$1\"; if [ \"$s\" -ge $(( )) ]; then echo 1; fi; }; f",
        );
        assert!(json3.contains("\"test\""), "empty arith lost its test: {json3}");
    }

    #[test]
    fn arith_expansion_operands_lower_to_binop() {
        // `$((E))` is a NUMBER by construction — the compare is numeric
        // exactly like the plain-var/literal shapes, so it lowers to a
        // typed BinOp (rendered natively by every backend — no test
        // string re-parse, no sh2.test callout).
        let json = lower(
            "f() { local n=\"$1\"; if [ $((n % 2)) -eq 0 ]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json.contains("\"op\":\"Eq\""), "missing Eq BinOp: {json}");
        assert!(!json.contains("\"test\""), "test call survived: {json}");
        // rhs arith + a plain-var lhs, with spaces INSIDE the expansion
        // (the op-split consumes them as part of the operand).
        let json2 = lower(
            "f() { local n=\"$1\" m=\"$2\"; if [ \"$n\" -lt $(( m + 1 )) ]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json2.contains("\"op\":\"Lt\""), "missing Lt BinOp: {json2}");
        assert!(!json2.contains("\"test\""), "test call survived: {json2}");
        // Both sides arith.
        let json3 = lower(
            "f() { local n=\"$1\" d=\"$2\"; if [ $((d * d)) -gt $((n)) ]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json3.contains("\"op\":\"Gt\""), "missing Gt BinOp: {json3}");
        assert!(!json3.contains("\"test\""), "test call survived: {json3}");
        // A live status still refuses (the BinOp leaf writes no status).
        let json4 = lower_raw(
            "f() { local n=\"$1\"; if [ $((n % 2)) -eq 0 ]; then x=1; fi; }; f",
        );
        assert!(
            json4.contains("\"test\""),
            "live-status arith cond must stay a test call: {json4}"
        );
    }

    #[test]
    fn live_status_keeps_the_numeric_test() {
        // A live status refuses the numeric rewrite too: the test call
        // records a status write, a bare BinOp leaf does not.
        let json = lower_raw(
            "f() { local i=\"$1\"; if [ \"$i\" -ge 3 ]; then x=1; fi; }; f",
        );
        assert!(
            json.contains("\"test\""),
            "live-status cond must stay a test call: {json}"
        );
    }

    #[test]
    fn non_affix_and_complex_operands_refuse() {
        // String ordering (`<`) is not an affix shape and has no typed
        // arm — it stays a runtime test (locale collation is the
        // runtime's business).
        let json = lower_raw(
            "f() { local s=\"$1\" p=\"$2\"; if [[ \"$s\" < \"$p\" ]]; then echo 1; fi; }; f",
        );
        assert!(json.contains("\"test\""), "ordering test lost: {json}");
        // A param-slice operand (`${v:0:1}`) is not a plain var.
        let json2 = lower_raw(
            "f() { local s=\"$1\"; if [[ \"$s\" == \"${v:0:1}\"* ]]; then echo 1; fi; }; f",
        );
        assert!(json2.contains("\"test\""), "slice-operand test lost: {json2}");
    }

    #[test]
    fn nonempty_tests_lower_to_string_ne() {
        // `-n "$a"` → `a ne ""` — a typed leaf (no test-string
        // re-parse); `-z` is its negation via the canonical Not.
        let json = lower(
            "f() { local a=\"$1\"; if [[ -n \"$a\" ]]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json.contains("\"op\":\"Ne\""), "missing Ne BinOp: {json}");
        assert!(!json.contains("\"test\""), "test call survived: {json}");
        let json2 = lower(
            "f() { local a=\"$1\"; if [[ -z \"$a\" ]]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json2.contains("\"op\":\"Not\""), "missing Not wrapper: {json2}");
        assert!(json2.contains("\"op\":\"Ne\""), "missing Ne BinOp: {json2}");
        assert!(!json2.contains("\"test\""), "test call survived: {json2}");
        // The quoted-empty operand lowers (`-n ""` is a false constant
        // fold away), and a literal operand too.
        let json3 = lower(
            "f() { if [[ -n \"\" ]]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(!json3.contains("\"test\""), "empty literal kept its test: {json3}");
    }

    #[test]
    fn string_eq_tests_lower_to_string_ne() {
        // `==` → Not(L ne R): the canonical both-sides Not, because
        // perl renders `Eq` as NUMERIC `==` (`'abc' == 'x'` is true)
        // while `Ne` is ` ne ` (string) everywhere.
        let json = lower(
            "f() { local s=\"$1\"; if [[ \"$s\" == \"x\" ]]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json.contains("\"op\":\"Not\""), "missing Not wrapper: {json}");
        assert!(json.contains("\"op\":\"Ne\""), "missing Ne BinOp: {json}");
        assert!(!json.contains("\"test\""), "test call survived: {json}");
        // `!=` → Ne directly.
        let json2 = lower(
            "f() { local s=\"$1\"; if [[ \"$s\" != \"x\" ]]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json2.contains("\"op\":\"Ne\""), "missing Ne BinOp: {json2}");
        assert!(!json2.contains("\"op\":\"Not\""), "unexpected Not: {json2}");
        assert!(!json2.contains("\"test\""), "test call survived: {json2}");
        // The POSIX single-`=` form.
        let json3 = lower(
            "f() { local s=\"$1\"; if [ \"$s\" = \"x\" ]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json3.contains("\"op\":\"Not\""), "missing Not wrapper: {json3}");
        assert!(!json3.contains("\"test\""), "test call survived: {json3}");
        // Exact equality whose operand is not a plain shape refuses.
        let json4 = lower_raw(
            "f() { local s=\"$1\"; if [[ \"$s\" == \"${v:0:1}\" ]]; then echo 1; fi; }; f",
        );
        assert!(json4.contains("\"test\""), "slice operand lost its test: {json4}");
    }

    #[test]
    fn nocasematch_program_refuses_string_shape_lifts() {
        // `shopt nocasematch` makes `[[ ]]` string/glob matching
        // case-INsensitive dynamically; the typed `ne`/`strHasPrefix`
        // leaves are case-sensitive. Any mention of the option in the
        // program text refuses the string-shape arms (the runtime test
        // path renders the nocasematch-aware wrapper).
        let json = lower_raw(
            "shopt -s nocasematch\nf() { local s=\"$1\"; if [[ \"$s\" == \"x\" ]]; then echo 1; fi; }; f",
        );
        assert!(json.contains("\"test\""), "nocasematch eq must stay a test: {json}");
        let json2 = lower_raw(
            "shopt -s nocasematch\nf() { local s=\"$1\" p=\"$2\"; if [[ \"$s\" == \"$p\"* ]]; then echo 1; fi; }; f",
        );
        assert!(json2.contains("\"test\""), "nocasematch glob must stay a test: {json2}");
        // The numeric arm is case-independent and still lowers.
        let json3 = lower(
            "shopt -s nocasematch\nf() { local i=\"$1\"; if [ \"$i\" -ge 3 ]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json3.contains("\"op\":\"Ge\""), "numeric arm must survive nocasematch: {json3}");
        assert!(!json3.contains("\"test\""), "test call survived: {json3}");
    }

    #[test]
    fn negated_affix_not_carries_primitive_on_both_sides() {
        // Regression for the `!(0)` bug: the Not must carry the
        // primitive on BOTH sides (the desugar/`until` convention every
        // renderer satisfies — estree/c/go/rust/zig/python read lhs,
        // the active perl emitter reads rhs). A placeholder rhs renders
        // `if (!(0))` in perl — an always-true branch.
        let json = lower(
            "f() { local s=\"$1\" p=\"$2\"; if [[ \"$s\" != \"$p\"* ]]; then echo 1; else echo 0; fi; }; f",
        );
        assert_eq!(
            json.matches("\"strHasPrefix\"").count(),
            2,
            "negation must carry the primitive on both sides: {json}"
        );
    }

    #[test]
    fn nonempty_refuses_error_and_positional_shapes() {
        // The one-arg form (`[[ -n ]]` — a nonempty-literal test on the
        // string "-n" in bash) has no `-n OPERAND` shape → refuse.
        let json = lower_raw("f() { if [[ -n ]]; then echo 1; fi; }; f");
        assert!(json.contains("\"test\""), "one-arg -n lost its test: {json}");
        // Positional `$3` is not a plain identifier for `operand()` —
        // same refusal the affix/numeric arms apply (refuse > guess).
        let json2 = lower_raw("f() { if [[ -n \"$3\" ]]; then echo 1; fi; }; f");
        assert!(json2.contains("\"test\""), "positional operand lost its test: {json2}");
    }

    #[test]
    fn braced_var_operands_lower_like_plain_ones() {
        // `${v}` and `$v` are the same read — the string-shape arms
        // accept the braced spelling exactly like the numeric arm
        // already did (measured gap: `-n "${so}"` stayed a sh2.test
        // callout where `-n "$so"` lifted, 10 corpus sites).
        let json = lower(
            "f() { local v=\"$1\"; if [[ -n \"${v}\" ]]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json.contains("\"op\":\"Ne\""), "missing Ne BinOp: {json}");
        assert!(!json.contains("\"test\""), "test call survived: {json}");
        let json2 = lower(
            "f() { local v=\"$1\"; if [[ -z \"${v}\" ]]; then echo 1; fi; }; f",
        );
        assert!(json2.contains("\"op\":\"Not\""), "missing Not wrapper: {json2}");
        assert!(!json2.contains("\"test\""), "test call survived: {json2}");
        // Glob-affix lhs + pattern in braced form → the primitive.
        let json3 = lower(
            "f() { local s=\"$1\" p=\"$2\"; if [[ \"${s}\" == \"${p}\"* ]]; then echo 1; fi; }; f",
        );
        assert!(json3.contains("\"strHasPrefix\""), "missing strHasPrefix: {json3}");
        assert!(!json3.contains("\"test\""), "test call survived: {json3}");
        // A braced NON-identifier (param expansion) still refuses —
        // the braces are stripped, then the same plain-ident gate as
        // every other arm applies.
        let json4 = lower_raw(
            "f() { local v=\"$1\"; if [[ -n \"${v:-x}\" ]]; then echo 1; fi; }; f");
        assert!(json4.contains("\"test\""), "default-expansion lost its test: {json4}");
        let json5 = lower_raw(
            "f() { if [[ -n \"${#v}\" ]]; then echo 1; fi; }; f");
        assert!(json5.contains("\"test\""), "length-expansion lost its test: {json5}");
        // The string-eq arm's metachar guard still refuses a braced
        // operand (`{`/`}` are glob metachars there — the guard runs
        // before operand(), so a quoted-braced compare stays on the
        // runtime path unchanged).
        let json6 = lower_raw(
            "f() { local v=\"$1\"; if [[ \"${v}\" = \"x\" ]]; then echo 1; fi; }; f");
        assert!(json6.contains("\"test\""), "braced eq must keep its test: {json6}");
    }

    #[test]
    fn two_operand_compounds_lift_both_halves() {
        // `$n -gt 3 -a $l != "c"` — one top-level -a; both halves are
        // liftable shapes -> And(numeric, string-ne). The compound text
        // used to keep the WHOLE condition on the runtime test path.
        let json = lower(
            "f() { local n=\"$1\" l=\"$2\"; if [ \"$n\" -gt 3 -a \"$l\" != \"c\" ]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json.contains("\"op\":\"And\""), "missing And BinOp: {json}");
        assert!(json.contains("\"op\":\"Gt\""), "missing Gt half: {json}");
        assert!(json.contains("\"op\":\"Ne\""), "missing Ne half: {json}");
        assert!(!json.contains("\"test\""), "test call survived: {json}");
        // `[ -n "$v" -a -z "$o" ]` -> And(ne, Not(ne)).
        let json2 = lower(
            "f() { local v=\"$1\" o=\"$2\"; if [ -n \"$v\" -a -z \"$o\" ]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json2.contains("\"op\":\"And\""), "missing And BinOp: {json2}");
        assert!(json2.contains("\"op\":\"Not\""), "missing Not half: {json2}");
        assert!(!json2.contains("\"test\""), "test call survived: {json2}");
        // `[[ "$m" == "test" -o "$m" == "debug" ]]` -> Or of two
        // string-ne halves (rhs literals — the rhs-`$` refusal holds).
        let json3 = lower(
            "f() { local m=\"$1\"; if [[ \"$m\" == \"test\" -o \"$m\" == \"debug\" ]]; then echo 1; else echo 0; fi; }; f",
        );
        assert!(json3.contains("\"op\":\"Or\""), "missing Or BinOp: {json3}");
        // NB: the operand literal is the string "test", so the callout
        // probe must be the func field — `"test"` matches "value":"test".
        assert!(!json3.contains("\"func\":\"test\""), "test call survived: {json3}");
    }

    #[test]
    fn compound_refusals_stay_on_the_runtime_path() {
        // TWO connectives — POSIX -a/-o precedence differs from [[ ]]
        // left-to-right: refuse the whole text.
        let json = lower_raw(
            "f() { local a=\"$1\"; if [ \"$a\" = x -o \"$a\" = y -a \"$a\" = z ]; then echo 1; fi; }; f",
        );
        assert!(json.contains("\"test\""), "two-connective text lost its test: {json}");
        // A half that is not a liftable shape (`-f`) keeps the whole
        // text (063_16's `[ -n "$var" -a -f "$file" -o -d "$dir" ]`).
        let json2 = lower_raw(
            "f() { local v=\"$1\"; if [ -n \"$v\" -a -f /tmp ]; then echo 1; fi; }; f");
        assert!(json2.contains("\"test\""), "-f half lost its test: {json2}");
        // Parens (grouping) stay runtime-tokenized.
        let json3 = lower_raw(
            "f() { local v=\"$1\"; if [ \\( -n \"$v\" \\) -a -d /tmp ]; then echo 1; fi; }; f");
        assert!(json3.contains("\"test\""), "paren compound lost its test: {json3}");
        // The connective INSIDE a quoted operand is not a connective:
        // `[ "a -o b" = x ]` is the one-operand test form (stays).
        let json4 = lower_raw(
            "f() { if [ \"a -o b\" = x ]; then echo 1; fi; }; f");
        assert!(json4.contains("\"test\""), "quoted token split as connective: {json4}");
        // A live status still refuses (the And/Or leaf writes no status).
        let json5 = lower_raw(
            "f() { local a=\"$1\"; if [ \"$a\" = x -o \"$a\" = y ]; then x=1; fi; }; f");
        assert!(json5.contains("\"test\""), "live-status compound kept nothing: {json5}");
        // nocasematch: the string-eq halves refuse, so the compound
        // stays on the runtime path (which is nocasematch-aware).
        let json6 = lower_raw(
            "shopt -s nocasematch\nf() { local m=\"$1\"; if [[ \"$m\" == \"test\" -o \"$m\" == \"debug\" ]]; then echo 1; fi; }; f");
        assert!(json6.contains("\"test\""), "nocasematch compound must stay a test: {json6}");
    }
}
