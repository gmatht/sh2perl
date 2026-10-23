//! echo-return-lifting — recognize pure-output "echo a value and return"
//! shell functions and lower them to the value-returning `fnValue`
//! convention (CROSS_BACKEND_RUNTIME.md §8.3).
//!
//! ## Need
//! The runtime polyfills use the bash function convention: positional args
//! in, stdout out (`echo "$result"`). The per-backend adapter must sink
//! stdout + strip the trailing newline to recover the value — the dominant
//! overhead on even the simplest functions (strLen's one-liner body is
//! still ~20× slower than the hand-written runtime, almost all adapter).
//! The runtime already has the value-returning dispatch (`sh2.fnValue`,
//! used by the C frontend): the define-arrow's NATIVE `return value`
//! comes back to the caller without any stdout round-trip.
//!
//! ## The transform
//! 1. **Recognition**: a function whose body is pure (no exec/capture/
//!    redirect/background/subshell/file-write/`$?`), where EVERY path
//!    through the body emits EXACTLY ONE single-arg `echo` (a value echo;
//!    no `-n`/`-e` flags, no multi-word output). A small state machine
//!    over {Need, Done, Dead} tracks each live path's echo count: an If
//!    unions over its arms; a loop whose body never reaches `Done` (an
//!    echo without a return would re-echo per iteration) is allowed; a
//!    bare `return` is only legal AFTER an echo on its path.
//! 2. **Body rewrite**: every value echo becomes `Return(Some(value))`;
//!    the bare `return`s after them are dropped (unreachable).
//! 3. **Call-site rewrite**: every call of a recognized function (the
//!    shIR `exec("f", [args])` shape) is wrapped in an echo of the
//!    `fnValue` result — `exec("echo", [fnValue("f", args)])`. This
//!    preserves BOTH the stdout value (the echo prints `value\n`, exactly
//!    the original's output) and the status (0), so every call position
//!    (statement, chain operand, capture, redirect) is equivalent.
//!
//! ## Guards (REFUSE > GUESS)
//! - Any impure construct anywhere in the body → the function stays as-is.
//! - A path with 0 or 2+ echoes (or a `return` before any echo) → refuse.
//! - The recognized body is provably await-free (no exec/capture), so the
//!   emitted arrow stays on the SYNC path — `fnValue` returns the raw
//!   value, not a Promise.
//!
//! ## Placement
//! Registered in `transforms.rs` (SH2_TRANSFORMS gated). The estree
//! emitter needs no new arms: `IrStmt::Return` renders natively and
//! `Call("fnValue")` renders `sh2.fnValue(...)` (the general call path).

use crate::ir::{AssignTarget, InterpPart, IrExpr, IrStmt, StrStyle};
use std::collections::HashSet;

/// Per-path echo state: no value echo yet / exactly one value echo /
/// path ended (a `return` after the echo).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum EchoState {
    Need,
    Done,
    Dead,
}

