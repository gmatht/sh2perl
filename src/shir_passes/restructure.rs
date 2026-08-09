//! restructure_goto — the shared goto-elimination pass.
//!
//! Frontends emit `IrStmt::Label`/`IrStmt::Goto` for source languages
//! with goto-like jumps (c-sh-go for C's `goto`; future frontends for
//! labeled-break families). Shell has no goto, so this pass rewrites the
//! jump edges into structured flow — `DoWhile`/`While` loops, `break`,
//! inverted `If`, and flag-variable multi-level exits — before any
//! renderer sees the IR.
//!
//! Placement: registered in the canonical pipeline AND run at A1 ingress
//! (cli `--shir-in-estree` / `--shir-in-perl`), so every backend and
//! every frontend shares ONE implementation (no per-language copies).
//!
//! Scope (refuse > guess): the common real-world C goto shapes are
//! restructured:
//!   - backward guarded goto `L: body; if (c) goto L;`   -> `DoWhile(body, c)`
//!   - backward bare goto / mid-test loops               -> `While(true)` +
//!     conditional `break` (successor-label exits fold in)
//!   - forward guarded goto `if (c) goto L; skip; L:`    -> inverted `If`
//!     (`If { cond: c, then: [], else_: skip }`)
//!   - goto out of nested loops                          -> flag var + `break`
//!     (`__g<label>_<n>`), each outer loop body ends with `if (flag) break`
//! Anything else is left in place and the renderers refuse loudly
//! (their `Label`/`Goto` arms emit the Unsupported marker), keeping the
//! failure-driven loops in charge.
//!
//! Pinned by frontends/c-sh-go/testdata/t27_goto.c (backward -> DoWhile),
//! t30_goto_nested.c (multi-level exit -> flag), t31_goto_forward.c
//! (forward cleanup -> inverted If).

use crate::ir::*;
use crate::shir_passes::{PassContext, Transform};

pub struct RestructureGoto;

impl Transform for RestructureGoto {
    fn name(&self) -> &'static str {
        "restructure_goto"
    }
    fn run(&self, prog: &mut IrProgram, _ctx: &PassContext) {
        let mut n = 0usize;
        restructure_stmts(&mut prog.stmts, &mut n);
        for sub in &mut prog.subs {
            restructure_stmts(&mut sub.body, &mut n);
        }
    }
}

// ── helpers ─────────────────────────────────────────────────────────

fn st(s: &str) -> IrExpr {
    IrExpr::Str(s.to_string(), StrStyle::DoubleQuoted)
}
fn var(name: &str) -> IrExpr {
    IrExpr::Var(name.to_string(), None)
}
fn assign_stmt(name: &str, value: IrExpr) -> IrStmt {
    IrStmt::Assign {
        targets: vec![AssignTarget {
            var: name.to_string(),
            sigil: None,
            indices: vec![],
        }],
        expr: value,
    }
}
fn break_stmt() -> IrStmt {
    IrStmt::Expr(IrExpr::Call {
        func: "break".to_string(),
        args: vec![],
    })
}
/// The canonical `while true` condition — exec the `true` builtin, the
/// same shape the shell parser produces for an empty while condition.
fn truthy() -> IrExpr {
    IrExpr::Call {
        func: "exec".to_string(),
        args: vec![st("true")],
    }
}
/// Fresh flag variable name (unique across the whole program run).
fn fresh_flag(label: &str, n: &mut usize) -> String {
    let name = format!("__g{label}_{n}");
    *n += 1;
    name
}

// ── region driver ───────────────────────────────────────────────────

/// Restructure a statement list. Handles every goto whose target label
/// is in this list's top level (outermost-first — the list's labels are
/// the targets for any depth of nested goto), then recurses inward.
fn restructure_stmts(stmts: &mut Vec<IrStmt>, n: &mut usize) {
    while restructure_pass(stmts, n) {}
    for s in stmts.iter_mut() {
        restructure_children(s, n);
    }
}

fn restructure_children(s: &mut IrStmt, n: &mut usize) {
    match s {
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            restructure_stmts(then, n);
            for (_, b) in elsifs.iter_mut() {
                restructure_stmts(b, n);
            }
            restructure_stmts(else_, n);
        }
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. }
        | IrStmt::Function { body, .. } => restructure_stmts(body, n),
        IrStmt::Case { clauses, .. } => {
            for c in clauses.iter_mut() {
                restructure_stmts(&mut c.body, n);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages.iter_mut() {
                restructure_stmts(stage, n);
            }
        }
        _ => {}
    }
}

