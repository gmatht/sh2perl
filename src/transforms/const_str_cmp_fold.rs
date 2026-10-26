//! const-str-cmp-fold — a comparison whose BOTH operands are frozen
//! literals is a compile-time decision: fold `Eq`/`Ne` to `Bool`.
//!
//! name: const-str-cmp-fold
//! prereqs: [copy-propagation, text-capture-fold — the operands must
//!   already be frozen literals; case-lowering / test-lowering — they
//!   mint literal-subject `Eq` chains (`case $lit in pat)`, `[[ lit ==
//!   lit ]]`)]
//! invariant: `Eq(Str(a), Str(b))` → `Bool(a == b)` ONLY when both
//!   styles are plain quoting (SingleQuoted/DoubleQuoted) and the RHS
//!   (the PATTERN position of `[[ x == y ]]` and case) carries no glob
//!   metacharacter (`* ? [ \` — refuse > guess, the affix-strip-fold
//!   refusal set). With no metacharacter the RHS pattern matches
//!   exactly one string — itself — so glob semantics and string
//!   equality coincide; a metacharacter RHS would need a glob
//!   evaluation (quoted-vs-unquoted source quoting is not preserved in
//!   the IR, so refusal is the only sound choice). `Eq(Int, Int)` /
//!   `Ne(Int, Int)` fold unconditionally (exact integer equality is
//!   also string equality on canonical int renderings). Mixed
//!   Int/Str and every non-literal operand refuse: the renderer picks
//!   numeric vs string coercion per backend and the IR does not carry
//!   that choice.
//! scope: offered to all backends. Every renderer then sees a Bool:
//!   rust's `const_cond_truth` inlines the taken branch and drops the
//!   dead one at render; const-condition-elim prunes `if Bool` where
//!   it runs after this fold; C/estree/java emit a literal `if (0)`.
//! updates: none (first offer)
//!
//! ## Need
//! Measured on the 563-example corpus (2026-10-25): case-lowering with
//! a const-propagated subject (`system_name='Darwin'; case $system_name
//! in Linux) … Darwin) …`) and test-lowering with two frozen operands
//! leave literal-vs-literal comparisons that every backend then
//! evaluates at RUNTIME — rust ships `("Darwin".to_string()) ==
//! "Linux"` plus the whole unreachable branch, C ships
//! `strcmp("Darwin", "Linux") != 0`, and the unquoted-clause form
//! ships a `globMatch("Darwin", "Linux")` callout into the glob
//! engine for a comparison with nothing to glob. The comparison is
//! decidable now; the dead-branch elimination is exactly what the rust
//! renderer's `const_cond_truth` machinery was built to do with a
//! `Bool`.
//!
//! Three shapes arrive, one per producer: `Eq`/`Ne` (the rust-gated
//! not-identity-flip turns exact_cond's `Not(Ne)` into `Eq` before
//! this slot), the raw `Not(disc ne lit)` exact_cond itself (every
//! other target — Bool never renders an operator, so folding it is
//! sound where flipping was not: the Perl Eq/Ne asymmetry), and
//! `globMatch(Str, Str)` (case-lowering's unquoted-clause dispatch).
//!
//! ## Placement
//! Registered ONCE, right after `case-lowering` (and the late
//! not-identity entries): that is where all producers of literal
//! comparisons have finished (`test-lowering`/`case-lowering` run at
//! the end of the list; `apply()` runs the list exactly once, so an
//! earlier slot would miss them). Pure structural rewrite — no verdict
//! statics, idempotent, shell-agnostic (any source language feeding
//! the shared IR compares frozen literals the same way).

use crate::ir::{BinOpKind, IrExpr, IrStmt, StrStyle};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    for st in stmts.iter_mut() {
        c |= stmt(st);
    }
    c
}

