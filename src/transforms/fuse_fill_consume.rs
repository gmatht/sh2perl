//! fuse-fill-consume — fuse a covering array-fill loop with a same-space
//! consuming loop, forwarding the fill RHS into the consumer and deleting
//! the fill (and its materialization).
//!
//! Pattern (all in ONE top-level block; nested blocks are veto sources
//! only, never fused — V2):
//!
//! ```text
//! for ((i=lo; i<bound; i+=step)); do A[<i>] = F(i); done
//! s=0                                    # zero or more plain assigns
//! for ((i=lo; i<bound; i+=step)); do S += ... A[<i>] ...; done
//! ```
//! becomes:
//! ```text
//! s=0
//! for ((i=lo; i<bound; i+=step)); do S += ... F(i) ...; done
//! ```
//!
//! Every backend wins from the same rewrite: C emits one pass with no
//! vec, CUDA candidacy sees a plain (array-free) reduce → a single
//! kernel with no device array, Vulkan likewise. The Perl/JS outputs
//! compute the same scalars (`echo` agrees) — the array was unobservable
//! (any other mention vetoes).
//!
//! REFUSE > GUESS — each of these vetoes, never misfires:
//! - fill body is not exactly one `A[<i>] = <pure arith>` store with a
//!   plain-`i` index (affine (1,0));
//! - consumer body writes the counter/bound, never reads `A[<i>]`, or
//!   reads/writes A anywhere but a plain `A[<i>]` arith read;
//! - headers (init/cond/step) differ structurally, or the fill header has
//!   side effects (a Call/Capture — its evaluations would vanish);
//! - the array is mentioned ANYWHERE else in the program (other fills,
//!   other reads, `${#A[@]}`, `unset`, string text, decls, raw bridge
//!   text);
//! - intervening statements aren't call-free scalar Assigns avoiding
//!   A/the counter/the bound/F's free vars/`$?`;
//! - F mentions `$?`/special vars, array reads, or impure nodes
//!   (Assign/IncDec inside the RHS).
//!
//! Traversal auditability: the `stmt_kids` / `stmt_exprs` / `stmt_binds`
//! / `expr_kids` / `expr_stmts` helpers below are EXHAUSTIVE over the IR
//! enums (no wildcards) — a new IR variant is a compile error here, which
//! forces a conscious allow/deny decision instead of silently missing a
//! mention (unsound) or silently vetoing (lost opt).

use crate::ir::{ArithAst, AssignTarget, IrExpr, IrStmt};
use std::collections::BTreeSet;

// ---------- exhaustive traversal helpers (no wildcards) ----------

/// Immediate child STATEMENTS of a stmt.
fn stmt_kids(s: &IrStmt) -> Vec<&IrStmt> {
    match s {
        IrStmt::Assign { .. }
        | IrStmt::Declare { .. }
        | IrStmt::DeclareArray { .. }
        | IrStmt::Continue
        | IrStmt::Break
        | IrStmt::Die { .. }
        | IrStmt::Warn { .. }
        | IrStmt::Exec { .. }
        | IrStmt::Return(_)
        | IrStmt::Exit(_)
        | IrStmt::SetChildError(_)
        | IrStmt::Require(_)
        | IrStmt::RawText(_)
        | IrStmt::Label(_)
        | IrStmt::Goto(_)
        | IrStmt::Output { .. }
        | IrStmt::WriteFile { .. }
        | IrStmt::Expr(_) => vec![],
        IrStmt::If { then, elsifs, else_, .. } => {
            let mut k: Vec<&IrStmt> = Vec::new();
            k.extend(then.iter());
            for (_, b) in elsifs {
                k.extend(b.iter());
            }
            k.extend(else_.iter());
            k
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            let mut k: Vec<&IrStmt> = Vec::new();
            k.extend(body.iter());
            for e in excepts {
                k.extend(e.body.iter());
            }
            k.extend(else_body.iter());
            k.extend(finally_body.iter());
            k
        }
        IrStmt::For { body, .. } => body.iter().collect(),
        IrStmt::ForInit { init, step, body, .. } => {
            let mut k: Vec<&IrStmt> = Vec::new();
            k.extend(init.iter());
            k.extend(step.iter());
            k.extend(body.iter());
            k
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } => body.iter().collect(),
        IrStmt::Pipeline { stages, .. } => stages.iter().flatten().collect(),
        IrStmt::Case { clauses, .. } => clauses.iter().flat_map(|c| c.body.iter()).collect(),
        IrStmt::Redirect { inner, .. } => inner.iter().collect(),
        IrStmt::Function { body, named_blocks, .. } => {
            let mut k: Vec<&IrStmt> = Vec::new();
            k.extend(body.iter());
            for (_, b) in named_blocks {
                k.extend(b.iter());
            }
            k
        }
        IrStmt::Subshell(b) | IrStmt::Background(b) | IrStmt::Block(b) => b.iter().collect(),
        IrStmt::Select { clauses } => clauses.iter().flat_map(|c| c.body.iter()).collect(),
        IrStmt::Asm { .. } => vec![],
        IrStmt::Shaderize { body, .. } => body.iter().collect(),
        IrStmt::Ext(x) => x.children(),
    }
}

/// Immediate child EXPRESSIONS of a stmt (conditions, values, targets…).
fn stmt_exprs(s: &IrStmt) -> Vec<&IrExpr> {
    match s {
        IrStmt::Assign { expr, .. } => vec![expr],
        IrStmt::Declare { init, .. } => init.iter().collect(),
        IrStmt::DeclareArray { elements, .. } => elements.iter().collect(),
        IrStmt::If { cond, elsifs, .. } => {
            let mut k = vec![cond];
            for (c, _) in elsifs {
                k.push(c);
            }
            k
        }
        IrStmt::Try { excepts, .. } => excepts.iter().filter_map(|e| e.match_expr.as_ref()).collect(),
        IrStmt::For { iter, .. } => vec![iter],
        IrStmt::ForInit { cond, .. } => vec![cond],
        IrStmt::While { cond, .. } | IrStmt::DoWhile { cond, .. } => vec![cond],
        IrStmt::Die { expr, .. } | IrStmt::Warn { expr, .. } => vec![expr],
        IrStmt::Exec { cmd, args, redirects, .. } => {
            let mut k = vec![cmd];
            k.extend(args.iter());
            k.extend(redirects.iter());
            k
        }
        IrStmt::Pipeline { .. } => vec![],
        IrStmt::Return(e) | IrStmt::Exit(e) => e.iter().collect(),
        IrStmt::SetChildError(e) => vec![e],
        IrStmt::Case { discriminant, .. } => vec![discriminant],
        IrStmt::Redirect { redirects, .. } => redirects.iter().map(|r| &r.target).collect(),
        IrStmt::Output { value, .. } => vec![value],
        IrStmt::WriteFile { path, content, .. } => vec![path, content],
        IrStmt::Expr(e) => vec![e],
        IrStmt::Select { clauses } => {
            let mut k = Vec::new();
            for c in clauses {
                k.extend(c.ch.iter());
                k.extend(c.value.iter());
            }
            k
        }
        IrStmt::Asm { outputs, inputs, .. } => {
            let mut k = Vec::new();
            for (_, e) in outputs.iter().chain(inputs.iter()) {
                k.push(e);
            }
            k
        }
        IrStmt::Shaderize { .. } => vec![],
        IrStmt::Function { .. }
        | IrStmt::Subshell(_)
        | IrStmt::Background(_)
        | IrStmt::Block(_)
        | IrStmt::Continue
        | IrStmt::Break
        | IrStmt::Require(_)
        | IrStmt::RawText(_)
        | IrStmt::Label(_)
        | IrStmt::Goto(_) => vec![],
        IrStmt::Ext(_) => vec![],
    }
}

/// Names a stmt BINDS (conservative: any of these equal to a watched
/// name counts as a mention).
fn stmt_binds(s: &IrStmt) -> Vec<&str> {
    match s {
        IrStmt::Declare { vars, .. } => vars.iter().map(|d| d.name.as_str()).collect(),
        IrStmt::DeclareArray { var, .. } => vec![var.as_str()],
        IrStmt::For { var, .. } => vec![var.as_str()],
        IrStmt::Shaderize { var, .. } => vec![var.as_str()],
        IrStmt::Exec { capture, .. } => capture.iter().map(|c| c.as_str()).collect(),
        IrStmt::Select { clauses } => clauses.iter().filter_map(|c| c.target.as_deref()).collect(),
        IrStmt::Try { excepts, .. } => {
            excepts.iter().filter_map(|e| e.as_name.as_deref()).collect()
        }
        IrStmt::Asm { outputs, inputs, .. } => outputs
            .iter()
            .map(|(n, _)| n.as_str())
            .chain(inputs.iter().map(|(n, _)| n.as_str()))
            .collect(),
        _ => vec![],
    }
}

