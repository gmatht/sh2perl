//! Op census per list (LISTS.MD §4.1) — prototype.
//!
//! Walks an [`IrProgram`] and records, per list variable, the set of
//! operations applied to it, plus append/midmut/concat sites with loop
//! trip counts and depths (for `bounds.rs`), alias edges, call edges,
//! and escapes. Interprocedural propagation is a fixed-point over the
//! call graph (union actuals with positional-materialized locals).
//!
//! Prototype simplifications (documented; the drop-in hardens them):
//! - Names are GLOBAL (no per-scope shadowing). Conflation errs toward
//!   union (over-conservative profiles — pessimization, never miscompile).
//! - `setArray` inside a loop is treated as an accumulating append for
//!   bound purposes (over-approximates resets — safe direction; a
//!   reaching-definitions pass would sharpen it).
//! - Element *values* mentioning `$N` are ignored (values don't choose
//!   containers; the class analysis of OPTIMISED_LIST.md owns them).
//! - `subs` bodies are walked; `IrSub` params map positionally like `$N`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ir::{ArithAst, InterpPart, IrExpr, IrProgram, IrStmt};

use super::bounds::{TripCtx, trip_of_loop};

/// Operations that select or gate a container shape. `Read`/`Len` alone
/// never make a name a list (see [`is_list_like`]).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Op {
    Def,
    Append,
    IndexRead,
    IndexWrite,
    Slice,
    Len,
    Iterate,
    Read,
    Sorted,
    Max,
    Min,
    Sum,
    Concat,
    MidMut,
    EndPop,
    Escape,
}

/// One counted site: `count` elements per execution *of its own scope*,
/// `trip` executions of that scope (`None` = unbounded), `depth` =
/// enclosing loop-nest depth, `idx` = constant target index for
/// [`SiteKind::MidMut`], `fun` = enclosing function/sub (`None` =
/// top level — total executions multiply by the callee's activations,
/// resolved in `finish`; see LISTS.MD §4.2).
#[derive(Clone, Debug)]
pub struct Site {
    pub kind: SiteKind,
    pub count: u64,
    pub trip: Option<u64>,
    pub depth: usize,
    pub idx: Option<i64>,
    pub fun: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SiteKind {
    Append,
    /// Bulk define (`setArray` literal): counts toward the upper bound
    /// like an append (a def inside a loop resets rather than
    /// accumulates — over-approximating a reset errs HIGH, the safe
    /// direction; a reaching-definitions pass would sharpen it).
    Def,
    MidMut,
    Concat,
}

/// Per-list census record.
#[derive(Clone, Debug, Default)]
pub struct ListInfo {
    pub ops: BTreeSet<Op>,
    pub literal_floor: u64,
    pub append_sites: Vec<Site>,
    pub midmut_sites: Vec<Site>,
    pub concat_sites: Vec<Site>,
    /// All midmut sites target one constant index (gap-buffer rule).
    pub fixed_cursor: Option<i64>,
    /// Max constant indexed-write target (`arr[5]=x` may grow len to 6).
    pub max_const_index: Option<i128>,
    /// An indexed write with a non-constant index exists: indexed growth
    /// is unbounded (append_hi must be None).
    pub dyn_index_write: bool,
    pub escaped: bool,
}

fn is_pos_pseudo(s: &str) -> bool {
    s.starts_with('@')
}

/// Whole-program census with interprocedural fixed-point.
pub struct Census {
    pub lists: BTreeMap<String, ListInfo>,
    /// True when `eval`/`source`/`.` poisons everything.
    pub dynamic_poison: bool,
}

pub fn census(prog: &IrProgram) -> Census {
    let ranges = crate::shir::analyze_var_ranges(prog);
    let ctx = TripCtx { ranges };
    // defined functions: Function stmts (no params: positionals) + subs
    let mut defined: HashSet<String> = HashSet::new();
    let mut sub_params: HashMap<String, Vec<String>> = HashMap::new();
    for st in &prog.stmts {
        if let IrStmt::Function { name, .. } = st {
            defined.insert(name.clone());
        }
    }
    for sub in &prog.subs {
        defined.insert(sub.name.clone());
        sub_params.insert(sub.name.clone(), sub.params.clone());
    }

    let mut acc = Acc::default();
    // monotone fixed-point: profiles/aliases/escapes only grow
    let mut rounds = 0usize;
    for _ in 0..64 {
        rounds += 1;
        let mut w = Walker {
            ctx: &ctx,
            defined: &defined,
            sub_params: &sub_params,
            cur_fn: None,
            trip: Some(1),
            depth: 0,
            acc: &mut acc,
        };
        for st in &prog.stmts {
            w.stmt(st);
        }
        for sub in &prog.subs {
            w.with_fn(&sub.name, |w| {
                for st in &sub.body {
                    w.stmt(st);
                }
            });
        }
        if !w.acc.propagate() {
            break;
        }
    }
    if std::env::var("LISTPROF_DEBUG").is_ok() {
        eprintln!("listprof: fixpoint rounds = {rounds}");
        for (name, sites) in acc.append_sites.iter() {
            eprintln!(
                "listprof: append sites of {name}: {}",
                sites.iter().map(|s| format!("{}x{:?}", s.count, s.trip)).collect::<Vec<_>>().join(", ")
            );
        }
    }
    acc.finish()
}

/// Union-find over names (aliases + positional materialization +
/// call-site actuals).
#[derive(Default)]
struct UnionFind {
    parent: HashMap<String, String>,
}

impl UnionFind {
    fn find(&mut self, x: &str) -> String {
        if !self.parent.contains_key(x) {
            self.parent.insert(x.to_string(), x.to_string());
            return x.to_string();
        }
        let p = self.parent[x].clone();
        if p == x {
            return x.to_string();
        }
        let r = self.find(&p);
        self.parent.insert(x.to_string(), r.clone());
        r
    }
    /// Returns true when the partition changed.
    fn union(&mut self, a: &str, b: &str) -> bool {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return false;
        }
        // deterministic root: lexicographically smaller wins
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        self.parent.insert(hi, lo);
        true
    }
}