fn stmt(st: &mut IrStmt) -> bool {
    match st {
        IrStmt::Ext(e) => {
            let mut c = false;
            for s in e.children_mut() {
                c |= stmt(s);
            }
            c
        }
        IrStmt::Output { value, .. } | IrStmt::WriteFile { content: value, .. } => expr(value),
        IrStmt::WriteFile { path, .. } => expr(path),
        IrStmt::Assign { targets, expr: e, .. } => {
            let mut c = expr(e);
            for t in targets.iter_mut() {
                for i in t.indices.iter_mut() {
                    c |= expr(i);
                }
            }
            c
        }
        IrStmt::Declare { init: Some(e), .. } => expr(e),
        IrStmt::Declare { init: None, .. } => false,
        IrStmt::DeclareArray { elements, .. } => {
            let mut c = false;
            for e in elements.iter_mut() {
                c |= expr(e);
            }
            c
        }
        IrStmt::If { cond, then, elsifs, else_ } => {
            let mut c = expr(cond);
            for (ec, eb) in elsifs.iter_mut() {
                c |= expr(ec);
                for s in eb.iter_mut() {
                    c |= stmt(s);
                }
            }
            for s in then.iter_mut().chain(else_.iter_mut()) {
                c |= stmt(s);
            }
            c
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            let mut c = false;
            for s in body.iter_mut().chain(else_body.iter_mut()).chain(finally_body.iter_mut()) {
                c |= stmt(s);
            }
            for ex in excepts.iter_mut() {
                if let Some(m) = ex.match_expr.as_mut() {
                    c |= expr(m);
                }
                for s in ex.body.iter_mut() {
                    c |= stmt(s);
                }
            }
            c
        }
        IrStmt::ForInit { init, cond, step, body, .. } => {
            let mut c = expr(cond);
            for v in [init, step, body] {
                for s in v.iter_mut() {
                    c |= stmt(s);
                }
            }
            c
        }
        IrStmt::While { cond, body } => {
            let mut c = expr(cond);
            for s in body.iter_mut() {
                c |= stmt(s);
            }
            c
        }
        IrStmt::DoWhile { body, cond, .. } => {
            let mut c = expr(cond);
            for s in body.iter_mut() {
                c |= stmt(s);
            }
            c
        }
        IrStmt::For { var: _, iter, body } => {
            let mut c = expr(iter);
            for s in body.iter_mut() {
                c |= stmt(s);
            }
            c
        }
        IrStmt::Die { expr: e, .. }
        | IrStmt::Warn { expr: e, .. }
        | IrStmt::SetChildError(e) => expr(e),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => expr(e),
        IrStmt::Return(None) | IrStmt::Exit(None) => false,
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            let mut c = expr(cmd);
            for a in args.iter_mut().chain(redirects.iter_mut()) {
                c |= expr(a);
            }
            for (_, e) in env.iter_mut() {
                c |= expr(e);
            }
            c
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            for stg in stages.iter_mut() {
                for s in stg.iter_mut() {
                    c |= stmt(s);
                }
            }
            c
        }
        IrStmt::Case { discriminant, clauses } => {
            let mut c = expr(discriminant);
            for cl in clauses.iter_mut() {
                for s in cl.body.iter_mut() {
                    c |= stmt(s);
                }
            }
            c
        }
        IrStmt::Redirect { inner, redirects, .. } => {
            let mut c = false;
            for s in inner.iter_mut() {
                c |= stmt(s);
            }
            for r in redirects.iter_mut() {
                c |= expr(&mut r.target);
            }
            c
        }
        IrStmt::Function { body, named_blocks, .. } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt(s);
            }
            for (_, blk) in named_blocks.iter_mut() {
                for s in blk.iter_mut() {
                    c |= stmt(s);
                }
            }
            c
        }
        IrStmt::Subshell(v) | IrStmt::Background(v) | IrStmt::Block(v) => {
            let mut c = false;
            for s in v.iter_mut() {
                c |= stmt(s);
            }
            c
        }
        IrStmt::Select { clauses } => {
            let mut c = false;
            for cl in clauses.iter_mut() {
                for e in cl.ch.iter_mut().chain(cl.value.iter_mut()) {
                    c |= expr(e);
                }
                for s in cl.body.iter_mut() {
                    c |= stmt(s);
                }
            }
            c
        }
        IrStmt::Asm { outputs, inputs, .. } => {
            let mut c = false;
            for (_, e) in outputs.iter_mut().chain(inputs.iter_mut()) {
                c |= expr(e);
            }
            c
        }
        IrStmt::Expr(e) => expr(e),
        IrStmt::Shaderize { body, .. } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= stmt(s);
            }
            c
        }
        // no expression children
        IrStmt::Continue
        | IrStmt::Break
        | IrStmt::Label(_)
        | IrStmt::Goto(_)
        | IrStmt::Require(_)
        | IrStmt::RawText(_) => false,
    }
}

