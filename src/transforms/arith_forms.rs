//! arith-forms — rewrite `let "x+=1"` / `let "x++"` execs as structured
//! `IrStmt::Assign` with `IrExpr::Arith` payloads.
//!
//! ## Need
//! `let` in the IR is `exec("let", ["x+=1", ...])` (one arith string per
//! arg). The current renderer path renders it as a side-effect-free test
//! — `x=$((x + 1))` ends up inside a command-substitution subshell, so
//! `x` is never assigned (the `let-plusassign` wrong-answer bug). The
//! natural IR shape for every backend is a structured Assign with an
//! Arith(Assign{compound}) / Arith(IncDec) payload; this transform
//! produces it.
//!
//! ## Scope
//! - `let "x=1"` / `let "x+=1"` / `let "x=$y+1"` → `Assign{var, expr: Arith(Assign{op,rhs})}`
//! - `let "x++"` / `let "++x"` / `let "x--"` → `Assign{var, expr: Arith(IncDec{delta,prefix})}`
//! - Mixed: `let "x=1" "y++"` → two Assign stmts in a Block.
//! - Any arg that doesn't parse as an arith Assign/IncDec (or has a
//!   target the core's `parse_arith` doesn't classify as an Assign/IncDec
//!   node) leaves the whole exec untouched — refuse > guess.
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated, like the
//! rest of the registry). The sh renderer's separate job is to render
//! `Assign{Arith(Assign{compound})}` as a plain `x=$((x + 1))` (and
//! IncDec as `((x++))` / `x=$((x + 1))`); the IR shape is now what
//! the renderer needs to match.

use crate::ir::{ArithAst, AssignTarget, InterpPart, IrExpr, IrStmt, StrStyle};
use crate::shir::parse_arith;

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for s in stmts.iter_mut() {
        c |= transform_stmt(s);
    }
    c
}

fn transform_stmt(st: &mut IrStmt) -> bool {
    // 1) recurse into children first (bottom-up — so a nested let inside
    //    an If body is rewritten before we look at the parent Exec)
    let mut c = match st {
        IrStmt::If { cond, then, elsifs, else_ } => {
            let mut x = transform_expr(cond);
            x |= transform(then);
            for (ec, eb) in elsifs.iter_mut() {
                x |= transform_expr(ec);
                x |= transform(eb);
            }
            x |= transform(else_);
            x
        }
        IrStmt::For { iter, body, .. } => {
            let mut x = transform_expr(iter);
            x |= transform(body);
            x
        }
        IrStmt::While { cond, body, .. } => {
            let mut x = transform(body);
            x |= transform_expr(cond);
            x
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut x = transform(body);
            x |= transform_expr(cond);
            x
        }
        IrStmt::Case { discriminant, clauses } => {
            let mut x = transform_expr(discriminant);
            for cl in clauses.iter_mut() {
                x |= transform(&mut cl.body);
            }
            x
        }
        IrStmt::Block(b)
        | IrStmt::Subshell(b)
        | IrStmt::Background(b)
        | IrStmt::Function { body: b, .. } => transform(b),
        IrStmt::Pipeline { stages, .. } => {
            let mut x = false;
            for stg in stages.iter_mut() {
                x |= transform(stg);
            }
            x
        }
        IrStmt::Expr(e) => {
            // let-in-expr (the common shape: `let "x+=1"` as a statement
            // is an Expr(Call("exec", ...)) — handled here, with the stmt
            // potentially replaced by a Block of assigns).
            let r = lower_let_expr(e);
            let mut x = r.1;
            if let Some(assigns) = r.0 {
                // replace the enclosing IrStmt::Expr with a Block
                *st = IrStmt::Block(assigns);
                x = true;
            }
            x
        }
        IrStmt::Assign { expr, .. }
        | IrStmt::Output { value: expr, .. }
        | IrStmt::Declare { init: Some(expr), .. } => transform_expr(expr),
        IrStmt::WriteFile { path, content, .. } => {
            transform_expr(path) | transform_expr(content)
        }
        IrStmt::Redirect { inner, .. } => transform(inner),
        // stmt-level `IrStmt::Exec { cmd:Str("let"), args:[.., Array([Str,..])] }`
        // (defensive — the common shape is the Expr(Call("exec", ...)) above)
        IrStmt::Exec { cmd, args, env, .. } => {
            let mut x = transform_expr(cmd);
            for a in args.iter_mut() {
                x |= transform_expr(a);
            }
            for (_, v) in env.iter_mut() {
                x |= transform_expr(v);
            }
            x |= lower_let_stmt(st);
            x
        }
        _ => false,
    };
    c
}

