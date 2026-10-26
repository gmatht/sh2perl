//! str-splice-fold — `${b:0:$o}${repl}${b:$k}` → ONE `StrSplice` node.
//!
//! ## The pattern
//! A bash statement that splices a slice of a string back into itself
//! across a store:
//!
//! ```sh
//! RFB_ROWS[$y]="${rbp_row:0:$o}${rb_run}${rbp_row:$k}"
//! ```
//!
//! lowers to an `Interpolate` of two `param("slice", …)` reads over the
//! SAME base — `join(slice(b, 0, o)) ++ repl ++ join(slice(b, k, ""))` —
//! which every backend must then render as two substring TEMPORARIES, a
//! join temporary, and a store that copies the joined result one more
//! time: four passes over a string the IR already described in one
//! statement. (MIMEcroft's rasterizer does this per pixel over a
//! 2560-byte framebuffer row: it was 28% of self-time in `rb_putrun`
//! plus the substr/arr_set helpers it drives — measured ~40% of the
//! demo's in-binary samples.)
//!
//! ## The rewrite (shIR → shIR, backend-neutral)
//! The Interpolate becomes `StrSplice { base, start, end, repl }`
//! (`shir_nodes/str_splice.node`), whose meaning is EXACTLY the concat
//! it replaced: `base[0..start] ++ repl ++ base[end..]`, both offsets
//! clamped by the same arithmetic `_sh_substr` applies. Nothing is
//! restructured or reordered — the node just states the splice as one
//! operation so a store site can write the result into the destination's
//! own buffer instead of shuttling it through three temps. A backend
//! that does not consume the node never runs this fold (the §11
//! target-gate: `transform_applies` lists only `c`, so every other
//! target, and an UNSET target on the byte-pinned ingress path, keeps
//! the original Interpolate byte-for-byte).
//!
//! ## Refuse > guess (each condition keeps the original shape)
//! - fewer than 2 parts, or a non-`Expr` part at either end;
//! - either end is not a `slice` read (`join` unwraps the frontend's
//!   single-arg wrapper first);
//! - the two slices read DIFFERENT bases (`${a:0:$o}$r${b:$k}` is a
//!   concat of two strings, not a splice of one);
//! - the prefix does not start at offset 0 (a middle slice would need
//!   two more node fields and fires nowhere in the corpus);
//! - the suffix does not run to the end of the string (`len` present and
//!   non-empty — a bounded tail is a different cut);
//! - the base is not a plain identifier (no `${…}`/positional forms).
//!
//! ## Kill switch / cache
//! `SH2_NO_SPLICE=1` disables the fold without a rebuild (the A/B
//! switch; it keys bash-O4's artifact cache like every other SH2_NO_*
//! render switch, and `SH2_TRANSFORMS` gates it by name).

use crate::ir::{InterpPart, IrExpr, IrStmt};
use crate::shir_nodes::StrSplice;

/// A plain identifier (`rbp_row`, `_y`) — never `${…}`, a positional, or
/// an index expression.
fn is_ident(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

/// The one A/B kill switch (transform + C fused arm read the same
/// predicate). Tests that EXPECT a fold skip under it — the suite may be
/// invoked with `SH2_NO_SPLICE=1` to exercise the OFF arm.
fn kill_switch() -> bool {
    std::env::var("SH2_NO_SPLICE").map(|v| v == "1").unwrap_or(false)
}

/// Apply the fold. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    if kill_switch() {
        return false;
    }
    let mut changed = false;
    walk(stmts, &mut changed);
    changed
}

fn walk(stmts: &mut Vec<IrStmt>, changed: &mut bool) {
    for st in stmts.iter_mut() {
        if let IrStmt::Assign { expr, .. } = st {
            if let Some(folded) = fold(expr) {
                *expr = folded;
                *changed = true;
            }
        }
        for body in stmt_bodies_mut(st) {
            walk(body, changed);
        }
        // pipeline stages (`a | b` bodies)
        if let IrStmt::Pipeline { stages, .. } = st {
            for s in stages.iter_mut() {
                walk(s, changed);
            }
        }
    }
}

