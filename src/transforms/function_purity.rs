//! function-purity — compute FUNCTION-LEVEL side-effect classes (Pure /
//! Emulable / Impure) by fixpoint over the call graph, the shIR
//! equivalent of sh2runtime's estree passes `lowerPureFunctions` +
//! `flattenAndOrAll` (JS-only today). The A1 JSON already carries
//! PER-CALL purity from the Rust parser (`"purity": "Emulable" |
//! "PureCpu" | "Spawn"` on every `Call`) — this pass lifts that to
//! function-level verdicts the renderers can act on.
//!
//! ## Need
//! Three backends need function purity and each derives it separately:
//!   - the estree pass `lowerPureFunctions` specializes pure helpers
//!     (the texture generators' per-pixel math) and `flattenAndOrAll`
//!     flattens `sh2.and()`/`sh2.or()` to native `&&`/`||` ONLY when the
//!     operands are side-effect-free (a lazy native operator would skip
//!     a side effect);
//!   - the C backend wants `static` + `const`-style knowledge (and the
//!     `v=$(sq 3)` capture-of-function lowering — see direct-calls);
//!   - the GLSL backend must know a helper emits no strings (only pure
//!     math) to inline it into a shader.
//! A single shIR verdict set serves all three.
//!
//! ## The classes
//! ```text
//! Pure    — the function body has NO observable effect and NO control
//!           flow that could diverge: only Assign/Declare/DeclareArray/
//!           Expr of pure expressions, If/While/For over pure conds.
//!           A call is pure iff the callee is Pure AND the args are pure.
//! Emulable— pure PLUS the body is only arithmetic/string Output
//!           (the `v=$(sq 3)` capture form): the output can be computed
//!           in-process, no subprocess.
//! Impure  — anything else (any Exec/Pipeline/WriteFile/Capture/
//!           Background/Subshell/Output-with-side-effect/Exit/Die).
//! ```
//!
//! ## Scope
//! Analysis-only (like `sync-ok-loops`): verdicts stored in module
//! statics keyed by function name, read by the renderers. No structural
//! mutation.
//!
//! ## Placement
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). The estree
//! worker mediates the renderer hooks: estree.rs reads the verdicts for
//! its and/or flatten + pure-helper specialization; the C/GLSL/Go
//! generators consume the same.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::ir::{IrExpr, IrStmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Purity {
    Impure,
    Emulable,
    Pure,
}

static VERDICTS: Mutex<Option<HashMap<String, Purity>>> = Mutex::new(None);

pub fn purity(name: &str) -> Option<Purity> {
    VERDICTS.lock().unwrap().as_ref().and_then(|m| m.get(name).copied())
}

/// Apply the transform (analysis-only — computes + caches the verdicts).
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // bodies keyed by function name
    let mut bodies: HashMap<String, Vec<IrStmt>> = HashMap::new();
    for st in stmts.iter() {
        if let IrStmt::Function { name, body, .. } = st {
            bodies.insert(name.clone(), body.clone());
        }
    }
    // fixpoint: purity(f) = min over its body of the per-stmt purity,
    // where a call's purity is the callee's (0 = Impure, 1 = Emulable,
    // 2 = Pure — the min is the conservative meet).
    let mut v: HashMap<String, Purity> = HashMap::new();
    loop {
        let mut changed = false;
        for (name, body) in bodies.iter() {
            let p = body.iter().map(|s| stmt_purity(s, &v)).min().unwrap_or(Purity::Pure);
            if v.get(name) != Some(&p) {
                v.insert(name.clone(), p);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    *VERDICTS.lock().unwrap() = Some(v);
    false // analysis-only
}

fn stmt_purity(st: &IrStmt, callees: &HashMap<String, Purity>) -> Purity {
    match st {
        IrStmt::Assign { expr, .. } => expr_purity(expr, callees),
        IrStmt::Declare { init, .. } => init
            .as_ref()
            .map(|i| expr_purity(i, callees))
            .unwrap_or(Purity::Pure),
        IrStmt::DeclareArray { elements, .. } => elements
            .iter()
            .map(|e| expr_purity(e, callees))
            .min()
            .unwrap_or(Purity::Pure),
        IrStmt::Expr(e) => expr_purity(e, callees),
        IrStmt::If { cond, then, elsifs, else_, .. } => {
            let mut p = expr_purity(cond, callees);
            for s in then {
                p = p.min(stmt_purity(s, callees));
            }
            for (c, b) in elsifs {
                p = p.min(expr_purity(c, callees));
                for s in b {
                    p = p.min(stmt_purity(s, callees));
                }
            }
            for s in else_ {
                p = p.min(stmt_purity(s, callees));
            }
            p
        }
        IrStmt::For { iter, body, .. } | IrStmt::While { cond: iter, body } => {
            let mut p = expr_purity(iter, callees);
            for s in body {
                p = p.min(stmt_purity(s, callees));
            }
            p
        }
        IrStmt::Output { value, .. } => {
            // an Output is EMULABLE (the value can be captured) but not
            // PURE (it emits bytes)
            if expr_purity(value, callees) == Purity::Impure {
                Purity::Impure
            } else {
                Purity::Emulable
            }
        }
        _ => Purity::Impure, // Exec/Pipeline/WriteFile/Capture/Background/… 
    }
}

fn expr_purity(e: &IrExpr, callees: &HashMap<String, Purity>) -> Purity {
    match e {
        IrExpr::Int(_) | IrExpr::Str(_, _) | IrExpr::Var(_, _) | IrExpr::Bool(_) | IrExpr::Range { .. } => Purity::Pure,
        IrExpr::BinOp { lhs, rhs, .. } => expr_purity(lhs, callees).min(expr_purity(rhs, callees)),
        IrExpr::Call { func, args } => {
            // a builtin (arith/test/…) is pure; a shell function's
            // purity is the callee verdict (unknown → conservative
            // Impure)
            let f = callees.get(func).copied().unwrap_or(Purity::Impure);
            let a = args.iter().map(|a| expr_purity(a, callees)).min().unwrap_or(Purity::Pure);
            f.min(a)
        }
        IrExpr::Arith(_) => Purity::Pure,
        IrExpr::Interpolate(parts) => parts
            .iter()
            .map(|p| match p {
                crate::ir::InterpPart::Lit(_) => Purity::Pure,
                crate::ir::InterpPart::Expr(x) => expr_purity(x, callees),
            })
            .min()
            .unwrap_or(Purity::Pure),
        IrExpr::Index { key, .. } => expr_purity(key, callees),
        IrExpr::Ternary { cond, then, else_, .. } => {
            expr_purity(cond, callees)
                .min(expr_purity(then, callees))
                .min(expr_purity(else_, callees))
        }
        IrExpr::Capture { .. } | IrExpr::Arrow(_) | IrExpr::MethodCall { .. } => Purity::Impure,
        _ => Purity::Impure,
    }
}

// name: function-purity
// prereqs: [none — the call-graph fixpoint is self-contained; feeds
//   direct-calls (the Emulable verdict gates the capture lowering)]
// invariant: analysis-only; no structural mutation. A function is
//   Impure unless every statement/expr in its body meets the class —
//   the meet is the MIN (conservative). The renderers must re-run (or
//   reset) the transform per program.
// scope: offered to estree (owner — supersedes lowerPureFunctions +
//   the flattenAndOrAll operand gate), c, glsl, go, perl, sh
// updates: none (first offer)