/// Apply the transform. Returns whether anything changed.
pub fn transform(stmts: &mut Vec<IrStmt>) -> bool {
    // Pass 1 — recognition, to a FIXPOINT over the call graph's STRONGLY
    // CONNECTED COMPONENTS. A function whose body captures an
    // ALREADY-eligible function (e.g. wcLines' `r=$(strCount ...)`) is
    // itself eligible once the callee is (the capture is then a pure value
    // read — pass 3 collapses it to the bare fnValue). The matchers'
    // MUTUAL recursion (globMatch ↔ ext_alt_match ↔ ext_match) forms an
    // SCC that a single-function fixpoint can never break — neither is
    // eligible on the first pass, so neither ever becomes eligible. The
    // SCC is recognized as a WHOLE: every function in it must pass with
    // the coinductive assumption that captures of functions in the SAME
    // SCC are pure (the recursion is a value read of the same pure
    // functions), plus the eligible set for captures of OTHER SCCs (the
    // condensation is a DAG — the fixpoint terminates). The SCC
    // recognition is the SHARED analysis (`shir_passes::scc` — the same
    // call-graph Tarjan the test-parser work and the glob-matcher lift
    // consume).
    if std::env::var("ECHO_RETURN_DBG").is_ok() {
        let graph = crate::shir_passes::scc::build_call_graph_stmts(stmts);
        eprintln!("[er] graph edges: {:?}", graph);
        for st in stmts.iter() {
            eprintln!("[er] stmt: {:?}", std::mem::discriminant(st));
        }
    }
    let graph = crate::shir_passes::scc::build_call_graph_stmts(stmts);
    let sccs = crate::shir_passes::scc::tarjan_sccs(&graph);
    let mut eligible: HashSet<String> = HashSet::new();
    loop {
        let mut grew = false;
        for scc in &sccs {
            if scc.iter().any(|n| !eligible.contains(n)) {
                let ok = scc.iter().all(|name| {
                    let body = function_body(stmts, name);
                    let scc_set: HashSet<String> = scc.iter().cloned().collect();
                    body.map(|b| body_echo_ok(b, &eligible, &scc_set)).unwrap_or(false)
                });
                if ok {
                    for n in scc {
                        eligible.insert(n.clone());
                    }
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    if eligible.is_empty() {
        return false;
    }
    // Pass 2 — body rewrite: value echoes → Return(Some(value)), the
    // trailing bare returns dropped.
    let mut c = false;
    for st in stmts.iter_mut() {
        if let IrStmt::Function { name, body, .. } = st {
            if eligible.contains(name) {
                c |= rewrite_body(body, &eligible);
            }
        }
    }
    // Pass 3 — call-site rewrite: `exec("f", [args])` of an eligible
    // function → `exec("echo", [fnValue("f", [args])])` (the value +
    // newline the original echo produced).
    // A pipeline's LAST stage writes the pipeline's output to the current
    // fd-1 sink (sh2.pipelineSync restores `saved[1]` for it), so a bare `f`
    // there is not in VALUE position: pass 3 (and rewrite_stmt's bare-call
    // arms) must not collapse it to fnValue. Otherwise `y=$(echo hi | f)`
    // with `f() { echo a+b*; }` captures "" where bash captures the echoed
    // value (measured 2026-10-22) — an echo-return function called as the
    // final pipeline stage is extremely common (every `… | wcLines`-style
    // adapter), so this is a live shape, not a corner.
    //
    // The skip is handled INSIDE rewrite_call_stmt (which knows the stage
    // index) rather than here, so it applies at every nesting depth.
    for st in stmts.iter_mut() {
        c |= rewrite_call_stmt(st, &eligible);
    }
    // Pass 4 — `case` PATTERN captures. A pattern is opaque TEXT to every
    // renderer, so a `$(f)` occupying a WHOLE pattern never reaches the
    // lift above: the call is invisible (the callee is dropped as
    // unreferenced) and the capture shells out to a child that cannot see
    // the shell function → empty → the clause never matches. Hoist a
    // whole-pattern capture of an eligible function into a preceding
    // `__sh2_casepat_N=$(f)` capture-assign (which the normal capture
    // path then lifts to `fnValue`) and point the pattern at the temp.
    // Fires only for eligible (pure, echo-return) callees — a bare
    // `echo`/external pattern is left exactly as-is.
    let mut seq = 0usize;
    c |= hoist_case_pattern_captures(stmts, &eligible, &mut seq);
    c
}

/// Is a statement list "echo-return": every path from the start to the
/// block end (or a `return`) emits EXACTLY one single-arg echo?
/// `scc` is the function's strongly connected component — captures of
/// functions in the SAME scc are pure (the coinductive assumption for
/// the mutual recursion).
fn body_echo_ok(body: &[IrStmt], eligible: &HashSet<String>, scc: &HashSet<String>) -> bool {
    let mut states: HashSet<EchoState> = HashSet::new();
    states.insert(EchoState::Need);
    let Some(final_states) = parse_stmts(body, &states, eligible, scc) else {
        return false;
    };
    // Every live path at the block end must have echoed (Done) or ended
    // (Dead — returned AFTER an echo). A Need path (no echo) would emit
    // nothing — refuse.
    final_states
        .iter()
        .all(|s| matches!(s, EchoState::Done | EchoState::Dead))
}

/// Parse a statement list forward, tracking each live path's echo state.
/// None = some path violates the echo discipline.
fn parse_stmts(
    stmts: &[IrStmt],
    incoming: &HashSet<EchoState>,
    eligible: &HashSet<String>,
    scc: &HashSet<String>,
) -> Option<HashSet<EchoState>> {
    let mut cur = incoming.clone();
    for st in stmts {
        cur = parse_stmt(st, &cur, eligible, scc)?;
        if cur.is_empty() {
            // every path ended (all returned) — the rest is unreachable
            return Some(cur);
        }
    }
    Some(cur)
}

fn parse_stmt(
    st: &IrStmt,
    states: &HashSet<EchoState>,
    eligible: &HashSet<String>,
    scc: &HashSet<String>,
) -> Option<HashSet<EchoState>> {
    match st {
        // A single-arg value echo: Need → Done; Dead paths stay dead
        // (unreachable); a Done path would echo a second value → refuse.
        IrStmt::Expr(IrExpr::Call { func, args, .. })
            if matches!(func.as_str(), "builtin" | "exec")
                && value_echo(args, eligible, scc).is_some() =>
        {
            let mut out = HashSet::new();
            for s in states {
                match s {
                    EchoState::Need => {
                        out.insert(EchoState::Done);
                    }
                    EchoState::Dead => {
                        out.insert(EchoState::Dead);
                    }
                    EchoState::Done => return None,
                }
            }
            Some(out)
        }
        // A bare call to a function in the SAME SCC or an already-eligible
        // function (the matchers' bare-statement recursion —
        // `ext_match "$c" "$p" "$v"; return`): the callee ECHOES its
        // value, so the path's output is the callee's value — an echo
        // (Need → Done). The rewrite turns it into a value return of the
        // fnValue result.
        IrStmt::Expr(IrExpr::Call { func, args, .. })
            if matches!(func.as_str(), "exec" | "fnCall")
                && matches!(args.first(), Some(IrExpr::Str(fname, _))
                    if scc.contains(fname) || eligible.contains(fname)) =>
        {
            let mut out = HashSet::new();
            for s in states {
                match s {
                    EchoState::Need => {
                        out.insert(EchoState::Done);
                    }
                    EchoState::Dead => {
                        out.insert(EchoState::Dead);
                    }
                    EchoState::Done => return None,
                }
            }
            Some(out)
        }
        // A bare `return` — only legal AFTER the path echoed (a no-output
        // return path emits nothing — the value channel would lose it).
        IrStmt::Return(_) => {
            if states.iter().any(|s| *s == EchoState::Need) {
                return None;
            }
            let mut out = HashSet::new();
            out.insert(EchoState::Dead);
            Some(out)
        }
        // Silent statements (pure assignments/decls/loops/ifs) pass
        // through unchanged.
        _ if stmt_silent(st, eligible, scc) => Some(states.clone()),
        // A conditional: every arm must satisfy the discipline; the
        // outcomes union. An empty else falls through (state unchanged).
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            if !expr_pure(cond, eligible, scc) {
                return None;
            }
            let mut out = HashSet::new();
            out.extend(parse_stmts(then, states, eligible, scc)?);
            for (ec, eb) in elsifs {
                if !expr_pure(ec, eligible, scc) {
                    return None;
                }
                out.extend(parse_stmts(eb, states, eligible, scc)?);
            }
            out.extend(parse_stmts(else_, states, eligible, scc)?);
            Some(out)
        }
        // the matchers' `case` dispatch — parse each clause body, union
        // the outcomes (any clause may match at runtime; the last `*)`
        // default always matches)
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            if !expr_pure(discriminant, eligible, scc) {
                return None;
            }
            let mut out = HashSet::new();
            for clause in clauses {
                let r = parse_stmts(&clause.body, states, eligible, scc);
                out.extend(r?);
            }
            Some(out)
        }
        // Loops: the body may not reach `Done` (an echo without a return
        // would re-echo on the next iteration — 2+ echoes). A Need-iteration
        // loops on; a Dead iteration ends the path. 0 iterations → Need
        // (the state passes through).
        IrStmt::While { cond, body } => loop_states(cond, body, states, eligible, scc),
        IrStmt::For { iter, body, .. } => {
            if !expr_pure(iter, eligible, scc) {
                return None;
            }
            loop_body_states(body, states, eligible, scc)
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            if init.iter().any(|s| !stmt_silent(s, eligible, scc))
                || !expr_pure(cond, eligible, scc)
                || step.iter().any(|s| !stmt_silent(s, eligible, scc))
            {
                return None;
            }
            loop_body_states(body, states, eligible, scc)
        }
        _other => None,
    }
}

fn loop_states(
    cond: &IrExpr,
    body: &[IrStmt],
    states: &HashSet<EchoState>,
    eligible: &HashSet<String>,
    scc: &HashSet<String>,
) -> Option<HashSet<EchoState>> {
    if !expr_pure(cond, eligible, scc) {
        return None;
    }
    loop_body_states(body, states, eligible, scc)
}

/// The loop body's per-iteration outcomes must be ⊆ {Need} — an echo
/// without a return would re-echo per iteration (Done), and a return
/// inside a loop body lowers to the runtime RETURN signal inside the
/// loop callback (`sh2.return`) — the VALUE channel is lost there, so
/// the value-returning rewrite refuses it (REFUSE > GUESS). 0 iterations
/// leaves the state untouched.
fn loop_body_states(
    body: &[IrStmt],
    states: &HashSet<EchoState>,
    eligible: &HashSet<String>,
    scc: &HashSet<String>,
) -> Option<HashSet<EchoState>> {
    let iter = parse_stmts(body, states, eligible, scc);
    if iter.is_none() {
        return None;
    }
    let iter = iter.unwrap();
    if iter.contains(&EchoState::Done) || iter.contains(&EchoState::Dead) {
        return None;
    }
    Some(states.clone())
}

/// A single-arg value echo: `builtin("echo", [Array([word])])` /
/// `exec("echo", [Array([word])])` — one word, no `-n`/`-e` flags. The
/// word must be a pure value expression (the returned value).
fn value_echo<'a>(
    args: &'a [IrExpr],
    eligible: &HashSet<String>,
    scc: &HashSet<String>,
) -> Option<&'a IrExpr> {
    let [IrExpr::Str(n, _), IrExpr::Array(words)] = args else {
        return None;
    };
    if n != "echo" {
        return None;
    }
    let [word] = words.as_slice() else {
        return None;
    };
    if !expr_pure(word, eligible, scc) {
        return None;
    }
    Some(word)
}

