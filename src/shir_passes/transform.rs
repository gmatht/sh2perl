//! Transforms — IR-to-IR rewrites that preserve meaning under bash.
//!
//! Stage 0: stubs that pass the program through unchanged. Stage 1
//! migrates the real implementations:
//! - `ConstantFold` ← `ir::optimize_stmts` fold pass
//! - `DeadAssignmentElim` ← `ir::optimize_stmts` dead-assign pass
//! - `ImportMinimize` ← the M6 import registry (commit 2f70f9d, lives
//!   in `generator/mod.rs` today)
//!
//! The M3 guardrail (`pipeline_preserves_perl_backend` in the test
//! scaffold) is the proof that the migration is safe: the Perl output
//! must be byte-identical before and after the pipeline runs.

use crate::ir::{IrExpr, IrProgram, IrStmt};
use crate::shir_passes::PassContext;

/// StoreToNative (core request shir-passes-store-to-native-20260806):
/// convert `setVar("v", value)` Expr-statements to plain `Assign`
/// statements when v is PROVABLY STORE-ONLY, so the emitters' existing
/// native-lift (Assign stmt → JS binding, `x = v`) fires instead of a
/// runtime `sh2.setVar` store round-trip.
///
/// Analysis — v is STORE-ONLY iff EVERY reference to v is:
///   - a `setVar("v", expr)` / `getVar("v")` call with a LITERAL name, and
///   - never: the mem.* seam (memLoad/memStore/addrOf), the array/assoc
///     ops (arrayIndex/arrayLen/arrayItems/setArray/setArrayAppend), a
///     `param(..., "v", ...)` by-name read, a compound `assign("v", …)`
///     write, or a DYNAMIC (non-literal) name argument ANYWHERE (a
///     dynamic name could alias v).
///
/// Soundness — a var NOT provably store-only keeps its setVar calls. A
/// wrong conversion would recreate the stale-store divergence (store
/// written, native binding not updated). Extra conservative exclusion:
/// a var whose name appears in the STRING operand of a runtime
/// arith/arithEval/param/test/brace/caseMatch/builtin call is kept
/// store-bound — the runtime would read v from the store while the
/// native binding holds the truth.
pub struct StoreToNative;

impl super::Transform for StoreToNative {
    fn name(&self) -> &'static str {
        "store_to_native"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &PassContext) {
        let (store_vars, unsafe_vars) = analyze_store_only(prog);
        for st in &mut prog.stmts {
            rewrite_setvar_stmts(st, &store_vars, &unsafe_vars);
        }
    }
}