// ── one restructuring pass ──────────────────────────────────────────

/// One pass over a statement list: find the first goto (in tree order)
/// whose target label is in THIS list's top level, and rewrite it.
/// Returns false when no such goto exists (nothing left to do here).
fn restructure_pass(stmts: &mut Vec<IrStmt>, n: &mut usize) -> bool {
    let labels: Vec<(String, usize)> = stmts
        .iter()
        .enumerate()
        .filter_map(|(i, s)| match s {
            IrStmt::Label(name) => Some((name.clone(), i)),
            _ => None,
        })
        .collect();
    if labels.is_empty() {
        return false;
    }
    for i in 0..stmts.len() {
        // bare top-level goto
        if let IrStmt::Goto(name) = &stmts[i] {
            if let Some((_, lpos)) = labels.iter().find(|(l, _)| l == name) {
                return handle_bare_goto(stmts, i, *lpos, n);
            }
        }
        // guarded `if (c) goto L;` at top level
        if let IrStmt::If { cond, then, .. } = &stmts[i] {
            if then.len() == 1 {
                if let IrStmt::Goto(name) = &then[0] {
                    if let Some((_, lpos)) = labels.iter().find(|(l, _)| l == name) {
                        return handle_guarded_goto(stmts, i, cond.clone(), *lpos, n);
                    }
                }
            }
        }
        // nested goto targeting this list's label
        if let Some(hit) = find_nested(&stmts[i], i, &labels) {
            return handle_nested(stmts, hit, n);
        }
    }
    false
}

/// A bare top-level `Goto(L)`. Backward -> infinite loop; forward -> the
/// skipped region is unreachable and dropped.
fn handle_bare_goto(stmts: &mut Vec<IrStmt>, gpos: usize, lpos: usize, _n: &mut usize) -> bool {
    if lpos < gpos {
        // backward: the span between label and goto is the loop body
        let successor_label = match stmts.get(gpos + 1) {
            Some(IrStmt::Label(name)) => Some(name.clone()),
            _ => None,
        };
        let mut span: Vec<IrStmt> = stmts.drain(lpos + 1..gpos).collect();
        let converted = convert_successor_exits(&mut span, successor_label.as_deref());
        let while_stmt = IrStmt::While {
            cond: truthy(),
            body: span,
        };
        stmts.splice(lpos..=gpos, [while_stmt]);
        if converted {
            stmts.remove(lpos + 1); // the successor label (now the exit point)
        }
        true
    } else {
        // forward: stmts between the goto and the label are unreachable
        stmts.splice(gpos..=lpos, []);
        true
    }
}

/// A top-level `If { cond, then: [Goto(L)] }`. Backward -> DoWhile (or
/// While(true) for mid-test loops); forward -> inverted If (skip the
/// region via the else branch).
fn handle_guarded_goto(
    stmts: &mut Vec<IrStmt>,
    i: usize,
    cond: IrExpr,
    lpos: usize,
    _n: &mut usize,
) -> bool {
    if lpos < i {
        // backward: `L: body; if (c) goto L;`
        let successor = i + 1;
        let successor_label = match stmts.get(successor) {
            Some(IrStmt::Label(name)) => Some(name.clone()),
            _ => None,
        };
        let mut span: Vec<IrStmt> = stmts.drain(lpos + 1..i).collect();
        // backward guarded goto: `L: body; if (c) goto L;` — a post-test
        // loop. Expressed as `while true { body; if (c) {} else break }`
        // (portable: DoWhile is Perl-only, the ESTree renderer refuses
        // it). Successor-label exits inside the span fold to `break`.
        let converted = convert_successor_exits(&mut span, successor_label.as_deref());
        span.push(IrStmt::If {
            cond,
            then: vec![],
            elsifs: vec![],
            else_: vec![break_stmt()],
        });
        let replaced = IrStmt::While {
            cond: truthy(),
            body: span,
        };
        stmts.splice(lpos..=lpos + 1, [replaced]);
        // after the splice the successor label (if any) sits at lpos+1
        if !matches!(&stmts.get(lpos + 1), Some(IrStmt::Label(_))) {
            // (no successor label to drop — nothing to do)
        } else {
            // drop it only if we converted its exits; otherwise the label
            // still belongs to an enclosing region. Since the mid-test
            // conversion already happened inside `replaced`, drop it.
            stmts.remove(lpos + 1);
        }
        true
    } else {
        // forward: `if (c) goto L; skip; L:` -> `if (c) {} else { skip }`
        let skipped: Vec<IrStmt> = stmts.drain(i + 1..lpos).collect();
        let new_if = IrStmt::If {
            cond,
            then: vec![],
            elsifs: vec![],
            else_: skipped,
        };
        // after the drain the label sits at i+1
        stmts.splice(i..=i + 1, [new_if]);
        true
    }
}