/// Is the statement side-effect-free beyond the local store (no stdout,
/// no exec/capture/redirect/background/subshell, no status reads)?
fn stmt_silent(st: &IrStmt, eligible: &HashSet<String>, scc: &HashSet<String>) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => expr_pure(expr, eligible, scc),
        IrStmt::Declare { init, .. } => {
            init.as_ref().map(|i| expr_pure(i, eligible, scc)).unwrap_or(true)
        }
        IrStmt::DeclareArray { elements, .. } => {
            elements.iter().all(|e| expr_pure(e, eligible, scc))
        }
        // `local x="$1"` / `declare -i n` / `shift` — the runtime decl
        // builtins with pure value args are silent (function-scoped store
        // writes).
        IrStmt::Expr(IrExpr::Call { func, args, .. })
            if matches!(func.as_str(), "builtin" | "exec") =>
        {
            if let Some(IrExpr::Str(n, _)) = args.first() {
                if matches!(n.as_str(), "local" | "declare" | "typeset" | "readonly" | "shift") {
                    return args.iter().skip(1).all(|a| expr_pure(a, eligible, scc));
                }
                // a call to a function in the SAME SCC (the matchers'
                // mutual recursion) or an already-eligible function — a
                // pure value read
                if scc.contains(n) || eligible.contains(n) {
                    return args.iter().skip(1).all(|a| expr_pure(a, eligible, scc));
                }
            }
            false
        }
        // A `while IFS= read -r line` loop (line_count's shape): the read
        // builtin consumes stdin, emits nothing — silent when the body is.
        IrStmt::While { cond, body } => {
            (is_read_loop(cond) || expr_pure(cond, eligible, scc))
                && body.iter().all(|s| stmt_silent(s, eligible, scc))
        }
        IrStmt::For { iter, body, .. } => {
            expr_pure(iter, eligible, scc)
                && body.iter().all(|s| stmt_silent(s, eligible, scc))
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            expr_pure(cond, eligible, scc)
                && then.iter().all(|s| stmt_silent(s, eligible, scc))
                && elsifs.iter().all(|(c, b)| {
                    expr_pure(c, eligible, scc)
                        && b.iter().all(|s| stmt_silent(s, eligible, scc))
                })
                && else_.iter().all(|s| stmt_silent(s, eligible, scc))
        }
        IrStmt::Block(body) => body.iter().all(|s| stmt_silent(s, eligible, scc)),
        // a `break` (the loop-return-lift transform's flag+break) exits
        // the loop — no output, silent
        IrStmt::Break => true,
        // A herestring/heredoc redirect feeding a silent inner (line_count's
        // `while read ... <<< "$s"`): the redirect is a pure input source.
        IrStmt::Redirect { inner, redirects } => {
            redirects.iter().all(|r| {
                matches!(r.mode.as_str(), "herestring" | "heredoc")
                    && expr_pure(&r.target, eligible, scc)
            }) && inner.iter().all(|s| stmt_silent(s, eligible, scc))
        }
        _ => false,
    }
}

/// Is the cond a `while read` loop guard (`exec("read", ...)` /
/// `builtin("read", ...)` with pure args)? The read builtin consumes
/// stdin and emits nothing — a silent loop condition.
fn is_read_loop(cond: &IrExpr) -> bool {
    match cond {
        IrExpr::Call { func, args } if matches!(func.as_str(), "exec" | "builtin") => {
            matches!(args.first(), Some(IrExpr::Str(n, _)) if n == "read")
        }
        _ => false,
    }
}

/// Is the expression free of side effects and status reads? Refuses
/// exec/capture/pipeline/background/subshell/file-write/`$?`; the pure
/// sh2.* namespace calls (param/join/contains/strLen/…) and the local
/// store reads (getVar) pass.
fn expr_pure(e: &IrExpr, eligible: &HashSet<String>, scc: &HashSet<String>) -> bool {
    match e {
        // A capture of an ALREADY-eligible function is a pure value read
        // (pass 3 collapses it to the bare fnValue) — the fixpoint lets
        // wcLines' `r=$(strCount ...)` through once strCount is eligible.
        // A capture of a function in the SAME SCC (the matchers' mutual
        // recursion — globMatch calls `$(ext_alt_match ...)` and vice
        // versa) is pure too: the recursion is a value read of the same
        // pure functions (the coinductive assumption — the SCC is
        // recognized as a whole). The callee may be a bare Call (the
        // direct_calls collapse) or an Arrow wrapping a single call
        // statement (a loop-bearing callee like strCount stays an Arrow).
        IrExpr::Capture { expr, .. } => match expr.as_ref() {
            IrExpr::Call { func, args } if matches!(func.as_str(), "exec" | "fnCall") => {
                matches!(args.as_slice(), [IrExpr::Str(fname, _), IrExpr::Array(_)]
                    if scc.contains(fname) || eligible.contains(fname))
            }
            IrExpr::Arrow(stmts) => matches!(
                stmts.as_slice(),
                [IrStmt::Expr(IrExpr::Call { func, args })]
                    if matches!(func.as_str(), "exec" | "fnCall")
                        && matches!(args.as_slice(), [IrExpr::Str(fname, _), IrExpr::Array(_)]
                            if scc.contains(fname) || eligible.contains(fname))
            ),
            _ => false,
        },
        IrExpr::RawExpr(_) => false,
        IrExpr::Var(n, _) => n != "?",
        IrExpr::Str(s, _) => !s.contains("$?"),
        IrExpr::Call { func, args } => {
            if matches!(
                func.as_str(),
                "exec" | "builtin" | "pipeline" | "capture" | "captureSync" | "captureWords"
                    | "captureWordsSync" | "background" | "subshell" | "exit" | "fnCall"
                    | "redirect"
            ) {
                // a PURE `[[ ]]` / `(( ))` test/let cond — `test "$x" == y`
                // or `let "i < N"` — is a status query, not a side effect
                // (the arith/test text is a pure string). Only the exact
                // cond shapes: `test`/`let`/`[`/`[[`/`:` as the name arg.
                if matches!(func.as_str(), "builtin" | "exec") {
                    if let Some(IrExpr::Str(n, _)) = args.first() {
                        if matches!(n.as_str(), "test" | "let" | "[" | "[[" | ":" | "true" | "false") {
                            return args.iter().skip(1).all(|a| expr_pure(a, eligible, scc));
                        }
                    }
                }
                return false;
            }
            // the `$?` status read
            if func == "getVar" && matches!(args.as_slice(), [IrExpr::Str(n, _)] if n == "?") {
                return false;
            }
            args.iter().all(|a| expr_pure(a, eligible, scc))
        }
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            InterpPart::Lit(s) => !s.contains("$?"),
            InterpPart::Expr(ie) => expr_pure(ie, eligible, scc),
        }),
        IrExpr::BinOp { lhs, rhs, .. } => {
            expr_pure(lhs, eligible, scc) && expr_pure(rhs, eligible, scc)
        }
        IrExpr::MethodCall { obj, args, .. } => {
            expr_pure(obj, eligible, scc)
                && args.iter().all(|a| expr_pure(a, eligible, scc))
        }
        IrExpr::Index { key, .. } => expr_pure(key, eligible, scc),
        IrExpr::DefinedOr { expr, default } => {
            expr_pure(expr, eligible, scc) && expr_pure(default, eligible, scc)
        }
        IrExpr::Array(items) => items.iter().all(|i| expr_pure(i, eligible, scc)),
        IrExpr::Arrow(stmts) => stmts.iter().all(|s| stmt_silent(s, eligible, scc)),
        IrExpr::Lambda { body, .. } => body.iter().all(|s| stmt_silent(s, eligible, scc)),
        // literals, arith, ranges, bools, idents, objects, json — pure
        _ => true,
    }
}

