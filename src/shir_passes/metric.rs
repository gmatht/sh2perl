//! Metric — the sh2.* call-site tally (PLAN §9.2: "the metric is the
//! progress signal for the shared library").
//!
//! `fail-estree --metric` produces a TSV of `callee<TAB>count` lines; the
//! `Metric` struct is the in-process representation. The worker's
//! "improvement mode" reads the diff between runs:
//! - total decreased AND corpus green → commit
//! - total increased (tolerance +1 for flakiness) → stash
//! - flat for 3 rounds → idle (sleep 300), recheck later

use std::collections::HashMap;

use crate::ir::{IrExpr, IrProgram, IrStmt};

/// Per-callee count of sh2.* call sites in the post-pipeline IR.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metric {
    counts: HashMap<String, usize>,
}

impl Metric {
    /// Tally sh2.* call sites in `prog`. Walks every statement and every
    /// expression; counts each `IrExpr::Call { func, .. }` whose callee
    /// is a sh2.* call (or one of the whitelisted sync-loop families).
    ///
    /// The walk is intentionally simple: a pass that adds a new sh2.*
    /// call site is automatically reflected in the metric on the next
    /// run. The metric is the heartbeat of the shared library; the
    /// backend renderers do not need to know about it.
    pub fn tally(prog: &IrProgram) -> Self {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for stmt in &prog.stmts {
            walk_stmt(stmt, &mut counts);
        }
        for sub in &prog.subs {
            for stmt in &sub.body {
                walk_stmt(stmt, &mut counts);
            }
        }
        Metric { counts }
    }

    /// Total sh2.* call sites across all callees.
    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    /// Count for a specific callee (0 if not present).
    pub fn count_of(&self, callee: &str) -> usize {
        self.counts.get(callee).copied().unwrap_or(0)
    }

    /// Sorted (callee, count) pairs — the canonical order for the
    /// `.estree_metric.tsv` artefact the worker reads.
    pub fn sorted(&self) -> Vec<(String, usize)> {
        let mut v: Vec<(String, usize)> =
            self.counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    /// Diff against a previous metric. Returns the set of callees whose
    /// count changed (positive = increased, negative = decreased). The
    /// worker uses the *total* change as the commit/no-progress signal;
    /// this is the per-callee breakdown.
    pub fn diff(&self, prev: &Metric) -> HashMap<String, i64> {
        let mut out: HashMap<String, i64> = HashMap::new();
        let all: std::collections::HashSet<&String> =
            self.counts.keys().chain(prev.counts.keys()).collect();
        for k in all {
            let now = self.count_of(k) as i64;
            let was = prev.count_of(k) as i64;
            let d = now - was;
            if d != 0 {
                out.insert(k.clone(), d);
            }
        }
        out
    }

    /// Render as TSV (`callee<TAB>count` per line, sorted by callee).
    /// Format: stable, byte-equal between runs (the sort is the
    /// determinism guarantee).
    pub fn to_tsv(&self) -> String {
        let mut s = String::new();
        for (callee, count) in self.sorted() {
            s.push_str(&callee);
            s.push('\t');
            s.push_str(&count.to_string());
            s.push('\n');
        }
        s
    }

    /// True if no sh2.* call sites are tallied (i.e. the program either
    /// has no sh2.* calls or the renderer natively handled everything).
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// Per-callee count (for the worker's prompt — the table that
    /// drives improvement-mode decisions).
    pub fn callee_counts(&self) -> Vec<CalleeCount> {
        self.sorted()
            .into_iter()
            .map(|(callee, count)| CalleeCount { callee, count })
            .collect()
    }
}

/// A single row of the metric table (used by the worker prompt and
/// by `fail-estree --metric`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalleeCount {
    pub callee: String,
    pub count: usize,
}

