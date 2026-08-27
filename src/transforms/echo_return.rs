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
//! Registered in `transforms.rs` (DEBASHC_TRANSFORMS gated). The estree
//! emitter needs no new arms: `IrStmt::Return` renders natively and
//! `Call("fnValue")` renders `sh2.fnValue(...)` (the general call path).

use crate::ir::{InterpPart, IrExpr, IrStmt, StrStyle};
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
    // Pass 1 — recognition, to a FIXPOINT: a function whose body captures
    // an ALREADY-eligible function (e.g. wcLines' `r=$(strCount ...)`) is
    // itself eligible once the callee is (the capture is then a pure value
    // read — pass 3 collapses it to the bare fnValue). Iterate until no
    // new names appear.
    let mut eligible: HashSet<String> = HashSet::new();
    loop {
        let mut grew = false;
        for st in stmts.iter() {
            if let IrStmt::Function { name, body, .. } = st {
                if !eligible.contains(name) && body_echo_ok(body, &eligible) {
                    eligible.insert(name.clone());
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
    for st in stmts.iter_mut() {
        c |= rewrite_call_stmt(st, &eligible);
    }
    c
}

/// Is a statement list "echo-return": every path from the start to the
/// block end (or a `return`) emits EXACTLY one single-arg echo?
fn body_echo_ok(body: &[IrStmt], eligible: &HashSet<String>) -> bool {
    let mut states: HashSet<EchoState> = HashSet::new();
    states.insert(EchoState::Need);
    let Some(final_states) = parse_stmts(body, &states, eligible) else {
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
) -> Option<HashSet<EchoState>> {
    let mut cur = incoming.clone();
    for st in stmts {
        cur = parse_stmt(st, &cur, eligible)?;
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
) -> Option<HashSet<EchoState>> {
    match st {
        // A single-arg value echo: Need → Done; Dead paths stay dead
        // (unreachable); a Done path would echo a second value → refuse.
        IrStmt::Expr(IrExpr::Call { func, args, .. })
            if matches!(func.as_str(), "builtin" | "exec")
                && value_echo(args, eligible).is_some() =>
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
        _ if stmt_silent(st, eligible) => Some(states.clone()),
        // A conditional: every arm must satisfy the discipline; the
        // outcomes union. An empty else falls through (state unchanged).
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            if !expr_pure(cond, eligible) {
                return None;
            }
            let mut out = HashSet::new();
            out.extend(parse_stmts(then, states, eligible)?);
            for (ec, eb) in elsifs {
                if !expr_pure(ec, eligible) {
                    return None;
                }
                out.extend(parse_stmts(eb, states, eligible)?);
            }
            out.extend(parse_stmts(else_, states, eligible)?);
            Some(out)
        }
        // Loops: the body may not reach `Done` (an echo without a return
        // would re-echo on the next iteration — 2+ echoes). A Need-iteration
        // loops on; a Dead iteration ends the path. 0 iterations → Need
        // (the state passes through).
        IrStmt::While { cond, body } => loop_states(cond, body, states, eligible),
        IrStmt::For { iter, body, .. } => {
            if !expr_pure(iter, eligible) {
                return None;
            }
            loop_body_states(body, states, eligible)
        }
        IrStmt::ForInit {
            init,
            cond,
            step,
            body,
        } => {
            if init.iter().any(|s| !stmt_silent(s, eligible))
                || !expr_pure(cond, eligible)
                || step.iter().any(|s| !stmt_silent(s, eligible))
            {
                return None;
            }
            loop_body_states(body, states, eligible)
        }
        _ => None,
    }
}

fn loop_states(
    cond: &IrExpr,
    body: &[IrStmt],
    states: &HashSet<EchoState>,
    eligible: &HashSet<String>,
) -> Option<HashSet<EchoState>> {
    if !expr_pure(cond, eligible) {
        return None;
    }
    loop_body_states(body, states, eligible)
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
) -> Option<HashSet<EchoState>> {
    let iter = parse_stmts(body, states, eligible)?;
    if iter.contains(&EchoState::Done) || iter.contains(&EchoState::Dead) {
        return None;
    }
    Some(states.clone())
}

/// A single-arg value echo: `builtin("echo", [Array([word])])` /
/// `exec("echo", [Array([word])])` — one word, no `-n`/`-e` flags. The
/// word must be a pure value expression (the returned value).
fn value_echo<'a>(args: &'a [IrExpr], eligible: &HashSet<String>) -> Option<&'a IrExpr> {
    let [IrExpr::Str(n, _), IrExpr::Array(words)] = args else {
        return None;
    };
    if n != "echo" {
        return None;
    }
    let [word] = words.as_slice() else {
        return None;
    };
    if !expr_pure(word, eligible) {
        return None;
    }
    Some(word)
}

/// Is the statement side-effect-free beyond the local store (no stdout,
/// no exec/capture/redirect/background/subshell, no status reads)?
fn stmt_silent(st: &IrStmt, eligible: &HashSet<String>) -> bool {
    match st {
        IrStmt::Assign { expr, .. } => expr_pure(expr, eligible),
        IrStmt::Declare { init, .. } => init.as_ref().map(|i| expr_pure(i, eligible)).unwrap_or(true),
        IrStmt::DeclareArray { elements, .. } => elements.iter().all(|e| expr_pure(e, eligible)),
        // `local x="$1"` / `declare -i n` / `shift` — the runtime decl
        // builtins with pure value args are silent (function-scoped store
        // writes).
        IrStmt::Expr(IrExpr::Call { func, args, .. })
            if matches!(func.as_str(), "builtin" | "exec") =>
        {
            if let Some(IrExpr::Str(n, _)) = args.first() {
                if matches!(n.as_str(), "local" | "declare" | "typeset" | "readonly" | "shift") {
                    return args.iter().skip(1).all(|a| expr_pure(a, eligible));
                }
            }
            false
        }
        // A `while IFS= read -r line` loop (line_count's shape): the read
        // builtin consumes stdin, emits nothing — silent when the body is.
        IrStmt::While { cond, body } => {
            (is_read_loop(cond) || expr_pure(cond, eligible))
                && body.iter().all(|s| stmt_silent(s, eligible))
        }
        IrStmt::For { iter, body, .. } => {
            expr_pure(iter, eligible) && body.iter().all(|s| stmt_silent(s, eligible))
        }
        IrStmt::If {
            cond,
            then,
            elsifs,
            else_,
        } => {
            expr_pure(cond, eligible)
                && then.iter().all(|s| stmt_silent(s, eligible))
                && elsifs
                    .iter()
                    .all(|(c, b)| expr_pure(c, eligible) && b.iter().all(|s| stmt_silent(s, eligible)))
                && else_.iter().all(|s| stmt_silent(s, eligible))
        }
        IrStmt::Block(body) => body.iter().all(|s| stmt_silent(s, eligible)),
        // A herestring/heredoc redirect feeding a silent inner (line_count's
        // `while read ... <<< "$s"`): the redirect is a pure input source.
        IrStmt::Redirect { inner, redirects } => {
            redirects.iter().all(|r| {
                matches!(r.mode.as_str(), "herestring" | "heredoc")
                    && expr_pure(&r.target, eligible)
            }) && inner.iter().all(|s| stmt_silent(s, eligible))
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
fn expr_pure(e: &IrExpr, eligible: &HashSet<String>) -> bool {
    match e {
        // A capture of an ALREADY-eligible function is a pure value read
        // (pass 3 collapses it to the bare fnValue) — the fixpoint lets
        // wcLines' `r=$(strCount ...)` through once strCount is eligible.
        // The callee may be a bare Call (the direct_calls collapse) or an
        // Arrow wrapping a single call statement (a loop-bearing callee
        // like strCount stays an Arrow — direct_calls refuses loops).
        IrExpr::Capture { expr, .. } => match expr.as_ref() {
            IrExpr::Call { func, args } if matches!(func.as_str(), "exec" | "fnCall") => {
                matches!(args.as_slice(), [IrExpr::Str(fname, _), IrExpr::Array(_)]
                    if eligible.contains(fname))
            }
            IrExpr::Arrow(stmts) => matches!(
                stmts.as_slice(),
                [IrStmt::Expr(IrExpr::Call { func, args })]
                    if matches!(func.as_str(), "exec" | "fnCall")
                        && matches!(args.as_slice(), [IrExpr::Str(fname, _), IrExpr::Array(_)]
                            if eligible.contains(fname))
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
                            return args.iter().skip(1).all(|a| expr_pure(a, eligible));
                        }
                    }
                }
                return false;
            }
            // the `$?` status read
            if func == "getVar" && matches!(args.as_slice(), [IrExpr::Str(n, _)] if n == "?") {
                return false;
            }
            args.iter().all(|a| expr_pure(a, eligible))
        }
        IrExpr::Interpolate(parts) => parts.iter().all(|p| match p {
            InterpPart::Lit(s) => !s.contains("$?"),
            InterpPart::Expr(ie) => expr_pure(ie, eligible),
        }),
        IrExpr::BinOp { lhs, rhs, .. } => expr_pure(lhs, eligible) && expr_pure(rhs, eligible),
        IrExpr::MethodCall { obj, args, .. } => {
            expr_pure(obj, eligible) && args.iter().all(|a| expr_pure(a, eligible))
        }
        IrExpr::Index { key, .. } => expr_pure(key, eligible),
        IrExpr::DefinedOr { expr, default } => {
            expr_pure(expr, eligible) && expr_pure(default, eligible)
        }
        IrExpr::Array(items) => items.iter().all(|i| expr_pure(i, eligible)),
        IrExpr::Arrow(stmts) => stmts.iter().all(|s| stmt_silent(s, eligible)),
        IrExpr::Lambda { body, .. } => body.iter().all(|s| stmt_silent(s, eligible)),
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
        out.push(rewrite_stmt(&st, &mut changed, eligible));
    }
    *body = out;
    changed
}

/// Rewrite one statement: a value echo → Return(Some(word)); a bare
/// return → dropped (unreachable after the echo-return); otherwise recurse
/// into nested containers.
fn rewrite_stmt(st: &IrStmt, changed: &mut bool, eligible: &HashSet<String>) -> IrStmt {
    match st {
        IrStmt::Expr(IrExpr::Call { func, args, .. })
            if matches!(func.as_str(), "builtin" | "exec") =>
        {
            if let Some(word) = value_echo(args, eligible) {
                *changed = true;
                return IrStmt::Return(Some(word.clone()));
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
                t.push(rewrite_stmt(s, changed, eligible));
            }
            let mut es: Vec<(IrExpr, Vec<IrStmt>)> = Vec::with_capacity(elsifs.len());
            for (c, b) in elsifs {
                let mut nb: Vec<IrStmt> = Vec::with_capacity(b.len());
                for s in b {
                    nb.push(rewrite_stmt(s, changed, eligible));
                }
                es.push((c.clone(), nb));
            }
            let mut el: Vec<IrStmt> = Vec::with_capacity(else_.len());
            for s in else_ {
                el.push(rewrite_stmt(s, changed, eligible));
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
                b.push(rewrite_stmt(s, changed, eligible));
            }
            IrStmt::While {
                cond: cond.clone(),
                body: b,
            }
        }
        IrStmt::For { var, iter, body } => {
            let mut b: Vec<IrStmt> = Vec::with_capacity(body.len());
            for s in body {
                b.push(rewrite_stmt(s, changed, eligible));
            }
            IrStmt::For {
                var: var.clone(),
                iter: iter.clone(),
                body: b,
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
                b.push(rewrite_stmt(s, changed, eligible));
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
                b.push(rewrite_stmt(s, changed, eligible));
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
            for stage in stages.iter_mut() {
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
        _ => false,
    }
}

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
        if let IrExpr::Call { func, args } = expr.as_ref() {
            if eligible_fn_call(func, args, eligible) {
                *e = fn_value_call(args);
                return true;
            }
        }
        return rewrite_expr_calls(expr, eligible);
    }
    match e {
        IrExpr::Call { func, args } if matches!(func.as_str(), "exec" | "fnCall") => {
            if eligible_fn_call(func, args, eligible) {
                *args = echo_of_fn_value(args);
                return true;
            }
            false
        }
        IrExpr::Call { func, args } => {
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
}
