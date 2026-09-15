//! lane_private — privatization proofs for sequential lane-nests.
//!
//! A lane-nest is an outer counted loop whose body is:
//!  1. straight-line scalar inits (lane-private seeds),
//!  2. exactly one data-dependent `While` of scalar assigns/ifs (the
//!     sequential chain, executed whole per lane),
//!  3. exactly one scalar accumulate into a loop-carried total.
//! When every chain-written var is (re)defined by the inits on every
//! outer iteration, lanes are independent: a GPU may run each outer
//! iteration's chain sequentially in one lane and tree-reduce the
//! per-lane results (collatz; CUDA consumes first, C/Vulkan later).
//!
//! The analysis is body-local and backend-neutral. The consumer owns
//! whole-program scope (no lane-private reads after the nest —
//! unobservable on the device otherwise) and lowers to its own IR.
//!
//! Twin predicates: `is_plain_scalar` and the let-text audit below are
//! duplicated from `transforms::fuse_fill_consume` (same crate, private
//! there). Unify on the third user (rule of three) — not before, to
//! avoid churning validated code.

use crate::ir::{ArithAst, IrExpr, IrStmt};
use std::collections::BTreeSet;

/// Right-hand side of a lane init: literal or pure arith (cloned).
#[derive(Debug, Clone, PartialEq)]
pub enum InitRhs {
    Num(i64),
    Arith(ArithAst),
}

/// A classified sequential lane-nest (indices into the outer body).
#[derive(Debug, Clone)]
pub struct SeqChain {
    /// Lane-private vars, init order (deduped).
    pub private: Vec<String>,
    /// (name, rhs) inits — straight-line prefix of the body.
    pub inits: Vec<(String, InitRhs)>,
    /// Position of the chain `While` in the body.
    pub while_idx: usize,
    /// Position of the accumulate tail (always last).
    pub tail_idx: usize,
    /// Loop-carried total.
    pub acc: String,
    /// The private var accumulated (`total += s` → `s`).
    pub acc_var: String,
}

