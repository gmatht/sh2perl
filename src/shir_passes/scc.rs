//! Strongly-connected-component (SCC) recognition over the function
//! call graph.
//!
//! ## Why
//! A *single-function* eligibility fixpoint can never recognize a
//! mutually-recursive cluster: every function in the cycle depends on
//! another member, so none is eligible until another is, and the
//! fixpoint terminates with the whole cluster ineligible. The glob
//! matchers (`globMatch` ↔ `ext_alt_match` ↔ `ext_match`), the `test`
//! parser (`test` ↔ `tokenizeTest` + helpers), and several other
//! polyfill functions form exactly such cycles.
//!
//! SCC recognition collapses the call graph to its condensation (a DAG
//! of SCCs). A transform can then reason about an SCC *as a whole*:
//! the whole component is eligible iff every member is eligible under
//! the coinductive assumption that calls within the same SCC are pure
//! (the recursion is a value read of the same pure functions). This is
//! the enabler for the glob-matcher pattern lift and for making the
//! `test`/`tokenizeTest` parser cluster easier to optimize (the
//! token-accumulation rewrite the concurrent worker owns can query the
//! SCC to know which functions move together).
//!
//! ## Algorithm
//! Tarjan's SCC (iterative, to stay safe on large graphs). The call
//! graph is built once from an [`IrProgram`] by walking every function
//! body for calls to defined functions (`exec("f", ..)` / `fnCall("f",
//! ..)` and `$(f ..)` captures). Output is deterministic: nodes are
//! visited in sorted order and each SCC is a sorted set.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ir::{IrExpr, IrProgram, IrStmt};

/// Function-name → set of called function names (the call-graph edges).
#[derive(Clone, Debug, Default)]
pub struct CallGraph {
    /// `from` → the functions `from` calls (by name).
    pub edges: BTreeMap<String, BTreeSet<String>>,
}

static EMPTY: BTreeSet<String> = BTreeSet::new();

impl CallGraph {
    /// Every defined function name.
    pub fn nodes(&self) -> impl Iterator<Item = &String> + '_ {
        self.edges.keys()
    }

    /// The functions `name` calls (empty set if `name` is unknown or
    /// calls nothing).
    pub fn callees(&self, name: &str) -> &BTreeSet<String> {
        self.edges.get(name).unwrap_or(&EMPTY)
    }

    /// Add a call edge `from → to`.
    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges
            .entry(from.to_string())
            .or_default()
            .insert(to.to_string());
    }
}

/// Collect the names of defined functions called anywhere inside `stmts`
/// (recursively through every container node, captures, and arrows).
fn collect_callees(stmts: &[IrStmt], out: &mut HashSet<String>, defined: &BTreeSet<String>) {
    for st in stmts {
        collect_stmt(st, out, defined);
    }
}

fn collect_stmt(st: &IrStmt, out: &mut HashSet<String>, defined: &BTreeSet<String>) {
    match st {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            collect_expr(cond, out, defined);
            collect_callees(then, out, defined);
            for (ec, eb) in elsifs {
                collect_expr(ec, out, defined);
                collect_callees(eb, out, defined);
            }
            collect_callees(else_, out, defined);
        }
        IrStmt::For { iter, body, .. } => {
            collect_expr(iter, out, defined);
            collect_callees(body, out, defined);
        }
        IrStmt::While { cond, body } => {
            collect_expr(cond, out, defined);
            collect_callees(body, out, defined);
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            for s in init.iter().chain(step.iter()) {
                collect_stmt(s, out, defined);
            }
            collect_expr(cond, out, defined);
            collect_callees(body, out, defined);
        }
        IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body) => collect_callees(body, out, defined),
        IrStmt::Redirect { inner, .. } => collect_callees(inner, out, defined),
        IrStmt::Function { body, .. } => collect_callees(body, out, defined),
        IrStmt::Return(Some(e)) | IrStmt::Exit(Some(e)) => collect_expr(e, out, defined),
        IrStmt::Assign { expr, .. } => collect_expr(expr, out, defined),
        IrStmt::Declare { init, .. } => {
            if let Some(i) = init {
                collect_expr(i, out, defined);
            }
        }
        IrStmt::DeclareArray { elements, .. } => {
            for el in elements {
                collect_expr(el, out, defined);
            }
        }
        IrStmt::Output { value, .. } => collect_expr(value, out, defined),
        IrStmt::Expr(e) => collect_expr(e, out, defined),
        // `case` dispatch — the matchers' bodies are case statements; the
        // calls inside the clause bodies are real call-graph edges
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            collect_expr(discriminant, out, defined);
            for clause in clauses {
                collect_callees(&clause.body, out, defined);
            }
        }
        _ => {}
    }
}

