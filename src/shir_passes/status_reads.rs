//! Status-liveness analysis — does the program READ the shell status (`$?`),
//! and WHICH per-statement status stores can a read observe?
//!
//! A renderer that maintains a status register (`__sh_rc`, `__SH_RC`, …)
//! emits a store after nearly every statement so a later `$?` sees the
//! right value. When the program contains NO `$?` read, every one of those
//! stores is unobservable except the one that becomes the process exit
//! code — so a backend may drop the register entirely (and render the
//! final `exit` from the statically-known last statement instead).
//!
//! When the program DOES read `$?`, a store is still dead unless a read
//! (or the process exit) can reach it before the NEXT store: `false` at
//! line 3 is invisible to a `$?` reader at line 40 if line 30 stored its
//! own status. [`status_liveness`] is that per-statement fact, resolved
//! per body — a `local ec=$?` inside a function body localises to that
//! body instead of arming every store in the enclosing script.
//!
//! These are ANALYSES, not transforms: they answer questions about the
//! program and leave it untouched. They are language-neutral and live here
//! so every backend can share one definition of "status is observable"
//! (the Python renderer consumes both today).
//!
//! ## Method (conservative)
//! [`program_reads_status`] is `true` if ANY of:
//!   - the program's debug text contains `$?` (the test/param operand text
//!     spellings carry it verbatim — this is the cheap total fallback),
//!   - a `getVar("?")` / `Var("?", …)` read exists (a `$?` expansion),
//!   - a status-named call exists (`lastStatus`, `lastexit`, `exitCode`, …),
//!   - a bare `exit` exists (bash exits with the current status),
//!   - the `block` seam exists (its Python helper returns `__sh_rc == 0`).
//! `false` is only returned when none of those appear, which is the sound
//! direction: a missed read would keep the register, a false positive only
//! costs the optimization.
//!
//! [`status_liveness`] localises the reads to top-level statement positions
//! within each body (the main list and every function/sub body). A body
//! whose FIRST read is not preceded by a definite store observes the
//! CALLER's status, which no body-local position explains — then nothing is
//! localised (`None` everywhere), the sound default. A read visible only
//! through the coarse debug-text fallback likewise yields `None`.

use crate::ir::{InterpPart, IrExpr, IrProgram, IrStmt};

/// Names that read the shell status register in the shared call vocabulary.
fn status_func(func: &str) -> bool {
    matches!(
        func,
        "lastStatus" | "lastexit" | "lastExit" | "status" | "exitCode"
    )
}

fn expr_reads_status(e: &IrExpr, fns: bool) -> bool {
    match e {
        IrExpr::Var(n, _) => n == "?",
        IrExpr::Str(s, _) => s.contains("$?"),
        IrExpr::RawExpr(s) => s.contains("$?"),
        IrExpr::Index { key, .. } => expr_reads_status(key, fns),
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_reads_status(lhs, fns) || expr_reads_status(rhs, fns)
        }
        IrExpr::Call { func, args } => {
            status_func(func)
                // the `block` seam's helper is `return __sh_rc == 0`: a
                // call to it observes the register like a `$?` read
                || func == "block"
                || (func == "getVar"
                    && matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "?"))
                || args.iter().any(|a| expr_reads_status(a, fns))
        }
        IrExpr::MethodCall { obj, args, .. } => {
            expr_reads_status(obj, fns) || args.iter().any(|a| expr_reads_status(a, fns))
        }
        IrExpr::Ternary { cond, then, else_ } => {
            expr_reads_status(cond, fns)
                || expr_reads_status(then, fns)
                || expr_reads_status(else_, fns)
        }
        IrExpr::DefinedOr { expr, default } => {
            expr_reads_status(expr, fns) || expr_reads_status(default, fns)
        }
        IrExpr::Interpolate(parts) => parts.iter().any(|p| match p {
            InterpPart::Lit(t) => t.contains("$?"),
            InterpPart::Expr(x) => expr_reads_status(x, fns),
        }),
        IrExpr::Capture { expr, .. } => expr_reads_status(expr, fns),
        // a `$( … )` body runs at the point of use, so it reads HERE
        IrExpr::Arrow(b) => stmts_read_status_in(b, fns),
        IrExpr::Array(items) => items.iter().any(|x| expr_reads_status(x, fns)),
        _ => false,
    }
}

fn stmts_read_status_in(stmts: &[IrStmt], fns: bool) -> bool {
    stmts.iter().any(|s| stmt_reads_status_in(s, fns))
}