/// Every nested statement list this fold must reach (the shell function
/// bodies — `IrStmt::Function` — are where programs like MIMEcroft keep
/// their rasterizer).
fn stmt_bodies_mut(st: &mut IrStmt) -> Vec<&mut Vec<IrStmt>> {
    let mut v = Vec::new();
    match st {
        IrStmt::If { then, elsifs, else_, .. } => {
            v.push(then);
            for (_, b) in elsifs {
                v.push(b);
            }
            v.push(else_);
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            v.push(body);
            for e in excepts {
                v.push(&mut e.body);
            }
            v.push(else_body);
            v.push(finally_body);
        }
        IrStmt::Function { body, .. } => v.push(body),
        IrStmt::For { body, .. }
        | IrStmt::ForInit { body, .. }
        | IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. } => v.push(body),
        IrStmt::Case { clauses, .. } => {
            for c in clauses {
                v.push(&mut c.body);
            }
        }
        IrStmt::Select { clauses } => {
            for c in clauses {
                v.push(&mut c.body);
            }
        }
        IrStmt::Subshell(b)
        | IrStmt::Block(b)
        | IrStmt::Background(b)
        | IrStmt::Redirect { inner: b, .. } => v.push(b),
        _ => {}
    }
    v
}

/// Unwrap `join(x)` (the frontend's single-word join wrapper) to `x`.
fn unjoin(e: &IrExpr) -> &IrExpr {
    if let IrExpr::Call { func, args } = e {
        if func == "join" && args.len() == 1 {
            return &args[0];
        }
    }
    e
}

/// A `param("slice", BASE, off, len)` read → (base name, off, len).
/// The `len` may be absent (rendered as the end sentinel).
fn slice_args(e: &IrExpr) -> Option<(&str, &IrExpr, Option<&IrExpr>)> {
    let IrExpr::Call { func, args } = unjoin(e) else {
        return None;
    };
    if func != "param" {
        return None;
    }
    match args.as_slice() {
        [IrExpr::Str(op, _), IrExpr::Str(base, _), off, len] if op == "slice" => {
            Some((base.as_str(), off, Some(len)))
        }
        [IrExpr::Str(op, _), IrExpr::Str(base, _), off] if op == "slice" => {
            Some((base.as_str(), off, None))
        }
        _ => None,
    }
}

/// Is this expr the literal offset `0`?
fn is_zero(e: &IrExpr) -> bool {
    match e {
        IrExpr::Str(s, _) => s.trim() == "0",
        IrExpr::Int(n) => *n == 0,
        _ => false,
    }
}

/// Is this expr the empty length (`""` → the "to the end" sentinel)?
fn is_end(e: Option<&IrExpr>) -> bool {
    match e {
        None => true,
        Some(IrExpr::Str(s, _)) => s.trim_matches(|c| c == '"' || c == '\'').is_empty(),
        _ => false,
    }
}