#[derive(Default)]
struct Acc {
    ops: HashMap<String, BTreeSet<Op>>,
    floor: HashMap<String, u64>,
    append_sites: HashMap<String, Vec<Site>>,
    midmut_sites: HashMap<String, Vec<Site>>,
    concat_sites: HashMap<String, Vec<Site>>,
    /// (actual_var, caller_fn, callee, index1, call-site trip) edges.
    /// A set (dedup): re-walks of the fixpoint must not duplicate edges.
    calls: BTreeSet<(String, Option<String>, String, u32, Option<u64>)>,
    /// `setArray` defs with an unknown (non-Array) value shape: bulk
    /// unknown, upper bound must be None.
    unknown_def: HashSet<String>,
    max_const_index: HashMap<String, i128>,
    dyn_index_write: HashSet<String>,
    /// (callee, N) -> local var materialized from $(N)
    mat: HashMap<(String, u32), String>,
    /// alias edges (wholesale Var copies)
    aliases: Vec<(String, String)>,
    /// names mentioned by unknown-callee actuals
    unknown_actual_names: HashSet<String>,
    dynamic_poison: bool,
    uf: UnionFind,
}

impl Acc {
    /// Indexed-write growth accounting: constant indices tighten the
    /// upper bound (`arr[5]=x` may grow len to 6); anything else
    /// poisons it (append_hi must be None).
    fn record_index_write(&mut self, base: &str, key: Option<i64>) {
        match key {
            Some(k) => {
                let e = self.max_const_index.entry(base.to_string()).or_insert(k as i128);
                *e = (*e).max(k as i128);
            }
            None => {
                self.dyn_index_write.insert(base.to_string());
            }
        }
    }
    fn op(&mut self, v: &str, o: Op) {
        self.ops.entry(v.to_string()).or_default().insert(o);
    }
    /// Propagate unions + call edges; true when anything changed (re-walk).
    fn propagate(&mut self) -> bool {
        let mut changed = false;
        for (a, b) in self.aliases.clone() {
            changed |= self.uf.union(&a, &b);
        }
        // call edges: actual_var <-> materialized local of (callee, N)
        for (actual, _caller, callee, n, _trip) in self.calls.iter().cloned() {
            if let Some(local) = self.mat.get(&(callee.clone(), n)).cloned() {
                changed |= self.uf.union(&actual, &local);
            }
        }
        changed
    }
    fn finish(mut self) -> Census {
        // fold union classes: every member gets the union of member ops
        let mut roots: HashMap<String, Vec<String>> = HashMap::new();
        for name in self.ops.keys().cloned().collect::<Vec<_>>() {
            let r = self.uf.find(&name);
            roots.entry(r).or_default().push(name);
        }
        // escapes from unknown calls: any tracked name mentioned there
        let mut lists: BTreeMap<String, ListInfo> = BTreeMap::new();
        for (name, ops) in &self.ops {
            if !is_list_like(ops) {
                continue;
            }
            let r = self.uf.find(name);
            let mut u = BTreeSet::new();
            for m in roots.get(&r).cloned().unwrap_or_default() {
                if let Some(o) = self.ops.get(&m) {
                    u.extend(o.iter().cloned());
                }
            }
            let mut info = ListInfo {
                ops: u,
                literal_floor: 0,
                append_sites: vec![],
                midmut_sites: vec![],
                concat_sites: vec![],
                fixed_cursor: None,
                max_const_index: None,
                dyn_index_write: false,
                escaped: false,
            };
            // floor + sites + index growth: union over class members.
            // Site trips inside functions are scaled by the callee's
            // activations (single top-level call) or widened to unbounded,
            // unless the list is defined in the same function (fresh per
            // activation — no scaling).
            let mut trip_scale: HashMap<Option<String>, Option<u64>> = HashMap::new();
            let members = roots.get(&r).cloned().unwrap_or_default();
            let mut def_funs: HashSet<Option<String>> = HashSet::new();
            for m in &members {
                for s in self.append_sites.get(m).cloned().unwrap_or_default() {
                    if s.kind == SiteKind::Def {
                        def_funs.insert(s.fun.clone());
                    }
                }
            }
            for m in members {
                info.literal_floor = info.literal_floor.saturating_add(
                    self.floor.get(&m).copied().unwrap_or(0),
                );
                for s in self.append_sites.get(&m).cloned().unwrap_or_default() {
                    info.append_sites.push(scale_site(s, &mut trip_scale, &self, &def_funs));
                }
                for s in self.midmut_sites.get(&m).cloned().unwrap_or_default() {
                    info.midmut_sites.push(scale_site(s, &mut trip_scale, &self, &def_funs));
                }
                for s in self.concat_sites.get(&m).cloned().unwrap_or_default() {
                    info.concat_sites.push(scale_site(s, &mut trip_scale, &self, &def_funs));
                }
                if self.dyn_index_write.contains(&m) {
                    info.dyn_index_write = true;
                }
                if self.unknown_actual_names.contains(&m) {
                    info.escaped = true;
                }
                if self.unknown_def.contains(&m) {
                    // unknown def bulk: force the upper bound to None
                    // via an unboundedtrip site (no struct change needed)
                    info.append_sites.push(Site {
                        kind: SiteKind::Def,
                        count: 0,
                        trip: None,
                        depth: 0,
                        idx: None,
                        fun: None,
                    });
                }
            }
            if self.dynamic_poison {
                info.escaped = true;
            }
            lists.insert(name.clone(), info);
        }
        // fixed_cursor: every midmut site const and equal
        for info in lists.values_mut() {
            let mut it = info.midmut_sites.iter();
            info.fixed_cursor = match it.next() {
                None => None,
                Some(first) => {
                    let c = first.idx;
                    if c.is_some() && it.all(|s| s.idx == c) {
                        c
                    } else {
                        None
                    }
                }
            };
        }
        Census { lists, dynamic_poison: self.dynamic_poison }
    }
}