/// Rewrite an eligible function body: value echoes → `Return(Some(v))`,
/// trailing bare returns dropped; recurse into nested containers.
fn rewrite_body(body: &mut Vec<IrStmt>, eligible: &HashSet<String>) -> bool {
    let mut changed = false;
    let mut out: Vec<IrStmt> = Vec::with_capacity(body.len());
    for st in body.drain(..) {
        // A pipeline's LAST stage writes the pipeline's output to the
        // current fd-1 sink — a bare `f` there is not in value position, so
        // it must keep its echo (see the pass-3 arm of rewrite_call_stmt:
        // collapsing it makes `y=$(echo hi | f)` capture "").
        let keep_pipeline_last = is_pipeline_with_last_call(&st, eligible);
        out.push(rewrite_stmt(&st, &mut changed, eligible, keep_pipeline_last));
    }
    *body = out;
    changed
}

/// Is `st` a pipeline whose LAST stage is a bare call to an eligible
/// function (the shape whose echo must survive)?
fn is_pipeline_with_last_call(st: &IrStmt, eligible: &HashSet<String>) -> bool {
    let IrStmt::Pipeline { stages, .. } = st else {
        return false;
    };
    let Some(last) = stages.last() else {
        return false;
    };
    matches!(
        last.as_slice(),
        [IrStmt::Expr(IrExpr::Call { func, args })]
            if matches!(func.as_str(), "exec" | "fnCall")
                && matches!(args.first(), Some(IrExpr::Str(n, _)) if eligible.contains(n))
    )
}

/// Pipeline-stage twin of [`echo_pipeline_last_stage`] for the shell
/// path's encoding: `a | b` lowers to
/// `pipeline([Arrow([exec(a)]), Arrow([exec(b)])])`, so the stage is an
/// `Arrow` wrapping the bare call statement.
fn echo_pipeline_last_expr(stage: &mut IrExpr, eligible: &HashSet<String>) -> bool {
    match stage {
        IrExpr::Arrow(body) => {
            let replacement = match body.as_slice() {
                [IrStmt::Expr(IrExpr::Call { func, args })]
                    if eligible_fn_call(func, args, eligible) =>
                {
                    Some(IrStmt::Expr(IrExpr::Call {
                        func: "exec".to_string(),
                        args: echo_of_fn_value(args),
                    }))
                }
                _ => None,
            };
            match replacement {
                Some(st) => {
                    body[0] = st;
                    true
                }
                None => false,
            }
        }
        _ => rewrite_expr_calls(stage, eligible),
    }
}

/// A pipeline's LAST stage writes the pipeline's output through the
/// current fd-1 sink, so a bare call there is NOT in value position. The
/// callee has been rewritten to a value `return`, and the emitter lowers a
/// bare call to a value-returning `fnCall` that writes nothing — so the
/// stage must ECHO the value: rewrite it to
/// `exec("echo", [fnValue("f", [args…])])`, exactly the shape pass 3 uses
/// for every other call position. (`y=$(echo hi | f)` / `y=$(f | cat)`
/// captured "" without this; bash captures the callee's echoed value.)
fn echo_pipeline_last_stage(stage: &mut Vec<IrStmt>, eligible: &HashSet<String>) -> bool {
    let is_last_call = matches!(
        stage.as_slice(),
        [IrStmt::Expr(IrExpr::Call { func, args })]
            if eligible_fn_call(func, args, eligible)
    );
    if !is_last_call {
        return false;
    }
    let IrStmt::Expr(IrExpr::Call { args, .. }) = &stage[0] else {
        unreachable!("checked by is_last_call");
    };
    let echo_args = echo_of_fn_value(args);
    stage[0] = IrStmt::Expr(IrExpr::Call {
        func: "exec".to_string(),
        args: echo_args,
    });
    true
}

/// Rewrite one statement: a value echo → Return(Some(word)); a bare
/// return → dropped (unreachable after the echo-return); otherwise recurse
/// into nested containers.
fn rewrite_stmt(
    st: &IrStmt,
    changed: &mut bool,
    eligible: &HashSet<String>,
    keep_pipeline_last: bool,
) -> IrStmt {
    match st {
        IrStmt::Expr(IrExpr::Call { func, args, .. })
            if matches!(func.as_str(), "builtin" | "exec") =>
        {
            if let Some(word) = value_echo(args, eligible, &HashSet::new()) {
                *changed = true;
                return IrStmt::Return(Some(word.clone()));
            }
            // a bare call to an eligible function (the matchers' bare
            // recursion — `ext_match "$c" "$p" "$v"` inside globMatch):
            // the callee's echo becomes a value return of the fnValue
            // result. MUST be checked here — this arm matches every
            // exec/builtin call, so a separate arm below would be
            // shadowed (the old code returned `st.clone()` for every
            // non-echo exec call, leaving the bare call for pass 3 to
            // wrap in an ECHO — printing the value instead of returning
            // it, the extra-`0` self-test regression).
            if !keep_pipeline_last {
                if let Some(IrExpr::Str(fname, _)) = args.first() {
                    if eligible.contains(fname) {
                        *changed = true;
                        return IrStmt::Return(Some(fn_value_call(args)));
                    }
                }
            }
            st.clone()
        }
        // a bare call to an eligible function (the matchers' bare
        // recursion) — the callee's echo becomes a value return of the
        // fnValue result
        IrStmt::Expr(IrExpr::Call { func, args, .. })
            if matches!(func.as_str(), "exec" | "fnCall") =>
        {
            if !keep_pipeline_last {
                if let Some(IrExpr::Str(fname, _)) = args.first() {
                    if eligible.contains(fname) {
                        *changed = true;
                        return IrStmt::Return(Some(fn_value_call(args)));
                    }
                }
            }
            st.clone()
        }
        // the bare `return` after an echo-return is unreachable
        IrStmt::Return(None) => {
            *changed = true;
            IrStmt::Block(Vec::new())
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut t: Vec<IrStmt> = Vec::with_capacity(then.len());
            for s in then {
                t.push(rewrite_stmt(s, changed, eligible, false));
            }
            let mut es: Vec<(IrExpr, Vec<IrStmt>)> = Vec::with_capacity(elsifs.len());
            for (c, b) in elsifs {
                let mut nb: Vec<IrStmt> = Vec::with_capacity(b.len());
                for s in b {
                    nb.push(rewrite_stmt(s, changed, eligible, false));
                }
                es.push((c.clone(), nb));
            }
            let mut el: Vec<IrStmt> = Vec::with_capacity(else_.len());
            for s in else_ {
                el.push(rewrite_stmt(s, changed, eligible, false));
            }
            IrStmt::If {
                cond: cond.clone(),
                then: t,
                elsifs: es,
                else_: el,
            }
        }
        IrStmt::While { cond, body } => {
            let mut b: Vec<IrStmt> = Vec::with_capacity(body.len());
            for s in body {
                b.push(rewrite_stmt(s, changed, eligible, false));
            }
            IrStmt::While {
                cond: cond.clone(),
                body: b,
            }
        }
        IrStmt::For { var, iter, body } => {
            let mut b: Vec<IrStmt> = Vec::with_capacity(body.len());
            for s in body {
                b.push(rewrite_stmt(s, changed, eligible, false));
            }
            IrStmt::For {
                var: var.clone(),
                iter: iter.clone(),
                body: b,
            }
        }
        // the matchers' echoes live inside `case` clause bodies
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut new_clauses: Vec<crate::ir::IrCaseClause> = Vec::with_capacity(clauses.len());
            for clause in clauses {
                let mut b: Vec<IrStmt> = Vec::with_capacity(clause.body.len());
                for s in &clause.body {
                    b.push(rewrite_stmt(s, changed, eligible, false));
                }
                new_clauses.push(crate::ir::IrCaseClause {
                    patterns: clause.patterns.clone(),
                    body: b,
                });
            }
            IrStmt::Case {
                discriminant: discriminant.clone(),
                clauses: new_clauses,
            }
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            let mut b: Vec<IrStmt> = Vec::with_capacity(body.len());
            for s in body {
                b.push(rewrite_stmt(s, changed, eligible, false));
            }
            IrStmt::ForInit {
                init: init.clone(),
                cond: cond.clone(),
                step: step.clone(),
                body: b,
            }
        }
        IrStmt::Block(body) => {
            let mut b: Vec<IrStmt> = Vec::with_capacity(body.len());
            for s in body {
                b.push(rewrite_stmt(s, changed, eligible, false));
            }
            IrStmt::Block(b)
        }
        other => other.clone(),
    }
}

