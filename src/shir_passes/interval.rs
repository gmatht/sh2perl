//! Generic integer interval analysis — the single range lattice for
//! every frontend.
//!
//! `shir.rs:analyze_var_ranges` (bash, wrapping i64, trunc `%`) and
//! `frontends/py-sh-go/autocython_ranges.go` (Python, unbounded `int`,
//! floor `//`/`%`, `for x in <proved list>` exact trip count) are the
//! same analysis param'd by language. This module is that param'd
//! lattice — `IntervalConfig` selects the transfer functions; the walk
//! (`loop_fixpoint`, `join`, `widen` to `INT_DOMAIN`) is shared.
//!
//! Stage: the walk delegates to the existing `shir::analyze_var_ranges`
//! for `bash()` and to a floor-mod port of `autocython_ranges.go` for
//! `python()`. The next step is to thread `IntervalConfig` through the
//! shared walk so both are one codepath; the `PassContext::var_ranges`
//! field is the single verdict every backend reads.

use std::collections::HashMap;

use crate::ir::{ArithAst, IrProgram};
use crate::shir_passes::PassContext;

/// How `%`/`//` behave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modulo {
    /// Bash / C: trunc toward zero, sign follows dividend.
    Trunc,
    /// Python: floor, `a % m` with `m>0` is `[0,m-1]` when `a` provably `>=0`.
    Floor,
}

/// Language config for the interval lattice.
#[derive(Clone, Copy, Debug)]
pub struct IntervalConfig {
    /// `true` = bash wrapping at 2^64 (`wrap_interval`); `false` = Python
    /// unbounded — any overflow is `⊤` (`None`).
    pub wrapping: bool,
    pub modulo: Modulo,
}

impl IntervalConfig {
    pub fn bash() -> Self {
        Self {
            wrapping: true,
            modulo: Modulo::Trunc,
        }
    }
    pub fn python() -> Self {
        Self {
            wrapping: false,
            modulo: Modulo::Floor,
        }
    }
}

/// Single entry point every frontend uses. Populates `ctx.var_ranges`
/// (mirrors `shir::analyze_var_ranges` for bash).
pub fn analyze_ranges(prog: &IrProgram, cfg: IntervalConfig) -> HashMap<String, (i128, i128)> {
    match cfg.modulo {
        Modulo::Trunc => crate::shir::analyze_var_ranges(prog),
        Modulo::Floor => analyze_ranges_floor(prog, cfg),
    }
}

/// Python floor-mod variant: same walk as bash, but `%`/`//` use the
/// floor transfer from `autocython_ranges.go` (proved-nonneg lhs only).
/// For now implemented by post-filtering the trunc result through the
/// floor rule; a full shared walk will thread `cfg` through `arith_range`.
fn analyze_ranges_floor(prog: &IrProgram, _cfg: IntervalConfig) -> HashMap<String, (i128, i128)> {
    // Reuse bash walk then re-derive floor-mod sites: the only
    // difference the corpus exercises is `h = (h*31+i) % M` and
    // `a // b` with proven `a>=0, b>0`. Those are sound under both
    // semantics when `a>=0`; the bash result is already sound, just
    // less precise for negative `a`. Porting the exact `applyBin` floor
    // rule (autocython `applyBin`/`floorDivIV`) keeps Python precise
    // without forking the walk.
    let mut ranges = crate::shir::analyze_var_ranges(prog);
    // Refinement: re-evaluate any `%`/`//` arith nodes with floor
    // semantics and intersect. This is conservative — we only narrow.
    refine_floor_mod(prog, &mut ranges);
    ranges
}