/// Activation scaling for a site inside a function/sub: per-activation
/// trip × callee activations. Skipped (factor 1) when the site's list
/// is *defined in the same function* — each activation builds a fresh
/// list, so the per-activation max is the max, regardless of call
/// count or recursion. Otherwise exact only for a single top-level
/// call site of a non-recursive callee; anything else widens to
/// unbounded — erring HIGH is the safe direction (may choose Vec over
/// Fixed — pessimization, never the exit-127 miscompile).
fn scale_site(
    mut s: Site,
    cache: &mut HashMap<Option<String>, Option<u64>>,
    acc: &Acc,
    def_funs: &HashSet<Option<String>>,
) -> Site {
    let f = match &s.fun {
        None => return s,
        Some(f) => f.clone(),
    };
    // fresh list per activation: no scaling
    if def_funs.contains(&Some(f.clone())) {
        return s;
    }
    let factor = cache.entry(Some(f.clone())).or_insert_with(|| {
        // call graph edges caller -> callee (top-level caller = None,
        // which cannot recurse)
        let mut edges: HashMap<Option<String>, Vec<String>> = HashMap::new();
        for (_, caller, callee, _, _) in acc.calls.iter() {
            edges.entry(caller.clone()).or_default().push(callee.clone());
        }
        // recursive iff f reaches itself
        let mut stack = vec![f.clone()];
        let mut seen = HashSet::new();
        let mut recursive = false;
        while let Some(cur) = stack.pop() {
            if let Some(nexts) = edges.get(&Some(cur.clone())) {
                for nx in nexts {
                    if nx == &f {
                        recursive = true;
                    }
                    if seen.insert(nx.clone()) {
                        stack.push(nx.clone());
                    }
                }
            }
        }
        if recursive {
            return None;
        }
        let sites: Vec<(Option<String>, Option<u64>)> = acc
            .calls
            .iter()
            .filter(|(_, _, callee, _, _)| callee == &f)
            .map(|(_, caller, _, _, trip)| (caller.clone(), *trip))
            .collect();
        match sites.as_slice() {
            [(caller, trip)] if caller.is_none() => *trip,
            _ => None,
        }
    });
    s.trip = mul_trip(s.trip, *factor);
    s
}

/// Indexed-write growth accounting is inline in the Assign arm (see
/// `record_index_write`); no side table is needed.

/// Constant index of an index expression (`arr[5]` → 5). Only integer
/// literals count — anything else is a dynamic index.
fn const_index_of(e: &IrExpr) -> Option<i64> {
    match e {
        IrExpr::Int(i) => Some(*i),
        _ => None,
    }
}

/// List-hood gate: only names BUILT as arrays count. `Def`/`Append`
/// (`setArray`/`setArrayAppend`/`DeclareArray`) are build ops no scalar
/// ever carries; every other op is overloaded — `param("slice", s)`
/// is substring on a string, `max("abc")` is char-max, `getVar`/`len`
/// read scalars. Tracking those as lists produced Vec verdicts for
/// scalars (t37 `_fi`-free miscompile caught by the A/B sweep).
/// Param-fed read-only lists stay untracked → Fixed, today's behavior.
#[allow(dead_code)]
fn is_list_like(ops: &BTreeSet<Op>) -> bool {
    ops.contains(&Op::Def) || ops.contains(&Op::Append)
}

struct Walker<'a> {
    ctx: &'a TripCtx,
    defined: &'a HashSet<String>,
    sub_params: &'a HashMap<String, Vec<String>>,
    cur_fn: Option<String>,
    trip: Option<u64>,
    depth: usize,
    acc: &'a mut Acc,
}

impl<'a> Walker<'a> {
    fn with_fn(&mut self, f: &str, body: impl FnOnce(&mut Self)) {
        let save = self.cur_fn.clone();
        self.cur_fn = Some(f.to_string());
        // a call may execute the body 0..N times — trips inside are
        // relative to one activation; the caller's trip multiplies at
        // the call site (prototype: keep callee-relative, document).
        body(self);
        self.cur_fn = save;
    }
    fn with_trip(&mut self, trip: Option<u64>, body: impl FnOnce(&mut Self)) {
        let save_trip = self.trip;
        let save_depth = self.depth;
        self.trip = mul_trip(self.trip, trip);
        self.depth += 1;
        body(self);
        self.trip = save_trip;
        self.depth = save_depth;
    }

    fn op(&mut self, v: &str, o: Op) {
        if is_pos_pseudo(v) {
            // positional pseudo-vars are union anchors, not lists
            self.acc.op(v, o);
            return;
        }
        self.acc.op(v, o);
    }