/// Pass 3 — rewrite call sites of eligible functions: every
/// `exec("f", [args])` where f ∈ eligible becomes
/// `exec("echo", [fnValue("f", [args])])` — the value + newline the
/// function's echo produced, and the echo's status (0) == the original
/// function's status (0).
fn rewrite_call_stmt(st: &mut IrStmt, eligible: &HashSet<String>) -> bool {
    match st {
        IrStmt::Expr(IrExpr::Call { func, args, .. })
            if func == "exec" || func == "fnCall" =>
        {
            // `exec("f", [args])` — a shell-function call of an eligible
            // function. (fnCall is the A1 form of the same; exec is the
            // shell path's shape.)
            if let [IrExpr::Str(fname, _), IrExpr::Array(call_args)] = args.as_slice() {
                if eligible.contains(fname) {
                    // the fnValue call: fnValue("f", [args...])
                    let fnv = IrExpr::Call {
                        func: "fnValue".to_string(),
                        args: vec![
                            IrExpr::Str(fname.clone(), StrStyle::DoubleQuoted),
                            IrExpr::Array(call_args.clone()),
                        ],
                    };
                    // exec("echo", [fnValue...]) — prints value + \n
                    *args = vec![
                        IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
                        IrExpr::Array(vec![fnv]),
                    ];
                    return true;
                }
            }
            false
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            let mut c = rewrite_expr_calls(cond, eligible);
            for s in then.iter_mut() {
                c |= rewrite_call_stmt(s, eligible);
            }
            for (ec, eb) in elsifs.iter_mut() {
                c |= rewrite_expr_calls(ec, eligible);
                for s in eb.iter_mut() {
                    c |= rewrite_call_stmt(s, eligible);
                }
            }
            for s in else_.iter_mut() {
                c |= rewrite_call_stmt(s, eligible);
            }
            c
        }
        IrStmt::While { cond, body } | IrStmt::For { iter: cond, body, .. } => {
            let mut c = rewrite_expr_calls(cond, eligible);
            for s in body.iter_mut() {
                c |= rewrite_call_stmt(s, eligible);
            }
            c
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            let mut c = rewrite_expr_calls(cond, eligible);
            for s in init.iter_mut().chain(step.iter_mut()) {
                c |= rewrite_call_stmt(s, eligible);
            }
            for s in body.iter_mut() {
                c |= rewrite_call_stmt(s, eligible);
            }
            c
        }
        IrStmt::Block(body) | IrStmt::Subshell(body) | IrStmt::Background(body) => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= rewrite_call_stmt(s, eligible);
            }
            c
        }
        IrStmt::Redirect { inner, .. } => {
            let mut c = false;
            for s in inner.iter_mut() {
                c |= rewrite_call_stmt(s, eligible);
            }
            c
        }
        IrStmt::Pipeline { stages, .. } => {
            let mut c = false;
            // Last stage = the pipeline's OUTPUT sink: see the pass-3 note.
            let last = stages.len().saturating_sub(1);
            for (i, stage) in stages.iter_mut().enumerate() {
                if i == last {
                    c |= echo_pipeline_last_stage(stage, eligible);
                    continue;
                }
                for s in stage.iter_mut() {
                    c |= rewrite_call_stmt(s, eligible);
                }
            }
            c
        }
        // the param dispatcher's `case` — descend into the clause bodies
        IrStmt::Case {
            discriminant,
            clauses,
        } => {
            let mut c = rewrite_expr_calls(discriminant, eligible);
            for clause in clauses.iter_mut() {
                for s in clause.body.iter_mut() {
                    c |= rewrite_call_stmt(s, eligible);
                }
            }
            c
        }
        // descend into function bodies (the param/globMatch dispatchers'
        // captures of now-eligible primitives)
        IrStmt::Function { body, .. } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= rewrite_call_stmt(s, eligible);
            }
            c
        }
        // assignments/captures/decls/outputs carry expressions (the
        // param dispatcher's `r=$(strReplaceAll ...)` capture-assign)
        IrStmt::Assign { targets, expr, .. } => {
            let mut c = rewrite_expr_calls(expr, eligible);
            for t in targets.iter_mut() {
                for idx in t.indices.iter_mut() {
                    c |= rewrite_expr_calls(idx, eligible);
                }
            }
            c
        }
        IrStmt::Declare { init, .. } => init
            .as_mut()
            .map(|i| rewrite_expr_calls(i, eligible))
            .unwrap_or(false),
        IrStmt::DeclareArray { elements, .. } => {
            let mut c = false;
            for el in elements.iter_mut() {
                c |= rewrite_expr_calls(el, eligible);
            }
            c
        }
        IrStmt::Output { value, .. } => rewrite_expr_calls(value, eligible),
        IrStmt::Return(Some(v)) => rewrite_expr_calls(v, eligible),
        // a `pipeline([...])` expression statement — the capture-assign
        // path reaches its stages through the Capture/Arrow recursion, but
        // a pipeline in statement position arrives here.
        IrStmt::Expr(e) => {
            if matches!(e, IrExpr::Call { func, .. } if func == "pipeline") {
                rewrite_expr_calls(e, eligible)
            } else {
                false
            }
        }
        _ => false,
    }
}