/// `fns` = descend into function BODIES (a definition DOES read `$?` when
/// the function is called). The liveness pass sets it false: a body is its
/// own statement list, analysed separately.
fn stmt_reads_status_in(s: &IrStmt, fns: bool) -> bool {
    match s {
        IrStmt::Output { value, .. } => expr_reads_status(value, fns),
        IrStmt::WriteFile { path, content, .. } => {
            expr_reads_status(path, fns) || expr_reads_status(content, fns)
        }
        IrStmt::Assign { targets, expr, .. } => {
            targets
                .iter()
                .any(|t| t.indices.iter().any(|i| expr_reads_status(i, fns)))
                || expr_reads_status(expr, fns)
        }
        IrStmt::Declare { init, .. } => {
            init.as_ref().map(|e| expr_reads_status(e, fns)).unwrap_or(false)
        }
        IrStmt::DeclareArray { elements, .. } => {
            elements.iter().any(|e| expr_reads_status(e, fns))
        }
        IrStmt::Expr(e) => expr_reads_status(e, fns),
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            expr_reads_status(cond, fns)
                || stmts_read_status_in(then, fns)
                || elsifs.iter().any(|(c, b)| {
                    expr_reads_status(c, fns) || stmts_read_status_in(b, fns)
                })
                || stmts_read_status_in(else_, fns)
        }
        IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, .. } => {
            expr_reads_status(cond, fns) || stmts_read_status_in(body, fns)
        }
        IrStmt::DoWhile { body, cond, .. } => {
            stmts_read_status_in(body, fns) || expr_reads_status(cond, fns)
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            stmts_read_status_in(init, fns)
                || expr_reads_status(cond, fns)
                || stmts_read_status_in(step, fns)
                || stmts_read_status_in(body, fns)
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            stmts_read_status_in(body, fns)
                || excepts.iter().any(|e| stmts_read_status_in(&e.body, fns))
                || stmts_read_status_in(else_body, fns)
                || stmts_read_status_in(finally_body, fns)
        }
        // `exit` with no argument exits with the current status.
        IrStmt::Exit(e) => e.as_ref().map(|x| expr_reads_status(x, fns)).unwrap_or(true),
        IrStmt::Function { body, .. } => fns && stmts_read_status_in(body, fns),
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
            stmts_read_status_in(b, fns)
        }
        IrStmt::Redirect { inner, .. } => stmts_read_status_in(inner, fns),
        IrStmt::Pipeline { stages, .. } => stages.iter().any(|st| stmts_read_status_in(st, fns)),
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            expr_reads_status(discriminant, fns)
                || clauses.iter().any(|c| stmts_read_status_in(&c.body, fns))
        }
        _ => false,
    }
}

/// Does this statement observe the shell status (`$?`)? A bare `exit` does
/// too: bash exits with the CURRENT status, so the preceding store must be
/// live.
pub fn stmt_reads_status(s: &IrStmt) -> bool {
    stmt_reads_status_in(s, true)
}

fn stmts_read_status(stmts: &[IrStmt]) -> bool {
    stmts_read_status_in(stmts, true)
}

/// True when the program observes the shell status (`$?` / lastStatus).
pub fn program_reads_status(prog: &IrProgram) -> bool {
    // Total fallback: the lowered operand text (`test("-z \"$?\"")`,
    // `param("or", "?", …)`) carries `$?` verbatim, and every IR node's
    // Debug form includes its own strings.
    if format!("{:?}", prog).contains("$?") {
        return true;
    }
    if stmts_read_status(&prog.stmts) {
        return true;
    }
    prog.subs.iter().any(|s| stmts_read_status(&s.body))
}

// ── per-statement store liveness ────────────────────────────────────────