    fn stmt(&mut self, st: &IrStmt) {
        match st {
            IrStmt::Ext(_) => {
                // Opaque node: assume nothing list-specific (conservative
                // reads are handled by callers that need them). Prototype:
                // Ext payloads are transform-created, never list builds.
            }
            IrStmt::Output { value, .. } => self.expr(value),
            IrStmt::WriteFile { path, content, .. } => {
                self.expr(path);
                self.expr(content);
            }
            IrStmt::Assign { targets, expr, .. } => {
                // wholesale Var copy = alias edge
                if let IrExpr::Var(v, _) = expr {
                    for t in targets {
                        if t.indices.is_empty() && !t.var.contains('[') {
                            self.acc.aliases.push((t.var.clone(), v.clone()));
                        }
                    }
                }
                // positional materialization: v = expr-mentioning-$(N)
                for n in positional_mentions(expr) {
                    for t in targets {
                        if t.indices.is_empty() && !t.var.contains('[') {
                            if let Some(f) = &self.cur_fn {
                                self.acc.mat.insert((f.clone(), n), t.var.clone());
                            }
                            // sub params map positionally too
                            if let Some(f) = &self.cur_fn {
                                if let Some(params) = self.sub_params.get(f) {
                                    if (n as usize) >= 1 && (n as usize) <= params.len() {
                                        self.acc.aliases.push((
                                            t.var.clone(),
                                            params[(n as usize) - 1].clone(),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                for t in targets {
                    if t.indices.is_empty() {
                        if !t.var.contains('[') {
                            let _ = &t.var;
                        }
                    } else {
                        // arr[k] = v — indexed write; base may grow
                        let base = t.var.split('[').next().unwrap_or(&t.var);
                        self.op(base, Op::IndexWrite);
                        for ix in &t.indices {
                            self.expr(ix);
                        }
                        let key = t.indices.first().and_then(const_index_of);
                        self.acc.record_index_write(base, key);
                    }
                }
                // single value walk (a second walk would duplicate sites)
                self.read_expr_as_value(expr);
            }
            IrStmt::Declare { vars, init, .. } => {
                for d in vars {
                    let _ = &d.name;
                }
                if let Some(e) = init {
                    self.expr(e);
                }
            }
            IrStmt::DeclareArray { var, elements, .. } => {
                self.op(var, Op::Def);
                let lit = elements
                    .iter()
                    .filter(|e| matches!(e, IrExpr::Int(_) | IrExpr::Str(_, _)))
                    .count() as u64;
                if self.trip == Some(1) {
                    *self.acc.floor.entry(var.clone()).or_default() += lit;
                }
                self.acc.append_sites.entry(var.clone()).or_default().push(Site {
                    kind: SiteKind::Def,
                    count: elements.len() as u64,
                    trip: self.trip,
                    depth: self.depth,
                    idx: None,
                    fun: self.cur_fn.clone(),
                });
                for e in elements {
                    self.expr(e);
                }
            }
            IrStmt::If { cond, then, elsifs, else_ } => {
                self.expr(cond);
                for b in then.iter().chain(else_) {
                    self.stmt(b);
                }
                for (c, b) in elsifs {
                    self.expr(c);
                    for stm in b {
                        self.stmt(stm);
                    }
                }
            }
            IrStmt::Try { body, excepts, else_body, finally_body } => {
                for b in body.iter().chain(else_body).chain(finally_body) {
                    self.stmt(b);
                }
                for e in excepts {
                    if let Some(m) = &e.match_expr {
                        self.expr(m);
                    }
                    for b in &e.body {
                        self.stmt(b);
                    }
                }
            }
            IrStmt::For { var, iter, body } => {
                // `for x in <iter>`: a list-valued iter is iterated
                for v in expr_var_mentions(iter) {
                    self.op(&v, Op::Iterate);
                }
                self.expr(iter);
                let trip = for_trip(iter);
                self.with_trip(trip, |w| {
                    let _ = var;
                    for b in body {
                        w.stmt(b);
                    }
                });
            }
            IrStmt::ForInit { init, cond, step, body } => {
                for b in init.iter().chain(step) {
                    self.stmt(b);
                }
                let trip = trip_of_loop(cond, &self.ctx.ranges);
                self.with_trip(trip, |w| {
                    for b in body {
                        w.stmt(b);
                    }
                });
            }
            IrStmt::Continue => {}
            IrStmt::Break => {}
            IrStmt::While { cond, body } => {
                self.expr(cond);
                let trip = trip_of_loop(cond, &self.ctx.ranges);
                self.with_trip(trip, |w| {
                    for b in body {
                        w.stmt(b);
                    }
                });
            }
            IrStmt::DoWhile { body, cond, .. } => {
                let trip = trip_of_loop(cond, &self.ctx.ranges);
                self.with_trip(trip, |w| {
                    for b in body {
                        w.stmt(b);
                    }
                });
            }
            IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => self.expr(expr),
            IrStmt::Exec { cmd, args, capture, redirects, env } => {
                self.exec_call(cmd, args, capture.as_deref(), redirects, env);
            }
            IrStmt::Pipeline { stages, .. } => {
                for stage in stages {
                    for b in stage {
                        self.stmt(b);
                    }
                }
            }
            IrStmt::Return(e) | IrStmt::Exit(e) => {
                if let Some(x) = e {
                    self.expr(x);
                }
            }
            IrStmt::SetChildError(e) => self.expr(e),
            IrStmt::Require(_) | IrStmt::RawText(_) => {}
            IrStmt::Case { discriminant, clauses } => {
                self.expr(discriminant);
                for c in clauses {
                    for b in &c.body {
                        self.stmt(b);
                    }
                }
            }
            IrStmt::Redirect { inner, redirects } => {
                for b in inner {
                    self.stmt(b);
                }
                for r in redirects {
                    self.expr(&r.target);
                }
            }
            IrStmt::Function { name, body, named_blocks, .. } => {
                // fresh frame, but the frame IS named: positional
                // materialization inside maps to this function, and
                // site trips are per-activation (scaled by call sites
                // in `finish`).
                let save_trip = self.trip;
                let save_depth = self.depth;
                let save_fn = self.cur_fn.clone();
                self.cur_fn = Some(name.clone());
                self.trip = Some(1);
                self.depth = 0;
                for b in body {
                    self.stmt(b);
                }
                for (_, b) in named_blocks {
                    for st in b {
                        self.stmt(st);
                    }
                }
                self.cur_fn = save_fn;
                self.trip = save_trip;
                self.depth = save_depth;
            }
            IrStmt::Subshell(body) | IrStmt::Background(body) | IrStmt::Block(body) => {
                for b in body {
                    self.stmt(b);
                }
            }
            IrStmt::Select { clauses } => {
                for c in clauses {
                    if let Some(ch) = &c.ch {
                        self.expr(ch);
                    }
                    if let Some(v) = &c.value {
                        self.expr(v);
                    }
                    for b in &c.body {
                        self.stmt(b);
                    }
                }
            }
            IrStmt::Shaderize { body, .. } => {
                for b in body {
                    self.stmt(b);
                }
            }
            IrStmt::Expr(e) => self.expr(e),
            IrStmt::Label(_) | IrStmt::Goto(_) => {}
            IrStmt::Asm { outputs, inputs, .. } => {
                for (_, e) in outputs.iter().chain(inputs) {
                    self.expr(e);
                }
            }
        }
    }

    /// `exec`/`fnCall` dispatch: user-function calls map actuals to the
    /// callee's positional materializations; builtins are scanned for
    /// list-op shapes (`unset` element removal counts as middle
    /// mutation); unknown callees escape their list-typed actuals;
    /// `eval`/`source`/`.` poison everything.
    fn exec_call(
        &mut self,
        cmd: &IrExpr,
        args: &[IrExpr],
        capture: Option<&str>,
        redirects: &[IrExpr],
        env: &[(String, IrExpr)],
    ) {
        // dynamic poison first (convention: eval/source/. anywhere)
        if let IrExpr::Str(cname, _) = cmd {
            if matches!(cname.as_str(), "eval" | "source" | ".") {
                self.acc.dynamic_poison = true;
            }
        }
        // builtin write words: read/unset/local/declare/... name args
        if let IrExpr::Str(cname, _) = cmd {
            if matches!(
                cname.as_str(),
                "read" | "unset" | "mapfile" | "readarray" | "let"
            ) {
                for a in args {
                    self.exec_word_builtin(cname, a);
                }
            }
        }
        // user-function call mapping is shared with the
        // Call-expression dispatch form (see `user_call`)
        let mut callee: Option<String> = None;
        let mut actuals: &[IrExpr] = &[];
        if let IrExpr::Str(cname, _) = cmd {
            if matches!(cname.as_str(), "exec" | "fnCall" | "callDirect") {
                if let Some(IrExpr::Str(f, _)) = args.first() {
                    callee = Some(f.clone());
                    actuals = array_or_rest(args);
                }
            }
        }
        if let Some(f) = callee {
            self.user_call(&f, actuals);
        } else {
            // dynamic callee (non-literal command): same escape rule
            self.expr(cmd);
            for a in args {
                for v in expr_var_mentions(a) {
                    self.acc.unknown_actual_names.insert(v);
                }
                self.expr(a);
            }
        }
        if let Some(c) = capture {
            let _ = c;
        }
        for r in redirects {
            self.expr(r);
        }
        for (_, e) in env {
            self.expr(e);
        }
    }

    /// Shared user-call mapping (Exec-statement and Call-expression
    /// dispatch forms): defined callees record call edges (actuals map
    /// positionally onto the callee's materialized locals); builtins are
    /// walked; unknown callees escape their list-typed actuals.
    fn user_call(&mut self, f: &str, actuals: &[IrExpr]) {
        if self.defined.contains(f) {
            for (i, a) in actuals.iter().enumerate() {
                if let Some(v) = actual_var_of(a) {
                    self.acc.calls.insert((
                        v,
                        self.cur_fn.clone(),
                        f.to_string(),
                        (i + 1) as u32,
                        self.trip,
                    ));
                }
                self.expr(a);
            }
        } else if is_builtin_call(f) {
            for a in actuals {
                self.expr(a);
            }
        } else {
            for a in actuals {
                for v in expr_var_mentions(a) {
                    self.acc.unknown_actual_names.insert(v);
                }
                self.expr(a);
            }
        }
    }

    /// One word of a `read`/`unset`/`mapfile`/`readarray`/`let` builtin:
    /// `unset arr[k]` (const k) is a middle mutation; non-const is an
    /// unknown-index mutation; anything else is scanned for mentions.
    fn exec_word_builtin(&mut self, cname: &str, a: &IrExpr) {
        if cname == "unset" {
            if let IrExpr::Str(w, _) = a {
                if let Some((base, sub)) = w.split_once('[') {
                    if sub.ends_with(']') {
                        let inner = sub[..sub.len() - 1].trim_start_matches('$');
                        let inner = inner
                            .strip_prefix('{')
                            .and_then(|s| s.strip_suffix('}'))
                            .unwrap_or(inner);
                        let k = inner.parse::<i64>().ok();
                        self.op(base, Op::MidMut);
                        self.acc.midmut_sites.entry(base.to_string()).or_default().push(Site {
                            kind: SiteKind::MidMut,
                            count: 1,
                            trip: self.trip,
                            depth: self.depth,
                            idx: k,
                            fun: self.cur_fn.clone(),
                        });
                        return;
                    }
                }
                if is_bare_ident(w) {
                    // whole-array unset: the list dies here; subsequent
                    // reads see unset. Prototype: escape (conservative —
                    // the drop-in models kill/re-def instead).
                    self.op(w, Op::Escape);
                    return;
                }
            }
        }
        self.expr(a);
    }
    /// General call expression: precise list-op shapes plus the generic
    /// read rule (any tracked-name Str arg outside a def position reads).
    fn call_expr(&mut self, func: &str, args: &[IrExpr]) {
        // dispatch forms (same as the Exec-statement path): exec("f",
        // [actuals]) / fnCall("f", ...) map actuals onto the callee's
        // positional materializations.
        if matches!(func, "exec" | "fnCall" | "callDirect") {
            if let Some(IrExpr::Str(f, _)) = args.first() {
                let actuals = array_or_rest(args);
                self.user_call(f, actuals);
                return;
            }
            // dynamic callee: list-typed actuals escape
            for a in args {
                for v in expr_var_mentions(a) {
                    self.acc.unknown_actual_names.insert(v);
                }
                self.expr(a);
            }
            return;
        }
        match func {
            "setArray" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.op(name, Op::Def);
                    if let Some(IrExpr::Array(items)) = args.get(1) {
                        let lit = items
                            .iter()
                            .filter(|e| matches!(e, IrExpr::Int(_) | IrExpr::Str(_, _)))
                            .count() as u64;
                        if self.trip == Some(1) {
                            *self.acc.floor.entry(name.clone()).or_default() += lit;
                        }
                        // the def bulk-counts toward the upper bound (see
                        // SiteKind::Def)
                        self.acc.append_sites.entry(name.clone()).or_default().push(Site {
                            kind: SiteKind::Def,
                            count: items.len() as u64,
                            trip: self.trip,
                            depth: self.depth,
                            idx: None,
                            fun: self.cur_fn.clone(),
                        });
                        for it in items {
                            self.expr(it);
                        }
                    } else {
                        // unknown def shape (no element list): bulk unknown
                        self.acc.unknown_def.insert(name.clone());
                    }
                    return;
                }
            }
            "setArrayAppend" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.op(name, Op::Append);
                    let n = match args.get(1) {
                        Some(IrExpr::Array(items)) => {
                            for it in items {
                                self.expr(it);
                            }
                            items.len() as u64
                        }
                        _ => 1,
                    };
                    self.acc.append_sites.entry(name.clone()).or_default().push(Site {
                        kind: SiteKind::Append,
                        count: n.max(1),
                        trip: self.trip,
                        depth: self.depth,
                        idx: None,
                        fun: self.cur_fn.clone(),
                    });
                    return;
                }
            }
            "sortedIntJoin" | "sortedBigintJoin" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.op(name, Op::Sorted);
                    for a in args {
                        self.expr(a);
                    }
                    return;
                }
            }
            "max" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.op(name, Op::Max);
                    for a in args {
                        self.expr(a);
                    }
                    return;
                }
            }
            "min" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.op(name, Op::Min);
                    for a in args {
                        self.expr(a);
                    }
                    return;
                }
            }
            "sum" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.op(name, Op::Sum);
                    for a in args {
                        self.expr(a);
                    }
                    return;
                }
            }
            "arrayIndex" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.op(name, Op::IndexRead);
                    for a in args {
                        self.expr(a);
                    }
                    return;
                }
            }
            "arrayLen" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.op(name, Op::Len);
                    for a in args {
                        self.expr(a);
                    }
                    return;
                }
            }
            "arrayItems" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    self.op(name, Op::Iterate);
                    for a in args {
                        self.expr(a);
                    }
                    return;
                }
            }
            "getVar" => {
                if let Some(IrExpr::Str(name, _)) = args.first() {
                    // scalar-context read (element 0 for arrays per the
                    // store rule) — still a value read of the var
                    self.op(name, Op::Read);
                    for a in args {
                        self.expr(a);
                    }
                    return;
                }
            }
            "param" => {
                // param("slice", name, start, len) — order-preserving slice
                if let [IrExpr::Str(kind, _), IrExpr::Str(name, _), rest @ ..] = args {
                    if kind == "slice" {
                        self.op(name, Op::Slice);
                        for a in rest {
                            self.expr(a);
                        }
                        return;
                    }
                }
            }
            _ => {}
        }
        // user-function value call f(args): like exec_call's mapping
        if self.defined.contains(func) {
            for (i, a) in args.iter().enumerate() {
                if let Some(v) = actual_var_of(a) {
                    // IrSub params are 0-based here (params[i] ↔ arg i)
                    self.acc.calls.insert((
                        v,
                        self.cur_fn.clone(),
                        func.to_string(),
                        (i + 1) as u32,
                        self.trip,
                    ));
                }
                self.expr(a);
            }
            return;
        }
        if !is_builtin_call(func) {
            for a in args {
                for v in expr_var_mentions(a) {
                    self.acc.unknown_actual_names.insert(v);
                }
                self.expr(a);
            }
            return;
        }
        for a in args {
            self.expr(a);
        }
    }

    /// Expression walk: `$`-refs and bare-name Str args count as reads
    /// (conservative — a literal value that looks like a name
    /// over-refuses, never miscompiles).
    fn expr(&mut self, e: &IrExpr) {
        match e {
            IrExpr::Int(_) | IrExpr::Bool(_) | IrExpr::Range { .. } => {}
            IrExpr::Str(s, _) => {
                for v in dollar_vars(s) {
                    self.op(&v, Op::Read);
                }
            }
            IrExpr::Var(v, _) | IrExpr::Ident(v) => {
                self.op(v, Op::Read);
            }
            IrExpr::Index { key, .. } => {
                // base handled by callers that know the position; the key
                // is always a value read
                self.expr(key);
            }
            IrExpr::BinOp { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            IrExpr::Call { func, args } => self.call_expr(func, args),
            IrExpr::MethodCall { obj, args, .. } => {
                self.expr(obj);
                for a in args {
                    self.expr(a);
                }
            }
            IrExpr::Ternary { cond, then, else_ } => {
                self.expr(cond);
                self.expr(then);
                self.expr(else_);
            }
            IrExpr::DefinedOr { expr, default } => {
                self.expr(expr);
                self.expr(default);
            }
            IrExpr::Interpolate(parts) => {
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => {
                            for v in dollar_vars(s) {
                                self.op(&v, Op::Read);
                            }
                        }
                        InterpPart::Expr(x) => self.expr(x),
                    }
                }
            }
            IrExpr::Capture { expr, .. } => self.expr(expr),
            IrExpr::Regex { .. } => {}
            IrExpr::RawExpr(_) => {}
            IrExpr::Arrow(body) => {
                for b in body {
                    self.stmt(b);
                }
            }
            IrExpr::Array(items) => {
                for it in items {
                    self.expr(it);
                }
            }
            IrExpr::ArrayComp { iter, elem, cond, .. } => {
                self.expr(iter);
                self.expr(elem);
                if let Some(c) = cond {
                    self.expr(c);
                }
            }
            IrExpr::Lambda { body, .. } => {
                for b in body {
                    self.stmt(b);
                }
            }
            IrExpr::Arith(a) => self.arith(a),
            IrExpr::Json(_) => {}
            IrExpr::Object(props) => {
                for (_, v) in props {
                    self.expr(v);
                }
            }
            IrExpr::Splice(x) => self.expr(x),
            IrExpr::Ext(_) => {}
        }
    }

    fn arith(&mut self, a: &ArithAst) {
        match a {
            ArithAst::Num(_) | ArithAst::Sizeof(_) => {}
            ArithAst::Var(v) | ArithAst::Ident(v) => {
                // `$1`-style positionals are materialization sources,
                // not list reads — but a numeric read of a list var is
                // still a read (conservative)
                if v.parse::<u32>().is_ok() {
                    return;
                }
                self.op(v, Op::Read);
            }
            ArithAst::Index { key, .. } => {
                self.arith(key);
            }
            ArithAst::Bin { lhs, rhs, .. } => {
                self.arith(lhs);
                self.arith(rhs);
            }
            ArithAst::Un { arg, .. } => self.arith(arg),
            ArithAst::Cond { test, then, else_ } => {
                self.arith(test);
                self.arith(then);
                self.arith(else_);
            }
            ArithAst::Assign { rhs, .. } => self.arith(rhs),
            ArithAst::IncDec { .. } => {}
            ArithAst::Cast { arg, .. } => self.arith(arg),
        }
    }

    /// The assigned value read in full (RHS of `target = expr`): records
    /// list-op shapes nested in the value (e.g. an element that is itself
    /// a reduction over another list). Called exactly once per Assign.
    fn read_expr_as_value(&mut self, e: &IrExpr) {
        self.expr(e);
    }
}