/// Immediate child EXPRESSIONS of an expr. NOTE: `Arith` yields no
/// kids (type boundary) — every expr-scanner MUST handle
/// `IrExpr::Arith` explicitly via its arith-level twin, or array/var
/// mentions inside `$((…))` go invisible (unsound).
fn expr_kids(e: &IrExpr) -> Vec<&IrExpr> {
    match e {
        IrExpr::Int(_)
        | IrExpr::Str(_, _)
        | IrExpr::Var(_, _)
        | IrExpr::Ident(_)
        | IrExpr::Regex { .. }
        | IrExpr::Range { .. }
        | IrExpr::RawExpr(_)
        | IrExpr::Bool(_)
        | IrExpr::Json(_) => vec![],
        IrExpr::Index { key, .. } => vec![key],
        IrExpr::BinOp { lhs, rhs, .. } => vec![lhs, rhs],
        IrExpr::Call { args, .. } => args.iter().collect(),
        IrExpr::MethodCall { obj, args, .. } => {
            let mut k = vec![obj.as_ref()];
            k.extend(args.iter());
            k
        }
        IrExpr::Ternary { cond, then, else_ } => vec![cond, then, else_],
        IrExpr::DefinedOr { expr, default } => vec![expr, default],
        IrExpr::Interpolate(parts) => parts
            .iter()
            .filter_map(|p| match p {
                crate::ir::InterpPart::Lit(_) => None,
                crate::ir::InterpPart::Expr(x) => Some(x.as_ref()),
            })
            .collect(),
        IrExpr::Capture { expr, .. } => vec![expr],
        IrExpr::Array(els) => els.iter().collect(),
        IrExpr::ArrayComp { iter, elem, cond, .. } => {
            let mut k = vec![iter.as_ref(), elem.as_ref()];
            k.extend(cond.iter().map(|c| c.as_ref()));
            k
        }
        IrExpr::Lambda { .. } | IrExpr::Arrow(_) => vec![],
        IrExpr::Arith(_) => vec![],
        IrExpr::Object(pairs) => pairs.iter().map(|(_, v)| v).collect(),
        IrExpr::Splice(x) => vec![x],
        IrExpr::Ext(x) => x.children(),
    }
}

/// Statement bodies nested inside an expr (Lambda / Arrow).
fn expr_stmts(e: &IrExpr) -> Vec<&IrStmt> {
    match e {
        IrExpr::Lambda { body, .. } => body.iter().collect(),
        IrExpr::Arrow(b) => b.iter().collect(),
        _ => vec![],
    }
}

/// Immediate child ARITH nodes of an arith node (closed enum — no Ext).
fn arith_kids(a: &ArithAst) -> Vec<&ArithAst> {
    match a {
        ArithAst::Num(_) | ArithAst::Var(_) | ArithAst::Ident(_) | ArithAst::Sizeof(_) => vec![],
        ArithAst::Index { key, .. } => vec![key],
        ArithAst::Bin { lhs, rhs, .. } => vec![lhs, rhs],
        ArithAst::Un { arg, .. } => vec![arg],
        ArithAst::Cond { test, then, else_ } => vec![test, then, else_],
        ArithAst::Assign { rhs, .. } => vec![rhs],
        ArithAst::IncDec { .. } => vec![],
        ArithAst::Cast { arg, .. } => vec![arg],
    }
}

// ---------- entry ----------

/// Apply the transform to one top-level statement list. Returns whether
/// anything changed. Sweeps to fixpoint (each fusion removes a loop, so
/// this terminates); nested blocks are never fused, only veto sources.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    let mut changed = false;
    loop {
        let mut actions = find_fusions(stmts);
        if actions.is_empty() {
            break;
        }
        // Descending l1 so removals don't disturb pending indices
        // (verified actions are always disjoint — see find_fusions).
        actions.sort_by(|a, b| b.l1.cmp(&a.l1));
        for a in &actions {
            apply_one(stmts, a);
        }
        changed = true;
    }
    changed
}

struct Action {
    l1: usize,
    l2: usize,
    new_body: Vec<IrStmt>,
}

fn apply_one(stmts: &mut Vec<IrStmt>, a: &Action) {
    stmts.remove(a.l1);
    // l2 > l1 always (a non-negative gap sits between), so it shifts.
    let l2 = a.l2 - 1;
    if let Some(IrStmt::ForInit { body, .. }) = stmts.get_mut(l2) {
        *body = a.new_body.clone();
    }
}

/// One verified fusion per disjoint pair (pairs are disjoint: a loop
/// that is a consumer (scalar body) can't be a fill (single store) and
/// vice versa, and any third mention of the array vetoes).
fn find_fusions(stmts: &[IrStmt]) -> Vec<Action> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < stmts.len() {
        let (fill, counter, bound, init, cond, step) = match &stmts[i] {
            IrStmt::ForInit { init, cond, step, body } => {
                let Some((counter, _)) = counter_of_init(init) else {
                    i += 1;
                    continue;
                };
                let Some(f) = fill_shape(body, &counter) else {
                    i += 1;
                    continue;
                };
                if !header_pure(init, cond, step, &counter) {
                    i += 1;
                    continue;
                }
                // The bound is the loop-invariant every trip count
                // depends on — extract it from the cond text so gap
                // writes to it veto (`i<n` form; unknown → only an
                // exactly-empty gap can pass, since any gap write to an
                // unknown name can't be exonerated… actually unknown
                // bound just skips that one check — the trip counts are
                // still equal because headers are structurally equal and
                // neither loop body writes through the header names…
                // NO: gap writes could change the bound VALUE between
                // the loops even with equal header TEXT. With unknown
                // bound name, a gap write is harmless only if it can't
                // affect the cond — unprovable → veto ANY gap write when
                // the bound is unknown. Enforced in the gap loop below.)
                let bound = bound_of_cond(cond, &counter);
                (f, counter, bound, init, cond, step)
            }
            _ => {
                i += 1;
                continue;
            }
        };
        // Scan forward: plain assigns, then a same-header consumer.
        let mut gap_writes: BTreeSet<String> = BTreeSet::new();
        let mut j = i + 1;
        let mut consumer: Option<usize> = None;
        while j < stmts.len() {
            match &stmts[j] {
                IrStmt::Assign { targets, expr, .. } => {
                    if bound.is_none() {
                        // Unknown bound: any gap write could shift trip
                        // counts between the loops → veto.
                        break;
                    }
                    if !gap_assign_ok(targets, expr, &counter, bound.as_deref(), &fill) {
                        break;
                    }
                    gap_writes.insert(targets[0].var.clone());
                    j += 1;
                }
                IrStmt::ForInit { init: i2, cond: c2, step: s2, .. } => {
                    if init == i2 && cond == c2 && step == s2 {
                        consumer = Some(j);
                    }
                    break;
                }
                _ => break,
            }
        }
        let Some(l2) = consumer else {
            i += 1;
            continue;
        };
        let IrStmt::ForInit { body: b2, .. } = &stmts[l2] else {
            i += 1;
            continue;
        };
        // Names the trip count may read (over-approx): the consumer
        // must not write them (self-modifying trips would diverge from
        // the fill's coverage). The counter is excluded (step machinery
        // owns it; body writes to it veto separately).
        let mut condvars = BTreeSet::new();
        collect_names_expr(cond, &mut condvars);
        for t in cond_texts(cond) {
            lex_names(&t, &mut condvars);
        }
        condvars.remove(&counter);
        let Some(new_body) =
            consume_shape(b2, &counter, bound.as_deref(), &condvars, &fill, &gap_writes)
        else {
            i += 1;
            continue;
        };
        // Whole-program array discipline: every mention of the array is
        // the fill store or a forwarded consumer read.
        if array_used_elsewhere(stmts, &fill, i, l2, &counter) {
            i += 1;
            continue;
        }
        out.push(Action { l1: i, l2, new_body });
        // Continue after the consumer (pairs are disjoint by the
        // array-discipline check — but keep scanning for OTHER arrays).
        i = l2 + 1;
    }
    out
}

// ---------- fill shape ----------

/// One array's fill binding: the store target and the expression to
/// forward at each `arr[<i>]` read.
struct FillStore {
    arr: String,
    rhs: ArithAst,
}

/// A fill loop's stores (one per array) with the union of RHS free vars.
/// MULTI-STORE since the twoarr shape (`a[$i]=..; b[$i]=..`) is the same
/// rewrite applied per array — the CUDA map candidacy already accepted
/// multi-store fills, so the shIR transform was the odd one out.
struct Fill {
    stores: Vec<FillStore>,
    /// Union of RHS free vars, excluding the counter.
    free: BTreeSet<String>,
}

impl Fill {
    /// Store for `arr`, if the fill writes it.
    fn rhs(&self, arr: &str) -> Option<&ArithAst> {
        self.stores.iter().find(|s| s.arr == arr).map(|s| &s.rhs)
    }
    /// Does a NAME (possibly the flattened `a[...]` text) name any
    /// filled array?
    fn name_hits(&self, v: &str) -> bool {
        self.stores.iter().any(|s| v == s.arr || v.starts_with(&format!("{}[", s.arr)))
    }
    fn arrays(&self) -> impl Iterator<Item = &String> {
        self.stores.iter().map(|s| &s.arr)
    }
    fn expr_hits(&self, e: &IrExpr) -> bool {
        self.stores.iter().any(|s| expr_has_arr(e, &s.arr))
    }
    fn stmt_hits(&self, s: &IrStmt) -> bool {
        self.stores.iter().any(|fs| stmt_has_arr(s, &fs.arr))
    }
    fn stmts_hit(&self, v: &[IrStmt]) -> bool {
        self.stores.iter().any(|s| stmts_have_arr(v, &s.arr))
    }
    /// Does any ArithAst node inside a key expr name a filled array?
    fn expr_arith_hits(&self, a: &ArithAst) -> bool {
        self.stores.iter().any(|s| arith_has_arr(a, &s.arr))
    }
}