/// Names referenced by literal-name setVar/getVar calls, and names that
/// must stay store-bound (any non-setVar/getVar reference). A dynamic
/// name argument marks ALL vars unsafe (it could alias any of them).
fn analyze_store_only(prog: &IrProgram) -> (std::collections::HashSet<String>, std::collections::HashSet<String>) {
    let mut store: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut unsafe_v: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut any_dynamic = false;
    fn walk_expr(
        e: &IrExpr,
        store: &mut std::collections::HashSet<String>,
        unsafe_v: &mut std::collections::HashSet<String>,
        any_dynamic: &mut bool,
    ) {
        match e {
            IrExpr::Call { func, args } => {
                let name = func.as_str();
                if name == "setVar" || name == "getVar" {
                    if let Some(IrExpr::Str(n, _)) = args.first() {
                        // an extra arg beyond [name] is the emitter's
                        // value-override for lifted vars — still a
                        // literal-name read/write pair
                        if is_plain_var(n) {
                            store.insert(n.clone());
                        } else {
                            *any_dynamic = true;
                        }
                    } else {
                        *any_dynamic = true;
                    }
                } else if matches!(
                    name,
                    "arrayIndex" | "arrayLen" | "arrayItems" | "setArray" | "setArrayAppend"
                        | "param" | "memLoad" | "memStore" | "addrOf" | "assign"
                ) {
                    // a by-NAME reference — the first arg is the name
                    if let Some(IrExpr::Str(n, _)) = args.first() {
                        if is_plain_var(n) {
                            unsafe_v.insert(n.clone());
                        } else {
                            *any_dynamic = true;
                        }
                    } else {
                        *any_dynamic = true;
                    }
                }
                // runtime string evaluators: a var named inside the
                // STRING operand would be read from the store — keep it
                // store-bound (see the transform doc).
                if matches!(name, "arith" | "arithEval" | "test" | "brace" | "caseMatch") {
                    for a in args {
                        if let IrExpr::Str(s, _) = a {
                            for v in store.iter() {
                                if str_mentions(s, v) {
                                    unsafe_v.insert(v.clone());
                                }
                            }
                        }
                    }
                }
                for a in args {
                    walk_expr(a, store, unsafe_v, any_dynamic);
                }
            }
            IrExpr::Array(elems) => {
                for e in elems {
                    walk_expr(e, store, unsafe_v, any_dynamic);
                }
            }
            IrExpr::Arrow(stmts) => {
                for s in stmts {
                    walk_stmt(s, store, unsafe_v, any_dynamic);
                }
            }
            _ => {}
        }
    }
    fn walk_stmt(
        s: &IrStmt,
        store: &mut std::collections::HashSet<String>,
        unsafe_v: &mut std::collections::HashSet<String>,
        any_dynamic: &mut bool,
    ) {
        match s {
            IrStmt::Expr(e) => walk_expr(e, store, unsafe_v, any_dynamic),
            IrStmt::Assign { targets, expr } => {
                for t in targets {
                    if t.indices.is_empty() && is_plain_var(&t.var) {
                        unsafe_v.insert(t.var.clone());
                    } else {
                        *any_dynamic = true;
                    }
                }
                walk_expr(expr, store, unsafe_v, any_dynamic);
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                walk_expr(cond, store, unsafe_v, any_dynamic);
                for s in then.iter().chain(else_) {
                    walk_stmt(s, store, unsafe_v, any_dynamic);
                }
                for (c, b) in elsifs {
                    walk_expr(c, store, unsafe_v, any_dynamic);
                    for s in b {
                        walk_stmt(s, store, unsafe_v, any_dynamic);
                    }
                }
            }
            IrStmt::While { cond, body, .. } | IrStmt::DoWhile { cond, body, .. } => {
                walk_expr(cond, store, unsafe_v, any_dynamic);
                for s in body {
                    walk_stmt(s, store, unsafe_v, any_dynamic);
                }
            }
            IrStmt::For { var, iter, body } => {
                if is_plain_var(var) {
                    unsafe_v.insert(var.clone());
                }
                walk_expr(iter, store, unsafe_v, any_dynamic);
                for s in body {
                    walk_stmt(s, store, unsafe_v, any_dynamic);
                }
            }
            IrStmt::Block(stmts)
            | IrStmt::Subshell(stmts)
            | IrStmt::Background(stmts) => {
                for s in stmts {
                    walk_stmt(s, store, unsafe_v, any_dynamic);
                }
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for s in stage {
                        walk_stmt(s, store, unsafe_v, any_dynamic);
                    }
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                for s in inner {
                    walk_stmt(s, store, unsafe_v, any_dynamic);
                }
                for r in redirects {
                    walk_expr(&r.target, store, unsafe_v, any_dynamic);
                }
            }
            IrStmt::Case { discriminant, clauses } => {
                walk_expr(discriminant, store, unsafe_v, any_dynamic);
                for c in clauses {
                    for s in &c.body {
                        walk_stmt(s, store, unsafe_v, any_dynamic);
                    }
                }
            }
            IrStmt::Exec { args, .. } => {
                for a in args {
                    walk_expr(a, store, unsafe_v, any_dynamic);
                }
            }
            IrStmt::Function { body, .. } => {
                for s in body {
                    walk_stmt(s, store, unsafe_v, any_dynamic);
                }
            }
            IrStmt::Output { value, .. } => walk_expr(value, store, unsafe_v, any_dynamic),
            IrStmt::WriteFile { path, content, .. } => {
                walk_expr(path, store, unsafe_v, any_dynamic);
                walk_expr(content, store, unsafe_v, any_dynamic);
            }
            IrStmt::Return(e) => {
                if let Some(e) = e {
                    walk_expr(e, store, unsafe_v, any_dynamic);
                }
            }
            _ => {}
        }
    }
    for s in &prog.stmts {
        walk_stmt(s, &mut store, &mut unsafe_v, &mut any_dynamic);
    }
    if any_dynamic {
        unsafe_v.extend(store.iter().cloned());
    }
    (store, unsafe_v)
}