/// The pure-CPU sh2.* namespace ops (the runtime/ polyfill inventory) —
/// these are IR ops the backends render natively, NOT user functions. A
/// user-defined function with one of these names (polyfills.sh defines
/// param/contains/globMatch/…) must not be confused with the op:
/// collapsing `param("len", "v")` (the `${#v}` op) to
/// `fnValue("param", ["len", "v"])` makes the param function's OWN
/// `${#v}` a recursive call (stack overflow).
const SH2_OPS: &[&str] = &[
    "param", "join", "contains", "strLen", "strHasPrefix", "strHasSuffix",
    "strSlice", "strCompare", "strIndex", "strLastIndex", "strCount",
    "strReplaceAll", "strContainsAny", "globMatch", "caseMatch", "brace",
    "basename", "dirname", "test", "line_count", "line_at", "wcLines",
    "headLines", "tailLines", "ext_alt_match", "ext_match", "joinSep",
    "split", "arrayIndex", "arrayLen", "arrayItems", "listVar", "assocGet",
    "getVar", "setVar", "arith",
];

/// Rewrite `exec("f", ...)` / `fnCall("f", ...)` calls inside an
/// expression (capture bodies, and/or chains, ternary conds, …). A
/// CAPTURE of an eligible function's call collapses to the bare `fnValue`
/// result (the value needs no stdout round-trip; the capture's trailing-
/// newline strip is a no-op on the raw value).
fn rewrite_expr_calls(e: &mut IrExpr, eligible: &HashSet<String>) -> bool {
    // `$(f args)` — a capture of an eligible function call: the value
    // channel is the fnValue result directly (the capture would wrap an
    // echo of it and strip the newline — identical, minus the round-trip).
    if let IrExpr::Capture { expr, .. } = e {
        // `$(f args)` — a capture of an eligible function call: the value
        // channel is the fnValue result directly (the capture would wrap
        // an echo of it and strip the newline — identical, minus the
        // round-trip). The capture may be a direct Call (the direct-calls
        // collapse) or an Arrow wrapping a single exec (case/loop-bodied
        // fns direct-calls refuses — the matchers).
        let call = match expr.as_ref() {
            IrExpr::Call { func, args } => Some((func.as_str(), args)),
            IrExpr::Arrow(body) => match body.as_slice() {
                [IrStmt::Expr(IrExpr::Call { func, args })]
                    if func == "exec" || func == "fnCall" =>
                {
                    Some((func.as_str(), args))
                }
                _ => None,
            },
            _ => None,
        };
        if let Some((func, args)) = call {
            let eligible_call = if matches!(func, "exec" | "fnCall") {
                matches!(args.as_slice(), [IrExpr::Str(fname, _), IrExpr::Array(_)]
                    if eligible.contains(fname))
            } else {
                eligible.contains(func)
            };
            if eligible_call {
                let (fname, call_args) = if matches!(func, "exec" | "fnCall") {
                    let [IrExpr::Str(fname, _), IrExpr::Array(call_args)] = args.as_slice() else {
                        unreachable!("checked by eligible_call")
                    };
                    (fname.clone(), call_args.clone())
                } else {
                    (func.to_string(), args.clone())
                };
                *e = IrExpr::Call {
                    func: "fnValue".to_string(),
                    args: vec![
                        IrExpr::Str(fname, StrStyle::DoubleQuoted),
                        IrExpr::Array(call_args),
                    ],
                };
                return true;
            }
        }
        return rewrite_expr_calls(expr, eligible);
    }
    match e {
        // A shell pipeline lowered as `pipeline([Arrow(…), Arrow(…), …])`.
        // The LAST stage writes the pipeline's output through the current
        // fd-1 sink, so an eligible call there is not in value position —
        // it must ECHO the fnValue result (see echo_pipeline_last_stage).
        IrExpr::Call { func, args } if func == "pipeline" => {
            let mut c = false;
            if let Some(IrExpr::Array(stages)) = args.get_mut(0) {
                let last = stages.len().saturating_sub(1);
                for (i, stage) in stages.iter_mut().enumerate() {
                    if i == last {
                        c |= echo_pipeline_last_expr(stage, eligible);
                    } else {
                        c |= rewrite_expr_calls(stage, eligible);
                    }
                }
            }
            c
        }
        IrExpr::Call { func, args } if matches!(func.as_str(), "exec" | "fnCall") => {
            if eligible_fn_call(func, args, eligible) {
                *args = echo_of_fn_value(args);
                return true;
            }
            false
        }
        IrExpr::Call { func, args } => {
            // a DIRECT call of an eligible function (the parameter-
            // expansion lowering / direct-calls collapse for non-eligible
            // callers — e.g. brace/test's `${s%/*}` → `param('%', s, "/")`):
            // the value channel is the fnValue result. The pure-CPU sh2.*
            // namespace ops are NOT user functions — they render natively
            // per-backend, so they must NOT be collapsed to fnValue (when
            // the polyfill defines a function with the same name — e.g.
            // `param` — the param function's OWN `${#v}` would become a
            // recursive call).
            if eligible.contains(func.as_str()) && !SH2_OPS.contains(&func.as_str()) {
                *e = IrExpr::Call {
                    func: "fnValue".to_string(),
                    args: vec![
                        IrExpr::Str(func.clone(), StrStyle::DoubleQuoted),
                        IrExpr::Array(args.clone()),
                    ],
                };
                return true;
            }
            let mut c = false;
            for a in args.iter_mut() {
                c |= rewrite_expr_calls(a, eligible);
            }
            // the func name is never a call site
            let _ = func;
            c
        }
        IrExpr::BinOp { lhs, rhs, .. } => {
            let l = rewrite_expr_calls(lhs, eligible);
            let r = rewrite_expr_calls(rhs, eligible);
            l | r
        }
        IrExpr::Interpolate(parts) => {
            let mut c = false;
            for p in parts.iter_mut() {
                if let InterpPart::Expr(ie) = p {
                    c |= rewrite_expr_calls(ie, eligible);
                }
            }
            c
        }
        IrExpr::Array(items) => {
            let mut c = false;
            for it in items.iter_mut() {
                c |= rewrite_expr_calls(it, eligible);
            }
            c
        }
        IrExpr::Ternary {
            cond, then, else_, ..
        } => {
            let a = rewrite_expr_calls(cond, eligible);
            let b = rewrite_expr_calls(then, eligible);
            let c = rewrite_expr_calls(else_, eligible);
            a | b | c
        }
        IrExpr::DefinedOr { expr, default } => {
            let a = rewrite_expr_calls(expr, eligible);
            let b = rewrite_expr_calls(default, eligible);
            a | b
        }
        IrExpr::Arrow(stmts) => {
            let mut c = false;
            for s in stmts.iter_mut() {
                c |= rewrite_call_stmt(s, eligible);
            }
            c
        }
        IrExpr::Lambda { params, body } => {
            let mut c = false;
            for s in body.iter_mut() {
                c |= rewrite_call_stmt(s, eligible);
            }
            let _ = params;
            c
        }
        _ => false,
    }
}

/// Is `args` the `["f", [callargs...]]` shape with f ∈ eligible?
fn eligible_fn_call(
    func: &str,
    args: &[IrExpr],
    eligible: &HashSet<String>,
) -> bool {
    matches!(func, "exec" | "fnCall")
        && matches!(args, [IrExpr::Str(fname, _), IrExpr::Array(_)]
            if eligible.contains(fname))
}