fn expr(e: &mut IrExpr) -> bool {
    let mut c = fold(e);
    match e {
        IrExpr::Index { key, .. } => c |= expr(key),
        IrExpr::BinOp { lhs, rhs, .. } => c |= expr(lhs) | expr(rhs),
        IrExpr::Call { args, .. } | IrExpr::Array(args) => {
            for a in args.iter_mut() {
                c |= expr(a);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            c |= expr(obj);
            for a in args.iter_mut() {
                c |= expr(a);
            }
        }
        IrExpr::Ternary { cond, then, else_ } => c |= expr(cond) | expr(then) | expr(else_),
        IrExpr::DefinedOr { expr: x, default } => c |= expr(x) | expr(default),
        IrExpr::Interpolate(parts) => {
            for p in parts.iter_mut() {
                if let crate::ir::InterpPart::Expr(x) = p {
                    c |= expr(x);
                }
            }
        }
        IrExpr::Capture { expr: x, .. } => c |= expr(x),
        IrExpr::Arrow(v) | IrExpr::Lambda { body: v, .. } => {
            for s in v.iter_mut() {
                c |= stmt(s);
            }
        }
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            c |= expr(iter) | expr(elem);
            if let Some(x) = cond.as_mut() {
                c |= expr(x);
            }
        }
        IrExpr::Splice(x) => c |= expr(x),
        IrExpr::Object(pairs) => {
            for (_, x) in pairs.iter_mut() {
                c |= expr(x);
            }
        }
        IrExpr::Ext(x) => {
            for x in x.children_mut() {
                c |= expr(x);
            }
        }
        // leaves
        _ => {}
    }
    c
}

/// A plain quoting style: the frozen literal's bytes are its value
/// (command/heredoc styles can carry re-evaluation semantics in some
/// renderer — refuse those, module doc).
fn plain_style(s: &StrStyle) -> bool {
    matches!(s, StrStyle::SingleQuoted | StrStyle::DoubleQuoted)
}

/// The affix-strip-fold refusal set for the PATTERN (RHS) position:
/// any glob metacharacter or escape — refusal > guess.
fn has_glob_metachar(s: &str) -> bool {
    s.contains(['*', '?', '[', '\\'])
}