fn transform_expr(e: &mut IrExpr) -> bool {
    match e {
        IrExpr::Arrow(stmts) => transform(stmts),
        IrExpr::Call { args, .. } => {
            let mut c = false;
            for a in args.iter_mut() {
                c |= transform_expr(a);
            }
            c
        }
        IrExpr::Array(items) => {
            let mut c = false;
            for a in items.iter_mut() {
                c |= transform_expr(a);
            }
            c
        }
        IrExpr::Object(pairs) => {
            let mut c = false;
            for (_, v) in pairs.iter_mut() {
                c |= transform_expr(v);
            }
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => transform_expr(lhs) | transform_expr(rhs),
        IrExpr::Index { key, .. } => transform_expr(key),
        _ => false,
    }
}

/// Extract a single arith string from a let-arg IR element. The parser
/// wraps every let arg in a double-quoted Interpolate, so `let "x+=1"`
/// produces `Array([Interpolate([StringPart::Literal("x+=1")])])` — the
/// bare `Str` shape is the stmt-level / synthetic path.
fn let_arg_text(a: &IrExpr) -> Option<&str> {
    match a {
        IrExpr::Str(s, _) => Some(s.as_str()),
        IrExpr::Interpolate(parts) if parts.len() == 1 => match &parts[0] {
            InterpPart::Lit(s) => Some(s.as_str()),
            _ => None,
        },
        _ => None,
    }
}

/// `let "x+=1" "y++"` → `Some([Assign{x, Arith(Assign{+=,1})}, Assign{y, Arith(IncDec{+1,postfix})}])`
/// on success; `None` if any arg fails to parse as a let-able arith
/// (or the arg list shape doesn't match — defensive).
fn build_assigns(items: &[IrExpr]) -> Option<Vec<IrStmt>> {
    let mut out = Vec::with_capacity(items.len());
    for a in items {
        let text = let_arg_text(a)?;
        let ast = parse_arith(text)?;
        let target = match &ast {
            ArithAst::Assign { var, .. } => var.clone(),
            ArithAst::IncDec { var, .. } => var.clone(),
            // any other node is not a let-able form (`let 5` is invalid)
            _ => return None,
        };
        out.push(IrStmt::Assign {
            targets: vec![AssignTarget {
                var: target,
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Arith(Box::new(ast)),
        });
    }
    Some(out)
}

/// Stmt-level `IrStmt::Exec { cmd:"let", args:[_, Array([Str,..])] }`:
/// the call-form inside an Exec stmt (defensive — the common shape is
/// the Expr(Call("exec", ...)) handled in `lower_let_expr`).
fn lower_let_stmt(st: &mut IrStmt) -> bool {
    let IrStmt::Exec { cmd, args, .. } = st else { return false };
    let is_let = match &*cmd {
        IrExpr::Str(n, _) if n == "let" => true,
        _ => false,
    };
    if !is_let {
        return false;
    }
    let [_, IrExpr::Array(items)] = args.as_slice() else { return false };
    let assigns = match build_assigns(items) {
        Some(a) => a,
        None => return false,
    };
    if assigns.len() == 1 && items.len() == 1 {
        *st = assigns.into_iter().next().unwrap();
    } else {
        *st = IrStmt::Block(assigns);
    }
    true
}

/// Expr-level `IrExpr::Call { func:"exec", args:[Str("let"), Array([Str,..])] }`.
/// Returns `(Some(assigns), changed)`: the caller replaces the enclosing
/// `IrStmt::Expr` with a `Block` of the assigns when `Some`. On no-match,
/// the expr is left in place (recurse into it to find nested lets) and
/// `changed` reports whether the recursion found anything.
fn lower_let_expr(e: &mut IrExpr) -> (Option<Vec<IrStmt>>, bool) {
    // recurse first — a let nested inside a deeper expr is handled by
    // the same machinery on the way down
    let mut recursed = transform_expr(e);
    if let IrExpr::Call { func, args } = e {
        if func == "exec" {
            if let [IrExpr::Str(name, _), IrExpr::Array(items)] = args.as_slice() {
                if name == "let" {
                    if let Some(assigns) = build_assigns(items) {
                        return (Some(assigns), true);
                    }
                }
            }
        }
    }
    (None, recursed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::commands::parse_commands_from_text;
    use crate::shir::ast_to_ir_raw;
    use crate::shir_json::shir_to_shir_json;

    /// Lower + run the transform + serialize to compact JSON.
    fn lower(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        let _ = transform(&mut prog.stmts);
        shir_to_shir_json(&prog)
    }

    /// Lower + run the transform + assert SOMETHING changed.
    fn assert_changes(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(transform(&mut prog.stmts), "transform was a no-op for {src}");
        shir_to_shir_json(&prog)
    }

    /// Lower WITHOUT the transform — compare against the transformed IR
    /// to confirm the let-exec is gone.
    fn baseline(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse source");
        let prog = ast_to_ir_raw(&commands);
        shir_to_shir_json(&prog)
    }

    #[test]
    fn let_simple_assign_becomes_structured_assign() {
        let json = assert_changes("let \"x=1\"");
        // the let Exec is gone — a real Assign stmt is in its place
        assert!(!json.contains("\"let\""), "let exec should be gone: {json}");
        // the Assign payload is the Arith(Assign) form
        assert!(json.contains("\"Arith\""), "missing Arith payload: {json}");
        assert!(json.contains("\"Assign\""), "missing inner Assign node: {json}");
        assert!(json.contains("\"x\""), "missing target var: {json}");
    }

    #[test]
    fn let_compound_assign_plus_equals() {
        // the real wrong-answer bug: x+=1 as `let` must become a real
        // `x = x + 1` Assign, not a side-effect-free command substitution
        let base = baseline("let \"x+=1\"");
        let json = assert_changes("let \"x+=1\"");
        assert!(!json.contains("\"let\""), "let exec should be gone: {json}");
        assert!(json.contains("\"+=\""), "compound op survived: {json}");
        assert!(json.contains("\"x\""), "missing var: {json}");
        // and the transform actually changed something (vs the renderer
        // path that emitted the broken side-effect-free form)
        assert_ne!(json, base, "transform produced no visible change");
    }

    #[test]
    fn let_incdec_becomes_incdec_arith() {
        let json = assert_changes("let \"x++\"");
        assert!(!json.contains("\"let\""), "let exec should be gone: {json}");
        assert!(json.contains("\"IncDec\""), "missing IncDec node: {json}");
        assert!(json.contains("\"prefix\":false"), "postfix marker wrong: {json}");
    }

    #[test]
    fn let_prefix_incdec() {
        let json = assert_changes("let \"++x\"");
        assert!(json.contains("\"IncDec\""), "missing IncDec: {json}");
        assert!(json.contains("\"prefix\":true"), "prefix marker wrong: {json}");
    }

    #[test]
    fn let_multiple_forms_emit_multiple_assigns() {
        let json = assert_changes("let \"x=1\" \"y++\" \"z+=2\"");
        assert!(!json.contains("\"let\""), "let exec should be gone: {json}");
        // three Arith payloads (one per arg) — the Block contains three
        // Assign stmts
        let n = json.matches("\"Arith\"").count();
        assert_eq!(n, 3, "expected 3 Arith payloads, got {n}: {json}");
        // the three targets
        assert!(json.contains("\"x\""));
        assert!(json.contains("\"y\""));
        assert!(json.contains("\"z\""));
    }

    #[test]
    fn let_arith_with_var_rhs() {
        // `let "x=y+1"` — the RHS is a bin arith (Var + Num). The parser's
        // static arith parser (parse_arith) handles bare identifiers (bash
        // arith uses bare names, not `$var` — `$` is a shell-expansion
        // sigil that resolves BEFORE let sees the string). A `$var`
        // form (e.g. `let "x=$y+1"`) is refused conservatively: the
        // transform would rather let the runtime arith evaluator handle
        // a `$` sigil than mis-parse a shell-expansion artifact.
        let json = assert_changes("let \"x=y+1\"");
        assert!(!json.contains("\"let\""));
        assert!(json.contains("\"Bin\""), "missing Bin arith: {json}");
    }

    #[test]
    fn non_let_exec_is_left_alone() {
        // the transform only fires on `let`; an `exec echo` must be untouched
        let commands = parse_commands_from_text("echo hello").expect("parse");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(!transform(&mut prog.stmts), "echo should be a no-op");
    }

    #[test]
    fn let_in_nested_block_rewrites_recursively() {
        // the let is buried two levels deep
        let json = assert_changes("if true; then let \"x=1\"; fi");
        assert!(!json.contains("\"let\""), "nested let should be gone: {json}");
        assert!(json.contains("\"Arith\""));
    }

    #[test]
    fn let_with_non_arith_arg_is_left_alone() {
        // `let "foo bar"` is not a valid arith expression; refuse >
        // guess — the whole exec stays (the runtime would error too,
        // so leaving it preserves the user's signal).
        let commands =
            parse_commands_from_text("let \"foo bar\"").expect("parse");
        let mut prog = ast_to_ir_raw(&commands);
        let before = shir_to_shir_json(&prog);
        let changed = transform(&mut prog.stmts);
        let after = shir_to_shir_json(&prog);
        assert_eq!(before, after, "non-arith let must be untouched");
        assert!(!changed);
    }

    #[test]
    fn deterministic_across_runs() {
        let src = "let \"x+=1\" \"y++\"";
        assert_eq!(lower(src), lower(src));
    }
}