// ── nested gotos (multi-level exit via a flag) ───────────────────────

/// Descent-path step: which sub-list of the stmt at the given index to
/// enter next.
#[derive(Clone, Copy, PartialEq)]
enum Branch {
    Body,
    Then,
    Else,
    Elsif(usize),
    CaseClause(usize),
    Stage(usize),
}

struct Hit {
    /// path steps from the top-level list down to the goto's list
    path: Vec<(usize, Branch)>,
    /// indices into `path` of the enclosing LOOP bodies (innermost last)
    loop_steps: Vec<usize>,
    /// target label + its top-level position
    label: String,
    lpos: usize,
    /// index of the goto within the final list
    goto_index: usize,
}

/// Tree-order search for the first goto inside `root` (a top-level stmt)
/// whose target label is one of `labels`. `top_idx` is root's index in
/// the top-level list (path steps reference it).
fn find_nested(root: &IrStmt, top_idx: usize, labels: &[(String, usize)]) -> Option<Hit> {
    if let IrStmt::Goto(name) = root {
        if let Some((_, lpos)) = labels.iter().find(|(l, _)| l == name) {
            return Some(Hit {
                path: vec![],
                loop_steps: vec![],
                label: name.clone(),
                lpos: *lpos,
                goto_index: top_idx,
            });
        }
    }
    let mut out: Option<Hit> = None;
    let mut path: Vec<(usize, Branch)> = Vec::new();
    let mut loop_steps: Vec<usize> = Vec::new();
    descend_from(
        root,
        top_idx,
        0,
        labels,
        &mut path,
        &mut loop_steps,
        &mut out,
    );
    out
}

/// Record the path steps into `s`'s nested statement lists (tree order).
fn descend_from(
    s: &IrStmt,
    idx: usize,
    lc: usize,
    labels: &[(String, usize)],
    path: &mut Vec<(usize, Branch)>,
    loop_steps: &mut Vec<usize>,
    out: &mut Option<Hit>,
) {
    if out.is_some() {
        return;
    }
    match s {
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            push_and_walk(then, idx, Branch::Then, lc, labels, path, loop_steps, out);
            for (k, (_, b)) in elsifs.iter().enumerate() {
                push_and_walk(b, idx, Branch::Elsif(k), lc, labels, path, loop_steps, out);
            }
            push_and_walk(else_, idx, Branch::Else, lc, labels, path, loop_steps, out);
        }
        IrStmt::While { body, .. } | IrStmt::DoWhile { body, .. } | IrStmt::For { body, .. } => {
            push_and_walk(
                body,
                idx,
                Branch::Body,
                lc + 1,
                labels,
                path,
                loop_steps,
                out,
            );
        }
        IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. }
        | IrStmt::Function { body, .. } => {
            push_and_walk(body, idx, Branch::Body, lc, labels, path, loop_steps, out);
        }
        IrStmt::Case { clauses, .. } => {
            for (k, c) in clauses.iter().enumerate() {
                push_and_walk(
                    &c.body,
                    idx,
                    Branch::CaseClause(k),
                    lc,
                    labels,
                    path,
                    loop_steps,
                    out,
                );
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for (k, stg) in stages.iter().enumerate() {
                push_and_walk(
                    stg,
                    idx,
                    Branch::Stage(k),
                    lc,
                    labels,
                    path,
                    loop_steps,
                    out,
                );
            }
        }
        _ => {}
    }
}