/// Classify an outer-loop body as a sequential lane-nest. `counter` is
/// the outer counter; `invariant` names readable-but-unwritten
/// lane-external vars (bound, etc.). Returns None (veto) on any
/// non-conforming shape — never a partial classification.
pub fn classify_seq_chain(
    body: &[IrStmt],
    counter: &str,
    invariant: &[&str],
) -> Option<SeqChain> {
    let inv: BTreeSet<&str> = invariant.iter().copied().collect();
    // 1) Leading scalar inits with ordered dominance (each RHS may read
    // the counter, invariants, or EARLIER inits only — never a
    // later-init var from a previous iteration).
    let mut inits: Vec<(String, InitRhs)> = Vec::new();
    let mut defined: BTreeSet<String> = BTreeSet::new();
    defined.insert(counter.to_string());
    for v in inv.iter() {
        defined.insert(v.to_string());
    }
    let mut w = body.len();
    for (idx, s) in body.iter().enumerate() {
        let IrStmt::Assign { targets, expr, .. } = s else {
            // First non-assign must be the chain While (checked below).
            w = idx;
            break;
        };
        let [t] = targets.as_slice() else {
            w = idx;
            break;
        };
        if !t.indices.is_empty() || t.var.contains('[') {
            w = idx;
            break;
        }
        if &t.var == counter || inv.contains(t.var.as_str()) {
            // Inits never redefine the counter or invariants.
            return None;
        }
        let rhs = match init_rhs(expr) {
            Some(r) => r,
            None => {
                w = idx;
                break;
            }
        };
        // Ordered dominance + plain names.
        let mut free = BTreeSet::new();
        init_rhs_free(&rhs, &mut free);
        if free.iter().any(|v| !defined.contains(v) || !is_plain_scalar(v)) {
            return None;
        }
        defined.insert(t.var.clone());
        inits.push((t.var.clone(), rhs));
    }
    if inits.is_empty() {
        return None;
    }
    // 2) Exactly one While next.
    let IrStmt::While { cond, body: wbody } = body.get(w)? else {
        return None;
    };
    let init_names: BTreeSet<String> = inits.iter().map(|(n, _)| n.clone()).collect();
    let mut readable: BTreeSet<String> = init_names.clone();
    readable.insert(counter.to_string());
    for v in inv.iter() {
        readable.insert(v.to_string());
    }
    if !test_pure(cond, &readable) {
        return None;
    }
    // 3) Chain body: assigns + flat ifs, all readable, writes ⊆ inits.
    let mut writes: BTreeSet<String> = BTreeSet::new();
    for s in wbody.iter() {
        match s {
            IrStmt::Assign { targets, expr, .. } => {
                let [t] = targets.as_slice() else {
                    return None;
                };
                if !t.indices.is_empty() || t.var.contains('[') {
                    return None;
                }
                if &t.var == counter || inv.contains(t.var.as_str()) {
                    return None;
                }
                let IrExpr::Arith(a) = expr else {
                    return None;
                };
                if !pure_chain_arith(a) {
                    return None;
                }
                let mut free = BTreeSet::new();
                arith_free_vars(a, &mut free);
                if free.iter().any(|v| !readable.contains(v) || !is_plain_scalar(v)) {
                    return None;
                }
                writes.insert(t.var.clone());
            }
            IrStmt::If { cond, then, elsifs, else_, .. } => {
                if !test_pure(cond, &readable) {
                    return None;
                }
                if !check_assign_block(then, counter, &inv, &readable, &mut writes) {
                    return None;
                }
                for (c, b) in elsifs {
                    if !test_pure(c, &readable) {
                        return None;
                    }
                    if !check_assign_block(b, counter, &inv, &readable, &mut writes) {
                        return None;
                    }
                }
                if !check_assign_block(else_, counter, &inv, &readable, &mut writes) {
                    return None;
                }
            }
            _ => return None,
        }
    }
    if writes.is_empty() {
        return None;
    }
    // Dominating defs: every chain-written var is init-defined.
    if !writes.iter().all(|v| init_names.contains(v)) {
        return None;
    }
    // 4) Tail: last stmt, `acc = acc + P` / `P + acc` with P private.
    if w + 1 != body.len() - 1 {
        return None;
    }
    let IrStmt::Assign { targets, expr, .. } = &body[w + 1] else {
        return None;
    };
    let [t] = targets.as_slice() else {
        return None;
    };
    if !t.indices.is_empty() || t.var.contains('[') || &t.var == counter {
        return None;
    }
    let acc = t.var.clone();
    if init_names.contains(&acc) || inv.contains(acc.as_str()) {
        return None;
    }
    let IrExpr::Arith(a) = expr else {
        return None;
    };
    let ArithAst::Bin { op, lhs, rhs } = a.as_ref() else {
        return None;
    };
    if op != "+" {
        return None;
    }
    // The frontends wrap int-domain reads in `Cast(Int64, …)` (an
    // exactness marker), so peel casts before matching the accumulator
    // operand. The CUDA lane model is signed i64, where the cast is
    // identity.
    let acc_var = match (strip_cast(lhs), strip_cast(rhs)) {
        (ArithAst::Var(x), ArithAst::Var(y)) if x == &acc => y.clone(),
        (ArithAst::Var(y), ArithAst::Var(x)) if x == &acc => y.clone(),
        (ArithAst::Ident(x), ArithAst::Var(y)) if x == &acc => y.clone(),
        (ArithAst::Var(y), ArithAst::Ident(x)) if x == &acc => y.clone(),
        (ArithAst::Ident(x), ArithAst::Ident(y)) if x == &acc => y.clone(),
        _ => return None,
    };
    if !init_names.contains(&acc_var) {
        return None;
    }
    // The acc is written ONLY by the tail (inits/chain never touch it).
    if inits.iter().any(|(n, _)| n == &acc) || writes.contains(&acc) {
        return None;
    }
    let mut private: Vec<String> = Vec::new();
    for (n, _) in inits.iter() {
        if !private.contains(n) {
            private.push(n.clone());
        }
    }
    Some(SeqChain {
        private,
        inits,
        while_idx: w,
        tail_idx: w + 1,
        acc,
        acc_var,
    })
}

/// Peel frontend `Cast` markers (identity for the signed-i64 lane model).
fn strip_cast(a: &ArithAst) -> &ArithAst {
    match a {
        ArithAst::Cast { arg, .. } => strip_cast(arg),
        other => other,
    }
}

