//! redundant-store-elim — drop an assignment whose value is overwritten
//! by a LATER store to the same scalar variable before anything can read
//! it (`x=A; …; x=B` with no read of x in between). The complement of
//! dead-store-elim (which needs ZERO reads anywhere) — here the var IS
//! read later, but an intermediate store is dead.
//!
//! ## Need
//! Real and generated code reassigns a scalar (`x="$a"; …; x="$b"`); each
//! write emits a store. Only the last write before a read matters.
//!
//! ## Scope — the sound rule
//! A scalar store `x=A` at index `i` is dropped iff ALL of:
//!   - `A` is PURE — no `Capture`, `Arrow`, `Call`, `MethodCall` (an
//!     impure store's side effect must not be skipped),
//!   - there is a LATER scalar store to the same `x` in the same block,
//!   - NO read of `x` and NO "indirect observer" (a function `Call`,
//!     `Capture`, `Subshell`, `Background`, `Exec`, `WriteFile`,
//!     `setVar`) occurs strictly between `i` and that later store.
//! Dropping the earlier store is then unconditionally safe: its value is
//! replaced before any possible observation, whether `x` is a local or a
//! shared-store global (the later store is what any caller observes).
//! The LAST store in a block is never dropped (it is the surviving
//! value). Trailing-stores-before-end-of-function are likewise kept — a
//! later store is REQUIRED.
//!
//! ## Placement
//! Bundle for run_core_worker.sh (register + manifest). Registered in
//! `transforms.rs` (SH2_TRANSFORMS gated). Prereq: function-purity
//! (the pure-expression gate; a local conservative fallback is used here).

use crate::ir::{ArithAst, IrExpr, IrStmt};

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut c = false;
    // top-level list
    c |= drop_redundant(stmts);
    // function bodies (recurse)
    let mut i = 0;
    while i < stmts.len() {
        if let IrStmt::Function { name: _, body, .. } = &mut stmts[i] {
            c |= drop_redundant(body);
        }
        i += 1;
    }
    c
}

fn drop_redundant(stmts: &mut Vec<IrStmt>) -> bool {
    let n = stmts.len();
    let mut remove = vec![false; n];
    for i in 0..n {
        let var = match scalar_pure_assign(&stmts[i]) {
            Some(v) => v,
            None => continue,
        };
        // scan forward: any read/observer stops it; a later scalar store
        // to the same var makes store-i dead
        let mut j = i + 1;
        while j < n {
            if stmt_reads(&stmts[j], &var) || indirect_observer(&stmts[j]) {
                break;
            }
            if scalar_assign_to(&stmts[j], &var) {
                remove[i] = true;
                break;
            }
            j += 1;
        }
    }
    let changed = remove.iter().any(|&r| r);
    *stmts = stmts
        .drain(..)
        .enumerate()
        .filter_map(|(i, s)| if remove[i] { None } else { Some(s) })
        .collect();
    changed
}