/// Positional `$N` mentions inside an expression (materialization +
/// union sources). Scans arith idents, bare vars, Str texts and
/// `getVar("N")` name args.
fn positional_mentions(e: &IrExpr) -> Vec<u32> {
    let mut out = vec![];
    positional_expr(e, &mut out);
    out.sort();
    out.dedup();
    out
}

fn positional_expr(e: &IrExpr, out: &mut Vec<u32>) {
    match e {
        IrExpr::Str(s, _) => out.extend(dollar_pos_refs(s)),
        IrExpr::Var(v, _) | IrExpr::Ident(v) => {
            if let Ok(n) = v.parse::<u32>() {
                out.push(n);
            }
        }
        IrExpr::Call { args, .. } => {
            for a in args {
                positional_expr(a, out);
            }
        }
        IrExpr::Arith(a) => arith_positionals(a, out),
        IrExpr::Array(items) => {
            for it in items {
                positional_expr(it, out);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            positional_expr(lhs, out);
            positional_expr(rhs, out);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                match p {
                    InterpPart::Lit(s) => out.extend(dollar_pos_refs(s)),
                    InterpPart::Expr(x) => positional_expr(x, out),
                }
            }
        }
        _ => {}
    }
}

fn arith_positionals(a: &ArithAst, out: &mut Vec<u32>) {
    match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => {
            if let Ok(n) = v.parse::<u32>() {
                out.push(n);
            }
        }
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_positionals(lhs, out);
            arith_positionals(rhs, out);
        }
        ArithAst::Un { arg, .. } => arith_positionals(arg, out),
        ArithAst::Cond { test, then, else_ } => {
            arith_positionals(test, out);
            arith_positionals(then, out);
            arith_positionals(else_, out);
        }
        ArithAst::Assign { rhs, .. } => arith_positionals(rhs, out),
        ArithAst::Cast { arg, .. } => arith_positionals(arg, out),
        ArithAst::Index { key, .. } => arith_positionals(key, out),
        _ => {}
    }
}