/// Recursive walk over a statement, tallying every `IrExpr::Call`.
/// Wildcard arms on the variant matches keep the walker forward-compatible
/// with future IrStmt additions (the metric is a heartbeat, not a
/// coverage oracle — an unrecognised variant just contributes zero).
fn walk_stmt(stmt: &IrStmt, counts: &mut HashMap<String, usize>) {
    match stmt {
        IrStmt::Label(_) | IrStmt::Goto(_) => {} // no sh2.* call sites
        IrStmt::RawText(_) => {
            // Raw text: no sh2.* call sites can be known without parsing
            // the embedded language. Skip — the metric is an under-count
            // for programs that still use RawText heavily (the Perl
            // generator's migration bridge). Once the generator is fully
            // on the shIR, the count converges.
        }
        IrStmt::Output { value, .. } => walk_expr(value, counts),
        IrStmt::WriteFile { path, content, .. } => {
            walk_expr(path, counts);
            walk_expr(content, counts);
        }
        IrStmt::Assign { expr, .. } => walk_expr(expr, counts),
        IrStmt::Declare { init, .. } => {
            if let Some(e) = init {
                walk_expr(e, counts);
            }
        }
        IrStmt::DeclareArray { elements, .. } => {
            for e in elements {
                walk_expr(e, counts);
            }
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            walk_expr(cond, counts);
            for s in then {
                walk_stmt(s, counts);
            }
            for (c, body) in elsifs {
                walk_expr(c, counts);
                for s in body {
                    walk_stmt(s, counts);
                }
            }
            for s in else_ {
                walk_stmt(s, counts);
            }
        }
        IrStmt::For { iter, body, .. } => {
            walk_expr(iter, counts);
            for s in body {
                walk_stmt(s, counts);
            }
        }
        IrStmt::While { cond, body, .. } | IrStmt::DoWhile { body, cond, .. } => {
            walk_expr(cond, counts);
            for s in body {
                walk_stmt(s, counts);
            }
        }
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => {
            walk_expr(expr, counts);
        }
        IrStmt::Exec { cmd, args, env, .. } => {
            walk_expr(cmd, counts);
            for a in args {
                walk_expr(a, counts);
            }
            for (_k, v) in env {
                walk_expr(v, counts);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages {
                for s in stage {
                    walk_stmt(s, counts);
                }
            }
        }
        IrStmt::Return(e) | IrStmt::Exit(e) => {
            if let Some(e) = e {
                walk_expr(e, counts);
            }
        }
        IrStmt::SetChildError(e) => walk_expr(e, counts),
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            walk_expr(discriminant, counts);
            for clause in clauses {
                for s in &clause.body {
                    walk_stmt(s, counts);
                }
            }
        }
        IrStmt::Redirect { inner, .. } => {
            for s in inner {
                walk_stmt(s, counts);
            }
        }
        IrStmt::Function { body, .. } | IrStmt::Subshell(body) | IrStmt::Background(body) => {
            for s in body {
                walk_stmt(s, counts);
            }
        }
        IrStmt::Block(body) => {
            for s in body {
                walk_stmt(s, counts);
            }
        }
        IrStmt::Expr(e) => walk_expr(e, counts),
        IrStmt::Require(_) => {
            // `require` is a bare string; no IrExpr children.
        }
    }
}

fn walk_expr(expr: &IrExpr, counts: &mut HashMap<String, usize>) {
    match expr {
        IrExpr::Int(_)
        | IrExpr::Bool(_)
        | IrExpr::Str(_, _)
        | IrExpr::Var(_, _)
        | IrExpr::Regex { .. }
        | IrExpr::RawExpr(_)
        | IrExpr::Ident(_)
        | IrExpr::Json(_) => {}
        IrExpr::Index { key, .. } => walk_expr(key, counts),
        IrExpr::BinOp { lhs, rhs, .. } => {
            walk_expr(lhs, counts);
            walk_expr(rhs, counts);
        }
        IrExpr::Ternary { cond, then, else_ } => {
            walk_expr(cond, counts);
            walk_expr(then, counts);
            walk_expr(else_, counts);
        }
        IrExpr::DefinedOr { expr, default } => {
            walk_expr(expr, counts);
            walk_expr(default, counts);
        }
        IrExpr::Call { func, args } => {
            if super::context::PassContext::is_sh2_call(func) {
                *counts.entry(func.clone()).or_insert(0) += 1;
            }
            for a in args {
                walk_expr(a, counts);
            }
        }
        IrExpr::MethodCall { obj, args, .. } => {
            walk_expr(obj, counts);
            for a in args {
                walk_expr(a, counts);
            }
        }
        IrExpr::Array(items) => {
            for it in items {
                walk_expr(it, counts);
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                match p {
                    crate::ir::InterpPart::Lit(_) => {}
                    crate::ir::InterpPart::Expr(e) => walk_expr(e, counts),
                }
            }
        }
        IrExpr::Object(entries) => {
            for (_k, v) in entries {
                walk_expr(v, counts);
            }
        }
        IrExpr::Arrow(body) => {
            for s in body {
                walk_stmt(s, counts);
            }
        }
        IrExpr::Capture { expr, .. } => walk_expr(expr, counts),
        IrExpr::Range { .. } => {}
        IrExpr::Arith(a) => walk_arith(a, counts),
    }
}