/// A single scalar-target Assign whose value is pure; returns the var.
fn scalar_pure_assign(st: &IrStmt) -> Option<String> {
    match st {
        IrStmt::Assign { targets, expr, .. } if targets.len() == 1 && targets[0].indices.is_empty() => {
            if expr_pure(expr) {
                Some(targets[0].var.clone())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// A scalar Assign to the given var (purity not required — a later
/// impure store still overwrites the value; we drop the EARLIER pure one).
fn scalar_assign_to(st: &IrStmt, var: &str) -> bool {
    match st {
        IrStmt::Assign { targets, .. } => {
            targets.len() == 1 && targets[0].indices.is_empty() && targets[0].var == var
        }
        _ => false,
    }
}

fn expr_pure(e: &IrExpr) -> bool {
    match e {
        IrExpr::Int(_) | IrExpr::Var(_, _) | IrExpr::Str(_, _) | IrExpr::Range { .. } | IrExpr::Bool(_) => true,
        IrExpr::BinOp { lhs, rhs, .. } => expr_pure(lhs) && expr_pure(rhs),
        // Arithmetic is pure ONLY when it performs no assignment: an
        // `Assign`/`IncDec` node mutates its variable (the value AND the
        // side effect would both be lost if the store were dropped).
        // Same rule as dead_store_elim's `arith_pure`.
        IrExpr::Arith(a) => arith_pure(a),
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            crate::ir::InterpPart::Lit(_) => true,
            crate::ir::InterpPart::Expr(x) => expr_pure(x),
        }),
        _ => false, // Call / Capture / Arrow / MethodCall / array — refuse
    }
}

/// Whether an arithmetic tree is free of assignments (mirrors
/// dead_store_elim's `arith_pure`: `Assign`/`IncDec` mutate state).
fn arith_pure(a: &crate::ir::ArithAst) -> bool {
    match a {
        crate::ir::ArithAst::Num(_)
        | crate::ir::ArithAst::Var(_)
        | crate::ir::ArithAst::Ident(_)
        | crate::ir::ArithAst::Sizeof(_) => true,
        crate::ir::ArithAst::Index { key, .. } => arith_pure(key),
        crate::ir::ArithAst::Bin { lhs, rhs, .. } => arith_pure(lhs) && arith_pure(rhs),
        crate::ir::ArithAst::Un { arg, .. } => arith_pure(arg),
        crate::ir::ArithAst::Cond { test, then, else_, .. } => {
            arith_pure(test) && arith_pure(then) && arith_pure(else_)
        }
        crate::ir::ArithAst::Cast { arg, .. } => arith_pure(arg),
        crate::ir::ArithAst::Assign { .. } | crate::ir::ArithAst::IncDec { .. } => false,
    }
}

fn stmt_reads(st: &IrStmt, var: &str) -> bool {
    match st {
        // An indexed store (`arr[x]=…`) READS its index expressions, so
        // a var used only there still observes an earlier store of it.
        IrStmt::Assign { targets, expr, .. } => {
            expr_reads(expr, var)
                || targets
                    .iter()
                    .any(|t| t.indices.iter().any(|k| expr_reads(k, var)))
        }
        IrStmt::Output { value, .. } => expr_reads(value, var),
        IrStmt::WriteFile { path, content, .. } => expr_reads(path, var) || expr_reads(content, var),
        IrStmt::Declare { init, .. } => init.as_ref().map(|i| expr_reads(i, var)).unwrap_or(false),
        IrStmt::DeclareArray { elements, .. } => elements.iter().any(|e| expr_reads(e, var)),
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            expr_reads(cond, var)
                || then.iter().any(|s| stmt_reads(s, var))
                || elsifs.iter().any(|(c, b)| expr_reads(c, var) || b.iter().any(|s| stmt_reads(s, var)))
                || else_.iter().any(|s| stmt_reads(s, var))
        }
        IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, .. } => {
            expr_reads(cond, var) || body.iter().any(|s| stmt_reads(s, var))
        }
        IrStmt::DoWhile { body, cond, .. } => {
            body.iter().any(|s| stmt_reads(s, var)) || expr_reads(cond, var)
        }
        IrStmt::Exec { cmd, args, redirects, env, .. } => {
            expr_reads(cmd, var)
                || args.iter().any(|a| expr_reads(a, var))
                || redirects.iter().any(|r| expr_reads(r, var))
                || env.iter().any(|(_, e)| expr_reads(e, var))
        }
        IrStmt::Pipeline { stages, .. } => {
            stages.iter().any(|s| s.iter().any(|st| stmt_reads(st, var)))
        }
        IrStmt::Return(e) => e.as_ref().map(|e| expr_reads(e, var)).unwrap_or(false),
        IrStmt::Exit(e) => e.as_ref().map(|e| expr_reads(e, var)).unwrap_or(false),
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => expr_reads(expr, var),
        IrStmt::SetChildError(e) => expr_reads(e, var),
        IrStmt::Case { discriminant, clauses, .. } => {
            expr_reads(discriminant, var)
                || clauses.iter().any(|c| c.body.iter().any(|s| stmt_reads(s, var)))
        }
        IrStmt::Redirect { inner, redirects, .. } => {
            inner.iter().any(|s| stmt_reads(s, var))
                || redirects.iter().any(|r| expr_reads(&r.target, var))
        }
        IrStmt::Function { body, .. } => body.iter().any(|s| stmt_reads(s, var)),
        IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => {
            b.iter().any(|s| stmt_reads(s, var))
        }
        IrStmt::Expr(e) => expr_reads(e, var),
        _ => false,
    }
}