/// `$N`/`${N}` positional refs in a text (digits only).
fn dollar_pos_refs(text: &str) -> Vec<u32> {
    let b = text.as_bytes();
    let mut i = 0usize;
    let mut out = vec![];
    while i < b.len() {
        if b[i] != b'$' {
            i += 1;
            continue;
        }
        i += 1;
        if i >= b.len() {
            break;
        }
        if b[i] == b'{' {
            i += 1;
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if start < i {
                if let Ok(n) = text[start..i].parse::<u32>() {
                    out.push(n);
                }
            }
            continue;
        }
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if let Ok(n) = text[start..i].parse::<u32>() {
                out.push(n);
            }
            continue;
        }
        i += 1;
    }
    out
}

/// `$`-reference variable names in a text (`$v`, `${v}`, `${v…}`).
/// Used for the conservative read rule.
fn dollar_vars(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut i = 0usize;
    let mut out = vec![];
    while i < b.len() {
        if b[i] != b'$' {
            i += 1;
            continue;
        }
        i += 1;
        if i >= b.len() {
            break;
        }
        if b[i] == b'{' {
            i += 1;
            if i < b.len() && (b[i] == b'#' || b[i] == b'!') {
                i += 1;
            }
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            if start < i {
                out.push(text[start..i].to_string());
            }
            continue;
        }
        if b[i].is_ascii_alphabetic() || b[i] == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(text[start..i].to_string());
            continue;
        }
        i += 1;
    }
    out
}