/// Fold one node (bottom-up callers recurse after the fold).
fn fold(e: &mut IrExpr) -> bool {
    // `globMatch(value, pattern)` (case-lowering's engine-glob
    // dispatch) with two frozen operands and a metacharacter-free
    // pattern: the pattern matches exactly one string — itself — so
    // the glob IS string equality. A metacharacter pattern refuses
    // (runtime glob stays). Args order per case_lowering::glob_cond:
    // [value, pattern].
    //
    // NOT folded here: `strHasPrefix`/`strHasSuffix`/`contains` —
    // their literal-operand forms are decidable in principle, but the
    // committed `grep_*_lifts_to_contains` tests pin those nodes as
    // the test-lifting FEATURE's structure (refuse what other lanes
    // have claimed; measured corpus gain was 0 sites anyway).
    if let IrExpr::Call { func, args } = e {
        if func == "globMatch" && args.len() == 2 {
            let folded = match (&args[0], &args[1]) {
                (IrExpr::Str(v, vs), IrExpr::Str(pat, ps))
                    if plain_style(vs) && plain_style(ps) && !has_glob_metachar(pat) =>
                {
                    Some(v == pat)
                }
                _ => None,
            };
            if let Some(truth) = folded {
                *e = IrExpr::Bool(truth);
                return true;
            }
        }
        return false;
    }
    // The wrapped shape case-lowering's exact_cond builds directly
    // (`Not(disc ne lit)`): flip+fold would need the rust-gated flip,
    // but Bool never renders an operator — fold it for EVERY target.
    if let IrExpr::BinOp { op: BinOpKind::Not, lhs, rhs } = e {
        if lhs.as_ref() != rhs.as_ref() {
            return false; // single-sided Not — construction bug
        }
        if let IrExpr::BinOp { op: BinOpKind::Ne, lhs: i, rhs: r } = rhs.as_ref() {
            if let (IrExpr::Str(a, sa), IrExpr::Str(b, sb)) = (i.as_ref(), r.as_ref()) {
                if plain_style(sa)
                    && plain_style(sb)
                    && !has_glob_metachar(b)
                {
                    *e = IrExpr::Bool(a == b); // ¬(a ne b) ≡ a == b
                    return true;
                }
            }
        }
        return false;
    }
    let (op, l, r) = match e {
        IrExpr::BinOp { op: op @ (BinOpKind::Eq | BinOpKind::Ne), lhs, rhs } => (op, lhs, rhs),
        _ => return false,
    };
    let truth = match (l.as_ref(), r.as_ref()) {
        (IrExpr::Str(a, sa), IrExpr::Str(b, sb)) => {
            if !plain_style(sa) || !plain_style(sb) || has_glob_metachar(b) {
                return false;
            }
            let eq = a == b;
            if matches!(op, BinOpKind::Eq) { eq } else { !eq }
        }
        (IrExpr::Int(a), IrExpr::Int(b)) => {
            let eq = a == b;
            if matches!(op, BinOpKind::Eq) { eq } else { !eq }
        }
        // mixed Int/Str and every non-literal operand: the coercion
        // choice lives in the renderer — refuse (module doc).
        _ => return false,
    };
    *e = IrExpr::Bool(truth);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrExpr, StrStyle};

    fn s(x: &str) -> IrExpr {
        IrExpr::Str(x.to_string(), StrStyle::DoubleQuoted)
    }

    fn cmp(op: BinOpKind, l: IrExpr, r: IrExpr) -> IrExpr {
        IrExpr::BinOp { op, lhs: Box::new(l), rhs: Box::new(r) }
    }

    fn cond_of(stmts: &[IrStmt]) -> IrExpr {
        match &stmts[0] {
            IrStmt::If { cond, .. } => cond.clone(),
            other => panic!("expected If, got {other:?}"),
        }
    }

    fn run_if(cond: IrExpr) -> IrExpr {
        let mut stmts = vec![IrStmt::If { cond, then: vec![], elsifs: vec![], else_: vec![] }];
        transform(&mut stmts);
        cond_of(&stmts)
    }

    #[test]
    fn folds_equal_literals() {
        assert_eq!(run_if(cmp(BinOpKind::Eq, s("Darwin"), s("Darwin"))), IrExpr::Bool(true));
        assert_eq!(run_if(cmp(BinOpKind::Eq, s("Darwin"), s("Linux"))), IrExpr::Bool(false));
        assert_eq!(run_if(cmp(BinOpKind::Ne, s("a"), s("b"))), IrExpr::Bool(true));
    }

    #[test]
    fn folds_int_literals() {
        assert_eq!(run_if(cmp(BinOpKind::Eq, IrExpr::Int(2), IrExpr::Int(2))), IrExpr::Bool(true));
        assert_eq!(run_if(cmp(BinOpKind::Ne, IrExpr::Int(2), IrExpr::Int(3))), IrExpr::Bool(true));
    }

    #[test]
    fn refuses_glob_rhs() {
        // the RHS is the PATTERN position of `[[ x == a* ]]` — a glob
        // must stay a runtime match (quoted-vs-unquoted is not in IR)
        let c = cmp(BinOpKind::Eq, s("abc"), s("a*"));
        assert_eq!(run_if(c.clone()), c);
        let c2 = cmp(BinOpKind::Eq, s("abc"), s("a?c"));
        assert_eq!(run_if(c2.clone()), c2);
        let c3 = cmp(BinOpKind::Ne, s("x"), s("[ab]"));
        assert_eq!(run_if(c3.clone()), c3);
    }

    #[test]
    fn refuses_dynamic_and_mixed_operands() {
        let var = IrExpr::Var("v".into(), None);
        let c = cmp(BinOpKind::Eq, var.clone(), s("x"));
        assert_eq!(run_if(c.clone()), c);
        let c2 = cmp(BinOpKind::Eq, IrExpr::Int(1), s("1"));
        assert_eq!(run_if(c2.clone()), c2);
    }

    #[test]
    fn refuses_command_style() {
        let l = IrExpr::Str("x".into(), StrStyle::Command);
        let c = cmp(BinOpKind::Eq, l, s("x"));
        assert_eq!(run_if(c.clone()), c);
    }

    #[test]
    fn folds_globmatch_with_literal_pattern() {
        // globMatch(value, pattern) — no metacharacter in the pattern
        // ⇒ the glob predicate is string equality
        let glob = IrExpr::Call {
            func: "globMatch".to_string(),
            args: vec![s("Darwin"), s("Darwin")],
        };
        let mut stmts = vec![IrStmt::If { cond: glob.clone(), then: vec![], elsifs: vec![], else_: vec![] }];
        transform(&mut stmts);
        assert_eq!(cond_of(&stmts), IrExpr::Bool(true));
        // metacharacter pattern stays a runtime glob
        let glob2 = IrExpr::Call {
            func: "globMatch".to_string(),
            args: vec![s("abc"), s("a*")],
        };
        let mut stmts2 = vec![IrStmt::If { cond: glob2.clone(), then: vec![], elsifs: vec![], else_: vec![] }];
        transform(&mut stmts2);
        assert_eq!(cond_of(&stmts2), glob2);
        // dynamic value stays runtime
        let glob3 = IrExpr::Call {
            func: "globMatch".to_string(),
            args: vec![IrExpr::Var("v".into(), None), s("abc")],
        };
        let mut stmts3 = vec![IrStmt::If { cond: glob3.clone(), then: vec![], elsifs: vec![], else_: vec![] }];
        transform(&mut stmts3);
        assert_eq!(cond_of(&stmts3), glob3);
    }

    #[test]
    fn folds_wrapped_exact_cond() {
        // case-lowering's exact_cond: Not(disc ne lit) with a frozen
        // subject — folds for EVERY target (no operator is rendered)
        let ne = IrExpr::BinOp {
            op: BinOpKind::Ne,
            lhs: Box::new(s("Darwin")),
            rhs: Box::new(IrExpr::Str("Linux".into(), StrStyle::SingleQuoted)),
        };
        let wrapped = IrExpr::BinOp {
            op: BinOpKind::Not,
            lhs: Box::new(ne.clone()),
            rhs: Box::new(ne),
        };
        let mut stmts = vec![IrStmt::If { cond: wrapped, then: vec![], elsifs: vec![], else_: vec![] }];
        transform(&mut stmts);
        assert_eq!(cond_of(&stmts), IrExpr::Bool(false)); // Darwin == Linux is false
        // glob-metachar rhs refuses even wrapped
        let ne2 = IrExpr::BinOp {
            op: BinOpKind::Ne,
            lhs: Box::new(s("abc")),
            rhs: Box::new(IrExpr::Str("a*".into(), StrStyle::SingleQuoted)),
        };
        let wrapped2 = IrExpr::BinOp {
            op: BinOpKind::Not,
            lhs: Box::new(ne2.clone()),
            rhs: Box::new(ne2),
        };
        let mut stmts2 = vec![IrStmt::If { cond: wrapped2.clone(), then: vec![], elsifs: vec![], else_: vec![] }];
        transform(&mut stmts2);
        assert_eq!(cond_of(&stmts2), wrapped2);
    }

    #[test]
    fn folds_inside_assignments_too() {
        let mut stmts = vec![IrStmt::Expr(cmp(BinOpKind::Eq, s("a"), s("a")))];
        assert!(transform(&mut stmts));
        match &stmts[0] {
            IrStmt::Expr(e) => assert_eq!(*e, IrExpr::Bool(true)),
            other => panic!("expected Expr, got {other:?}"),
        }
        assert!(!transform(&mut stmts), "idempotent");
    }

    #[test]
    fn affix_predicates_are_left_alone() {
        // contains/strHasPrefix are CLAIMED by the test-lifting
        // feature's pinned tests — even the decidable literal form
        // stays (refuse what other lanes own; module doc).
        let mk = |func: &str, a: &str, b: &str| IrExpr::Call {
            func: func.to_string(),
            args: vec![s(a), s(b)],
        };
        let c = mk("contains", "Darwin", "ar");
        let mut stmts = vec![IrStmt::If { cond: c.clone(), then: vec![], elsifs: vec![], else_: vec![] }];
        transform(&mut stmts);
        assert_eq!(cond_of(&stmts), c);
        let c2 = mk("strHasPrefix", "Darwin", "Dar");
        let mut stmts2 = vec![IrStmt::If { cond: c2.clone(), then: vec![], elsifs: vec![], else_: vec![] }];
        transform(&mut stmts2);
        assert_eq!(cond_of(&stmts2), c2);
    }
}