fn expr_reads(e: &IrExpr, var: &str) -> bool {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => v == var,
        IrExpr::Index { var: v, key, .. } => v == var || expr_reads(key, var),
        IrExpr::BinOp { lhs, rhs, .. } => expr_reads(lhs, var) || expr_reads(rhs, var),
        IrExpr::Arith(a) => arith_reads(a, var),
        IrExpr::Call { func, args, .. } => {
            call_reads_var(func, args, var)
                || assign_rmw_reads(func, args, var)
                || args.iter().any(|a| expr_reads(a, var))
                // A `$name` reference inside ANY string argument is a
                // read — `eval 'echo $x'` carries its variable inside a
                // plain Str node, invisible to the walk above. Without
                // this, `x=1; y=$(eval 'echo $x'); x=2` dropped `x=1`
                // and the eval read the overwritten value. Mirrors
                // dead_store_elim's `string_read_names`.
                || args.iter().any(|a| match a {
                    IrExpr::Str(s, _) => str_reads_var(s, var),
                    _ => false,
                })
        }
        IrExpr::MethodCall { obj, args, .. } => expr_reads(obj, var) || args.iter().any(|a| expr_reads(a, var)),
        IrExpr::Ternary { cond, then, else_, .. } => {
            expr_reads(cond, var) || expr_reads(then, var) || expr_reads(else_, var)
        }
        IrExpr::DefinedOr { expr, default, .. } => expr_reads(expr, var) || expr_reads(default, var),
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            crate::ir::InterpPart::Expr(x) => expr_reads(x, var),
            _ => false,
        }),
        IrExpr::Capture { expr, .. } => expr_reads(expr, var),
        IrExpr::Arrow(body) => body.iter().any(|s| stmt_reads(s, var)),
        IrExpr::Array(elems) => elems.iter().any(|e| expr_reads(e, var)),
        IrExpr::Object(fields) => fields.iter().any(|(_, e)| expr_reads(e, var)),
        // A bare Str node can carry `$name` references too — the heredoc
        // target (`cat << EOF` with `Hello $name`) arrives as a Str with
        // interpolate:true, not a Call arg, so the scan above never sees
        // it. Mirrors dead_store_elim's Str arm (079_heredoc case).
        IrExpr::Str(s, _) => str_reads_var(s, var),
        _ => false,
    }
}

fn arith_reads(a: &ArithAst, var: &str) -> bool {
    match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => v == var,
        ArithAst::Index { var: v, key, .. } => v == var || arith_reads(key, var),
        ArithAst::Bin { lhs, rhs, .. } => arith_reads(lhs, var) || arith_reads(rhs, var),
        ArithAst::Un { arg, .. } => arith_reads(arg, var),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_reads(test, var) || arith_reads(then, var) || arith_reads(else_, var)
        }
        ArithAst::Assign { var: v, op, rhs } => {
            arith_reads(rhs, var) || (op != "=" && v == var)
        }
        ArithAst::IncDec { var: v, .. } => v == var,
        ArithAst::Cast { arg, .. } => arith_reads(arg, var),
        _ => false,
    }
}

/// `assign(V, OP, …)` with a compound `OP` is a read-modify-write of `V`
/// (bash's scalar `+=` string-appends, the arithmetic ops combine with the
/// current value). Without counting it as a read, `n=1; n+=2` looked like
/// two plain stores with nothing in between, so this pass dropped the base
/// write (zig-20261021-param-lowering-gaps §3).
fn assign_rmw_reads(func: &str, args: &[IrExpr], var: &str) -> bool {
    if func != "assign" {
        return false;
    }
    match (args.first(), args.get(1)) {
        (Some(IrExpr::Str(v, _)), Some(IrExpr::Str(op, _))) => v == var && op != "=",
        _ => false,
    }
}