/// Plain variable names mentioned by an expression (for unknown-callee
/// escape attribution and `for x in <iter>` iterate detection).
fn expr_var_mentions(e: &IrExpr) -> Vec<String> {
    let mut out = vec![];
    var_mentions(e, &mut out);
    out.sort();
    out.dedup();
    out
}

fn var_mentions(e: &IrExpr, out: &mut Vec<String>) {
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => out.push(v.clone()),
        IrExpr::Str(s, _) => out.extend(dollar_vars(s)),
        IrExpr::Call { func, args } => {
            if func == "getVar" {
                if let Some(IrExpr::Str(n, _)) = args.first() {
                    out.push(n.clone());
                }
            }
            for a in args {
                var_mentions(a, out);
            }
        }
        IrExpr::Arith(a) => arith_var_mentions(a, out),
        IrExpr::Array(items) => {
            for it in items {
                var_mentions(it, out);
            }
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            var_mentions(lhs, out);
            var_mentions(rhs, out);
        }
        IrExpr::Interpolate(parts) => {
            for p in parts {
                match p {
                    InterpPart::Lit(s) => out.extend(dollar_vars(s)),
                    InterpPart::Expr(x) => var_mentions(x, out),
                }
            }
        }
        _ => {}
    }
}

fn arith_var_mentions(a: &ArithAst, out: &mut Vec<String>) {
    match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => out.push(v.clone()),
        ArithAst::Bin { lhs, rhs, .. } => {
            arith_var_mentions(lhs, out);
            arith_var_mentions(rhs, out);
        }
        ArithAst::Un { arg, .. } => arith_var_mentions(arg, out),
        ArithAst::Cond { test, then, else_ } => {
            arith_var_mentions(test, out);
            arith_var_mentions(then, out);
            arith_var_mentions(else_, out);
        }
        ArithAst::Assign { rhs, .. } => arith_var_mentions(rhs, out),
        ArithAst::Cast { arg, .. } => arith_var_mentions(arg, out),
        ArithAst::Index { key, .. } => arith_var_mentions(key, out),
        _ => {}
    }
}

