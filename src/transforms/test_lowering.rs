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
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). Reuses the
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
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt_pass(st, &live, None);
    }
    c
}

/// Walk statements, tracking the enclosing function name (for the
/// self-recursion guard).
fn stmt_pass(st: &mut IrStmt, live: &HashSet<usize>, fname: Option<&str>) -> bool {
    // The statement pointer (the liveness key) is computed BEFORE the
    // mutable field borrows below.
    let self_live = live.contains(&(st as *const IrStmt as usize));
    match st {
        IrStmt::Function { name, body, .. } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt_pass(s, live, Some(name));
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
            let mut c = rewrite_cond(cond, observable, fname);
            for s in then.iter_mut() {
                c |= stmt_pass(s, live, fname);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= rewrite_cond(ec, observable, fname);
                for s in eb.iter_mut() {
                    c |= stmt_pass(s, live, fname);
                }
            }
            for s in else_.iter_mut() {
                c |= stmt_pass(s, live, fname);
            }
            c
        }
        IrStmt::While { cond, body } => {
            let observable = cond_status_observable(body, &[], &[], self_live);
            let mut c = rewrite_cond(cond, observable, fname);
            for s in body.iter_mut() {
                c |= stmt_pass(s, live, fname);
            }
            c
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let observable = cond_status_observable(body, &[], &[], self_live);
            let mut c = rewrite_cond(cond, observable, fname);
            for s in body.iter_mut() {
                c |= stmt_pass(s, live, fname);
            }
            c
        }
        IrStmt::For { body, .. } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt_pass(s, live, fname);
            }
            c
        }
        IrStmt::ForInit { init, cond, step, body } => {
            let observable = cond_status_observable(body, &[], &[], self_live);
            let mut c = rewrite_cond(cond, observable, fname);
            for s in init.iter_mut().chain(step.iter_mut()).chain(body.iter_mut()) {
                c |= stmt_pass(s, live, fname);
            }
            c
        }
        IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt_pass(s, live, fname);
            }
            c
        }
        IrStmt::Redirect { inner, .. } => {
            let mut c = false;
            for s in inner.iter_mut() {
                c |= stmt_pass(s, live, fname);
            }
            c
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for stage in stages.iter_mut() {
                for s in stage.iter_mut() {
                    c |= stmt_pass(s, live, fname);
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
                c |= stmt_pass(s, live, fname);
            }
            for ex in excepts.iter_mut() {
                for s in ex.body.iter_mut() {
                    c |= stmt_pass(s, live, fname);
                }
            }
            c
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
fn rewrite_cond(cond: &mut IrExpr, observable: bool, fname: Option<&str>) -> bool {
    if observable || !crate::shir::is_pure_test_chain(cond) {
        return false;
    }
    rewrite_test_chain(cond, fname)
}

/// Rewrite a pure test chain's glob-affix leaves. Returns whether
/// anything changed.
fn rewrite_test_chain(e: &mut IrExpr, fname: Option<&str>) -> bool {
    match e {
        IrExpr::BinOp { op, lhs, rhs } if matches!(op, BinOpKind::And | BinOpKind::Or) => {
            let l = rewrite_test_chain(lhs, fname);
            let r = rewrite_test_chain(rhs, fname);
            l | r
        }
        IrExpr::BinOp { op: BinOpKind::Not, lhs, .. } => {
            // `!`-negated leaf: rewrite the inner test, wrap in Not.
            if let IrExpr::Call { func, args, .. } = &**lhs {
                if func == "test" {
                    if let Some(prim) = glob_affix_primitive(args, fname) {
                        **lhs = prim;
                        return true;
                    }
                }
            }
            rewrite_test_chain(lhs, fname)
        }
        IrExpr::Call { func, args, .. } if func == "test" => {
            if let Some(prim) = glob_affix_primitive(args, fname) {
                *e = prim;
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// The glob-affix shapes → the primitive call (or None). The test text is
/// args[0] (a Str); args[1] is the `[[` style tag (ignored).
fn glob_affix_primitive(args: &[IrExpr], fname: Option<&str>) -> Option<IrExpr> {
    let text = match args.first() {
        Some(IrExpr::Str(s, _)) => s.as_str(),
        _ => return None,
    };
    let text = text.trim();
    // `"$s"==*"$p"*` — the operator splits the text into lhs/rhs. The
    // runtime's test-string grammar: quoted `$var` operands, glob
    // metachars outside quotes.
    let (lhs, rhs, negate) = if let Some((l, r)) = text.split_once("==") {
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
    // Self-recursion guard: the polyfill's own primitive implementations
    // must not call themselves through the sh2.* namespace (the adapter
    // would recurse infinitely).
    if fname == Some(prim_name) {
        return None;
    }
    // The pattern operand: a var read or a literal. `"$p"` → Var(p);
    // `"/"` → Str("/").
    let pat_expr = operand(pat)?;
    let call = IrExpr::Call {
        func: prim_name.to_string(),
        args: vec![s, pat_expr],
    };
    if negate {
        Some(IrExpr::BinOp {
            op: BinOpKind::Not,
            lhs: Box::new(call),
            rhs: Box::new(IrExpr::Bool(false)),
        })
    } else {
        Some(call)
    }
}

/// A test operand: `$name` / `"$name"` (a plain identifier read) or a
/// literal with no expansion chars (`/`). Returns the IrExpr for the
/// operand's VALUE.
fn operand(e: &str) -> Option<IrExpr> {
    let e = e.trim();
    let inner = e
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(e);
    if let Some(name) = inner.strip_prefix('$') {
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
        // The polyfill's own `contains` body must NOT call `sh2.contains`
        // (the per-backend adapter would recurse infinitely).
        let json = lower_raw(
            "contains() { local h=\"$1\" n=\"$2\"; if [[ \"$h\" == *\"$n\"* ]]; then echo 1; else echo 0; fi; }; contains a b",
        );
        assert!(
            !json.contains("\"func\":\"contains\""),
            "primitive body must stay a test: {json}"
        );
        assert!(json.contains("\"test\""), "test call lost: {json}");
        // But a DIFFERENT function's same-shaped test lowers.
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
    fn non_affix_and_complex_operands_refuse() {
        // Exact equality (`== "$p"`) is not a glob-affix shape.
        let json = lower_raw(
            "f() { local s=\"$1\" p=\"$2\"; if [[ \"$s\" == \"$p\" ]]; then echo 1; fi; }; f",
        );
        assert!(json.contains("\"test\""), "exact test lost: {json}");
        // A param-slice operand (`${v:0:1}`) is not a plain var.
        let json2 = lower_raw(
            "f() { local s=\"$1\"; if [[ \"$s\" == \"${v:0:1}\"* ]]; then echo 1; fi; }; f",
        );
        assert!(json2.contains("\"test\""), "slice-operand test lost: {json2}");
    }
}