/// `fnValue("f", [args...])`.
fn fn_value_call(args: &[IrExpr]) -> IrExpr {
    let [IrExpr::Str(fname, _), IrExpr::Array(call_args)] = args else {
        unreachable!("checked by eligible_fn_call");
    };
    IrExpr::Call {
        func: "fnValue".to_string(),
        args: vec![
            IrExpr::Str(fname.clone(), StrStyle::DoubleQuoted),
            IrExpr::Array(call_args.clone()),
        ],
    }
}

/// `exec("echo", [fnValue("f", [args...])])` — the value + newline the
/// function's echo produced.
fn echo_of_fn_value(args: &[IrExpr]) -> Vec<IrExpr> {
    vec![
        IrExpr::Str("echo".to_string(), StrStyle::DoubleQuoted),
        IrExpr::Array(vec![fn_value_call(args)]),
    ]
}

fn function_body<'a>(stmts: &'a [IrStmt], name: &str) -> Option<&'a [IrStmt]> {
    for st in stmts {
        if let IrStmt::Function { name: n, body, .. } = st {
            if n == name {
                return Some(body);
            }
        }
    }
    None
}

/// Hoist whole-pattern `$(f)` / `` `f` `` captures out of `case` clause
/// patterns. For each such pattern whose command is a SINGLE call to an
/// ELIGIBLE function, insert a `__sh2_casepat_N = fnValue("f", […])`
/// statement before the `case` and rewrite the pattern to the temp-var
/// read. Recurses into every statement container. Returns changed.
fn hoist_case_pattern_captures(
    stmts: &mut Vec<IrStmt>,
    eligible: &HashSet<String>,
    seq: &mut usize,
) -> bool {
    let mut changed = false;
    let mut out: Vec<IrStmt> = Vec::with_capacity(stmts.len());
    for mut st in stmts.drain(..) {
        // Scope the borrow of `st` so it can be moved into `out` after.
        let mut hoists: Vec<IrStmt> = Vec::new();
        {
            if let IrStmt::Case { clauses, .. } = &mut st {
                for clause in clauses.iter_mut() {
                    changed |= hoist_case_pattern_captures(&mut clause.body, eligible, seq);
                    let mut new_pats: Vec<String> = Vec::with_capacity(clause.patterns.len());
                    for p in clause.patterns.drain(..) {
                        match lift_pattern_capture(&p, eligible) {
                            Some((fname, call_args, quoted)) => {
                                let var = format!("__sh2_casepat_{seq}");
                                *seq += 1;
                                hoists.push(fnvalue_assign(&var, &fname, &call_args));
                                new_pats.push(if quoted {
                                    format!("\"${var}\"")
                                } else {
                                    format!("${var}")
                                });
                                changed = true;
                            }
                            None => new_pats.push(p),
                        }
                    }
                    clause.patterns = new_pats;
                }
            } else {
                changed |= rebuild_children(&mut st, eligible, seq);
            }
            // A pipeline's LAST stage writes the pipeline's output through
            // the current fd-1 sink — a bare `f` there is not in value
            // position, so the pass-3 collapse (`exec echo fnValue`) must
            // not reach it either, from this recursion or from pass 3.
            if is_pipeline_with_last_call(&st, eligible) {
                if let IrStmt::Pipeline { stages, .. } = &mut st {
                    if let Some(last) = stages.last_mut() {
                        last.retain(|s| {
                            !matches!(
                                s,
                                IrStmt::Expr(IrExpr::Call { func, args, .. })
                                    if matches!(func.as_str(), "exec" | "fnCall")
                                        && matches!(args.first(), Some(IrExpr::Str(n, _))
                                            if eligible.contains(n))
                            )
                        });
                    }
                }
            }
        }
        out.extend(hoists);
        out.push(st);
    }
    *stmts = out;
    changed
}

/// Recurse into the statement-bearing children of a non-`case` statement.
fn rebuild_children(st: &mut IrStmt, eligible: &HashSet<String>, seq: &mut usize) -> bool {
    let mut changed = false;
    match st {
        IrStmt::If {
            then,
            elsifs,
            else_,
            ..
        } => {
            changed |= hoist_case_pattern_captures(then, eligible, seq);
            for (_, b) in elsifs.iter_mut() {
                changed |= hoist_case_pattern_captures(b, eligible, seq);
            }
            changed |= hoist_case_pattern_captures(else_, eligible, seq);
        }
        IrStmt::While { body, .. }
        | IrStmt::DoWhile { body, .. }
        | IrStmt::For { body, .. }
        | IrStmt::Block(body)
        | IrStmt::Subshell(body)
        | IrStmt::Background(body) => {
            changed |= hoist_case_pattern_captures(body, eligible, seq);
        }
        IrStmt::ForInit {
            init,
            step,
            body,
            ..
        } => {
            changed |= hoist_case_pattern_captures(init, eligible, seq);
            changed |= hoist_case_pattern_captures(step, eligible, seq);
            changed |= hoist_case_pattern_captures(body, eligible, seq);
        }
        IrStmt::Redirect { inner, .. } => {
            changed |= hoist_case_pattern_captures(inner, eligible, seq);
        }
        IrStmt::Pipeline { stages, .. } => {
            for s in stages.iter_mut() {
                changed |= hoist_case_pattern_captures(s, eligible, seq);
            }
        }
        IrStmt::Function {
            body,
            named_blocks,
            ..
        } => {
            changed |= hoist_case_pattern_captures(body, eligible, seq);
            for (_, b) in named_blocks.iter_mut() {
                changed |= hoist_case_pattern_captures(b, eligible, seq);
            }
        }
        IrStmt::Try {
            body,
            excepts,
            else_body,
            finally_body,
        } => {
            changed |= hoist_case_pattern_captures(body, eligible, seq);
            for e in excepts.iter_mut() {
                changed |= hoist_case_pattern_captures(&mut e.body, eligible, seq);
            }
            changed |= hoist_case_pattern_captures(else_body, eligible, seq);
            changed |= hoist_case_pattern_captures(finally_body, eligible, seq);
        }
        _ => {}
    }
    changed
}

/// `Some((fname, args, quoted))` iff `pat` is EXACTLY a whole-pattern
/// command substitution `$(f args…)` / `` `f args…` `` (optionally wrapped
/// in one matched quote pair) whose command is a single call to an
/// ELIGIBLE function. Anything else (glob text around the substitution,
/// a builtin/external command, an ineligible callee) returns `None`.
fn lift_pattern_capture(
    pat: &str,
    eligible: &HashSet<String>,
) -> Option<(String, Vec<IrExpr>, bool)> {
    let t = pat.trim();
    let (inner, quoted) = strip_one_quote_pair(t);
    let body = whole_subst_body(inner.trim())?;
    let cmds = crate::parser::commands::Parser::new(body).parse().ok()?;
    let stmts: Vec<IrStmt> = cmds
        .iter()
        .filter_map(crate::shir::stmt_for_command)
        .collect();
    let args = match stmts.as_slice() {
        [IrStmt::Expr(IrExpr::Call { func, args })] if matches!(func.as_str(), "exec" | "fnCall") => {
            args
        }
        _ => return None,
    };
    let [IrExpr::Str(fname, _), IrExpr::Array(call_args)] = args.as_slice() else {
        return None;
    };
    if !eligible.contains(fname) {
        return None;
    }
    Some((fname.clone(), call_args.clone(), quoted))
}