/// Push a path step, then walk the sub-list (recording loop-body steps).
fn push_and_walk(
    list: &[IrStmt],
    idx: usize,
    br: Branch,
    lc: usize,
    labels: &[(String, usize)],
    path: &mut Vec<(usize, Branch)>,
    loop_steps: &mut Vec<usize>,
    out: &mut Option<Hit>,
) {
    if out.is_some() {
        return;
    }
    path.push((idx, br));
    if br == Branch::Body {
        loop_steps.push(path.len() - 1);
    }
    for (j, st) in list.iter().enumerate() {
        if let IrStmt::Goto(name) = st {
            if let Some((_, lpos)) = labels.iter().find(|(l, _)| l == name) {
                *out = Some(Hit {
                    path: path.clone(),
                    loop_steps: loop_steps.clone(),
                    label: name.clone(),
                    lpos: *lpos,
                    goto_index: j,
                });
                path.pop();
                return;
            }
        }
        descend_from(st, j, lc, labels, path, loop_steps, out);
        if out.is_some() {
            path.pop();
            return;
        }
    }
    if br == Branch::Body {
        loop_steps.pop();
    }
    path.pop();
}

/// A goto nested inside K loops, targeting a top-level label after the
/// loop chain: flag + break. The innermost loop is exited by the break;
/// every outer loop body gets `if (flag) break` appended.
fn handle_nested(stmts: &mut Vec<IrStmt>, hit: Hit, n: &mut usize) -> bool {
    if hit.lpos <= hit.path.first().map(|(i, _)| *i).unwrap_or(usize::MAX) {
        return false; // label before/inside the loop chain — refuse
    }
    let flag = fresh_flag(&hit.label, n);
    // 1. replace the goto with `flag = 1; break`
    let final_list = descend_mut(stmts, &hit.path);
    final_list[hit.goto_index] = assign_stmt(&flag, st("1"));
    final_list.insert(hit.goto_index + 1, break_stmt());
    // 2. guard every outer LOOP body (skip the innermost — its break is
    //    already in place). The parent at each `step` may be a Block
    //    (e.g. the c-sh-go wraps each for-lowered-to-while in a Block)
    //    rather than a loop; guarding that body emits an unguarded
    //    `if (flag) break` that escapes the program (no enclosing
    //    loop catches the throw). Only real loops (While/For/DoWhile)
    //    may carry the guard.
    for (k, step) in hit.loop_steps.iter().enumerate() {
        if k == hit.loop_steps.len() - 1 {
            continue;
        }
        if !is_loop_stmt_at(stmts, &hit.path, *step) {
            continue;
        }
        let body = descend_loop_body_mut(stmts, &hit.path, *step);
        body.push(IrStmt::If {
            cond: var(&flag),
            then: vec![break_stmt()],
            elsifs: vec![],
            else_: vec![],
        });
    }
    // 3. remove the (now fall-through) label
    stmts.remove(hit.lpos);
    true
}

/// True when the stmt at the given path step is a real loop (the only
/// stmt kind whose `body` is a loop body). A `Block` at the same
/// position would also be a `Body` step (Block's body is its child list)
/// but is NOT a loop — appending `if (flag) break` there escapes the
/// program. Used by `handle_nested` to skip non-loop steps.
fn is_loop_stmt_at(stmts: &Vec<IrStmt>, path: &[(usize, Branch)], step: usize) -> bool {
    let mut list = stmts;
    for (k, (idx, _br)) in path.iter().enumerate() {
        let s = &list[*idx];
        if k == step {
            return matches!(
                s,
                IrStmt::While { .. } | IrStmt::For { .. } | IrStmt::DoWhile { .. }
            );
        }
        list = match s {
            IrStmt::If { then, .. } => then,
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Redirect { inner: body, .. }
            | IrStmt::Function { body, .. } => body,
            IrStmt::Case { clauses, .. } => {
                // path's body step into a Case is the union of clause
                // bodies — but for our purposes, an `if (flag) break`
                // at the case level is still inside the loop (the
                // whileLoop catches it). A pure-Case step (no loop
                // above) would still be wrong, so we conservatively
                // say false for case steps.
                return false;
            }
            IrStmt::Pipeline { stages, .. } => return false,
            _ => return false,
        };
    }
    false
}

/// Re-descend to the statement list at the end of `path`.
fn descend_mut<'a>(stmts: &'a mut Vec<IrStmt>, path: &[(usize, Branch)]) -> &'a mut Vec<IrStmt> {
    let mut list = stmts;
    for (idx, br) in path {
        let s = &mut list[*idx];
        list = match br {
            Branch::Body => body_list_mut(s),
            Branch::Then => match s {
                IrStmt::If { then, .. } => then,
                _ => unreachable!("path corrupted"),
            },
            Branch::Else => match s {
                IrStmt::If { else_, .. } => else_,
                _ => unreachable!("path corrupted"),
            },
            Branch::Elsif(k) => match s {
                IrStmt::If { elsifs, .. } => &mut elsifs[*k].1,
                _ => unreachable!("path corrupted"),
            },
            Branch::CaseClause(k) => match s {
                IrStmt::Case { clauses, .. } => &mut clauses[*k].body,
                _ => unreachable!("path corrupted"),
            },
            Branch::Stage(k) => match s {
                IrStmt::Pipeline { stages, .. } => &mut stages[*k],
                _ => unreachable!("path corrupted"),
            },
        };
    }
    list
}