/// One or more `A[<i>] = <pure arith>` stores, each with a plain-`i`
/// index and a DISTINCT array (a repeated array would make the
/// forwarding target ambiguous — veto).
fn fill_shape(body: &[IrStmt], counter: &str) -> Option<Fill> {
    let mut stores: Vec<FillStore> = Vec::new();
    let mut free: BTreeSet<String> = BTreeSet::new();
    for st in body {
        let IrStmt::Assign { targets, expr, .. } = st else {
            return None;
        };
        let [t] = targets.as_slice() else {
            return None;
        };
        let (arr, key) = store_target(t, counter)?;
        if key != (1, 0) {
            return None;
        }
        let IrExpr::Arith(f) = expr else {
            return None;
        };
        if !pure_fill_rhs(f) {
            return None;
        }
        if stores.iter().any(|s| s.arr == arr) {
            return None;
        }
        arith_free_vars(f, &mut free);
        stores.push(FillStore { arr, rhs: (**f).clone() });
    }
    if stores.is_empty() {
        return None;
    }
    free.remove(counter);
    // Special vars (`$?`, `$@`, positionals) are statement-sensitive —
    // their values differ between fill-time and consume-time.
    if free.iter().any(|v| !is_plain_scalar(v)) {
        return None;
    }
    Some(Fill { stores, free })
}

/// `(array, (a, b))` for an array-store target in `counter`, or None.
/// Accepts the flattened `A[$i]` text form and the structured
/// single-index form.
fn store_target(t: &AssignTarget, counter: &str) -> Option<(String, (i64, i64))> {
    if t.indices.len() == 1 {
        let base = t.var.split('[').next().unwrap_or(&t.var).to_string();
        if base.is_empty()
            || !base.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return None;
        }
        let key = index_expr_affine_in(&t.indices[0], counter)?;
        return Some((base, key));
    }
    if !t.indices.is_empty() {
        return None;
    }
    let (name, idx) = split_target(&t.var)?;
    let key = affine_of_text(idx, counter)?;
    Some((name.to_string(), key))
}

/// Split flattened `name[...]` into (name, index-text).
fn split_target(var: &str) -> Option<(&str, &str)> {
    let open = var.rfind('[')?;
    if !var.ends_with(']') {
        return None;
    }
    let name = &var[..open];
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((name, &var[open + 1..var.len() - 1]))
}

/// Affine index of a structured key expr in a KNOWN counter.
fn index_expr_affine_in(key: &IrExpr, counter: &str) -> Option<(i64, i64)> {
    match key {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => {
            let v = v.strip_prefix('$').unwrap_or(v);
            if v == counter {
                Some((1, 0))
            } else {
                None
            }
        }
        IrExpr::Str(s, _) => affine_of_text(s, counter),
        IrExpr::Int(n) => Some((0, *n)),
        IrExpr::Arith(a) => arith_affine(a, counter),
        _ => None,
    }
}

/// Parse text affine `i | $i | i±N | N+i | N-i | N*i | i*N | N` in counter.
fn affine_of_text(text: &str, counter: &str) -> Option<(i64, i64)> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let t = t.strip_prefix('$').unwrap_or(&t);
    if t == counter {
        return Some((1, 0));
    }
    if let Ok(n) = t.parse::<i64>() {
        return Some((0, n));
    }
    if let Some((l, r)) = t.split_once('*') {
        let (l, r) = (l.strip_prefix('$').unwrap_or(l), r.strip_prefix('$').unwrap_or(r));
        if l == counter {
            return r.parse::<i64>().ok().map(|n| (n, 0));
        }
        if r == counter {
            return l.parse::<i64>().ok().map(|n| (n, 0));
        }
        return None;
    }
    if let Some((l, r)) = t.split_once('+') {
        let l = l.strip_prefix('$').unwrap_or(l);
        if l == counter {
            return r.parse::<i64>().ok().map(|n| (1, n));
        }
        let r = r.strip_prefix('$').unwrap_or(r);
        if r == counter {
            return l.parse::<i64>().ok().map(|n| (1, n));
        }
        return None;
    }
    if let Some(pos) = t.rfind('-') {
        if pos > 0 {
            let (l, r) = t.split_at(pos);
            let r = &r[1..];
            let l = l.strip_prefix('$').unwrap_or(l);
            if l == counter {
                return r.parse::<i64>().ok().map(|n| (1, -n));
            }
            if r.strip_prefix('$').unwrap_or(r) == counter {
                return l.parse::<i64>().ok().map(|n| (-1, n));
            }
        }
        return None;
    }
    None
}