/// Strip exactly one matched surrounding quote pair (double or single).
fn strip_one_quote_pair(t: &str) -> (&str, bool) {
    let b = t.as_bytes();
    if b.len() >= 2
        && ((b[0] == b'"' && b[b.len() - 1] == b'"')
            || (b[0] == b'\'' && b[b.len() - 1] == b'\''))
    {
        (&t[1..t.len() - 1], true)
    } else {
        (t, false)
    }
}

/// The body of a WHOLE `$( … )` / `` ` … ` `` substitution, or `None` when
/// `s` is not one (glob text around it, unbalanced parens). Mirrors the
/// renderer-side `case_subst_value` balanced scan (quote/escape aware).
fn whole_subst_body(s: &str) -> Option<&str> {
    if s.strip_prefix("$(").is_some() {
        let b = s.as_bytes();
        let mut depth = 0i32;
        let mut in_s = false;
        let mut in_d = false;
        let mut esc = false;
        let mut i = 0;
        while i < b.len() {
            let c = b[i] as char;
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if in_s {
                if c == '\'' {
                    in_s = false;
                }
            } else if in_d {
                if c == '"' {
                    in_d = false;
                }
            } else if c == '\'' {
                in_s = true;
            } else if c == '"' {
                in_d = true;
            } else if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
                if depth == 0 {
                    // whole iff the matching close is the final byte
                    return if i == b.len() - 1 {
                        Some(&s[2..i])
                    } else {
                        None
                    };
                }
            }
            i += 1;
        }
        None
    } else if s.len() >= 2 && s.starts_with('`') && s.ends_with('`') {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
}

/// `var = fnValue("fname", [args…])`.
fn fnvalue_assign(var: &str, fname: &str, call_args: &[IrExpr]) -> IrStmt {
    IrStmt::Assign {
        targets: vec![AssignTarget {
            var: var.to_string(),
            sigil: None,
            indices: vec![],
        }],
        expr: IrExpr::Call {
            func: "fnValue".to_string(),
            args: vec![
                IrExpr::Str(fname.to_string(), StrStyle::DoubleQuoted),
                IrExpr::Array(call_args.to_vec()),
            ],
        },
        asm: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::commands::parse_commands_from_text;
    use crate::shir::ast_to_ir_raw;
    use crate::shir_json::shir_to_shir_json;

    fn lower(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(transform(&mut prog.stmts), "transform was a no-op for {src}");
        shir_to_shir_json(&prog)
    }

    fn lower_raw(src: &str) -> String {
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        transform(&mut prog.stmts);
        shir_to_shir_json(&prog)
    }

    #[test]
    fn simple_value_function_lifts() {
        // strLen-style: a local decl + a single echo.
        let json = lower("f() { local s=\"$1\"; echo \"${#s}\"; }; f hello");
        assert!(json.contains("\"fnValue\""), "missing fnValue call: {json}");
        assert!(json.contains("\"Return\""), "missing value return: {json}");
        // the statement call became an echo of the fnValue result
        assert!(
            json.contains("\"echo\"") && json.contains("\"fnValue\""),
            "call site not wrapped: {json}"
        );
    }

    #[test]
    fn if_arms_lift() {
        // contains() { if [[ "$h" == *"$n"* ]]; then echo 1; else echo 0; fi; }
        let json = lower_raw(
            "contains() { local h=\"$1\" n=\"$2\"; if [[ \"$h\" == *\"$n\"* ]]; then echo 1; else echo 0; fi; }; contains a b",
        );
        assert!(json.contains("\"fnValue\""), "missing fnValue: {json}");
        assert!(json.contains("\"Return\""), "missing return: {json}");
        // both echo arms became returns (two Return nodes)
        let n = json.matches("\"type\":\"Return\"").count();
        assert!(n >= 2, "expected both arms to return, got {n}: {json}");
    }

    #[test]
    fn impure_body_refuses() {
        // an exec inside the body (a capture) → not eligible
        let json = lower_raw(
            "f() { local s=\"$1\"; r=$(echo hi); echo \"$r\"; }; f",
        );
        assert!(!json.contains("\"fnValue\""), "impure body lifted: {json}");
    }

    #[test]
    fn no_echo_path_refuses() {
        // a path without an echo (bare return before any echo) → refuse
        let json = lower_raw(
            "f() { local s=\"$1\"; if [[ \"$s\" == x ]]; then return; fi; echo \"$s\"; }; f",
        );
        assert!(!json.contains("\"fnValue\""), "no-echo path lifted: {json}");
    }

    #[test]
    fn multi_echo_refuses() {
        // two sequential echoes → 2 lines on a path → refuse
        let json = lower_raw("f() { echo 1; echo 2; }; f");
        assert!(!json.contains("\"fnValue\""), "multi-echo lifted: {json}");
    }

    /// A pipeline's LAST stage is the pipeline's output sink: an eligible
    /// call there must ECHO its value, not become a silent value-returning
    /// call. Regression for `y=$(echo hi | f)` / `y=$(f | cat)` capturing
    /// "" where bash captures the callee's echoed value.
    #[test]
    fn pipeline_last_stage_echoes_the_value() {
        let src = "f() { echo \"x\"; }; y=$(echo hi | f); z=$(f | cat)";
        let commands = parse_commands_from_text(src).expect("parse source");
        let mut prog = ast_to_ir_raw(&commands);
        assert!(transform(&mut prog.stmts), "transform was a no-op");
        // both captures must have their `f` stage rewritten to an echo of
        // fnValue — the trailing-pipe case (the call is the LAST stage) and
        // the leading-call case (a non-last stage).
        let mut echoed_f = 0;
        let mut bare_f = 0;
        for st in &prog.stmts {
            let IrStmt::Assign { expr, .. } = st else { continue };
            let IrExpr::Capture { expr: cap, .. } = expr else { continue };
            let IrExpr::Arrow(body) = cap.as_ref() else { continue };
            let [IrStmt::Expr(IrExpr::Call { func, args })] = body.as_slice() else {
                continue;
            };
            if func != "pipeline" {
                continue;
            }
            let Some(IrExpr::Array(stages)) = args.first() else { continue };
            for stage in stages {
                let IrExpr::Arrow(lbody) = stage else { continue };
                let [IrStmt::Expr(IrExpr::Call { func, args })] = lbody.as_slice() else {
                    continue;
                };
                let calls_f = matches!(args.first(), Some(IrExpr::Str(n, _)) if n == "f");
                if func == "exec" && calls_f {
                    bare_f += 1;
                }
                let echoes_fnvalue = func == "exec"
                    && matches!(args.first(), Some(IrExpr::Str(s, _)) if s == "echo")
                    && matches!(args.get(1), Some(IrExpr::Array(items))
                        if matches!(items.as_slice(), [IrExpr::Call { func, .. }] if func == "fnValue"));
                if echoes_fnvalue {
                    echoed_f += 1;
                }
            }
        }
        assert_eq!(bare_f, 0, "a pipeline stage still calls f without echoing");
        assert_eq!(echoed_f, 2, "expected both f stages to echo fnValue");
    }
}