fn collect_expr(e: &IrExpr, out: &mut HashSet<String>, defined: &BTreeSet<String>) {
    match e {
        IrExpr::Capture { expr, .. } => {
            // `$(f ..)` — the capture may wrap the call directly
            // (`Capture { expr: Call }`) or behind an arrow
            // (`Capture { expr: Arrow([Expr(Call)]) }`).
            match expr.as_ref() {
                IrExpr::Arrow(body) => collect_callees(body, out, defined),
                other => collect_expr(other, out, defined),
            }
        }
        IrExpr::Call { func, args } => {
            if matches!(func.as_str(), "exec" | "fnCall") {
                // `exec("f", [args])` / `fnCall("f", [args])` — the
                // first arg is the callee name (a bare `f` call may
                // carry no args array at all: `[Str("f")]`).
                if let [IrExpr::Str(fname, _), ..] = args.as_slice() {
                    out.insert(fname.clone());
                }
            }
            for a in args {
                collect_expr(a, out, defined);
            }
        }
        IrExpr::Arrow(body) => collect_callees(body, out, defined),
        IrExpr::Array(items) => {
            for it in items {
                collect_expr(it, out, defined);
            }
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                if let crate::ir::InterpPart::Expr(ie) = p {
                    collect_expr(ie, out, defined);
                }
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            collect_expr(lhs, out, defined);
            collect_expr(rhs, out, defined);
        }
        _ => {}
    }
}

/// Build the call graph for a program: every `IrStmt::Function` is a
/// node; edges are calls to other defined functions discovered by
/// walking the body. Later definitions of the same name replace earlier
/// ones (the last definition wins, matching bash semantics).
pub fn build_call_graph(prog: &IrProgram) -> CallGraph {
    build_call_graph_stmts(&prog.stmts)
}

/// Same as [`build_call_graph`], over a top-level statement list (the
/// shape transforms receive — `transform(stmts: &mut Vec<IrStmt>)`).
pub fn build_call_graph_stmts(stmts: &[IrStmt]) -> CallGraph {
    // name → body (last definition wins)
    let mut bodies: HashMap<String, Vec<IrStmt>> = HashMap::new();
    for st in stmts {
        if let IrStmt::Function { name, body, .. } = st {
            bodies.insert(name.clone(), body.clone());
        }
    }
    let defined: BTreeSet<String> = bodies.keys().cloned().collect();
    let mut g = CallGraph::default();
    // every defined function is a node, even with no edges to other
    // defined functions (a leaf like `g() { echo hi; }` is its own
    // singleton SCC)
    for name in &defined {
        g.edges.entry(name.clone()).or_default();
    }
    for name in &defined {
        let mut callees: HashSet<String> = HashSet::new();
        if let Some(body) = bodies.get(name) {
            collect_callees(body, &mut callees, &defined);
        }
        // keep only edges to *defined* functions (the graph is over the
        // program's own functions; calling an undefined name is a no-op
        // edge for SCC purposes)
        for c in callees.iter().filter(|c| defined.contains(*c)) {
            g.add_edge(name, c);
        }
    }
    g
}