fn walk_arith(a: &crate::ir::ArithAst, counts: &mut HashMap<String, usize>) {
    use crate::ir::ArithAst;
    match a {
        ArithAst::Num(_) | ArithAst::Var(_) => {}
        ArithAst::Index { key, .. } => walk_arith(key, counts),
        ArithAst::Bin { lhs, rhs, .. } => {
            walk_arith(lhs, counts);
            walk_arith(rhs, counts);
        }
        ArithAst::Un { arg, .. } => walk_arith(arg, counts),
        ArithAst::Cond { test, then, else_ } => {
            walk_arith(test, counts);
            walk_arith(then, counts);
            walk_arith(else_, counts);
        }
        ArithAst::Assign { rhs, .. } => walk_arith(rhs, counts),
        ArithAst::IncDec { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{IrProgram, StrStyle};

    fn make_prog(stmts: Vec<IrStmt>) -> IrProgram {
        IrProgram {
            imports: vec![],
            requires: vec![],
            stmts,
            subs: vec![],
            var_types: vec![],
            stmt_lines: vec![],
            var_lengths: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
        }
    }

    #[test]
    fn tallies_simple_call() {
        let prog = make_prog(vec![IrStmt::Expr(IrExpr::Call {
            func: "sh2.exec".to_string(),
            args: vec![IrExpr::Str("echo".to_string(), StrStyle::SingleQuoted)],
        })]);
        let m = Metric::tally(&prog);
        assert_eq!(m.total(), 1);
        assert_eq!(m.count_of("sh2.exec"), 1);
    }

    #[test]
    fn ignores_non_sh2_calls() {
        let prog = make_prog(vec![IrStmt::Expr(IrExpr::Call {
            func: "println".to_string(),
            args: vec![IrExpr::Str("hello".to_string(), StrStyle::SingleQuoted)],
        })]);
        let m = Metric::tally(&prog);
        assert_eq!(m.total(), 0);
    }

    #[test]
    fn walks_into_nested_stmts() {
        // An `if` with a sh2.* call in the then branch.
        let prog = make_prog(vec![IrStmt::If {
            cond: IrExpr::Int(1),
            then: vec![IrStmt::Expr(IrExpr::Call {
                func: "sh2.test".to_string(),
                args: vec![],
            })],
            elsifs: vec![],
            else_: vec![],
        }]);
        let m = Metric::tally(&prog);
        assert_eq!(m.count_of("sh2.test"), 1);
    }

    #[test]
    fn diff_marks_changes() {
        let prev = make_prog(vec![IrStmt::Expr(IrExpr::Call {
            func: "sh2.exec".to_string(),
            args: vec![],
        })]);
        let curr = make_prog(vec![
            IrStmt::Expr(IrExpr::Call {
                func: "sh2.exec".to_string(),
                args: vec![],
            }),
            IrStmt::Expr(IrExpr::Call {
                func: "sh2.test".to_string(),
                args: vec![],
            }),
        ]);
        let d = Metric::tally(&curr).diff(&Metric::tally(&prev));
        assert_eq!(d.get("sh2.exec"), None); // unchanged
        assert_eq!(d.get("sh2.test"), Some(&1)); // new
    }

    #[test]
    fn to_tsv_is_sorted_and_stable() {
        let prog = make_prog(vec![
            IrStmt::Expr(IrExpr::Call {
                func: "sh2.test".to_string(),
                args: vec![],
            }),
            IrStmt::Expr(IrExpr::Call {
                func: "sh2.exec".to_string(),
                args: vec![],
            }),
        ]);
        let m = Metric::tally(&prog);
        let tsv = m.to_tsv();
        // Sorted alphabetically: exec before test.
        let exec_pos = tsv.find("sh2.exec").unwrap();
        let test_pos = tsv.find("sh2.test").unwrap();
        assert!(exec_pos < test_pos);
        // TSV is byte-equal between runs.
        let tsv2 = m.to_tsv();
        assert_eq!(tsv, tsv2);
    }
}