fn floor_arith_range(a: &ArithAst, env: &HashMap<String, (i128, i128)>) -> Option<(i128, i128)> {
    match a {
        ArithAst::Bin { op, lhs, rhs } if op == "%" => {
            let (l0, _l1) = arith_floor_range(lhs, env)?;
            let (r0, r1) = arith_floor_range(rhs, env)?;
            if l0 >= 0 && r0 > 0 {
                // Python floor-mod with positive divisor
                Some((0, r1 - 1))
            } else {
                None
            }
        }
        ArithAst::Bin { op, lhs, rhs } if op == "/" => {
            // `//` is lowered as `/` with floor semantics in Python
            let (l0, l1) = arith_floor_range(lhs, env)?;
            let (r0, r1) = arith_floor_range(rhs, env)?;
            if l0 >= 0 && r0 > 0 {
                // floor div, both nonneg -> trunc == floor, endpoints suffice
                let qs = [l0 / r1, l0 / r0, l1 / r1, l1 / r0];
                Some((*qs.iter().min()?, *qs.iter().max()?))
            } else {
                None
            }
        }
        _ => None,
    }
}
fn arith_floor_range(a: &ArithAst, env: &HashMap<String, (i128, i128)>) -> Option<(i128, i128)> {
    match a {
        ArithAst::Num(i) => Some((*i as i128, *i as i128)),
        ArithAst::Var(n) | ArithAst::Ident(n) => env.get(n).copied(),
        ArithAst::Bin { op, lhs, rhs } => {
            let l = arith_floor_range(lhs, env)?;
            let r = arith_floor_range(rhs, env)?;
            match op.as_str() {
                "+" => Some((l.0.checked_add(r.0)?, l.1.checked_add(r.1)?)),
                "-" => Some((l.0.checked_sub(r.1)?, l.1.checked_sub(r.0)?)),                    "*" => {
                        // Checked like `+`/`-` and every other corner site
                        // in the tree: a raw `*` panics in debug and wraps
                        // in release, and a wrapped corner is a silently
                        // wrong verdict (IntervalFloor's `v0_not_sound` --
                        // 2^64 * 2^64 naming [0,0]). Overflow refuses.
                        let ps = [
                            l.0.checked_mul(r.0)?,
                            l.0.checked_mul(r.1)?,
                            l.1.checked_mul(r.0)?,
                            l.1.checked_mul(r.1)?,
                        ];
                        Some((*ps.iter().min()?, *ps.iter().max()?))
                    }
                _ => None,
            }
        }
        _ => None,
    }
}
fn refine_floor_mod(prog: &IrProgram, ranges: &mut HashMap<String, (i128, i128)>) {
    // Walk assigns whose RHS is an arith `%`/`//` and, when the lhs is
    // provably >=0 and divisor >0, clamp to the floor interval
    // `[0,m-1]` / floor-div bounds. Uses the current `ranges` as the
    // env — one pass is enough for the corpus (`h` chain converges in
    // the fixpoint already).
    use crate::ir::{ArithAst, IrExpr, IrStmt};
    fn stmt_refine(s: &IrStmt, ranges: &mut HashMap<String, (i128, i128)>) {
        match s {
            IrStmt::Assign { targets, expr, .. } if targets.len() == 1 && targets[0].indices.is_empty() => {
                if let IrExpr::Arith(a) = expr {
                    if let Some((lo, hi)) = floor_arith_range(a, ranges) {
                        let name = &targets[0].var;
                        // intersect with existing (narrow only)
                        if let Some((elo, ehi)) = ranges.get(name).copied() {
                            let nlo = lo.max(elo);
                            let nhi = hi.min(ehi);
                            if nlo <= nhi {
                                ranges.insert(name.clone(), (nlo, nhi));
                            }
                        }
                    }
                }
                if let IrExpr::Arith(a) = expr {
                    arith_writes_refine(a, ranges);
                }
            }
            IrStmt::Expr(IrExpr::Arith(a)) => arith_writes_refine(a, ranges),
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                for s in b {
                    stmt_refine(s, ranges)
                }
            }
            IrStmt::If { then, elsifs, else_, .. } => {
                for s in then {
                    stmt_refine(s, ranges)
                }
                for (_, arm) in elsifs {
                    for s in arm {
                        stmt_refine(s, ranges)
                    }
                }
                for s in else_ {
                    stmt_refine(s, ranges)
                }
            }
            IrStmt::While { body, .. } | IrStmt::For { body, .. } | IrStmt::DoWhile { body, .. } => {
                for s in body {
                    stmt_refine(s, ranges)
                }
            }
            IrStmt::Function { body, .. } => {
                for s in body {
                    stmt_refine(s, ranges)
                }
            }
            _ => {}
        }
    }
    fn arith_writes_refine(_a: &ArithAst, _ranges: &mut HashMap<String, (i128, i128)>) {}
    for s in &prog.stmts {
        stmt_refine(s, ranges);
    }
}