/// A wholesale variable actual (`Var(v)` / `getVar("v")`) — the aliasing
/// position for call-site union. Anything else is a value, not an alias.
fn actual_var_of(a: &IrExpr) -> Option<String> {
    match a {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => Some(v.clone()),
        IrExpr::Call { func, args } if func == "getVar" => match args.first() {
            Some(IrExpr::Str(n, _)) => Some(n.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// `exec("f", rest...)`: actuals live in the Array wrapper at args[1]
/// when present, else the flat rest.
fn array_or_rest<'x>(args: &'x [IrExpr]) -> &'x [IrExpr] {
    if let Some(IrExpr::Array(items)) = args.get(1) {
        return items;
    }
    if args.len() > 1 {
        return &args[1..];
    }
    &[]
}

/// Runtime builtins that never touch user lists opaquely (their list-op
/// shapes are matched explicitly in `call_expr`).
fn is_builtin_call(f: &str) -> bool {
    matches!(
        f,
        "exec" | "fnCall" | "callDirect"
            | "setArray" | "setArrayAppend"
            | "sortedIntJoin" | "sortedBigintJoin"
            | "max" | "min" | "sum"
            | "arrayIndex" | "arrayLen" | "arrayItems"
            | "getVar" | "param" | "assign" | "test"
            | "echo" | "printf" | "local" | "declare" | "export"
            | "readonly" | "read" | "unset" | "let" | "eval" | "source" | "."
            | "join" | "split" | "capture" | "captureWords"
    )
}

fn is_bare_ident(w: &str) -> bool {
    !w.is_empty()
        && w.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !w.bytes().next().is_some_and(|b| b.is_ascii_digit())
}

/// Trip product with saturation (`None` = unbounded wins).
fn mul_trip(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.saturating_mul(y)),
        _ => None,
    }
}

/// Trip bound of a `for x in <iter>`: literal arrays and int ranges.
fn for_trip(iter: &IrExpr) -> Option<u64> {
    match iter {
        IrExpr::Array(items) => Some(items.len() as u64),
        IrExpr::Range { start, end } => {
            if *end >= *start {
                Some((*end as i128 - *start as i128 + 1).min(u64::MAX as i128) as u64)
            } else {
                Some(0)
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;

    fn assign(var: &str, expr: IrExpr) -> IrStmt {
        IrStmt::Assign {
            targets: vec![crate::ir::AssignTarget {
                var: var.to_string(),
                sigil: None,
                indices: vec![],
            }],
            expr,
            asm: None,
        }
    }

    fn set_array(name: &str, n: usize) -> IrExpr {
        IrExpr::Call {
            func: "setArray".to_string(),
            args: vec![
                IrExpr::Str(name.to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array((0..n).map(|i| IrExpr::Int(i as i64)).collect()),
            ],
        }
    }

    fn append(name: &str) -> IrExpr {
        IrExpr::Call {
            func: "setArrayAppend".to_string(),
            args: vec![
                IrExpr::Str(name.to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Int(1)]),
            ],
        }
    }

    fn prog(stmts: Vec<IrStmt>) -> IrProgram {
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
            var_nospace: vec![],
            var_storage: vec![],
            var_bash_env: vec![],
        }
    }

    #[test]
    fn append_census_and_bound() {
        let p = prog(vec![
            assign("xs", set_array("xs", 2)),
            assign("xs", append("xs")),
        ]);
        let c = census(&p);
        let xs = &c.lists["xs"];
        assert!(xs.ops.contains(&Op::Def));
        assert!(xs.ops.contains(&Op::Append));
        assert_eq!(crate::shir_passes::list_profile::bounds::append_hi(xs), Some(2 + 1));
    }

    #[test]
    fn interprocedural_append_propagates_to_actual() {
        // f(xs): xs=Var("1") materializes positional 1; appends inside f
        // must union onto the caller's actual.
        let f = IrStmt::Function {
            name: "f".to_string(),
            body: vec![
                assign("xs", IrExpr::Var("1".to_string(), None)),
                assign("xs", append("xs")),
            ],
            named_blocks: vec![],
        };
        let call = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("f".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Var("mylist".to_string(), None)]),
            ],
        });
        let p = prog(vec![
            assign("mylist", set_array("mylist", 0)),
            f,
            call,
        ]);
        let c = census(&p);
        let my = &c.lists["mylist"];
        assert!(my.ops.contains(&Op::Def), "ops: {:?}", my.ops);
        assert!(my.ops.contains(&Op::Append), "ops: {:?}", my.ops);
    }

    #[test]
    fn unknown_callee_escapes_actual() {
        let call = IrStmt::Expr(IrExpr::Call {
            func: "exec".to_string(),
            args: vec![
                IrExpr::Str("mystery".to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(vec![IrExpr::Var("xs".to_string(), None)]),
            ],
        });
        let p = prog(vec![assign("xs", set_array("xs", 1)), call]);
        let c = census(&p);
        assert!(c.lists["xs"].escaped);
    }

    #[test]
    fn scalar_reads_are_not_lists() {
        let p = prog(vec![
            assign("s", IrExpr::Call {
                func: "getVar".to_string(),
                args: vec![IrExpr::Str("s".to_string(), StrStyle::DoubleQuoted)],
            }),
            assign(
                "n",
                IrExpr::Arith(Box::new(ArithAst::Bin {
                    op: "+".to_string(),
                    lhs: Box::new(ArithAst::Var("s".to_string())),
                    rhs: Box::new(ArithAst::Num(0)),
                })),
            ),
        ]);
        let c = census(&p);
        assert!(!c.lists.contains_key("s"), "keys: {:?}", c.lists.keys());
        assert!(!c.lists.contains_key("n"));
    }

    #[test]
    fn string_slice_is_not_a_list() {
        // t37 regression: `param("slice", s)` on a string (substring)
        // must not track `s` as a list — a Vec verdict for a scalar
        // emitted element-frees for a non-array. Only build ops
        // (Def/Append) confer list-hood.
        use crate::ir::StrStyle;
        let word = |s: &str| IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted);
        let p = prog(vec![
            assign(
                "s",
                IrExpr::Call {
                    func: "param".to_string(),
                    args: vec![word("slice"), word("s"), word("0"), word("3")],
                },
            ),
            IrStmt::Expr(IrExpr::Call {
                func: "max".to_string(),
                args: vec![word("s")],
            }),
        ]);
        let c = census(&p);
        assert!(!c.lists.contains_key("s"), "keys: {:?}", c.lists.keys());
    }
}
