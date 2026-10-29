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
//!   - forward goto to a label in the SAME statement list -> the skipped
//!     statements are dead-coded (`If { cond: false }`); no flag is needed.
//!     This is the Go idiom `for { switch { … default: goto L } L: rest }`
//!     (posix-sh-go main.go:2072, zig-sh-go main.go:1302, and go-sh's own
//!     `goto matched` format-spec scan) — the jump skips the tail of the
//!     switch and continues at the label, WITHOUT leaving the loop.
//! Anything else is left in place and the renderers refuse loudly
//! (their `Label`/`Goto` arms emit the Unsupported marker), keeping the
//! failure-driven loops in charge.
//!
//! Pinned by frontends/c-sh-go/testdata/t27_goto.c (backward -> DoWhile),
//! t30_goto_nested.c (multi-level exit -> flag), t31_goto_forward.c
//! (forward cleanup -> inverted If), plus tests/goto_c_frontend_pins.rs
//! and tests/goto_restructure_e2e.rs (the Go frontend's own gotos, against
//! native `go run`).

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
        asm: None,
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
        // C-style for (frontend-emitted A1): gotos inside its body
        // target labels OUTSIDE the loop — same nesting rules (the
        // c-sh-go corpus's nested-goto pins regressed when the shell
        // lowering adopted the rich A1 ForInit node)
        | IrStmt::ForInit { body, .. }
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
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            restructure_stmts(body, n);
            for e in excepts.iter_mut() {
                restructure_stmts(&mut e.body, n);
            }
            restructure_stmts(else_body, n);
            restructure_stmts(finally_body, n);
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
        // THE BODY-INSIDE-IF SHAPE — the go-sh frontend's own goto loop:
        //     L:  if (c) { body…; goto L }
        // The body sits INSIDE the guarded If and the Goto is its LAST
        // statement (Go's idiomatic goto-loop), which is the mirror of
        // the `L: body; if (c) goto L;` shape handled above. The test is
        // the loop CONDITION, so this restructures to `While(c) { body… }`
        // — no flag variable needed (the jump is the last thing in the
        // body, so falling off the end re-tests) and the Label drops.
        if let IrStmt::If { cond, then, elsifs, else_ } = &stmts[i] {
            if elsifs.is_empty() && else_.is_empty() && then.len() >= 2 {
                if let IrStmt::Goto(name) = &then[then.len() - 1] {
                    if let Some((_, lpos)) = labels.iter().find(|(l, _)| l == name) {
                        // the goto targets an ENCLOSING label; the rest of
                        // the then-body is the loop and `cond` is the test
                        if *lpos <= i {
                            let cond = cond.clone();
                            let mut body: Vec<IrStmt> =
                                then[..then.len() - 1].to_vec();
                            restructure_stmts(&mut body, n);
                            let while_stmt = IrStmt::While { cond, body };
                            stmts[*lpos] = IrStmt::Block(vec![while_stmt]);
                            stmts.remove(i);
                            return true;
                        }
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

/// Does this program still carry any `Label`/`Goto` node?
///
/// The estree ingress historically skipped the restructure pass on the
/// theory that "the estree emitter handles goto/cfor natively" — it does
/// not (shir.rs has no Label/Goto arm, so a surviving jump renders as
/// `sh2.unsupported("goto <label>")` and dies at runtime). Running the
/// pass UNCONDITIONALLY there changed the emitted output for
/// goto/cfor-bearing examples, so gate it on this predicate: a program
/// with no jumps is byte-identical to before.
pub fn program_has_goto(prog: &IrProgram) -> bool {
    fn in_stmts(stmts: &[IrStmt]) -> bool {
        stmts.iter().any(in_stmt)
    }
    fn in_stmt(s: &IrStmt) -> bool {
        match s {
            IrStmt::Label(_) | IrStmt::Goto(_) => true,
            IrStmt::If { then, elsifs, else_, .. } => {
                in_stmts(then) || elsifs.iter().any(|(_, b)| in_stmts(b)) || in_stmts(else_)
            }
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::ForInit { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Redirect { inner: body, .. }
            | IrStmt::Function { body, .. } => in_stmts(body),
            IrStmt::Case { clauses, .. } => clauses.iter().any(|c| in_stmts(&c.body)),
            _ => false,
        }
    }
    in_stmts(&prog.stmts) || prog.subs.iter().any(|sub| in_stmts(&sub.body))
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
        // after the drain the list is [Label, Goto] at lpos, lpos+1 —
        // the splice must use the POST-drain positions (the old
        // `lpos..=gpos` panicked for any non-empty loop body: a real
        // backward goto — the batch `:loop ... goto loop` idiom —
        // never reached the C frontend's forward-goto corpus).
        stmts.splice(lpos..lpos + 2, [while_stmt]);
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
        let _converted = convert_successor_exits(&mut span, successor_label.as_deref());
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
#[derive(Clone, Copy, PartialEq, Debug)]
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
        None,
        labels,
        &mut path,
        &mut loop_steps,
        &mut out,
    );
    out
}

/// `s` is a statement that IS a loop (its `body` may carry a meaningful
/// `break`). Blocks/Subshells/Functions have a `body` too, but a `break`
/// inside them targets an enclosing loop of the SOURCE, not the node —
/// so they must never be recorded as loop steps or receive a guard.
fn is_loop_stmt(s: &IrStmt) -> bool {
    matches!(
        s,
        IrStmt::While { .. }
            | IrStmt::DoWhile { .. }
            | IrStmt::For { .. }
            | IrStmt::ForInit { .. }
    )
}

/// Record the path steps into `s`'s nested statement lists (tree order).
///
/// `loop_step` is the path index of the nearest enclosing LOOP body (the
/// loop currently being descended into), or `None` at the top level. It is
/// pushed onto `loop_steps` when the goto is found so that gotos reached
/// through a NON-body branch (a `Case` clause, an `If` arm, a pipeline
/// stage) still know how many loops enclose them. Without it the descent
/// through `CaseClause`/`Then`/`Else`/`Stage` silently DROPPED the loop
/// (a `default: goto L` in a switch inside a `for` recorded zero loop
/// steps), so `handle_nested` had no loop to guard and the tail after the
/// statement was left to run with the flag set.
fn descend_from(
    s: &IrStmt,
    idx: usize,
    loop_step: Option<usize>,
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
            push_and_walk(then, idx, Branch::Then, loop_step, labels, path, loop_steps, out);
            for (k, (_, b)) in elsifs.iter().enumerate() {
                push_and_walk(b, idx, Branch::Elsif(k), loop_step, labels, path, loop_steps, out);
            }
            push_and_walk(else_, idx, Branch::Else, loop_step, labels, path, loop_steps, out);
        }
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::ForInit { body, .. } => {
            // this node IS a loop: the body list it opens gets path index
            // `path.len()`, which becomes the enclosing-loop step for
            // everything below it.
            let step = Some(path.len());
            push_and_walk(body, idx, Branch::Body, step, labels, path, loop_steps, out);
        }
        IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. }
        | IrStmt::Function { body, .. } => {
            // transparent for loop nesting: a non-loop body must inherit
            // the enclosing loop step, not clear it.
            push_and_walk(body, idx, Branch::Body, loop_step, labels, path, loop_steps, out);
        }
        IrStmt::Case { clauses, .. } => {
            for (k, c) in clauses.iter().enumerate() {
                push_and_walk(
                    &c.body,
                    idx,
                    Branch::CaseClause(k),
                    loop_step,
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
                    loop_step,
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
    loop_step: Option<usize>,
    labels: &[(String, usize)],
    path: &mut Vec<(usize, Branch)>,
    loop_steps: &mut Vec<usize>,
    out: &mut Option<Hit>,
) {
    if out.is_some() {
        return;
    }
    path.push((idx, br));
    // `loop_step` is the step of the loop this list is the body OF (None
    // for a Block/If arm/pipeline stage that is not a loop body), so it is
    // recorded only when it names THIS step — a goto reached through a
    // non-body branch still inherits the enclosing loop from the caller.
    let recorded = loop_step == Some(path.len() - 1);
    if recorded {
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
        descend_from(st, j, loop_step, labels, path, loop_steps, out);
        if out.is_some() {
            path.pop();
            return;
        }
    }
    if recorded {
        loop_steps.pop();
    }
    path.pop();
}

/// Eliminate a goto whose target label is NOT a top-level sibling after the
/// goto's enclosing loop. Three topologies reach here and they need
/// different lowerings; what they share is that the jump's destination is
/// the label, reachable by UNWINDING `hit.path` outward until a list that
/// actually contains the label.
///
/// * **residual (a), cross-list** — the label is a top-level sibling after
///   the ancestor: the jump leaves the loop chain. `flag = 1; break` exits
///   the innermost loop, each outer loop body gets `if (flag) break`, and
///   the tail between the ancestor and the label is guarded by the flag.
/// * **ancestor-list** — the goto sits inside a NESTED list (a `Case`
///   clause, an `If` arm) and the label is a later statement of one of the
///   enclosing lists (the go-sh `for { switch { … default: goto L } L: rest }`
///   shape at posix-sh-go main.go:2072 and zig-sh-go main.go:1302). The
///   jump must abandon the inner list and continue at the label WITHOUT
///   leaving the loop, so `break` is wrong. Both the skipped region (after
///   the goto, out to the label's list) and the label's own tail stay put;
///   only the skipped statements are dead-coded away.
/// * **same-list** — a special case of the above where the goto's own list
///   already holds the label.
fn handle_nested(stmts: &mut Vec<IrStmt>, hit: Hit, n: &mut usize) -> bool {
    if hit.lpos <= hit.path.first().map(|(i, _)| *i).unwrap_or(usize::MAX) {
        return false; // label before/inside the loop chain — refuse
    }
    // A forward jump that never leaves a loop is lowered by dead-coding the
    // skipped statements; only a jump that DOES leave a loop needs the
    // flag+break machinery below.
    if let Some(sites) = forward_skip_site(stmts, &hit.path, hit.goto_index, &hit.label) {
        // Apply the ranges deepest-first: draining a deeper range does not
        // move anything in an outer list, but a shallower drain would
        // invalidate the deeper paths' indices.
        let mut sites = sites;
        sites.sort_by_key(|(p, _, _)| std::cmp::Reverse(p.len()));
        for (prefix, from, to) in sites {
            let list = descend_mut(stmts, &prefix);
            let to = to.min(list.len());
            if from >= to {
                continue;
            }
            let mut skipped: Vec<IrStmt> = list.drain(from..to).collect();
            // The dead region may still CONTAIN a goto (when the jump's
            // ancestor had successors between the goto and this level).
            // Leaving one there would make `restructure_pass` re-find it on
            // the next iteration and never converge — and it is unreachable
            // by construction, so a jump inside it can never be taken.
            strip_jumps(&mut skipped);
            if !skipped.is_empty() {
                list.insert(
                    from,
                    IrStmt::If {
                        cond: IrExpr::Str("false".to_string(), StrStyle::DoubleQuoted),
                        then: vec![],
                        elsifs: vec![],
                        else_: skipped,
                    },
                );
            }
        }
        return true;
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
    // 2b. GUARD THE TAIL between the goto and its label. After the
    //     innermost `break` unwinds, every statement that sits between
    //     them would otherwise run UNGUARDED with the flag set (the
    //     observed zig-sh-go spin: `no-match` printed forever because
    //     `i` never advanced — the flag was set but nothing read it).
    //     Skip the tail under the flag, and CLEAR the flag at the label
    //     position so subsequent iterations start clean.
    //
    //     By this point the jump is known to LEAVE a loop (a forward jump
    //     that stays inside one returned above), so the label is either a
    //     top-level sibling AFTER the ancestor — the c-sh-go / go-sh
    //     `for { … goto L }; L: rest` shape — or, when the ancestor is the
    //     loop itself and the label is its immediate sibling, there is no
    //     tail and the guard degenerates to reading the flag once (which
    //     is what an immediate exit needs: the loop body already broke
    //     out, and the flag must be consumed exactly once).
    let anc_idx = hit.path.first().map(|(i, _)| *i);
    let Some(aidx) = anc_idx else {
        // the goto is itself at the top level of a list whose label is a
        // sibling — no enclosing loop, so no unwind to wait for
        stmts.remove(hit.lpos);
        return true;
    };
    let anc_is_loop = is_loop_stmt_at_root(stmts, aidx);
    if !anc_is_loop && hit.lpos <= aidx {
        return false; // label before/inside a non-loop ancestor — refuse
    }
    // recompute the label position (outer guards above may have
    // spliced bodies deeper — the top level is untouched by them)
    let lpos_now = stmts
        .iter()
        .position(|s| matches!(s, IrStmt::Label(nm) if nm == &hit.label))
        .unwrap_or(hit.lpos);
    if anc_is_loop && lpos_now > aidx {
        // case (a): guard the region between the loop and its label. The
        // `else_` holds the tail; when the label is immediately adjacent
        // the `else_` is empty and the If degenerates to reading the flag
        // (which is exactly what an immediate exit needs — the loop body
        // already broke out, and the flag must be consumed once).
        let tail: Vec<IrStmt> = stmts.drain(aidx + 1..lpos_now).collect();
        let guarded = IrStmt::If {
            cond: var(&flag),
            then: vec![],
            elsifs: vec![],
            else_: tail,
        };
        stmts.insert(aidx + 1, guarded);
    }
    // clear the flag where the label was (fall-through entry point)
    let lpos_after = stmts
        .iter()
        .position(|s| matches!(s, IrStmt::Label(nm) if nm == &hit.label));
    if let Some(lp) = lpos_after {
        stmts[lp] = assign_stmt(&flag, st("0"));
    }
    true
}

/// Decide how to eliminate a goto that is NOT a plain top-level sibling
/// jump. The jump's destination — the `Label` — is found by walking OUTWARD
/// from the goto's own statement list:
///
/// * the label is in the goto's own list, later in the list → pure forward
///   jump *within* that list; nothing is left, no loop is crossed;
/// * the label is in an ENCLOSING list reached only through `Block`/`If`/
///   `Case`/`Pipeline` steps → a forward jump out of a nested list; the
///   statements skipped are those after the goto out to (but not including)
///   the label, and again no loop is crossed;
/// * the label is outside a LOOP body (or the goto is at the top level and
///   the label is a sibling after the enclosing loop) → the jump leaves a
///   loop, and only that case needs the flag+break machinery.
///
/// `Some(skip_sites)` for the first two (a list of `(path_prefix, from, to)`
/// ranges to dead-code, innermost first), `None` for the third.
///
/// The ranges matter: the jump's destination is the label, so what becomes
/// unreachable is only what lies AFTER the jump's ancestor at each level —
/// never the ancestor itself, which may hold statements that run before the
/// jump (the go-sh `default: print; goto L` arm: the print must still
/// execute, only the jump's successors are dead).
fn forward_skip_site(
    stmts: &[IrStmt],
    path: &[(usize, Branch)],
    goto_index: usize,
    label: &str,
) -> Option<Vec<(Vec<(usize, Branch)>, usize, usize)>> {
    // Try every prefix from the deepest (the goto's own list) outward.
    for depth in (0..=path.len()).rev() {
        let prefix: Vec<(usize, Branch)> = path[..depth].to_vec();
        let Some(list) = list_at(stmts, &prefix) else {
            continue;
        };
        let Some(lpos) = list
            .iter()
            .position(|s| matches!(s, IrStmt::Label(nm) if nm == label))
        else {
            continue;
        };
        // Where does the dead region start in THIS list?
        let _skip_from = if depth == path.len() {
            // the goto is a statement of this list
            if lpos <= goto_index {
                continue; // backward — not a forward jump
            }
            goto_index + 1
        } else {
            // the goto is inside a nested list of the statement at
            // `path[depth].0`; the jump escapes that statement, so the
            // statement's SUCCESSORS in this list are dead
            let Some((idx, _)) = path.get(depth) else {
                continue;
            };
            if lpos <= *idx {
                continue;
            }
            *idx + 1
        };
        // Crossing a LOOP body on the way out is not a forward jump. Only
        // the steps BELOW the label's list are traversed by the jump, so
        // scan `depth..path.len()` — scanning the prefix would look at lists
        // the goto is above, and miss the loop it is actually inside.
        if (depth..path.len()).any(|k| {
            let (_, br) = path[k];
            if br != Branch::Body {
                return false;
            }
            let Some(ls) = list_at(stmts, &path[..k]) else {
                return false;
            };
            ls.get(path[k].0).map(is_loop_stmt).unwrap_or(false)
        }) {
            return None;
        }
        // The dead ranges, innermost first. Inside the goto's own list the
        // region is (goto+1 .. label); at every level outward it is the
        // successors of the statement the jump escaped, up to the label's
        // position in that list.
        let mut sites: Vec<(Vec<(usize, Branch)>, usize, usize)> = Vec::new();
        if depth < path.len() {
            // Innermost: the goto's own list — drop the goto itself and
            // everything after it (all of it is skipped by the jump).
            sites.push((path.to_vec(), goto_index, usize::MAX));
            for d in (depth..path.len()).rev() {
                let Some((idx, _)) = path.get(d) else { continue };
                let outer_prefix = path[..d].to_vec();
                let outer_list = list_at(stmts, &outer_prefix)?;
                let from = idx + 1;
                let to = if d == depth { lpos } else { outer_list.len() };
                sites.push((outer_prefix, from, to));
            }
        } else {
            sites.push((prefix.clone(), goto_index + 1, lpos));
        }
        return Some(sites);
    }
    None
}

/// The statement list at the end of `path` (immutable).
fn list_at<'a>(stmts: &'a [IrStmt], path: &[(usize, Branch)]) -> Option<&'a [IrStmt]> {
    let mut list: &[IrStmt] = stmts;
    for (idx, br) in path {
        let s = list.get(*idx)?;
        list = match br {
            Branch::Body => body_list_ref(s),
            Branch::Then => match s {
                IrStmt::If { then, .. } => then,
                _ => return None,
            },
            Branch::Else => match s {
                IrStmt::If { else_, .. } => else_,
                _ => return None,
            },
            Branch::Elsif(k) => match s {
                IrStmt::If { elsifs, .. } => &elsifs[*k].1,
                _ => return None,
            },
            Branch::CaseClause(k) => match s {
                IrStmt::Case { clauses, .. } => &clauses[*k].body,
                _ => return None,
            },
            Branch::Stage(k) => match s {
                IrStmt::Pipeline { stages, .. } => &stages[*k],
                _ => return None,
            },
        };
    }
    Some(list)
}

/// Immutable twin of [`body_list_mut`]: the child statement list of a node
/// whose `body` is a statement list.
fn body_list_ref(s: &IrStmt) -> &[IrStmt] {
    match s {
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::ForInit { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. }
        | IrStmt::Function { body, .. } => body,
        _ => &[],
    }
}


/// Remove `Label`/`Goto` nodes from a region that has been made unreachable.
///
/// Used when a forward jump's skipped region is dead-coded: the region is
/// unreachable, so a jump inside it can never be taken, but a surviving
/// `Goto` would be re-discovered by [`restructure_pass`] on the next
/// iteration and the pass would never converge.
fn strip_jumps(stmts: &mut Vec<IrStmt>) {
    stmts.retain(|s| !matches!(s, IrStmt::Label(_) | IrStmt::Goto(_)));
    for s in stmts.iter_mut() {
        strip_jumps_children(s);
    }
}

fn strip_jumps_children(s: &mut IrStmt) {
    match s {
        IrStmt::If { then, elsifs, else_, .. } => {
            strip_jumps(then);
            for (_, b) in elsifs.iter_mut() {
                strip_jumps(b);
            }
            strip_jumps(else_);
        }
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::ForInit { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body)
        | IrStmt::Redirect { inner: body, .. }
        | IrStmt::Function { body, .. } => strip_jumps(body),
        IrStmt::Case { clauses, .. } => {
            for c in clauses.iter_mut() {
                strip_jumps(&mut c.body);
            }
        }
        IrStmt::Pipeline { stages, .. } => {
            for stage in stages.iter_mut() {
                strip_jumps(stage);
            }
        }
        IrStmt::Try { body, excepts, else_body, finally_body } => {
            strip_jumps(body);
            for e in excepts.iter_mut() {
                strip_jumps(&mut e.body);
            }
            strip_jumps(else_body);
            strip_jumps(finally_body);
        }
        _ => {}
    }
}

/// Is the top-level statement at `idx` a loop (a node whose `body` is a
/// loop body, so a `break` inside it targets THIS list)?
fn is_loop_stmt_at_root(stmts: &[IrStmt], idx: usize) -> bool {
    stmts.get(idx).map(is_loop_stmt).unwrap_or(false)
}

/// True when the stmt at the given path step is a real loop. See
/// [`is_loop_stmt`]: a `Block` at the same position is also a `Body` step
/// (Block's body is its child list) but is NOT a loop — appending
/// `if (flag) break` there escapes the program. Used by `handle_nested`
/// to skip non-loop steps.
fn is_loop_stmt_at(stmts: &Vec<IrStmt>, path: &[(usize, Branch)], step: usize) -> bool {
    let mut list = stmts;
    for (k, (idx, _br)) in path.iter().enumerate() {
        let s = &list[*idx];
        if k == step {
            return is_loop_stmt(s);
        }
        list = match s {
            IrStmt::If { then, .. } => then,
            IrStmt::While { body, .. }
            | IrStmt::DoWhile { body, .. }
            | IrStmt::For { body, .. }
            | IrStmt::ForInit { body, .. }
            | IrStmt::Block(body)
            | IrStmt::Subshell(body)
            | IrStmt::Background(body)
            | IrStmt::Redirect { inner: body, .. }
            | IrStmt::Function { body, .. } => body,
            IrStmt::Case { clauses: _, .. } => {
                // path's body step into a Case is the union of clause
                // bodies — but for our purposes, an `if (flag) break`
                // at the case level is still inside the loop (the
                // whileLoop catches it). A pure-Case step (no loop
                // above) would still be wrong, so we conservatively
                // say false for case steps.
                return false;
            }
            IrStmt::Pipeline { stages: _, .. } => return false,
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
        | IrStmt::ForInit { body, .. }
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
            var_storage: vec![],
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


    /// The go-sh `for { switch { … default: goto L } L: … }` shape: the
    /// goto is inside a Case clause and BOTH the goto and its label live
    /// inside the loop's own body list. Pre-fix this left `loop_steps`
    /// EMPTY (the descent through `CaseClause` dropped the enclosing
    /// loop) and `handle_nested` bailed out at `lpos <= anc_idx`, so the
    /// statements after the label ran with the flag still set and the
    /// jump never took effect.
    ///
    /// Pinned by frontends/posix-sh-go/main.go:2072 (`default: goto
    /// fiDone`) and :2857 (`goto additionalArgs`), and by
    /// frontends/zig-sh-go/main.go:1302.
    #[test]
    fn goto_inside_a_switch_inside_the_loop_body_reaches_its_tail() {
        // while true { switch { case a: output("kw"); break
        //                       default: output("dflt"); goto tail }
        //              tail: output("args") }
        let case_body = vec![IrStmt::Break];
        let default_body = vec![output("dflt"), goto("tail")];
        let switch_stmt = IrStmt::Case {
            discriminant: st("kw"),
            clauses: vec![
                IrCaseClause {
                    patterns: vec!["if".to_string()],
                    body: case_body,
                },
                IrCaseClause {
                    patterns: vec![],
                    body: default_body,
                },
            ],
        };
        let out = run(vec![while_loop(
            truthy(),
            vec![
                switch_stmt,
                label("tail"),
                output("args"),
                IrStmt::Break,
            ],
        )]);
        let json = shir_to_shir_json_raw(&program(out.clone()));
        assert!(!json.contains("\"Goto\""), "goto survived: {json}");
        // The jump is a forward jump that never leaves the loop: the `goto`
        // is simply dropped (the label is right there in the enclosing
        // list), so no flag variable and no dead region are needed. Pre-fix
        // the descent through `CaseClause` dropped the enclosing loop from
        // `loop_steps` and `handle_nested` bailed out at `lpos <= anc_idx`,
        // rewriting NOTHING: the `default` arm set a flag nothing read, the
        // `break` unwound the loop, and the label's tail never ran.
        assert_eq!(
            json.matches("\"__gtail\"").count(),
            0,
            "no flag variable should be needed for a forward jump: {json}"
        );
        // The `default` arm's own statements must SURVIVE (they run before
        // the jump) — "dflt" is the print that precedes the goto.
        assert!(
            json.contains("\"dflt\""),
            "the statements before the goto were dropped: {json}"
        );
        // `args` (the label's tail) must remain reachable.
        assert!(
            json.contains("\"args\""),
            "the label's tail was swallowed: {json}"
        );
    }

    /// The sibling shape: the goto's top-level ancestor IS the loop and
    /// the label is a top-level sibling AFTER it (`for { … goto L }; L:`).
    /// Pre-fix `handle_nested` returned early whenever the target label
    /// sat at or before the ancestor's index, so nothing guarded the tail
    /// and the loop simply ran to completion with the flag set.
    #[test]
    fn goto_out_of_a_top_level_loop_guards_the_tail() {
        let loop_body = vec![
            if_cond(vec![goto("done")]),
            output("no"),
        ];
        let out = run(vec![
            IrStmt::ForInit {
                init: vec![],
                cond: st("c"),
                step: vec![],
                body: loop_body,
            },
            label("done"),
            output("end"),
        ]);
        let json = shir_to_shir_json_raw(&program(out.clone()));
        assert!(!json.contains("\"Goto\""), "goto survived: {json}");
        assert!(!json.contains("\"Label\""), "label survived: {json}");
        assert_eq!(
            json.matches("__gdone").count(),
            3,
            "expected flag set + guard read + tail clear: {json}"
        );
        // `end` must be reachable only through the flag guard (an If
        // whose else_ holds the tail), never unconditionally at top level.
        assert!(json.contains("\"If\""), "no guard If: {json}");
    }

    #[test]
    fn body_inside_if_goto_loop() {
        // L: if (c) { body; goto L }
        let stmts = vec![
            IrStmt::Label("loop".into()),
            IrStmt::If {
                cond: IrExpr::Str("c".into(), StrStyle::DoubleQuoted),
                then: vec![
                    IrStmt::Expr(IrExpr::Str("body".into(), StrStyle::DoubleQuoted)),
                    IrStmt::Goto("loop".into()),
                ],
                elsifs: vec![],
                else_: vec![],
            },
        ];
        let mut prog = program(stmts);
        RestructureGoto.run(&mut prog, &PassContext::default());
        let raw = shir_to_shir_json_raw(&prog);
        println!("RESULT: {raw}");
    }
}