/// Does rendering this statement ALWAYS store a status, so that a later
/// read cannot see an earlier store? Deliberately narrow: a statement that
/// only MAY store (an `if`, a loop) is not a barrier, which keeps the
/// earlier store live — the sound direction. A false negative costs one
/// `__sh_rc = …` line; a false positive would drop a store a later `$?`
/// can observe.
pub fn stmt_definitely_stores_status(s: &IrStmt) -> bool {
    match s {
        // A single-target, non-asm assign ends in a status store — except a
        // command substitution, which renders as `x = __sh_bash(…)` and
        // leaves the register exactly as it found it.
        IrStmt::Assign {
            targets, asm, expr, ..
        } => asm.is_none() && targets.len() == 1 && !expr_has_capture(expr),
        IrStmt::Declare { init, .. } => init.as_ref().map(|e| !expr_has_capture(e)).unwrap_or(true),
        IrStmt::DeclareArray { .. } => true,
        IrStmt::Output { .. } => true,
        IrStmt::WriteFile { .. } => true,
        IrStmt::Exit(_) => true,
        IrStmt::Expr(e) => expr_definitely_stores_status(e),
        // The control-flow arms all clear the status BEFORE they do
        // anything else (a false condition with no branch leaves bash `$?`
        // at 0; a taken branch overwrites it), so a read after one of these
        // can never observe an EARLIER store.
        IrStmt::If { .. }
        | IrStmt::While { .. }
        | IrStmt::For { .. }
        | IrStmt::Case { .. } => true,
        _ => false,
    }
}

/// Does this expression hold a command substitution? The backend renders it
/// as `__sh_bash(…)`/`__sh_cap(…)` and does not write the status register for
/// it, so a statement containing one cannot be assumed to store the status.
fn expr_has_capture(e: &IrExpr) -> bool {
    let d = format!("{e:?}");
    d.contains("Capture")
        || d.contains("\"capture\"")
        || d.contains("\"captureWords\"")
        || d.contains("__sh_bash")
}

/// A statement-position expression: the renderer's arms all record a
/// status except the pure-print shapes (`grepMatches`, the pipeline
/// fallback) and `redirect` (which re-enters the statement arms).
pub fn expr_definitely_stores_status(e: &IrExpr) -> bool {
    match e {
        // both arith statement arms store a boolean rc
        IrExpr::Arith(_) => true,
        IrExpr::Call { func, .. } => !matches!(
            func.as_str(),
            // pure-print arms, the pipeline fallback, and the fnValue
            // TODO stub all return without writing the status
            "grepMatches" | "pipeline" | "redirect" | "fnValue"
        ),
        _ => true,
    }
}

/// Does this list read `$?` before its OWN first definite store? Such a read
/// observes the status the enclosing statement started with, so the
/// enclosing statement's store must stay live.
pub fn list_reads_before_store(stmts: &[IrStmt]) -> bool {
    let reads: Vec<bool> = stmts
        .iter()
        .map(|s| stmt_reads_status_in(s, false))
        .collect();
    match reads.iter().position(|r| *r) {
        Some(first) => !stmts[..first].iter().any(stmt_definitely_stores_status),
        None => false,
    }
}

/// Can a `$?` read inside this statement observe the status it starts with
/// (a read in the condition, or at the head of a nested list)? Only then
/// does the statement need its own (leading) store when nothing after it
/// reads the register.
pub fn stmt_nested_reads_before_store(s: &IrStmt) -> bool {
    match s {
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            expr_reads_status(cond, false)
                || list_reads_before_store(then)
                || elsifs.iter().any(|(c, b)| {
                    expr_reads_status(c, false) || list_reads_before_store(b)
                })
                || list_reads_before_store(else_)
        }
        IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, .. } => {
            expr_reads_status(cond, false) || list_reads_before_store(body)
        }
        IrStmt::DoWhile { body, cond, .. } => {
            list_reads_before_store(body) || expr_reads_status(cond, false)
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            list_reads_before_store(init)
                || expr_reads_status(cond, false)
                || list_reads_before_store(step)
                || list_reads_before_store(body)
        }
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            expr_reads_status(discriminant, false)
                || clauses.iter().any(|c| list_reads_before_store(&c.body))
        }
        IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
            list_reads_before_store(b)
        }
        IrStmt::Redirect { inner, .. } => list_reads_before_store(inner),
        IrStmt::Pipeline { stages, .. } => stages.iter().any(|st| list_reads_before_store(st)),
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            list_reads_before_store(body)
                || excepts.iter().any(|e| list_reads_before_store(&e.body))
                || list_reads_before_store(else_body)
                || list_reads_before_store(finally_body)
        }
        _ => false,
    }
}

/// Read-before-next-store liveness for a whole program: the main statement
/// list plus every function/sub body, each with its own per-statement
/// vector. A body's `$?` reads localise there, so a `local ec=$?` inside a
/// function no longer arms every store in the enclosing script.
#[derive(Default)]
pub struct StatusLiveness {
    /// Main statement list (`None` = keep every store).
    pub main: Option<Vec<bool>>,
    /// Function/sub bodies by name (`None`/absent = keep every store).
    pub bodies: Vec<(String, Option<Vec<bool>>)>,
    /// Is the register's INITIAL value observed? True when some list reads
    /// `$?` before any of its own live store (so a backend that dropped
    /// every earlier store still needs the zero initialiser).
    pub init_observed: bool,
}