/// One flat assign-block of a chain arm: every stmt a scalar Assign
/// with pure-arith RHS over readable vars; records written vars.
/// Anything else (nested ifs/loops/non-arith exprs) refuses.
fn check_assign_block(
    stmts: &[IrStmt],
    counter: &str,
    inv: &BTreeSet<&str>,
    readable: &BTreeSet<String>,
    writes: &mut BTreeSet<String>,
) -> bool {
    for a in stmts.iter() {
        let IrStmt::Assign { targets, expr, .. } = a else {
            return false;
        };
        let [t] = targets.as_slice() else {
            return false;
        };
        if !t.indices.is_empty()
            || t.var.contains('[')
            || &t.var == counter
            || inv.contains(t.var.as_str())
        {
            return false;
        }
        let IrExpr::Arith(x) = expr else {
            return false;
        };
        if !pure_chain_arith(x) {
            return false;
        }
        let mut free = BTreeSet::new();
        arith_free_vars(x, &mut free);
        if free.iter().any(|v| !readable.contains(v) || !is_plain_scalar(v)) {
            return false;
        }
        writes.insert(t.var.clone());
    }
    true
}

/// Init RHS: literal or pure arith (GPU-executable: no calls possible in
/// ArithAst; forbid Index/Assign/IncDec).
fn init_rhs(expr: &IrExpr) -> Option<InitRhs> {
    match expr {
        IrExpr::Int(n) => Some(InitRhs::Num(*n)),
        IrExpr::Str(s, _) => s.trim().parse::<i64>().ok().map(InitRhs::Num),
        IrExpr::Arith(a) => {
            if pure_chain_arith(a) {
                Some(InitRhs::Arith((**a).clone()))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn init_rhs_free(rhs: &InitRhs, out: &mut BTreeSet<String>) {
    match rhs {
        InitRhs::Num(_) => {}
        InitRhs::Arith(a) => arith_free_vars(a, out),
    }
}

/// Chain-arith purity: no Index (array reads can't run per-lane without
/// buffers — V2), no Assign/IncDec (side-effecting). Calls are
/// unrepresentable in ArithAst; Sizeof/Cast deterministic.
fn pure_chain_arith(a: &ArithAst) -> bool {
    match a {
        ArithAst::Index { .. } | ArithAst::Assign { .. } | ArithAst::IncDec { .. } => false,
        ArithAst::Bin { lhs, rhs, .. } => pure_chain_arith(lhs) && pure_chain_arith(rhs),
        ArithAst::Un { arg, .. } => pure_chain_arith(arg),
        ArithAst::Cond { test, then, else_ } => {
            pure_chain_arith(test) && pure_chain_arith(then) && pure_chain_arith(else_)
        }
        ArithAst::Cast { arg, .. } => pure_chain_arith(arg),
        _ => true,
    }
}

fn arith_free_vars(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => {
            out.insert(v.clone());
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_free_vars(lhs, out);
            arith_free_vars(rhs, out);
        }
        ArithAst::Un { arg, .. } => arith_free_vars(arg, out),
        ArithAst::Cond { test, then, else_ } => {
            arith_free_vars(test, out);
            arith_free_vars(then, out);
            arith_free_vars(else_, out);
        }
        ArithAst::Assign { var, rhs, .. } => {
            out.insert(var.clone());
            arith_free_vars(rhs, out);
        }
        ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        ArithAst::Cast { arg, .. } => arith_free_vars(arg, out),
        ArithAst::Index { var, key } => {
            out.insert(var.clone());
            arith_free_vars(key, out);
        }
        _ => {}
    }
}

/// Plain scalar name (twin of transforms::fuse_fill_consume's —
/// excludes `$?`, `$@`, positionals, non-identifiers).
fn is_plain_scalar(name: &str) -> bool {
    let mut cs = name.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A pure boolean test over readable vars: builtin/exec-let TEXT
/// (audited; twin of fuse's carve-out) or an Arith comparison with pure
/// sides. Anything else (test calls, `[ … ]`) vetoes.
fn test_pure(cond: &IrExpr, readable: &BTreeSet<String>) -> bool {
    if let Some(texts) = let_texts(cond) {
        return texts.iter().all(|t| let_text_pure(t))
            && texts.iter().all(|t| {
                let mut names = BTreeSet::new();
                lex_names(t, &mut names);
                names.iter().all(|n| readable.contains(n))
            });
    }
    // Only Arith comparisons besides let-text (test calls, `[ … ]`,
    // interpolation and friends veto — no call-scan needed since
    // anything unlisted here refuses).
    match cond {
        IrExpr::Arith(a) => {
            let ArithAst::Bin { op, lhs, rhs } = a.as_ref() else {
                return false;
            };
            if !matches!(op.as_str(), "<" | ">" | "<=" | ">=" | "==" | "!=") {
                return false;
            }
            if !pure_chain_arith(lhs) || !pure_chain_arith(rhs) {
                return false;
            }
            let mut free = BTreeSet::new();
            arith_free_vars(lhs, &mut free);
            arith_free_vars(rhs, &mut free);
            free.iter().all(|v| readable.contains(v) && is_plain_scalar(v))
        }
        _ => false,
    }
}

/// Code texts of a builtin/exec-let call, or None (not that shape).
fn let_texts(cond: &IrExpr) -> Option<Vec<String>> {
    let IrExpr::Call { func, args } = cond else {
        return None;
    };
    if func != "builtin" && func != "exec" {
        return None;
    }
    match args.first() {
        Some(IrExpr::Str(name, _)) if name == "let" => {}
        _ => return None,
    }
    let mut out = Vec::new();
    for a in args.iter().skip(1) {
        match a {
            IrExpr::Str(s, _) => out.push(s.clone()),
            IrExpr::Array(els) => {
                for e in els {
                    if let IrExpr::Str(s, _) = e {
                        out.push(s.clone());
                    }
                }
            }
            _ => {}
        }
    }
    Some(out)
}

/// Inert arithmetic text: no `$`/backtick expansions, no `=`/`++`/`--`
/// after comparison-op removal (twin of fuse's audit).
fn let_text_pure(text: &str) -> bool {
    if text.contains('$') || text.contains('`') {
        return false;
    }
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut t = t;
    for op in ["<=", ">=", "==", "!="] {
        t = t.replace(op, "");
    }
    !(t.contains('=') || t.contains("++") || t.contains("--"))
}

/// Lex identifier-like names from code text.
fn lex_names(text: &str, out: &mut BTreeSet<String>) {
    for tok in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if is_plain_scalar(tok) {
            out.insert(tok.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;

    fn scalar(var: &str) -> crate::ir::AssignTarget {
        crate::ir::AssignTarget { var: var.to_string(), sigil: None, indices: vec![] }
    }

    fn assign(var: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign { targets: vec![scalar(var)], expr, asm: None }
    }

    fn str_lit(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn arith(a: ArithAst) -> IrExpr {
        IrExpr::Arith(Box::new(a))
    }

    fn v(name: &str) -> ArithAst {
        ArithAst::Var(name.to_string())
    }

    fn let_call(text: &str) -> IrExpr {
        IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![str_lit("let"), IrExpr::Array(vec![str_lit(text)])],
        }
    }

    /// Collatz outer body: v=seed; s=0; while(v>1){ if(v%2==0){v=v/2} else {v=3v+1}; s++ }; total+=s.
    fn collatz_body() -> Vec<IrStmt> {
        let seed = arith(ArithAst::Bin {
            op: "%".to_string(),
            lhs: Box::new(ArithAst::Bin {
                op: "+".to_string(),
                lhs: Box::new(ArithAst::Bin {
                    op: "*".to_string(),
                    lhs: Box::new(v("k")),
                    rhs: Box::new(ArithAst::Num(37)),
                }),
                rhs: Box::new(ArithAst::Num(3)),
            }),
            rhs: Box::new(ArithAst::Num(251)),
        });
        let halve = assign("v", arith(ArithAst::Bin {
            op: "/".to_string(),
            lhs: Box::new(v("v")),
            rhs: Box::new(ArithAst::Num(2)),
        }));
        let triple = assign("v", arith(ArithAst::Bin {
            op: "+".to_string(),
            lhs: Box::new(ArithAst::Bin {
                op: "*".to_string(),
                lhs: Box::new(ArithAst::Num(3)),
                rhs: Box::new(v("v")),
            }),
            rhs: Box::new(ArithAst::Num(1)),
        }));
        let chain = IrStmt::While {
            cond: let_call("v > 1"),
            body: vec![
                IrStmt::If {
                    cond: let_call("v % 2 == 0"),
                    then: vec![halve],
                    elsifs: vec![],
                    else_: vec![triple],
                },
                assign("s", arith(ArithAst::Bin {
                    op: "+".to_string(),
                    lhs: Box::new(v("s")),
                    rhs: Box::new(ArithAst::Num(1)),
                })),
            ],
        };
        let tail = assign("total", arith(ArithAst::Bin {
            op: "+".to_string(),
            lhs: Box::new(v("total")),
            rhs: Box::new(v("s")),
        }));
        vec![assign("v", seed), assign("s", str_lit("0")), chain, tail]
    }

    #[test]
    fn collatz_classifies() {
        let body = collatz_body();
        let c = classify_seq_chain(&body, "k", &["n"]).expect("collatz is a lane-nest");
        assert_eq!(c.private, vec!["v".to_string(), "s".to_string()]);
        assert_eq!(c.acc, "total");
        assert_eq!(c.acc_var, "s");
        assert_eq!(c.while_idx, 2);
        assert_eq!(c.tail_idx, 3);
        assert!(matches!(c.inits[1], (_, InitRhs::Num(0))));
    }

    #[test]
    fn cast_wrapped_collatz_classifies() {
        // The Python frontend wraps bigint-domain reads in `Cast(Int64, …)`
        // (its exactness marker). The lane model is signed i64, so the
        // accumulator match must peel casts — otherwise the additive
        // growth guard (docs/PY_BENCH.md finding E) would silently
        // disable the CUDA seq path for collatz.
        let cast = |a: ArithAst| ArithAst::Cast {
            ty: crate::ir::IrType::Int64,
            arg: Box::new(a),
        };
        let mut body = collatz_body();
        // tail: total = Cast(total) + Cast(s)
        body[3] = assign(
            "total",
            arith(ArithAst::Bin {
                op: "+".to_string(),
                lhs: Box::new(cast(v("total"))),
                rhs: Box::new(cast(v("s"))),
            }),
        );
        // chain: s = Cast(s) + 1
        let IrStmt::While { body: wbody, .. } = &mut body[2] else {
            panic!("shape");
        };
        wbody[1] = assign(
            "s",
            arith(ArithAst::Bin {
                op: "+".to_string(),
                lhs: Box::new(cast(v("s"))),
                rhs: Box::new(ArithAst::Num(1)),
            }),
        );
        let c = classify_seq_chain(&body, "k", &["n"])
            .expect("cast-wrapped collatz is still a lane-nest");
        assert_eq!(c.acc, "total");
        assert_eq!(c.acc_var, "s");
    }

    #[test]
    fn chain_writes_uninit_vetoes() {
        // Chain writes `q` which no init defines (carries across iters).
        let mut body = collatz_body();
        let IrStmt::While { body: wbody, .. } = &mut body[2] else {
            panic!("shape");
        };
        wbody.push(assign("q", arith(ArithAst::Num(1))));
        assert!(classify_seq_chain(&body, "k", &["n"]).is_none());
    }

    #[test]
    fn acc_written_in_chain_vetoes() {
        let mut body = collatz_body();
        let IrStmt::While { body: wbody, .. } = &mut body[2] else {
            panic!("shape");
        };
        wbody.push(assign("total", arith(ArithAst::Num(0))));
        assert!(classify_seq_chain(&body, "k", &["n"]).is_none());
    }

    #[test]
    fn tail_not_last_vetoes() {
        let mut body = collatz_body();
        body.push(assign("z", str_lit("1")));
        assert!(classify_seq_chain(&body, "k", &["n"]).is_none());
    }

    #[test]
    fn forward_ref_init_vetoes() {
        // `v` seed reads `s` before `s` is defined (previous-iteration flow).
        let mut body = collatz_body();
        body[0] = assign("v", arith(v("s")));
        assert!(classify_seq_chain(&body, "k", &["n"]).is_none());
    }

    #[test]
    fn call_in_chain_vetoes() {
        // Capture/call nodes can't execute per-lane on the device.
        let mut body = collatz_body();
        let IrStmt::While { body: wbody, .. } = &mut body[2] else {
            panic!("shape");
        };
        wbody.push(assign(
            "v",
            IrExpr::Call { func: "capture".to_string(), args: vec![str_lit("x")] },
        ));
        assert!(classify_seq_chain(&body, "k", &["n"]).is_none());
    }

    #[test]
    fn output_in_chain_vetoes() {
        let mut body = collatz_body();
        let IrStmt::While { body: wbody, .. } = &mut body[2] else {
            panic!("shape");
        };
        wbody.push(IrStmt::Output {
            value: str_lit("hi"),
            newline: true,
            target: None,
        });
        assert!(classify_seq_chain(&body, "k", &["n"]).is_none());
    }

    #[test]
    fn counter_write_in_chain_vetoes() {
        let mut body = collatz_body();
        let IrStmt::While { body: wbody, .. } = &mut body[2] else {
            panic!("shape");
        };
        wbody.push(assign("k", arith(ArithAst::Num(0))));
        assert!(classify_seq_chain(&body, "k", &["n"]).is_none());
    }
}