/// Re-descend to the loop body whose path step is `step_index`.
fn descend_loop_body_mut<'a>(
    stmts: &'a mut Vec<IrStmt>,
    path: &[(usize, Branch)],
    step_index: usize,
) -> &'a mut Vec<IrStmt> {
    let mut list = stmts;
    for (k, (idx, br)) in path.iter().enumerate() {
        let s = &mut list[*idx];
        if k == step_index {
            return body_list_mut(s);
        }
        list = match br {
            Branch::Body => body_list_mut(s),
            Branch::Then => match s {
                IrStmt::If { then, .. } => then,
                _ => unreachable!("path corrupted"),
            },
            Branch::Else => match s {
                IrStmt::If { else_, .. } => else_,
                _ => unreachable!("path corrupted"),
            },
            Branch::Elsif(k2) => match s {
                IrStmt::If { elsifs, .. } => &mut elsifs[*k2].1,
                _ => unreachable!("path corrupted"),
            },
            Branch::CaseClause(k2) => match s {
                IrStmt::Case { clauses, .. } => &mut clauses[*k2].body,
                _ => unreachable!("path corrupted"),
            },
            Branch::Stage(k2) => match s {
                IrStmt::Pipeline { stages, .. } => &mut stages[*k2],
                _ => unreachable!("path corrupted"),
            },
        };
    }
    unreachable!("loop step out of path")
}

fn body_list_mut(s: &mut IrStmt) -> &mut Vec<IrStmt> {
    match s {
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. }
        | IrStmt::Function { body, .. } => body,
        _ => unreachable!("path corrupted"),
    }
}