/// Tarjan's strongly-connected-components algorithm (iterative).
///
/// Returns the SCCs as sorted sets, in the order their roots are
/// finished. Each SCC is a maximal set of mutually-reachable nodes.
pub fn tarjan_sccs(graph: &CallGraph) -> Vec<BTreeSet<String>> {
    // adjacency in sorted order for determinism
    let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (from, tos) in &graph.edges {
        let mut sorted: Vec<&str> = tos.iter().map(|s| s.as_str()).collect();
        sorted.sort_unstable();
        adj.insert(from.as_str(), sorted);
    }
    let nodes: BTreeSet<&str> = graph.edges.keys().map(|s| s.as_str()).collect();

    // iterative Tarjan
    const UNSEEN: usize = usize::MAX;
    let mut index: HashMap<&str, usize> = HashMap::new();
    let mut lowlink: HashMap<&str, usize> = HashMap::new();
    let mut on_stack: HashMap<&str, bool> = HashMap::new();
    let mut stack: Vec<&str> = Vec::new();
    let mut idx = 0usize;
    let mut sccs: Vec<BTreeSet<String>> = Vec::new();

    // explicit DFS frame: node, and the position in its adjacency list
    for root in nodes.iter() {
        if *index.get(root).unwrap_or(&UNSEEN) != UNSEEN {
            continue;
        }
        // frame stack: (node, next_child_index)
        let mut frame: Vec<(&str, usize)> = Vec::new();
        index.insert(root, idx);
        lowlink.insert(root, idx);
        idx += 1;
        stack.push(root);
        on_stack.insert(root, true);
        frame.push((root, 0));

        while let Some(&(v, ci)) = frame.last() {
            let neighbors = adj.get(v).cloned().unwrap_or_default();
            if ci < neighbors.len() {
                // advance child pointer
                frame.last_mut().unwrap().1 += 1;
                let w = neighbors[ci];
                if *index.get(w).unwrap_or(&UNSEEN) == UNSEEN {
                    index.insert(w, idx);
                    lowlink.insert(w, idx);
                    idx += 1;
                    stack.push(w);
                    on_stack.insert(w, true);
                    frame.push((w, 0));
                } else if *on_stack.get(w).unwrap_or(&false) {
                    let v_low = lowlink[&v];
                    let w_idx = index[w];
                    lowlink.insert(v, v_low.min(w_idx));
                }
            } else {
                // done with v: if v is an SCC root, pop it
                if lowlink[&v] == index[&v] {
                    let mut comp = BTreeSet::new();
                    loop {
                        let w = stack.pop().unwrap();
                        on_stack.insert(w, false);
                        comp.insert(w.to_string());
                        if w == v {
                            break;
                        }
                    }
                    sccs.push(comp);
                }
                frame.pop();
                // update parent's lowlink
                if let Some(&(parent, _)) = frame.last() {
                    let p_low = lowlink[&parent];
                    let v_low = lowlink[&v];
                    lowlink.insert(parent, p_low.min(v_low));
                }
            }
        }
    }
    sccs
}

/// Map each function name to the index of its SCC in the `tarjan_sccs`
/// output (0-based, in condensation order).
pub fn scc_index(sccs: &[BTreeSet<String>]) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for (i, scc) in sccs.iter().enumerate() {
        for n in scc {
            m.insert(n.clone(), i);
        }
    }
    m
}

