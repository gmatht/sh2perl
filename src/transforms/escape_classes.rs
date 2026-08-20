//! escape-classes — classify every variable's STORE requirement, the
//! shIR equivalent of sh2runtime's estree passes `liftLocalVars` +
//! `nativeArrays` (both JS-only today; each backend re-derives the same
//! analysis — the C renderer's local-vs-runtime-store decision, the
//! estree's lift/native-array passes — with no shared verdict).
//!
//! ## Need
//! The shIR `Assign { targets }` / `Var` / `Index` model does not say
//! WHERE a variable lives. The JS pipeline spends two passes deciding:
//! `liftLocalVars` (which single-scope vars can leave the runtime store
//! and become native bindings) and `nativeArrays` (which store-backed
//! arrays are only ever indexed with resolvable keys and can become
//! native arrays). Both are pure PROGRAM ANALYSIS — nothing about them
//! is JS-specific — yet the C/Go/Perl backends re-derive the same
//! classification independently. A shared shIR verdict lets every
//! backend emit the cheapest representation.
//!
//! ## The classes
//! ```text
//! Local       — referenced by NAME only, in ONE scope (top level or a
//!               single function), never captured, never indexed.
//!               → a native local binding.
//! Store       — cross-function, or referenced through an indirection
//!               that defeats static resolution (a `$name`-style access
//!               where name is data, an eval'd assignment, a `Readonly`
//!               rebind, a capture that outlives its writer).
//!               → must live in the runtime store.
//! NativeArray  — array written/read only at LITERAL or statically
//!               resolvable keys; never passed by name; no whole-array
//!               capture.
//!               → a native array binding.
//! StoreArray   — any dynamic-key access (`arr[$i]` where $i is not a
//!               compile-time constant), whole-array alias, or var-name
//!               indirection.
//!               → stays in the runtime store.
//! ```
//!
//! ## Scope
//! Analysis-only (like `sync-ok-loops`): the transform computes the
//! verdicts and stores them in module statics; the RENDERERS read them.
//! No structural mutation — the tree must stay put so verdict pointers
//! match emission.
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). The estree
//! worker mediates the renderer hooks: estree.rs replaces its local
//! `liftLocalVars`-style analysis with the verdicts; the C/Go/Perl
//! generators read the store-vs-local verdict instead of guessing.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::ir::{ArithAst, IrExpr, IrStmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeClass {
    Local,
    Store,
    NativeArray,
    StoreArray,
}

/// verdicts keyed by variable name (one compilation per process).
static VERDICTS: Mutex<Option<HashMap<String, EscapeClass>>> = Mutex::new(None);

pub fn verdict(name: &str) -> Option<EscapeClass> {
    VERDICTS.lock().unwrap().as_ref().and_then(|m| m.get(name).copied())
}

/// Apply the transform (analysis-only — computes + caches the verdicts).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // ── pass 1: the reference census ──────────────────────────────
    // var → (function scopes referenced in, dynamic-index?, whole-array?)
    let mut scopes: HashMap<String, HashSet<Option<String>>> = HashMap::new();
    let mut dynamic_key: HashSet<String> = HashSet::new();
    let mut whole_array: HashSet<String> = HashSet::new();
    let mut arrays: HashSet<String> = HashSet::new(); // declared via DeclareArray

    census(stmts, None, &mut scopes, &mut dynamic_key, &mut whole_array, &mut arrays);

    // ── pass 2: classify ──────────────────────────────────────────
    let mut out = HashMap::new();
    for (name, scope_set) in scopes.iter() {
        let multi_scope = scope_set.len() > 1 || (scope_set.len() == 1 && scope_set.contains(&None) && scope_set.iter().any(|s| s.is_some()));
        let is_array = arrays.contains(name);
        let cls = if multi_scope {
            EscapeClass::Store
        } else if is_array {
            if dynamic_key.contains(name) || whole_array.contains(name) {
                EscapeClass::StoreArray
            } else {
                EscapeClass::NativeArray
            }
        } else if dynamic_key.contains(name) || whole_array.contains(name) {
            EscapeClass::Store
        } else {
            EscapeClass::Local
        };
        out.insert(name.clone(), cls);
    }
    *VERDICTS.lock().unwrap() = Some(out);
    false // analysis-only: no structural change
}