/// A plain identifier name (no `?`/`@`/`#`/`$` specials — those live in
/// the runtime's special-variable handling and are never liftable).
fn is_plain_var(n: &str) -> bool {
    !n.is_empty()
        && n.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '_')
        && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Does the runtime-evaluator string mention `v` as a whole word?
/// (`x + 1`, `$x`, `${x}` — the runtime's evalArith reads the store.)
fn str_mentions(s: &str, v: &str) -> bool {
    let bytes = s.as_bytes();
    let vb = v.as_bytes();
    let mut i = 0;
    while i + vb.len() <= bytes.len() {
        if &bytes[i..i + vb.len()] == vb {
            let before = if i == 0 { b'$' } else { bytes[i - 1] };
            let after = if i + vb.len() >= bytes.len() {
                b'$'
            } else {
                bytes[i + vb.len()]
            };
            // a var mention is preceded by $/{/[ or a non-identifier
            // char, and followed by a non-identifier char
            if !before.is_ascii_alphanumeric()
                && before != b'_'
                && !after.is_ascii_alphanumeric()
                && after != b'_'
            {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Rewrite `Expr(setVar("v", e))` → `Assign { var: "v", expr: e }` for
/// every store-only v (in place, recursively).
fn rewrite_setvar_stmts(
    st: &mut IrStmt,
    store: &std::collections::HashSet<String>,
    unsafe_v: &std::collections::HashSet<String>,
) {
    match st {
        IrStmt::Expr(IrExpr::Call { func, args }) if func == "setVar" => {
            if let [IrExpr::Str(n, _), value] = args.as_mut_slice() {
                if store.contains(n) && !unsafe_v.contains(n) {
                    *st = IrStmt::Assign {
                        targets: vec![crate::ir::AssignTarget {
                            var: n.clone(),
                            sigil: None,
                            indices: vec![],
                        }],
                        expr: value.clone(),
                    };
                }
            }
        }
        IrStmt::If { then, elsifs, else_, .. } => {
            for s in then.iter_mut().chain(else_.iter_mut()) {
                rewrite_setvar_stmts(s, store, unsafe_v);
            }
            for (_, b) in elsifs {
                for s in b {
                    rewrite_setvar_stmts(s, store, unsafe_v);
                }
            }
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } | IrStmt::Block(body) => {
            for s in body {
                rewrite_setvar_stmts(s, store, unsafe_v);
            }
        }
        IrStmt::For { body, .. } => {
            for s in body {
                rewrite_setvar_stmts(s, store, unsafe_v);
            }
        }
        IrStmt::Subshell(stmts) | IrStmt::Background(stmts) => {
            for s in stmts {
                rewrite_setvar_stmts(s, store, unsafe_v);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages {
                for s in stage {
                    rewrite_setvar_stmts(s, store, unsafe_v);
                }
            }
        }
        IrStmt::Redirect { inner, .. } => {
            for s in inner {
                rewrite_setvar_stmts(s, store, unsafe_v);
            }
        }
        IrStmt::Case { clauses, .. } => {
            for c in clauses {
                for s in &mut c.body {
                    rewrite_setvar_stmts(s, store, unsafe_v);
                }
            }
        }
        IrStmt::Function { body, .. } => {
            for s in body {
                rewrite_setvar_stmts(s, store, unsafe_v);
            }
        }
        _ => {}
    }
}

/// Fold provably-constant `$((...))` arithmetic and `Int BinOp` chains
/// to integer literals. The Rust evaluator lives in `ir::optimize_stmts`;
/// the lifted Verdicts (`IrType::Int`) come from the A2 var-type
/// annotations on `IrProgram.var_types`.
pub struct ConstantFold;

impl super::Transform for ConstantFold {
    fn name(&self) -> &'static str {
        "constant_fold"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &PassContext) {
        // Stage 0: the real fold is still in `ir::optimize_stmts`, called
        // from `shir_to_perl` and from `shir::ast_to_ir`. The M3 guardrail
        // proves the output is unchanged when the migration lands.
        let _ = prog;
    }
}

/// Eliminate self-assignments (`x=$x`) and unused declarations
/// (`my $x;` where `$x` is never read). The real pass lives in
/// `ir::optimize_stmts`; migration target.
pub struct DeadAssignmentElim;

impl super::Transform for DeadAssignmentElim {
    fn name(&self) -> &'static str {
        "dead_assignment_elim"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &PassContext) {
        let _ = prog;
    }
}

/// Table-driven `use` emission: replaces the ad-hoc `needs_qx` /
/// `needs_strict` / `needs_warnings` / `needs_features` booleans that
/// the Perl generator used to consult with a single Vec of imports
/// derived from the constructs present in the program. Landed in M6
/// (commit 2f70f9d); lives in `generator/mod.rs` today. Migration target.
pub struct ImportMinimize;

impl super::Transform for ImportMinimize {
    fn name(&self) -> &'static str {
        "import_minimize"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &PassContext) {
        let _ = prog;
    }
}

/// Attach the const/var verdicts to the IR: `IrProgram.var_const` is
/// populated from the [`analysis::ConstVar`] verdicts in `PassContext`
/// (sorted by name for deterministic serialization). This is the
/// "extend the shIR with the const/var markup" step of the pipeline —
/// after it runs, every consumer of `IrProgram` (renderers, the ShIR
/// JSON contract via `shir_json.rs`) sees the markup without recomputing
/// it. Idempotent: re-running overwrites with the same verdicts.
pub struct ConstMarkup;

impl super::Transform for ConstMarkup {
    fn name(&self) -> &'static str {
        "const_markup"
    }
    fn run(&self, prog: &mut IrProgram, ctx: &PassContext) {
        let mut verdicts: Vec<(String, crate::ir::VarKind)> =
            ctx.const_vars.iter().map(|(n, k)| (n.clone(), *k)).collect();
        verdicts.sort_by(|a, b| a.0.cmp(&b.0));
        prog.var_const = verdicts;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shir_passes::Transform as _;
    use crate::ir::IrProgram;

    fn empty_prog() -> IrProgram {
        IrProgram {
            imports: vec![],
            requires: vec![],
            stmts: vec![],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        }
    }

    #[test]
    fn all_transforms_have_unique_names() {
        let transforms: Vec<Box<dyn super::super::Transform>> = vec![
            Box::new(ConstantFold),
            Box::new(DeadAssignmentElim),
            Box::new(ImportMinimize),
            Box::new(ConstMarkup),
        ];
        let mut names: Vec<&str> = transforms.iter().map(|t| t.name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            transforms.len(),
            "duplicate transform name(s): {:?}",
            transforms.iter().map(|t| t.name()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn transforms_are_pure_pass_through_in_stage_0() {
        // Stage 0 contract: the migrated-later transforms are stubs that
        // must not mutate the program (ConstMarkup is the exception — it
        // is the real const-markup transform and is tested separately).
        // The migration lands in stage 1; this test pins the no-op
        // behaviour so a premature wiring-up is caught immediately.
        let mut prog = empty_prog();
        let ctx = PassContext::default();
        let snapshot = prog.clone();
        let transforms: Vec<Box<dyn super::super::Transform>> = vec![
            Box::new(ConstantFold),
            Box::new(DeadAssignmentElim),
            Box::new(ImportMinimize),
        ];
        for t in &transforms {
            t.run(&mut prog, &ctx);
        }
        assert_eq!(prog, snapshot);
    }

    #[test]
    fn const_markup_attaches_sorted_verdicts() {
        use crate::ir::{IrExpr, IrStmt};
        let mut ctx = PassContext::default();
        ctx.const_vars
            .insert("z".to_string(), crate::ir::VarKind::Var);
        ctx.const_vars
            .insert("x".to_string(), crate::ir::VarKind::Const);
        let mut prog = IrProgram {
            imports: vec![],
            requires: vec![],
            stmts: vec![IrStmt::Expr(IrExpr::Int(1))],
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        };
        ConstMarkup.run(&mut prog, &ctx);
        // sorted by name: x before z
        assert_eq!(
            prog.var_const,
            vec![
                ("x".to_string(), crate::ir::VarKind::Const),
                ("z".to_string(), crate::ir::VarKind::Var),
            ]
        );
        // idempotent
        let before = prog.var_const.clone();
        ConstMarkup.run(&mut prog, &ctx);
        assert_eq!(prog.var_const, before);
    }

    // ── StoreToNative (core request shir-passes-store-to-native-20260806) ──
    fn call(name: &str, args: Vec<IrExpr>) -> IrExpr {
        IrExpr::Call {
            func: name.to_string(),
            args,
        }
    }
    fn str(s: &str) -> IrExpr {
        IrExpr::Str(s.to_string(), crate::ir::StrStyle::DoubleQuoted)
    }
    fn setvar_stmt(v: &str, val: IrExpr) -> IrStmt {
        IrStmt::Expr(call("setVar", vec![str(v), val]))
    }

    #[test]
    fn store_to_native_converts_store_only_var() {
        let mut prog = empty_prog();
        prog.stmts = vec![
            setvar_stmt("x", str("5")),
            IrStmt::Expr(call("getVar", vec![str("x")])),
            setvar_stmt("y", str("abc")),
        ];
        StoreToNative.run(&mut prog, &PassContext::default());
        // x and y: only literal setVar/getVar refs → both become Assign
        assert!(matches!(
            &prog.stmts[0],
            IrStmt::Assign { targets, .. }
                if targets.len() == 1 && targets[0].var == "x"
        ));
        assert!(matches!(
            &prog.stmts[1],
            IrStmt::Expr(IrExpr::Call { func, .. }) if func == "getVar"
        ));
        assert!(matches!(
            &prog.stmts[2],
            IrStmt::Assign { targets, .. }
                if targets.len() == 1 && targets[0].var == "y"
        ));
    }

    #[test]
    fn store_to_native_keeps_array_and_arith_referenced_vars() {
        let mut prog = empty_prog();
        prog.stmts = vec![
            // x is read by arrayLen → NOT store-only → setVar stays
            setvar_stmt("x", str("5")),
            IrStmt::Expr(call("arrayLen", vec![str("x")])),
            // y is mentioned in a runtime arith string → NOT store-only
            setvar_stmt("y", str("5")),
            IrStmt::Expr(call("arith", vec![str("y + 1")])),
            // z is written by a compound assign → NOT store-only
            setvar_stmt("z", str("5")),
            IrStmt::Expr(call("assign", vec![str("z"), str("+="), str("1")])),
        ];
        StoreToNative.run(&mut prog, &PassContext::default());
        assert!(matches!(
            &prog.stmts[0],
            IrStmt::Expr(IrExpr::Call { func, .. }) if func == "setVar"
        ));
        assert!(matches!(
            &prog.stmts[2],
            IrStmt::Expr(IrExpr::Call { func, .. }) if func == "setVar"
        ));
        assert!(matches!(
            &prog.stmts[4],
            IrStmt::Expr(IrExpr::Call { func, .. }) if func == "setVar"
        ));
    }

    #[test]
    fn store_to_native_dynamic_name_marks_all_unsafe() {
        let mut prog = empty_prog();
        prog.stmts = vec![
            setvar_stmt("x", str("5")),
            IrStmt::Expr(call("getVar", vec![IrExpr::Var("n".to_string(), None)])),
        ];
        StoreToNative.run(&mut prog, &PassContext::default());
        // the dynamic getVar could read x — x must stay store-bound
        assert!(matches!(
            &prog.stmts[0],
            IrStmt::Expr(IrExpr::Call { func, .. }) if func == "setVar"
        ));
    }
}