/// Runtime-call variable access: `getVar`/`arrayIndex` read their name
/// arg, `param` reads its second arg, and the string-eval calls
/// (`arith`/`test`/`caseMatch`) may reference vars by name inside their
/// string payloads.
fn call_reads_var(func: &str, args: &[IrExpr], var: &str) -> bool {
    if matches!(func, "getVar" | "arrayIndex") {
        if let Some(IrExpr::Str(n, _)) = args.first() {
            return n == var;
        }
    }
    if func == "param" {
        if let Some(IrExpr::Str(n, _)) = args.get(1) {
            return n == var;
        }
    }
    if matches!(func, "arith" | "test" | "caseMatch") {
        return args.iter().any(|a| match a {
            IrExpr::Str(s, _) => str_maybe_reads(s, var),
            _ => false,
        });
    }
    false
}

/// Does the string reference `var` as a `$name` / `${name}` expansion?
/// Predicate twin of dead_store_elim's `string_read_names` (which fills
/// a set; here the scan answers for one variable): records the name even
/// when the `}` is missing or delayed by expansion operators
/// (`${x:-d}`, `${MAXWAIT% *}`).
fn str_reads_var(s: &str, var: &str) -> bool {
    if var.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    let vb = var.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'$' || i + 1 >= bytes.len() {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        if bytes[j] == b'{' {
            j += 1;
            // `${#name}` / `${!name}` prefix operators
            if j < bytes.len() && (bytes[j] == b'#' || bytes[j] == b'!') {
                j += 1;
            }
        }
        let start = j;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
        }
        if j > start
            && (bytes[start].is_ascii_alphabetic() || bytes[start] == b'_')
            && &bytes[start..j] == vb
        {
            return true;
        }
        // No match at this `$`: advance past it (not past j — a `$` inside
        // the scanned run, e.g. `$x$y`, must be re-examined as an anchor).
        i += 1;
    }
    false
}

/// Does the string reference `var` as a standalone token?
fn str_maybe_reads(s: &str, var: &str) -> bool {
    if var.is_empty() {
        return false;
    }
    let mut rest = s;
    while let Some(pos) = rest.find(var) {
        let before = pos == 0
            || !rest[..pos]
                .chars()
                .next_back()
                .map(|c| c.is_alphanumeric())
                .unwrap_or(false);
        let after = pos + var.len() >= rest.len()
            || !rest[pos + var.len()..]
                .chars()
                .next()
                .map(|c| c.is_alphanumeric())
                .unwrap_or(false);
        if before && after {
            return true;
        }
        rest = &rest[pos + var.len()..];
    }
    false
}