fn census(
    stmts: &[IrStmt],
    scope: Option<String>,
    scopes: &mut HashMap<String, HashSet<Option<String>>>,
    dynamic_key: &mut HashSet<String>,
    whole_array: &mut HashSet<String>,
    arrays: &mut HashSet<String>,
) {
    for st in stmts {
        match st {
            IrStmt::Function { name, body, .. } => {
                // a function's OWN name is not a store var; its body runs
                // in its own scope
                for s in body {
                    census_stmt(s, Some(name.clone()), scopes, dynamic_key, whole_array, arrays);
                }
            }
            _ => census_stmt(st, scope.clone(), scopes, dynamic_key, whole_array, arrays),
        }
    }
}

fn census_stmt(
    st: &IrStmt,
    scope: Option<String>,
    scopes: &mut HashMap<String, HashSet<Option<String>>>,
    dynamic_key: &mut HashSet<String>,
    whole_array: &mut HashSet<String>,
    arrays: &mut HashSet<String>,
) {
    match st {
        IrStmt::Assign { targets, expr, .. } => {
            for t in targets {
                if t.indices.is_empty() {
                    touch(&t.var, &scope, scopes);
                } else {
                    arrays.insert(t.var.clone());
                    for k in &t.indices {
                        if !literal_key(k) {
                            dynamic_key.insert(t.var.clone());
                        }
                    }
                }
            }
            census_expr(expr, &scope, scopes, dynamic_key, whole_array, arrays);
        }
        IrStmt::DeclareArray { var, .. } => {
            arrays.insert(var.clone());
            touch(var, &scope, scopes);
        }
        IrStmt::Declare { vars, init, .. } => {
            for v in vars {
                touch(&v.name, &scope, scopes);
            }
            if let Some(i) = init {
                census_expr(i, &scope, scopes, dynamic_key, whole_array, arrays);
            }
        }
        IrStmt::Output { value, .. } => {
            census_expr(value, &scope, scopes, dynamic_key, whole_array, arrays);
        }
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            census_expr(cond, &scope, scopes, dynamic_key, whole_array, arrays);
            for s in then {
                census_stmt(s, scope.clone(), scopes, dynamic_key, whole_array, arrays);
            }
            for (c, b) in elsifs {
                census_expr(c, &scope, scopes, dynamic_key, whole_array, arrays);
                for s in b {
                    census_stmt(s, scope.clone(), scopes, dynamic_key, whole_array, arrays);
                }
            }
            for s in else_ {
                census_stmt(s, scope.clone(), scopes, dynamic_key, whole_array, arrays);
            }
        }
        IrStmt::For { iter, body, .. } => {
            census_expr(iter, &scope, scopes, dynamic_key, whole_array, arrays);
            for s in body {
                census_stmt(s, scope.clone(), scopes, dynamic_key, whole_array, arrays);
            }
        }
        IrStmt::While { cond, body } => {
            census_expr(cond, &scope, scopes, dynamic_key, whole_array, arrays);
            for s in body {
                census_stmt(s, scope.clone(), scopes, dynamic_key, whole_array, arrays);
            }
        }
        IrStmt::Function { .. } => { /* handled by census() */ }
        IrStmt::Subshell(v) | IrStmt::Background(v) | IrStmt::Block(v) => {
            for s in v {
                census_stmt(s, scope.clone(), scopes, dynamic_key, whole_array, arrays);
            }
        }
        IrStmt::Redirect { inner, .. } => {
            for s in inner {
                census_stmt(s, scope.clone(), scopes, dynamic_key, whole_array, arrays);
            }
        }
        IrStmt::Expr(e) => census_expr(e, &scope, scopes, dynamic_key, whole_array, arrays),
        _ => {}
    }
}