/// Collect every function definition nested in `stmts`, computing each
/// body's liveness independently.
fn collect_function_bodies(
    stmts: &[IrStmt],
    bodies: &mut Vec<(String, Option<Vec<bool>>)>,
    escaped: &mut bool,
    init_observed: &mut bool,
) {
    for s in stmts {
        match s {
            IrStmt::Function {
                name,
                body,
                named_blocks,
                ..
            } => {
                // a named block renders inside the same body, so a `$?`
                // read there has no position of its own: keep them all
                let named_reads = named_blocks
                    .iter()
                    .any(|(_, b)| stmts_read_status_in(b, false));
                let v = if named_reads {
                    None
                } else {
                    body_store_liveness(body, bodies, escaped, init_observed, true)
                };
                bodies.push((name.clone(), v));
            }
            IrStmt::If {
                then,
                elsifs,
                else_,
                ..
            } => {
                collect_function_bodies(then, bodies, escaped, init_observed);
                for (_, b) in elsifs {
                    collect_function_bodies(b, bodies, escaped, init_observed);
                }
                collect_function_bodies(else_, bodies, escaped, init_observed);
            }
            IrStmt::While { body, .. } | IrStmt::For { body, .. } | IrStmt::DoWhile { body, .. } => {
                collect_function_bodies(body, bodies, escaped, init_observed)
            }
            IrStmt::ForInit {
                init,
                step,
                body,
                ..
            } => {
                collect_function_bodies(init, bodies, escaped, init_observed);
                collect_function_bodies(step, bodies, escaped, init_observed);
                collect_function_bodies(body, bodies, escaped, init_observed);
            }
            IrStmt::Block(b) | IrStmt::Subshell(b) | IrStmt::Background(b) => {
                collect_function_bodies(b, bodies, escaped, init_observed)
            }
            IrStmt::Case { clauses, .. } => {
                for c in clauses {
                    collect_function_bodies(&c.body, bodies, escaped, init_observed);
                }
            }
            IrStmt::Try {
                body,
                excepts,
                else_body,
                finally_body,
            } => {
                collect_function_bodies(body, bodies, escaped, init_observed);
                for e in excepts {
                    collect_function_bodies(&e.body, bodies, escaped, init_observed);
                }
                collect_function_bodies(else_body, bodies, escaped, init_observed);
                collect_function_bodies(finally_body, bodies, escaped, init_observed);
            }
            IrStmt::Pipeline { stages, .. } => {
                for st in stages {
                    collect_function_bodies(st, bodies, escaped, init_observed);
                }
            }
            IrStmt::Redirect { inner, .. } => collect_function_bodies(inner, bodies, escaped, init_observed),
            _ => {}
        }
    }
}

/// One statement list's liveness. `need` starts true: the list's last
/// store is observed by whatever runs next (the process exit for main, the
/// caller's `$?` for a body).
fn body_store_liveness(
    stmts: &[IrStmt],
    bodies: &mut Vec<(String, Option<Vec<bool>>)>,
    escaped: &mut bool,
    init_observed: &mut bool,
    is_function_body: bool,
) -> Option<Vec<bool>> {
    let n = stmts.len();
    if n == 0 {
        return None;
    }
    collect_function_bodies(stmts, bodies, escaped, init_observed);
    let reads: Vec<bool> = stmts
        .iter()
        .map(|s| stmt_reads_status_in(s, false))
        .collect();
    if is_function_body {
        // A body's FIRST read must be preceded by one of its OWN stores.
        // Otherwise it observes the CALLER's status, whose position this
        // body cannot describe — every store in the program must stay.
        if let Some(first) = reads.iter().position(|r| *r) {
            if !stmts[..first].iter().any(stmt_definitely_stores_status) {
                *escaped = true;
            }
        }
    }
    let mut live = vec![false; n];
    let mut need = true;
    for i in (0..n).rev() {
        let stores = stmt_definitely_stores_status(&stmts[i]);
        let later_needs = need;
        if reads[i] {
            // A read inside i observes the PRECEDING store. i's own store
            // is observed only by later reads (a simple statement reads its
            // expression before writing the status) — unless a nested list
            // reads `$?` before its own first store, which observes the
            // status i starts with.
            live[i] = later_needs
                || (!stores && stmt_nested_reads_before_store(&stmts[i]));
            need = true;
        } else {
            live[i] = later_needs;
            if later_needs && stores {
                need = false;
            }
        }
    }
    // A read before this list's first LIVE store observes the register's
    // initial value: the backend must still emit the initialiser.
    if let Some(r) = reads.iter().position(|x| *x) {
        if !live[..r].iter().any(|x| *x) {
            *init_observed = true;
        }
    }
    Some(live)
}