/// Returns the SCC (as a set of names) containing `name`, or `None` if
/// `name` is not a node of the graph.
pub fn scc_of<'a>(name: &str, sccs: &'a [BTreeSet<String>]) -> Option<&'a BTreeSet<String>> {
    sccs.iter().find(|scc| scc.contains(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::commands::parse_commands_from_text;
    use crate::shir::ast_to_ir_raw;

    fn sccs_of_src(src: &str) -> Vec<BTreeSet<String>> {
        let commands = parse_commands_from_text(src).expect("parse source");
        let prog = ast_to_ir_raw(&commands);
        let g = build_call_graph(&prog);
        tarjan_sccs(&g)
    }

    /// Two functions calling each other form a single SCC of size 2.
    #[test]
    fn mutual_recursion_is_one_scc() {
        let src = r#"
        a() { b; }
        b() { a; }
        "#;
        let sccs = sccs_of_src(src);
        let mut sizes: Vec<usize> = sccs.iter().map(|s| s.len()).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, vec![2]);
        let big = sccs.iter().find(|s| s.len() == 2).unwrap();
        assert!(big.contains("a"));
        assert!(big.contains("b"));
    }

    /// A self-recursive function is its own SCC of size 1.
    #[test]
    fn self_recursion_is_scc_of_one() {
        let src = r#"
        f() { f; }
        g() { echo hi; }
        "#;
        let sccs = sccs_of_src(src);
        assert_eq!(sccs.len(), 2);
        assert!(sccs.iter().any(|s| s == &bt_set(&["f"])));
        assert!(sccs.iter().any(|s| s == &bt_set(&["g"])));
    }

    /// A simple chain a→b→c has no cycle: three singleton SCCs.
    #[test]
    fn acyclic_chain_is_singletons() {
        let src = r#"
        a() { b; }
        b() { c; }
        c() { echo x; }
        "#;
        let sccs = sccs_of_src(src);
        let mut sizes: Vec<usize> = sccs.iter().map(|s| s.len()).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, vec![1, 1, 1]);
    }

    /// The glob matcher shape: globMatch ↔ ext_match ↔ ext_alt_match is
    /// one SCC (the recognition the glob-matcher pattern lift needs).
    #[test]
    fn matcher_cycle_is_one_scc() {
        let src = r#"
        globMatch() { ext_match; ext_alt_match; }
        ext_match() { globMatch; }
        ext_alt_match() { globMatch; }
        other() { echo no; }
        "#;
        let sccs = sccs_of_src(src);
        let big = sccs.iter().find(|s| s.len() == 3).expect("three-node SCC");
        assert!(big.contains("globMatch"));
        assert!(big.contains("ext_match"));
        assert!(big.contains("ext_alt_match"));
        assert!(!big.contains("other"));
    }

    /// `$(f ..)` captures are recognized as calls.
    #[test]
    fn capture_is_a_call_edge() {
        let src = r#"
        caller() { r=$(callee x); echo "$r"; }
        callee() { echo y; }
        "#;
        let commands = parse_commands_from_text(src).expect("parse source");
        let prog = ast_to_ir_raw(&commands);
        let g = build_call_graph(&prog);
        assert!(g.callees("caller").contains("callee"));
    }

    /// The `test` parser's recursive cluster — `eval_or` ↔ `eval_and`
    /// ↔ `eval_not` ↔ `eval_primary` (the polyfill's test-expression
    /// evaluator) — is recognized as ONE SCC. This is the recognition
    /// that makes the test-parser hot path easier to optimize: a
    /// transform (e.g. the worker's token-accumulation rewrite) can
    /// reason about the whole parser cluster at once instead of being
    /// defeated by the single-function fixpoint.
    #[test]
    fn test_parser_cluster_is_one_scc() {
        let src = r#"
        test() { toks=$(tokenizeTest "$1"); eval_or; echo "$__result"; }
        tokenizeTest() { echo "$1"; }
        eval_or() { eval_and; eval_or; }
        eval_and() { eval_not; eval_and; }
        eval_not() { eval_primary; }
        eval_primary() { eval_or; }
        "#;
        let sccs = sccs_of_src(src);
        let parser = sccs
            .iter()
            .find(|s| s.contains("eval_or"))
            .expect("eval_or SCC");
        // the whole evaluator cluster is one SCC
        assert!(parser.contains("eval_and"));
        assert!(parser.contains("eval_not"));
        assert!(parser.contains("eval_primary"));
        // `test` and `tokenizeTest` are NOT in the evaluator SCC (they
        // only call INTO it — a DAG edge, not a cycle)
        assert!(!parser.contains("test"));
        assert!(!parser.contains("tokenizeTest"));
    }

    fn bt_set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }
}