/// A statement that could read `x` indirectly (a call/capture/subshell/
/// exec/write) — conservatively stops the drop-scan.
fn indirect_observer(st: &IrStmt) -> bool {
    matches!(
        st,
        IrStmt::Exec { .. }
            | IrStmt::Subshell(_)
            | IrStmt::Background(_)
            | IrStmt::WriteFile { .. }
            | IrStmt::Expr(
                IrExpr::Call { .. }
                    | IrExpr::Capture { .. }
                    | IrExpr::MethodCall { .. }
                    | IrExpr::Arrow(_)
            )
    ) || (matches!(st, IrStmt::Return(_) | IrStmt::Exit(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, StrStyle};

    fn assign(var: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: var.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        }
    }

    fn str_lit(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
    }

    fn int_lit(n: i64) -> IrExpr {
        IrExpr::Int(n)
    }

    fn getvar(var: &str) -> IrExpr {
        IrExpr::Call {
            func: "getVar".to_string(),
            args: vec![str_lit(var)],
        }
    }

    fn expr_stmt(e: IrExpr) -> IrStmt {
        IrStmt::Expr(e)
    }

    /// `x=A; y="v$x"; x=B` — the intermediate `y="v$x"` READS x via a
    /// getVar call inside an interpolation; the scan must stop there and
    /// keep the `x=A` store (its value is observed).
    #[test]
    fn keeps_store_read_via_getvar_in_assign_rhs() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            assign(
                "y",
                IrExpr::Interpolate(vec![
                    crate::ir::InterpPart::Lit("v".to_string()),
                    crate::ir::InterpPart::Expr(Box::new(getvar("x"))),
                ]),
            ),
            assign("x", int_lit(6)),
        ];
        // the first store must survive: y observes x=5
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[0], assign("x", int_lit(5)));
        assert_eq!(stmts.len(), 3);
    }

    /// `x=A; x=B` with NO read between — the intermediate store drops.
    #[test]
    fn drops_store_with_no_read_between() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            assign("x", int_lit(6)),
        ];
        assert!(transform(&mut stmts));
        assert_eq!(stmts.len(), 1);
        if let IrStmt::Assign { expr, .. } = &stmts[0] {
            assert_eq!(expr, &int_lit(6));
        } else {
            panic!("expected Assign");
        }
    }

    /// `x=A; echo $y` where the echo reads x via getVar inside an exec
    /// arg — the store must survive.
    #[test]
    fn keeps_store_read_via_exec_arg() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            expr_stmt(IrExpr::Call {
                func: "exec".to_string(),
                args: vec![
                    str_lit("echo"),
                    IrExpr::Array(vec![IrExpr::Interpolate(vec![
                        crate::ir::InterpPart::Lit("v".to_string()),
                        crate::ir::InterpPart::Expr(Box::new(getvar("x"))),
                    ])]),
                ],
            }),
            assign("x", int_lit(6)),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[0], assign("x", int_lit(5)));
    }

    /// `x=A; if cond; then echo $x; fi; x=B` — the conditional read of x
    /// must stop the scan.
    #[test]
    fn keeps_store_read_inside_if() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            IrStmt::If {
                cond: getvar("c"),
                then: vec![expr_stmt(getvar("x"))],
                elsifs: vec![],
                else_: vec![],
            },
            assign("x", int_lit(6)),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[0], assign("x", int_lit(5)));
    }

    /// `x=A; y=$((x+1)); x=B` — the RHS reads x via native arith.
    #[test]
    fn keeps_store_read_via_arith() {
        let mut stmts = vec![
            assign("x", int_lit(5)),
            assign(
                "y",
                IrExpr::Arith(Box::new(ArithAst::Bin {
                    op: "+".to_string(),
                    lhs: Box::new(ArithAst::Var("x".to_string())),
                    rhs: Box::new(ArithAst::Num(1)),
                })),
            ),
            assign("x", int_lit(6)),
        ];
        assert!(!transform(&mut stmts));
        assert_eq!(stmts[0], assign("x", int_lit(5)));
    }

    /// `n=1; n+=2` — a compound assignment is a READ-MODIFY-WRITE: the
    /// base store feeds it, so it must NOT be treated as an overwritten
    /// dead store. Before the RMW read was counted, this dropped `n=1`
    /// and every backend computed from an unset variable
    /// (zig-20261021-param-lowering-gaps §3).
    #[test]
    fn keeps_base_store_for_compound_assignment() {
        let plus_assign = |var: &str, rhs: &str| IrStmt::Assign {
            targets: vec![AssignTarget {
                var: var.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Call {
                func: "assign".to_string(),
                args: vec![str_lit(var), str_lit("+="), str_lit(rhs)],
            },
            asm: None,
        };
        let mut stmts = vec![assign("n", str_lit("1")), plus_assign("n", "2")];
        assert!(!transform(&mut stmts), "the base write must survive");
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], assign("n", str_lit("1")));

        // Control: a plain `=` overwrite still makes the earlier store dead.
        let mut plain = vec![assign("n", str_lit("1")), assign("n", str_lit("2"))];
        assert!(transform(&mut plain));
        assert_eq!(plain.len(), 1);
    }

    /// Bug 1 (verus-rse): `x=$((i++)); x=<later>` — an arith RHS with a
    /// side effect (`Assign`/`IncDec`) is NOT pure, so the earlier store
    /// must not be dropped: dropping it skips the increment and every
    /// later read of `i` sees the wrong value. `expr_pure` claimed
    /// `Arith(_) => true`; dead_store_elim's `arith_pure` correctly
    /// rejects `Assign`/`IncDec`.
    #[test]
    fn keeps_store_with_side_effecting_arith_rhs() {
        let incdec = |var: &str| {
            IrExpr::Arith(Box::new(crate::ir::ArithAst::IncDec {
                var: var.to_string(),
                delta: 1,
                prefix: false,
            }))
        };
        // `x=$((i++)); x=6` — dropping `x=$((i++))` would skip the
        // increment of i. The store must survive (it is impure).
        let mut stmts = vec![assign("x", incdec("i")), assign("x", int_lit(6))];
        assert!(
            !transform(&mut stmts),
            "a side-effecting arith RHS is not a droppable pure store"
        );
        assert_eq!(stmts.len(), 2);

        // Control: `$((i + 1))` (no Assign/IncDec) IS pure and still drops.
        let pure_arith = IrExpr::Arith(Box::new(crate::ir::ArithAst::Bin {
            op: "+".to_string(),
            lhs: Box::new(crate::ir::ArithAst::Var("i".to_string())),
            rhs: Box::new(crate::ir::ArithAst::Num(1)),
        }));
        let mut pure = vec![assign("x", pure_arith), assign("x", int_lit(6))];
        assert!(transform(&mut pure));
        assert_eq!(pure.len(), 1);
    }

    /// Bug 2 (verus-rse): `x=5; arr[x]=1; x=6` — the middle store's    /// TARGET INDEX reads `x`, so the earlier store is observed and must
    /// survive. `stmt_reads` on `Assign` checked only the RHS and missed
    /// target indices, dropping `x=5` and indexing `arr` by the unset
    /// (empty) value instead.
    #[test]
    fn keeps_store_read_via_assign_target_index() {
        let indexed_assign = |arr: &str, idx: IrExpr, rhs: IrExpr| IrStmt::Assign {
            targets: vec![AssignTarget {
                var: arr.to_string(),
                sigil: None,
                indices: vec![idx],
            }],
            expr: rhs,
            asm: None,
        };
        let mut stmts = vec![
            assign("x", int_lit(5)),
            indexed_assign(
                "arr",
                IrExpr::Var("x".to_string(), None),
                int_lit(1),
            ),
            assign("x", int_lit(6)),
        ];
        assert!(
            !transform(&mut stmts),
            "the index read of x observes the earlier store"
        );
        assert_eq!(stmts.len(), 3);
        assert_eq!(stmts[0], assign("x", int_lit(5)));
    }

    /// Bug 3 (verus-rse): `x=1; y=$(eval 'echo $x'); x=2` — the `$x`
    /// inside the eval's plain-Str argument is a read of `x`, so the
    /// earlier store is observed and must survive. The Call-arm walk
    /// never looked inside Str nodes (only `arith`/`test`/`caseMatch`
    /// strings were scanned), and an `Assign` is never an indirect
    /// observer, so the scan sailed past the capture and dropped `x=1`.
    /// Mirrors dead_store_elim's `string_read_names`.
    #[test]
    fn keeps_store_read_via_str_arg_of_call() {
        let eval_call = IrExpr::Call {
            func: "builtin".to_string(),
            args: vec![str_lit("eval"), str_lit("echo $x")],
        };
        let cap = IrExpr::Capture {
            expr: Box::new(eval_call),
            native: false,
        };
        let mut stmts = vec![
            assign("x", str_lit("1")),
            assign("y", cap),
            assign("x", str_lit("2")),
        ];
        assert!(
            !transform(&mut stmts),
            "the $x inside the eval string observes the earlier store"
        );
        assert_eq!(stmts.len(), 3);
        assert_eq!(stmts[0], assign("x", str_lit("1")));

        // Unit pins for the scanner itself.
        assert!(super::str_reads_var("echo $x", "x"));
        assert!(super::str_reads_var("echo ${x}", "x"));
        assert!(super::str_reads_var("echo ${x:-d}", "x"));
        assert!(super::str_reads_var("$x$y", "y"));
        assert!(!super::str_reads_var("echo hi", "x"));
        assert!(!super::str_reads_var("echo $xy", "x"));
    }
}