/// The read-before-next-store fact for every body of the program.
pub fn status_liveness(prog: &IrProgram) -> StatusLiveness {
    let mut out = StatusLiveness::default();
    let mut escaped = false;
    let mut init_observed = false;
    out.main = body_store_liveness(
        &prog.stmts,
        &mut out.bodies,
        &mut escaped,
        &mut init_observed,
        false,
    );
    for s in &prog.subs {
        let v = body_store_liveness(
            &s.body,
            &mut out.bodies,
            &mut escaped,
            &mut init_observed,
            true,
        );
        out.bodies.push((s.name.clone(), v));
    }
    // The main list starts with the register's INITIAL value: a read before
    // any of its stores observes it (function bodies are covered above, where
    // the same shape means the CALLER's status instead).
    if list_reads_before_store(&prog.stmts) {
        init_observed = true;
    }
    out.init_observed = init_observed;
    if std::env::var("SH2_RC_DEBUG").is_ok() {
        eprintln!(
            "[rc-liveness] escaped={} main={:?} bodies={:?}",
            escaped,
            out.main,
            out.bodies
                .iter()
                .map(|(n, v)| (n.clone(), v.as_ref().map(|x| x.len())))
                .collect::<Vec<_>>()
        );
    }
    if escaped {
        // A body reads the caller's status before storing one of its own:
        // which store it observes depends on the CALL SITE, so no body can
        // be localised. Every store stays (still correct, just bigger).
        return StatusLiveness::default();
    }
    out
}

/// The main statement list's liveness (see [`status_liveness`]).
pub fn status_store_liveness(prog: &IrProgram) -> Option<Vec<bool>> {
    status_liveness(prog).main
}