fn census_expr(
    e: &IrExpr,
    scope: &Option<String>,
    scopes: &mut HashMap<String, HashSet<Option<String>>>,
    dynamic_key: &mut HashSet<String>,
    whole_array: &mut HashSet<String>,
    arrays: &mut HashSet<String>,
) {
    match e {
        IrExpr::Var(v, _) => touch(v, scope, scopes),
        IrExpr::Ident(v) => touch(v, scope, scopes),
        IrExpr::Index { var, key } => {
            arrays.insert(var.clone());
            if !literal_key(key) {
                dynamic_key.insert(var.clone());
            }
            census_expr(key, scope, scopes, dynamic_key, whole_array, arrays);
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            census_expr(lhs, scope, scopes, dynamic_key, whole_array, arrays);
            census_expr(rhs, scope, scopes, dynamic_key, whole_array, arrays);
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                census_expr(a, scope, scopes, dynamic_key, whole_array, arrays);
            }
        }
        IrExpr::Arith(a) => arith_census(a, scope, scopes, dynamic_key, whole_array, arrays),
        IrExpr::Capture { expr, .. } => {
            census_expr(expr, scope, scopes, dynamic_key, whole_array, arrays);
        }
        IrExpr::Arrow(body) => {
            for s in body {
                census_stmt(s, scope.clone(), scopes, dynamic_key, whole_array, arrays);
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(x) = p {
                    census_expr(x, scope, scopes, dynamic_key, whole_array, arrays);
                }
            }
        }
        IrExpr::Array(items) => {
            for it in items {
                census_expr(it, scope, scopes, dynamic_key, whole_array, arrays);
            }
        }
        _ => {}
    }
}

fn arith_census(
    a: &ArithAst,
    scope: &Option<String>,
    scopes: &mut HashMap<String, HashSet<Option<String>>>,
    dynamic_key: &mut HashSet<String>,
    whole_array: &mut HashSet<String>,
    arrays: &mut HashSet<String>,
) {
    match a {
        ArithAst::Var(v) => touch(v, scope, scopes),
        ArithAst::Index { var, key } => {
            arrays.insert(var.clone());
            if !literal_key_arith(key) {
                dynamic_key.insert(var.clone());
            }
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_census(lhs, scope, scopes, dynamic_key, whole_array, arrays);
            arith_census(rhs, scope, scopes, dynamic_key, whole_array, arrays);
        }
        ArithAst::Un { arg, .. } => arith_census(arg, scope, scopes, dynamic_key, whole_array, arrays),
        ArithAst::Cond { test, then, else_, .. } => {
            arith_census(test, scope, scopes, dynamic_key, whole_array, arrays);
            arith_census(then, scope, scopes, dynamic_key, whole_array, arrays);
            arith_census(else_, scope, scopes, dynamic_key, whole_array, arrays);
        }
        ArithAst::Assign { rhs, .. } => arith_census(rhs, scope, scopes, dynamic_key, whole_array, arrays),
        _ => {}
    }
}

fn touch(
    name: &str,
    scope: &Option<String>,
    scopes: &mut HashMap<String, HashSet<Option<String>>>,
) {
    scopes.entry(name.to_string()).or_default().insert(scope.clone());
}

fn literal_key(k: &IrExpr) -> bool {
    matches!(k, IrExpr::Int(_))
}

fn literal_key_arith(k: &ArithAst) -> bool {
    matches!(k, ArithAst::Num(_))
}

// name: escape-classes
// prereqs: [none — a pure reference census]
// invariant: analysis-only; no structural mutation. The verdicts map is
//   REPLACED per compilation — the renderers must re-run (or reset) the
//   transform per program, never cache across compilations.
// scope: offered to estree (owner — supersedes liftLocalVars/
//   nativeArrays), c, go, perl, sh
// updates: none (first offer)