/// Affine (a, b) with idx = a*counter + b, or None.
fn arith_affine(a: &ArithAst, counter: &str) -> Option<(i64, i64)> {
    match a {
        // A `Cast` is a value coercion (identity for the i64 index
        // model): treat it as transparent so frontend-emitted keys like
        // `a[Cast(i)]` forward. The forwarded RHS keeps its own casts.
        ArithAst::Cast { arg, .. } => arith_affine(arg, counter),
        ArithAst::Num(n) => Some((0, *n)),
        ArithAst::Var(v) | ArithAst::Ident(v) => {
            if v == counter {
                Some((1, 0))
            } else {
                None
            }
        }
        ArithAst::Bin { op, lhs, rhs } => {
            let (l, r) = (arith_affine(lhs, counter), arith_affine(rhs, counter));
            match op.as_str() {
                "+" => {
                    let ((a1, b1), (a2, b2)) = (l?, r?);
                    Some((a1.checked_add(a2)?, b1.checked_add(b2)?))
                }
                "-" => {
                    let ((a1, b1), (a2, b2)) = (l?, r?);
                    Some((a1.checked_sub(a2)?, b1.checked_sub(b2)?))
                }
                "*" => match (l?, r?) {
                    ((0, c), (a, b)) | ((a, b), (0, c)) => {
                        Some((a.checked_mul(c)?, b.checked_mul(c)?))
                    }
                    _ => None,
                },
                _ => None,
            }
        }
        ArithAst::Un { op, arg } => {
            let (a, b) = arith_affine(arg, counter)?;
            match op.as_str() {
                "-" => Some((a.checked_neg()?, b.checked_neg()?)),
                "+" => Some((a, b)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Fill RHS purity: no Index (nested array reads), no Assign/IncDec
/// (side-effecting). (Calls can't appear in ArithAst; Sizeof/Cast are
/// deterministic.)
fn pure_fill_rhs(a: &ArithAst) -> bool {
    match a {
        ArithAst::Index { .. } | ArithAst::Assign { .. } | ArithAst::IncDec { .. } => false,
        _ => arith_kids(a).iter().all(|k| pure_fill_rhs(k)),
    }
}

fn arith_mentions_var(a: &ArithAst, name: &str) -> bool {
    let own = match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => v == name,
        ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => var == name,
        ArithAst::Index { var, .. } => var == name,
        _ => false,
    };
    own || arith_kids(a).iter().any(|k| arith_mentions_var(k, name))
}

fn arith_free_vars(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Var(v) | ArithAst::Ident(v) => {
            out.insert(v.clone());
        }
        ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        ArithAst::Index { var, .. } => {
            out.insert(var.clone());
        }
        _ => {}
    }
    for k in arith_kids(a) {
        arith_free_vars(k, out);
    }
}

/// Plain scalar name (excludes `$?`, `$@`, `$*`, `$#`, `$-`, `$!`,
/// positionals and anything non-identifier).
fn is_plain_scalar(name: &str) -> bool {
    let mut cs = name.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ---------- headers ----------

/// `(counter, lo)` from `v = NUM` init tail.
fn counter_of_init(init: &[IrStmt]) -> Option<(String, i64)> {
    let IrStmt::Assign { targets, expr, .. } = init.last()? else {
        return None;
    };
    let [t] = targets.as_slice() else {
        return None;
    };
    if !t.indices.is_empty() || t.var.contains('[') {
        return None;
    }
    let n = init_num(expr, &t.var)?;
    Some((t.var.clone(), n))
}

/// Init literal, Cast/Assign-tolerant (frontend `i = Cast(Int64, 0)`,
/// shell `i = "0"`).
fn init_num(expr: &IrExpr, var: &str) -> Option<i64> {
    match expr {
        IrExpr::Int(n) => Some(*n),
        IrExpr::Str(s, _) => s.trim().parse::<i64>().ok(),
        IrExpr::Arith(a) => match a.as_ref() {
            ArithAst::Assign { var: v, op, rhs } if v == var && op == "=" => arith_num(rhs),
            _ => arith_num(a),
        },
        _ => None,
    }
}

/// Numeric literal under Cast/Assign(=) wrapping.
fn arith_num(a: &ArithAst) -> Option<i64> {
    match a {
        ArithAst::Num(n) => Some(*n),
        ArithAst::Cast { arg, .. } => arith_num(arg),
        ArithAst::Assign { op, rhs, .. } if op == "=" => arith_num(rhs),
        _ => None,
    }
}

/// Strip frontend `Cast` markers (identity for the affine/i64 model).
fn strip_cast(a: &ArithAst) -> &ArithAst {
    match a {
        ArithAst::Cast { arg, .. } => strip_cast(arg),
        _ => a,
    }
}

/// The identifier a (possibly cast-wrapped) arith subtree names.
fn arith_ident(a: &ArithAst) -> Option<&str> {
    match strip_cast(a) {
        ArithAst::Var(v) | ArithAst::Ident(v) => Some(v.as_str()),
        _ => None,
    }
}

/// A `let`-evaluation call in cond position: the shell path lowers
/// `for ((…))` conds to `exec("let", […])` and a later transform
/// refines it to `builtin` — accept both (same inert-text audit).
fn bound_of_cond(cond: &IrExpr, counter: &str) -> Option<String> {
    // Structured-Arith form (the frontends' `for i in range(N)` / c-style
    // loops): `i < N` / `i <= N` with a bare bound ident. A literal bound
    // returns None (no dynamic bound name — the gap-write discipline then
    // refuses any gap, fail-closed).
    if let IrExpr::Arith(a) = cond {
        if let ArithAst::Bin { op, lhs, rhs } = strip_cast(a) {
            if op == "<" || op == "<=" {
                if arith_ident(lhs) == Some(counter) {
                    if let Some(b) = arith_ident(rhs) {
                        if b.parse::<i64>().is_err() {
                            return Some(b.to_string());
                        }
                    }
                }
            }
        }
        return None;
    }
    // Shared shape check with cond_pure/cond_texts (exec→builtin).
    let args = let_call_args(cond)?;
    let mut texts = Vec::new();
    for a in args {
        match a {
            IrExpr::Str(s, _) => texts.push(s.clone()),
            IrExpr::Array(els) => {
                for e in els {
                    if let IrExpr::Str(s, _) = e {
                        texts.push(s.clone());
                    }
                }
            }
            _ => {}
        }
    }
    for t in texts {
        let t: String = t.chars().filter(|c| !c.is_whitespace()).collect();
        for op in ["<=", "<"] {
            if let Some((l, r)) = t.split_once(op) {
                let l = l.strip_prefix('$').unwrap_or(l);
                if l == counter {
                    let r = r.strip_prefix('$').unwrap_or(r);
                    if is_plain_scalar(r) {
                        return Some(r.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Fill headers must be side-effect free AND write only the counter
/// (their evaluations vanish with the loop): init/step are counter
/// Assigns, no Call/Capture/MethodCall/RawExpr anywhere in header
/// exprs, and no Assign/IncDec nodes inside header arithmetic (an
/// `i++` in the cond would vanish). Headers are structurally equal,
/// so checking the fill's headers covers the consumer's.
fn header_pure(init: &[IrStmt], cond: &IrExpr, step: &[IrStmt], counter: &str) -> bool {
    for s in init.iter().chain(step.iter()) {
        // Vanishing writes: only the counter may be written.
        let IrStmt::Assign { targets, expr, .. } = s else {
            return false;
        };
        let [t] = targets.as_slice() else {
            return false;
        };
        if t.var != counter || !t.indices.is_empty() {
            return false;
        }
        if expr_has_call(expr) || expr_has_closure_or_ext(expr) {
            return false;
        }
        // Arith effects (IncDec/Assign nodes) may only hit the counter
        // (the step machinery owns it; it runs identically fused).
        let mut eff = BTreeSet::new();
        collect_arith_effects_expr(expr, &mut eff);
        eff.remove(counter);
        if !eff.is_empty() {
            return false;
        }
    }
    if !cond_pure(cond, counter) {
        return false;
    }
    let mut eff = BTreeSet::new();
    collect_arith_effects_expr(cond, &mut eff);
    eff.remove(counter);
    eff.is_empty()
}

/// Loop-cond purity. The shell path lowers `for ((i=0;i<n;i++))` conds
/// to a `builtin("let", ["i<n"])` CALL — a call node that is pure
/// arithmetic text, so it gets a carve-out with a text-effect audit
/// (no `$`/backtick expansions, no `=`/`++`/`--` after comparison-op
/// removal). Anything else (test calls, `[ … ]`) vetoes — V2 may
/// allowlist side-effect-free test shapes.
fn cond_pure(cond: &IrExpr, _counter: &str) -> bool {
    // (Shared shape check with bound_of_cond/cond_texts.)
    if let_call_args(cond).is_some() {
        return cond_texts(cond).iter().all(|t| let_text_pure(t));
    }
    // Any other call in cond position has unknown effects (test calls,
    // `[ … ]`) — veto. V2 may allowlist pure test shapes.
    if expr_has_call(cond) || expr_has_closure_or_ext(cond) {
        return false;
    }
    true
}

/// `let`-evaluation call args in cond position (`exec` pre-refinement
/// or `builtin` post-refinement), or None. Requires the exact
/// placeholder shape: callee `exec`/`builtin`, first arg Str"let".
fn let_call_args(cond: &IrExpr) -> Option<&Vec<IrExpr>> {
    let IrExpr::Call { func, args } = cond else {
        return None;
    };
    if func != "builtin" && func != "exec" {
        return None;
    }
    match args.first() {
        Some(IrExpr::Str(name, _)) if name == "let" => Some(args),
        _ => None,
    }
}

/// Inert arithmetic text (after the carve-out above): no shell
/// expansions, no assignments/incdec. Comparisons are stripped first
/// (`<=` contains `=`, but is pure).
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

/// The code texts of a builtin-let cond (skips the `let` name itself).
fn cond_texts(cond: &IrExpr) -> Vec<String> {
    let mut out = Vec::new();
    let Some(args) = let_call_args(cond) else {
        return out;
    };
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
    out
}

/// Lex identifier-like names from code text (over-approx for condvars).
fn lex_names(text: &str, out: &mut BTreeSet<String>) {
    for tok in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if is_plain_scalar(tok) {
            out.insert(tok.to_string());
        }
    }
}

fn stmt_has_call(s: &IrStmt) -> bool {
    stmt_kids(s).iter().any(|k| stmt_has_call(k))
        || stmt_exprs(s).iter().any(|e| expr_has_call(e))
}

fn expr_has_call(e: &IrExpr) -> bool {
    // Raw bridge text is opaque executable code (side effects unknown
    // statically) — treat as a call (veto) everywhere this is asked.
    if matches!(e, IrExpr::RawExpr(_)) {
        return true;
    }
    matches!(
        e,
        IrExpr::Call { .. } | IrExpr::Capture { .. } | IrExpr::MethodCall { .. }
    ) || expr_kids(e).iter().any(|k| expr_has_call(k))
        || expr_stmts(e).iter().any(|k| stmt_has_call(k))
}

// ---------- gap assigns ----------

/// Intervening assign discipline: single scalar target, no calls/`$?`,
/// no mention of the counter or array, target isn't the bound or an
/// F free var (F evaluates at fill-time originally, consume-time fused).
fn gap_assign_ok(
    targets: &[AssignTarget],
    expr: &IrExpr,
    counter: &str,
    bound: Option<&str>,
    fill: &Fill,
) -> bool {
    let [t] = targets else { return false };
    if !t.indices.is_empty() || t.var.contains('[') {
        return false;
    }
    if fill.name_hits(&t.var) || &t.var == counter {
        return false;
    }
    if bound.is_some_and(|b| t.var == b) {
        return false;
    }
    if fill.free.contains(&t.var) {
        return false;
    }
    if expr_mentions_status_or_call(expr) {
        return false;
    }
    if expr_mentions_var(expr, counter) || fill.expr_hits(expr) {
        return false;
    }
    true
}

/// Any Call/Capture/MethodCall or `$?` read (status-sensitive: the fill's
/// deletion shifts which statement's status `$?` observes).
fn expr_mentions_status_or_call(e: &IrExpr) -> bool {
    expr_has_call(e) || expr_mentions_var(e, "?")
}

/// Every Var/Ident/target/binder name in an expr (over-approx for the
/// veto direction: the consumer must not write trip-count inputs).
fn collect_names_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    if let IrExpr::Arith(a) = e {
        arith_free_vars(a, out);
        return;
    }
    match e {
        IrExpr::Var(v, _) | IrExpr::Ident(v) => {
            out.insert(v.clone());
        }
        _ => {}
    }
    for k in expr_kids(e) {
        collect_names_expr(k, out);
    }
    for st in expr_stmts(e) {
        collect_names_stmt(st, out);
    }
}

fn collect_names_stmt(s: &IrStmt, out: &mut BTreeSet<String>) {
    match s {
        IrStmt::Assign { targets, .. } => {
            for t in targets {
                out.insert(t.var.clone());
            }
        }
        _ => {}
    }
    for b in stmt_binds(s) {
        out.insert(b.to_string());
    }
    for k in stmt_kids(s) {
        collect_names_stmt(k, out);
    }
    for e in stmt_exprs(s) {
        collect_names_expr(e, out);
    }
}

/// Var/Ident mention of `name` anywhere in an IrExpr (incl. nested
/// bodies, interpolation, Ext children).
fn expr_mentions_var(e: &IrExpr, name: &str) -> bool {
    if let IrExpr::Arith(a) = e {
        return arith_mentions_var(a, name);
    }
    let own = matches!(e, IrExpr::Var(v, _) | IrExpr::Ident(v) if v == name);
    own || expr_kids(e).iter().any(|k| expr_mentions_var(k, name))
        || expr_stmts(e).iter().any(|k| stmt_mentions_var(k, name))
}

fn stmt_mentions_var(s: &IrStmt, name: &str) -> bool {
    // Raw bridge text is opaque (may name anything) — veto.
    if matches!(s, IrStmt::RawText(_)) {
        return true;
    }
    let own = match s {
        IrStmt::Assign { targets, .. } => targets.iter().any(|t| t.var == name),
        _ => stmt_binds(s).iter().any(|b| *b == name),
    };
    own || stmt_kids(s).iter().any(|k| stmt_mentions_var(k, name))
        || stmt_exprs(s).iter().any(|e| expr_mentions_var(e, name))
        || stmt_targets_mentioned(s, name)
}

/// Index-key mentions inside assign targets (`b[a[i]]`).
fn stmt_targets_mentioned(s: &IrStmt, name: &str) -> bool {
    match s {
        IrStmt::Assign { targets, .. } => targets
            .iter()
            .any(|t| t.indices.iter().any(|k| expr_mentions_var(k, name))),
        _ => false,
    }
}

// ---------- consumer ----------

/// Consumer discipline + forwarding. Returns the rewritten body, or None
/// (veto). Every stmt is a scalar Assign (counter/bound never written);
/// every `A[<i>]` arith read becomes F; at least one read must forward;
/// F's free vars are untouched by the consumer and gap.
fn consume_shape(
    body: &[IrStmt],
    counter: &str,
    bound: Option<&str>,
    condvars: &BTreeSet<String>,
    fill: &Fill,
    gap_writes: &BTreeSet<String>,
) -> Option<Vec<IrStmt>> {
    let mut written: BTreeSet<String> = BTreeSet::new();
    let mut new_body = Vec::with_capacity(body.len());
    let mut forwarded = 0usize;
    for s in body {
        let IrStmt::Assign { targets, .. } = s else {
            return None;
        };
        let [t] = targets.as_slice() else {
            return None;
        };
        if !t.indices.is_empty() || t.var.contains('[') || fill.name_hits(&t.var) {
            return None;
        }
        if &t.var == counter
            || bound.is_some_and(|b| t.var == b)
            || condvars.contains(&t.var)
        {
            return None;
        }
        written.insert(t.var.clone());
        let IrStmt::Assign { expr, .. } = s else {
            return None;
        };
        let (new_expr, n) = substitute_reads(expr, fill, counter)?;
        forwarded += n;
        let mut s2 = s.clone();
        if let IrStmt::Assign { expr: e, .. } = &mut s2 {
            *e = new_expr;
        }
        new_body.push(s2);
    }
    if forwarded == 0 {
        return None;
    }
    if !fill.free.iter().all(|v| !written.contains(v) && !gap_writes.contains(v)) {
        return None;
    }
    Some(new_body)
}

/// Replace `A[<i>]` arith reads with F. Returns (new_expr, count), or
/// None on any non-forwardable A mention (wrong key, word-level read,
/// raw bridge text).
fn substitute_reads(
    expr: &IrExpr,
    fill: &Fill,
    counter: &str,
) -> Option<(IrExpr, usize)> {
    struct Sub<'a> {
        fill: &'a Fill,
        counter: &'a str,
        count: usize,
    }
    impl Sub<'_> {
        fn arith(&mut self, a: &ArithAst) -> Option<ArithAst> {
            match a {
                // `arr[<i>]` on a FILLED array forwards that array's RHS.
                // Reads of other arrays pass through (the whole-program
                // discipline check vetoes them before commit).
                ArithAst::Index { var, key } if self.fill.rhs(var).is_some() => {
                    let f = self.fill.rhs(var)?.clone();
                    match arith_affine(key, self.counter) {
                        Some((1, 0)) => {
                            self.count += 1;
                            Some(f)
                        }
                        _ => None,
                    }
                }
                ArithAst::Bin { op, lhs, rhs } => Some(ArithAst::Bin {
                    op: op.clone(),
                    lhs: Box::new(self.arith(lhs)?),
                    rhs: Box::new(self.arith(rhs)?),
                }),
                ArithAst::Un { op, arg } => Some(ArithAst::Un {
                    op: op.clone(),
                    arg: Box::new(self.arith(arg)?),
                }),
                ArithAst::Cond { test, then, else_ } => Some(ArithAst::Cond {
                    test: Box::new(self.arith(test)?),
                    then: Box::new(self.arith(then)?),
                    else_: Box::new(self.arith(else_)?),
                }),
                ArithAst::Assign { var, op, rhs } => {
                    if self.fill.name_hits(var) {
                        return None;
                    }
                    Some(ArithAst::Assign {
                        var: var.clone(),
                        op: op.clone(),
                        rhs: Box::new(self.arith(rhs)?),
                    })
                }
                ArithAst::IncDec { var, .. } => {
                    if self.fill.name_hits(var) {
                        return None;
                    }
                    Some(a.clone())
                }
                ArithAst::Cast { ty, arg } => Some(ArithAst::Cast {
                    ty: ty.clone(),
                    arg: Box::new(self.arith(arg)?),
                }),
                ArithAst::Index { var, key } => Some(ArithAst::Index {
                    var: var.clone(),
                    key: Box::new(self.arith(key)?),
                }),
                _ => Some(a.clone()),
            }
        }
        fn expr(&mut self, e: &IrExpr) -> Option<IrExpr> {
            // Whole-array scalar reads and word-level element reads are
            // not forwardable (v1: miss, safe).
            match e {
                IrExpr::Var(v, _) | IrExpr::Ident(v) if self.fill.name_hits(v) => return None,
                IrExpr::Index { var, .. } if self.fill.name_hits(var) => return None,
                IrExpr::RawExpr(_) => return None,
                _ => {}
            }
            // Nested statement bodies keep strict discipline (any array
            // mention vetoes — no forwarding inside closures).
            for st in expr_stmts(e) {
                if self.fill.stmt_hits(st) {
                    return None;
                }
            }
            match e {
                IrExpr::Arith(a) => Some(IrExpr::Arith(Box::new(self.arith(a)?))),
                IrExpr::BinOp { lhs, op, rhs } => Some(IrExpr::BinOp {
                    lhs: Box::new(self.expr(lhs)?),
                    op: op.clone(),
                    rhs: Box::new(self.expr(rhs)?),
                }),
                IrExpr::Ternary { cond, then, else_ } => Some(IrExpr::Ternary {
                    cond: Box::new(self.expr(cond)?),
                    then: Box::new(self.expr(then)?),
                    else_: Box::new(self.expr(else_)?),
                }),
                IrExpr::DefinedOr { expr, default } => Some(IrExpr::DefinedOr {
                    expr: Box::new(self.expr(expr)?),
                    default: Box::new(self.expr(default)?),
                }),
                IrExpr::Interpolate(parts) => {
                    let mut np = Vec::with_capacity(parts.len());
                    for p in parts {
                        match p {
                            crate::ir::InterpPart::Lit(s) => {
                                // Inert text — but bridge text naming the
                                // array stays a veto (caught above only
                                // for RawExpr; interpolation Lit is inert
                                // shell text, keep it).
                                np.push(crate::ir::InterpPart::Lit(s.clone()))
                            }
                            crate::ir::InterpPart::Expr(x) => {
                                np.push(crate::ir::InterpPart::Expr(Box::new(self.expr(x)?)))
                            }
                        }
                    }
                    Some(IrExpr::Interpolate(np))
                }
                IrExpr::Call { func, args } => {
                    let mut na = Vec::with_capacity(args.len());
                    for x in args {
                        na.push(self.expr(x)?);
                    }
                    Some(IrExpr::Call { func: func.clone(), args: na })
                }
                IrExpr::MethodCall { obj, method, args } => {
                    let mut na = Vec::with_capacity(args.len());
                    for x in args {
                        na.push(self.expr(x)?);
                    }
                    Some(IrExpr::MethodCall {
                        obj: Box::new(self.expr(obj)?),
                        method: method.clone(),
                        args: na,
                    })
                }
                IrExpr::Array(els) => {
                    let mut ne = Vec::with_capacity(els.len());
                    for x in els {
                        ne.push(self.expr(x)?);
                    }
                    Some(IrExpr::Array(ne))
                }
                IrExpr::Capture { expr, native } => Some(IrExpr::Capture {
                    expr: Box::new(self.expr(expr)?),
                    native: *native,
                }),
                IrExpr::ArrayComp { var, iter, elem, cond } => Some(IrExpr::ArrayComp {
                    var: var.clone(),
                    iter: Box::new(self.expr(iter)?),
                    elem: Box::new(self.expr(elem)?),
                    cond: match cond {
                        Some(c) => Some(Box::new(self.expr(c)?)),
                        None => None,
                    },
                }),
                IrExpr::Object(pairs) => {
                    let mut no = Vec::with_capacity(pairs.len());
                    for (k, v) in pairs {
                        no.push((k.clone(), self.expr(v)?));
                    }
                    Some(IrExpr::Object(no))
                }
                IrExpr::Splice(x) => Some(IrExpr::Splice(Box::new(self.expr(x)?))),
                IrExpr::Ext(x) => {
                    if x.children().iter().any(|c| self.fill.expr_hits(c)) {
                        return None;
                    }
                    Some(e.clone())
                }
                _ => Some(e.clone()),
            }
        }
    }
    let mut sub = Sub { fill, counter, count: 0 };
    let out = sub.expr(expr)?;
    Some((out, sub.count))
}

// ---------- whole-program array discipline ----------

/// `a = setArray(a, [])` — the array's empty initialisation. After a
/// successful fusion the array is dead, so this store is a dead store;
/// a frontend-emitted init must not veto the fusion (the shell path
/// usually has no explicit init statement).
fn empty_array_init_of(s: &IrStmt, arr: &str) -> bool {
    let IrStmt::Assign { targets, expr, .. } = s else {
        return false;
    };
    let Some(t) = targets.first() else { return false };
    if !t.indices.is_empty() || t.var != arr {
        return false;
    }
    match expr {
        IrExpr::Call { func, args } if func == "setArray" => {
            matches!(args.get(1), Some(IrExpr::Array(items)) if items.is_empty())
        }
        IrExpr::Array(items) => items.is_empty(),
        _ => false,
    }
}

/// True = the array is used outside the fill store / forwarded consumer
/// reads (veto). Allowed: the fill's store target (l1 body), and plain
/// `A[<i>]` arith reads inside l2's body (re-verified here, independent
/// of the substitution above — defense in depth), plus the array's empty
/// initialisation.
fn array_used_elsewhere(
    stmts: &[IrStmt],
    fill: &Fill,
    l1: usize,
    l2: usize,
    counter: &str,
) -> bool {
    for (k, s) in stmts.iter().enumerate() {
        if k == l1 {
            // Fill: headers fully scanned; body TARGETS skipped (they
            // ARE the stores — fill_shape pinned one distinct array per
            // store with a plain-`i` index); body RHS + index keys
            // scanned (a `$a` read inside F vetoes).
            let IrStmt::ForInit { init, cond, step, body } = s else {
                return true;
            };
            if fill.stmts_hit(init) || fill.expr_hits(cond) || fill.stmts_hit(step) {
                return true;
            }
            for b in body {
                let IrStmt::Assign { targets, expr, .. } = b else {
                    return true;
                };
                if targets
                    .iter()
                    .any(|t| t.indices.iter().any(|kk| fill.expr_hits(kk)))
                {
                    return true;
                }
                if fill.expr_hits(expr) {
                    return true;
                }
            }
        } else if k == l2 {
            // Consumer: headers + targets scanned; body exprs re-verified
            // (only plain `A[<i>]` arith reads of a filled array allowed).
            let IrStmt::ForInit { init, cond, step, body } = s else {
                return true;
            };
            if fill.stmts_hit(init) || fill.expr_hits(cond) || fill.stmts_hit(step) {
                return true;
            }
            for b in body {
                match b {
                    IrStmt::Assign { targets, expr, .. } => {
                        if targets.iter().any(|t| fill_any_target_hits(t, fill)) {
                            return true;
                        }
                        if expr_reads_violate(expr, fill, counter) {
                            return true;
                        }
                    }
                    _ => {
                        if fill.stmt_hits(b) {
                            return true;
                        }
                    }
                }
            }
        } else if fill.stmt_hits(s) {
            // Tolerated: an empty `a = setArray(a, [])` init of a filled
            // array (dead after fusion). Any other mention vetoes.
            let empty_ok = fill.arrays().any(|a| empty_array_init_of(s, a))
                && !fill
                    .arrays()
                    .any(|a| stmt_has_arr(s, a) && !empty_array_init_of(s, a));
            if !empty_ok {
                return true;
            }
        }
    }
    false
}

/// Does an assign target mention any filled array (base name or index)?
fn fill_any_target_hits(t: &AssignTarget, fill: &Fill) -> bool {
    if fill.name_hits(&t.var) {
        return true;
    }
    if let Some(open) = t.var.rfind('[') {
        if t.var.ends_with(']') && fill.name_hits(&t.var[..open]) {
            return true;
        }
    }
    t.indices.iter().any(|k| fill.expr_hits(k))
}

/// Body-expr re-verification: every array mention is a plain `A[<i>]`
/// arith read (anything else → true = veto). Nested statement bodies
/// (Arrow/Lambda) are strict (any mention vetoes).
fn expr_reads_violate(e: &IrExpr, fill: &Fill, counter: &str) -> bool {
    // Returns true on VIOLATION (veto).
    match e {
        IrExpr::Arith(a) => arith_reads_violate(a, fill, counter),
        IrExpr::Index { var, .. } if fill.name_hits(var) => true,
        IrExpr::Var(v, _) | IrExpr::Ident(v) if fill.name_hits(v) => true,
        IrExpr::Str(s, _) => fill
            .arrays()
            .any(|a| s == a || s.contains(&format!("{a}["))),
        IrExpr::RawExpr(_) => true,
        IrExpr::Lambda { body, .. } => fill.stmts_hit(body),
        IrExpr::Arrow(b) => fill.stmts_hit(b),
        IrExpr::Ext(x) => x.children().iter().any(|c| fill.expr_hits(c)),
        _ => {
            expr_kids(e).iter().any(|k| expr_reads_violate(k, fill, counter))
                || expr_stmts(e).iter().any(|k| fill.stmt_hits(k))
        }
    }
}

fn arith_reads_violate(a: &ArithAst, fill: &Fill, counter: &str) -> bool {
    // Returns true on VIOLATION (veto).
    match a {
        ArithAst::Index { var, key } if fill.rhs(var).is_some() => {
            // Allowed ONLY as a plain-counter read of clean key.
            arith_affine(key, counter) != Some((1, 0)) || fill.expr_arith_hits(key)
        }
        ArithAst::Var(v) | ArithAst::Ident(v) => fill.name_hits(v),
        ArithAst::Assign { var, .. } if fill.name_hits(var) => true,
        ArithAst::IncDec { var, .. } if fill.name_hits(var) => true,
        _ => arith_kids(a).iter().any(|k| arith_reads_violate(k, fill, counter)),
    }
}

// ---------- strict mention scans (any mention vetoes) ----------

/// Shared scope-audit scanners (consumed cross-crate by CUDA
/// candidacy's nest-scope checks). Visibility-only; logic untouched.
pub fn stmts_have_arr(stmts: &[IrStmt], arr: &str) -> bool {
    stmts.iter().any(|s| stmt_has_arr(s, arr))
}

/// Any mention of the array (reads, writes, decls, string text,
/// reflective uses, raw bridge text).
/// Shared scope-audit scanners (consumed cross-crate by CUDA
/// candidacy's nest-scope checks). Visibility-only; logic untouched.
pub fn stmt_has_arr(s: &IrStmt, arr: &str) -> bool {
    let own = match s {
        IrStmt::Assign { targets, .. } => targets.iter().any(|t| target_has_arr(t, arr)),
        IrStmt::Declare { vars, .. } => vars.iter().any(|d| d.name == arr),
        IrStmt::DeclareArray { var, .. } => var == arr,
        IrStmt::RawText(t) => t == arr || t.contains(&format!("{arr}[")),
        _ => stmt_binds(s).iter().any(|b| *b == arr),
    };
    own || stmt_kids(s).iter().any(|k| stmt_has_arr(k, arr))
        || stmt_exprs(s).iter().any(|e| expr_has_arr(e, arr))
}

fn target_has_arr(t: &AssignTarget, arr: &str) -> bool {
    if t.var == arr {
        return true;
    }
    if let Some(open) = t.var.rfind('[') {
        if t.var.ends_with(']') && t.var[..open] == *arr {
            return true;
        }
    }
    t.indices.iter().any(|k| expr_has_arr(k, arr))
}

/// Shared scope-audit scanners (consumed cross-crate by CUDA
/// candidacy's nest-scope checks). Visibility-only; logic untouched.
pub fn expr_has_arr(e: &IrExpr, arr: &str) -> bool {
    let own = match e {
        IrExpr::Arith(a) => arith_has_arr(a, arr),
        IrExpr::Var(v, _) | IrExpr::Ident(v) => v == arr || v.starts_with(&format!("{arr}[")),
        IrExpr::Index { var, .. } => var == arr,
        IrExpr::Str(s, _) => s == arr || s.contains(&format!("{arr}[")),
        IrExpr::RawExpr(_) => true,
        _ => false,
    };
    own || expr_kids(e).iter().any(|k| expr_has_arr(k, arr))
        || expr_stmts(e).iter().any(|k| stmt_has_arr(k, arr))
}

fn arith_has_arr(a: &ArithAst, arr: &str) -> bool {
    let own = match a {
        ArithAst::Index { var, .. } => var == arr,
        ArithAst::Var(v) | ArithAst::Ident(v) => v == arr || v.starts_with(&format!("{arr}[")),
        ArithAst::Assign { var, .. } => var == arr,
        ArithAst::IncDec { var, .. } => var == arr,
        _ => false,
    };
    own || arith_kids(a).iter().any(|k| arith_has_arr(k, arr))
}

fn expr_has_closure_or_ext(e: &IrExpr) -> bool {
    matches!(e, IrExpr::Lambda { .. } | IrExpr::Arrow(_) | IrExpr::Ext(_))
        || expr_kids(e).iter().any(|k| expr_has_closure_or_ext(k))
        || expr_stmts(e).iter().any(|k| stmt_has_closure_or_ext(k))
}

fn stmt_has_closure_or_ext(s: &IrStmt) -> bool {
    stmt_kids(s).iter().any(|k| stmt_has_closure_or_ext(k))
        || stmt_exprs(s).iter().any(|e| expr_has_closure_or_ext(e))
}

fn collect_arith_effects_expr(e: &IrExpr, out: &mut BTreeSet<String>) {
    match e {
        IrExpr::Arith(a) => collect_arith_effects(a, out),
        _ => {}
    }
    for k in expr_kids(e) {
        collect_arith_effects_expr(k, out);
    }
    for st in expr_stmts(e) {
        collect_arith_effects_stmt(st, out);
    }
}

fn collect_arith_effects_stmt(s: &IrStmt, out: &mut BTreeSet<String>) {
    // Nested-closure Assigns don't execute during header eval, but the
    // closure VALUE vanishes with the header — closures are already
    // vetoed above; any nested Assign here vetoes too (unreachable in
    // practice, airtight in principle: the empty name never equals the
    // counter, so the caller's emptiness check fails).
    if matches!(s, IrStmt::Assign { .. }) {
        out.insert(String::new());
    }
    for k in stmt_kids(s) {
        collect_arith_effects_stmt(k, out);
    }
    for e in stmt_exprs(s) {
        collect_arith_effects_expr(e, out);
    }
}

fn collect_arith_effects(a: &ArithAst, out: &mut BTreeSet<String>) {
    match a {
        ArithAst::Assign { var, .. } | ArithAst::IncDec { var, .. } => {
            out.insert(var.clone());
        }
        _ => {}
    }
    for k in arith_kids(a) {
        collect_arith_effects(k, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StrStyle;

    fn scalar(var: &str) -> AssignTarget {
        AssignTarget { var: var.to_string(), sigil: None, indices: vec![] }
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

    /// `for ((i=0; i<n; i++))` header triple (mirrors the shell lowering:
    /// let-cond text, IncDec step). `callee` is `exec` pre-refinement /
    /// `builtin` post-refinement — both must fuse.
    fn header() -> (Vec<IrStmt>, IrExpr, Vec<IrStmt>) {
        header_with("builtin")
    }

    fn header_with(callee: &str) -> (Vec<IrStmt>, IrExpr, Vec<IrStmt>) {
        let init = vec![assign("i", IrExpr::Int(0))];
        let cond = IrExpr::Call {
            func: callee.to_string(),
            args: vec![
                str_lit("let"),
                IrExpr::Array(vec![str_lit("i<n")]),
            ],
        };
        let step = vec![assign(
            "i",
            arith(ArithAst::IncDec { var: "i".to_string(), delta: 1, prefix: false }),
        )];
        (init, cond, step)
    }

    fn for_init(body: Vec<IrStmt>) -> IrStmt {
        let (init, cond, step) = header();
        IrStmt::ForInit { init, cond, step, body }
    }

    fn for_init_with(body: Vec<IrStmt>, callee: &str) -> IrStmt {
        let (init, cond, step) = header_with(callee);
        IrStmt::ForInit { init, cond, step, body }
    }

    /// Fill `a[$i] = i*i`.
    fn fill() -> IrStmt {
        for_init(vec![IrStmt::Assign {
            targets: vec![scalar("a[$i]")],
            expr: arith(ArithAst::Bin {
                op: "*".to_string(),
                lhs: Box::new(v("i")),
                rhs: Box::new(v("i")),
            }),
            asm: None,
        }])
    }

    /// Consumer `s = ((s + a[i]) & mask)`.
    fn consume(mask: i64) -> IrStmt {
        for_init(vec![assign(
            "s",
            arith(ArithAst::Bin {
                op: "&".to_string(),
                lhs: Box::new(ArithAst::Bin {
                    op: "+".to_string(),
                    lhs: Box::new(v("s")),
                    rhs: Box::new(ArithAst::Index {
                        var: "a".to_string(),
                        key: Box::new(v("i")),
                    }),
                }),
                rhs: Box::new(ArithAst::Num(mask)),
            }),
        )])
    }

    fn squares_map_prog() -> Vec<IrStmt> {
        vec![fill(), assign("s", str_lit("0")), consume(4294967295)]
    }

    /// Fill with an `exec`-form cond (pre-refinement pipeline shape).
    fn fill_exec() -> IrStmt {
        for_init_with(
            vec![IrStmt::Assign {
                targets: vec![scalar("a[$i]")],
                expr: arith(ArithAst::Bin {
                    op: "*".to_string(),
                    lhs: Box::new(v("i")),
                    rhs: Box::new(v("i")),
                }),
                asm: None,
            }],
            "exec",
        )
    }

    fn consume_exec(mask: i64) -> IrStmt {
        for_init_with(
            vec![assign(
                "s",
                arith(ArithAst::Bin {
                    op: "&".to_string(),
                    lhs: Box::new(ArithAst::Bin {
                        op: "+".to_string(),
                        lhs: Box::new(v("s")),
                        rhs: Box::new(ArithAst::Index {
                            var: "a".to_string(),
                            key: Box::new(v("i")),
                        }),
                    }),
                    rhs: Box::new(ArithAst::Num(mask)),
                }),
            )],
            "exec",
        )
    }

    #[test]
    fn squares_map_fuses() {
        let mut prog = squares_map_prog();
        assert!(transform(&mut prog));
        // Fill gone, gap kept, single fused loop.
        assert_eq!(prog.len(), 2);
        assert!(matches!(&prog[0], IrStmt::Assign { .. }));
        let IrStmt::ForInit { body, .. } = &prog[1] else {
            panic!("expected fused loop");
        };
        assert_eq!(body.len(), 1);
        // No array mention survives anywhere.
        assert!(!stmts_have_arr(&prog, "a"));
        // The forwarded body computes s from i*i (a Mul present, no Index).
        let IrStmt::Assign { expr, .. } = &body[0] else {
            panic!("expected assign");
        };
        let IrExpr::Arith(a) = expr else {
            panic!("expected arith");
        };
        assert!(!arith_has_arr(a, "a"));
        fn has_mul(x: &ArithAst) -> bool {
            matches!(x, ArithAst::Bin { op, .. } if op == "*")
                || arith_kids(x).iter().any(|k| has_mul(k))
        }
        assert!(has_mul(a));
    }

    #[test]
    fn exec_cond_form_fuses() {
        // Pre-refinement `exec("let",…)` conds fuse exactly like the
        // `builtin` form (same text audit).
        let mut prog = vec![fill_exec(), assign("s", str_lit("0")), consume_exec(4294967295)];
        assert!(transform(&mut prog));
        assert_eq!(prog.len(), 2);
        assert!(!stmts_have_arr(&prog, "a"));
    }

    /// Multi-store fill: `a[$i]=i*i; b[$i]=i+1` consumed as
    /// `(s + a[i]*b[i]) & mask`.
    fn twoarr_prog() -> Vec<IrStmt> {
        let fill = for_init(vec![
            IrStmt::Assign {
                targets: vec![scalar("a[$i]")],
                expr: arith(ArithAst::Bin {
                    op: "*".to_string(),
                    lhs: Box::new(v("i")),
                    rhs: Box::new(v("i")),
                }),
                asm: None,
            },
            IrStmt::Assign {
                targets: vec![scalar("b[$i]")],
                expr: arith(ArithAst::Bin {
                    op: "+".to_string(),
                    lhs: Box::new(v("i")),
                    rhs: Box::new(ArithAst::Num(1)),
                }),
                asm: None,
            },
        ]);
        let cons = for_init(vec![assign(
            "s",
            arith(ArithAst::Bin {
                op: "&".to_string(),
                lhs: Box::new(ArithAst::Bin {
                    op: "+".to_string(),
                    lhs: Box::new(v("s")),
                    rhs: Box::new(ArithAst::Bin {
                        op: "*".to_string(),
                        lhs: Box::new(ArithAst::Index {
                            var: "a".to_string(),
                            key: Box::new(v("i")),
                        }),
                        rhs: Box::new(ArithAst::Index {
                            var: "b".to_string(),
                            key: Box::new(v("i")),
                        }),
                    }),
                }),
                rhs: Box::new(ArithAst::Num(4294967295)),
            }),
        )]);
        vec![fill, assign("s", str_lit("0")), cons]
    }

    #[test]
    fn multi_store_fill_fuses() {
        let mut prog = twoarr_prog();
        assert!(transform(&mut prog));
        assert_eq!(prog.len(), 2, "fill deleted, gap + fused loop left");
        assert!(!stmts_have_arr(&prog, "a") && !stmts_have_arr(&prog, "b"));
        let IrStmt::ForInit { body, .. } = &prog[1] else {
            panic!("expected fused loop");
        };
        let IrStmt::Assign { expr, .. } = &body[0] else {
            panic!("expected assign");
        };
        assert!(expr_has_arr(expr, "a") == false && expr_has_arr(expr, "b") == false);
        // Both RHS shapes survive.
        let IrExpr::Arith(a) = expr else {
            panic!("arith");
        };
        fn has(x: &ArithAst, want: i64) -> bool {
            matches!(x, ArithAst::Num(n) if *n == want) || arith_kids(x).iter().any(|k| has(k, want))
        }
        assert!(has(a, 1), "b's `i+1` survived: {a:?}");
    }

    #[test]
    fn duplicate_store_array_vetoes() {
        // Same array written twice in the fill — forwarding target is
        // ambiguous.
        let mut prog = twoarr_prog();
        let IrStmt::ForInit { body, .. } = &mut prog[0] else {
            panic!("shape");
        };
        let IrStmt::Assign { targets, .. } = &mut body[1] else {
            panic!("shape");
        };
        targets[0] = scalar("a[$i]");
        assert!(!transform(&mut prog));
    }

    #[test]
    fn extra_mention_of_second_array_vetoes() {
        let mut prog = twoarr_prog();
        prog.push(IrStmt::Output {
            value: IrExpr::Index {
                var: "b".to_string(),
                key: Box::new(IrExpr::Int(0)),
            },
            newline: true,
            target: None,
        });
        assert!(!transform(&mut prog));
    }

    #[test]
    fn partially_read_multi_store_fuses() {
        // Only `a` is read by the consumer — `b` becomes dead, which is
        // still sound (its store is unobservable). Forwarded count is 1.
        let mut prog = twoarr_prog();
        let IrStmt::ForInit { body, .. } = &mut prog[2] else {
            panic!("shape");
        };
        let IrStmt::Assign { expr, .. } = &mut body[0] else {
            panic!("shape");
        };
        let IrExpr::Arith(a) = expr else {
            panic!("shape");
        };
        // Replace `a[i]*b[i]` with plain `a[i]`.
        let ArithAst::Bin { lhs, .. } = a.as_ref() else {
            panic!("shape");
        };
        let ArithAst::Bin { lhs: plus, .. } = lhs.as_ref() else {
            panic!("shape");
        };
        *a = Box::new(ArithAst::Bin {
            op: "&".to_string(),
            lhs: Box::new(ArithAst::Bin {
                op: "+".to_string(),
                lhs: plus.clone(),
                rhs: Box::new(ArithAst::Index {
                    var: "a".to_string(),
                    key: Box::new(v("i")),
                }),
            }),
            rhs: Box::new(ArithAst::Num(4294967295)),
        });
        assert!(transform(&mut prog));
        assert!(!stmts_have_arr(&prog, "a"));
        assert!(!stmts_have_arr(&prog, "b"));
    }

    #[test]
    fn transform_is_idempotent() {
        let mut prog = squares_map_prog();
        assert!(transform(&mut prog));
        assert!(!transform(&mut prog));
    }

    #[test]
    fn stray_read_vetoes() {
        let mut prog = squares_map_prog();
        prog.push(IrStmt::Output {
            value: IrExpr::Index {
                var: "a".to_string(),
                key: Box::new(IrExpr::Int(0)),
            },
            newline: true,
            target: None,
        });
        assert!(!transform(&mut prog));
    }

    #[test]
    fn status_gap_vetoes() {
        let mut prog = vec![
            fill(),
            assign("s", IrExpr::Var("?".to_string(), None)),
            consume(4294967295),
        ];
        assert!(!transform(&mut prog));
    }

    #[test]
    fn impure_fill_vetoes() {
        // `a[i] = a[i] + 1` (update-in-place reads the array).
        let mut prog = vec![
            for_init(vec![IrStmt::Assign {
                targets: vec![scalar("a[$i]")],
                expr: arith(ArithAst::Bin {
                    op: "+".to_string(),
                    lhs: Box::new(ArithAst::Index {
                        var: "a".to_string(),
                        key: Box::new(v("i")),
                    }),
                    rhs: Box::new(ArithAst::Num(1)),
                }),
                asm: None,
            }]),
            assign("s", str_lit("0")),
            consume(4294967295),
        ];
        assert!(!transform(&mut prog));
    }

    #[test]
    fn header_mismatch_vetoes() {
        // Consumer steps by 2 (different space).
        let (init, cond, _) = header();
        let step2 = vec![assign(
            "i",
            arith(ArithAst::Bin {
                op: "+".to_string(),
                lhs: Box::new(v("i")),
                rhs: Box::new(ArithAst::Num(2)),
            }),
        )];
        let mut prog = vec![
            fill(),
            assign("s", str_lit("0")),
            IrStmt::ForInit {
                init,
                cond,
                step: step2,
                body: vec![assign("s", str_lit("1"))],
            },
        ];
        assert!(!transform(&mut prog));
    }

    #[test]
    fn bound_write_vetoes() {
        // Consumer writes the bound var mid-loop (self-modifying trips).
        let mut c = consume(4294967295);
        let IrStmt::ForInit { body, .. } = &mut c else {
            panic!("shape");
        };
        body.push(assign("n", IrExpr::Int(5)));
        let mut prog = vec![fill(), assign("s", str_lit("0")), c];
        assert!(!transform(&mut prog));
    }

    #[test]
    fn stencil_read_vetoes() {
        // Consumer reads a[i+1] (not the filled slot).
        let mut c = consume(4294967295);
        let IrStmt::ForInit { body, .. } = &mut c else {
            panic!("shape");
        };
        let IrStmt::Assign { expr, .. } = &mut body[0] else {
            panic!("shape");
        };
        let IrExpr::Arith(a) = expr else {
            panic!("shape");
        };
        // Replace the Index key i with i+1.
        fn bump(x: ArithAst) -> ArithAst {
            match x {
                ArithAst::Index { var, .. } => ArithAst::Index {
                    var,
                    key: Box::new(ArithAst::Bin {
                        op: "+".to_string(),
                        lhs: Box::new(v("i")),
                        rhs: Box::new(ArithAst::Num(1)),
                    }),
                },
                ArithAst::Bin { op, lhs, rhs } => ArithAst::Bin {
                    op,
                    lhs: Box::new(bump(*lhs)),
                    rhs: Box::new(bump(*rhs)),
                },
                other => other,
            }
        }
        *a = Box::new(bump((**a).clone()));
        let mut prog = vec![fill(), assign("s", str_lit("0")), c];
        assert!(!transform(&mut prog));
    }

    #[test]
    fn second_consumer_vetoes() {
        // Two loops read the fill — deleting it would starve the second.
        let mut prog = squares_map_prog();
        prog.push(consume(255));
        assert!(!transform(&mut prog));
    }

    #[test]
    fn no_consumer_no_fire() {
        let mut prog = vec![fill(), assign("s", str_lit("0"))];
        assert!(!transform(&mut prog));
    }

    #[test]
    fn adjacent_without_gap_fuses() {
        let mut prog = vec![fill(), consume(4294967295)];
        assert!(transform(&mut prog));
        assert_eq!(prog.len(), 1);
    }

    #[test]
    fn affine_text_shapes() {
        assert_eq!(affine_of_text("i", "i"), Some((1, 0)));
        assert_eq!(affine_of_text("$i", "i"), Some((1, 0)));
        assert_eq!(affine_of_text("i+0", "i"), Some((1, 0)));
        assert_eq!(affine_of_text("2*i", "i"), Some((2, 0)));
        assert_eq!(affine_of_text("i", "j"), None);
        assert_eq!(affine_of_text("5", "i"), Some((0, 5)));
    }

    // ---- frontend-emitted (structured `Arith`) shapes ----
    // py-sh-go / c-sh-go A1: structured `Arith` cond, `Cast(Int64, …)`
    // exactness markers, flattened `a[i]` target, composed floor-mod
    // value, and an explicit `a = setArray(a, [])` init statement. The
    // shared `counted-arith-forinit` + `append-to-store` passes produce
    // the loop shapes; fusion must then forward across the Casts.
    fn fe_cast(a: ArithAst) -> ArithAst {
        ArithAst::Cast { ty: crate::ir::IrType::Int64, arg: Box::new(a) }
    }
    fn fe_header() -> (Vec<IrStmt>, IrExpr, Vec<IrStmt>) {
        let init = vec![assign("i", arith(ArithAst::Num(0)))];
        let cond = arith(ArithAst::Bin {
            op: "<".to_string(),
            lhs: Box::new(v("i")),
            rhs: Box::new(v("N")),
        });
        let step = vec![assign(
            "i",
            arith(ArithAst::IncDec { var: "i".to_string(), delta: 1, prefix: false }),
        )];
        (init, cond, step)
    }
    fn fe_fill() -> IrStmt {
        let (init, cond, step) = fe_header();
        IrStmt::ForInit {
            init,
            cond,
            step,
            body: vec![IrStmt::Assign {
                targets: vec![scalar("a[i]")],
                expr: arith(ArithAst::Bin {
                    op: "*".to_string(),
                    lhs: Box::new(fe_cast(v("i"))),
                    rhs: Box::new(fe_cast(v("i"))),
                }),
                asm: None,
            }],
        }
    }
    fn fe_consume() -> IrStmt {
        let (init, cond, step) = fe_header();
        // ((s + a[Cast(i)]) % M + M) % M
        let m = || fe_cast(ArithAst::Num(4294967296));
        let inner = ArithAst::Bin {
            op: "+".to_string(),
            lhs: Box::new(fe_cast(v("s"))),
            rhs: Box::new(ArithAst::Index {
                var: "a".to_string(),
                key: Box::new(fe_cast(v("i"))),
            }),
        };
        let value = ArithAst::Bin {
            op: "%".to_string(),
            lhs: Box::new(ArithAst::Bin {
                op: "+".to_string(),
                lhs: Box::new(ArithAst::Bin {
                    op: "%".to_string(),
                    lhs: Box::new(inner),
                    rhs: Box::new(m()),
                }),
                rhs: Box::new(m()),
            }),
            rhs: Box::new(m()),
        };
        IrStmt::ForInit {
            init,
            cond,
            step,
            body: vec![assign("s", arith(value))],
        }
    }
    fn fe_empty_init() -> IrStmt {
        assign(
            "a",
            IrExpr::Call {
                func: "setArray".to_string(),
                args: vec![
                    IrExpr::Str("a".to_string(), StrStyle::DoubleQuoted),
                    IrExpr::Array(vec![]),
                ],
            },
        )
    }

    #[test]
    fn frontend_cast_shapes_fuse() {
        let mut prog = vec![fe_empty_init(), fe_fill(), assign("s", str_lit("0")), fe_consume()];
        assert!(transform(&mut prog), "frontend shapes must fuse");
        // The two loops collapsed into one; that loop's body no longer
        // mentions the array (the dead empty init may remain).
        let loops: Vec<&IrStmt> = prog
            .iter()
            .filter(|s| matches!(s, IrStmt::ForInit { .. }))
            .collect();
        assert_eq!(loops.len(), 1, "one loop left: {prog:?}");
        let IrStmt::ForInit { body, .. } = loops[0] else { panic!() };
        assert!(
            !body.iter().any(|s| stmt_has_arr(s, "a")),
            "fused body still reads the array: {body:?}"
        );
    }

    #[test]
    fn frontend_literal_bound_still_vetoes_gap_write() {
        // `i < 5` has no dynamic bound name: any gap write is unprovable.
        let mut prog = vec![fe_empty_init(), fe_fill(), assign("s", str_lit("0")), fe_consume()];
        let IrStmt::ForInit { cond, .. } = &mut prog[1] else { panic!() };
        *cond = arith(ArithAst::Bin {
            op: "<".to_string(),
            lhs: Box::new(v("i")),
            rhs: Box::new(ArithAst::Num(5)),
        });
        assert!(!transform(&mut prog), "literal bound + gap write must veto");
    }
}