/// Liveness for a NESTED statement list (`if`/`case`/loop body) whose
/// enclosing compound statement has already stored a status — those arms
/// clear `$?` before their body runs, so a read inside the list can never
/// observe a store from outside it. The list's own last store stays (the
/// enclosing statement's status is observed after it).
pub fn list_liveness(stmts: &[IrStmt]) -> Option<Vec<bool>> {
    let mut bodies = Vec::new();
    let mut escaped = false;
    let mut init_observed = false;
    body_store_liveness(stmts, &mut bodies, &mut escaped, &mut init_observed, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{AssignTarget, IrProgram, IrStmt, StrStyle};

    fn program(stmts: Vec<IrStmt>) -> IrProgram {
        IrProgram {
            stmts,
            subs: vec![],
            imports: vec![],
            requires: vec![],
            var_types: vec![],
            var_const: vec![],
            var_lifetimes: vec![],
            var_nospace: vec![],
            stmt_lines: vec![],
            var_bash_env: vec![],
            var_lengths: vec![],
            var_storage: vec![],
        }
    }

    fn read_stmt() -> IrStmt {
        IrStmt::Expr(IrExpr::Call {
            func: "test".into(),
            args: vec![IrExpr::Str("-z \"$?\"".into(), StrStyle::DoubleQuoted)],
        })
    }

    fn assign(name: &str) -> IrStmt {
        IrStmt::Assign {
            targets: vec![AssignTarget {
                var: name.into(),
                sigil: None,
                indices: vec![],
            }],
            expr: IrExpr::Int(1),
            asm: None,
        }
    }

    fn echo_hi() -> IrStmt {
        IrStmt::Output {
            value: IrExpr::Str("hi".into(), StrStyle::DoubleQuoted),
            newline: true,
            target: None,
        }
    }

    #[test]
    fn detects_test_operand_status() {
        assert!(program_reads_status(&program(vec![read_stmt()])));
    }

    #[test]
    fn detects_getvar_status() {
        let stmts = vec![IrStmt::Expr(IrExpr::Call {
            func: "getVar".into(),
            args: vec![IrExpr::Str("?".into(), StrStyle::DoubleQuoted)],
        })];
        assert!(program_reads_status(&program(stmts)));
    }

    #[test]
    fn status_free_program_is_false() {
        assert!(!program_reads_status(&program(vec![echo_hi()])));
    }

    #[test]
    fn bare_exit_reads_the_status() {
        assert!(program_reads_status(&program(vec![IrStmt::Exit(None)])));
        assert!(!program_reads_status(&program(vec![IrStmt::Exit(Some(
            IrExpr::Int(0)
        ))])));
    }

    #[test]
    fn stores_before_the_nearest_read_are_dropped() {
        // a=1; echo hi; read   →  only the store the read observes survives
        let prog = program(vec![assign("a"), echo_hi(), read_stmt()]);
        let live = status_store_liveness(&prog).expect("localises");
        assert_eq!(live, vec![false, true, true]);
    }

    #[test]
    fn stores_after_the_last_read_keep_only_the_exit_store() {
        // read; a=1; b=2  →  nothing reads the first statement's store, so
        // only the last store (the process exit code) survives
        let prog = program(vec![read_stmt(), assign("a"), assign("b")]);
        let live = status_store_liveness(&prog).expect("localises");
        assert_eq!(live, vec![false, false, true]);
    }

    #[test]
    fn each_read_keeps_the_store_it_observes() {
        // read; a=1; read  →  the second read observes `a=1`'s store; the
        // first reads the initial 0 (the preamble), so its own store is
        // only the exit code the last statement still needs
        let prog = program(vec![read_stmt(), assign("a"), read_stmt()]);
        let live = status_store_liveness(&prog).expect("localises");
        assert_eq!(live, vec![false, true, true]);
    }

    #[test]
    fn last_statement_store_is_live_without_a_reader() {
        // no read at all: only the exit code's store survives
        assert_eq!(
            status_store_liveness(&program(vec![assign("a"), assign("b")])),
            Some(vec![false, true])
        );
    }

    #[test]
    fn a_function_body_localises_its_own_reads() {
        // cmd; local ec=$? inside the body → the body keeps one store, and
        // the enclosing main list is not armed by that read
        let prog = program(vec![
            IrStmt::Function {
                name: "f".into(),
                params: vec![],
                body: vec![assign("a"), assign("b"), read_stmt()],
                named_blocks: vec![],
            },
            echo_hi(),
            echo_hi(),
        ]);
        let lv = status_liveness(&prog);
        assert_eq!(lv.bodies.len(), 1);
        assert_eq!(lv.bodies[0].1, Some(vec![false, true, true]));
        // main: no read of its own → only the last store (the exit code)
        assert_eq!(lv.main, Some(vec![false, false, true]));
    }

    #[test]
    fn a_sub_body_localises_its_own_reads() {
        let mut prog = program(vec![assign("a"), echo_hi()]);
        prog.subs = vec![crate::ir::IrSub {
            name: "f".into(),
            params: vec![],
            param_defaults: vec![],
            body: vec![assign("a"), read_stmt()],
        }];
        let lv = status_liveness(&prog);
        assert_eq!(lv.bodies, vec![("f".to_string(), Some(vec![true, true]))]);
        assert_eq!(lv.main, Some(vec![false, true]));
    }

    #[test]
    fn a_body_read_of_the_callers_status_keeps_everything() {
        // the read comes BEFORE the body's first store: it observes the
        // caller's status, so no position in the program explains it
        let mut prog = program(vec![assign("a"), echo_hi(), read_stmt()]);
        prog.subs = vec![crate::ir::IrSub {
            name: "f".into(),
            params: vec![],
            param_defaults: vec![],
            body: vec![read_stmt(), assign("a")],
        }];
        let lv = status_liveness(&prog);
        assert!(lv.main.is_none());
        assert!(lv.bodies.iter().all(|(_, v)| v.is_none()));
    }
}
// Tests generated from the Verus manifest's [pins] rows by
// proofs/verus/gen_pin_tests.py -- DO NOT EDIT the generated file by hand.
// There is one test per pin; a pin flips to MAPPED only after its test passes.
#[cfg(test)]
#[path = "status_reads_pin_tests.rs"]
mod pin_tests;
// Tests generated from the Verus manifest's [pins] rows by
// proofs/verus/gen_pin_tests.py -- DO NOT EDIT the generated file by hand.
// There is one test per pin; a pin flips to MAPPED only after its test passes.
#[cfg(test)]
#[path = "status_reads_liveness_pin_tests.rs"]
mod pin_tests_liveness;