/// Convert `if (c) goto L` at the top level of a loop body into
/// `if (c) break` when L is the label immediately after the loop (the
/// loop's natural exit point). Returns true if anything converted.
fn convert_successor_exits(body: &mut [IrStmt], successor_label: Option<&str>) -> bool {
    let Some(label) = successor_label else {
        return false;
    };
    let mut changed = false;
    for s in body.iter_mut() {
        if let IrStmt::If { then, .. } = s {
            if then.len() == 1 {
                if let IrStmt::Goto(name) = &then[0] {
                    if name == label {
                        then[0] = break_stmt();
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}

// ── tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shir_json::shir_to_shir_json_raw;

    fn program(stmts: Vec<IrStmt>) -> IrProgram {
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
            var_bash_env: vec![],
        }
    }

    fn run(stmts: Vec<IrStmt>) -> Vec<IrStmt> {
        let mut prog = program(stmts);
        RestructureGoto.run(&mut prog, &PassContext::default());
        prog.stmts
    }

    fn label(name: &str) -> IrStmt {
        IrStmt::Label(name.to_string())
    }
    fn goto(name: &str) -> IrStmt {
        IrStmt::Goto(name.to_string())
    }
    fn output(s: &str) -> IrStmt {
        IrStmt::Output {
            value: st(s),
            newline: true,
            target: None,
        }
    }
    fn if_cond(then: Vec<IrStmt>) -> IrStmt {
        IrStmt::If {
            cond: IrExpr::Call {
                func: "test".to_string(),
                args: vec![st("cond")],
            },
            then,
            elsifs: vec![],
            else_: vec![],
        }
    }
    fn while_loop(cond: IrExpr, body: Vec<IrStmt>) -> IrStmt {
        IrStmt::While { cond, body }
    }

    /// t27: backward guarded goto -> while-true loop with inverted exit
    #[test]
    fn backward_goto_becomes_while() {
        let out = run(vec![
            assign_stmt("i", IrExpr::Int(0)),
            label("loop"),
            assign_stmt(
                "i",
                IrExpr::BinOp {
                    lhs: Box::new(var("i")),
                    op: BinOpKind::Add,
                    rhs: Box::new(IrExpr::Int(1)),
                },
            ),
            if_cond(vec![goto("loop")]),
            output("i"),
        ]);
        let json = shir_to_shir_json_raw(&program(out.clone()));
        assert!(json.contains("\"While\""), "expected While, got: {json}");
        assert!(
            json.contains("\"break\""),
            "expected an exit break, got: {json}"
        );
        assert!(!json.contains("\"Goto\""), "goto survived: {json}");
    }

    /// t31: forward guarded goto -> inverted If
    #[test]
    fn forward_goto_becomes_inverted_if() {
        let out = run(vec![
            assign_stmt("err", IrExpr::Int(0)),
            if_cond(vec![goto("cleanup")]),
            output("work"),
            label("cleanup"),
            output("cleanup"),
        ]);
        let json = shir_to_shir_json_raw(&program(out.clone()));
        assert!(!json.contains("\"Goto\""), "goto survived: {json}");
        assert!(!json.contains("\"Label\""), "label survived: {json}");
        // the skipped region must be inside an If
        assert!(json.contains("\"If\""), "no If wrapper: {json}");
    }

    /// t30: nested goto -> flag + break, label removed
    #[test]
    fn nested_goto_becomes_flag_break() {
        let inner = vec![if_cond(vec![goto("out")]), output("inner")];
        let outer = vec![while_loop(truthy(), inner.clone())];
        let out = run(vec![
            while_loop(truthy(), outer),
            label("out"),
            output("done"),
        ]);
        let json = shir_to_shir_json_raw(&program(out.clone()));
        assert!(!json.contains("\"Goto\""), "goto survived: {json}");
        assert!(!json.contains("\"Label\""), "label survived: {json}");
        assert!(json.contains("__gout"), "no flag var: {json}");
        assert!(json.contains("\"break\""), "no break: {json}");
    }

    /// t30-bug: nested goto where the loop bodies are wrapped in
    /// Blocks (the c-sh-go lowers `for (i; c; u) { ... }` to `init;
    /// while (c) { <Block { body; u }> }`). The pre-fix code added
    /// `if (flag) break` to the Block's body (a non-loop), so the
    /// throw escaped every enclosing whileLoopSync and aborted the
    /// program. The fix: only guard actual loop bodies.
    #[test]
    fn nested_goto_through_block_wrapper_does_not_escape() {
        // Shape: outer Block { outer while { inner Block { inner while {
        //   if (cond) goto out; printf; j++ } ; i++ } } }
        // Two real loops; the c-sh-go wraps each for-lowered-to-while
        // body in a Block. The pre-fix code would have emitted
        // `if (flag) break` at the outer Block level (after the
        // outer while) — a break that escapes everything and aborts.
        let inner_body = vec![
            if_cond(vec![goto("out")]),
            output("inner"),
            assign_stmt(
                "j",
                IrExpr::BinOp {
                    lhs: Box::new(var("j")),
                    op: BinOpKind::Add,
                    rhs: Box::new(IrExpr::Int(1)),
                },
            ),
        ];
        let inner_while = while_loop(truthy(), inner_body);
        let inner_block_body = vec![
            inner_while,
            assign_stmt(
                "i",
                IrExpr::BinOp {
                    lhs: Box::new(var("i")),
                    op: BinOpKind::Add,
                    rhs: Box::new(IrExpr::Int(1)),
                },
            ),
        ];
        let inner_block = IrStmt::Block(inner_block_body);
        let outer_while = while_loop(truthy(), vec![inner_block]);
        let outer_block_body = vec![outer_while];
        let outer_block = IrStmt::Block(outer_block_body);
        let out = run(vec![outer_block, label("out"), output("done")]);
        let json = shir_to_shir_json_raw(&program(out.clone()));
        assert!(!json.contains("\"Goto\""), "goto survived: {json}");
        assert!(!json.contains("\"Label\""), "label survived: {json}");
        // The inner goto becomes `flag=1; break` (one break). The
        // outer (the only LOOP step) gets `if (flag) break` at the
        // end of its body (a second break). The Block wrappers must
        // receive NO guard — the pre-fix code added one to the outer
        // Block, which would escape the program.
        let n_breaks = json.matches("\"break\"").count();
        assert_eq!(
            n_breaks, 2,
            "expected 2 break sites (goto+guard), got {n_breaks} in {json}"
        );
    }

    /// A pass over an empty list must not panic.
    #[test]
    fn empty_list_noop() {
        let out = run(vec![]);
        assert!(out.is_empty());
    }
}