/// Analysis impl — the `Pipeline` entry. Uses `bash()` config; Python
/// frontends construct `IntervalAnalysis { config: IntervalConfig::python() }`
/// directly when they need floor semantics.
pub struct Interval;

impl crate::shir_passes::Analysis for Interval {
    fn name(&self) -> &'static str {
        "interval"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        ctx.var_ranges = analyze_ranges(prog, IntervalConfig::bash());
    }
}

/// Python-config analysis for frontends that lower Python to `IrProgram`.
pub struct PyInterval;

impl crate::shir_passes::Analysis for PyInterval {
    fn name(&self) -> &'static str {
        "py_interval"
    }
    fn run(&self, prog: &IrProgram, ctx: &mut PassContext) {
        ctx.var_ranges = analyze_ranges(prog, IntervalConfig::python());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges_of(src: &str) -> HashMap<String, (i128, i128)> {
        let cmds = crate::Parser::new(src).parse().expect("parse");
        let prog = crate::shir::ast_to_ir_raw(&cmds);
        analyze_ranges(&prog, IntervalConfig::python())
    }

    /// The `*` arm's corner products: `2^120 * 2^120 = 2^240` escapes
    /// i128. The raw `*` that used to compute the corners panicked in a
    /// debug build (this test, red before the fix) and wrapped in
    /// release -- a wrapped corner is a silently wrong verdict, the
    /// defect class the BigInt pass was built to dodge one level down.
    /// Overflow now refuses, like the `+`/`-` siblings.
    /// Mirrors `v0_not_sound` in proofs/IntervalFloor/IntervalFloor.thy.
    #[test]
    fn floor_multiply_overflow_refuses() {
        let mut env: HashMap<String, (i128, i128)> = HashMap::new();
        env.insert("c".to_string(), (1_i128 << 120, 1_i128 << 120));
        let cc = ArithAst::Bin {
            op: "*".to_string(),
            lhs: Box::new(ArithAst::Var("c".to_string())),
            rhs: Box::new(ArithAst::Var("c".to_string())),
        };
        // 2^120 fits i128 but its square does not: refuse, do not wrap
        assert_eq!(arith_floor_range(&cc, &env), None);
    }

    /// Full pipeline: a deep `c*c` chain under `%` reaches the floor
    /// refine without panicking and leaves `m` with no verdict (today's
    /// env comes from the wrapping bash walk, which caps values near
    /// 2^63, so the `r0 > 0` guard refuses this one before any corner
    /// product is formed).
    #[test]
    fn deep_chain_reaches_floor_refine_without_verdict() {
        let r = ranges_of("a=1073741824\nb=$((a*a))\nc=$((b*b))\nm=$(( (c*c) % c ))");
        assert!(!r.contains_key("m"), "got {:?}", r.get("m"));
    }

    /// The floor-mod transfer itself: proved-nonneg lhs with positive
    /// divisor refines toward [0, r1-1] and only ever narrows (the
    /// intersect keeps the existing range when it is tighter).
    #[test]
    fn floor_mod_refines_to_zero_based() {
        let r = ranges_of("x=100\nd=30\nm=$((x % d))");
        let (lo, hi) = r.get("m").expect("m has a range");
        assert!(
            *lo >= 0 && *hi <= 29,
            "floor mod of a nonneg lhs in [0,29], got {:?}",
            r.get("m")
        );
    }
}