/// Fold one statement's RHS. Returns the node when the shape matches.
fn fold(expr: &IrExpr) -> Option<IrExpr> {
    let IrExpr::Interpolate(parts) = expr else {
        return None;
    };
    if parts.len() < 2 {
        return None;
    }
    let InterpPart::Expr(first) = parts.first()? else { return None; };
    let InterpPart::Expr(last) = parts.last()? else { return None; };
    let (b_first, off_first, len_first) = slice_args(first)?;
    let (b_last, off_last, len_last) = slice_args(last)?;
    // Both slices must read the SAME base — one string spliced into one
    // string — and the base must be a plain identifier the renderers can
    // read as a scalar.
    if b_first != b_last || !is_ident(b_first) {
        return None;
    }
    // Prefix `[0, start)`: offset literal 0, bounded by `start` (the
    // node's `start` field). Suffix from `end` to the string's end.
    if !is_zero(off_first) || !is_end(len_last) {
        return None;
    }
    let start = len_first?.clone();
    let end = off_last.clone();
    // Middle parts = repl (empty when the Interpolate is exactly the
    // two slices — `${b:0:$o}${b:$k}`).
    let repl = IrExpr::Interpolate(parts[1..parts.len() - 1].to_vec());
    Some(IrExpr::Ext(Box::new(StrSplice {
        // base as the plain Var READ (not a name string): the analysis
        // walkers recurse ExtExpr::children, so this keeps the base's
        // liveness visible (DSE census / copy-prop precedent) — a
        // `string` field would have hidden the read.
        base: IrExpr::Var(b_first.to_string(), None),
        start,
        end,
        repl,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, StrStyle};

    fn slice(base: &str, off: IrExpr, len: IrExpr) -> IrExpr {
        IrExpr::Call {
            func: "join".to_string(),
            args: vec![IrExpr::Call {
                func: "param".to_string(),
                args: vec![
                    IrExpr::Str("slice".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Str(base.to_string(), StrStyle::DoubleQuoted),
                    off,
                    len,
                ],
            }],
        }
    }

    fn str(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn splice_stmt(expr: IrExpr) -> Vec<IrStmt> {
        vec![IrStmt::Assign {
            targets: vec![AssignTarget {
                var: "ROWS[$y]".to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        }]
    }

    fn target_expr() -> IrExpr {
        IrExpr::Interpolate(vec![
            InterpPart::Expr(Box::new(slice("row", str("0"), str("$o")))),
            InterpPart::Expr(Box::new(str("$run"))),
            InterpPart::Expr(Box::new(slice("row", str("$k"), str("")))),
        ])
    }

    #[test]
    fn folds_the_mimecroft_splice_shape() {
        if kill_switch() { return; }
        let mut s = splice_stmt(target_expr());
        assert!(transform(&mut s));
        let IrStmt::Assign { expr, .. } = &s[0] else { panic!() };
        let IrExpr::Ext(n) = expr else { panic!("not folded: {expr:?}") };
        assert_eq!(n.tag(), "StrSplice");
        let sp = n.as_any().downcast_ref::<StrSplice>().unwrap();
        assert!(matches!(&sp.base, IrExpr::Var(n, _) if n == "row"));
        assert_eq!(sp.start, str("$o"));
        assert_eq!(sp.end, str("$k"));
        assert!(matches!(&sp.repl, IrExpr::Interpolate(p) if p.len() == 1));
    }

    #[test]
    fn two_slice_interpolate_is_idempotent_after_folding() {
        if kill_switch() { return; }
        let mut s = splice_stmt(target_expr());
        assert!(transform(&mut s));
        // second run: the node is not an Interpolate → no change
        assert!(!transform(&mut s));
    }

    #[test]
    fn refuses_different_bases() {
        let mut s = splice_stmt(IrExpr::Interpolate(vec![
            InterpPart::Expr(Box::new(slice("a", str("0"), str("$o")))),
            InterpPart::Expr(Box::new(str("$run"))),
            InterpPart::Expr(Box::new(slice("b", str("$k"), str("")))),
        ]));
        assert!(!transform(&mut s));
        assert!(matches!(s[0], IrStmt::Assign { ref expr, .. } if matches!(expr, IrExpr::Interpolate(_))));
    }

    #[test]
    fn refuses_prefix_not_starting_at_zero() {
        let mut s = splice_stmt(IrExpr::Interpolate(vec![
            InterpPart::Expr(Box::new(slice("row", str("$z"), str("$o")))),
            InterpPart::Expr(Box::new(str("$run"))),
            InterpPart::Expr(Box::new(slice("row", str("$k"), str("")))),
        ]));
        assert!(!transform(&mut s));
    }

    #[test]
    fn refuses_bounded_suffix() {
        let mut s = splice_stmt(IrExpr::Interpolate(vec![
            InterpPart::Expr(Box::new(slice("row", str("0"), str("$o")))),
            InterpPart::Expr(Box::new(str("$run"))),
            InterpPart::Expr(Box::new(slice("row", str("$k"), str("$n")))),
        ]));
        assert!(!transform(&mut s));
    }

    #[test]
    fn refuses_non_identifier_base() {
        let mut s = splice_stmt(IrExpr::Interpolate(vec![
            InterpPart::Expr(Box::new(slice("${x}", str("0"), str("$o")))),
            InterpPart::Expr(Box::new(str("$run"))),
            InterpPart::Expr(Box::new(slice("${x}", str("$k"), str("")))),
        ]));
        assert!(!transform(&mut s));
    }

    #[test]
    fn kill_switch_disables_the_fold() {
        // The A/B switch is env-gated; the OFF side is only observable
        // when the suite itself is invoked with the switch set (env vars
        // are process-global — setting it inside one parallel test would
        // race the fold tests above).
        if kill_switch() {
            let mut s = splice_stmt(target_expr());
            assert!(!transform(&mut s));
        }
    }

    #[test]
    fn reaches_function_bodies() {
        if kill_switch() { return; }
        let body = splice_stmt(target_expr());
        let mut s = vec![IrStmt::Function {
            name: "rb_putrun".to_string(),
            params: vec![],
            body,
            named_blocks: vec![],
        }];
        assert!(transform(&mut s));
        let IrStmt::Function { body, .. } = &s[0] else { panic!() };
        let IrStmt::Assign { expr, .. } = &body[0] else { panic!() };
        assert!(matches!(expr, IrExpr::Ext(_)));
    }
}
